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
