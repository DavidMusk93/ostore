//! Cluster status and synchronization routes.

use axum::extract::{Query, State};
use axum::response::Json;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::AppState;
use crate::error::ApiError;

#[derive(Deserialize, Default)]
pub struct SyncQuery {
    #[serde(default)]
    pub prefix: String,
}

/// GET /api/cluster/nodes — return all cluster nodes status and total storage.
pub async fn get_nodes(State(st): State<AppState>) -> Result<Json<Value>, ApiError> {
    match &st.cluster {
        Some(cluster) => {
            let status = cluster.get_status().await;
            Ok(Json(json!(status)))
        }
        None => {
            // Standalone mode: return single local node
            let (total, avail) = crate::metrics::disk_space(&st.root).unwrap_or((0, 0));
            Ok(Json(json!({
                "nodes": [
                    {
                        "id": "local",
                        "name": "local-master",
                        "url": "",
                        "is_local": true,
                        "healthy": true,
                        "rtt_ms": 0,
                        "disk_total_bytes": total,
                        "disk_available_bytes": avail,
                        "last_seen": chrono::Utc::now(),
                        "error": null
                    }
                ],
                "total_disk_bytes": total,
                "available_disk_bytes": avail,
                "replicate_prefixes": []
            })))
        }
    }
}

#[derive(Deserialize)]
pub struct FileStatusQuery {
    pub path: String,
    pub verify_hash: Option<bool>,
}

/// GET /api/cluster/file-status?path=... — return per-file replica topology across main & followers.
pub async fn get_file_status(
    State(st): State<AppState>,
    Query(q): Query<FileStatusQuery>,
) -> Result<Json<Value>, ApiError> {
    let verify = q.verify_hash.unwrap_or(false);
    match &st.cluster {
        Some(cluster) => {
            let status = cluster.check_file_status(&q.path, verify).await;
            Ok(Json(json!(status)))
        }
        None => {
            let (abs, rel) = crate::paths::resolve_nonroot(&st.root, &q.path)?;
            let meta = tokio::fs::metadata(&abs).await.map_err(|_| crate::error::ApiError::not_found(&rel))?;
            let sha = if verify {
                st.checksum_cache.get_or_compute(&abs, &meta).await.ok()
            } else {
                None
            };
            Ok(Json(json!({
                "path": rel,
                "fully_synced": true,
                "replica_count": 1,
                "expected_replicas": 1,
                "main": {
                    "id": "main",
                    "name": "local-main",
                    "exists": true,
                    "size": meta.len(),
                    "sha256": sha,
                    "mtime": meta.modified().ok().map(chrono::DateTime::<chrono::Utc>::from)
                },
                "followers": [],
                "metrics": {
                    "total_ms": 0,
                    "main_lookup_ms": 0,
                    "follower_timings": {},
                    "verify_hash": verify
                }
            })))
        }
    }
}

#[derive(Deserialize)]
pub struct SyncFileQuery {
    pub path: String,
}

/// POST /api/cluster/sync-file?path=... — sync a single file to all followers that are missing or mismatched.
pub async fn sync_single_file(
    State(st): State<AppState>,
    Query(q): Query<SyncFileQuery>,
) -> Result<Json<Value>, ApiError> {
    match &st.cluster {
        Some(cluster) => {
            let (abs, rel) = crate::paths::resolve_nonroot(&st.root, &q.path)?;
            if !tokio::fs::metadata(&abs).await.map(|m| m.is_file()).unwrap_or(false) {
                return Err(ApiError::not_found(&rel));
            }
            cluster.replicate_put(&rel, &abs).await;
            let status = cluster.check_file_status(&rel, false).await;
            Ok(Json(json!(status)))
        }
        None => Err(ApiError::bad_request(
            "CLUSTER_DISABLED",
            "cluster mode is not enabled",
        )),
    }
}

/// GET /api/cluster/topology — return complete distributed topology and file distributions.
pub async fn get_topology(State(st): State<AppState>) -> Result<Json<Value>, ApiError> {
    match &st.cluster {
        Some(cluster) => {
            let topo = cluster.get_topology().await;
            Ok(Json(json!(topo)))
        }
        None => {
            let (total, avail) = crate::metrics::disk_space(&st.root).unwrap_or((0, 0));
            Ok(Json(json!({
                "control_plane": {
                    "id": "local",
                    "name": "local-master",
                    "url": "",
                    "is_local": true,
                    "healthy": true,
                    "rtt_ms": 0,
                    "disk_total_bytes": total,
                    "disk_available_bytes": avail,
                    "last_seen": chrono::Utc::now(),
                    "error": null
                },
                "workers": [],
                "total_disk_bytes": total,
                "available_disk_bytes": avail,
                "distributions": [],
                "replicate_prefixes": []
            })))
        }
    }
}

/// POST /api/cluster/sync?prefix=... — trigger cluster self-healing and replication.
pub async fn trigger_sync(
    State(st): State<AppState>,
    Query(q): Query<SyncQuery>,
) -> Result<Json<Value>, ApiError> {
    match &st.cluster {
        Some(cluster) => {
            let report = cluster.sync_prefix(&q.prefix).await;
            Ok(Json(json!(report)))
        }
        None => Err(ApiError::bad_request(
            "CLUSTER_DISABLED",
            "cluster mode is not enabled",
        )),
    }
}
