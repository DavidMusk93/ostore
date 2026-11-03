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
        "text/plain; charset=utf-8",
        "mime_guess should detect .txt with utf-8 charset"
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

/// Read SSE frames until `pred` matches the accumulated text (5s cap).
async fn read_sse_until(resp: axum::response::Response, pred: impl Fn(&str) -> bool) -> String {
    let mut body = resp.into_body();
    let mut acc = String::new();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    while !pred(&acc) {
        let frame = tokio::time::timeout_at(deadline, body.frame())
            .await
            .expect("timed out waiting for SSE frame")
            .expect("stream ended")
            .expect("frame error");
        if let Some(data) = frame.data_ref() {
            acc.push_str(&String::from_utf8_lossy(data));
        }
    }
    acc
}

#[tokio::test]
async fn multipart_accepts_bodies_over_2mb() {
    let f = fixture();
    let boundary = "X-BOUNDARY";
    let payload = vec![b'v'; 3 * 1024 * 1024];
    let mut body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"files\"; filename=\"big.mp4\"\r\nContent-Type: video/mp4\r\n\r\n"
    )
    .into_bytes();
    body.extend_from_slice(&payload);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    let r = Request::builder()
        .method("POST")
        .uri("/api/upload?path=v")
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
        .header(header::CONTENT_TYPE, format!("multipart/form-data; boundary={boundary}"))
        .body(Body::from(body))
        .unwrap();
    let resp = f.app.clone().oneshot(r).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let j = body_json(resp).await;
    assert_eq!(j["uploaded"][0]["size"], payload.len());
    // No temp file left behind after commit.
    let names: Vec<String> = std::fs::read_dir(f._root.path().join("v"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, vec!["big.mp4".to_string()]);
}

#[tokio::test]
async fn events_carry_ids_and_type() {
    let f = fixture();
    put_file(&f.app, "d/a.bin", b"a").await;
    f.app.clone().oneshot(req_json("POST", "/api/mkdir", json!({"path": "e"}))).await.unwrap();
    let evs = f.state.events.snapshot();
    assert_eq!(evs.len(), 2);
    assert!(evs[0].id < evs[1].id);
    assert_eq!(evs[0].entry_type, "file");
    assert_eq!(evs[1].entry_type, "dir");
}

#[tokio::test]
async fn sse_resumes_after_cursor() {
    let f = fixture();
    put_file(&f.app, "one.bin", b"1").await;
    put_file(&f.app, "two.bin", b"2").await;
    let first = f.state.events.snapshot()[0].id;

    let resp = f.app.clone().oneshot(req("GET", &format!("/api/events?since={first}"))).await.unwrap();
    let acc = read_sse_until(resp, |a| a.contains("two.bin")).await;
    assert!(!acc.contains("one.bin"), "event before the cursor was replayed");

    // Browser reconnect path: Last-Event-ID header.
    let r = Request::builder()
        .uri("/api/events")
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
        .header("Last-Event-ID", first.to_string())
        .body(Body::empty())
        .unwrap();
    let acc = read_sse_until(f.app.clone().oneshot(r).await.unwrap(), |a| a.contains("two.bin")).await;
    assert!(!acc.contains("one.bin"));
    assert!(acc.contains(&format!("id: {}", first + 1)));
}

#[tokio::test]
async fn sse_stale_cursor_sends_reset() {
    let f = fixture();
    put_file(&f.app, "x.bin", b"x").await;
    let resp = f.app.clone().oneshot(req("GET", "/api/events?since=5")).await.unwrap();
    let acc = read_sse_until(resp, |a| a.contains("stream.reset")).await;
    assert!(!acc.contains("x.bin"));
}

#[tokio::test]
async fn sse_ends_on_shutdown() {
    let f = fixture();
    let resp = f.app.clone().oneshot(req("GET", "/api/events?replay=0")).await.unwrap();
    f.state.shutdown.cancel();
    let collected = tokio::time::timeout(std::time::Duration::from_secs(5), resp.into_body().collect()).await;
    assert!(collected.is_ok(), "SSE stream did not end after shutdown");
}

#[tokio::test]
async fn download_etag_revalidates() {
    let f = fixture();
    put_file(&f.app, "c.bin", b"cached").await;
    let resp = f.app.clone().oneshot(req("GET", "/api/files/c.bin")).await.unwrap();
    let etag = resp.headers()[header::ETAG].to_str().unwrap().to_string();
    let r = Request::builder()
        .uri("/api/files/c.bin")
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
        .header(header::IF_NONE_MATCH, &etag)
        .body(Body::empty())
        .unwrap();
    let resp = f.app.clone().oneshot(r).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_MODIFIED);
}

#[tokio::test]
async fn metrics_exposition() {
    let f = fixture();
    put_file(&f.app, "m.bin", b"12345").await;
    let resp = f.app.clone().oneshot(req("GET", "/api/files/m.bin")).await.unwrap();
    body_bytes(resp).await;
    let resp = f.app.clone().oneshot(Request::builder().uri("/api/metrics").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    let resp = f.app.clone().oneshot(req("GET", "/api/metrics")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let text = String::from_utf8(body_bytes(resp).await.to_vec()).unwrap();
    assert!(text.contains("ostore_upload_bytes_total 5"), "{text}");
    assert!(text.contains("ostore_download_bytes_total 5"));
    assert!(text.contains("ostore_objects_written_total 1"));
    assert!(text.contains("ostore_events_published_total{kind=\"object.created\"} 1"));
    assert!(text.contains("route=\"/api/files/{*path}\",method=\"PUT\",status=\"200\""), "{text}");
    assert!(text.contains("route=\"/api/metrics\",method=\"GET\",status=\"401\""));
    assert!(text.contains("ostore_root_disk_available_bytes"));
}

#[test]
fn sweep_removes_stale_tmp_files() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("a/b")).unwrap();
    std::fs::write(root.path().join("a/b/v.mp4.ostore-tmp-1-0"), b"x").unwrap();
    std::fs::write(root.path().join("a/keep.mp4"), b"x").unwrap();
    assert_eq!(ostore::sweep_stale_tmp(root.path()), 1);
    assert!(root.path().join("a/keep.mp4").exists());
}

fn png_bytes(w: u32, h: u32) -> Vec<u8> {
    let img = image::RgbaImage::from_fn(w, h, |x, _| image::Rgba([(x % 255) as u8, 80, 160, if x < w / 2 { 255 } else { 0 }]));
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

#[tokio::test]
async fn thumbnail_fits_box_and_caches() {
    let f = fixture();
    put_file(&f.app, "pics/wide.png", &png_bytes(400, 200)).await;

    let resp = f.app.clone().oneshot(req("GET", "/api/thumb/pics/wide.png?w=100")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.headers()[header::CONTENT_TYPE], "image/jpeg");
    let etag = resp.headers()[header::ETAG].to_str().unwrap().to_string();
    let bytes = body_bytes(resp).await;
    let thumb = image::load_from_memory(&bytes).unwrap();
    // w=100 snaps to 128; 2:1 aspect preserved, never cropped.
    assert_eq!((thumb.width(), thumb.height()), (128, 64));

    let resp = f.app.clone().oneshot(req("GET", "/api/thumb/pics/wide.png?w=128")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(f.state.metrics.thumbs_hit.load(std::sync::atomic::Ordering::Relaxed), 1);
    assert_eq!(f.state.metrics.thumbs_generated.load(std::sync::atomic::Ordering::Relaxed), 1);

    let r = Request::builder()
        .uri("/api/thumb/pics/wide.png?w=128")
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
        .header(header::IF_NONE_MATCH, &etag)
        .body(Body::empty())
        .unwrap();
    assert_eq!(f.app.clone().oneshot(r).await.unwrap().status(), StatusCode::NOT_MODIFIED);

    // Cache lives in the hidden meta dir, never in listings.
    let resp = f.app.clone().oneshot(req("GET", "/api/list?recursive=1")).await.unwrap();
    let j = body_json(resp).await;
    assert!(j["entries"].as_array().unwrap().iter().all(|e| !e["path"].as_str().unwrap().contains("thumbs")));
}

#[tokio::test]
async fn thumbnail_rejects_non_images() {
    let f = fixture();
    put_file(&f.app, "notes.txt", b"hello").await;
    let resp = f.app.clone().oneshot(req("GET", "/api/thumb/notes.txt")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    put_file(&f.app, "fake.jpg", b"not really a jpeg").await;
    let resp = f.app.clone().oneshot(req("GET", "/api/thumb/fake.jpg")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn ui_serves_html_at_root_and_ostore() {
    let f = fixture();
    let resp = f.app.clone().oneshot(Request::builder().uri("/").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = body_bytes(resp).await;
    let html = String::from_utf8_lossy(&bytes);
    assert!(html.contains("ostore"));
    assert!(html.contains("Distributed Object Storage"));
    assert!(html.contains("getFileCategory"));
    assert!(html.contains("CADDYFILE"));
    assert!(html.contains("mediaDownloadBtn"));

    let resp_ostore = f.app.clone().oneshot(Request::builder().uri("/ostore").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(resp_ostore.status(), StatusCode::OK);

    let resp_ostore_slash = f.app.clone().oneshot(Request::builder().uri("/ostore/").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(resp_ostore_slash.status(), StatusCode::OK);
}

#[tokio::test]
async fn cluster_nodes_endpoint_standalone() {
    let f = fixture();
    let resp = f.app.clone().oneshot(req("GET", "/api/cluster/nodes")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let j = body_json(resp).await;
    let nodes = j["nodes"].as_array().unwrap();
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0]["is_local"], true);
    assert_eq!(nodes[0]["healthy"], true);
}

#[tokio::test]
async fn cluster_topology_endpoint_standalone() {
    let f = fixture();
    let resp = f.app.clone().oneshot(req("GET", "/api/cluster/topology")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let j = body_json(resp).await;
    assert!(j["control_plane"].is_object());
    assert_eq!(j["control_plane"]["id"], "local");
}

#[tokio::test]
async fn cluster_file_status_endpoint_standalone() {
    let f = fixture();
    put_file(&f.app, "doc.txt", b"ostore replica test").await;
    let resp = f.app.clone().oneshot(req("GET", "/api/cluster/file-status?path=doc.txt")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let j = body_json(resp).await;
    assert_eq!(j["fully_synced"], true);
    assert_eq!(j["main"]["exists"], true);
    assert!(j["metrics"]["total_ms"].is_number());
}

#[tokio::test]
async fn chinese_utf8_content_type_preserved() {
    let f = fixture();
    let chinese_text = "分布式存储 中文测试 内容完整性 2026";
    put_file(&f.app, "chinese.txt", chinese_text.as_bytes()).await;
    let resp = f.app.clone().oneshot(req("GET", "/api/files/chinese.txt")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let ct = resp.headers()[header::CONTENT_TYPE].to_str().unwrap();
    assert!(ct.contains("charset=utf-8"));
    let bytes = body_bytes(resp).await;
    let s = String::from_utf8(bytes.to_vec()).unwrap();
    assert_eq!(s, chinese_text);
}

#[tokio::test]
async fn directory_shows_aggregated_size_and_pagination() {
    let f = fixture();
    put_file(&f.app, "folder/file1.bin", &[1u8; 100]).await;
    put_file(&f.app, "folder/file2.bin", &[2u8; 250]).await;

    // Check directory size in list
    let resp = f.app.clone().oneshot(req("GET", "/api/list")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let j = body_json(resp).await;
    let entries = j["entries"].as_array().unwrap();
    let folder_entry = entries.iter().find(|e| e["name"] == "folder").unwrap();
    assert_eq!(folder_entry["type"], "dir");
    assert_eq!(folder_entry["size"], 350); // Aggregated 100 + 250 bytes
    assert_eq!(folder_entry["item_count"], 2); // Aggregated total items count

    // Check pagination
    let resp_page = f.app.clone().oneshot(req("GET", "/api/list?path=folder&limit=1&offset=0")).await.unwrap();
    assert_eq!(resp_page.status(), StatusCode::OK);
    let j_page = body_json(resp_page).await;
    assert_eq!(j_page["total_count"], 2);
    assert_eq!(j_page["total_bytes"], 350);
    assert_eq!(j_page["has_more"], true);
    assert_eq!(j_page["entries"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn download_directory_archive_tar_streaming() {
    let f = fixture();
    put_file(&f.app, "archive_test/sub/f1.txt", b"hello tar archive").await;
    let resp = f.app.clone().oneshot(req("GET", "/api/archive?path=archive_test")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.headers()[header::CONTENT_TYPE], "application/x-tar");
    let bytes = body_bytes(resp).await;
    assert!(bytes.len() > 512);
    // Posix ustar header magic check
    let s = String::from_utf8_lossy(&bytes[..512]);
    assert!(s.contains("ustar") || bytes.windows(5).any(|w| w == b"ustar"));
}

#[tokio::test]
async fn totp_authenticator_login_and_session_access() {
    let f = fixture();

    // 1. Unauthenticated request to /api/list without token should yield 401
    let unauth_req = Request::builder().uri("/api/list").body(Body::empty()).unwrap();
    let resp = f.app.clone().oneshot(unauth_req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // 2. Query /api/auth/setup to get secret
    let setup_req = Request::builder().uri("/api/auth/setup").body(Body::empty()).unwrap();
    let resp = f.app.clone().oneshot(setup_req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let j = body_json(resp).await;
    assert!(j["secret"].as_str().is_some());
    assert!(j["otpauth_uri"].as_str().unwrap().starts_with("otpauth://totp/"));

    // 3. Compute valid current OTP using the state's secret
    let duration = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap();
    let step = duration.as_secs() / 30;
    let secret_bytes = ostore::auth_otp::base32_decode(f.state.totp.get_secret_base32()).unwrap();
    use hmac::{Hmac, Mac};
    use sha1::Sha1;
    let mut mac = Hmac::<Sha1>::new_from_slice(&secret_bytes).unwrap();
    mac.update(&step.to_be_bytes());
    let h = mac.finalize().into_bytes();
    let offset = (h[19] & 0x0f) as usize;
    let code_u32 = (((h[offset] & 0x7f) as u32) << 24)
        | ((h[offset + 1] as u32) << 16)
        | ((h[offset + 2] as u32) << 8)
        | (h[offset + 3] as u32);
    let valid_otp = format!("{:06}", code_u32 % 1_000_000);

    // 4. Try invalid code -> 401
    let bad_login = Request::builder()
        .method("POST")
        .uri("/api/auth/login")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"code":"000000"}"#))
        .unwrap();
    let resp = f.app.clone().oneshot(bad_login).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // 5. Submit valid code -> 200 and receive session
    let good_login = Request::builder()
        .method("POST")
        .uri("/api/auth/login")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(format!(r#"{{"code":"{valid_otp}"}}"#)))
        .unwrap();
    let resp = f.app.clone().oneshot(good_login).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let set_cookie = resp.headers().get(header::SET_COOKIE).unwrap().to_str().unwrap().to_string();
    assert!(set_cookie.contains("ostore_sess="));
    let j = body_json(resp).await;
    let sess_token = j["token"].as_str().unwrap().to_string();
    assert!(sess_token.starts_with("sess_"));

    // 6. Access protected API using session token Bearer
    let sess_req = Request::builder()
        .uri("/api/list")
        .header(header::AUTHORIZATION, format!("Bearer {sess_token}"))
        .body(Body::empty())
        .unwrap();
    let resp = f.app.clone().oneshot(sess_req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn ostore_prefix_routes_api() {
    let f = fixture();
    put_file(&f.app, "doc.txt", b"ostore path test").await;
    let resp = f.app.clone().oneshot(req("GET", "/ostore/api/list")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let j = body_json(resp).await;
    assert_eq!(j["entries"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn caddyfile_and_timestamped_backup_content_type() {
    let f = fixture();
    let caddy_data = b":8080 {\n  respond \"hello\"\n}\n";
    put_file(&f.app, "backups/caddy/Caddyfile.20261102-214513", caddy_data).await;
    put_file(&f.app, "Caddyfile", caddy_data).await;

    let resp = f
        .app
        .clone()
        .oneshot(req("GET", "/api/files/backups/caddy/Caddyfile.20261102-214513"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.headers()["content-type"], "text/plain; charset=utf-8");

    let resp = f
        .app
        .clone()
        .oneshot(req("GET", "/api/files/Caddyfile"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.headers()["content-type"], "text/plain; charset=utf-8");
}


