//! Minimal S3 read surface, so an S3 client (notably a read-through cache
//! such as cachey) can point at ostore.
//!
//! Implemented: `GET`/`HEAD` object with SigV4 header authentication,
//! path-style addressing, and `Range`. The first path segment is the bucket
//! name; ostore binds one root, so the bucket is either pinned
//! (`OSTORE_S3_BUCKET`) or accepted and ignored.
//!
//! Objects are served straight off disk with a strong `ETag` of size + mtime,
//! so an overwrite is visible to any client that revalidates. A cache in front
//! still assumes immutability — see the README's "S3 read surface" section.
//!
//! Errors use S3's XML shape, including `InvalidRange` with
//! `Content-Range: bytes */<size>`, which is what an SDK needs to recover the
//! object size from a 416.

pub mod sigv4;

use std::sync::atomic::Ordering;

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use chrono::{DateTime, Utc};
use futures::StreamExt;
use tokio::io::{AsyncSeekExt, AsyncReadExt};
use tokio_util::io::ReaderStream;

use crate::AppState;
use crate::paths;
use crate::range;

/// Credentials and naming for the S3 surface.
#[derive(Debug, Clone)]
pub struct S3Auth {
    pub access_key: String,
    pub secret_key: String,
    /// When set, requests whose credential scope names another region are
    /// rejected. `None` accepts any region.
    pub region: Option<String>,
    /// When set, only this bucket name is served.
    pub bucket: Option<String>,
}

impl S3Auth {
    /// `OSTORE_S3_ACCESS_KEY` + `OSTORE_S3_SECRET_KEY` enable the surface;
    /// `OSTORE_S3_REGION` and `OSTORE_S3_BUCKET` are optional constraints.
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

/// Root-level router for the S3 surface. Specific routes (`/healthz`,
/// `/api/*`) win over the wildcard.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/{*path}", get(object).head(object))
        .fallback(not_found)
}

async fn not_found(uri: Uri) -> Response {
    xml_error(StatusCode::NOT_FOUND, "NoSuchKey", "the requested resource does not exist", uri.path())
}

/// `GET`/`HEAD /{bucket}/{key}`.
async fn object(
    State(st): State<AppState>,
    Path(path): Path<String>,
    method: Method,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let Some(auth) = st.s3.clone() else {
        return xml_error(
            StatusCode::FORBIDDEN,
            "AccessDenied",
            "the S3 surface is disabled; set OSTORE_S3_ACCESS_KEY and OSTORE_S3_SECRET_KEY",
            uri.path(),
        );
    };
    let Some((bucket, key)) = split_path(&path) else {
        return xml_error(StatusCode::BAD_REQUEST, "InvalidRequest", "expected /{bucket}/{key}", uri.path());
    };
    if let Err(e) = sigv4::verify(&auth, method.as_str(), &uri, &headers, Utc::now()) {
        st.metrics.s3_denied.fetch_add(1, Ordering::Relaxed);
        return xml_error(StatusCode::FORBIDDEN, e.code(), &e.message(), uri.path());
    }
    if let Some(want) = &auth.bucket
        && *want != bucket
    {
        return xml_error(StatusCode::NOT_FOUND, "NoSuchBucket", "no such bucket", uri.path());
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

    // Conditional GET, as an SDK's retry path or a browser may send it.
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
            // An SDK recovers the object size from this Content-Range.
            return xml_error_with(
                StatusCode::RANGE_NOT_SATISFIABLE,
                "InvalidRange",
                "the requested range is not satisfiable",
                uri.path(),
                Some((header::CONTENT_RANGE, format!("bytes */{size}"))),
            );
        }
    };
    // S3 answers 206 to every satisfiable `Range`, including one that spans the
    // whole object, and always includes `Content-Range`. RFC 9110 would allow a
    // 200 there, but S3 clients (cachey's downloader included) validate the
    // Content-Range of a ranged read and reject a response without it.
    let partial = matches!(parsed, Ok(Some(_)));

    let mut builder = Response::builder()
        .status(if partial { StatusCode::PARTIAL_CONTENT } else { StatusCode::OK })
        .header(header::CONTENT_TYPE, HeaderValue::from_str(&mime).unwrap_or(HeaderValue::from_static("application/octet-stream")))
        .header(header::CONTENT_LENGTH, len)
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::ETAG, etag)
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

/// Split `bucket/key...` into its two parts. The key may contain slashes.
fn split_path(path: &str) -> Option<(&str, String)> {
    let trimmed = path.trim_start_matches('/');
    let (bucket, key) = trimmed.split_once('/')?;
    if bucket.is_empty() || key.is_empty() {
        return None;
    }
    Some((bucket, key.to_string()))
}

/// Strong validator from size + mtime. Distinct from the weak ETag the JSON
/// API returns, because S3 clients expect a quoted strong ETag and compare it
/// with `If-None-Match`.
fn strong_etag(meta: &std::fs::Metadata) -> String {
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("\"{:x}-{:x}\"", meta.len(), mtime)
}

fn http_date(t: Option<std::time::SystemTime>) -> String {
    let dt: DateTime<Utc> = t.map(DateTime::<Utc>::from).unwrap_or_else(Utc::now);
    dt.format("%a, %d %b %Y %H:%M:%S GMT").to_string()
}

fn request_id(st: &AppState, rel: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    rel.hash(&mut h);
    st.metrics.started_ms().wrapping_add(h.finish())
}

/// S3-style XML error document.
fn xml_error(status: StatusCode, code: &str, message: &str, resource: &str) -> Response {
    xml_error_with(status, code, message, resource, None)
}

fn xml_error_with(
    status: StatusCode,
    code: &str,
    message: &str,
    resource: &str,
    extra: Option<(header::HeaderName, String)>,
) -> Response {
    let body = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Error><Code>{}</Code><Message>{}</Message><Resource>{}</Resource><RequestId>ostore</RequestId></Error>\n",
        escape_xml(code),
        escape_xml(message),
        escape_xml(resource)
    );
    let mut builder = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/xml")
        .header(header::CONTENT_LENGTH, body.len());
    if let Some((name, value)) = extra {
        builder = builder.header(name, value);
    }
    builder.body(Body::from(body)).unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

fn escape_xml(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_bucket_and_key() {
        assert_eq!(split_path("b/f"), Some(("b", "f".to_string())));
        assert_eq!(split_path("/b/a/b c/d.mp4"), Some(("b", "a/b c/d.mp4".to_string())));
        assert_eq!(split_path("b"), None);
        assert_eq!(split_path("b/"), None);
        assert_eq!(split_path("/"), None);
    }

    #[test]
    fn xml_is_escaped() {
        assert_eq!(escape_xml("a<b&c\"d"), "a&lt;b&amp;c&quot;d");
    }

    #[test]
    fn http_date_is_rfc_7231() {
        let t = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_440_938_160);
        assert_eq!(http_date(Some(t)), "Sun, 30 Aug 2015 12:36:00 GMT");
    }
}
