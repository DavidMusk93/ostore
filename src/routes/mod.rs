//! Router assembly.

pub mod events;
pub mod files;
pub mod help;
pub mod list;
pub mod thumb;
pub mod webhooks;

use axum::extract::DefaultBodyLimit;
use axum::middleware;
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use serde_json::json;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use crate::AppState;
use crate::auth;
use crate::metrics;

/// Build the full router. `/healthz` is unauthenticated; everything under
/// `/api` requires the bearer token. Returns `Router<()>` so tests can drive
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
        .route_layer(middleware::from_fn_with_state(state.clone(), auth::require_auth));

    Router::new()
        .route("/healthz", get(|| async { Json(json!({ "status": "ok" })) }))
        .nest("/api", api)
        .layer(middleware::from_fn_with_state(state.clone(), metrics::track))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
