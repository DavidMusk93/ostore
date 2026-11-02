//! Cluster management and multi-node storage replication.
//!
//! Provides a unified control plane across multiple ostore instances,
//! periodic node health/disk monitoring, automatic replication on write,
//! and on-demand cluster synchronization.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tokio_util::io::ReaderStream;
use tokio_util::sync::CancellationToken;

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
    pub sha256: Option<String>,
    pub rtt_ms: Option<u64>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct FileReplicaTopology {
    pub path: String,
    pub fully_synced: bool,
    pub replica_count: usize,
    pub expected_replicas: usize,
    pub main: MainFileNode,
    pub followers: Vec<FollowerFileNode>,
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

#[derive(Clone, Debug, Serialize)]
pub struct NodeSyncResult {
    pub node_id: String,
    pub total_checked: usize,
    pub synced: usize,
    pub skipped: usize,
    pub errors: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct SyncReport {
    pub prefix: String,
    pub node_results: Vec<NodeSyncResult>,
    pub duration_ms: u64,
}

pub struct ClusterManager {
    pub config: ClusterConfig,
    client: reqwest::Client,
    statuses: Arc<RwLock<HashMap<String, NodeStatus>>>,
    root: PathBuf,
    cache: Arc<crate::ChecksumCache>,
}

impl ClusterManager {
    pub fn new(config: ClusterConfig, root: PathBuf, cache: Arc<crate::ChecksumCache>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(5))
            .build()
            .unwrap_or_default();

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
                },
            );
        }

        Self {
            config,
            client,
            statuses: Arc::new(RwLock::new(initial_statuses)),
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
        for node in &self.config.nodes {
            if node.is_local {
                if let Some((total, avail)) = crate::metrics::disk_space(&self.root) {
                    let mut lock = self.statuses.write().await;
                    if let Some(st) = lock.get_mut(&node.id) {
                        st.healthy = true;
                        st.rtt_ms = Some(0);
                        st.disk_total_bytes = total;
                        st.disk_available_bytes = avail;
                        st.last_seen = Some(Utc::now());
                        st.error = None;
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
                    if let Some(st) = lock.get_mut(&node.id) {
                        st.healthy = true;
                        st.rtt_ms = Some(rtt);
                        st.disk_total_bytes = total;
                        st.disk_available_bytes = avail;
                        st.last_seen = Some(Utc::now());
                        st.error = None;
                    }
                }
                Ok(resp) => {
                    let mut lock = self.statuses.write().await;
                    if let Some(st) = lock.get_mut(&node.id) {
                        st.healthy = false;
                        st.rtt_ms = None;
                        st.last_seen = Some(Utc::now());
                        st.error = Some(format!("HTTP {}", resp.status()));
                    }
                }
                Err(e) => {
                    let mut lock = self.statuses.write().await;
                    if let Some(st) = lock.get_mut(&node.id) {
                        st.healthy = false;
                        st.rtt_ms = None;
                        st.last_seen = Some(Utc::now());
                        st.error = Some(e.to_string());
                    }
                }
            }
        }
    }

    pub async fn get_status(&self) -> ClusterStatus {
        let lock = self.statuses.read().await;
        let mut nodes = Vec::new();
        let mut total_disk = 0u64;
        let mut avail_disk = 0u64;

        for node_cfg in &self.config.nodes {
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
            replicate_prefixes: self.config.replicate_prefixes.clone(),
        }
    }

    pub async fn get_topology(&self) -> ClusterTopology {
        let lock = self.statuses.read().await;
        let mut control_plane = None;
        let mut workers = Vec::new();
        let mut total_disk = 0u64;
        let mut avail_disk = 0u64;

        for node_cfg in &self.config.nodes {
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

        ClusterTopology {
            control_plane,
            workers,
            total_disk_bytes: total_disk,
            available_disk_bytes: avail_disk,
            distributions,
            replicate_prefixes: self.config.replicate_prefixes.clone(),
        }
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
        if self.config.replicate_prefixes.is_empty() {
            return true;
        }
        for prefix in &self.config.replicate_prefixes {
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
        let nodes = self.config.nodes.clone();
        for node in nodes {
            if node.is_local {
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
        tokio::spawn(async move {
            let nodes = self.config.nodes.clone();
            for node in nodes {
                if node.is_local {
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
    pub async fn check_file_status(&self, rel_path: &str) -> FileReplicaTopology {
        let rel_path = rel_path.trim_matches('/').to_string();
        let abs = self.root.join(&rel_path);

        // 1. Check main node
        let main = match tokio::fs::metadata(&abs).await {
            Ok(meta) if meta.is_file() => {
                let sha = self.cache.get_or_compute(&abs, &meta).await.ok();
                let mtime = meta.modified().ok().map(DateTime::<Utc>::from);
                MainFileNode {
                    id: "main".to_string(),
                    name: "cc-main".to_string(),
                    exists: true,
                    size: meta.len(),
                    sha256: sha,
                    mtime,
                }
            }
            _ => MainFileNode {
                id: "main".to_string(),
                name: "cc-main".to_string(),
                exists: false,
                size: 0,
                sha256: None,
                mtime: None,
            },
        };

        // 2. Concurrently check follower nodes
        let mut follower_nodes = Vec::new();
        for node in &self.config.nodes {
            if node.is_local {
                continue;
            }
            follower_nodes.push(node.clone());
        }

        let main_sha = main.sha256.clone();
        let main_size = main.size;
        let main_exists = main.exists;
        let client = self.client.clone();
        let path_clone = rel_path.clone();

        let checks_stream = futures::stream::iter(follower_nodes)
            .map(|node| {
                let client = client.clone();
                let path = path_clone.clone();
                let main_sha = main_sha.clone();
                async move {
                    let start = Instant::now();
                    let base = node.url.trim_end_matches('/');
                    let stat_url = format!("{base}/api/stat?path={}&checksums=1", urlencoding::encode(&path));
                    let mut req = client.get(&stat_url);
                    if !node.token.is_empty() {
                        req = req.header("Authorization", format!("Bearer {}", node.token));
                    }

                    match req.send().await {
                        Ok(resp) if resp.status().is_success() => {
                            let rtt = start.elapsed().as_millis() as u64;
                            let val: serde_json::Value = resp.json().await.unwrap_or_default();
                            let size = val.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
                            let sha = val.get("sha256").and_then(|v| v.as_str()).map(|s| s.to_string());
                            let matches = main_exists && sha.is_some() && sha == main_sha && size == main_size;

                            FollowerFileNode {
                                id: node.id,
                                name: node.name,
                                url: node.url,
                                exists: true,
                                matches,
                                size,
                                sha256: sha,
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
                                sha256: None,
                                rtt_ms: Some(rtt),
                                error: None,
                            }
                        }
                        Ok(resp) => {
                            FollowerFileNode {
                                id: node.id,
                                name: node.name,
                                url: node.url,
                                exists: false,
                                matches: false,
                                size: 0,
                                sha256: None,
                                rtt_ms: None,
                                error: Some(format!("HTTP {}", resp.status())),
                            }
                        }
                        Err(e) => {
                            FollowerFileNode {
                                id: node.id,
                                name: node.name,
                                url: node.url,
                                exists: false,
                                matches: false,
                                size: 0,
                                sha256: None,
                                rtt_ms: None,
                                error: Some(e.to_string()),
                            }
                        }
                    }
                }
            })
            .buffer_unordered(8);

        let followers: Vec<FollowerFileNode> = checks_stream.collect().await;

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
        }
    }

    /// Sync a directory prefix from local master to all remote worker nodes.
    pub async fn sync_prefix(&self, prefix: &str) -> SyncReport {
        let start = Instant::now();
        let prefix = prefix.trim_matches('/').to_string();

        // 1. Gather all local files with sha256 under prefix
        let local_files = self.list_local_files_recursive(&prefix).await;
        let mut node_results = Vec::new();

        for node in &self.config.nodes {
            if node.is_local {
                continue;
            }

            let mut res = NodeSyncResult {
                node_id: node.id.clone(),
                total_checked: local_files.len(),
                synced: 0,
                skipped: 0,
                errors: Vec::new(),
            };

            // 2. Query remote node list with checksums
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
                                if let (Some(path), Some(sha)) = (
                                    e.get("path").and_then(|v| v.as_str()),
                                    e.get("sha256").and_then(|v| v.as_str()),
                                ) {
                                    map.insert(path.to_string(), sha.to_string());
                                }
                            }
                        }
                    }
                    map
                }
                Ok(resp) if resp.status() == reqwest::StatusCode::NOT_FOUND => {
                    // Target directory does not exist on remote node yet; treat as empty
                    HashMap::new()
                }
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

            // 3. Compare and push missing or modified files concurrently
            let mut to_sync = Vec::new();
            for (rel_path, sha256) in &local_files {
                if let Some(r_sha) = remote_map.get(rel_path) {
                    if r_sha == sha256 {
                        res.skipped += 1;
                        continue;
                    }
                }
                to_sync.push(rel_path.clone());
            }

            let client = self.client.clone();
            let base_url = base.to_string();
            let token = node.token.clone();
            let root = self.root.clone();

            let upload_stream = futures::stream::iter(to_sync)
                .map(|rel_path| {
                    let client = client.clone();
                    let base_url = base_url.clone();
                    let token = token.clone();
                    let root = root.clone();
                    async move {
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
                            Ok(resp) if resp.status().is_success() => Ok(()),
                            Ok(resp) => Err(format!("sync {rel_path} HTTP {}", resp.status())),
                            Err(e) => Err(format!("sync {rel_path} error: {e}")),
                        }
                    }
                })
                .buffer_unordered(16);

            let results: Vec<Result<(), String>> = upload_stream.collect().await;
            for r in results {
                match r {
                    Ok(()) => res.synced += 1,
                    Err(e) => res.errors.push(e),
                }
            }

            node_results.push(res);
        }

        SyncReport {
            prefix,
            node_results,
            duration_ms: start.elapsed().as_millis() as u64,
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
}
