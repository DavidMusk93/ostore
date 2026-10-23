//! Integration tests: drive the router in-process with tower::ServiceExt
//! (no real port). Uses a tempfile::TempDir as the object root.

use axum::Router;
use axum::body::{Body, Bytes};
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use ostore::{AppState, build_router};
use serde_json::{Value, json};
use tempfile::TempDir;
use tower::ServiceExt;

const TOKEN: &str = "test-token";

struct Fixture {
    app: Router,
    state: AppState,
    _root: TempDir,
}

fn fixture() -> Fixture {
    let root = tempfile::tempdir().expect("tempdir");
    let state = AppState::new(root.path().to_path_buf(), TOKEN.to_string()).expect("state");
    let app = build_router(state.clone());
    Fixture { app, state, _root: root }
}

fn req(method: &str, uri: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
        .body(Body::empty())
        .expect("request")
}

fn req_json(method: &str, uri: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .expect("request")
}

async fn body_bytes(resp: axum::response::Response) -> Bytes {
    resp.into_body()
        .collect()
        .await
        .expect("collect body")
        .to_bytes()
}

async fn body_json(resp: axum::response::Response) -> Value {
    let b = body_bytes(resp).await;
    serde_json::from_slice(&b).expect("json body")
}

async fn put_file(app: &Router, path: &str, data: &[u8]) -> axum::response::Response {
    let r = Request::builder()
        .method("PUT")
        .uri(format!("/api/files/{path}"))
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
        .body(Body::from(data.to_vec()))
        .expect("request");
    app.clone().oneshot(r).await.expect("response")
}

#[tokio::test]
async fn healthz_is_unauthenticated() {
    let f = fixture();
    let resp = f
        .app
        .clone()
        .oneshot(Request::builder().uri("/healthz").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let j = body_json(resp).await;
    assert_eq!(j["status"], "ok");
}

#[tokio::test]
async fn put_list_stat_download_move_mkdir_delete_flow() {
    let f = fixture();
    let data = b"hello ostore world";

    // PUT upload.
    let resp = put_file(&f.app, "a/b/hello.txt", data).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let j = body_json(resp).await;
    assert_eq!(j["path"], "a/b/hello.txt");
    assert_eq!(j["size"], data.len() as u64);
    assert!(j["sha256"].as_str().unwrap().len() == 64);

    // List root (recursive) sees the new object; .ostore stays hidden.
    let resp = f.app.clone().oneshot(req("GET", "/api/list?recursive=1")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let j = body_json(resp).await;
    let entries = j["entries"].as_array().unwrap();
    let paths: Vec<&str> = entries.iter().map(|e| e["path"].as_str().unwrap()).collect();
    assert!(paths.contains(&"a"));
    assert!(paths.contains(&"a/b"));
    assert!(paths.contains(&"a/b/hello.txt"));
    assert!(!paths.iter().any(|p| p.contains(".ostore")));
    // Directories sort before files within a directory.
    let names: Vec<&str> = entries.iter().map(|e| e["name"].as_str().unwrap()).collect();
    let mut sorted_names = names.clone();
    sorted_names.sort_unstable();
    assert_eq!(names, sorted_names, "entries not grouped/sorted as expected");

    // Stat.
    let resp = f.app.clone().oneshot(req("GET", "/api/stat?path=a/b/hello.txt")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let j = body_json(resp).await;
    assert_eq!(j["type"], "file");
    assert_eq!(j["size"], data.len() as u64);

    // Download full.
    let resp = f.app.clone().oneshot(req("GET", "/api/files/a/b/hello.txt")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.headers()["accept-ranges"], "bytes");
    assert_eq!(
        resp.headers()["content-type"],
        "text/plain",
        "mime_guess should detect .txt"
    );
    assert_eq!(body_bytes(resp).await, &data[..]);

    // Download with range.
    let r = Request::builder()
        .uri("/api/files/a/b/hello.txt")
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
        .header(header::RANGE, "bytes=6-12")
        .body(Body::empty())
        .unwrap();
    let resp = f.app.clone().oneshot(r).await.unwrap();
    assert_eq!(resp.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(
        resp.headers()["content-range"].to_str().unwrap(),
        format!("bytes 6-12/{}", data.len())
    );
    assert_eq!(body_bytes(resp).await, &data[6..=12]);

    // Suffix range.
    let r = Request::builder()
        .uri("/api/files/a/b/hello.txt")
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
        .header(header::RANGE, "bytes=-5")
        .body(Body::empty())
        .unwrap();
    let resp = f.app.clone().oneshot(r).await.unwrap();
    assert_eq!(resp.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(body_bytes(resp).await, &data[data.len() - 5..]);

    // Unsatisfiable range → 416 with Content-Range: bytes */size.
    let r = Request::builder()
        .uri("/api/files/a/b/hello.txt")
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
        .header(header::RANGE, "bytes=9999-10000")
        .body(Body::empty())
        .unwrap();
    let resp = f.app.clone().oneshot(r).await.unwrap();
    assert_eq!(resp.status(), StatusCode::RANGE_NOT_SATISFIABLE);
    assert_eq!(
        resp.headers()["content-range"].to_str().unwrap(),
        format!("bytes */{}", data.len())
    );

    // Move.
    let resp = f
        .app
        .clone()
        .oneshot(req_json("POST", "/api/move", json!({"from": "a/b/hello.txt", "to": "x/y/bye.txt"})))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let resp = f.app.clone().oneshot(req("GET", "/api/stat?path=a/b/hello.txt")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    let resp = f.app.clone().oneshot(req("GET", "/api/stat?path=x/y/bye.txt")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Mkdir (idempotent).
    for _ in 0..2 {
        let resp = f
            .app
            .clone()
            .oneshot(req_json("POST", "/api/mkdir", json!({"path": "d/e/f"})))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }
    let resp = f.app.clone().oneshot(req("GET", "/api/stat?path=d/e/f")).await.unwrap();
    let j = body_json(resp).await;
    assert_eq!(j["type"], "dir");

    // Delete file.
    let resp = f.app.clone().oneshot(req("DELETE", "/api/files/x/y/bye.txt")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let resp = f.app.clone().oneshot(req("GET", "/api/stat?path=x/y/bye.txt")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    // Delete non-empty dir without recursive → 409, with recursive → OK.
    let resp = f.app.clone().oneshot(req("DELETE", "/api/files/d")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT);
    let resp = f.app.clone().oneshot(req("DELETE", "/api/files/d?recursive=1")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Delete missing → 404.
    let resp = f.app.clone().oneshot(req("DELETE", "/api/files/nope.txt")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    let j = body_json(resp).await;
    assert_eq!(j["code"], "NOT_FOUND");
}

#[tokio::test]
async fn overwrite_publishes_updated_event() {
    let f = fixture();
    put_file(&f.app, "f.bin", b"v1").await;
    put_file(&f.app, "f.bin", b"v2-longer").await;
    let resp = f.app.clone().oneshot(req("GET", "/api/files/f.bin")).await.unwrap();
    assert_eq!(&body_bytes(resp).await[..], &b"v2-longer"[..]);
    let events = f.state.events.snapshot();
    let kinds: Vec<&str> = events.iter().map(|e| e.kind.as_str()).collect();
    assert_eq!(kinds, vec!["object.created", "object.updated"]);
}

#[tokio::test]
async fn put_publishes_created_event_in_ring_buffer() {
    let f = fixture();
    put_file(&f.app, "dir/new.txt", b"data").await;
    let events = f.state.events.snapshot();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].kind, "object.created");
    assert_eq!(events[0].path, "dir/new.txt");
    assert_eq!(events[0].size, Some(4));
}

#[tokio::test]
async fn move_publishes_from_and_to() {
    let f = fixture();
    put_file(&f.app, "m.txt", b"x").await;
    f.app
        .clone()
        .oneshot(req_json("POST", "/api/move", json!({"from": "m.txt", "to": "m2.txt"})))
        .await
        .unwrap();
    let events = f.state.events.snapshot();
    let mv = events.iter().find(|e| e.kind == "object.moved").expect("moved event");
    assert_eq!(mv.from.as_deref(), Some("m.txt"));
    assert_eq!(mv.path, "m2.txt");
}

#[tokio::test]
async fn path_traversal_is_rejected() {
    let f = fixture();
    for uri in [
        "/api/files/../evil.txt",
        "/api/files/..%2Fevil.txt",
        "/api/files/a/..%2F..%2Fevil.txt",
        "/api/files/%2e%2e/evil.txt",
    ] {
        let resp = put_file(&f.app, &uri["/api/files/".len()..], b"x").await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST, "uri {uri}");
        let j = body_json(resp).await;
        assert_eq!(j["code"], "BAD_PATH");
    }
    // Nothing escaped onto the real filesystem outside the root.
    assert!(!f._root.path().parent().unwrap().join("evil.txt").exists());
}

#[tokio::test]
async fn auth_required_and_query_token_works() {
    let f = fixture();
    put_file(&f.app, "secret.txt", b"s").await;

    // Missing token → 401.
    let r = Request::builder().uri("/api/list").body(Body::empty()).unwrap();
    let resp = f.app.clone().oneshot(r).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    let j = body_json(resp).await;
    assert_eq!(j["code"], "UNAUTHORIZED");

    // Wrong token → 401.
    let r = Request::builder()
        .uri("/api/list")
        .header(header::AUTHORIZATION, "Bearer wrong")
        .body(Body::empty())
        .unwrap();
    let resp = f.app.clone().oneshot(r).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // ?token= query param works (for <video src> / EventSource).
    let resp = f.app.clone().oneshot(
        Request::builder()
            .uri(format!("/api/files/secret.txt?token={TOKEN}"))
            .body(Body::empty())
            .unwrap(),
    ).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(&body_bytes(resp).await[..], &b"s"[..]);

    // /healthz never requires auth.
    let resp = f.app.clone().oneshot(
        Request::builder().uri("/healthz").body(Body::empty()).unwrap(),
    ).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn multipart_upload_sanitizes_filenames() {
    let f = fixture();
    let boundary = "X-BOUNDARY";
    let body = format!(
        "--{b}\r\nContent-Disposition: form-data; name=\"file1\"; filename=\"hello.txt\"\r\nContent-Type: text/plain\r\n\r\nfirst\r\n\
         --{b}\r\nContent-Disposition: form-data; name=\"file2\"; filename=\"../evil.txt\"\r\nContent-Type: text/plain\r\n\r\nsecond\r\n\
         --{b}--\r\n",
        b = boundary
    );
    let r = Request::builder()
        .method("POST")
        .uri("/api/upload?path=docs")
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
        .header(header::CONTENT_TYPE, format!("multipart/form-data; boundary={boundary}"))
        .body(Body::from(body))
        .unwrap();
    let resp = f.app.clone().oneshot(r).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let j = body_json(resp).await;
    let uploaded = j["uploaded"].as_array().unwrap();
    let paths: Vec<&str> = uploaded.iter().map(|u| u["path"].as_str().unwrap()).collect();
    assert_eq!(paths, vec!["docs/hello.txt", "docs/evil.txt"]);

    let resp = f.app.clone().oneshot(req("GET", "/api/files/docs/hello.txt")).await.unwrap();
    assert_eq!(&body_bytes(resp).await[..], &b"first"[..]);
    let resp = f.app.clone().oneshot(req("GET", "/api/files/docs/evil.txt")).await.unwrap();
    assert_eq!(&body_bytes(resp).await[..], &b"second"[..]);
    // The traversal attempt did not escape the target dir.
    assert!(!f._root.path().join("evil.txt").exists());
}

#[tokio::test]
async fn multipart_accepts_any_field_name() {
    // The xstream frontend appends parts under the field name "files"; any
    // field name must work, and plain (filename-less) fields are skipped.
    let f = fixture();
    let boundary = "B2";
    let body = format!(
        "--{b}\r\nContent-Disposition: form-data; name=\"files\"; filename=\"one.bin\"\r\nContent-Type: application/octet-stream\r\n\r\nAAA\r\n\
         --{b}\r\nContent-Disposition: form-data; name=\"files\"; filename=\"two.bin\"\r\nContent-Type: application/octet-stream\r\n\r\nBBB\r\n\
         --{b}\r\nContent-Disposition: form-data; name=\"comment\"\r\n\r\nnot a file\r\n\
         --{b}--\r\n",
        b = boundary
    );
    let r = Request::builder()
        .method("POST")
        .uri("/api/upload?path=up")
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
        .header(header::CONTENT_TYPE, format!("multipart/form-data; boundary={boundary}"))
        .body(Body::from(body))
        .unwrap();
    let resp = f.app.clone().oneshot(r).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let j = body_json(resp).await;
    let uploaded = j["uploaded"].as_array().unwrap();
    let paths: Vec<&str> = uploaded.iter().map(|u| u["path"].as_str().unwrap()).collect();
    assert_eq!(paths, vec!["up/one.bin", "up/two.bin"]);
    let resp = f.app.clone().oneshot(req("GET", "/api/files/up/two.bin")).await.unwrap();
    assert_eq!(&body_bytes(resp).await[..], &b"BBB"[..]);
}

#[tokio::test]
async fn webhooks_crud() {
    let f = fixture();
    // Create.
    let resp = f
        .app
        .clone()
        .oneshot(req_json("POST", "/api/webhooks", json!({"url": "http://127.0.0.1:9/hook", "events": ["object.created"]})))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let j = body_json(resp).await;
    let id = j["id"].as_str().unwrap().to_string();
    assert_eq!(j["url"], "http://127.0.0.1:9/hook");

    // Bad URL → 400.
    let resp = f
        .app
        .clone()
        .oneshot(req_json("POST", "/api/webhooks", json!({"url": "ftp://nope"})))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    // List.
    let resp = f.app.clone().oneshot(req("GET", "/api/webhooks")).await.unwrap();
    let j = body_json(resp).await;
    assert_eq!(j["webhooks"].as_array().unwrap().len(), 1);

    // Registration is persisted under .ostore.
    let persisted = f._root.path().join(".ostore").join("webhooks.json");
    assert!(persisted.exists());
    let on_disk: Value = serde_json::from_str(&std::fs::read_to_string(persisted).unwrap()).unwrap();
    assert_eq!(on_disk.as_array().unwrap()[0]["id"], id);

    // Delete.
    let resp = f.app.clone().oneshot(req("DELETE", &format!("/api/webhooks/{id}"))).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let resp = f.app.clone().oneshot(req("GET", "/api/webhooks")).await.unwrap();
    let j = body_json(resp).await;
    assert_eq!(j["webhooks"].as_array().unwrap().len(), 0);

    // Delete again → 404.
    let resp = f.app.clone().oneshot(req("DELETE", &format!("/api/webhooks/{id}"))).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn sse_replays_recent_events() {
    let f = fixture();
    put_file(&f.app, "watched.bin", b"payload").await;

    let resp = f
        .app
        .clone()
        .oneshot(req("GET", "/api/events"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Read a bounded amount of the body stream with a timeout; the replayed
    // object.created frame must appear.
    let mut body = resp.into_body();
    let mut acc = String::new();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    while !acc.contains("object.created") {
        let frame = tokio::time::timeout_at(deadline, body.frame())
            .await
            .expect("timed out waiting for replay frame")
            .expect("stream ended before replay")
            .expect("frame error");
        if let Some(data) = frame.data_ref() {
            acc.push_str(&String::from_utf8_lossy(data));
        }
        assert!(acc.len() < 64 * 1024, "replay frame never arrived");
    }
    assert!(acc.contains("event: object.created"));
    assert!(acc.contains("\"path\":\"watched.bin\""));
}

#[tokio::test]
async fn move_refuses_to_overwrite_nonempty_dir() {
    let f = fixture();
    put_file(&f.app, "src/a.txt", b"a").await;
    put_file(&f.app, "dst/b.txt", b"b").await;
    let resp = f
        .app
        .clone()
        .oneshot(req_json("POST", "/api/move", json!({"from": "src", "to": "dst"})))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT);
    let j = body_json(resp).await;
    assert_eq!(j["code"], "DIR_NOT_EMPTY");
}
