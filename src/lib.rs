//! ostore — a minimal agent-friendly object-storage daemon.
//!
//! Exposes object-storage semantics (upload/list/stat/download/delete/mkdir/
//! move) over HTTP for one bound server directory, plus an event system
//! (SSE + webhooks) so observers can watch object mutations, and Prometheus
//! metrics.

pub mod auth;
pub mod cluster;
pub mod config;
pub mod error;
pub mod events;
pub mod metrics;
pub mod paths;
pub mod range;
pub mod routes;
pub mod s3;
pub mod ui;
pub mod webhooks;

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::Utc;
use tokio::sync::{RwLock, Semaphore};
use tokio_util::sync::CancellationToken;

use crate::events::{EventBus, ObjectEvent};
use crate::metrics::Metrics;
use crate::s3::S3Auth;
use crate::webhooks::WebhookStore;

pub use routes::build_router;

/// Marker embedded in in-flight upload temp file names.
pub const TMP_MARKER: &str = ".ostore-tmp-";

/// Fast in-memory cache of (size, mtime_nanos) -> sha256_hex to eliminate
/// disk re-reads on /api/list and cluster syncs.
#[derive(Default)]
pub struct ChecksumCache {
    cache: RwLock<HashMap<PathBuf, (u64, u128, String)>>,
}

impl ChecksumCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn get_or_compute(&self, abs: &Path, meta: &std::fs::Metadata) -> Result<String, crate::error::ApiError> {
        let size = meta.len();
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_nanos())
            .unwrap_or(0);

        {
            let lock = self.cache.read().await;
            if let Some((c_size, c_mtime, sha)) = lock.get(abs) {
                if *c_size == size && *c_mtime == mtime {
                    return Ok(sha.clone());
                }
            }
        }

        // Cache miss: compute from disk
        let sha = crate::routes::files::sha256_file(abs).await?;
        let mut lock = self.cache.write().await;
        lock.insert(abs.to_path_buf(), (size, mtime, sha.clone()));
        Ok(sha)
    }

    pub async fn insert(&self, abs: &Path, size: u64, mtime: u128, sha: String) {
        let mut lock = self.cache.write().await;
        lock.insert(abs.to_path_buf(), (size, mtime, sha));
    }

    pub async fn invalidate(&self, abs: &Path) {
        let mut lock = self.cache.write().await;
        lock.remove(abs);
    }
}

/// Shared application state, cheap to clone.
#[derive(Clone)]
pub struct AppState {
    /// Bound root directory (absolute or as configured).
    pub root: PathBuf,
    /// Bearer token required for all /api requests.
    pub token: Arc<str>,
    /// In-memory event bus (broadcast + replay ring).
    pub events: Arc<EventBus>,
    /// Persistent webhook registrations + delivery.
    pub webhooks: Arc<WebhookStore>,
    pub metrics: Arc<Metrics>,
    /// Cancelled on SIGTERM/SIGINT so long-lived SSE streams end and the
    /// graceful shutdown can complete.
    pub shutdown: CancellationToken,
    /// Caps concurrent thumbnail decodes (CPU/memory heavy).
    pub thumb_permits: Arc<Semaphore>,
    /// Credentials for the S3 read surface; `None` disables it.
    pub s3: Option<Arc<S3Auth>>,
    /// Multi-node storage cluster manager (None in standalone mode).
    pub cluster: Option<Arc<crate::cluster::ClusterManager>>,
    /// Memory cache for file checksums.
    pub checksum_cache: Arc<ChecksumCache>,
}

impl AppState {
    /// Create state, ensuring `<root>` and `<root>/.ostore/` exist and
    /// loading persisted webhook registrations.
    pub fn new(root: PathBuf, token: String) -> io::Result<Self> {
        Self::with_s3(root, token, None)
    }

    /// [`AppState::new`], plus optional S3-surface credentials.
    pub fn with_s3(root: PathBuf, token: String, s3: Option<S3Auth>) -> io::Result<Self> {
        Self::with_cluster_and_s3(root, token, None, s3, Arc::new(ChecksumCache::new()))
    }

    /// [`AppState::new`], plus cluster manager, optional S3 credentials, and checksum cache.
    pub fn with_cluster_and_s3(
        root: PathBuf,
        token: String,
        cluster: Option<Arc<crate::cluster::ClusterManager>>,
        s3: Option<S3Auth>,
        checksum_cache: Arc<ChecksumCache>,
    ) -> io::Result<Self> {
        std::fs::create_dir_all(&root)?;
        std::fs::create_dir_all(root.join(".ostore"))?;
        let metrics = Arc::new(Metrics::new());
        Ok(Self {
            webhooks: Arc::new(WebhookStore::load(&root, metrics.clone())),
            root,
            token: token.into(),
            events: Arc::new(EventBus::new()),
            metrics,
            shutdown: CancellationToken::new(),
            thumb_permits: Arc::new(Semaphore::new(
                std::thread::available_parallelism().map(|n| n.get().saturating_sub(1).max(1)).unwrap_or(1),
            )),
            s3: s3.map(Arc::new),
            cluster,
            checksum_cache,
        })
    }

    /// Publish an object event to SSE subscribers and webhook targets.
    /// `is_dir` describes the object the event is about.
    pub fn publish(&self, kind: &str, path: &str, is_dir: bool, size: Option<u64>, from: Option<String>) {
        let ev = self.events.publish(ObjectEvent {
            kind: kind.to_string(),
            id: 0,
            path: path.to_string(),
            entry_type: if is_dir { "dir" } else { "file" },
            size,
            from,
            ts: Utc::now(),
        });
        self.metrics.event_published(kind);
        self.webhooks.deliver(ev);
    }
}

/// Remove temp files left behind by a crash mid-upload. Run once at startup,
/// before serving, so no live upload can be affected. Returns files removed.
pub fn sweep_stale_tmp(root: &Path) -> usize {
    let mut removed = 0;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for de in rd.flatten() {
            let Ok(ft) = de.file_type() else { continue };
            let name = de.file_name();
            let name = name.to_string_lossy();
            if ft.is_dir() {
                if name != paths::META_DIR {
                    stack.push(de.path());
                }
            } else if ft.is_file() && name.contains(TMP_MARKER) && std::fs::remove_file(de.path()).is_ok() {
                tracing::warn!(path = %de.path().display(), "removed stale upload temp file");
                removed += 1;
            }
        }
    }
    removed
}
