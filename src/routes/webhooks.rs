//! Webhook registration CRUD: /api/webhooks.

use axum::Json;
use axum::extract::{Path, State};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::AppState;
use crate::error::ApiError;

#[derive(Deserialize)]
pub struct CreateReq {
    url: String,
    #[serde(default)]
    events: Option<Vec<String>>,
}

/// POST /api/webhooks {"url", "events"?} — register a webhook.
pub async fn create(
    State(st): State<AppState>,
    Json(req): Json<CreateReq>,
) -> Result<Json<Value>, ApiError> {
    if !(req.url.starts_with("http://") || req.url.starts_with("https://")) {
        return Err(ApiError::bad_request("BAD_URL", "url must start with http:// or https://"));
    }
    let hook = st.webhooks.add(req.url, req.events);
    tracing::info!(id = %hook.id, url = %hook.url, "webhook registered");
    Ok(Json(serde_json::to_value(&hook).map_err(ApiError::internal)?))
}

/// GET /api/webhooks — list registrations.
pub async fn list(State(st): State<AppState>) -> Json<Value> {
    Json(json!({ "webhooks": st.webhooks.list() }))
}

/// DELETE /api/webhooks/{id} — remove a registration.
pub async fn delete(
    State(st): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    if st.webhooks.remove(&id) {
        tracing::info!(id = %id, "webhook removed");
        Ok(Json(json!({ "deleted": id })))
    } else {
        Err(ApiError::not_found(format!("webhook {id}")))
    }
}
