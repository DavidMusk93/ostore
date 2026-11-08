//! Router assembly.

pub mod archive;
pub mod auth_routes;
pub mod batch;
pub mod cluster;
pub mod events;
pub mod files;
pub mod help;
pub mod list;
pub mod thumb;
pub mod webhooks;

use axum::extract::DefaultBodyLimit;
use axum::middleware;
use axum::routing::{any, delete, get, post, put};
use axum::{Json, Router};
use serde_json::json;
use tower_http::compression::CompressionLayer;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use crate::AppState;
use crate::auth;
use crate::metrics;
use crate::s3;

/// Fallback for unknown paths under `/api`.
///
/// Without this, an unmatched `/api/...` path falls out of the nested router
/// and is captured by the S3 surface's root wildcard, which answers in XML.
async fn api_not_found() -> crate::error::ApiError {
    crate::error::ApiError::not_found("endpoint")
}

/// Build the full router. `/healthz` is unauthenticated; everything under
/// `/api` requires the bearer token. The S3 read surface is mounted at the
/// root and guards itself with SigV4. Returns `Router<()>` so tests can drive
/// it with `tower::ServiceExt::oneshot` without binding a port.
pub fn build_router(state: AppState) -> Router {
    let protected_api = Router::new()
        .route("/help", get(help::help))
        .route("/list", get(list::list))
        .route("/stat", get(list::stat))
        .route(
            "/files/{*path}",
            put(files::put_file).get(files::get_file).delete(files::delete_file),
        )
        // Multipart is the browser upload path for multi-GB videos; axum's
        // default 2 MB extractor limit would reject them.
        .route("/thumb/{*path}", get(thumb::thumb))
        .route("/upload", post(files::upload_multipart).layer(DefaultBodyLimit::disable()))
        // Batch of small objects as base64 JSON: its own limit (16 MiB decoded
        // => ~21.3 MiB base64 + framing), well above the 2 MB default.
        .route("/batch", post(batch::batch_put).layer(DefaultBodyLimit::max(batch::MAX_BODY_BYTES)))
        .route("/mkdir", post(files::mkdir))
        .route("/move", post(files::mv))
        .route("/events", get(events::sse))
        .route("/webhooks", post(webhooks::create).get(webhooks::list))
        .route("/webhooks/{id}", delete(webhooks::delete))
        .route("/metrics", get(metrics::handler))
        .route("/archive", get(archive::download_archive))
        .route("/cluster/nodes", get(cluster::get_nodes))
        .route("/cluster/topology", get(cluster::get_topology))
        .route("/cluster/file-status", get(cluster::get_file_status))
        .route("/cluster/dir-status", get(cluster::get_dir_status))
        .route("/cluster/sync-status", get(cluster::get_sync_status))
        .route("/cluster/sync-file", post(cluster::sync_single_file))
        .route("/cluster/sync", post(cluster::trigger_sync))
        .route("/cluster/sync-diff", post(cluster::get_sync_diff))
        .route("/cluster/sync/cancel", post(cluster::cancel_sync))
        .route("/cluster/gdrive/status", get(cluster::gdrive_status))
        .route("/cluster/gdrive/auth-url", post(cluster::gdrive_auth_url))
        .route("/cluster/gdrive/connect", post(cluster::gdrive_connect))
        .route("/cluster/gdrive/disconnect", post(cluster::gdrive_disconnect))
        // Catch-all *inside* `/api` so unmatched API paths stay JSON and stay
        // behind the token; a nested router's own `fallback` is not consulted,
        // and the request would otherwise fall out to the S3 surface.
        .route("/{*rest}", any(api_not_found))
        .route_layer(middleware::from_fn_with_state(state.clone(), auth::require_auth));

    let api = Router::new()
        .route("/auth/login", post(auth_routes::login))
        .route("/auth/logout", post(auth_routes::logout))
        .route("/auth/setup", get(auth_routes::setup_info))
        .route("/auth/verify", get(auth_routes::verify))
        .route("/cluster/gdrive/callback", get(cluster::gdrive_callback))
        .merge(protected_api);

    Router::new()
        .route("/", get(crate::ui::handler))
        .route("/ostore", get(crate::ui::handler))
        .route("/ostore/", get(crate::ui::handler))
        .route("/healthz", get(|| async { Json(json!({ "status": "ok" })) }))
        .nest("/api", api.clone())
        .nest("/ostore/api", api)
        // S3 read surface (`/{bucket}/{key}`): static routes above win.
        .merge(s3::router())
        .layer(CompressionLayer::new())
        .layer(middleware::from_fn_with_state(state.clone(), metrics::track))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
