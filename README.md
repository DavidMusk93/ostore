# ostore

A minimal, agent-friendly object-storage daemon. ostore binds one server
directory (`OSTORE_ROOT`) and exposes S3-flavored semantics over plain HTTP:
upload to a specified path, list, stat, download (with `Range`), delete,
mkdir, move — plus an event subscription system (SSE stream + webhook
callbacks) so observers, including AI agents, can watch object mutations.

Designed as the video "network drive" backend for the xstream project, and
generally for machine→server file backup. Single static binary, no database,
no dependencies at runtime.

## Quickstart

Environment variables:

| Var | Default | Meaning |
|---|---|---|
| `OSTORE_ROOT` | (required) | Bound root directory. Created if missing; `<root>/.ostore/` meta dir is created at startup. |
| `OSTORE_TOKEN` | (required) | Bearer token for all `/api` requests. |
| `OSTORE_HOST` | `127.0.0.1` | Bind address. |
| `OSTORE_PORT` | `8781` | Bind port. |

Run from source:

```bash
export OSTORE_ROOT=/srv/ostore OSTORE_TOKEN='change-me'
cargo run --release
```

Or as a systemd service (see `deploy/ostore.service`):

```bash
cargo build --release
sudo cp deploy/ostore.service /etc/systemd/system/
sudo tee /etc/ostore.env <<'EOF'
OSTORE_ROOT=/srv/ostore
OSTORE_TOKEN=change-me
OSTORE_HOST=127.0.0.1
OSTORE_PORT=8781
EOF
sudo systemctl enable --now ostore
```

Sanity check:

```bash
curl http://127.0.0.1:8781/healthz          # {"status":"ok"} (no auth)
curl -H "Authorization: Bearer change-me" http://127.0.0.1:8781/api/help
```

## Authentication

Every `/api` endpoint requires either:

- `Authorization: Bearer <OSTORE_TOKEN>` header, or
- `?token=<token>` query parameter — needed because browser `<video src>`
  and `EventSource` cannot set headers.

Errors are always JSON: `{"error": "human message", "code": "MACHINE_CODE"}`
with an appropriate HTTP status (`400 BAD_PATH`, `401 UNAUTHORIZED`,
`404 NOT_FOUND`, `409 DIR_NOT_EMPTY`, `416 RANGE_NOT_SATISFIABLE`, …).

## API reference

All paths are relative to the server root; `""`, `/` and `.` mean the root.
Absolute paths, `..` components, NUL bytes, and the reserved `.ostore` name
are rejected with `400 BAD_PATH`.

| Method | Path | Description |
|---|---|---|
| GET | `/healthz` | Liveness probe (unauthenticated). |
| GET | `/api/help` | Machine-readable listing of all endpoints (self-discovery). |
| GET | `/api/list?path=&recursive=&checksums=` | List a directory. |
| GET | `/api/stat?path=&checksums=` | Metadata for one object. |
| PUT | `/api/files/{path}` | Raw-body streaming upload to an exact path. |
| POST | `/api/upload?path=<dir>` | multipart/form-data upload (browser FormData). |
| GET | `/api/files/{path}` | Download; supports `Range`. |
| DELETE | `/api/files/{path}?recursive=` | Delete file, or dir if empty (or `recursive=1`). |
| POST | `/api/mkdir` | `{"path"}` — mkdir -p, idempotent. |
| POST | `/api/move` | `{"from","to"}` — rename/move. |
| GET | `/api/events?since=&replay=` | SSE stream; resumable by event id (see [Events](#events)). |
| POST | `/api/webhooks` | `{"url","events"?}` — register webhook. |
| GET | `/api/webhooks` | List webhook registrations. |
| DELETE | `/api/webhooks/{id}` | Remove a registration. |
| GET | `/api/metrics` | Prometheus metrics (see [Metrics](#metrics)). |

### Examples

```bash
T=http://127.0.0.1:8781
AUTH="Authorization: Bearer change-me"

# Upload a file to an exact path (parents auto-created, atomic tmp+rename,
# overwrite allowed). Response: {"path","size","sha256"}
curl -X PUT -H "$AUTH" --data-binary @video.mp4 "$T/api/files/videos/2026/video.mp4"

# List (dirs first, then files, sorted by name; recursive + sha256 checksums)
curl -H "$AUTH" "$T/api/list?path=videos&recursive=1&checksums=1"
# {"path":"videos","entries":[{"name":"2026","path":"videos/2026","type":"dir",
#   "size":0,"mtime":"2026-10-23T10:00:00Z"}, ..., {"name":"video.mp4",...,
#   "type":"file","size":12345,"mtime":"...","sha256":"..."}]}

# Stat one object (checksums=1 adds sha256 for files)
curl -H "$AUTH" "$T/api/stat?path=videos/2026/video.mp4&checksums=1"

# Download, full or ranged (206 + Content-Range; 416 + 'Content-Range: bytes */SIZE' if invalid)
curl -H "$AUTH" -o video.mp4 "$T/api/files/videos/2026/video.mp4"
curl -H "$AUTH" -H "Range: bytes=0-1048575" -o part.bin "$T/api/files/videos/2026/video.mp4"
curl -H "$AUTH" -H "Range: bytes=-1048576" -o tail.bin "$T/api/files/videos/2026/video.mp4"

# Multipart upload (browser FormData); any form field name works, each
# field's filename (directory components stripped) becomes the object name.
curl -X POST -H "$AUTH" -F "files=@a.jpg" -F "files=@b.jpg" "$T/api/upload?path=photos"
# {"uploaded":[{"path":"photos/a.jpg","size":...},{"path":"photos/b.jpg","size":...}]}

# mkdir -p (idempotent)
curl -X POST -H "$AUTH" -H 'Content-Type: application/json' \
  -d '{"path":"backups/host1"}' "$T/api/mkdir"

# Move/rename (destination parents auto-created; refuses to overwrite a
# non-empty directory; overwriting a file is fine)
curl -X POST -H "$AUTH" -H 'Content-Type: application/json' \
  -d '{"from":"photos/a.jpg","to":"archive/a.jpg"}' "$T/api/move"

# Delete (dir must be empty unless recursive=1)
curl -X DELETE -H "$AUTH" "$T/api/files/archive/a.jpg"
curl -X DELETE -H "$AUTH" "$T/api/files/archive?recursive=1"

# Watch events over SSE (replays last 100 events, then streams live);
# resume after a disconnect with ?since=<last id seen>, live-only with ?replay=0
curl -N -H "$AUTH" "$T/api/events"
curl -N -H "$AUTH" "$T/api/events?since=1792807221000123"
# ... or from a browser where headers are impossible:
#   new EventSource('/api/events?token=change-me')

# Webhooks: register / list / delete (persisted in <root>/.ostore/webhooks.json)
curl -X POST -H "$AUTH" -H 'Content-Type: application/json' \
  -d '{"url":"https://example.com/hook","events":["object.created","object.deleted"]}' \
  "$T/api/webhooks"
curl -H "$AUTH" "$T/api/webhooks"
curl -X DELETE -H "$AUTH" "$T/api/webhooks/<id>"
```

## Backup semantics

`GET /api/list?recursive=1&checksums=1` gives every file with its sha256, so a
client can mirror a local directory up with three operations: PUT changed
files, DELETE extra remote files. Uploads are atomic (`.ostore-tmp-<pid>` +
rename), so a reader never sees a partial object.

```bash
#!/usr/bin/env bash
# mirror-up.sh SRC_DIR REMOTE_PREFIX — push-based mirror to ostore.
set -euo pipefail
SRC=$1; PREFIX=$2
T=http://127.0.0.1:8781; TOKEN=change-me

# Remote state: path <TAB> sha256
curl -sf -H "Authorization: Bearer $TOKEN" \
  "$T/api/list?path=$PREFIX&recursive=1&checksums=1" |
  jq -r '.entries[] | select(.type=="file") | [.path, .sha256] | @tsv' |
  sort > /tmp/remote.tsv

# Local state.
( cd "$SRC" && find . -type f -print0 |
  while IFS= read -r -d '' f; do
    f=${f#./}
    printf '%s/%s\t%s\n' "$PREFIX" "$f" "$(sha256sum "$f" | cut -d' ' -f1)"
  done ) | sort > /tmp/local.tsv

# Upload new or changed files.
join -t $'\t' -v 1 <(cut -f1 /tmp/local.tsv) <(cut -f1 /tmp/remote.tsv) |
while IFS= read -r p; do
  rel=${p#"$PREFIX"/}
  curl -sf -X PUT -H "Authorization: Bearer $TOKEN" \
    --data-binary @"$SRC/$rel" "$T/api/files/$p"
done
# Changed content, same path:
join -t $'\t' /tmp/local.tsv /tmp/remote.tsv |
  awk -F'\t' '$2 != $3 {print $1}' |
while IFS= read -r p; do
  curl -sf -X PUT -H "Authorization: Bearer $TOKEN" \
    --data-binary @"$SRC/${p#"$PREFIX"/}" "$T/api/files/$p"
done

# Delete remote files that no longer exist locally.
join -t $'\t' -v 2 <(cut -f1 /tmp/local.tsv) <(cut -f1 /tmp/remote.tsv) |
while IFS= read -r p; do
  curl -sf -X DELETE -H "Authorization: Bearer $TOKEN" "$T/api/files/$p"
done
```

## Events

Every mutation publishes an event, delivered two ways: the SSE stream
(`GET /api/events`) and registered webhooks.

| Type | Fired when | Extra fields |
|---|---|---|
| `object.created` | New object uploaded | `size` |
| `object.updated` | Existing object overwritten | `size` |
| `object.deleted` | File or directory deleted | — |
| `object.moved` | Object renamed/moved | `from`, `size` (files) |
| `object.mkdir` | Directory created by `/api/mkdir` | — |

Every event has a monotonically increasing `id` (seeded from the boot time
in microseconds, so ids keep increasing across restarts) and a `type`
(`file` / `dir`) describing the object. SSE frame:

```
id: 1792807221000123
event: object.created
data: {"id":1792807221000123,"path":"videos/2026/video.mp4","type":"file","size":12345,"ts":"2026-10-23T10:00:00Z"}
```

Cursor rules for `GET /api/events` (the last 1024 events are retained):

| Request | Replay before live events |
|---|---|
| `?since=<id>` or `Last-Event-ID: <id>` | Exactly the events after `<id>` |
| no cursor | The last `?replay=N` events (default 100; `0` = live only) |
| cursor older than the ring, or from a previous process | One `stream.reset` frame |

Browsers send `Last-Event-ID` automatically when `EventSource` reconnects, so
a reconnect never re-delivers old events. On `stream.reset` (also sent when
a subscriber falls behind the live broadcast) a client must re-list what it
mirrors, then keep consuming. Streams end cleanly on SIGTERM.

Webhook delivery: `POST` JSON to the registered URL with the SSE `data`
payload plus an `"event"` field:

```json
{"event":"object.moved","path":"archive/a.jpg","from":"photos/a.jpg","size":123,"ts":"..."}
```

Delivery runs on background tasks (at most 32 in flight, so one slow
target can't delay others or pile up unbounded) and never blocks API
handlers. On failure
it retries 3 times with backoff (1s, 5s, 15s), then drops that delivery — the
registration itself is kept. Registrations persist across restarts in
`<root>/.ostore/webhooks.json`.

## Metrics

`GET /api/metrics` (bearer auth like every `/api` route) returns Prometheus
text format. Scrape config: `authorization: {credentials: <token>}`.

| Metric | Type | Meaning |
|---|---|---|
| `ostore_http_requests_total{route,method,status}` | counter | Requests by matched route template |
| `ostore_http_request_duration_seconds{route,method}` | histogram | Time to response headers |
| `ostore_http_requests_in_flight` | gauge | Requests not yet answered |
| `ostore_upload_bytes_total` / `ostore_download_bytes_total` | counter | Bytes written / streamed |
| `ostore_uploads_in_flight` | gauge | Objects being written right now |
| `ostore_objects_written_total` / `ostore_upload_failures_total` | counter | Commits / aborted uploads |
| `ostore_events_published_total{kind}` | counter | Events by kind |
| `ostore_event_last_id`, `ostore_event_ring_len` | gauge | Event cursor head / retained events |
| `ostore_sse_subscribers` | gauge | Open SSE streams |
| `ostore_sse_resets_total` | counter | `stream.reset` frames (gaps, lagging clients) |
| `ostore_webhook_deliveries_total{result=ok\|retry\|dropped}` | counter | Webhook outcomes |
| `ostore_webhook_deliveries_in_flight`, `ostore_webhooks_registered` | gauge | Webhook load |
| `ostore_root_disk_total_bytes` / `ostore_root_disk_available_bytes` | gauge | Space under `OSTORE_ROOT` |
| `ostore_tmp_files_swept_total` | counter | Crash leftovers removed at startup |

## Durability

Uploads stream into a unique sibling temp file
(`<name>.ostore-tmp-<pid>-<seq>`), are fsync'd, then atomically renamed, so
readers only ever see complete objects and a committed object survives a
crash. Temp files orphaned by a crash are swept at startup. Multipart
uploads have no body size limit (videos are multi-GB); downloads carry a
weak `ETag` (size + mtime) and answer `If-None-Match` with `304`.

## Security notes

- **Bearer token** guards every `/api` endpoint; keep `OSTORE_TOKEN` secret
  and serve behind TLS (e.g. Caddy) when exposed beyond localhost.
- **`?token=` caveat**: the query-token variant exists so browser
  `<video src>` / `EventSource` work, but URLs leak into access logs, browser
  history and proxies. Prefer the header wherever you can set it, and treat
  the token as compromised if query-token URLs are widely logged.
- **Path confinement**: absolute paths, `..` components, NUL bytes and the
  reserved `.ostore` meta dir are rejected; every resolved path is verified
  to stay under `OSTORE_ROOT`. The `.ostore` meta dir and in-flight
  `.ostore-tmp-*` upload files are hidden from listings and unreachable
  through the API.
- The daemon binds `127.0.0.1` by default — expose it only through a
  TLS-terminating reverse proxy.

## Development

```bash
cargo test            # unit (paths, range) + integration (in-process router)
cargo build --release # binary at target/release/ostore
```

Integration tests mount `build_router(state)` on a `tempfile::TempDir` and
drive it with `tower::ServiceExt::oneshot` — no port needed.
