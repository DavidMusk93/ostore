//! AWS Signature Version 4 verification (header-based auth).
//!
//! Header-based SigV4 for S3-shaped reads and writes. No presigned query auth,
//! no chunked `STREAMING-AWS4-HMAC-SHA256-PAYLOAD`. PUT accepts a declared
//! `x-amz-content-sha256` of the body hex digest or `UNSIGNED-PAYLOAD`.
//! signing, no presigned query auth. The canonical-request / string-to-sign /
//! signing-key pipeline is exposed verbatim so the test suite can check it
//! against the AWS signing test vectors.

use std::fmt::Write as _;

use axum::http::{HeaderMap, Uri};
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};

use super::S3Auth;

/// SHA-256 of the empty string; the payload hash of a bodyless request.
pub const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

/// Declared in `x-amz-content-sha256` when the body is not hashed (HTTPS only).
pub const UNSIGNED_PAYLOAD: &str = "UNSIGNED-PAYLOAD";

/// Clock skew tolerated between the client's `x-amz-date` and ours.
const MAX_SKEW_SECS: i64 = 900;

const SERVICE: &str = "s3";
const TERMINATOR: &str = "aws4_request";

#[derive(Debug, PartialEq, Eq)]
pub enum AuthError {
    /// No `Authorization` header at all.
    Missing,
    /// Unparseable, or a scope we cannot honor.
    Malformed(&'static str),
    /// Presigned query auth is not implemented.
    PresignedUnsupported,
    InvalidAccessKeyId,
    SignatureMismatch,
    RequestTimeTooSkewed,
}

impl AuthError {
    /// S3 error code for the XML body.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Missing | Self::Malformed(_) => "AccessDenied",
            Self::PresignedUnsupported => "InvalidRequest",
            Self::InvalidAccessKeyId => "InvalidAccessKeyId",
            Self::SignatureMismatch => "SignatureDoesNotMatch",
            Self::RequestTimeTooSkewed => "RequestTimeTooSkewed",
        }
    }

    pub fn message(&self) -> String {
        match self {
            Self::Missing => "request is missing an Authorization header".to_string(),
            Self::Malformed(why) => format!("malformed Authorization header: {why}"),
            Self::PresignedUnsupported => {
                "presigned query authentication is not supported; use the Authorization header".to_string()
            }
            Self::InvalidAccessKeyId => "the access key id is not valid".to_string(),
            Self::SignatureMismatch => {
                "the request signature we calculated does not match the signature you provided".to_string()
            }
            Self::RequestTimeTooSkewed => format!(
                "the difference between the request time and the server time is larger than {MAX_SKEW_SECS}s"
            ),
        }
    }
}

/// Verify the SigV4 `Authorization` header on `method uri`.
pub fn verify(
    auth: &S3Auth,
    method: &str,
    uri: &Uri,
    headers: &HeaderMap,
    now: DateTime<Utc>,
) -> Result<(), AuthError> {
    if uri.query().is_some_and(|q| q.contains("X-Amz-Signature")) {
        return Err(AuthError::PresignedUnsupported);
    }
    let value = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .ok_or(AuthError::Missing)?;
    let rest = value
        .strip_prefix("AWS4-HMAC-SHA256 ")
        .ok_or(AuthError::Malformed("expected the AWS4-HMAC-SHA256 algorithm"))?;
    let (mut credential, mut signed_headers, mut signature) = (None, None, None);
    for field in rest.split(',') {
        let field = field.trim();
        if let Some(v) = field.strip_prefix("Credential=") {
            credential = Some(v);
        } else if let Some(v) = field.strip_prefix("SignedHeaders=") {
            signed_headers = Some(v);
        } else if let Some(v) = field.strip_prefix("Signature=") {
            signature = Some(v);
        }
    }
    let credential = credential.ok_or(AuthError::Malformed("no Credential"))?;
    let signed_headers = signed_headers.ok_or(AuthError::Malformed("no SignedHeaders"))?;
    let signature = signature.ok_or(AuthError::Malformed("no Signature"))?;

    let scope: Vec<&str> = credential.split('/').collect();
    let [access_key, date, region, service, terminator] = scope[..] else {
        return Err(AuthError::Malformed("Credential is not <key>/<date>/<region>/<service>/<terminator>"));
    };
    if terminator != TERMINATOR {
        return Err(AuthError::Malformed("Credential terminator must be aws4_request"));
    }
    if service != SERVICE {
        return Err(AuthError::Malformed("Credential service must be s3"));
    }
    if access_key != &*auth.access_key {
        return Err(AuthError::InvalidAccessKeyId);
    }
    if let Some(want) = &auth.region
        && *want != region
    {
        return Err(AuthError::Malformed("Credential region does not match the configured region"));
    }

    let amz_date = headers
        .get("x-amz-date")
        .and_then(|v| v.to_str().ok())
        .ok_or(AuthError::Malformed("no x-amz-date header"))?;
    let parsed = parse_amz_date(amz_date).ok_or(AuthError::Malformed("x-amz-date is not ISO 8601 basic"))?;
    if (now - parsed).num_seconds().abs() > MAX_SKEW_SECS {
        return Err(AuthError::RequestTimeTooSkewed);
    }
    if &amz_date[..8] != date {
        return Err(AuthError::Malformed("Credential date does not match x-amz-date"));
    }

    let signed: Vec<String> = signed_headers.split(';').map(|s| s.to_ascii_lowercase()).collect();
    if signed.is_empty() || signed.windows(2).any(|w| w[0] >= w[1]) {
        return Err(AuthError::Malformed("SignedHeaders must be sorted and non-empty"));
    }
    let headers_block = canonical_headers(headers, &signed)?;
    let payload_hash = headers
        .get("x-amz-content-sha256")
        .and_then(|v| v.to_str().ok())
        .unwrap_or(EMPTY_SHA256);

    let canonical = canonical_request_from(
        method,
        uri.path(),
        uri.query().unwrap_or(""),
        &headers_block,
        &signed.join(";"),
        payload_hash,
    );
    let scope = format!("{date}/{region}/{service}/{TERMINATOR}");
    let string_to_sign = string_to_sign(amz_date, &scope, &canonical);
    let key = signing_key(&auth.secret_key, date, region, service);
    let expected = hex::encode(hmac_sha256(&key, string_to_sign.as_bytes()));
    if !constant_time_eq(expected.as_bytes(), signature.trim().as_bytes()) {
        return Err(AuthError::SignatureMismatch);
    }
    Ok(())
}

fn parse_amz_date(s: &str) -> Option<DateTime<Utc>> {
    // `20260830T123600Z`: basic format, no colon in the offset, so chrono's
    // DateTime parser rejects it; parse naive and pin the zone to UTC.
    chrono::NaiveDateTime::parse_from_str(s, "%Y%m%dT%H%M%SZ")
        .ok()
        .map(|d| d.and_utc())
}

/// `name:value\n` for each signed header, in the client's (sorted) order.
/// `host` must be present: HTTP/1.1 always sends it.
fn canonical_headers(headers: &HeaderMap, signed: &[String]) -> Result<String, AuthError> {
    let mut out = String::new();
    for name in signed {
        let values: Vec<String> = headers
            .get_all(name.as_str())
            .iter()
            .filter_map(|v| v.to_str().ok())
            .map(normalize_ws)
            .collect();
        if values.is_empty() {
            return Err(AuthError::Malformed("SignedHeaders lists a header the request does not carry"));
        }
        let _ = writeln!(out, "{name}:{}", values.join(","));
    }
    Ok(out)
}

/// Trim and collapse runs of whitespace, per SigV4's header canonicalization.
fn normalize_ws(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    let mut space = false;
    for c in v.trim().chars() {
        if c == ' ' || c == '\t' {
            space = true;
        } else {
            if space && !out.is_empty() {
                out.push(' ');
            }
            space = false;
            out.push(c);
        }
    }
    out
}

/// Canonical query string: every pair percent-decoded, re-encoded with the
/// unreserved set, then sorted by key and value.
fn canonical_query(raw: &str) -> String {
    let mut pairs: Vec<(String, String)> = raw
        .split('&')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (k, v) = p.split_once('=').unwrap_or((p, ""));
            (uri_encode(&percent_decode(k)), uri_encode(&percent_decode(v)))
        })
        .collect();
    pairs.sort();
    pairs.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join("&")
}

/// RFC 3986 unreserved characters stay literal; everything else becomes %XX.
fn uri_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => {
                let _ = write!(out, "%{b:02X}");
            }
        }
    }
    out
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2])) {
                out.push(h << 4 | l);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Build a canonical request from parts, returning `(canonical_request,
/// signed_headers)`.
///
/// The URI is used exactly as transmitted (S3 does not double-encode or
/// normalize the path); the query is canonicalized here.
pub fn canonical_request(
    method: &str,
    uri_path: &str,
    raw_query: &str,
    headers: &[(String, String)],
    payload_hash: &str,
) -> (String, String) {
    let mut pairs: Vec<(String, String)> = headers
        .iter()
        .map(|(k, v)| (k.to_ascii_lowercase(), normalize_ws(v)))
        .collect();
    pairs.sort_by(|a, b| a.0.cmp(&b.0));
    let signed = pairs.iter().map(|(k, _)| k.clone()).collect::<Vec<_>>().join(";");
    let mut block = String::new();
    for (k, v) in &pairs {
        let _ = writeln!(block, "{k}:{v}");
    }
    let canonical = canonical_request_from(method, uri_path, raw_query, &block, &signed, payload_hash);
    (canonical, signed)
}

/// Assemble a canonical request from an already-built header block.
pub fn canonical_request_from(
    method: &str,
    uri_path: &str,
    raw_query: &str,
    headers_block: &str,
    signed_headers: &str,
    payload_hash: &str,
) -> String {
    let path = if uri_path.is_empty() { "/" } else { uri_path };
    format!(
        "{method}\n{path}\n{}\n{headers_block}\n{signed_headers}\n{payload_hash}",
        canonical_query(raw_query)
    )
}

pub fn string_to_sign(amz_date: &str, scope: &str, canonical_request: &str) -> String {
    format!(
        "AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{}",
        hex::encode(Sha256::digest(canonical_request.as_bytes()))
    )
}

pub fn signing_key(secret_key: &str, date: &str, region: &str, service: &str) -> [u8; 32] {
    let k_date = hmac_sha256(format!("AWS4{secret_key}").as_bytes(), date.as_bytes());
    let k_region = hmac_sha256(&k_date, region.as_bytes());
    let k_service = hmac_sha256(&k_region, service.as_bytes());
    hmac_sha256(&k_service, TERMINATOR.as_bytes())
}

pub fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    const BLOCK: usize = 64;
    let mut padded = [0u8; BLOCK];
    if key.len() > BLOCK {
        padded[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        padded[..key.len()].copy_from_slice(key);
    }
    let mut inner = Sha256::new();
    let mut outer = Sha256::new();
    let mut ipad = [0x36u8; BLOCK];
    let mut opad = [0x5cu8; BLOCK];
    for i in 0..BLOCK {
        ipad[i] ^= padded[i];
        opad[i] ^= padded[i];
    }
    inner.update(ipad);
    inner.update(data);
    let inner = inner.finalize();
    outer.update(opad);
    outer.update(inner);
    outer.finalize().into()
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Produce the `Authorization` header value for a request.
///
/// The server never needs this; it exists so tests and scripts can mint
/// requests the server accepts.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn authorization(
    access_key: &str,
    secret_key: &str,
    region: &str,
    service: &str,
    method: &str,
    uri_path: &str,
    raw_query: &str,
    headers: &[(String, String)],
    amz_date: &str,
    payload_hash: &str,
) -> String {
    let (canonical, signed) = canonical_request(method, uri_path, raw_query, headers, payload_hash);
    let scope = format!("{}/{region}/{service}/{TERMINATOR}", &amz_date[..8]);
    let sts = string_to_sign(amz_date, &scope, &canonical);
    let key = signing_key(secret_key, &amz_date[..8], region, service);
    let signature = hex::encode(hmac_sha256(&key, sts.as_bytes()));
    format!(
        "AWS4-HMAC-SHA256 Credential={access_key}/{scope}, SignedHeaders={signed}, Signature={signature}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const AK: &str = "AKIDEXAMPLE";
    const SK: &str = "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY";
    const DATE: &str = "20150830T123600Z";

    fn hdrs(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs.iter().map(|(k, v)| ((*k).to_string(), (*v).to_string())).collect()
    }

    // Vectors from awslabs/aws-c-auth tests/aws-signing-test-suite/v4.

    #[test]
    fn get_vanilla_header_auth() {
        let (canonical, signed) = canonical_request(
            "GET",
            "/",
            "",
            &hdrs(&[("Host", "example.amazonaws.com"), ("X-Amz-Date", DATE)]),
            EMPTY_SHA256,
        );
        assert_eq!(signed, "host;x-amz-date");
        assert_eq!(
            canonical,
            "GET\n/\n\nhost:example.amazonaws.com\nx-amz-date:20150830T123600Z\n\nhost;x-amz-date\n\
             e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        let sts = string_to_sign(DATE, "20150830/us-east-1/service/aws4_request", &canonical);
        assert_eq!(
            sts,
            "AWS4-HMAC-SHA256\n20150830T123600Z\n20150830/us-east-1/service/aws4_request\n\
             bb579772317eb040ac9ed261061d46c1f17a8133879d6129b6e1c25292927e63"
        );
        let key = signing_key(SK, "20150830", "us-east-1", "service");
        assert_eq!(
            hex::encode(hmac_sha256(&key, sts.as_bytes())),
            "5fa00fa31553b73ebf1942676e86291e8372ff2a2260956d9b8aae1d763fbf31"
        );
    }

    #[test]
    fn get_vanilla_query_auth() {
        let query = "X-Amz-Algorithm=AWS4-HMAC-SHA256&X-Amz-Credential=AKIDEXAMPLE%2F20150830%2F\
                     us-east-1%2Fservice%2Faws4_request&X-Amz-Date=20150830T123600Z&X-Amz-Expires=3600&\
                     X-Amz-SignedHeaders=host";
        assert_eq!(
            canonical_query(query),
            "X-Amz-Algorithm=AWS4-HMAC-SHA256&X-Amz-Credential=AKIDEXAMPLE%2F20150830%2Fus-east-1%2F\
             service%2Faws4_request&X-Amz-Date=20150830T123600Z&X-Amz-Expires=3600&X-Amz-SignedHeaders=host"
        );
        let canonical = canonical_request_from("GET", "/", query, "host:example.amazonaws.com\n", "host", EMPTY_SHA256);
        let sts = string_to_sign(DATE, "20150830/us-east-1/service/aws4_request", &canonical);
        assert_eq!(
            sts,
            "AWS4-HMAC-SHA256\n20150830T123600Z\n20150830/us-east-1/service/aws4_request\n\
             bb7705b4aa3cb8e8f5e1e0b3d4c0b64030797a313c8ceee43e33117cc43eadc5"
        );
        let key = signing_key(SK, "20150830", "us-east-1", "service");
        assert_eq!(
            hex::encode(hmac_sha256(&key, sts.as_bytes())),
            "e93c787ed7f371d5c6b165c1b38ede9550f4dce4144713e844b25b7192d3865d"
        );
    }

    #[test]
    fn header_value_is_trimmed_and_collapsed() {
        assert_eq!(normalize_ws("  a  b\t\tc  "), "a b c");
        let (canonical, _) = canonical_request(
            "GET",
            "/",
            "",
            &hdrs(&[("Host", "example.amazonaws.com"), ("My-Header1", "  value2  ")]),
            EMPTY_SHA256,
        );
        assert!(canonical.contains("my-header1:value2\n"), "{canonical}");
    }

    #[test]
    fn signed_headers_are_sorted() {
        let (_, signed) = canonical_request(
            "GET",
            "/",
            "",
            &hdrs(&[("X-Amz-Date", DATE), ("Host", "h"), ("Content-Length", "0")]),
            EMPTY_SHA256,
        );
        assert_eq!(signed, "content-length;host;x-amz-date");
    }

    #[test]
    fn uri_encode_keeps_only_unreserved() {
        assert_eq!(uri_encode("a-b_c.d~e"), "a-b_c.d~e");
        assert_eq!(uri_encode("a b/c"), "a%20b%2Fc");
        assert_eq!(uri_encode("é"), "%C3%A9");
    }

    #[test]
    fn percent_decode_roundtrip() {
        assert_eq!(percent_decode("a%20b%2Fc"), "a b/c");
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%zz"), "%zz");
    }

    #[test]
    fn authorization_matches_the_vector() {
        let authz = authorization(
            AK,
            SK,
            "us-east-1",
            "service",
            "GET",
            "/",
            "",
            &hdrs(&[("Host", "example.amazonaws.com"), ("X-Amz-Date", DATE)]),
            DATE,
            EMPTY_SHA256,
        );
        assert_eq!(
            authz,
            "AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/20150830/us-east-1/service/aws4_request, \
             SignedHeaders=host;x-amz-date, \
             Signature=5fa00fa31553b73ebf1942676e86291e8372ff2a2260956d9b8aae1d763fbf31"
        );
    }

    #[test]
    fn empty_payload_hash_is_sha256_of_nothing() {
        assert_eq!(hex::encode(Sha256::digest(b"")), EMPTY_SHA256);
    }
}
