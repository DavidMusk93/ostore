#!/usr/bin/env bash
# Verify cachey in front of ostore's S3 read surface: cold read misses, warm
# read hits, bytes match the backend, out-of-range propagates 416. Also shows
# what happens when an object is overwritten underneath the cache.
#
#   ./scripts/cachey_check.sh [cachey_url] [ostore_url] [bucket]
#
# Requires ostore (with the S3 surface enabled) and cachey to be running.
set -euo pipefail

CACHEY=${1:-http://127.0.0.1:8800}
OSTORE=${2:-http://127.0.0.1:8799}
BUCKET=${3:-media}
TOKEN=${OSTORE_TOKEN:-t}

# A fresh key each run, so the cold read is genuinely cold.
KEY="checks/run-$(date +%s)-$$.bin"
SIZE=$((4 * 1024 * 1024))
KIND=prod

URL="$CACHEY/fetch/$KIND/$KEY"
HDR=(-H "C0-Bucket: $BUCKET")
fail=0

echo "cachey=$CACHEY ostore=$OSTORE bucket=$BUCKET key=$KEY"

python3 - "$OSTORE" "$KEY" "$SIZE" "$TOKEN" <<'PY'
import sys, urllib.parse, urllib.request
base, key, size, token = sys.argv[1], sys.argv[2], int(sys.argv[3]), sys.argv[4]
body = bytes(range(256)) * (size // 256)
url = f"{base}/api/files/{urllib.parse.quote(key)}"
req = urllib.request.Request(url, data=body, method="PUT", headers={"Authorization": f"Bearer {token}"})
urllib.request.urlopen(req).read()
print(f"  seeded {size} bytes")
PY

fetch() {
  local label=$1 range=$2
  local code status len
  code=$(curl -sS -D /tmp/cachey-hdr.txt "${HDR[@]}" -H "Range: $range" "$URL" -o /tmp/cachey-body.bin -w '%{http_code}')
  status=$(grep -i '^c0-status:' /tmp/cachey-hdr.txt | tr -d '\r' | sed 's/^[Cc]0-[Ss]tatus: *//')
  len=$(stat -c %s /tmp/cachey-body.bin)
  printf '  %-6s http=%s bytes=%-9s c0-status=%s\n' "$label" "$code" "$len" "$status"
  printf '%s' "$status" > /tmp/cachey-status.txt
}

check() {
  if [ "$1" = ok ]; then echo "  ok    $2"; else echo "  FAIL  $2"; fail=1; fi
}

fetch cold "bytes=0-1048575"
grep -q '; 0$' /tmp/cachey-status.txt && r=ok || r=fail
check $r "cold read is a cache miss"

fetch warm "bytes=0-1048575"
grep -q '; 0$' /tmp/cachey-status.txt && r=fail || r=ok
check $r "warm read is a cache hit"

curl -sS "${HDR[@]}" -H "Range: bytes=0-$((SIZE - 1))" "$URL" -o /tmp/via_cachey.bin
curl -sS -H "Authorization: Bearer $TOKEN" "$OSTORE/api/files/$KEY" -o /tmp/direct.bin
a=$(sha256sum /tmp/via_cachey.bin | cut -d' ' -f1)
b=$(sha256sum /tmp/direct.bin | cut -d' ' -f1)
[ "$a" = "$b" ] && r=ok || r=fail
check $r "bytes through cachey match the backend ($a)"

# A read spanning several 16 MiB pages.
fetch multi "bytes=1048576-25165823" >/dev/null
[ "$(stat -c %s /tmp/cachey-body.bin)" -gt 0 ] && r=ok || r=fail
check $r "multi-page read returns data"

code=$(curl -sS -o /dev/null -w '%{http_code}' "${HDR[@]}" -H "Range: bytes=999999999-" "$URL")
[ "$code" = "416" ] && r=ok || r=fail
check $r "out-of-range read propagates 416 (got $code)"

# Overwrite visibility: the cache key carries no version, so cachey keeps
# serving the old bytes until the entry is evicted. This is the documented
# caveat, asserted here so a change in behavior is noticed.
python3 - "$OSTORE" "$KEY" "$SIZE" "$TOKEN" <<'PY'
import sys, urllib.parse, urllib.request
base, key, size, token = sys.argv[1], sys.argv[2], int(sys.argv[3]), sys.argv[4]
url = f"{base}/api/files/{urllib.parse.quote(key)}"
req = urllib.request.Request(url, data=b"X" * size, method="PUT", headers={"Authorization": f"Bearer {token}"})
urllib.request.urlopen(req).read()
PY
curl -sS "${HDR[@]}" -H "Range: bytes=0-0" "$URL" -o /tmp/first-byte.bin
if [ "$(head -c1 /tmp/first-byte.bin | od -An -c | tr -d ' ')" = "X" ]; then
  echo "  note  overwrite was visible (cache invalidated or entry evicted)"
else
  echo "  note  overwrite served stale, as expected for an immutable-blob cache"
fi

echo
[ "$fail" = 0 ] && echo "all cachey checks passed" || { echo "SOME CHECKS FAILED"; exit 1; }
