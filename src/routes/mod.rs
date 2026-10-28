//! Router assembly.

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
    let api = Router::new()
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
        .route("/mkdir", post(files::mkdir))
        .route("/move", post(files::mv))
        .route("/events", get(events::sse))
        .route("/webhooks", post(webhooks::create).get(webhooks::list))
        .route("/webhooks/{id}", delete(webhooks::delete))
        .route("/metrics", get(metrics::handler))
        // Catch-all *inside* `/api` so unmatched API paths stay JSON and stay
        // behind the token; a nested router's own `fallback` is not consulted,
        // and the request would otherwise fall out to the S3 surface.
        .route("/{*rest}", any(api_not_found))
        .route_layer(middleware::from_fn_with_state(state.clone(), auth::require_auth));

    Router::new()
        .route("/healthz", get(|| async { Json(json!({ "status": "ok" })) }))
        .nest("/api", api)
        // S3 read surface (`/{bucket}/{key}`): static routes above win.
        .merge(s3::router())
        .layer(middleware::from_fn_with_state(state.clone(), metrics::track))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
