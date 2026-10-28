use axum::body::Body;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};

pub fn xml_error(status: StatusCode, code: &str, message: &str, resource: &str) -> Response {
    xml_error_with(status, code, message, resource, None)
}

pub fn xml_error_with(
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

pub fn xml_ok(body: String) -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/xml")
        .header(header::CONTENT_LENGTH, body.len())
        .body(Body::from(body))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

pub fn escape_xml(s: &str) -> String {
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

pub fn s3_iso8601(t: chrono::DateTime<chrono::Utc>) -> String {
    t.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

pub fn http_date(t: Option<std::time::SystemTime>) -> String {
    let dt: chrono::DateTime<chrono::Utc> = t.map(chrono::DateTime::<chrono::Utc>::from).unwrap_or_else(chrono::Utc::now);
    dt.format("%a, %d %b %Y %H:%M:%S GMT").to_string()
}

pub fn strong_etag(meta: &std::fs::Metadata) -> String {
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("\"{:x}-{:x}\"", meta.len(), mtime)
}

pub fn content_md5_etag(hex_sha256: &str) -> String {
    // S3 ETag for a simple PUT is often quoted MD5; we expose sha256 hex as
    // quoted hex for lack of MD5 on the hot path (clients accept non-MD5 etags).
    format!("\"{hex_sha256}\"")
}
