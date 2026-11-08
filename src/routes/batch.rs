//! POST /api/batch — write a burst of small objects in one round trip.
//!
//! Request (JSON; `Content-Type` is not required):
//!   {"objects":[{"path":"a/b.json","data_b64":"eyJ2IjoxfQ==","content_type":"application/json"}]}
//! Response 200:
//!   {"written":[{"path","size","md5","sha256","created"}],"failed":[{"path","error"}]}
//!
//! The whole request is validated before anything touches the disk: at most
//! [`MAX_OBJECTS`] objects, at most [`MAX_DECODED_BYTES`] decoded bytes in
//! total, every `path` resolved like `PUT /api/files/{*path}` (root
//! rejected), every `data_b64` standard (padded) base64. Any violation is a
//! `400` (`TOO_MANY_OBJECTS`, `BODY_TOO_LARGE`, `BAD_PATH`, `BAD_BASE64`,
//! `BAD_JSON`) and nothing is written.
//!
//! Objects are then committed sequentially in request order through the same
//! helper as `PUT` ([`files::write_object`]): atomic temp-file + fsync +
//! rename, checksum cache refresh, upload metrics, one `object.created` /
//! `object.updated` event per object (so SSE/webhook consumers see exactly
//! what N `PUT`s would produce, in order), cluster replication. A per-object
//! failure at that stage (e.g. the target is a directory) is reported in
//! `failed` and does not abort the rest; the response is still 200.
//! `content_type` is accepted and ignored: like `PUT`, ostore stores bytes
//! only and detects the served type from the path/content on read.

use std::convert::Infallible;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use axum::Json;
use axum::body::Bytes;
use axum::extract::State;
use axum::extract::rejection::BytesRejection;
use axum::http::StatusCode;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::AppState;
use crate::error::ApiError;
use crate::paths;
use crate::routes::files;

/// Maximum objects per batch.
pub const MAX_OBJECTS: usize = 256;

/// Maximum total decoded payload per batch (16 MiB).
pub const MAX_DECODED_BYTES: usize = 16 * 1024 * 1024;

/// Request body limit for this route (overrides axum's 2 MB default): the
/// base64 expansion of [`MAX_DECODED_BYTES`] plus 4 MiB of headroom for JSON
/// framing, paths and content types, so every legal batch is accepted.
pub const MAX_BODY_BYTES: usize = MAX_DECODED_BYTES.div_ceil(3) * 4 + 4 * 1024 * 1024;

#[derive(Deserialize)]
struct BatchReq {
    objects: Vec<BatchObject>,
}

/// One requested object. Unknown fields (incl. `content_type`) are ignored.
#[derive(Deserialize)]
struct BatchObject {
    path: String,
    data_b64: String,
}

/// A validated object, ready to commit.
#[derive(Debug)]
struct Planned {
    abs: PathBuf,
    rel: String,
    data: Vec<u8>,
}

/// Parse and validate the whole request without touching the disk.
fn plan(root: &Path, body: &[u8]) -> Result<Vec<Planned>, ApiError> {
    let req: BatchReq =
        serde_json::from_slice(body).map_err(|e| ApiError::bad_request("BAD_JSON", format!("invalid batch body: {e}")))?;
    if req.objects.len() > MAX_OBJECTS {
        return Err(ApiError::bad_request(
            "TOO_MANY_OBJECTS",
            format!("{} objects in batch; the limit is {MAX_OBJECTS}", req.objects.len()),
        ));
    }
    let mut total = 0usize;
    let mut out = Vec::with_capacity(req.objects.len());
    for (i, obj) in req.objects.into_iter().enumerate() {
        let (abs, rel) = paths::resolve_nonroot(root, &obj.path)
            .map_err(|e| ApiError::bad_path(format!("objects[{i}] {:?}: {}", obj.path, e.message)))?;
        let data = STANDARD
            .decode(obj.data_b64.as_bytes())
            .map_err(|e| ApiError::bad_request("BAD_BASE64", format!("objects[{i}] {rel:?}: invalid base64: {e}")))?;
        total += data.len();
        if total > MAX_DECODED_BYTES {
            return Err(ApiError::bad_request(
                "BODY_TOO_LARGE",
                format!("decoded batch exceeds {MAX_DECODED_BYTES} bytes at objects[{i}] {rel:?}"),
            ));
        }
        out.push(Planned { abs, rel, data });
    }
    Ok(out)
}

/// POST /api/batch — see the module docs.
pub async fn batch_put(
    State(st): State<AppState>,
    body: Result<Bytes, BytesRejection>,
) -> Result<Json<Value>, ApiError> {
    let planned = body
        .map_err(|e| {
            if e.status() == StatusCode::PAYLOAD_TOO_LARGE {
                ApiError::bad_request("BODY_TOO_LARGE", format!("request body exceeds {MAX_BODY_BYTES} bytes"))
            } else {
                ApiError::bad_request("BODY_ERROR", e.body_text())
            }
        })
        .and_then(|b| plan(&st.root, &b));
    let planned = match planned {
        Ok(p) => p,
        Err(e) => {
            st.metrics.batch_rejected.fetch_add(1, Ordering::Relaxed);
            tracing::warn!(code = e.code, error = %e.message, "batch rejected");
            return Err(e);
        }
    };
    st.metrics.batch_ok.fetch_add(1, Ordering::Relaxed);

    let mut written = Vec::with_capacity(planned.len());
    let mut failed = Vec::new();
    let mut bytes = 0u64;
    for Planned { abs, rel, data } in planned {
        let stream = futures::stream::iter([Ok::<Bytes, Infallible>(Bytes::from(data))]);
        match files::write_object(&st, &abs, &rel, stream).await {
            Ok((size, md5, sha256, existed)) => {
                bytes += size;
                written.push(json!({ "path": rel, "size": size, "md5": md5, "sha256": sha256, "created": !existed }));
            }
            Err(e) => {
                tracing::warn!(path = %rel, code = e.code, error = %e.message, "batch object failed");
                failed.push(json!({ "path": rel, "error": format!("{}: {}", e.code, e.message) }));
            }
        }
    }
    st.metrics.batch_objects_written.fetch_add(written.len() as u64, Ordering::Relaxed);
    st.metrics.batch_objects_failed.fetch_add(failed.len() as u64, Ordering::Relaxed);
    tracing::info!(written = written.len(), failed = failed.len(), bytes, "batch committed");
    Ok(Json(json!({ "written": written, "failed": failed })))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> PathBuf {
        PathBuf::from("/srv/data")
    }

    fn body(objects: Value) -> Vec<u8> {
        json!({ "objects": objects }).to_string().into_bytes()
    }

    fn obj(path: &str, data: &[u8]) -> Value {
        json!({ "path": path, "data_b64": STANDARD.encode(data), "content_type": "application/json" })
    }

    fn code(r: Result<Vec<Planned>, ApiError>) -> &'static str {
        r.expect_err("expected a validation error").code
    }

    #[test]
    fn body_limit_admits_a_full_legal_batch() {
        let b64_len = STANDARD.encode(vec![0u8; MAX_DECODED_BYTES]).len();
        assert!(MAX_BODY_BYTES >= b64_len + 256 * 1024, "limit {MAX_BODY_BYTES} vs payload {b64_len}");
    }

    #[test]
    fn plans_valid_objects_in_order() {
        let p = plan(&root(), &body(json!([obj("a/x.json", b"{}"), obj("./a//y.json", b""), obj("z", b"zz")]))).unwrap();
        let rels: Vec<&str> = p.iter().map(|o| o.rel.as_str()).collect();
        assert_eq!(rels, ["a/x.json", "a/y.json", "z"]);
        assert_eq!(p[0].abs, PathBuf::from("/srv/data/a/x.json"));
        assert_eq!(p[0].data, b"{}");
        assert!(p[1].data.is_empty());
        // Empty batch is a valid no-op.
        assert!(plan(&root(), &body(json!([]))).unwrap().is_empty());
    }

    #[test]
    fn rejects_bad_paths_like_put() {
        for bad in ["", "/", ".", "/abs", "../x", "a/../../b", ".ostore/x", "a\u{0}b"] {
            assert_eq!(code(plan(&root(), &body(json!([obj("ok", b"1"), obj(bad, b"1")])))), "BAD_PATH", "{bad:?}");
        }
    }

    #[test]
    fn rejects_invalid_base64() {
        let b = body(json!([{ "path": "a", "data_b64": "not base64!" }]));
        assert_eq!(code(plan(&root(), &b)), "BAD_BASE64");
        // Unpadded input is not canonical standard base64.
        let b = body(json!([{ "path": "a", "data_b64": "eyJ2IjoxfQ" }]));
        assert_eq!(code(plan(&root(), &b)), "BAD_BASE64");
    }

    #[test]
    fn enforces_object_count_and_decoded_size() {
        let at_cap: Vec<Value> = (0..MAX_OBJECTS).map(|i| obj(&format!("o/{i}"), b"x")).collect();
        assert_eq!(plan(&root(), &body(json!(at_cap))).unwrap().len(), MAX_OBJECTS);
        let over: Vec<Value> = (0..=MAX_OBJECTS).map(|i| obj(&format!("o/{i}"), b"x")).collect();
        assert_eq!(code(plan(&root(), &body(json!(over)))), "TOO_MANY_OBJECTS");

        let half = vec![7u8; MAX_DECODED_BYTES / 2];
        assert_eq!(plan(&root(), &body(json!([obj("a", &half), obj("b", &half)]))).unwrap().len(), 2);
        assert_eq!(code(plan(&root(), &body(json!([obj("a", &half), obj("b", &half), obj("c", b"!")])))), "BODY_TOO_LARGE");
    }

    #[test]
    fn rejects_malformed_json() {
        assert_eq!(code(plan(&root(), b"{")), "BAD_JSON");
        assert_eq!(code(plan(&root(), br#"{"objects":[{"path":"a"}]}"#)), "BAD_JSON");
        assert_eq!(code(plan(&root(), br#"{}"#)), "BAD_JSON");
    }
}
