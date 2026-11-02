//! Authenticator OTP login and session routes.

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, header};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::AppState;
use crate::error::ApiError;

#[derive(Deserialize)]
pub struct LoginReq {
    pub code: String,
}

#[derive(Serialize)]
pub struct LoginResp {
    pub status: &'static str,
    pub token: String,
    pub expires_in_days: u64,
}

/// POST /api/auth/login — verify 6-digit Authenticator OTP and issue 14-day session.
pub async fn login(
    State(st): State<AppState>,
    Json(req): Json<LoginReq>,
) -> Result<Response, ApiError> {
    if st.totp.verify(&req.code) {
        let session_token = st.sessions.create_session().await;
        let cookie_val = format!(
            "ostore_sess={}; Path=/; Max-Age=1209600; SameSite=Lax; HttpOnly",
            session_token
        );

        let body = Json(json!(LoginResp {
            status: "ok",
            token: session_token,
            expires_in_days: 14,
        }));

        let mut resp = body.into_response();
        if let Ok(hdr) = HeaderValue::from_str(&cookie_val) {
            resp.headers_mut().insert(header::SET_COOKIE, hdr);
        }
        Ok(resp)
    } else {
        Err(ApiError::unauthorized())
    }
}

/// POST /api/auth/logout — revoke session.
pub async fn logout(
    State(st): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    if let Some(cookie_hdr) = headers.get(header::COOKIE).and_then(|v| v.to_str().ok()) {
        for cookie in cookie_hdr.split(';') {
            let trimmed = cookie.trim();
            if let Some(sess) = trimmed.strip_prefix("ostore_sess=") {
                st.sessions.revoke_session(sess).await;
            }
        }
    }

    let cookie_clear = "ostore_sess=; Path=/; Max-Age=0; SameSite=Lax; HttpOnly";
    let mut resp = Json(json!({ "status": "logged_out" })).into_response();
    if let Ok(hdr) = HeaderValue::from_str(cookie_clear) {
        resp.headers_mut().insert(header::SET_COOKIE, hdr);
    }
    Ok(resp)
}

/// GET /api/auth/setup — returns Base32 secret and otpauth URI for Authenticator setup.
pub async fn setup_info(State(st): State<AppState>) -> Json<serde_json::Value> {
    let secret = st.totp.get_secret_base32();
    let uri = st.totp.get_otpauth_uri("ostore", "admin");
    Json(json!({
        "issuer": "ostore",
        "account": "admin",
        "secret": secret,
        "otpauth_uri": uri,
        "instructions": "Enter this secret or scan QR code in Apple Passwords, Google Authenticator, or 1Password."
    }))
}
