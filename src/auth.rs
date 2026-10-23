//! Bearer-token auth middleware.
//!
//! Accepts either `Authorization: Bearer <token>` or a `?token=<token>` query
//! parameter (needed because browser `<video src>` and `EventSource` cannot
//! set headers). Applied to everything under /api.

use axum::extract::{Request, State};
use axum::http::header;
use axum::middleware::Next;
use axum::response::Response;

use crate::AppState;
use crate::error::ApiError;

pub async fn require_auth(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let bearer_ok = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .is_some_and(|t| t == state.token.as_ref());
    let query_ok = req
        .uri()
        .query()
        .and_then(query_token)
        .is_some_and(|t| t == state.token.as_ref());
    if bearer_ok || query_ok {
        Ok(next.run(req).await)
    } else {
        Err(ApiError::unauthorized())
    }
}

/// Extract and percent-decode the `token` query parameter, if present.
fn query_token(query: &str) -> Option<String> {
    for pair in query.split('&') {
        if let Some(v) = pair.strip_prefix("token=") {
            return urlencoding::decode(v).ok().map(|c| c.into_owned());
        }
    }
    None
}
