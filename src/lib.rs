//! ostore — a minimal agent-friendly object-storage daemon.
//!
//! Exposes object-storage semantics (upload/list/stat/download/delete/mkdir/
//! move) over HTTP for one bound server directory, plus an event system
//! (SSE + webhooks) so observers can watch object mutations, and Prometheus
//! metrics.

pub mod auth;
pub mod config;
pub mod error;
pub mod events;
pub mod metrics;
pub mod paths;
pub mod range;
pub mod routes;
pub mod webhooks;

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::Utc;
use tokio_util::sync::CancellationToken;

use crate::events::{EventBus, ObjectEvent};
use crate::metrics::Metrics;
use crate::webhooks::WebhookStore;

pub use routes::build_router;

/// Marker embedded in in-flight upload temp file names.
pub const TMP_MARKER: &str = ".ostore-tmp-";

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
}

impl AppState {
    /// Create state, ensuring `<root>` and `<root>/.ostore/` exist and
    /// loading persisted webhook registrations.
    pub fn new(root: PathBuf, token: String) -> io::Result<Self> {
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
