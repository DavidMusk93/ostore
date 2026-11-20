//! Deduplication management routes:
//! - `GET  /api/dedup/stats` — inspect CAS objects, shared references, and bytes reclaimed.
//! - `POST /api/dedup/scan`  — retroactively deduplicate pre-existing files under `OSTORE_ROOT`.
//! - `POST /api/dedup/gc`    — remove unreferenced CAS entries (`nlink <= 1`).

use axum::Json;
use axum::extract::State;
use serde_json::{Value, json};

use crate::AppState;
use crate::error::ApiError;

pub async fn stats(State(st): State<AppState>) -> Json<Value> {
    let s = st.dedup.stats().await;
    Json(json!(s))
}

pub async fn scan(State(st): State<AppState>) -> Result<Json<Value>, ApiError> {
    let report = st
        .dedup
        .scan_and_dedup(&st.checksum_cache)
        .await
        .map_err(|msg| ApiError::conflict("SCAN_IN_PROGRESS", msg))?;
    tracing::info!(
        scanned = report.scanned_files,
        anchored = report.anchored_new,
        deduplicated = report.deduplicated,
        bytes_saved = report.bytes_saved,
        errors = report.errors,
        "retroactive dedup scan completed"
    );
    Ok(Json(json!(report)))
}

pub async fn gc(State(st): State<AppState>) -> Json<Value> {
    let report = st.dedup.gc().await;
    tracing::info!(
        removed = report.removed,
        bytes_freed = report.bytes_freed,
        "dedup CAS garbage collection completed"
    );
    Json(json!(report))
}
