//! Object mutation/transfer routes: PUT/GET/DELETE /api/files/{*path},
//! multipart upload, mkdir, move.

use std::io::ErrorKind;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use axum::Json;
use axum::body::{Body, Bytes};
use axum::extract::{Multipart, Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use futures::StreamExt;
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt, BufWriter};
use tokio_util::io::ReaderStream;

use crate::AppState;
use crate::error::ApiError;
use crate::events;
use crate::metrics::GaugeGuard;
use crate::paths;
use crate::range;

fn truthy(v: &Option<String>) -> bool {
    matches!(v.as_deref(), Some("1") | Some("true") | Some("yes"))
}

fn hv(s: String) -> HeaderValue {
    HeaderValue::from_str(&s).unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream"))
}

fn io_err(e: std::io::Error, not_found: &str) -> ApiError {
    match e.kind() {
        ErrorKind::NotFound => ApiError::not_found(not_found),
        _ => ApiError::internal(e),
    }
}

static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

/// Write a byte stream to `abs` atomically via a unique sibling temp file
/// (`<path>.ostore-tmp-<pid>-<seq>`, so concurrent uploads to one path never
/// share a temp file), fsync'd before the rename so a committed object
/// survives a crash. Returns `(size, sha256_hex)`.
pub(crate) async fn stream_to_disk<S, E>(st: &AppState, abs: &std::path::Path, mut stream: S) -> Result<(u64, String), ApiError>
where
    S: futures::Stream<Item = Result<Bytes, E>> + Unpin,
    E: std::fmt::Display,
{
    let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let tmp = PathBuf::from(format!("{}{}{}-{seq}", abs.display(), crate::TMP_MARKER, std::process::id()));
    let _inflight = GaugeGuard::new(&st.metrics.uploads_in_flight);
    let written: Result<(u64, String), ApiError> = async {
        let file = tokio::fs::File::create(&tmp).await.map_err(ApiError::internal)?;
        let mut file = BufWriter::with_capacity(1 << 20, file);
        let mut hasher = Sha256::new();
        let mut size: u64 = 0;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| ApiError::bad_request("BODY_ERROR", e.to_string()))?;
            file.write_all(&chunk).await.map_err(ApiError::internal)?;
            hasher.update(&chunk);
            size += chunk.len() as u64;
            st.metrics.upload_bytes.fetch_add(chunk.len() as u64, Ordering::Relaxed);
        }
        file.flush().await.map_err(ApiError::internal)?;
        file.get_ref().sync_all().await.map_err(ApiError::internal)?;
        Ok((size, hex::encode(hasher.finalize())))
    }
    .await;
    match written {
        Ok(ok) => {
            tokio::fs::rename(&tmp, abs).await.map_err(ApiError::internal)?;
            st.metrics.objects_written.fetch_add(1, Ordering::Relaxed);
            Ok(ok)
        }
        Err(e) => {
            st.metrics.upload_failures.fetch_add(1, Ordering::Relaxed);
            let _ = tokio::fs::remove_file(&tmp).await;
            Err(e)
        }
    }
}

/// Weak validator from size + mtime (no content hashing on the hot path).
fn etag_of(meta: &std::fs::Metadata) -> String {
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("W/\"{:x}-{:x}\"", meta.len(), mtime)
}

/// PUT /api/files/{*path} — raw-body streaming upload to an exact path.
pub async fn put_file(
    State(st): State<AppState>,
    Path(path): Path<String>,
    body: Body,
) -> Result<Json<Value>, ApiError> {
    let (abs, rel) = paths::resolve_nonroot(&st.root, &path)?;
    if tokio::fs::metadata(&abs).await.map(|m| m.is_dir()).unwrap_or(false) {
        return Err(ApiError::conflict("IS_A_DIR", format!("{rel} is a directory")));
    }
    if let Some(parent) = abs.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(ApiError::internal)?;
    }
    let existed = tokio::fs::metadata(&abs).await.is_ok();
    let stream = body.into_data_stream();
    let (size, sha256) = stream_to_disk(&st, &abs, stream).await?;
    let kind = if existed { events::UPDATED } else { events::CREATED };
    st.publish(kind, &rel, false, Some(size), None);
    tracing::info!(kind, path = %rel, size, "object written");
    Ok(Json(json!({ "path": rel, "size": size, "sha256": sha256 })))
}

#[derive(Deserialize, Default)]
pub struct UploadQuery {
    #[serde(default)]
    path: String,
}

/// POST /api/upload?path=<dir> — multipart/form-data upload; each file
/// field's filename (directory components stripped) becomes the object name.
pub async fn upload_multipart(
    State(st): State<AppState>,
    Query(q): Query<UploadQuery>,
    mut mp: Multipart,
) -> Result<Json<Value>, ApiError> {
    let (dir_abs, dir_rel) = paths::resolve(&st.root, &q.path)?;
    if tokio::fs::metadata(&dir_abs).await.map(|m| !m.is_dir()).unwrap_or(false) {
        return Err(ApiError::bad_request("NOT_A_DIR", format!("{dir_rel} is not a directory")));
    }
    tokio::fs::create_dir_all(&dir_abs).await.map_err(ApiError::internal)?;
    let mut uploaded = Vec::new();
    while let Some(field) = mp
        .next_field()
        .await
        .map_err(|e| ApiError::bad_request("BAD_MULTIPART", e.body_text()))?
    {
        let Some(name) = field.file_name().map(|s| s.to_string()) else {
            continue;
        };
        // Sanitize: keep only the final path component.
        let fname = name.rsplit(['/', '\\']).next().unwrap_or("").to_string();
        if fname.is_empty() {
            continue;
        }
        let rel = if dir_rel.is_empty() { fname.clone() } else { format!("{dir_rel}/{fname}") };
        let (abs, rel) = paths::resolve_nonroot(&st.root, &rel)?;
        if let Some(parent) = abs.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(ApiError::internal)?;
        }
        let existed = tokio::fs::metadata(&abs).await.is_ok();
        let stream = field.map(|r| r.map_err(|e| e.to_string()));
        let (size, _sha256) = stream_to_disk(&st, &abs, stream).await?;
        let kind = if existed { events::UPDATED } else { events::CREATED };
        st.publish(kind, &rel, false, Some(size), None);
        tracing::info!(kind, path = %rel, size, "multipart object written");
        uploaded.push(json!({ "path": rel, "size": size }));
    }
    Ok(Json(json!({ "uploaded": uploaded })))
}

/// GET /api/files/{*path} — download, with single-range support.
pub async fn get_file(
    State(st): State<AppState>,
    Path(path): Path<String>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let (abs, rel) = paths::resolve_nonroot(&st.root, &path)?;
    let meta = tokio::fs::metadata(&abs).await.map_err(|e| io_err(e, &rel))?;
    if meta.is_dir() {
        return Err(ApiError::bad_request("IS_A_DIR", format!("{rel} is a directory")));
    }
    let size = meta.len();
    let etag = etag_of(&meta);
    let not_modified = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.split(',').any(|t| t.trim() == etag || t.trim() == "*"));
    if not_modified {
        return Ok((StatusCode::NOT_MODIFIED, [(header::ETAG, hv(etag))]).into_response());
    }
    let mime = mime_guess::from_path(&abs).first_or_octet_stream();
    let range_hdr = headers.get(header::RANGE).and_then(|v| v.to_str().ok());

    let parsed = match range_hdr {
        None => None,
        Some(h) => match range::parse_range(h, size) {
            Ok(r) => r,
            Err(()) => {
                return Ok((
                    StatusCode::RANGE_NOT_SATISFIABLE,
                    [(header::CONTENT_RANGE, hv(format!("bytes */{size}")))],
                    Json(json!({
                        "error": format!("range not satisfiable for object of {size} bytes"),
                        "code": "RANGE_NOT_SATISFIABLE",
                    })),
                )
                    .into_response());
            }
        },
    };

    let (status, start, len) = match parsed {
        Some((s, e)) => (StatusCode::PARTIAL_CONTENT, s, e - s + 1),
        None => (StatusCode::OK, 0, size),
    };
    let mut file = tokio::fs::File::open(&abs).await.map_err(ApiError::internal)?;
    if start > 0 {
        file.seek(std::io::SeekFrom::Start(start)).await.map_err(ApiError::internal)?;
    }
    let metrics = st.metrics.clone();
    let stream = ReaderStream::with_capacity(file.take(len), 256 * 1024).inspect(move |r| {
        if let Ok(b) = r {
            metrics.download_bytes.fetch_add(b.len() as u64, Ordering::Relaxed);
        }
    });
    let mut builder = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, hv(mime.to_string()))
        .header(header::CONTENT_LENGTH, len.to_string())
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::ETAG, hv(etag))
        .header(header::CACHE_CONTROL, "private, no-cache");
    if status == StatusCode::PARTIAL_CONTENT {
        builder = builder.header(header::CONTENT_RANGE, hv(format!("bytes {}-{}/{}", start, start + len - 1, size)));
    }
    builder
        .body(Body::from_stream(stream))
        .map_err(ApiError::internal)
}

#[derive(Deserialize, Default)]
pub struct DeleteQuery {
    recursive: Option<String>,
}

/// DELETE /api/files/{*path}?recursive=<bool> — delete a file, or a dir only
/// if empty unless recursive=1.
pub async fn delete_file(
    State(st): State<AppState>,
    Path(path): Path<String>,
    Query(q): Query<DeleteQuery>,
) -> Result<Json<Value>, ApiError> {
    let (abs, rel) = paths::resolve_nonroot(&st.root, &path)?;
    let meta = tokio::fs::metadata(&abs).await.map_err(|e| io_err(e, &rel))?;
    if meta.is_dir() {
        if truthy(&q.recursive) {
            tokio::fs::remove_dir_all(&abs).await.map_err(|e| io_err(e, &rel))?;
        } else if let Err(e) = tokio::fs::remove_dir(&abs).await {
            return Err(match e.kind() {
                ErrorKind::NotFound => ApiError::not_found(&rel),
                ErrorKind::DirectoryNotEmpty => {
                    ApiError::conflict("DIR_NOT_EMPTY", format!("{rel} is not empty; pass recursive=1"))
                }
                _ => ApiError::internal(e),
            });
        }
    } else {
        tokio::fs::remove_file(&abs).await.map_err(|e| io_err(e, &rel))?;
    }
    st.publish(events::DELETED, &rel, meta.is_dir(), None, None);
    tracing::info!(path = %rel, "object deleted");
    Ok(Json(json!({ "deleted": rel })))
}

#[derive(Deserialize)]
pub struct MkdirReq {
    path: String,
}

/// POST /api/mkdir {"path"} — mkdir -p semantics (idempotent).
pub async fn mkdir(
    State(st): State<AppState>,
    Json(req): Json<MkdirReq>,
) -> Result<Json<Value>, ApiError> {
    let (abs, rel) = paths::resolve(&st.root, &req.path)?;
    let existed = tokio::fs::metadata(&abs).await.map(|m| m.is_dir()).unwrap_or(false);
    tokio::fs::create_dir_all(&abs).await.map_err(ApiError::internal)?;
    if !existed && !rel.is_empty() {
        st.publish(events::MKDIR, &rel, true, None, None);
        tracing::info!(path = %rel, "directory created");
    }
    Ok(Json(json!({ "path": rel })))
}

#[derive(Deserialize)]
pub struct MoveReq {
    from: String,
    to: String,
}

/// POST /api/move {"from","to"} — rename/move; destination parents are
/// auto-created; overwriting a file is fine, overwriting a non-empty
/// directory is refused.
pub async fn mv(
    State(st): State<AppState>,
    Json(req): Json<MoveReq>,
) -> Result<Json<Value>, ApiError> {
    let (from_abs, from_rel) = paths::resolve_nonroot(&st.root, &req.from)?;
    let (to_abs, to_rel) = paths::resolve_nonroot(&st.root, &req.to)?;
    if from_rel == to_rel {
        return Err(ApiError::bad_path("from and to are the same"));
    }
    if to_rel.starts_with(&format!("{from_rel}/")) {
        return Err(ApiError::bad_path("cannot move a directory into itself"));
    }
    let from_meta = tokio::fs::metadata(&from_abs).await.map_err(|e| io_err(e, &from_rel))?;
    if let Ok(to_meta) = tokio::fs::metadata(&to_abs).await {
        if to_meta.is_dir() {
            if from_meta.is_file() {
                return Err(ApiError::conflict("CANNOT_OVERWRITE", format!("{to_rel} is an existing directory")));
            }
            let mut rd = tokio::fs::read_dir(&to_abs).await.map_err(ApiError::internal)?;
            if rd.next_entry().await.map_err(ApiError::internal)?.is_some() {
                return Err(ApiError::conflict("DIR_NOT_EMPTY", format!("{to_rel} is a non-empty directory")));
            }
        }
    }
    if let Some(parent) = to_abs.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(ApiError::internal)?;
    }
    tokio::fs::rename(&from_abs, &to_abs).await.map_err(ApiError::internal)?;
    let size = if from_meta.is_file() { Some(from_meta.len()) } else { None };
    st.publish(events::MOVED, &to_rel, from_meta.is_dir(), size, Some(from_rel.clone()));
    tracing::info!(from = %from_rel, to = %to_rel, "object moved");
    Ok(Json(json!({ "from": from_rel, "to": to_rel })))
}

/// Streaming sha256 of a file on disk (used by list/stat checksums).
pub async fn sha256_file(abs: &std::path::Path) -> Result<String, ApiError> {
    let mut file = tokio::fs::File::open(abs).await.map_err(ApiError::internal)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf).await.map_err(ApiError::internal)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}
