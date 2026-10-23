//! Read routes: /api/list and /api/stat.

use axum::Json;
use axum::extract::{Query, State};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::AppState;
use crate::error::ApiError;
use crate::paths;
use crate::routes::files::sha256_file;

fn truthy(v: &Option<String>) -> bool {
    matches!(v.as_deref(), Some("1") | Some("true") | Some("yes"))
}

#[derive(Deserialize, Default)]
pub struct ListQuery {
    #[serde(default)]
    path: String,
    recursive: Option<String>,
    checksums: Option<String>,
}

#[derive(Deserialize, Default)]
pub struct StatQuery {
    #[serde(default)]
    path: String,
    checksums: Option<String>,
}

#[derive(Serialize)]
pub struct Entry {
    name: String,
    path: String,
    #[serde(rename = "type")]
    kind: &'static str,
    size: u64,
    mtime: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sha256: Option<String>,
}

/// Names never exposed through listings: the meta dir itself (exactly
/// `.ostore` at any depth) and in-flight atomic-upload temp files.
fn hidden(name: &str) -> bool {
    name == paths::META_DIR || name.contains(".ostore-tmp-")
}

async fn entry(
    rel: String,
    name: String,
    abs: &std::path::Path,
    meta: &std::fs::Metadata,
    checksums: bool,
) -> Result<Entry, ApiError> {
    let is_dir = meta.is_dir();
    let mtime: DateTime<Utc> = meta.modified().map(DateTime::<Utc>::from).unwrap_or_else(|_| Utc::now());
    let sha256 = if checksums && !is_dir {
        Some(sha256_file(abs).await?)
    } else {
        None
    };
    Ok(Entry {
        name,
        path: rel,
        kind: if is_dir { "dir" } else { "file" },
        size: if is_dir { 0 } else { meta.len() },
        mtime,
        sha256,
    })
}

fn not_found_or_internal(e: std::io::Error, rel: &str) -> ApiError {
    match e.kind() {
        std::io::ErrorKind::NotFound => {
            ApiError::not_found(if rel.is_empty() { "/" } else { rel })
        }
        _ => ApiError::internal(e),
    }
}

/// GET /api/list?path=<rel>&recursive=<bool>&checksums=<bool>
pub async fn list(
    State(st): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let (abs, rel) = paths::resolve(&st.root, &q.path)?;
    let meta = tokio::fs::metadata(&abs).await.map_err(|e| not_found_or_internal(e, &rel))?;
    if !meta.is_dir() {
        return Err(ApiError::bad_request("NOT_A_DIR", format!("{rel} is not a directory")));
    }
    let recursive = truthy(&q.recursive);
    let checksums = truthy(&q.checksums);

    let mut entries: Vec<Entry> = Vec::new();
    let mut stack: Vec<(std::path::PathBuf, String)> = vec![(abs, rel.clone())];
    while let Some((dir_abs, dir_rel)) = stack.pop() {
        let mut rd = tokio::fs::read_dir(&dir_abs).await.map_err(ApiError::internal)?;
        let mut dirs: Vec<(String, std::path::PathBuf, String, std::fs::Metadata)> = Vec::new();
        let mut files: Vec<(String, std::path::PathBuf, String, std::fs::Metadata)> = Vec::new();
        while let Some(de) = rd.next_entry().await.map_err(ApiError::internal)? {
            let name = de.file_name().to_string_lossy().into_owned();
            if hidden(&name) {
                continue;
            }
            let child_abs = de.path();
            let child_rel = if dir_rel.is_empty() { name.clone() } else { format!("{dir_rel}/{name}") };
            let m = de.metadata().await.map_err(ApiError::internal)?;
            if m.is_dir() {
                dirs.push((name, child_abs, child_rel, m));
            } else {
                files.push((name, child_abs, child_rel, m));
            }
        }
        // Directories first, then files, each sorted by name.
        dirs.sort_by(|a, b| a.0.cmp(&b.0));
        files.sort_by(|a, b| a.0.cmp(&b.0));
        for (name, child_abs, child_rel, m) in &dirs {
            entries.push(entry(child_rel.clone(), name.clone(), child_abs, m, checksums).await?);
        }
        for (name, child_abs, child_rel, m) in &files {
            entries.push(entry(child_rel.clone(), name.clone(), child_abs, m, checksums).await?);
        }
        if recursive {
            // Push reversed so subdirectories are visited in name order.
            for (_, sub_abs, sub_rel, _) in dirs.into_iter().rev() {
                stack.push((sub_abs, sub_rel));
            }
        }
    }
    Ok(Json(json!({ "path": rel, "entries": entries })))
}

/// GET /api/stat?path=<rel>&checksums=<bool>
pub async fn stat(
    State(st): State<AppState>,
    Query(q): Query<StatQuery>,
) -> Result<Json<Entry>, ApiError> {
    let (abs, rel) = paths::resolve(&st.root, &q.path)?;
    let meta = tokio::fs::metadata(&abs).await.map_err(|e| not_found_or_internal(e, &rel))?;
    let name = rel.rsplit('/').next().unwrap_or("").to_string();
    entry(rel, name, &abs, &meta, truthy(&q.checksums)).await.map(Json)
}
