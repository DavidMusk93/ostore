//! Cluster management and multi-node storage replication.
//!
//! Provides a unified control plane across multiple ostore instances,
//! periodic node health/disk monitoring, automatic replication on write,
//! and on-demand cluster synchronization.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tokio_util::io::ReaderStream;
use tokio_util::sync::CancellationToken;

pub mod gdrive;
use self::gdrive::{GDriveClient, GDriveConfig};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodeConfig {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub is_local: bool,
    #[serde(default)]
    pub node_type: Option<String>,
    #[serde(default)]
    pub gdrive_client_id: Option<String>,
    #[serde(default)]
    pub gdrive_client_secret: Option<String>,
    #[serde(default)]
    pub gdrive_refresh_token: Option<String>,
    #[serde(default)]
    pub gdrive_root_folder_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct ClusterConfig {
    #[serde(default)]
    pub nodes: Vec<NodeConfig>,
    #[serde(default)]
    pub replicate_prefixes: Vec<String>,
}

impl ClusterConfig {
    pub fn from_env() -> Result<Option<Self>, String> {
        let val = match std::env::var("OSTORE_CLUSTER_CONFIG") {
            Ok(v) => v,
            Err(_) => {
                let default_path = Path::new("/etc/ostore-cluster.json");
                if default_path.exists() {
                    match std::fs::read_to_string(default_path) {
                        Ok(content) => content,
                        Err(e) => return Err(format!("failed to read /etc/ostore-cluster.json: {e}")),
                    }
                } else {
                    return Ok(None);
                }
            }
        };

        let trimmed = val.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }

        let content = if trimmed.starts_with('{') {
            trimmed.to_string()
        } else {
            std::fs::read_to_string(trimmed)
                .map_err(|e| format!("cannot read cluster config file {trimmed}: {e}"))?
        };

        let cfg: ClusterConfig = serde_json::from_str(&content)
            .map_err(|e| format!("cannot parse cluster config JSON: {e}"))?;
        Ok(Some(cfg))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodeStatus {
    pub id: String,
    pub name: String,
    pub url: String,
    pub is_local: bool,
    pub healthy: bool,
    pub rtt_ms: Option<u64>,
    pub disk_total_bytes: u64,
    pub disk_available_bytes: u64,
    pub last_seen: Option<DateTime<Utc>>,
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_type: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ClusterStatus {
    pub nodes: Vec<NodeStatus>,
    pub total_disk_bytes: u64,
    pub available_disk_bytes: u64,
    pub replicate_prefixes: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct MainFileNode {
    pub id: String,
    pub name: String,
    pub exists: bool,
    pub size: u64,
    pub md5: Option<String>,
    pub sha256: Option<String>,
    pub mtime: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Serialize)]
pub struct FollowerFileNode {
    pub id: String,
    pub name: String,
    pub url: String,
    pub exists: bool,
    pub matches: bool,
    pub size: u64,
    pub md5: Option<String>,
    pub sha256: Option<String>,
    pub rtt_ms: Option<u64>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ProbeMetrics {
    pub total_ms: u64,
    pub main_lookup_ms: u64,
    pub follower_timings: HashMap<String, u64>,
    pub verify_hash: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct FileReplicaTopology {
    pub path: String,
    pub fully_synced: bool,
    pub replica_count: usize,
    pub expected_replicas: usize,
    pub main: MainFileNode,
    pub followers: Vec<FollowerFileNode>,
    pub metrics: ProbeMetrics,
}

#[derive(Clone, Debug, Serialize)]
pub struct PrefixDistribution {
    pub prefix: String,
    pub object_count: usize,
    pub total_bytes: u64,
    pub replica_factor: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct ClusterTopology {
    pub control_plane: Option<NodeStatus>,
    pub workers: Vec<NodeStatus>,
    pub total_disk_bytes: u64,
    pub available_disk_bytes: u64,
    pub distributions: Vec<PrefixDistribution>,
    pub replicate_prefixes: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodeSyncResult {
    pub node_id: String,
    pub total_checked: usize,
    pub synced: usize,
    pub skipped: usize,
    pub errors: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct LiveSyncProgress {
    pub is_syncing: bool,
    pub current_file: String,
    pub current_node: String,
    pub total_to_sync: usize,
    pub synced_count: usize,
    pub skipped_count: usize,
    pub failed_count: usize,
    pub percent: f64,
    pub start_time_ms: u64,
    pub elapsed_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FileDiffEntry {
    pub path: String,
    pub size: u64,
    pub hash: String,
    pub missing_on: Vec<String>,
    pub mismatch_on: Vec<String>,
    pub in_sync_on: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SyncDiffReport {
    pub total_scanned: usize,
    pub diff_count: usize,
    pub in_sync_count: usize,
    pub differences: Vec<FileDiffEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct CustomSyncOptions {
    #[serde(default)]
    pub prefix: Option<String>,
    #[serde(default)]
    pub paths: Option<Vec<String>>,
    #[serde(default)]
    pub target_nodes: Option<Vec<String>>,
    #[serde(default)]
    pub concurrency: Option<usize>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SyncReport {
    pub prefix: String,
    pub total_checked: usize,
    pub total_synced: usize,
    pub total_skipped: usize,
    pub synced: usize,
    pub duration_ms: u64,
    pub timestamp: DateTime<Utc>,
    pub node_results: Vec<NodeSyncResult>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ClusterSyncStatus {
    pub is_syncing: bool,
    pub progress: LiveSyncProgress,
    pub last_sync: Option<SyncReport>,
    pub nodes: Vec<NodeStatus>,
}

#[derive(Clone, Debug, Serialize)]
pub struct FileReplicaSummary {
    pub cc2: bool,
    pub rn: bool,
    pub replica_count: usize,
    pub expected_replicas: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct DirReplicaStatus {
    pub prefix: String,
    pub files: HashMap<String, FileReplicaSummary>,
}

pub struct ClusterManager {
    pub config: std::sync::RwLock<ClusterConfig>,
    pub gdrive: Arc<RwLock<Option<Arc<GDriveClient>>>>,
    client: reqwest::Client,
    statuses: Arc<RwLock<HashMap<String, NodeStatus>>>,
    last_sync_report: Arc<RwLock<Option<SyncReport>>>,
    cached_topology: Arc<RwLock<Option<(Instant, ClusterTopology)>>>,
    is_syncing: Arc<AtomicBool>,
    pub live_progress: Arc<RwLock<LiveSyncProgress>>,
    pub active_cancel: Arc<RwLock<Option<CancellationToken>>>,
    root: PathBuf,
    cache: Arc<crate::ChecksumCache>,
}

impl ClusterManager {
    pub fn new(config: ClusterConfig, root: PathBuf, cache: Arc<crate::ChecksumCache>) -> Self {
        let client = reqwest::Client::builder()
            .tcp_nodelay(true)
            .pool_idle_timeout(Duration::from_secs(90))
            .pool_max_idle_per_host(32)
            .timeout(Duration::from_secs(15))
            .connect_timeout(Duration::from_secs(3))
            .build()
            .unwrap_or_default();

        let mut gdrive_client = GDriveClient::from_env();
        if gdrive_client.is_none() {
            for node in &config.nodes {
                if node.node_type.as_deref() == Some("gdrive") || node.id == "gdrive" {
                    if let (Some(client_id), Some(client_secret), Some(refresh_token)) = (
                        &node.gdrive_client_id,
                        &node.gdrive_client_secret,
                        &node.gdrive_refresh_token,
                    ) {
                        gdrive_client = Some(GDriveClient::new(GDriveConfig {
                            client_id: client_id.clone(),
                            client_secret: client_secret.clone(),
                            refresh_token: refresh_token.clone(),
                            root_folder_id: node
                                .gdrive_root_folder_id
                                .clone()
                                .unwrap_or_else(|| "root".to_string()),
                        }));
                        break;
                    }
                }
            }
        }
        let gdrive = Arc::new(RwLock::new(gdrive_client.map(Arc::new)));

        let mut initial_statuses = HashMap::new();
        for node in &config.nodes {
            initial_statuses.insert(
                node.id.clone(),
                NodeStatus {
                    id: node.id.clone(),
                    name: node.name.clone(),
                    url: node.url.clone(),
                    is_local: node.is_local,
                    healthy: node.is_local,
                    rtt_ms: if node.is_local { Some(0) } else { None },
                    disk_total_bytes: 0,
                    disk_available_bytes: 0,
                    last_seen: None,
                    error: None,
                    node_type: node.node_type.clone(),
                },
            );
        }

        Self {
            config: std::sync::RwLock::new(config),
            gdrive,
            client,
            statuses: Arc::new(RwLock::new(initial_statuses)),
            last_sync_report: Arc::new(RwLock::new(None)),
            cached_topology: Arc::new(RwLock::new(None)),
            is_syncing: Arc::new(AtomicBool::new(false)),
            live_progress: Arc::new(RwLock::new(LiveSyncProgress::default())),
            active_cancel: Arc::new(RwLock::new(None)),
            root,
            cache,
        }
    }

    pub fn start_probe_loop(self: Arc<Self>, shutdown: CancellationToken) {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(15));
            loop {
                tokio::select! {
                    _ = shutdown.cancelled() => break,
                    _ = interval.tick() => {
                        self.probe_all().await;
                    }
                }
            }
        });
    }

    pub async fn probe_all(&self) {
        let nodes = self.config.read().unwrap().nodes.clone();
        for node in &nodes {
            if node.is_local {
                if let Some((total, avail)) = crate::metrics::disk_space(&self.root) {
                    let mut lock = self.statuses.write().await;
                    let st = lock.entry(node.id.clone()).or_insert_with(|| NodeStatus {
                        id: node.id.clone(),
                        name: node.name.clone(),
                        url: node.url.clone(),
                        is_local: true,
                        healthy: true,
                        rtt_ms: Some(0),
                        disk_total_bytes: total,
                        disk_available_bytes: avail,
                        last_seen: Some(Utc::now()),
                        error: None,
                        node_type: node.node_type.clone(),
                    });
                    st.healthy = true;
                    st.rtt_ms = Some(0);
                    st.disk_total_bytes = total;
                    st.disk_available_bytes = avail;
                    st.last_seen = Some(Utc::now());
                    st.error = None;
                }
                continue;
            }

            if node.node_type.as_deref() == Some("gdrive") || node.id == "gdrive" {
                if let Some(gdrive) = self.gdrive.read().await.clone() {
                    let start = Instant::now();
                    match gdrive.get_storage_quota().await {
                        Ok((total, avail)) => {
                            let rtt = start.elapsed().as_millis() as u64;
                            let mut lock = self.statuses.write().await;
                            let st = lock.entry(node.id.clone()).or_insert_with(|| NodeStatus {
                                id: node.id.clone(),
                                name: node.name.clone(),
                                url: node.url.clone(),
                                is_local: false,
                                healthy: true,
                                rtt_ms: Some(rtt),
                                disk_total_bytes: total,
                                disk_available_bytes: avail,
                                last_seen: Some(Utc::now()),
                                error: None,
                                node_type: Some("gdrive".to_string()),
                            });
                            st.healthy = true;
                            st.rtt_ms = Some(rtt);
                            st.disk_total_bytes = total;
                            st.disk_available_bytes = avail;
                            st.last_seen = Some(Utc::now());
                            st.error = None;
                            st.node_type = Some("gdrive".to_string());
                        }
                        Err(e) => {
                            let mut lock = self.statuses.write().await;
                            let st = lock.entry(node.id.clone()).or_insert_with(|| NodeStatus {
                                id: node.id.clone(),
                                name: node.name.clone(),
                                url: node.url.clone(),
                                is_local: false,
                                healthy: false,
                                rtt_ms: None,
                                disk_total_bytes: 0,
                                disk_available_bytes: 0,
                                last_seen: Some(Utc::now()),
                                error: Some(e.clone()),
                                node_type: Some("gdrive".to_string()),
                            });
                            st.healthy = false;
                            st.rtt_ms = None;
                            st.last_seen = Some(Utc::now());
                            st.error = Some(e);
                        }
                    }
                }
                continue;
            }

            let start = Instant::now();
            let base = node.url.trim_end_matches('/');
            let health_url = format!("{base}/healthz");
            let metrics_url = format!("{base}/api/metrics");

            let res = self.client.get(&health_url).send().await;
            match res {
                Ok(resp) if resp.status().is_success() => {
                    let rtt = start.elapsed().as_millis() as u64;
                    let (mut total, mut avail) = (0u64, 0u64);

                    // Try fetching metrics to get exact disk usage
                    let mut req = self.client.get(&metrics_url);
                    if !node.token.is_empty() {
                        req = req.header("Authorization", format!("Bearer {}", node.token));
                    }
                    if let Ok(m_resp) = req.send().await {
                        if let Ok(text) = m_resp.text().await {
                            for line in text.lines() {
                                if let Some(rest) = line.strip_prefix("ostore_root_disk_total_bytes ") {
                                    if let Ok(v) = rest.trim().parse::<u64>() {
                                        total = v;
                                    }
                                } else if let Some(rest) = line.strip_prefix("ostore_root_disk_available_bytes ") {
                                    if let Ok(v) = rest.trim().parse::<u64>() {
                                        avail = v;
                                    }
                                }
                            }
                        }
                    }

                    let mut lock = self.statuses.write().await;
                    let st = lock.entry(node.id.clone()).or_insert_with(|| NodeStatus {
                        id: node.id.clone(),
                        name: node.name.clone(),
                        url: node.url.clone(),
                        is_local: false,
                        healthy: true,
                        rtt_ms: Some(rtt),
                        disk_total_bytes: total,
                        disk_available_bytes: avail,
                        last_seen: Some(Utc::now()),
                        error: None,
                        node_type: node.node_type.clone(),
                    });
                    st.healthy = true;
                    st.rtt_ms = Some(rtt);
                    st.disk_total_bytes = total;
                    st.disk_available_bytes = avail;
                    st.last_seen = Some(Utc::now());
                    st.error = None;
                }
                Ok(resp) => {
                    let mut lock = self.statuses.write().await;
                    let st = lock.entry(node.id.clone()).or_insert_with(|| NodeStatus {
                        id: node.id.clone(),
                        name: node.name.clone(),
                        url: node.url.clone(),
                        is_local: false,
                        healthy: false,
                        rtt_ms: None,
                        disk_total_bytes: 0,
                        disk_available_bytes: 0,
                        last_seen: Some(Utc::now()),
                        error: Some(format!("HTTP {}", resp.status())),
                        node_type: node.node_type.clone(),
                    });
                    st.healthy = false;
                    st.rtt_ms = None;
                    st.last_seen = Some(Utc::now());
                    st.error = Some(format!("HTTP {}", resp.status()));
                }
                Err(e) => {
                    let mut lock = self.statuses.write().await;
                    let st = lock.entry(node.id.clone()).or_insert_with(|| NodeStatus {
                        id: node.id.clone(),
                        name: node.name.clone(),
                        url: node.url.clone(),
                        is_local: false,
                        healthy: false,
                        rtt_ms: None,
                        disk_total_bytes: 0,
                        disk_available_bytes: 0,
                        last_seen: Some(Utc::now()),
                        error: Some(e.to_string()),
                        node_type: node.node_type.clone(),
                    });
                    st.healthy = false;
                    st.rtt_ms = None;
                    st.last_seen = Some(Utc::now());
                    st.error = Some(e.to_string());
                }
            }
        }
    }

    pub async fn get_status(&self) -> ClusterStatus {
        let lock = self.statuses.read().await;
        let mut nodes = Vec::new();
        let mut total_disk = 0u64;
        let mut avail_disk = 0u64;

        let cfg = self.config.read().unwrap().clone();
        for node_cfg in &cfg.nodes {
            if let Some(st) = lock.get(&node_cfg.id) {
                nodes.push(st.clone());
                total_disk = total_disk.saturating_add(st.disk_total_bytes);
                avail_disk = avail_disk.saturating_add(st.disk_available_bytes);
            }
        }

        ClusterStatus {
            nodes,
            total_disk_bytes: total_disk,
            available_disk_bytes: avail_disk,
            replicate_prefixes: cfg.replicate_prefixes.clone(),
        }
    }

    pub async fn get_topology(&self) -> ClusterTopology {
        {
            let lock = self.cached_topology.read().await;
            if let Some((ts, topo)) = &*lock {
                if ts.elapsed() < Duration::from_secs(15) {
                    return topo.clone();
                }
            }
        }

        let lock = self.statuses.read().await;
        let mut control_plane = None;
        let mut workers = Vec::new();
        let mut total_disk = 0u64;
        let mut avail_disk = 0u64;

        let cfg = self.config.read().unwrap().clone();
        for node_cfg in &cfg.nodes {
            if let Some(st) = lock.get(&node_cfg.id) {
                total_disk = total_disk.saturating_add(st.disk_total_bytes);
                avail_disk = avail_disk.saturating_add(st.disk_available_bytes);
                if st.is_local {
                    control_plane = Some(st.clone());
                } else {
                    workers.push(st.clone());
                }
            }
        }

        let mut distributions = Vec::new();
        if let Ok(mut rd) = tokio::fs::read_dir(&self.root).await {
            while let Ok(Some(entry)) = rd.next_entry().await {
                let fname = entry.file_name().to_string_lossy().to_string();
                if fname == crate::paths::META_DIR || fname.contains(crate::TMP_MARKER) {
                    continue;
                }
                if let Ok(ft) = entry.file_type().await {
                    if ft.is_dir() {
                        let (count, bytes) = self.scan_dir_summary(&entry.path()).await;
                        let replica_factor = if self.should_replicate(&fname) {
                            1 + workers.iter().filter(|w| w.healthy).count()
                        } else {
                            1
                        };
                        distributions.push(PrefixDistribution {
                            prefix: fname,
                            object_count: count,
                            total_bytes: bytes,
                            replica_factor,
                        });
                    }
                }
            }
        }
        distributions.sort_by(|a, b| b.total_bytes.cmp(&a.total_bytes));

        let topo = ClusterTopology {
            control_plane,
            workers,
            total_disk_bytes: total_disk,
            available_disk_bytes: avail_disk,
            distributions,
            replicate_prefixes: cfg.replicate_prefixes.clone(),
        };

        *self.cached_topology.write().await = Some((Instant::now(), topo.clone()));
        topo
    }

    async fn scan_dir_summary(&self, dir: &Path) -> (usize, u64) {
        let mut count = 0;
        let mut total_bytes = 0u64;
        let mut stack = vec![dir.to_path_buf()];
        while let Some(cur) = stack.pop() {
            let Ok(mut rd) = tokio::fs::read_dir(&cur).await else { continue };
            while let Ok(Some(e)) = rd.next_entry().await {
                let name = e.file_name().to_string_lossy().to_string();
                if name == crate::paths::META_DIR || name.contains(crate::TMP_MARKER) {
                    continue;
                }
                if let Ok(m) = e.metadata().await {
                    if m.is_dir() {
                        stack.push(e.path());
                    } else if m.is_file() {
                        count += 1;
                        total_bytes = total_bytes.saturating_add(m.len());
                    }
                }
            }
        }
        (count, total_bytes)
    }

    pub fn should_replicate(&self, rel_path: &str) -> bool {
        let prefixes = self.config.read().unwrap().replicate_prefixes.clone();
        if prefixes.is_empty() {
            return true;
        }
        for prefix in &prefixes {
            let p = prefix.trim_matches('/');
            if p == "*" || rel_path == p || rel_path.starts_with(&format!("{p}/")) {
                return true;
            }
        }
        false
    }

    /// Replicate an uploaded file to remote nodes asynchronously in background.
    pub fn replicate_put_bg(self: Arc<Self>, rel_path: String, abs_path: PathBuf) {
        if !self.should_replicate(&rel_path) {
            return;
        }
        tokio::spawn(async move {
            self.replicate_put(&rel_path, &abs_path).await;
        });
    }

    /// Replicate an uploaded file to all remote healthy nodes.
    pub async fn replicate_put(&self, rel_path: &str, abs_path: &Path) {
        let nodes = self.config.read().unwrap().nodes.clone();
        for node in nodes {
            if node.is_local {
                continue;
            }
            if node.node_type.as_deref() == Some("gdrive") || node.id == "gdrive" {
                if let Some(gdrive) = self.gdrive.read().await.clone() {
                    let md5 = match tokio::fs::metadata(abs_path).await {
                        Ok(meta) => self.cache.get_or_compute(abs_path, &meta).await.unwrap_or_default(),
                        Err(_) => String::new(),
                    };
                    if let Err(e) = gdrive.upload_file(rel_path, abs_path, &md5).await {
                        tracing::warn!(node = %node.id, path = %rel_path, error = %e, "gdrive replication failed");
                    }
                }
                continue;
            }
            let base = node.url.trim_end_matches('/');
            let target_url = format!("{base}/api/files/{rel_path}");

            let file = match tokio::fs::File::open(abs_path).await {
                Ok(f) => f,
                Err(e) => {
                    tracing::warn!(node = %node.id, path = %rel_path, error = %e, "cannot open file for replication");
                    continue;
                }
            };

            let stream = ReaderStream::new(file);
            let body = reqwest::Body::wrap_stream(stream);

            let mut req = self.client.put(&target_url).body(body);
            if !node.token.is_empty() {
                req = req.header("Authorization", format!("Bearer {}", node.token));
            }

            match req.send().await {
                Ok(resp) if resp.status().is_success() => {
                    tracing::info!(node = %node.id, path = %rel_path, "successfully replicated object");
                }
                Ok(resp) => {
                    tracing::warn!(node = %node.id, path = %rel_path, status = %resp.status(), "replication failed");
                }
                Err(e) => {
                    tracing::warn!(node = %node.id, path = %rel_path, error = %e, "replication request error");
                }
            }
        }
    }

    /// Replicate a delete to remote nodes.
    pub fn replicate_delete_bg(self: Arc<Self>, rel_path: String, recursive: bool) {
        if !self.should_replicate(&rel_path) {
            return;
        }
        let nodes = self.config.read().unwrap().nodes.clone();
        tokio::spawn(async move {
            for node in nodes {
                if node.is_local {
                    continue;
                }
                if node.node_type.as_deref() == Some("gdrive") || node.id == "gdrive" {
                    if let Some(gdrive) = self.gdrive.read().await.clone() {
                        let _ = gdrive.delete_file(&rel_path).await;
                    }
                    continue;
                }
                let base = node.url.trim_end_matches('/');
                let mut url = format!("{base}/api/files/{rel_path}");
                if recursive {
                    url.push_str("?recursive=1");
                }
                let mut req = self.client.delete(&url);
                if !node.token.is_empty() {
                    req = req.header("Authorization", format!("Bearer {}", node.token));
                }
                if let Err(e) = req.send().await {
                    tracing::warn!(node = %node.id, path = %rel_path, error = %e, "delete replication error");
                }
            }
        });
    }

    /// Check per-file replication status and topology across main and follower nodes.
    /// Supports tiered checks: fast metadata/size probe by default (<3ms), or deep hash verification.
    pub async fn check_file_status(&self, rel_path: &str, verify_hash: bool) -> FileReplicaTopology {
        let start_total = Instant::now();
        let rel_path = rel_path.trim_matches('/').to_string();
        let abs = self.root.join(&rel_path);

        // 1. Check main node
        let start_main = Instant::now();
        let main = match tokio::fs::metadata(&abs).await {
            Ok(meta) if meta.is_file() => {
                let md5 = if verify_hash {
                    self.cache.get_or_compute(&abs, &meta).await.ok()
                } else {
                    None
                };
                let mtime = meta.modified().ok().map(DateTime::<Utc>::from);
                MainFileNode {
                    id: "main".to_string(),
                    name: "cc-main".to_string(),
                    exists: true,
                    size: meta.len(),
                    md5: md5.clone(),
                    sha256: md5,
                    mtime,
                }
            }
            _ => MainFileNode {
                id: "main".to_string(),
                name: "cc-main".to_string(),
                exists: false,
                size: 0,
                md5: None,
                sha256: None,
                mtime: None,
            },
        };
        let main_lookup_ms = start_main.elapsed().as_millis() as u64;

        // 2. Concurrently check follower nodes
        let mut follower_nodes = Vec::new();
        let cfg_nodes = self.config.read().unwrap().nodes.clone();
        for node in &cfg_nodes {
            if node.is_local {
                continue;
            }
            follower_nodes.push(node.clone());
        }

        let main_md5 = main.md5.clone();
        let main_size = main.size;
        let main_exists = main.exists;
        let client = self.client.clone();
        let path_clone = rel_path.clone();
        let gdrive_client = self.gdrive.read().await.clone();
        let root_clone = self.root.clone();

        let checks_stream = futures::stream::iter(follower_nodes)
            .map(|node| {
                let client = client.clone();
                let path = path_clone.clone();
                let main_md5 = main_md5.clone();
                let gdrive = gdrive_client.clone();
                let root = root_clone.clone();
                async move {
                    if node.node_type.as_deref() == Some("gdrive") || node.id == "gdrive" {
                        if let Some(gdrive) = gdrive {
                            let start = Instant::now();
                            match gdrive.get_file_metadata(&path).await {
                                Ok(Some(meta)) => {
                                    let rtt = start.elapsed().as_millis() as u64;
                                    let md5 = meta.md5.clone();
                                    let matches = if verify_hash {
                                        main_exists && md5.is_some() && md5 == main_md5 && meta.size == main_size
                                    } else {
                                        main_exists && meta.size == main_size
                                    };
                                    FollowerFileNode {
                                        id: node.id,
                                        name: node.name,
                                        url: node.url,
                                        exists: true,
                                        matches,
                                        size: meta.size,
                                        md5: md5.clone(),
                                        sha256: md5,
                                        rtt_ms: Some(rtt),
                                        error: None,
                                    }
                                }
                                Ok(None) => {
                                    let rtt = start.elapsed().as_millis() as u64;
                                    FollowerFileNode {
                                        id: node.id,
                                        name: node.name,
                                        url: node.url,
                                        exists: false,
                                        matches: false,
                                        size: 0,
                                        md5: None,
                                        sha256: None,
                                        rtt_ms: Some(rtt),
                                        error: None,
                                    }
                                }
                                Err(e) => FollowerFileNode {
                                    id: node.id,
                                    name: node.name,
                                    url: node.url,
                                    exists: false,
                                    matches: false,
                                    size: 0,
                                    md5: None,
                                    sha256: None,
                                    rtt_ms: None,
                                    error: Some(e),
                                },
                            }
                        } else {
                            FollowerFileNode {
                                id: node.id,
                                name: node.name,
                                url: node.url,
                                exists: false,
                                matches: false,
                                size: 0,
                                md5: None,
                                sha256: None,
                                rtt_ms: None,
                                error: Some("GDrive backend uninitialized".to_string()),
                            }
                        }
                    } else {
                        let start = Instant::now();
                        let base = node.url.trim_end_matches('/');
                        let stat_url = if verify_hash {
                            format!("{base}/api/stat?path={}&checksums=1", urlencoding::encode(&path))
                        } else {
                            format!("{base}/api/stat?path={}", urlencoding::encode(&path))
                        };
                        let mut req = client.get(&stat_url);
                        if !node.token.is_empty() {
                            req = req.header("Authorization", format!("Bearer {}", node.token));
                        }

                        match req.send().await {
                            Ok(resp) if resp.status().is_success() => {
                                let rtt = start.elapsed().as_millis() as u64;
                                let val: serde_json::Value = resp.json().await.unwrap_or_default();
                                let size = val.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
                                let md5 = val
                                    .get("md5")
                                    .or_else(|| val.get("sha256"))
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.to_string());
                                let matches = if verify_hash {
                                    if let (Some(f_hash), Some(m_md5)) = (&md5, &main_md5) {
                                        if f_hash.len() == 64 {
                                            let abs = root.join(&path);
                                            let m_sha = crate::routes::files::sha256_file_real(&abs).await.ok();
                                            main_exists && m_sha.as_deref() == Some(f_hash.as_str()) && size == main_size
                                        } else {
                                            main_exists && f_hash == m_md5 && size == main_size
                                        }
                                    } else {
                                        false
                                    }
                                } else {
                                    main_exists && size == main_size
                                };

                                FollowerFileNode {
                                    id: node.id,
                                    name: node.name,
                                    url: node.url,
                                    exists: true,
                                    matches,
                                    size,
                                    md5: md5.clone(),
                                    sha256: md5,
                                    rtt_ms: Some(rtt),
                                    error: None,
                                }
                            }
                            Ok(resp) if resp.status() == reqwest::StatusCode::NOT_FOUND => {
                                let rtt = start.elapsed().as_millis() as u64;
                                FollowerFileNode {
                                    id: node.id,
                                    name: node.name,
                                    url: node.url,
                                    exists: false,
                                    matches: false,
                                    size: 0,
                                    md5: None,
                                    sha256: None,
                                    rtt_ms: Some(rtt),
                                    error: None,
                                }
                            }
                            Ok(resp) => {
                                let rtt = start.elapsed().as_millis() as u64;
                                FollowerFileNode {
                                    id: node.id,
                                    name: node.name,
                                    url: node.url,
                                    exists: false,
                                    matches: false,
                                    size: 0,
                                    md5: None,
                                    sha256: None,
                                    rtt_ms: Some(rtt),
                                    error: Some(format!("HTTP {}", resp.status())),
                                }
                            }
                            Err(e) => FollowerFileNode {
                                id: node.id,
                                name: node.name,
                                url: node.url,
                                exists: false,
                                matches: false,
                                size: 0,
                                md5: None,
                                sha256: None,
                                rtt_ms: None,
                                error: Some(e.to_string()),
                            },
                        }
                    }
                }
            })
            .buffer_unordered(8);

        let followers: Vec<FollowerFileNode> = checks_stream.collect().await;

        let mut follower_timings = HashMap::new();
        for f in &followers {
            if let Some(rtt) = f.rtt_ms {
                follower_timings.insert(f.id.clone(), rtt);
            }
        }

        let matching_followers = followers.iter().filter(|f| f.matches).count();
        let replica_count = if main.exists { 1 + matching_followers } else { matching_followers };
        let expected_replicas = 1 + followers.len();
        let fully_synced = main.exists && followers.iter().all(|f| f.matches);

        FileReplicaTopology {
            path: rel_path,
            fully_synced,
            replica_count,
            expected_replicas,
            main,
            followers,
            metrics: ProbeMetrics {
                total_ms: start_total.elapsed().as_millis() as u64,
                main_lookup_ms,
                follower_timings,
                verify_hash,
            },
        }
    }

    pub async fn get_node_statuses(&self) -> Vec<NodeStatus> {
        let nodes_map = self.statuses.read().await;
        let mut list = Vec::new();
        let cfg_nodes = self.config.read().unwrap().nodes.clone();
        for n in &cfg_nodes {
            if let Some(st) = nodes_map.get(&n.id) {
                list.push(st.clone());
            }
        }
        list
    }

    pub async fn cancel_sync(&self) -> bool {
        let guard = self.active_cancel.read().await;
        if let Some(cancel) = guard.as_ref() {
            cancel.cancel();
            true
        } else {
            false
        }
    }

    /// Scan and compute replication differences without altering remote state (dry run / preview).
    pub async fn get_sync_diff(
        &self,
        prefix: &str,
        paths: Option<&[String]>,
        target_nodes: Option<&[String]>,
    ) -> Result<SyncDiffReport, String> {
        let prefix = prefix.trim_matches('/').to_string();
        let local_files = if let Some(p) = paths {
            let mut list = Vec::new();
            for rel in p {
                let rel = rel.trim_matches('/').to_string();
                let abs = self.root.join(&rel);
                if let Ok(meta) = tokio::fs::metadata(&abs).await {
                    if meta.is_file() {
                        if let Ok(hash) = self.cache.get_or_compute(&abs, &meta).await {
                            list.push((rel, hash, meta.len()));
                        }
                    }
                }
            }
            list
        } else {
            let entries = self.list_local_files_recursive(&prefix).await;
            let mut list = Vec::new();
            for (rel, hash) in entries {
                let abs = self.root.join(&rel);
                let size = tokio::fs::metadata(&abs).await.map(|m| m.len()).unwrap_or(0);
                list.push((rel, hash, size));
            }
            list
        };

        let mut nodes_to_check = Vec::new();
        let cfg_nodes = self.config.read().unwrap().nodes.clone();
        for n in &cfg_nodes {
            if n.is_local {
                continue;
            }
            if let Some(targets) = target_nodes {
                if !targets.iter().any(|t| t == &n.id) {
                    continue;
                }
            }
            nodes_to_check.push(n.clone());
        }

        let mut remote_maps: HashMap<String, HashMap<String, String>> = HashMap::new();
        for node in &nodes_to_check {
            if node.node_type.as_deref() == Some("gdrive") || node.id == "gdrive" {
                if let Some(gdrive) = self.gdrive.read().await.clone() {
                    if let Ok(gdrive_items) = gdrive.list_dir(&prefix).await {
                        let mut map = HashMap::new();
                        for item in gdrive_items {
                            if !item.is_dir {
                                let item_rel = if prefix.is_empty() {
                                    item.name.clone()
                                } else {
                                    format!("{prefix}/{}", item.name)
                                };
                                if let Some(md5) = item.md5 {
                                    map.insert(item_rel, md5);
                                }
                            }
                        }
                        remote_maps.insert(node.id.clone(), map);
                    }
                }
                continue;
            }

            let base = node.url.trim_end_matches('/');
            let list_url = format!("{base}/api/list?path={prefix}&recursive=1&checksums=1");
            let mut req = self.client.get(&list_url);
            if !node.token.is_empty() {
                req = req.header("Authorization", format!("Bearer {}", node.token));
            }
            if let Ok(resp) = req.send().await {
                if resp.status().is_success() {
                    let val: serde_json::Value = resp.json().await.unwrap_or_default();
                    let mut map = HashMap::new();
                    if let Some(entries) = val.get("entries").and_then(|v| v.as_array()) {
                        for e in entries {
                            if e.get("type").and_then(|v| v.as_str()) == Some("file") {
                                if let (Some(path), Some(hash)) = (
                                    e.get("path").and_then(|v| v.as_str()),
                                    e.get("md5").or_else(|| e.get("sha256")).and_then(|v| v.as_str()),
                                ) {
                                    map.insert(path.to_string(), hash.to_string());
                                }
                            }
                        }
                    }
                    remote_maps.insert(node.id.clone(), map);
                } else if resp.status() == reqwest::StatusCode::NOT_FOUND {
                    remote_maps.insert(node.id.clone(), HashMap::new());
                }
            }
        }

        let mut differences = Vec::new();
        let mut in_sync_count = 0;

        for (rel, local_hash, size) in &local_files {
            let mut missing_on = Vec::new();
            let mut mismatch_on = Vec::new();
            let mut in_sync_on = Vec::new();

            for node in &nodes_to_check {
                if let Some(map) = remote_maps.get(&node.id) {
                    if let Some(r_hash) = map.get(rel) {
                        let is_match = r_hash == local_hash;
                        if is_match {
                            in_sync_on.push(node.id.clone());
                        } else {
                            mismatch_on.push(node.id.clone());
                        }
                    } else {
                        missing_on.push(node.id.clone());
                    }
                } else {
                    missing_on.push(node.id.clone());
                }
            }

            if missing_on.is_empty() && mismatch_on.is_empty() {
                in_sync_count += 1;
            } else {
                differences.push(FileDiffEntry {
                    path: rel.clone(),
                    size: *size,
                    hash: local_hash.clone(),
                    missing_on,
                    mismatch_on,
                    in_sync_on,
                });
            }
        }

        Ok(SyncDiffReport {
            total_scanned: local_files.len(),
            diff_count: differences.len(),
            in_sync_count,
            differences,
        })
    }

    /// Sync a directory prefix from local master to remote worker nodes with safe default concurrency and CPU governor.
    pub async fn sync_prefix(&self, prefix: &str) -> SyncReport {
        self.sync_custom(
            CustomSyncOptions {
                prefix: Some(prefix.to_string()),
                ..Default::default()
            },
            None,
        )
        .await
    }

    /// Custom interactive & selectable synchronization across target nodes and paths.
    pub async fn sync_custom(&self, opts: CustomSyncOptions, metrics: Option<&crate::metrics::Metrics>) -> SyncReport {
        let cancel = CancellationToken::new();
        *self.active_cancel.write().await = Some(cancel.clone());
        self.is_syncing.store(true, Ordering::SeqCst);
        if let Some(m) = metrics {
            m.sync_in_progress.store(1, Ordering::Relaxed);
        }

        let start = Instant::now();
        let prefix = opts.prefix.as_deref().unwrap_or("").trim_matches('/').to_string();

        let local_files = if let Some(p) = &opts.paths {
            let mut list = Vec::new();
            for rel in p {
                let rel = rel.trim_matches('/').to_string();
                let abs = self.root.join(&rel);
                if let Ok(meta) = tokio::fs::metadata(&abs).await {
                    if meta.is_file() {
                        if let Ok(hash) = self.cache.get_or_compute(&abs, &meta).await {
                            list.push((rel, hash));
                        }
                    }
                }
            }
            list
        } else {
            self.list_local_files_recursive(&prefix).await
        };

        let mut nodes_to_sync = Vec::new();
        let cfg_nodes = self.config.read().unwrap().nodes.clone();
        for node in &cfg_nodes {
            if node.is_local {
                continue;
            }
            if let Some(targets) = &opts.target_nodes {
                if !targets.iter().any(|t| t == &node.id) {
                    continue;
                }
            }
            nodes_to_sync.push(node.clone());
        }

        let concurrency = opts.concurrency.unwrap_or(3).clamp(1, 6);
        let total_files_estimated = local_files.len() * nodes_to_sync.len();

        *self.live_progress.write().await = LiveSyncProgress {
            is_syncing: true,
            current_file: String::new(),
            current_node: String::new(),
            total_to_sync: total_files_estimated,
            synced_count: 0,
            skipped_count: 0,
            failed_count: 0,
            percent: 0.0,
            start_time_ms: chrono::Utc::now().timestamp_millis() as u64,
            elapsed_ms: 0,
        };

        let mut node_results = Vec::new();

        for node in &nodes_to_sync {
            if cancel.is_cancelled() {
                break;
            }

            let mut res = NodeSyncResult {
                node_id: node.id.clone(),
                total_checked: local_files.len(),
                synced: 0,
                skipped: 0,
                errors: Vec::new(),
            };

            if node.node_type.as_deref() == Some("gdrive") || node.id == "gdrive" {
                if let Some(gdrive) = self.gdrive.read().await.clone() {
                    match gdrive.list_dir(&prefix).await {
                        Ok(gdrive_items) => {
                            let mut remote_map = HashMap::new();
                            for item in gdrive_items {
                                if !item.is_dir {
                                    let item_rel = if prefix.is_empty() {
                                        item.name.clone()
                                    } else {
                                        format!("{prefix}/{}", item.name)
                                    };
                                    if let Some(md5) = item.md5 {
                                        remote_map.insert(item_rel, md5);
                                    }
                                }
                            }

                            for (rel_path, md5) in &local_files {
                                if cancel.is_cancelled() {
                                    break;
                                }

                                // CPU safety governor & cooperative yield
                                if let Some(m) = metrics {
                                    if m.current_system_cpu_ratio() > 0.75 {
                                        tokio::time::sleep(Duration::from_millis(50)).await;
                                    }
                                }
                                tokio::task::yield_now().await;
                                tokio::time::sleep(Duration::from_millis(5)).await;

                                if let Some(r_md5) = remote_map.get(rel_path) {
                                    if r_md5 == md5 {
                                        res.skipped += 1;
                                        continue;
                                    }
                                }
                                let abs_path = self.root.join(rel_path);

                                {
                                    let mut p = self.live_progress.write().await;
                                    p.current_file = rel_path.clone();
                                    p.current_node = node.id.clone();
                                    p.elapsed_ms = start.elapsed().as_millis() as u64;
                                }

                                match gdrive.upload_file(rel_path, &abs_path, md5).await {
                                    Ok(_) => {
                                        res.synced += 1;
                                        let mut p = self.live_progress.write().await;
                                        p.synced_count += 1;
                                        if p.total_to_sync > 0 {
                                            p.percent = (p.synced_count as f64 / p.total_to_sync as f64) * 100.0;
                                        }
                                    }
                                    Err(e) => {
                                        res.errors.push(format!("gdrive sync {rel_path} error: {e}"));
                                        let mut p = self.live_progress.write().await;
                                        p.failed_count += 1;
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            res.errors.push(format!("failed to list gdrive dir {prefix}: {e}"));
                        }
                    }
                    node_results.push(res);
                }
                continue;
            }

            // Remote ostore node
            let base = node.url.trim_end_matches('/');
            let list_url = format!("{base}/api/list?path={prefix}&recursive=1&checksums=1");
            let mut req = self.client.get(&list_url);
            if !node.token.is_empty() {
                req = req.header("Authorization", format!("Bearer {}", node.token));
            }

            let remote_map = match req.send().await {
                Ok(resp) if resp.status().is_success() => {
                    let val: serde_json::Value = resp.json().await.unwrap_or_default();
                    let mut map = HashMap::new();
                    if let Some(entries) = val.get("entries").and_then(|v| v.as_array()) {
                        for e in entries {
                            if e.get("type").and_then(|v| v.as_str()) == Some("file") {
                                if let (Some(path), Some(hash)) = (
                                    e.get("path").and_then(|v| v.as_str()),
                                    e.get("md5").or_else(|| e.get("sha256")).and_then(|v| v.as_str()),
                                ) {
                                    map.insert(path.to_string(), hash.to_string());
                                }
                            }
                        }
                    }
                    map
                }
                Ok(resp) if resp.status() == reqwest::StatusCode::NOT_FOUND => HashMap::new(),
                Ok(resp) => {
                    res.errors.push(format!("failed to fetch remote list: HTTP {}", resp.status()));
                    node_results.push(res);
                    continue;
                }
                Err(e) => {
                    res.errors.push(format!("failed to connect to remote node: {e}"));
                    node_results.push(res);
                    continue;
                }
            };

            let mut to_sync = Vec::new();
            for (rel_path, md5) in &local_files {
                if let Some(r_hash) = remote_map.get(rel_path) {
                    if r_hash == md5 {
                        res.skipped += 1;
                        continue;
                    }
                    if r_hash.len() == 64 {
                        let abs = self.root.join(rel_path);
                        if let Ok(m_sha) = crate::routes::files::sha256_file_real(&abs).await {
                            if &m_sha == r_hash {
                                res.skipped += 1;
                                continue;
                            }
                        }
                    }
                }
                to_sync.push(rel_path.clone());
            }

            let client = self.client.clone();
            let base_url = base.to_string();
            let token = node.token.clone();
            let root = self.root.clone();
            let cancel_clone = cancel.clone();
            let progress_ref = self.live_progress.clone();
            let node_id_clone = node.id.clone();
            let start_instant = start;

            let upload_stream = futures::stream::iter(to_sync)
                .map(|rel_path| {
                    let client = client.clone();
                    let base_url = base_url.clone();
                    let token = token.clone();
                    let root = root.clone();
                    let cancel = cancel_clone.clone();
                    let progress = progress_ref.clone();
                    let node_id = node_id_clone.clone();
                    async move {
                        if cancel.is_cancelled() {
                            return Err("sync cancelled".to_string());
                        }

                        // CPU safety governor yield
                        tokio::task::yield_now().await;
                        tokio::time::sleep(Duration::from_millis(5)).await;

                        {
                            let mut p = progress.write().await;
                            p.current_file = rel_path.clone();
                            p.current_node = node_id;
                            p.elapsed_ms = start_instant.elapsed().as_millis() as u64;
                        }

                        let abs_path = root.join(&rel_path);
                        let upload_url = format!("{base_url}/api/files/{rel_path}");
                        let file = match tokio::fs::File::open(&abs_path).await {
                            Ok(f) => f,
                            Err(e) => return Err(format!("cannot open local {rel_path}: {e}")),
                        };
                        let stream = ReaderStream::new(file);
                        let body = reqwest::Body::wrap_stream(stream);
                        let mut put_req = client.put(&upload_url).body(body);
                        if !token.is_empty() {
                            put_req = put_req.header("Authorization", format!("Bearer {token}"));
                        }
                        match put_req.send().await {
                            Ok(resp) if resp.status().is_success() => {
                                let mut p = progress.write().await;
                                p.synced_count += 1;
                                if p.total_to_sync > 0 {
                                    p.percent = (p.synced_count as f64 / p.total_to_sync as f64) * 100.0;
                                }
                                Ok(())
                            }
                            Ok(resp) => {
                                let mut p = progress.write().await;
                                p.failed_count += 1;
                                Err(format!("sync {rel_path} HTTP {}", resp.status()))
                            }
                            Err(e) => {
                                let mut p = progress.write().await;
                                p.failed_count += 1;
                                Err(format!("sync {rel_path} error: {e}"))
                            }
                        }
                    }
                })
                .buffer_unordered(concurrency);

            let results: Vec<Result<(), String>> = upload_stream.collect().await;
            for r in results {
                match r {
                    Ok(()) => res.synced += 1,
                    Err(e) => res.errors.push(e),
                }
            }

            node_results.push(res);
        }

        let total_checked = node_results.iter().map(|n| n.total_checked).max().unwrap_or(0);
        let total_synced: usize = node_results.iter().map(|n| n.synced).sum();
        let total_skipped: usize = node_results.iter().map(|n| n.skipped).sum();
        let total_errors: usize = node_results.iter().map(|n| n.errors.len()).sum();
        let duration_ms = start.elapsed().as_millis() as u64;

        if let Some(m) = metrics {
            m.sync_in_progress.store(0, Ordering::Relaxed);
            m.sync_total_synced.fetch_add(total_synced as u64, Ordering::Relaxed);
            m.sync_total_skipped.fetch_add(total_skipped as u64, Ordering::Relaxed);
            m.sync_total_failed.fetch_add(total_errors as u64, Ordering::Relaxed);
            m.sync_last_duration_ms.store(duration_ms, Ordering::Relaxed);
            m.sync_last_synced.store(total_synced as u64, Ordering::Relaxed);
        }

        let report = SyncReport {
            prefix,
            total_checked,
            total_synced,
            total_skipped,
            synced: total_synced,
            duration_ms,
            timestamp: Utc::now(),
            node_results,
        };

        *self.last_sync_report.write().await = Some(report.clone());
        {
            let mut p = self.live_progress.write().await;
            p.is_syncing = false;
            p.percent = 100.0;
            p.elapsed_ms = duration_ms;
        }

        *self.active_cancel.write().await = None;
        self.is_syncing.store(false, Ordering::SeqCst);
        report
    }

    pub async fn get_sync_status(&self) -> ClusterSyncStatus {
        let is_syncing = self.is_syncing.load(Ordering::SeqCst);
        let progress = self.live_progress.read().await.clone();
        let last_sync = self.last_sync_report.read().await.clone();
        let nodes = self.get_node_statuses().await;
        ClusterSyncStatus {
            is_syncing,
            progress,
            last_sync,
            nodes,
        }
    }

    pub async fn check_dir_status(&self, prefix: &str) -> DirReplicaStatus {
        let prefix = prefix.trim_matches('/').to_string();
        let abs_dir = if prefix.is_empty() {
            self.root.clone()
        } else {
            self.root.join(&prefix)
        };

        let mut local_files: HashMap<String, u64> = HashMap::new();
        if let Ok(mut rd) = tokio::fs::read_dir(&abs_dir).await {
            while let Ok(Some(entry)) = rd.next_entry().await {
                let name = entry.file_name().to_string_lossy().to_string();
                if crate::routes::list::hidden(&name) {
                    continue;
                }
                if let Ok(m) = entry.metadata().await {
                    if m.is_file() {
                        local_files.insert(name, m.len());
                    }
                }
            }
        }

        let mut follower_nodes = Vec::new();
        let cfg_nodes = self.config.read().unwrap().nodes.clone();
        for node in &cfg_nodes {
            if !node.is_local {
                follower_nodes.push(node.clone());
            }
        }

        let client = self.client.clone();
        let prefix_clone = prefix.clone();
        let gdrive_client = self.gdrive.read().await.clone();
        let fetches = futures::stream::iter(follower_nodes)
            .map(|node| {
                let client = client.clone();
                let prefix = prefix_clone.clone();
                let gdrive = gdrive_client.clone();
                async move {
                    if node.node_type.as_deref() == Some("gdrive") || node.id == "gdrive" {
                        if let Some(gdrive) = gdrive {
                            let mut files = HashMap::new();
                            if let Ok(items) = gdrive.list_dir(&prefix).await {
                                for item in items {
                                    if !item.is_dir {
                                        files.insert(item.name, item.size);
                                    }
                                }
                            }
                            return (node.id, files);
                        } else {
                            return (node.id, HashMap::new());
                        }
                    }
                    let base = node.url.trim_end_matches('/');
                    let list_url = format!("{base}/api/list?path={}", urlencoding::encode(&prefix));
                    let mut req = client.get(&list_url);
                    if !node.token.is_empty() {
                        req = req.header("Authorization", format!("Bearer {}", node.token));
                    }
                    let map = match req.send().await {
                        Ok(resp) if resp.status().is_success() => {
                            let val: serde_json::Value = resp.json().await.unwrap_or_default();
                            let mut files = HashMap::new();
                            if let Some(entries) = val.get("entries").and_then(|v| v.as_array()) {
                                for e in entries {
                                    if e.get("type").and_then(|v| v.as_str()) == Some("file") {
                                        if let (Some(name), Some(size)) = (
                                            e.get("name").and_then(|v| v.as_str()),
                                            e.get("size").and_then(|v| v.as_u64()),
                                        ) {
                                            files.insert(name.to_string(), size);
                                        }
                                    }
                                }
                            }
                            files
                        }
                        _ => HashMap::new(),
                    };
                    (node.id, map)
                }
            })
            .buffer_unordered(8)
            .collect::<Vec<_>>()
            .await;

        let mut follower_maps: HashMap<String, HashMap<String, u64>> = HashMap::new();
        for (node_id, map) in fetches {
            follower_maps.insert(node_id, map);
        }

        let expected_replicas = 1 + self.config.read().unwrap().nodes.iter().filter(|n| !n.is_local).count();
        let mut files_summary = HashMap::new();

        for (fname, local_size) in local_files {
            let rel_path = if prefix.is_empty() {
                fname.clone()
            } else {
                format!("{prefix}/{fname}")
            };
            let cc2_match = follower_maps
                .get("cc2")
                .and_then(|m| m.get(&fname))
                .map(|s| *s == local_size)
                .unwrap_or(false);
            let rn_match = follower_maps
                .get("racknerd")
                .and_then(|m| m.get(&fname))
                .map(|s| *s == local_size)
                .unwrap_or(false);

            let mut count = 1;
            for (_node_id, m) in &follower_maps {
                if let Some(s) = m.get(&fname) {
                    if *s == local_size {
                        count += 1;
                    }
                }
            }

            files_summary.insert(
                rel_path,
                FileReplicaSummary {
                    cc2: cc2_match,
                    rn: rn_match,
                    replica_count: count,
                    expected_replicas,
                },
            );
        }

        DirReplicaStatus {
            prefix,
            files: files_summary,
        }
    }

    async fn list_local_files_recursive(&self, prefix: &str) -> Vec<(String, String)> {
        let dir = if prefix.is_empty() {
            self.root.clone()
        } else {
            self.root.join(prefix)
        };

        let mut results = Vec::new();
        if !dir.exists() {
            return results;
        }

        let mut stack = vec![dir];
        while let Some(current) = stack.pop() {
            let mut rd = match tokio::fs::read_dir(&current).await {
                Ok(r) => r,
                Err(_) => continue,
            };

            while let Ok(Some(entry)) = rd.next_entry().await {
                let path = entry.path();
                let fname = entry.file_name().to_string_lossy().to_string();
                if fname == crate::paths::META_DIR || fname.contains(crate::TMP_MARKER) {
                    continue;
                }

                let ft = match entry.file_type().await {
                    Ok(ft) => ft,
                    Err(_) => continue,
                };

                if ft.is_dir() {
                    stack.push(path);
                } else if ft.is_file() {
                    if let Ok(rel) = path.strip_prefix(&self.root) {
                        let rel_str = rel.to_string_lossy().to_string();
                        if let Ok(m) = entry.metadata().await {
                            if let Ok(sha) = self.cache.get_or_compute(&path, &m).await {
                                results.push((rel_str, sha));
                            }
                        }
                    }
                }
            }
        }

        results
    }

    /// Dynamically connect and persist Google Drive credentials to /etc/ostore-cluster.json and activate cloud node.
    pub async fn update_and_persist_gdrive(&self, cfg: GDriveConfig) -> Result<(), String> {
        let client = Arc::new(GDriveClient::new(cfg.clone()));
        let (total, avail) = client.get_storage_quota().await?;

        // 1. Update /etc/ostore-cluster.json on disk
        let config_path = Path::new("/etc/ostore-cluster.json");
        let mut cluster_cfg = if config_path.exists() {
            let s = std::fs::read_to_string(config_path).map_err(|e| e.to_string())?;
            serde_json::from_str::<ClusterConfig>(&s).unwrap_or_default()
        } else {
            ClusterConfig::default()
        };

        let gdrive_node = NodeConfig {
            id: "gdrive".to_string(),
            name: "Google Drive Cloud".to_string(),
            url: "".to_string(),
            token: "".to_string(),
            is_local: false,
            node_type: Some("gdrive".to_string()),
            gdrive_client_id: Some(cfg.client_id.clone()),
            gdrive_client_secret: Some(cfg.client_secret.clone()),
            gdrive_refresh_token: Some(cfg.refresh_token.clone()),
            gdrive_root_folder_id: Some(cfg.root_folder_id.clone()),
        };

        if let Some(pos) = cluster_cfg.nodes.iter().position(|n| n.id == "gdrive") {
            cluster_cfg.nodes[pos] = gdrive_node.clone();
        } else {
            cluster_cfg.nodes.push(gdrive_node.clone());
        }

        let json_text = serde_json::to_string_pretty(&cluster_cfg).map_err(|e| e.to_string())?;
        std::fs::write(config_path, json_text).map_err(|e| e.to_string())?;

        // 2. Update in-memory state
        *self.gdrive.write().await = Some(client);
        *self.config.write().unwrap() = cluster_cfg.clone();
        let mut statuses = self.statuses.write().await;
        statuses.insert(
            "gdrive".to_string(),
            NodeStatus {
                id: "gdrive".to_string(),
                name: "Google Drive Cloud".to_string(),
                url: "".to_string(),
                is_local: false,
                healthy: true,
                rtt_ms: Some(15),
                disk_total_bytes: total,
                disk_available_bytes: avail,
                last_seen: Some(Utc::now()),
                error: None,
                node_type: Some("gdrive".to_string()),
            },
        );

        *self.cached_topology.write().await = None;

        Ok(())
    }

    /// Disconnect Google Drive cloud node, remove from configuration and memory.
    pub async fn remove_and_persist_gdrive(&self) -> Result<(), String> {
        let config_path = Path::new("/etc/ostore-cluster.json");
        if config_path.exists() {
            let s = std::fs::read_to_string(config_path).map_err(|e| e.to_string())?;
            if let Ok(mut cluster_cfg) = serde_json::from_str::<ClusterConfig>(&s) {
                cluster_cfg
                    .nodes
                    .retain(|n| n.id != "gdrive" && n.node_type.as_deref() != Some("gdrive"));
                let json_text = serde_json::to_string_pretty(&cluster_cfg).map_err(|e| e.to_string())?;
                let _ = std::fs::write(config_path, json_text);
                *self.config.write().unwrap() = cluster_cfg;
            }
        }

        *self.gdrive.write().await = None;
        let mut statuses = self.statuses.write().await;
        statuses.remove("gdrive");

        *self.cached_topology.write().await = None;

        Ok(())
    }
}
