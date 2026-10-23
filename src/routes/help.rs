//! GET /api/help — machine-readable API listing for agent self-discovery.

use axum::Json;
use serde_json::{Value, json};

pub async fn help() -> Json<Value> {
    Json(json!({
        "service": "ostore",
        "description": "Minimal object-storage daemon over HTTP. All paths are relative to the server root. Auth: 'Authorization: Bearer <token>' header or '?token=<token>' query parameter on every /api endpoint.",
        "errors": {"shape": {"error": "human message", "code": "MACHINE_CODE"}},
        "events": ["object.created", "object.updated", "object.deleted", "object.moved", "object.mkdir"],
        "endpoints": [
            {"method": "GET", "path": "/healthz", "auth": false, "description": "Liveness probe."},
            {"method": "GET", "path": "/api/help", "description": "This document."},
            {"method": "GET", "path": "/api/list", "params": {"path": "dir path (default root)", "recursive": "1/true to recurse", "checksums": "1/true to include sha256 for files"}, "description": "List a directory. Dirs first, then files, sorted by name."},
            {"method": "GET", "path": "/api/stat", "params": {"path": "object path", "checksums": "1/true to include sha256"}, "description": "Metadata for one object."},
            {"method": "PUT", "path": "/api/files/{path}", "body": "raw bytes", "description": "Stream-upload to an exact path. Parents auto-created; overwrites. Returns {path,size,sha256}."},
            {"method": "POST", "path": "/api/upload", "params": {"path": "target dir (default root)"}, "body": "multipart/form-data", "description": "Browser FormData upload; each file field's filename becomes the object name."},
            {"method": "GET", "path": "/api/files/{path}", "headers": {"Range": "bytes=start-end | start- | -suffix"}, "description": "Download. 206 + Content-Range on ranges; 416 if unsatisfiable."},
            {"method": "DELETE", "path": "/api/files/{path}", "params": {"recursive": "1/true to delete non-empty dirs"}, "description": "Delete a file, or a dir only if empty unless recursive."},
            {"method": "POST", "path": "/api/mkdir", "body": {"path": "dir path"}, "description": "mkdir -p (idempotent)."},
            {"method": "POST", "path": "/api/move", "body": {"from": "source", "to": "destination"}, "description": "Rename/move. Destination parents auto-created; refuses to overwrite a non-empty dir."},
            {"method": "GET", "path": "/api/events", "description": "SSE stream. Replays the last 100 events, then live. Frame: 'event: <type>' + 'data: {path,size?,from?,ts}'."},
            {"method": "POST", "path": "/api/webhooks", "body": {"url": "http(s) callback", "events": "optional subset of event types (default all)"}, "description": "Register a webhook. Delivery: POST JSON {event,path,size?,from?,ts}, 3 retries (1s/5s/15s), then dropped."},
            {"method": "GET", "path": "/api/webhooks", "description": "List webhook registrations."},
            {"method": "DELETE", "path": "/api/webhooks/{id}", "description": "Remove a webhook registration."}
        ]
    }))
}
