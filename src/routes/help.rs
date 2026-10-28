//! GET /api/help — machine-readable API listing for agent self-discovery.

use axum::Json;
use serde_json::{Value, json};

pub async fn help() -> Json<Value> {
    Json(json!({
        "service": "ostore",
        "description": "Minimal object-storage daemon over HTTP. All paths are relative to the server root. Auth: 'Authorization: Bearer <token>' header or '?token=<token>' query parameter on every /api endpoint.",
        "errors": {"shape": {"error": "human message", "code": "MACHINE_CODE"}},
        "events": ["object.created", "object.updated", "object.deleted", "object.moved", "object.mkdir"],
        "control_events": {"stream.reset": "SSE only: your cursor can't be honored (gap or lag) - re-list, then continue."},
        "endpoints": [
            {"method": "GET", "path": "/healthz", "auth": false, "description": "Liveness probe."},
            {"method": "GET", "path": "/api/help", "description": "This document."},
            {"method": "GET", "path": "/api/list", "params": {"path": "dir path (default root)", "recursive": "1/true to recurse", "checksums": "1/true to include sha256 for files"}, "description": "List a directory. Dirs first, then files, sorted by name."},
            {"method": "GET", "path": "/api/stat", "params": {"path": "object path", "checksums": "1/true to include sha256"}, "description": "Metadata for one object."},
            {"method": "PUT", "path": "/api/files/{path}", "body": "raw bytes", "description": "Stream-upload to an exact path. Parents auto-created; overwrites. Returns {path,size,sha256}."},
            {"method": "POST", "path": "/api/upload", "params": {"path": "target dir (default root)"}, "body": "multipart/form-data", "description": "Browser FormData upload; each file field's filename becomes the object name."},
            {"method": "GET", "path": "/api/files/{path}", "headers": {"Range": "bytes=start-end | start- | -suffix"}, "description": "Download. 206 + Content-Range on ranges; 416 if unsatisfiable. Weak ETag; If-None-Match -> 304."},
            {"method": "GET", "path": "/api/thumb/{path}", "params": {"w": "box size, snaps to 128/256/512/1024 (default 256)"}, "description": "JPEG thumbnail of an image (jpg/png/gif/webp/bmp): fitted, never cropped, EXIF-rotated, cached. 415 NOT_AN_IMAGE for other types."},
            {"method": "DELETE", "path": "/api/files/{path}", "params": {"recursive": "1/true to delete non-empty dirs"}, "description": "Delete a file, or a dir only if empty unless recursive."},
            {"method": "POST", "path": "/api/mkdir", "body": {"path": "dir path"}, "description": "mkdir -p (idempotent)."},
            {"method": "POST", "path": "/api/move", "body": {"from": "source", "to": "destination"}, "description": "Rename/move. Destination parents auto-created; refuses to overwrite a non-empty dir."},
            {"method": "GET", "path": "/api/events", "params": {"since": "resume after this event id", "replay": "events to replay when no cursor (default 100, 0 = live only)"}, "headers": {"Last-Event-ID": "same as since; browsers send it on reconnect"}, "description": "SSE stream. Frame: 'id: <seq>' + 'event: <type>' + 'data: {id,path,type(file|dir),size?,from?,ts}'. Ids are monotonic across restarts; a stale cursor yields one 'stream.reset' frame."},
            {"method": "POST", "path": "/api/webhooks", "body": {"url": "http(s) callback", "events": "optional subset of event types (default all)"}, "description": "Register a webhook. Delivery: POST JSON {event,path,size?,from?,ts}, 3 retries (1s/5s/15s), then dropped."},
            {"method": "GET", "path": "/api/webhooks", "description": "List webhook registrations."},
            {"method": "DELETE", "path": "/api/webhooks/{id}", "description": "Remove a webhook registration."},
            {"method": "GET", "path": "/api/metrics", "description": "Prometheus text exposition: HTTP requests/latency by route, upload/download bytes, uploads in flight, events by kind, SSE subscribers/resets, webhook outcomes, root disk space, S3-surface counters."},
            {"method": "GET|HEAD", "path": "/{bucket}/{key}", "auth": "aws-sigv4", "headers": {"Range": "bytes=start-end", "If-None-Match": "ETag"}, "description": "S3-compatible object read (path-style). 206 + Content-Range for any Range; 416 + 'Content-Range: bytes */SIZE' when unsatisfiable; strong ETag; errors are S3 XML. Read-only: uploads go through /api. Enabled only when OSTORE_S3_ACCESS_KEY/SECRET are set."}
        ]
    }))
}
