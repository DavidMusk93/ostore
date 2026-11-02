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
    // 1. Check Bearer token
    if let Some(bearer) = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
    {
        if bearer == state.token.as_ref() || state.sessions.verify_session(bearer).await {
            return Ok(next.run(req).await);
        }
    }

    // 2. Check query parameter
    if let Some(token) = req.uri().query().and_then(query_token) {
        if token == state.token.as_ref() || state.sessions.verify_session(&token).await {
            return Ok(next.run(req).await);
        }
    }

    // 3. Check Cookie (ostore_sess=...)
    if let Some(cookie_hdr) = req.headers().get(header::COOKIE).and_then(|v| v.to_str().ok()) {
        for cookie in cookie_hdr.split(';') {
            let trimmed = cookie.trim();
            if let Some(sess) = trimmed.strip_prefix("ostore_sess=") {
                if state.sessions.verify_session(sess).await {
                    return Ok(next.run(req).await);
                }
            }
        }
    }

    Err(ApiError::unauthorized())
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
