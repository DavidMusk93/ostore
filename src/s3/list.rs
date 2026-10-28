//! S3 ListObjectsV2 (`GET /{bucket}?list-type=2`).

use std::collections::BTreeSet;

use axum::extract::{Query, State};
use axum::http::{HeaderMap, Method, StatusCode, Uri};
use axum::response::Response;
use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::AppState;
use crate::paths;
use crate::routes::list::hidden;

use super::xml::{escape_xml, s3_iso8601, strong_etag, xml_error, xml_ok};
use super::{authorize, bucket_ok};

#[derive(Deserialize, Default)]
pub struct ListV2Query {
    #[serde(rename = "list-type")]
    list_type: Option<String>,
    prefix: Option<String>,
    delimiter: Option<String>,
    #[serde(rename = "max-keys")]
    max_keys: Option<u32>,
    #[serde(rename = "continuation-token")]
    continuation_token: Option<String>,
    #[serde(rename = "start-after")]
    start_after: Option<String>,
}

pub async fn list_objects_v2(
    State(st): State<AppState>,
    bucket: String,
    Query(q): Query<ListV2Query>,
    method: Method,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    if method != Method::GET {
        return xml_error(StatusCode::METHOD_NOT_ALLOWED, "MethodNotAllowed", "method not allowed", uri.path());
    }
    if q.list_type.as_deref() != Some("2") {
        return xml_error(StatusCode::NOT_FOUND, "NoSuchKey", "list-type=2 is required for bucket listing", uri.path());
    }
    let auth = match authorize(&st, &uri, &headers, method.as_str()) {
        Ok(a) => a,
        Err(r) => return r,
    };
    if let Err(r) = bucket_ok(&auth, &bucket, uri.path()) {
        return r;
    }
    st.metrics.s3_lists.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    let prefix = q.prefix.unwrap_or_default();
    if prefix.bytes().any(|b| b == 0) {
        return xml_error(StatusCode::BAD_REQUEST, "InvalidRequest", "invalid prefix", uri.path());
    }
    let delimiter = q.delimiter.unwrap_or_default();
    let max_keys = q.max_keys.unwrap_or(1000).clamp(1, 1000) as usize;
    let start_after = q.continuation_token.or(q.start_after).unwrap_or_default();

    let list_root = list_base(&st.root, &prefix);
    let mut keys: Vec<ListRow> = Vec::new();
    let mut prefixes: BTreeSet<String> = BTreeSet::new();
    list_flat(
        &list_root.abs,
        &list_root.rel,
        &prefix,
        &delimiter,
        &start_after,
        max_keys + 1,
        &mut keys,
        &mut prefixes,
    )
    .await;

    let truncated = keys.len() > max_keys;
    if truncated {
        keys.truncate(max_keys);
    }
    let next_token = truncated.then(|| keys.last().expect("truncated").key.clone());

    let mut out = String::with_capacity(4096);
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<ListBucketResult xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\">");
    out.push_str(&format!("<Name>{}</Name>", escape_xml(&bucket)));
    out.push_str(&format!("<Prefix>{}</Prefix>", escape_xml(&prefix)));
    out.push_str(&format!("<KeyCount>{}</KeyCount>", keys.len()));
    out.push_str(&format!("<MaxKeys>{}</MaxKeys>", max_keys));
    out.push_str(if truncated { "<IsTruncated>true</IsTruncated>" } else { "<IsTruncated>false</IsTruncated>" });
    if !delimiter.is_empty() {
        out.push_str(&format!("<Delimiter>{}</Delimiter>", escape_xml(&delimiter)));
    }
    if let Some(tok) = &next_token {
        out.push_str(&format!("<NextContinuationToken>{}</NextContinuationToken>", escape_xml(tok)));
    }
    for p in &prefixes {
        out.push_str(&format!("<CommonPrefixes><Prefix>{}</Prefix></CommonPrefixes>", escape_xml(p)));
    }
    for row in &keys {
        out.push_str("<Contents>");
        out.push_str(&format!("<Key>{}</Key>", escape_xml(&row.key)));
        out.push_str(&format!("<LastModified>{}</LastModified>", s3_iso8601(row.mtime)));
        out.push_str(&format!("<ETag>{}</ETag>", escape_xml(&row.etag)));
        out.push_str(&format!("<Size>{}</Size>", row.size));
        out.push_str("<StorageClass>STANDARD</StorageClass>");
        out.push_str("</Contents>");
    }
    out.push_str("</ListBucketResult>\n");
    xml_ok(out)
}

struct ListBase {
    abs: std::path::PathBuf,
    rel: String,
}

fn list_base(root: &std::path::Path, prefix: &str) -> ListBase {
    let trimmed = prefix.trim_end_matches('/');
    if trimmed.is_empty() {
        return ListBase { abs: root.to_path_buf(), rel: String::new() };
    }
    match paths::resolve(root, trimmed) {
        Ok((abs, rel)) => ListBase { abs, rel },
        Err(_) => ListBase { abs: root.join(trimmed), rel: trimmed.to_string() },
    }
}

struct ListRow {
    key: String,
    size: u64,
    mtime: DateTime<Utc>,
    etag: String,
}

async fn list_flat(
    dir_abs: &std::path::Path,
    dir_rel: &str,
    prefix: &str,
    delimiter: &str,
    start_after: &str,
    limit: usize,
    keys: &mut Vec<ListRow>,
    prefixes: &mut BTreeSet<String>,
) {
    let prefix_trim = prefix.trim_end_matches('/');
    let mut stack: Vec<(std::path::PathBuf, String)> = vec![(dir_abs.to_path_buf(), dir_rel.to_string())];
    while let Some((abs, rel)) = stack.pop() {
        if keys.len() >= limit {
            break;
        }
        let Ok(meta) = tokio::fs::metadata(&abs).await else { continue };
        if !meta.is_dir() {
            if !start_after.is_empty() && rel.as_str() <= start_after {
                continue;
            }
            if !prefix.is_empty() && !rel.starts_with(prefix_trim) {
                continue;
            }
            keys.push(row_from(&rel, &meta));
            continue;
        }
        let Ok(mut rd) = tokio::fs::read_dir(&abs).await else { continue };
        let mut children: Vec<(std::path::PathBuf, String, bool)> = Vec::new();
        while let Ok(Some(de)) = rd.next_entry().await {
            let name = de.file_name().to_string_lossy().into_owned();
            if hidden(&name) {
                continue;
            }
            let child_rel = if rel.is_empty() { name } else { format!("{rel}/{name}") };
            if !prefix.is_empty() && !child_rel.starts_with(prefix_trim) {
                continue;
            }
            let Ok(ft) = de.file_type().await else { continue };
            children.push((de.path(), child_rel, ft.is_dir()));
        }
        children.sort_by(|a, b| a.1.cmp(&b.1));
        for (path, child_rel, is_dir) in children.into_iter().rev() {
            if is_dir {
                if delimiter == "/" {
                    prefixes.insert(format!("{child_rel}/"));
                } else {
                    stack.push((path, child_rel));
                }
            } else if keys.len() < limit {
                if !start_after.is_empty() && child_rel.as_str() <= start_after {
                    continue;
                }
                if let Ok(meta) = tokio::fs::metadata(&path).await {
                    keys.push(row_from(&child_rel, &meta));
                }
            }
        }
    }
    keys.sort_by(|a, b| a.key.cmp(&b.key));
}

fn row_from(key: &str, meta: &std::fs::Metadata) -> ListRow {
    let mtime: DateTime<Utc> = meta
        .modified()
        .map(DateTime::<Utc>::from)
        .unwrap_or_else(|_| Utc::now());
    ListRow {
        key: key.to_string(),
        size: meta.len(),
        mtime,
        etag: strong_etag(meta),
    }
}
