//! Integration tests for the S3 read surface: SigV4 authentication, path-style
//! addressing, ranged GET/HEAD, and S3-shaped errors, driven in-process.

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use ostore::s3::S3Auth;
use ostore::s3::sigv4::{self, EMPTY_SHA256};
use ostore::{AppState, build_router};
use tempfile::TempDir;
use tower::ServiceExt;

const AK: &str = "AKIDEXAMPLE";
const SK: &str = "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY";
const REGION: &str = "us-east-1";
const BUCKET: &str = "ostore-test";
const TOKEN: &str = "test-token";

struct Fixture {
    app: Router,
    _root: TempDir,
}

fn fixture_with(bucket: Option<&str>) -> Fixture {
    let root = tempfile::tempdir().expect("tempdir");
    let s3 = S3Auth {
        access_key: AK.to_string(),
        secret_key: SK.to_string(),
        region: Some(REGION.to_string()),
        bucket: bucket.map(str::to_string),
    };
    let state = AppState::with_s3(root.path().to_path_buf(), TOKEN.to_string(), Some(s3)).expect("state");
    let app = build_router(state);
    Fixture { app, _root: root }
}

fn fixture() -> Fixture {
    fixture_with(Some(BUCKET))
}

fn now_amz() -> String {
    chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string()
}

/// A request signed the way an S3 SDK signs a bodyless GET/HEAD.
fn signed(method: &str, uri: &str, extra: &[(&str, &str)]) -> Request<Body> {
    signed_with(method, uri, extra, AK, SK, REGION, &now_amz())
}

fn signed_with(
    method: &str,
    uri: &str,
    extra: &[(&str, &str)],
    access_key: &str,
    secret_key: &str,
    region: &str,
    amz_date: &str,
) -> Request<Body> {
    let (path, query) = uri.split_once('?').unwrap_or((uri, ""));
    let mut headers: Vec<(String, String)> = vec![
        ("host".to_string(), "ostore.test".to_string()),
        ("x-amz-date".to_string(), amz_date.to_string()),
        ("x-amz-content-sha256".to_string(), EMPTY_SHA256.to_string()),
    ];
    for (k, v) in extra {
        headers.push(((*k).to_string(), (*v).to_string()));
    }
    let authz = sigv4::authorization(
        access_key, secret_key, region, "s3", method, path, query, &headers, amz_date, EMPTY_SHA256,
    );
    let mut b = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::HOST, "ostore.test")
        .header("x-amz-date", amz_date)
        .header("x-amz-content-sha256", EMPTY_SHA256)
        .header(header::AUTHORIZATION, authz);
    for (k, v) in extra {
        b = b.header(*k, *v);
    }
    b.body(Body::empty()).expect("request")
}

async fn seed(app: &Router, key: &str, data: &[u8]) {
    // Percent-encode so keys with spaces/unicode survive the URI builder.
    let encoded: String = key
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect();
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/api/files/{encoded}"))
                .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
                .body(Body::from(data.to_vec()))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(resp.status(), StatusCode::OK);
}

async fn body_text(resp: axum::response::Response) -> String {
    let bytes = resp.into_body().collect().await.expect("collect").to_bytes();
    String::from_utf8_lossy(&bytes).into_owned()
}

#[tokio::test]
async fn get_object_serves_bytes_with_s3_headers() {
    let f = fixture();
    let data = b"hello s3 surface";
    seed(&f.app, "media/clip.bin", data).await;

    let resp = f
        .app
        .clone()
        .oneshot(signed("GET", &format!("/{BUCKET}/media/clip.bin"), &[]))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.headers()["accept-ranges"], "bytes");
    assert_eq!(resp.headers()["content-length"], data.len().to_string());
    let etag = resp.headers()["etag"].to_str().unwrap().to_string();
    assert!(etag.starts_with('"') && etag.ends_with('"'), "strong etag expected, got {etag}");
    assert!(resp.headers().contains_key(header::LAST_MODIFIED));
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&bytes[..], data);
}

#[tokio::test]
async fn head_object_has_headers_and_no_body() {
    let f = fixture();
    seed(&f.app, "a.bin", b"0123456789").await;

    let resp = f
        .app
        .clone()
        .oneshot(signed("HEAD", &format!("/{BUCKET}/a.bin"), &[]))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.headers()["content-length"], "10");
    assert!(resp.into_body().collect().await.unwrap().to_bytes().is_empty());
}

#[tokio::test]
async fn ranged_get_returns_206_and_content_range() {
    let f = fixture();
    let data = b"0123456789abcdef";
    seed(&f.app, "r.bin", data).await;

    let resp = f
        .app
        .clone()
        .oneshot(signed("GET", &format!("/{BUCKET}/r.bin"), &[("range", "bytes=4-9")]))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(
        resp.headers()["content-range"].to_str().unwrap(),
        format!("bytes 4-9/{}", data.len())
    );
    assert_eq!(&resp.into_body().collect().await.unwrap().to_bytes()[..], &data[4..=9]);
}

#[tokio::test]
async fn unsatisfiable_range_reports_object_size() {
    // This is the contract cachey relies on to learn an object's size.
    let f = fixture();
    seed(&f.app, "small.bin", &[0u8; 10]).await;

    let resp = f
        .app
        .clone()
        .oneshot(signed("GET", &format!("/{BUCKET}/small.bin"), &[("range", "bytes=64-127")]))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::RANGE_NOT_SATISFIABLE);
    assert_eq!(resp.headers()["content-range"], "bytes */10");
    let body = body_text(resp).await;
    assert!(body.contains("<Code>InvalidRange</Code>"), "{body}");
}

#[tokio::test]
async fn missing_key_is_s3_xml_error() {
    let f = fixture();
    let resp = f
        .app
        .clone()
        .oneshot(signed("GET", &format!("/{BUCKET}/nope.bin"), &[]))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    assert_eq!(resp.headers()["content-type"], "application/xml");
    let body = body_text(resp).await;
    assert!(body.contains("<Code>NoSuchKey</Code>"), "{body}");
}

#[tokio::test]
async fn wrong_secret_is_rejected() {
    let f = fixture();
    seed(&f.app, "x.bin", b"x").await;
    let resp = f
        .app
        .clone()
        .oneshot(signed_with(
            "GET",
            &format!("/{BUCKET}/x.bin"),
            &[],
            AK,
            "not-the-secret",
            REGION,
            &now_amz(),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    let body = body_text(resp).await;
    assert!(body.contains("<Code>SignatureDoesNotMatch</Code>"), "{body}");
}

#[tokio::test]
async fn wrong_access_key_is_rejected() {
    let f = fixture();
    let resp = f
        .app
        .clone()
        .oneshot(signed_with(
            "GET",
            &format!("/{BUCKET}/x.bin"),
            &[],
            "AKIDOTHER",
            SK,
            REGION,
            &now_amz(),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    assert!(body_text(resp).await.contains("<Code>InvalidAccessKeyId</Code>"));
}

#[tokio::test]
async fn unsigned_request_is_rejected() {
    let f = fixture();
    seed(&f.app, "y.bin", b"y").await;
    let resp = f
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/{BUCKET}/y.bin"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    assert!(body_text(resp).await.contains("<Code>AccessDenied</Code>"));
}

#[tokio::test]
async fn stale_timestamp_is_rejected() {
    let f = fixture();
    let resp = f
        .app
        .clone()
        .oneshot(signed_with(
            "GET",
            &format!("/{BUCKET}/x.bin"),
            &[],
            AK,
            SK,
            REGION,
            "20150830T123600Z",
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    assert!(body_text(resp).await.contains("<Code>RequestTimeTooSkewed</Code>"));
}

#[tokio::test]
async fn other_bucket_is_refused_when_pinned() {
    let f = fixture();
    let resp = f
        .app
        .clone()
        .oneshot(signed("GET", "/other-bucket/x.bin", &[]))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    assert!(body_text(resp).await.contains("<Code>NoSuchBucket</Code>"));
}

#[tokio::test]
async fn any_bucket_is_accepted_when_unpinned() {
    let f = fixture_with(None);
    seed(&f.app, "z.bin", b"zz").await;
    let resp = f
        .app
        .clone()
        .oneshot(signed("GET", "/whatever/z.bin", &[]))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(&resp.into_body().collect().await.unwrap().to_bytes()[..], b"zz");
}

#[tokio::test]
async fn if_none_match_returns_304() {
    let f = fixture();
    seed(&f.app, "e.bin", b"etag me").await;
    let first = f
        .app
        .clone()
        .oneshot(signed("GET", &format!("/{BUCKET}/e.bin"), &[]))
        .await
        .unwrap();
    let etag = first.headers()["etag"].to_str().unwrap().to_string();
    let resp = f
        .app
        .clone()
        .oneshot(signed("GET", &format!("/{BUCKET}/e.bin"), &[("if-none-match", &etag)]))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_MODIFIED);
}

#[tokio::test]
async fn overwrite_changes_etag() {
    let f = fixture();
    seed(&f.app, "o.bin", b"one").await;
    let first = f
        .app
        .clone()
        .oneshot(signed("HEAD", &format!("/{BUCKET}/o.bin"), &[]))
        .await
        .unwrap();
    let etag1 = first.headers()["etag"].to_str().unwrap().to_string();
    seed(&f.app, "o.bin", b"two-longer").await;
    let second = f
        .app
        .clone()
        .oneshot(signed("HEAD", &format!("/{BUCKET}/o.bin"), &[]))
        .await
        .unwrap();
    assert_ne!(etag1, second.headers()["etag"]);
}

#[tokio::test]
async fn key_with_slashes_and_dots_is_rejected_like_the_json_api() {
    let f = fixture();
    let resp = f
        .app
        .clone()
        .oneshot(signed("GET", &format!("/{BUCKET}/../escape.bin"), &[]))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    assert!(body_text(resp).await.contains("<Code>InvalidRequest</Code>"));
}

#[tokio::test]
async fn json_api_and_healthz_still_route() {
    let f = fixture();
    let resp = f
        .app
        .clone()
        .oneshot(Request::builder().uri("/healthz").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn unknown_api_path_stays_json_and_authenticated() {
    // Regression: unmatched /api paths must not fall through to the S3
    // wildcard, which would answer in XML and without the bearer token.
    let f = fixture();
    let resp = f
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/does-not-exist")
                .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    assert_eq!(resp.headers()["content-type"], "application/json");

    let resp = f
        .app
        .clone()
        .oneshot(Request::builder().uri("/api/does-not-exist").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn whole_object_range_is_206_with_content_range() {
    // S3 answers 206 to any satisfiable Range, even one spanning the whole
    // object; cachey rejects a ranged read that lacks Content-Range.
    let f = fixture();
    let data = b"0123456789";
    seed(&f.app, "whole.bin", data).await;
    let resp = f
        .app
        .clone()
        .oneshot(signed("GET", &format!("/{BUCKET}/whole.bin"), &[("range", "bytes=0-9")]))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(resp.headers()["content-range"], format!("bytes 0-9/{}", data.len()));
    assert_eq!(&resp.into_body().collect().await.unwrap().to_bytes()[..], data);
}

#[tokio::test]
async fn percent_encoded_key_round_trips() {
    let f = fixture();
    let key = "videos/2026 春节 clip (final)~v2.mp4";
    seed(&f.app, key, b"unicode key").await;
    let encoded = "/videos/2026%20%E6%98%A5%E8%8A%82%20clip%20(final)~v2.mp4";
    let resp = f
        .app
        .clone()
        .oneshot(signed("GET", &format!("/{BUCKET}{encoded}"), &[]))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(&resp.into_body().collect().await.unwrap().to_bytes()[..], b"unicode key");
}

#[tokio::test]
async fn disabled_surface_reports_how_to_enable_it() {
    let root = tempfile::tempdir().expect("tempdir");
    let state = AppState::new(root.path().to_path_buf(), TOKEN.to_string()).expect("state");
    let app = build_router(state);
    let resp = app
        .clone()
        .oneshot(signed("GET", &format!("/{BUCKET}/x.bin"), &[]))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    let body = body_text(resp).await;
    assert!(body.contains("OSTORE_S3_ACCESS_KEY"), "{body}");
}
