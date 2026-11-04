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

/// GET /api/cluster/sync-status — return cluster sync in-progress state, last report and node health.
pub async fn get_sync_status(State(st): State<AppState>) -> Result<Json<Value>, ApiError> {
    match &st.cluster {
        Some(cluster) => {
            let status = cluster.get_sync_status().await;
            Ok(Json(json!(status)))
        }
        None => Ok(Json(json!({
            "is_syncing": false,
            "last_sync": null,
            "nodes": []
        }))),
    }
}

/// GET /api/cluster/dir-status?prefix=... — fast replica probe for all files in a single directory.
pub async fn get_dir_status(
    State(st): State<AppState>,
    Query(q): Query<SyncQuery>,
) -> Result<Json<Value>, ApiError> {
    match &st.cluster {
        Some(cluster) => {
            let status = cluster.check_dir_status(&q.prefix).await;
            Ok(Json(json!(status)))
        }
        None => {
            let (_, rel) = crate::paths::resolve(&st.root, &q.prefix)?;
            Ok(Json(json!({
                "prefix": rel,
                "files": {}
            })))
        }
    }
}

#[derive(Deserialize, Default)]
pub struct DiffQuery {
    #[serde(default)]
    pub prefix: String,
    #[serde(default)]
    pub nodes: Option<String>,
}

#[derive(Deserialize, Default)]
pub struct DiffRequest {
    #[serde(default)]
    pub prefix: Option<String>,
    #[serde(default)]
    pub paths: Option<Vec<String>>,
    #[serde(default)]
    pub target_nodes: Option<Vec<String>>,
}

/// POST /api/cluster/sync-diff — preview differences without modifying state
pub async fn get_sync_diff(
    State(st): State<AppState>,
    Query(q): Query<DiffQuery>,
    body: Option<Json<DiffRequest>>,
) -> Result<Json<Value>, ApiError> {
    let cluster = st.cluster.as_ref().ok_or_else(|| {
        ApiError::bad_request("CLUSTER_DISABLED", "cluster mode is not enabled")
    })?;

    let (prefix, paths, target_nodes) = if let Some(Json(b)) = body {
        (
            b.prefix.unwrap_or(q.prefix),
            b.paths,
            b.target_nodes.or_else(|| {
                q.nodes.map(|s| s.split(',').map(|p| p.trim().to_string()).filter(|s| !s.is_empty()).collect())
            }),
        )
    } else {
        (
            q.prefix,
            None,
            q.nodes.map(|s| s.split(',').map(|p| p.trim().to_string()).filter(|s| !s.is_empty()).collect()),
        )
    };

    match cluster.get_sync_diff(&prefix, paths.as_deref(), target_nodes.as_deref()).await {
        Ok(diff) => Ok(Json(json!(diff))),
        Err(e) => Err(ApiError::bad_request("DIFF_FAILED", e)),
    }
}

/// POST /api/cluster/sync/cancel — cancel running sync
pub async fn cancel_sync(State(st): State<AppState>) -> Result<Json<Value>, ApiError> {
    let cluster = st.cluster.as_ref().ok_or_else(|| {
        ApiError::bad_request("CLUSTER_DISABLED", "cluster mode is not enabled")
    })?;
    let cancelled = cluster.cancel_sync().await;
    Ok(Json(json!({ "cancelled": cancelled })))
}

/// POST /api/cluster/sync — trigger cluster replication with options & metrics governor
pub async fn trigger_sync(
    State(st): State<AppState>,
    Query(q): Query<SyncQuery>,
    body: Option<Json<crate::cluster::CustomSyncOptions>>,
) -> Result<Json<Value>, ApiError> {
    let cluster = st.cluster.as_ref().ok_or_else(|| {
        ApiError::bad_request("CLUSTER_DISABLED", "cluster mode is not enabled")
    })?;

    let opts = if let Some(Json(mut b)) = body {
        if b.prefix.is_none() && !q.prefix.is_empty() {
            b.prefix = Some(q.prefix.clone());
        }
        b
    } else {
        crate::cluster::CustomSyncOptions {
            prefix: Some(q.prefix),
            ..Default::default()
        }
    };

    let report = cluster.sync_custom(opts, Some(&st.metrics)).await;
    Ok(Json(json!(report)))
}

// ==========================================
// Google Drive Cloud Node Management Routes
// ==========================================

static PENDING_OAUTH: std::sync::LazyLock<
    tokio::sync::RwLock<std::collections::HashMap<String, (String, String, String, std::time::Instant)>>,
> = std::sync::LazyLock::new(|| tokio::sync::RwLock::new(std::collections::HashMap::new()));

fn mask_str(s: &str) -> String {
    if s.len() > 10 {
        format!("{}...{}", &s[..6], &s[s.len() - 4..])
    } else {
        "***".to_string()
    }
}

fn html_message(title: &str, msg: &str, success: bool) -> axum::response::Response {
    use axum::response::IntoResponse;
    let icon = if success { "✅" } else { "❌" };
    let color = if success { "#34C759" } else { "#FF3B30" };
    let html = format!(
        r#"<!DOCTYPE html>
<html>
<head>
  <meta charset="utf-8">
  <title>{title} — ostore</title>
  <style>
    body {{ font-family: -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif; background: #F5F5F7; display: flex; align-items: center; justify-content: center; height: 100vh; margin: 0; }}
    .card {{ background: #fff; padding: 36px 40px; border-radius: 16px; box-shadow: 0 4px 20px rgba(0,0,0,0.08); text-align: center; max-width: 440px; border: 1px solid rgba(0,0,0,0.06); }}
    h2 {{ font-size: 1.25rem; color: #1D1D1F; margin: 12px 0 8px; }}
    p {{ font-size: 0.86rem; color: #86868B; line-height: 1.5; margin-bottom: 24px; }}
    a {{ display: inline-block; background: {color}; color: #fff; text-decoration: none; padding: 10px 24px; border-radius: 980px; font-weight: 500; font-size: 0.86rem; }}
  </style>
  {redirect_meta}
</head>
<body>
  <div class="card">
    <div style="font-size: 40px;">{icon}</div>
    <h2>{title}</h2>
    <p>{msg}</p>
    <a href="/ostore">Return to ostore console</a>
  </div>
</body>
</html>"#,
        redirect_meta = if success {
            r#"<meta http-equiv="refresh" content="2;url=/ostore">"#
        } else {
            ""
        }
    );
    (
        [(axum::http::header::CONTENT_TYPE, "text/html; charset=utf-8")],
        html,
    )
        .into_response()
}

/// GET /api/cluster/gdrive/status — return Google Drive connection status and storage quota.
pub async fn gdrive_status(State(st): State<AppState>) -> Result<Json<Value>, ApiError> {
    if let Some(cluster) = &st.cluster {
        if let Some(gdrive) = cluster.gdrive.read().await.clone() {
            match gdrive.get_storage_quota().await {
                Ok((total, avail)) => {
                    return Ok(Json(json!({
                        "connected": true,
                        "client_id": mask_str(&gdrive.config.client_id),
                        "root_folder_id": gdrive.config.root_folder_id,
                        "disk_total_bytes": total,
                        "disk_available_bytes": avail,
                        "error": null
                    })));
                }
                Err(e) => {
                    return Ok(Json(json!({
                        "connected": false,
                        "client_id": mask_str(&gdrive.config.client_id),
                        "root_folder_id": gdrive.config.root_folder_id,
                        "disk_total_bytes": 0,
                        "disk_available_bytes": 0,
                        "error": e
                    })));
                }
            }
        }
    }
    Ok(Json(json!({
        "connected": false,
        "client_id": null,
        "root_folder_id": "root",
        "disk_total_bytes": 0,
        "disk_available_bytes": 0,
        "error": null
    })))
}

#[derive(Deserialize)]
pub struct AuthUrlReq {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: Option<String>,
}

/// POST /api/cluster/gdrive/auth-url — generate Google OAuth authorization URL.
pub async fn gdrive_auth_url(
    State(_st): State<AppState>,
    Json(req): Json<AuthUrlReq>,
) -> Result<Json<Value>, ApiError> {
    let now = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);
    let state = format!("{:x}{:x}", now, std::process::id());
    let redirect = req
        .redirect_uri
        .unwrap_or_else(|| "https://xyz69.top/ostore/api/cluster/gdrive/callback".to_string());

    tracing::info!(
        client_id = %mask_str(&req.client_id),
        redirect = %redirect,
        state = %state,
        "generating gdrive oauth url"
    );

    {
        let mut lock = PENDING_OAUTH.write().await;
        lock.retain(|_, (_, _, _, t)| t.elapsed() < std::time::Duration::from_secs(900));
        lock.insert(
            state.clone(),
            (
                req.client_id.clone(),
                req.client_secret.clone(),
                redirect.clone(),
                std::time::Instant::now(),
            ),
        );
    }

    let url = format!(
        "https://accounts.google.com/o/oauth2/v2/auth?client_id={}&redirect_uri={}&response_type=code&scope=https://www.googleapis.com/auth/drive&access_type=offline&prompt=consent&state={}",
        urlencoding::encode(&req.client_id),
        urlencoding::encode(&redirect),
        urlencoding::encode(&state)
    );

    Ok(Json(json!({ "auth_url": url, "state": state })))
}

#[derive(Deserialize)]
pub struct CallbackQuery {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
}

/// GET /api/cluster/gdrive/callback — Google OAuth redirect handler.
pub async fn gdrive_callback(
    State(st): State<AppState>,
    Query(q): Query<CallbackQuery>,
) -> axum::response::Response {
    tracing::info!(has_code = q.code.is_some(), has_state = q.state.is_some(), error = ?q.error, "received gdrive oauth callback");

    if let Some(err) = q.error {
        tracing::warn!(error = %err, "google oauth callback returned error");
        return html_message("Google Authorization Failed", &format!("Google returned error: {err}"), false);
    }
    let Some(code) = q.code else {
        tracing::warn!("google oauth callback missing code");
        return html_message("Authorization Error", "Missing authorization code", false);
    };
    let Some(state_key) = q.state else {
        tracing::warn!("google oauth callback missing state");
        return html_message("Authorization Error", "Missing state parameter", false);
    };

    let pending = {
        let mut lock = PENDING_OAUTH.write().await;
        lock.remove(&state_key)
    };
    let Some((client_id, client_secret, redirect_uri, _)) = pending else {
        tracing::warn!(state = %state_key, "gdrive oauth session expired or unknown state");
        return html_message("Authorization Timeout", "Authorization session has expired (or the service restarted). Please return to the console and try again.", false);
    };

    let params = [
        ("code", code.as_str()),
        ("client_id", client_id.as_str()),
        ("client_secret", client_secret.as_str()),
        ("redirect_uri", redirect_uri.as_str()),
        ("grant_type", "authorization_code"),
    ];

    let client = reqwest::Client::new();
    let resp = match client
        .post("https://oauth2.googleapis.com/token")
        .form(&params)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error = %e, "gdrive token exchange network error");
            return html_message("Token Exchange Failed", &format!("Network request failed: {e}"), false);
        }
    };

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        tracing::error!(status = %status, body = %text, "gdrive token exchange failed");
        return html_message("Token Exchange Failed", &format!("Google returned error status ({status}): {text}"), false);
    }

    let val: serde_json::Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(error = %e, "gdrive token json decode error");
            return html_message("Token Parsing Failed", &format!("Failed to parse response JSON: {e}"), false);
        }
    };

    let Some(refresh_token) = val.get("refresh_token").and_then(|v| v.as_str()) else {
        tracing::warn!("google oauth did not return refresh_token");
        return html_message(
            "Missing Refresh Token",
            "Google did not return a refresh_token (access may have already been granted previously). Please visit Google Account Permissions (https://myaccount.google.com/connections) to revoke access for this app and re-authorize, or enter your Refresh Token manually in the console.",
            false,
        );
    };

    if let Some(cluster) = &st.cluster {
        let cfg = crate::cluster::gdrive::GDriveConfig {
            client_id,
            client_secret,
            refresh_token: refresh_token.to_string(),
            root_folder_id: "root".to_string(),
        };
        if let Err(e) = cluster.update_and_persist_gdrive(cfg).await {
            tracing::error!(error = %e, "update_and_persist_gdrive failed");
            return html_message("Save Failed", &format!("Failed to save Google Drive configuration: {e}"), false);
        }
    }

    tracing::info!("gdrive oauth successful and cluster updated");
    html_message(
        "Google Drive Connected Successfully",
        "Successfully linked your Google account. Cloud storage quota is now active! Redirecting to console...",
        true,
    )
}

#[derive(Deserialize)]
pub struct ConnectReq {
    pub client_id: String,
    pub client_secret: String,
    pub refresh_token: String,
    pub root_folder_id: Option<String>,
}

/// POST /api/cluster/gdrive/connect — manual credential connection.
pub async fn gdrive_connect(
    State(st): State<AppState>,
    Json(req): Json<ConnectReq>,
) -> Result<Json<Value>, ApiError> {
    let Some(cluster) = &st.cluster else {
        return Err(ApiError::bad_request("CLUSTER_DISABLED", "cluster mode not enabled"));
    };

    let cfg = crate::cluster::gdrive::GDriveConfig {
        client_id: req.client_id,
        client_secret: req.client_secret,
        refresh_token: req.refresh_token,
        root_folder_id: req.root_folder_id.unwrap_or_else(|| "root".to_string()),
    };

    tracing::info!(client_id = %mask_str(&cfg.client_id), "validating manual gdrive credentials");
    let client = crate::cluster::gdrive::GDriveClient::new(cfg.clone());
    let (total, avail) = client
        .get_storage_quota()
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "manual gdrive validation failed");
            ApiError::bad_request("GDRIVE_AUTH_FAILED", format!("Google Drive authentication failed: {e}"))
        })?;

    cluster.update_and_persist_gdrive(cfg).await.map_err(|e| {
        tracing::error!(error = %e, "update_and_persist_gdrive failed during manual connect");
        ApiError::internal(std::io::Error::new(std::io::ErrorKind::Other, e))
    })?;

    tracing::info!("manual gdrive node connected and persisted successfully");

    Ok(Json(json!({
        "status": "ok",
        "disk_total_bytes": total,
        "disk_available_bytes": avail
    })))
}

/// POST /api/cluster/gdrive/disconnect — disconnect Google Drive cloud node.
pub async fn gdrive_disconnect(State(st): State<AppState>) -> Result<Json<Value>, ApiError> {
    let Some(cluster) = &st.cluster else {
        return Err(ApiError::bad_request("CLUSTER_DISABLED", "cluster mode not enabled"));
    };

    cluster.remove_and_persist_gdrive().await.map_err(|e| {
        ApiError::internal(std::io::Error::new(std::io::ErrorKind::Other, e))
    })?;

    Ok(Json(json!({ "status": "ok" })))
}
