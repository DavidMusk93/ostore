//! S3-shaped API on the server root (path-style, SigV4 header auth).
//!
//! Enough for `aws s3 sync`, rclone, and cachey: ListObjectsV2, Get/Head/Put/
//! DeleteObject, HeadBucket. Not a full S3 clone (no multipart, ACL, versioning,
//! CopyObject, presigned URLs). Writes publish the same object events as `/api`.

pub mod list;
pub mod sigv4;
mod xml;

use std::sync::Arc;
use std::sync::atomic::Ordering;

use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, put};
use axum::Router;
use chrono::Utc;
use futures::StreamExt;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;

use crate::AppState;
use crate::events;
use crate::paths;
use crate::range;
use crate::routes::files::stream_to_disk;

pub use xml::{escape_xml, http_date, strong_etag};

use xml::{content_md5_etag, xml_error, xml_error_with};

use list::{list_objects_v2, ListV2Query};

/// Credentials and naming for the S3 surface.
#[derive(Debug, Clone)]
pub struct S3Auth {
    pub access_key: String,
    pub secret_key: String,
    pub region: Option<String>,
    pub bucket: Option<String>,
}

impl S3Auth {
    pub fn from_env() -> Result<Option<Self>, String> {
        let access = std::env::var("OSTORE_S3_ACCESS_KEY").ok().filter(|s| !s.is_empty());
        let secret = std::env::var("OSTORE_S3_SECRET_KEY").ok().filter(|s| !s.is_empty());
        match (access, secret) {
            (None, None) => Ok(None),
            (Some(access_key), Some(secret_key)) => Ok(Some(Self {
                access_key,
                secret_key,
                region: std::env::var("OSTORE_S3_REGION").ok().filter(|s| !s.is_empty()),
                bucket: std::env::var("OSTORE_S3_BUCKET").ok().filter(|s| !s.is_empty()),
            })),
            _ => Err(
                "OSTORE_S3_ACCESS_KEY and OSTORE_S3_SECRET_KEY must both be set (or both unset)"
                    .to_string(),
            ),
        }
    }
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/{bucket}", get(bucket_get).head(head_bucket))
        .route(
            "/{bucket}/{*key}",
            get(object_get)
                .head(object_get)
                .put(put_object)
                .delete(delete_object),
        )
        .fallback(not_found)
}

fn disabled(_st: &AppState, resource: &str) -> Response {
    xml_error(
        StatusCode::FORBIDDEN,
        "AccessDenied",
        "the S3 surface is disabled; set OSTORE_S3_ACCESS_KEY and OSTORE_S3_SECRET_KEY",
        resource,
    )
}

pub(super) fn authorize(st: &AppState, uri: &Uri, headers: &HeaderMap, method: &str) -> Result<Arc<S3Auth>, Response> {
    let Some(auth) = st.s3.clone() else {
        return Err(disabled(st, uri.path()));
    };
    if let Err(e) = sigv4::verify(&auth, method, uri, headers, Utc::now()) {
        st.metrics.s3_denied.fetch_add(1, Ordering::Relaxed);
        return Err(xml_error(StatusCode::FORBIDDEN, e.code(), &e.message(), uri.path()));
    }
    Ok(auth)
}

pub(super) fn bucket_ok(auth: &S3Auth, bucket: &str, resource: &str) -> Result<(), Response> {
    if let Some(want) = &auth.bucket
        && *want != bucket
    {
        return Err(xml_error(StatusCode::NOT_FOUND, "NoSuchBucket", "the specified bucket does not exist", resource));
    }
    Ok(())
}

async fn not_found(uri: Uri) -> Response {
    xml_error(StatusCode::NOT_FOUND, "NoSuchKey", "the requested resource does not exist", uri.path())
}

async fn bucket_get(
    State(st): State<AppState>,
    Path(bucket): Path<String>,
    Query(q): Query<ListV2Query>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    list_objects_v2(State(st), bucket, Query(q), Method::GET, headers, uri).await
}

async fn head_bucket(State(st): State<AppState>, Path(bucket): Path<String>, headers: HeaderMap, uri: Uri) -> Response {
    let auth = match authorize(&st, &uri, &headers, "HEAD") {
        Ok(a) => a,
        Err(r) => return r,
    };
    if let Err(r) = bucket_ok(&auth, &bucket, uri.path()) {
        return r;
    }
    let region = auth.region.as_deref().unwrap_or("us-east-1");
    Response::builder()
        .status(StatusCode::OK)
        .header("x-amz-bucket-region", region)
        .body(Body::empty())
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

async fn object_get(
    State(st): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
    method: Method,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let auth = match authorize(&st, &uri, &headers, method.as_str()) {
        Ok(a) => a,
        Err(r) => return r,
    };
    if let Err(r) = bucket_ok(&auth, &bucket, uri.path()) {
        return r;
    }
    let head_only = method == Method::HEAD;
    if head_only {
        st.metrics.s3_heads.fetch_add(1, Ordering::Relaxed);
    } else {
        st.metrics.s3_gets.fetch_add(1, Ordering::Relaxed);
    }

    let (abs, rel) = match paths::resolve_nonroot(&st.root, &key) {
        Ok(v) => v,
        Err(e) => return xml_error(StatusCode::BAD_REQUEST, "InvalidRequest", &e.message, uri.path()),
    };
    let meta = match tokio::fs::metadata(&abs).await {
        Ok(m) if !m.is_dir() => m,
        _ => return xml_error(StatusCode::NOT_FOUND, "NoSuchKey", "the specified key does not exist", uri.path()),
    };
    let size = meta.len();
    let etag = strong_etag(&meta);
    let last_modified = http_date(meta.modified().ok());
    let mime = mime_guess::from_path(&abs).first_or_octet_stream().to_string();

    if let Some(inm) = headers.get(header::IF_NONE_MATCH).and_then(|v| v.to_str().ok())
        && inm.split(',').any(|t| t.trim() == etag || t.trim() == "*" || t.trim().trim_start_matches("W/") == etag)
    {
        return Response::builder()
            .status(StatusCode::NOT_MODIFIED)
            .header(header::ETAG, etag)
            .header(header::LAST_MODIFIED, last_modified)
            .body(Body::empty())
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response());
    }

    let parsed = match headers.get(header::RANGE).and_then(|v| v.to_str().ok()) {
        None => Ok(None),
        Some(h) => range::parse_range(h, size),
    };
    let (start, len) = match parsed {
        Ok(Some((s, e))) => (s, e - s + 1),
        Ok(None) => (0, size),
        Err(()) => {
            return xml_error_with(
                StatusCode::RANGE_NOT_SATISFIABLE,
                "InvalidRange",
                "the requested range is not satisfiable",
                uri.path(),
                Some((header::CONTENT_RANGE, format!("bytes */{size}"))),
            );
        }
    };
    let partial = matches!(parsed, Ok(Some(_)));

    let mut builder = Response::builder()
        .status(if partial { StatusCode::PARTIAL_CONTENT } else { StatusCode::OK })
        .header(header::CONTENT_TYPE, hv(&mime))
        .header(header::CONTENT_LENGTH, len)
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::ETAG, etag.clone())
        .header(header::LAST_MODIFIED, last_modified)
        .header("x-amz-request-id", format!("{:x}", request_id(&st, &rel)));
    if partial {
        builder = builder.header(header::CONTENT_RANGE, format!("bytes {start}-{}/{size}", start + len - 1));
    }
    if head_only {
        return builder.body(Body::empty()).unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response());
    }

    let mut file = match tokio::fs::File::open(&abs).await {
        Ok(f) => f,
        Err(e) => return xml_error(StatusCode::INTERNAL_SERVER_ERROR, "InternalError", &e.to_string(), uri.path()),
    };
    if start > 0
        && let Err(e) = file.seek(std::io::SeekFrom::Start(start)).await
    {
        return xml_error(StatusCode::INTERNAL_SERVER_ERROR, "InternalError", &e.to_string(), uri.path());
    }
    let metrics = st.metrics.clone();
    let stream = ReaderStream::with_capacity(file.take(len), 256 * 1024).inspect(move |r| {
        if let Ok(b) = r {
            metrics.s3_bytes.fetch_add(b.len() as u64, Ordering::Relaxed);
        }
    });
    builder.body(Body::from_stream(stream)).unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

async fn put_object(
    State(st): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
    headers: HeaderMap,
    uri: Uri,
    body: Body,
) -> Response {
    let auth = match authorize(&st, &uri, &headers, "PUT") {
        Ok(a) => a,
        Err(r) => return r,
    };
    if let Err(r) = bucket_ok(&auth, &bucket, uri.path()) {
        return r;
    }
    let declared = match headers.get("x-amz-content-sha256").and_then(|v| v.to_str().ok()) {
        Some(h) => h.to_string(),
        None => {
            return xml_error(
                StatusCode::BAD_REQUEST,
                "InvalidRequest",
                "missing x-amz-content-sha256",
                uri.path(),
            );
        }
    };

    let (abs, rel) = match paths::resolve_nonroot(&st.root, &key) {
        Ok(v) => v,
        Err(e) => return xml_error(StatusCode::BAD_REQUEST, "InvalidRequest", &e.message, uri.path()),
    };
    if tokio::fs::metadata(&abs).await.map(|m| m.is_dir()).unwrap_or(false) {
        return xml_error(StatusCode::CONFLICT, "ObjectAlreadyExists", "object key names a directory", uri.path());
    }
    if let Some(parent) = abs.parent() {
        if let Err(e) = tokio::fs::create_dir_all(parent).await {
            return xml_error(StatusCode::INTERNAL_SERVER_ERROR, "InternalError", &e.to_string(), uri.path());
        }
    }
    let existed = tokio::fs::metadata(&abs).await.is_ok();
    let stream = body.into_data_stream();
    let (size, sha256) = match stream_to_disk(&st, &abs, stream).await {
        Ok(v) => v,
        Err(e) => {
            return xml_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "InternalError",
                &e.message,
                uri.path(),
            );
        }
    };
    if declared != sigv4::UNSIGNED_PAYLOAD && declared != sha256 {
        let _ = tokio::fs::remove_file(&abs).await;
        return xml_error(
            StatusCode::BAD_REQUEST,
            "XAmzContentSHA256Mismatch",
            "provided x-amz-content-sha256 does not match the body",
            uri.path(),
        );
    }

    st.metrics.s3_puts.fetch_add(1, Ordering::Relaxed);
    let kind = if existed { events::UPDATED } else { events::CREATED };
    st.publish(kind, &rel, false, Some(size), None);
    tracing::info!(kind, path = %rel, size, "s3 object written");

    let meta = tokio::fs::metadata(&abs).await.ok();
    let etag = meta.as_ref().map(strong_etag).unwrap_or_else(|| content_md5_etag(&sha256));
    Response::builder()
        .status(StatusCode::OK)
        .header(header::ETAG, etag)
        .body(Body::empty())
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

async fn delete_object(
    State(st): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let auth = match authorize(&st, &uri, &headers, "DELETE") {
        Ok(a) => a,
        Err(r) => return r,
    };
    if let Err(r) = bucket_ok(&auth, &bucket, uri.path()) {
        return r;
    }
    let (abs, rel) = match paths::resolve_nonroot(&st.root, &key) {
        Ok(v) => v,
        Err(e) => return xml_error(StatusCode::BAD_REQUEST, "InvalidRequest", &e.message, uri.path()),
    };
    let meta = match tokio::fs::metadata(&abs).await {
        Ok(m) => m,
        Err(_) => {
            // S3 delete is idempotent.
            return Response::builder()
                .status(StatusCode::NO_CONTENT)
                .body(Body::empty())
                .unwrap_or_else(|_| StatusCode::NO_CONTENT.into_response());
        }
    };
    if meta.is_dir() {
        return xml_error(StatusCode::CONFLICT, "AccessDenied", "cannot delete a directory without recursive delete", uri.path());
    }
    if let Err(e) = tokio::fs::remove_file(&abs).await {
        return xml_error(StatusCode::INTERNAL_SERVER_ERROR, "InternalError", &e.to_string(), uri.path());
    }
    st.metrics.s3_deletes.fetch_add(1, Ordering::Relaxed);
    st.publish(events::DELETED, &rel, false, None, None);
    Response::builder()
        .status(StatusCode::NO_CONTENT)
        .body(Body::empty())
        .unwrap_or_else(|_| StatusCode::NO_CONTENT.into_response())
}

fn hv(s: &str) -> HeaderValue {
    HeaderValue::from_str(s).unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream"))
}

fn request_id(st: &AppState, rel: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    rel.hash(&mut h);
    st.metrics.started_ms().wrapping_add(h.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn split_path(path: &str) -> Option<(&str, String)> {
        let trimmed = path.trim_start_matches('/');
        let (bucket, key) = trimmed.split_once('/')?;
        if bucket.is_empty() || key.is_empty() {
            return None;
        }
        Some((bucket, key.to_string()))
    }

    #[test]
    fn splits_bucket_and_key() {
        assert_eq!(split_path("b/f"), Some(("b", "f".to_string())));
        assert_eq!(split_path("/b/a/b c/d.mp4"), Some(("b", "a/b c/d.mp4".to_string())));
        assert_eq!(split_path("b"), None);
    }

    #[test]
    fn xml_is_escaped() {
        assert_eq!(escape_xml("a<b&c\"d"), "a&lt;b&amp;c&quot;d");
    }
}
