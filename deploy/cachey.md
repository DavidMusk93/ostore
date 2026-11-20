# Deploying ostore with cachey in front

Two processes, one shared dataset:

```
client ──HTTP──> cachey :8800  ──S3 (SigV4)──> ostore :8799 ──> OSTORE_ROOT
                  (16 MiB pages,                (reads straight
                   memory + disk cache)          off disk)
```

cachey needs ostore's **S3 read surface** enabled; the JSON API cannot be used
because cachey talks SigV4 S3 and requires `Content-Range` on every read.

## 1. ostore

```bash
export OSTORE_ROOT=/srv/ostore
export OSTORE_TOKEN=...                       # JSON API / uploads
export OSTORE_HOST=127.0.0.1
export OSTORE_PORT=8799

# Enables the S3 read surface for cachey only.
export OSTORE_S3_ACCESS_KEY=cachey
export OSTORE_S3_SECRET_KEY=$(openssl rand -hex 24)
export OSTORE_S3_REGION=us-east-1
export OSTORE_S3_BUCKET=media                 # must match cachey's C0-Bucket
```

Keep `OSTORE_S3_*` out of anything that reaches a browser or a media player;
they grant read access to every object. Clients keep using `OSTORE_TOKEN`
against `/api/files/...`.

## 2. cachey

cachey reads its S3 backend from the standard AWS environment, so the endpoint
and credentials come from the environment rather than flags:

```bash
export AWS_ACCESS_KEY_ID=cachey
export AWS_SECRET_ACCESS_KEY=<same secret as OSTORE_S3_SECRET_KEY>
export AWS_REGION=us-east-1
export AWS_DEFAULT_REGION=us-east-1
export AWS_ENDPOINT_URL=http://127.0.0.1:8799
export AWS_EC2_METADATA_DISABLED=true         # avoid IMDS probes on non-EC2 hosts

cachey-server \
  --memory 4GiB \
  --disk-path /var/cache/cachey \
  --disk-capacity 200GiB \
  --port 8800
```

Reads then go to `http://cachey:8800/fetch/<kind>/<key>` with:

```
Range: bytes=<first>-<last>      (required)
C0-Bucket: media                 (repeat the header for redundant buckets;
                                  a single bucket here is the normal case)
```

Path-style addressing is the default for a custom `AWS_ENDPOINT_URL`, so no
extra header is needed for ostore. `C0-Config: fps=true` overrides it per
request if an environment ever forces virtual-host addressing.

`<kind>` is an arbitrary namespace label and the first path segment of the
cache key, so reusing one `kind` per dataset keeps the cache coherent.

Cachey sends `Range` for every read and pages objects into 16 MiB entries, so
large media is fetched in page-sized chunks after the first miss.

## 3. Verification

```bash
# Real AWS SDK against ostore's S3 surface (needs `pip install boto3`).
python3 scripts/s3_smoke.py http://127.0.0.1:8799

# cachey end to end: cold miss, warm hit, byte-for-byte match, 416 propagation.
scripts/cachey_check.sh http://127.0.0.1:8800 http://127.0.0.1:8799 media
```

## 4. The staleness contract

cachey assumes immutable objects. ostore lets you overwrite, move and delete
through the JSON API, and cachey's cache key carries no version, so **a
mutation is invisible through cachey until the entry is evicted** (memory
pressure, disk capacity pressure, or a restart with the disk cache dropped).

Practical rules:

- Front cachey **only** for append-only or write-once trees — recordings,
  backups, build artifacts. Route mutable trees straight to ostore.
- When an object must change, write it to a **new key** (`clip.mp4` →
  `clip.v2.mp4`) and let the old one age out. This is the same discipline S3
  itself makes cheap.
- If you need overwrite visibility, invalidate at the cachey layer rather than
  expecting the ETag to help: cachey has no purge endpoint, so that means
  dropping its disk cache and restarting, or patching cachey to accept an
  invalidation API.
- Do not put cachey in front of trees that `ostore` clients delete and
  recreate under the same path; a recreated object of the same size and mtime
  granularity would look like the same cache entry.

`scripts/cachey_check.sh` asserts the stale-read behavior explicitly, so if a
future change makes overwrites visible, the notice in its output changes.

## 5. Architecture Evaluation & Alternatives (2026-11)

Before deploying `cachey` in front of `ostore`, evaluate whether an external
cache daemon is actually beneficial for your topology:

- **Same-host local storage (e.g., single VPS with ext4 SSD)**: **Do not deploy
  `cachey` on the same host.** `ostore` reads directly off local disk, which the
  Linux kernel Page Cache already caches in RAM with zero extra network hop,
  zero duplicate disk space, and zero stale-read risk. `ostore` also ships
  built-in caches (`ChecksumCache`, `DirSizeCache`, `.ostore/thumbs`, HTTP
  `ETag`/`304`/Gzip, and UI SWR `dirCache`).
- **Remote edge / cross-region read-through cache**: An external cache makes
  sense only when readers are separated from `ostore` by high-latency or
  metered links.

| Project | Lang | API / Transparency | Invalidation | Best Fit |
|---|---|---|---|---|
| **Linux Page Cache + ostore built-in** | Kernel / Rust | 100% transparent | Instant (VFS) | Same-host deployment (default, recommended) |
| **`s2-streamstore/cachey`** (v0.13.5+) | Rust (`foyer`) | Custom `/fetch/<kind>/<key>` + mandatory `Range` + `C0-Bucket` | None (immutable blobs only) | High-throughput range-streaming of immutable blobs from remote S3 |
| **`aws-samples/sample-s3-hybrid-cache`** | Rust | **Transparent S3 proxy** (works with `awscli`/`boto3`/`rclone`) | ETag / multi-tier | Drop-in remote S3 read-through cache without client changes |
| **`caddyserver/cache-handler` / `darkweak/souin`** | Go | Transparent HTTP (RFC 9111) inside Caddy | `Cache-Control` / Purge API | Edge HTTP caching in front of Caddy `/ostore/*` |
| **`foyer-rs/foyer`** | Rust crate | Embedded hybrid RAM+disk library | In-process on `PUT`/`DELETE` | Future in-process read-through cache for remote/GDrive followers |
