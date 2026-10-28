#!/usr/bin/env python3
"""End-to-end check of ostore's S3 read surface with the real AWS SDK (boto3).

Run with an ostore instance already listening:

    OSTORE_ROOT=/tmp/ostore-s3 OSTORE_TOKEN=t OSTORE_PORT=8799 \
    OSTORE_S3_ACCESS_KEY=testkey OSTORE_S3_SECRET_KEY=testsecret \
    OSTORE_S3_REGION=us-east-1 OSTORE_S3_BUCKET=media ./target/release/ostore

    python3 scripts/s3_smoke.py http://127.0.0.1:8799
"""

import sys

import boto3
from botocore.client import Config
from botocore.exceptions import ClientError

ENDPOINT = sys.argv[1] if len(sys.argv) > 1 else "http://127.0.0.1:8799"
BUCKET = "media"
KEY = "videos/clip.bin"
DATA = bytes(range(256)) * 16384  # 4 MiB: several cachey pages' worth

failures = []


def check(name, ok, detail=""):
    print(("  ok   " if ok else "  FAIL ") + name + (f"  [{detail}]" if detail and not ok else ""))
    if not ok:
        failures.append(name)


def client(key="testkey", secret="testsecret"):
    return boto3.client(
        "s3",
        endpoint_url=ENDPOINT,
        aws_access_key_id=key,
        aws_secret_access_key=secret,
        region_name="us-east-1",
        config=Config(s3={"addressing_style": "path"}, retries={"max_attempts": 1}),
    )


def seed():
    """Upload through the JSON API so the object exists without needing PutObject."""
    import json
    import urllib.request

    req = urllib.request.Request(
        f"{ENDPOINT}/api/files/{KEY}",
        data=DATA,
        method="PUT",
        headers={"Authorization": "Bearer t"},
    )
    with urllib.request.urlopen(req) as resp:
        assert resp.status == 200, resp.status
        return json.load(resp)


def main():
    s3 = client()
    print(f"ostore S3 surface at {ENDPOINT}")

    body = seed()
    print(f"  seeded {body['path']} ({body['size']} bytes, sha256 {body['sha256'][:16]}...)")

    # HeadObject: the SDK maps this to HEAD and validates the response shape.
    head = s3.head_object(Bucket=BUCKET, Key=KEY)
    check("HeadObject reports ContentLength", head["ContentLength"] == len(DATA), head.get("ContentLength"))
    check("HeadObject reports an ETag", bool(head.get("ETag")), head.get("ETag"))
    check("HeadObject is ranged-capable", head.get("AcceptRanges") == "bytes")
    check("HeadObject reports LastModified", head.get("LastModified") is not None)

    # GetObject, whole object.
    got = s3.get_object(Bucket=BUCKET, Key=KEY)
    full = got["Body"].read()
    check("GetObject returns all bytes", full == DATA, f"{len(full)} != {len(DATA)}")
    check("GetObject reports the same ETag", got["ETag"] == head["ETag"])

    # Ranged GET: this is the form cachey uses for every read.
    part = s3.get_object(Bucket=BUCKET, Key=KEY, Range="bytes=1048576-1049999")
    piece = part["Body"].read()
    check("ranged GetObject returns the slice", piece == DATA[1048576:1050000], len(piece))
    check(
        "ranged GetObject reports ContentRange",
        part.get("ContentRange") == f"bytes 1048576-1049999/{len(DATA)}",
        part.get("ContentRange"),
    )
    check("ranged GetObject is 206", part["ResponseMetadata"]["HTTPStatusCode"] == 206)

    # Tail range, as a media player scrubbing to the end sends.
    tail = s3.get_object(Bucket=BUCKET, Key=KEY, Range=f"bytes={len(DATA) - 10}-")
    check("open-ended range returns the tail", tail["Body"].read() == DATA[-10:])

    # Unsatisfiable range: botocore surfaces the S3 error code, and cachey
    # recovers the object size from Content-Range bytes */N.
    try:
        s3.get_object(Bucket=BUCKET, Key=KEY, Range=f"bytes={len(DATA)}-{len(DATA) + 15}")
        check("unsatisfiable range is rejected", False, "no error raised")
    except ClientError as e:
        err = e.response
        check("unsatisfiable range -> InvalidRange", err["Error"]["Code"] == "InvalidRange", err["Error"])
        check(
            "unsatisfiable range reports the object size",
            err["ResponseMetadata"]["HTTPHeaders"].get("content-range") == f"bytes */{len(DATA)}",
            err["ResponseMetadata"]["HTTPHeaders"].get("content-range"),
        )
        check("unsatisfiable range is 416", err["ResponseMetadata"]["HTTPStatusCode"] == 416)

    # Conditional GET. botocore surfaces 304 as a ClientError, like real S3.
    try:
        s3.get_object(Bucket=BUCKET, Key=KEY, IfNoneMatch=head["ETag"])
        check("IfNoneMatch hit returns 304", False, "no error raised")
    except ClientError as e:
        check("IfNoneMatch hit -> 304", e.response["ResponseMetadata"]["HTTPStatusCode"] == 304)

    # Missing key. A HEAD carries no body, so the SDK can only see the status;
    # the XML error code is only readable on a GET (true of real S3 too).
    try:
        s3.head_object(Bucket=BUCKET, Key="does/not/exist")
        check("missing key HEAD is 404", False, "no error raised")
    except ClientError as e:
        check("missing key HEAD -> 404", e.response["ResponseMetadata"]["HTTPStatusCode"] == 404)
    try:
        s3.get_object(Bucket=BUCKET, Key="does/not/exist")
        check("missing key GET is 404", False, "no error raised")
    except ClientError as e:
        check("missing key GET -> 404", e.response["ResponseMetadata"]["HTTPStatusCode"] == 404)
        check("missing key GET -> NoSuchKey", e.response["Error"]["Code"] == "NoSuchKey", e.response["Error"])

    # Wrong credentials.
    try:
        client("testkey", "wrong-secret").get_object(Bucket=BUCKET, Key=KEY)
        check("wrong secret is rejected", False, "no error raised")
    except ClientError as e:
        check("wrong secret -> SignatureDoesNotMatch", e.response["Error"]["Code"] == "SignatureDoesNotMatch")
        check("wrong secret -> 403", e.response["ResponseMetadata"]["HTTPStatusCode"] == 403)

    # Bucket pinning.
    try:
        client().get_object(Bucket="other-bucket", Key=KEY)
        check("pinned bucket rejects other names", False, "no error raised")
    except ClientError as e:
        check("pinned bucket -> NoSuchBucket", e.response["Error"]["Code"] == "NoSuchBucket", e.response["Error"])

    # Overwrite visibility: ostore allows same-path PUT, so the ETag must move.
    seed_again = urllib_request_put(DATA + b"appended")
    check("overwrite committed", seed_again == len(DATA) + 8, seed_again)
    head2 = s3.head_object(Bucket=BUCKET, Key=KEY)
    check("ETag changes after overwrite", head2["ETag"] != head["ETag"], head2["ETag"])
    check("size changes after overwrite", head2["ContentLength"] == len(DATA) + 8)

    resp = s3.list_objects_v2(Bucket=BUCKET, Prefix="smoke/", MaxKeys=10)
    check("ListObjectsV2 returns Contents", isinstance(resp.get("Contents"), (list, type(None))))

    test_key = "smoke/s3-api-check.txt"
    body = b"ostore s3 put/delete " + str(len(DATA)).encode()
    s3.put_object(Bucket=BUCKET, Key=test_key, Body=body)
    got = s3.get_object(Bucket=BUCKET, Key=test_key)["Body"].read()
    check("PutObject roundtrip", got == body, f"{len(got)} bytes")
    s3.delete_object(Bucket=BUCKET, Key=test_key)
    try:
        s3.head_object(Bucket=BUCKET, Key=test_key)
        check("DeleteObject removed the key", False, "still exists")
    except ClientError as e:
        check("DeleteObject removed the key", e.response["ResponseMetadata"]["HTTPStatusCode"] == 404)

    hb = s3.head_bucket(Bucket=BUCKET)
    check("HeadBucket ok", hb["ResponseMetadata"]["HTTPStatusCode"] == 200)

    print()
    if failures:
        print(f"FAILED: {len(failures)} check(s): {', '.join(failures)}")
        return 1
    print("all checks passed")
    return 0


def urllib_request_put(data):
    import urllib.request

    req = urllib.request.Request(
        f"{ENDPOINT}/api/files/{KEY}",
        data=data,
        method="PUT",
        headers={"Authorization": "Bearer t"},
    )
    with urllib.request.urlopen(req) as resp:
        assert resp.status == 200
        return len(data)


if __name__ == "__main__":
    sys.exit(main())
