# ClipVault → ostore Backup Strategy

> Based on ClipVault architecture decisions in nmem: `per-host trx/`, **`backup/hosts/{hostId}/latest`**,
> **eventual consistency**, **never whole-db replace**; backups are strictly for hydrate/repair, **never as a sync bus**.
> ostore on this server (cc.guohuasun.com) acts as a **third-site cold object storage tier**, not replacing iCloud/GDrive/Quark.

## 1. Role Division

| Layer | Location | Purpose |
|------|------|------|
| Primary Database | Each Mac `KEEPSAKE_HOME` | Daily read/write operations |
| Sync | `sync/v1/trx/{hostId}/` + cloud transport | Multi-machine eventual consistency |
| Local Disaster Recovery | iCloud / GDrive `cvbak` / Quark staging | Single-machine recovery, blob hydration |
| **Remote Cold Backup (New)** | **ostore** key `backup/hosts/{hostId}/latest/` (bucket `clipvault`) | Cross-machine, cross-cloud centralized retention; **mirror only, no sync participation** |

The local `clipvault-session` (Postgres replica) is **not** the Keepsake file store and **cannot** use `pg_dump` to substitute the Mac's `backup/latest`. The cold backup source is always the **directory tree generated on Mac after ClipFlow runs `POST /api/backup/run`**.

## 2. Server-side Setup (ostore)

Enable S3 interface in `/etc/ostore.env` (isolated from JSON `/api`'s `OSTORE_TOKEN`):

```bash
OSTORE_S3_ACCESS_KEY=clipvault-backup
OSTORE_S3_SECRET_KEY=<openssl rand -hex 32>
OSTORE_S3_REGION=us-east-1
OSTORE_S3_BUCKET=clipvault
```

A **dedicated root directory** is recommended to isolate from media files:

```bash
# Optional: dedicated instance or dedicated OSTORE_ROOT
OSTORE_ROOT=/srv/ostore-clipvault
```

Verify after `systemctl restart ostore`:

```bash
python3 scripts/s3_smoke.py http://127.0.0.1:8781   # requires boto3; or via Caddy /ostore prefix
```

When exposing externally: Caddy reverse-proxy with TLS via `handle /ostore/*`; **only the backup script holds the S3 credentials**, never exposed to browsers.

## 3. Mac-side Push (Recommended: S3 sync)

On **each** Mac configured with local backups, push to ostore after local backup succeeds:

```bash
export AWS_ACCESS_KEY_ID=clipvault-backup
export AWS_SECRET_ACCESS_KEY=<same as OSTORE_S3_SECRET_KEY>
export AWS_DEFAULT_REGION=us-east-1
export AWS_ENDPOINT_URL=https://cc.guohuasun.com/ostore   # adjust according to Caddy prefix

HOST_ID=$(defaults read com.davidmusk.clipflow hostId 2>/dev/null || hostname -s)
SRC="$HOME/Library/Application Support/Keepsake/backup/hosts/$HOST_ID/latest"

aws s3 sync "$SRC" "s3://clipvault/backup/hosts/$HOST_ID/latest/" \
  --only-show-errors \
  --no-progress
```

Notes:
- S3 keys are relative paths in ostore: `backup/hosts/{hostId}/latest/...` (path-style, `clipvault` is virtual bucket).
- **`aws s3 sync`** relies on ostore's **ListObjectsV2 + PutObject + DeleteObject + HeadObject**; do not use whole-database destructive rsync to incorrect paths.
- **Do not** treat ostore as the "latest truth" before sync completes; Mac local storage + iCloud/GDrive remain the primary recovery tier.

### Hook into ClipFlow Backup Pipeline

Trigger the sync above after `POST /api/backup/run` succeeds (`lastPhase=ok`) via LaunchAgent, `backup.json` hook, or shell script. Failures trigger alerts only and **never** roll back local backups.

## 4. Alternative: JSON API Mirror (Without AWS CLI)

Same semantics as "mirror-up" in README, suitable when only `OSTORE_TOKEN` is available without S3 enabled:

```bash
T=https://cc.guohuasun.com/ostore
AUTH="Authorization: Bearer <OSTORE_TOKEN>"
PREFIX="backup/hosts/$HOST_ID/latest"

curl -sf -H "$AUTH" "$T/api/list?path=$PREFIX&recursive=1&checksums=1" \
  | jq -r '.entries[] | select(.type=="file") | [.path,.sha256] | @tsv' | sort > /tmp/remote.tsv
# Reconcile locally, PUT changed files, DELETE stale files (see README Backup semantics)
```

The S3 interface is preferred for **large blob directories** and **incremental cron sync**.

## 5. Restoration Process

1. **Single machine loss**: First recover `latest/` + blobs from iCloud/GDrive `cvbak` or Quark staging.
2. **Mac loss & cloud drives unavailable**: Pull corresponding host prefix from ostore:
   ```bash
   aws s3 sync "s3://clipvault/backup/hosts/$HOST_ID/latest/" "$RESTORE_DIR/"
   ```
3. Attach `latest` back to `KEEPSAKE_HOME` per standard ClipVault procedure, run verify / hydrate (`live/attach` still relies on trx and repair; **never replace sync with ostore**).

## 6. Policies & Constraints (from nmem)

- **Write-once keys**: Filenames in backup trees primarily use CAS/hashes, avoiding same-key overwrites causing stale cache serving; ClipVault blobs are inherently content-addressed, matching ostore cleanly.
- **Forbidden**: Do not use `backup/latest` on ostore as a **multi-machine sync bus** (shared `latest/` is deprecated, only per-host allowed).
- **Forbidden**: No "whole-database replacement" restorations; only restore the **`latest` tree for a single `hostId`**.
- **web_archive** must include **document closure** (`blob_keys`); cold backup only copies the closed `latest`, without synthesizing archive logic on ostore.
- Credential rotation: Update `OSTORE_S3_*` concurrently with `AWS_*` on Macs. Rotate JSON `OSTORE_TOKEN` and S3 credentials independently.

## 7. Monitoring

```bash
curl -s -H "Authorization: Bearer $OSTORE_TOKEN" http://127.0.0.1:8781/api/metrics \
  | grep ostore_s3_
```

Mac side: Backup API `lastPhase` + sync exit code; optional weekly cron `aws s3 ls` spot-checks comparing key count against local blobs count.

## 8. Coexistence with Media Storage

Current production `OSTORE_ROOT=/root/xstream/data/videos` serves xstream `/files`. For ClipVault cold backup:

- **Option A (Recommended)**: Second `ostore` instance + `/srv/ostore-clipvault` + isolated port/environment.
- **Option B**: Single instance with prefix isolation (`clipvault/...` vs `videos/...`); shares single root, suitable for personal servers with strict path discipline.
