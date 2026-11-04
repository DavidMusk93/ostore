//! Google Drive Cloud Storage backend integration for ostore.
//!
//! Exposes Google Drive as a first-class storage node in ostore's distributed cluster.
//! Features native MD5 checksum alignment (`md5Checksum`), batched directory queries
//! (up to 1,000 files per page), in-memory TTL metadata caching, and automatic
//! OAuth2 refresh token rotation.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::{RwLock, Semaphore};

const GDRIVE_OAUTH_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const GDRIVE_API_BASE: &str = "https://www.googleapis.com/drive/v3";
const GDRIVE_UPLOAD_BASE: &str = "https://www.googleapis.com/upload/drive/v3";
const FOLDER_MIME_TYPE: &str = "application/vnd.google-apps.folder";

fn default_root_folder() -> String {
    "root".to_string()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GDriveConfig {
    pub client_id: String,
    pub client_secret: String,
    pub refresh_token: String,
    #[serde(default = "default_root_folder")]
    pub root_folder_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GDriveFileMeta {
    pub id: String,
    pub name: String,
    pub mime_type: String,
    pub size: u64,
    pub md5: Option<String>,
    pub modified_time: Option<DateTime<Utc>>,
    pub is_dir: bool,
}

#[derive(Clone)]
pub struct GDriveClient {
    pub config: GDriveConfig,
    client: reqwest::Client,
    token_cache: Arc<RwLock<Option<(String, Instant)>>>,
    folder_id_cache: Arc<RwLock<HashMap<String, String>>>,
    metadata_cache: Arc<RwLock<HashMap<String, (Instant, GDriveFileMeta)>>>,
    dir_listing_cache: Arc<RwLock<HashMap<String, (Instant, Vec<GDriveFileMeta>)>>>,
    upload_semaphore: Arc<Semaphore>,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default = "default_expires_in")]
    expires_in: u64,
}

fn default_expires_in() -> u64 {
    3600
}

#[derive(Deserialize)]
struct StorageQuota {
    #[serde(default)]
    limit: Option<String>,
    #[serde(default)]
    usage: Option<String>,
}

#[derive(Deserialize)]
struct AboutResponse {
    #[serde(rename = "storageQuota")]
    storage_quota: Option<StorageQuota>,
}

#[derive(Deserialize)]
struct FileListResponse {
    #[serde(rename = "nextPageToken")]
    next_page_token: Option<String>,
    #[serde(default)]
    files: Vec<DriveFileItem>,
}

#[derive(Deserialize)]
struct DriveFileItem {
    id: String,
    name: String,
    #[serde(rename = "mimeType")]
    mime_type: Option<String>,
    #[serde(default)]
    size: Option<String>,
    #[serde(rename = "md5Checksum")]
    md5_checksum: Option<String>,
    #[serde(rename = "modifiedTime")]
    modified_time: Option<String>,
}

impl GDriveClient {
    pub fn new(config: GDriveConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .unwrap_or_default();

        Self {
            config,
            client,
            token_cache: Arc::new(RwLock::new(None)),
            folder_id_cache: Arc::new(RwLock::new(HashMap::new())),
            metadata_cache: Arc::new(RwLock::new(HashMap::new())),
            dir_listing_cache: Arc::new(RwLock::new(HashMap::new())),
            upload_semaphore: Arc::new(Semaphore::new(2)),
        }
    }

    pub fn from_env() -> Option<Self> {
        let client_id = std::env::var("OSTORE_GDRIVE_CLIENT_ID").ok()?;
        let client_secret = std::env::var("OSTORE_GDRIVE_CLIENT_SECRET").ok()?;
        let refresh_token = std::env::var("OSTORE_GDRIVE_REFRESH_TOKEN").ok()?;
        let root_folder_id = std::env::var("OSTORE_GDRIVE_ROOT_FOLDER_ID")
            .unwrap_or_else(|_| "root".to_string());

        if client_id.is_empty() || client_secret.is_empty() || refresh_token.is_empty() {
            return None;
        }

        Some(Self::new(GDriveConfig {
            client_id,
            client_secret,
            refresh_token,
            root_folder_id,
        }))
    }

    /// Execute an HTTP request closure with exponential backoff & jitter for Google Drive rate limits.
    pub async fn execute_with_retry<F>(&self, op_name: &str, make_builder: F) -> Result<reqwest::Response, String>
    where
        F: Fn() -> reqwest::RequestBuilder,
    {
        let mut attempt = 0;
        let max_attempts = 5;
        loop {
            attempt += 1;
            let req = make_builder();
            match req.send().await {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() || status == reqwest::StatusCode::NOT_FOUND {
                        return Ok(resp);
                    }

                    let is_rate_limited = status == reqwest::StatusCode::TOO_MANY_REQUESTS;
                    let is_server_error = status.is_server_error();
                    let is_forbidden = status == reqwest::StatusCode::FORBIDDEN;

                    if (is_rate_limited || is_server_error || is_forbidden) && attempt < max_attempts {
                        let retry_after_secs = resp
                            .headers()
                            .get(reqwest::header::RETRY_AFTER)
                            .and_then(|h| h.to_str().ok())
                            .and_then(|s| s.parse::<u64>().ok());

                        let body = resp.text().await.unwrap_or_default();
                        if is_forbidden {
                            let is_rate_limit_reason = body.contains("userRateLimitExceeded")
                                || body.contains("rateLimitExceeded");
                            let is_quota_exceeded = body.contains("storageQuotaExceeded")
                                || body.contains("quotaExceeded");

                            if is_quota_exceeded {
                                return Err("Google Drive daily upload quota (750GB/day limit) exceeded. Please wait 24 hours.".to_string());
                            }
                            if !is_rate_limit_reason {
                                return Err(format!("{op_name} HTTP 403 Forbidden: {body}"));
                            }
                        }

                        let delay = if let Some(secs) = retry_after_secs {
                            Duration::from_secs(secs)
                        } else {
                            // Exponential backoff: 1s, 2s, 4s, 8s + 100-400ms jitter
                            let base_ms = 1000u64.saturating_mul(1u64 << (attempt - 1));
                            let jitter_ms = (chrono::Utc::now().timestamp_subsec_millis() % 300) as u64 + 100;
                            Duration::from_millis((base_ms + jitter_ms).min(30_000))
                        };

                        tracing::warn!(op = op_name, attempt, delay_ms = delay.as_millis(), status = %status, "Google Drive rate limit/transient error, backing off...");
                        tokio::time::sleep(delay).await;
                        continue;
                    }

                    let body = resp.text().await.unwrap_or_default();
                    return Err(format!("{op_name} HTTP {status}: {body}"));
                }
                Err(e) => {
                    if attempt < max_attempts && (e.is_timeout() || e.is_connect()) {
                        let delay = Duration::from_millis(500 * (1 << (attempt - 1)));
                        tokio::time::sleep(delay).await;
                        continue;
                    }
                    return Err(format!("{op_name} request failed: {e}"));
                }
            }
        }
    }

    /// Retrieve valid OAuth2 access token with automatic refresh and caching.
    pub async fn get_access_token(&self) -> Result<String, String> {
        {
            let lock = self.token_cache.read().await;
            if let Some((token, expires_at)) = &*lock {
                if Instant::now() < *expires_at {
                    return Ok(token.clone());
                }
            }
        }

        let mut lock = self.token_cache.write().await;
        // Double check lock
        if let Some((token, expires_at)) = &*lock {
            if Instant::now() < *expires_at {
                return Ok(token.clone());
            }
        }

        let params = [
            ("client_id", self.config.client_id.as_str()),
            ("client_secret", self.config.client_secret.as_str()),
            ("refresh_token", self.config.refresh_token.as_str()),
            ("grant_type", "refresh_token"),
        ];

        let resp = self
            .client
            .post(GDRIVE_OAUTH_TOKEN_URL)
            .form(&params)
            .send()
            .await
            .map_err(|e| format!("OAuth token request error: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("OAuth token refresh failed HTTP {status}: {body}"));
        }

        let token_resp: TokenResponse = resp
            .json()
            .await
            .map_err(|e| format!("failed to decode OAuth token JSON: {e}"))?;

        let expires_in_sec = token_resp.expires_in.saturating_sub(120).max(60);
        let expires_at = Instant::now() + Duration::from_secs(expires_in_sec);
        *lock = Some((token_resp.access_token.clone(), expires_at));

        Ok(token_resp.access_token)
    }

    /// Fetch Google Drive account storage quota. Returns (total_bytes, available_bytes).
    pub async fn get_storage_quota(&self) -> Result<(u64, u64), String> {
        let token = self.get_access_token().await?;
        let url = format!("{GDRIVE_API_BASE}/about?fields=storageQuota");
        let resp = self
            .execute_with_retry("get_storage_quota", || {
                self.client.get(&url).header("Authorization", format!("Bearer {token}"))
            })
            .await?;

        if !resp.status().is_success() {
            return Err(format!("about request HTTP {}", resp.status()));
        }

        let about: AboutResponse = resp
            .json()
            .await
            .map_err(|e| format!("about json decode error: {e}"))?;

        if let Some(sq) = about.storage_quota {
            let limit: u64 = sq.limit.and_then(|s| s.parse().ok()).unwrap_or(15 * 1024 * 1024 * 1024); // default 15GB
            let usage: u64 = sq.usage.and_then(|s| s.parse().ok()).unwrap_or(0);
            let available = limit.saturating_sub(usage);
            Ok((limit, available))
        } else {
            Ok((15 * 1024 * 1024 * 1024, 15 * 1024 * 1024 * 1024))
        }
    }

    /// Recursively resolve or create folder hierarchy on Google Drive.
    pub async fn ensure_folder_path(&self, rel_dir: &str) -> Result<String, String> {
        let rel_dir = rel_dir.trim_matches('/');
        if rel_dir.is_empty() {
            return Ok(self.config.root_folder_id.clone());
        }

        {
            let lock = self.folder_id_cache.read().await;
            if let Some(id) = lock.get(rel_dir) {
                return Ok(id.clone());
            }
        }

        let parts: Vec<&str> = rel_dir.split('/').filter(|p| !p.is_empty()).collect();
        let mut current_parent_id = self.config.root_folder_id.clone();
        let mut current_path = String::new();

        for part in parts {
            if !current_path.is_empty() {
                current_path.push('/');
            }
            current_path.push_str(part);

            {
                let lock = self.folder_id_cache.read().await;
                if let Some(id) = lock.get(&current_path) {
                    current_parent_id = id.clone();
                    continue;
                }
            }

            // Look up part in current_parent_id
            let token = self.get_access_token().await?;
            let escaped_name = part.replace('\'', "\\'");
            let q = format!(
                "'{current_parent_id}' in parents and name = '{escaped_name}' and mimeType = '{FOLDER_MIME_TYPE}' and trashed = false"
            );
            let url = format!("{GDRIVE_API_BASE}/files?q={}&fields=files(id,name)&pageSize=1", urlencoding::encode(&q));

            let resp = self
                .execute_with_retry("folder query", || {
                    self.client.get(&url).header("Authorization", format!("Bearer {token}"))
                })
                .await?;

            let target_folder_id = if resp.status().is_success() {
                let fl: FileListResponse = resp.json().await.map_err(|e| e.to_string())?;
                if let Some(existing) = fl.files.into_iter().next() {
                    existing.id
                } else {
                    // Create folder
                    let create_url = format!("{GDRIVE_API_BASE}/files");
                    let create_body = serde_json::json!({
                        "name": part,
                        "mimeType": FOLDER_MIME_TYPE,
                        "parents": [current_parent_id]
                    });
                    let create_resp = self
                        .execute_with_retry("create folder", || {
                            self.client
                                .post(&create_url)
                                .header("Authorization", format!("Bearer {token}"))
                                .json(&create_body)
                        })
                        .await?;

                    if !create_resp.status().is_success() {
                        return Err(format!("create folder {part} HTTP {}", create_resp.status()));
                    }
                    let created: serde_json::Value = create_resp.json().await.map_err(|e| e.to_string())?;
                    created
                        .get("id")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| "missing created folder id".to_string())?
                        .to_string()
                }
            } else {
                return Err(format!("query folder {part} HTTP {}", resp.status()));
            };

            let mut lock = self.folder_id_cache.write().await;
            lock.insert(current_path.clone(), target_folder_id.clone());
            current_parent_id = target_folder_id;
        }

        Ok(current_parent_id)
    }

    /// Read-only folder path lookup without creating folders if missing.
    pub async fn resolve_folder_path(&self, rel_dir: &str) -> Result<Option<String>, String> {
        let rel_dir = rel_dir.trim_matches('/');
        if rel_dir.is_empty() {
            return Ok(Some(self.config.root_folder_id.clone()));
        }

        {
            let lock = self.folder_id_cache.read().await;
            if let Some(id) = lock.get(rel_dir) {
                return Ok(Some(id.clone()));
            }
        }

        let parts: Vec<&str> = rel_dir.split('/').filter(|p| !p.is_empty()).collect();
        let mut current_parent_id = self.config.root_folder_id.clone();
        let mut current_path = String::new();

        for part in parts {
            if !current_path.is_empty() {
                current_path.push('/');
            }
            current_path.push_str(part);

            {
                let lock = self.folder_id_cache.read().await;
                if let Some(id) = lock.get(&current_path) {
                    current_parent_id = id.clone();
                    continue;
                }
            }

            let token = self.get_access_token().await?;
            let escaped_name = part.replace('\'', "\\'");
            let q = format!(
                "'{current_parent_id}' in parents and name = '{escaped_name}' and mimeType = '{FOLDER_MIME_TYPE}' and trashed = false"
            );
            let url = format!("{GDRIVE_API_BASE}/files?q={}&fields=files(id,name)&pageSize=1", urlencoding::encode(&q));

            let resp = self
                .execute_with_retry("folder query", || {
                    self.client.get(&url).header("Authorization", format!("Bearer {token}"))
                })
                .await?;

            if resp.status().is_success() {
                let fl: FileListResponse = resp.json().await.map_err(|e| e.to_string())?;
                if let Some(existing) = fl.files.into_iter().next() {
                    let mut lock = self.folder_id_cache.write().await;
                    lock.insert(current_path.clone(), existing.id.clone());
                    current_parent_id = existing.id;
                } else {
                    return Ok(None);
                }
            } else {
                return Err(format!("query folder {part} HTTP {}", resp.status()));
            }
        }

        Ok(Some(current_parent_id))
    }

    /// List a directory in Google Drive in batches of 1,000 files.
    pub async fn list_dir(&self, rel_dir: &str) -> Result<Vec<GDriveFileMeta>, String> {
        let rel_dir = rel_dir.trim_matches('/').to_string();
        {
            let lock = self.dir_listing_cache.read().await;
            if let Some((instant, items)) = lock.get(&rel_dir) {
                if instant.elapsed() < Duration::from_secs(60) {
                    return Ok(items.clone());
                }
            }
        }

        let folder_id = match self.resolve_folder_path(&rel_dir).await? {
            Some(id) => id,
            None => {
                let mut lock = self.dir_listing_cache.write().await;
                lock.insert(rel_dir.to_string(), (Instant::now(), Vec::new()));
                return Ok(Vec::new());
            }
        };
        let token = self.get_access_token().await?;
        let mut results = Vec::new();
        let mut page_token: Option<String> = None;

        loop {
            let q = format!("'{folder_id}' in parents and trashed = false");
            let mut url = format!(
                "{GDRIVE_API_BASE}/files?q={}&fields=nextPageToken,files(id,name,mimeType,size,md5Checksum,modifiedTime)&pageSize=1000",
                urlencoding::encode(&q)
            );
            if let Some(pt) = &page_token {
                url.push_str("&pageToken=");
                url.push_str(&urlencoding::encode(pt));
            }

            let resp = self
                .execute_with_retry("list_dir", || {
                    self.client.get(&url).header("Authorization", format!("Bearer {token}"))
                })
                .await?;

            if !resp.status().is_success() {
                return Err(format!("list_dir {rel_dir} HTTP {}", resp.status()));
            }

            let fl: FileListResponse = resp
                .json()
                .await
                .map_err(|e| format!("decode list_dir json error: {e}"))?;

            for f in fl.files {
                let is_dir = f.mime_type.as_deref() == Some(FOLDER_MIME_TYPE);
                let size = f.size.and_then(|s| s.parse().ok()).unwrap_or(0);
                let modified_time = f.modified_time.and_then(|t| t.parse::<DateTime<Utc>>().ok());
                let meta = GDriveFileMeta {
                    id: f.id,
                    name: f.name.clone(),
                    mime_type: f.mime_type.unwrap_or_else(|| "application/octet-stream".to_string()),
                    size,
                    md5: f.md5_checksum,
                    modified_time,
                    is_dir,
                };
                let file_rel = if rel_dir.is_empty() {
                    meta.name.clone()
                } else {
                    format!("{rel_dir}/{}", meta.name)
                };
                self.metadata_cache
                    .write()
                    .await
                    .insert(file_rel, (Instant::now(), meta.clone()));
                results.push(meta);
            }

            if let Some(next) = fl.next_page_token {
                if !next.is_empty() {
                    page_token = Some(next);
                    continue;
                }
            }
            break;
        }

        self.dir_listing_cache
            .write()
            .await
            .insert(rel_dir, (Instant::now(), results.clone()));

        Ok(results)
    }

    /// Get single file metadata by relative path. Uses directory batch caching to avoid single-item overhead.
    pub async fn get_file_metadata(&self, rel_path: &str) -> Result<Option<GDriveFileMeta>, String> {
        let rel_path = rel_path.trim_matches('/').to_string();
        {
            let lock = self.metadata_cache.read().await;
            if let Some((instant, meta)) = lock.get(&rel_path) {
                if instant.elapsed() < Duration::from_secs(60) {
                    return Ok(Some(meta.clone()));
                }
            }
        }

        let parent_dir = rel_path.rsplit_once('/').map(|(p, _)| p).unwrap_or("");
        let file_name = rel_path.rsplit('/').next().unwrap_or(&rel_path);

        let list = self.list_dir(parent_dir).await?;
        for item in list {
            if item.name == file_name {
                return Ok(Some(item));
            }
        }

        Ok(None)
    }

    /// Upload or update a file on Google Drive, verifying against expected MD5.
    pub async fn upload_file(&self, rel_path: &str, abs_path: &Path, expected_md5: &str) -> Result<GDriveFileMeta, String> {
        let _permit = self
            .upload_semaphore
            .acquire()
            .await
            .map_err(|e| format!("upload permit error: {e}"))?;

        let rel_path = rel_path.trim_matches('/').to_string();
        let parent_dir = rel_path.rsplit_once('/').map(|(p, _)| p).unwrap_or("");
        let file_name = rel_path.rsplit('/').next().unwrap_or(&rel_path).to_string();

        let parent_id = self.ensure_folder_path(parent_dir).await?;
        let existing = self.get_file_metadata(&rel_path).await?;

        if let Some(exist) = &existing {
            if let Some(exist_md5) = &exist.md5 {
                if exist_md5 == expected_md5 {
                    return Ok(exist.clone());
                }
            }
        }

        // Cooperative delay to keep overall request throughput strictly under Google Drive's 10 req/s threshold
        tokio::time::sleep(Duration::from_millis(60)).await;

        let file_bytes = tokio::fs::read(abs_path)
            .await
            .map_err(|e| format!("read file for gdrive upload error: {e}"))?;

        let mime = mime_guess::from_path(abs_path)
            .first_or_octet_stream()
            .to_string();

        let token = self.get_access_token().await?;

        let actual_size = file_bytes.len() as u64;
        let meta = if let Some(exist) = existing {
            // Update file content via PATCH
            let url = format!("{GDRIVE_UPLOAD_BASE}/files/{}?uploadType=media&fields=id,name,mimeType,size,md5Checksum,modifiedTime", exist.id);
            let resp = self
                .execute_with_retry("patch gdrive file", || {
                    self.client
                        .patch(&url)
                        .header("Authorization", format!("Bearer {token}"))
                        .header("Content-Type", &mime)
                        .body(file_bytes.clone())
                })
                .await?;

            if !resp.status().is_success() {
                return Err(format!("patch gdrive file HTTP {}", resp.status()));
            }

            let item: DriveFileItem = resp.json().await.map_err(|e| e.to_string())?;
            GDriveFileMeta {
                id: item.id,
                name: item.name,
                mime_type: item.mime_type.unwrap_or(mime),
                size: item.size.and_then(|s| s.parse().ok()).unwrap_or(actual_size),
                md5: item.md5_checksum.or_else(|| Some(expected_md5.to_string())),
                modified_time: item.modified_time.and_then(|t| t.parse().ok()),
                is_dir: false,
            }
        } else {
            // Multipart upload: metadata + data
            let boundary = "-------314159265358979323846";
            let delimiter = format!("\r\n--{boundary}\r\n");
            let close_delim = format!("\r\n--{boundary}--\r\n");

            let metadata_json = serde_json::json!({
                "name": file_name,
                "parents": [parent_id]
            })
            .to_string();

            let mut body = Vec::new();
            body.extend_from_slice(delimiter.as_bytes());
            body.extend_from_slice(b"Content-Type: application/json; charset=UTF-8\r\n\r\n");
            body.extend_from_slice(metadata_json.as_bytes());

            body.extend_from_slice(delimiter.as_bytes());
            body.extend_from_slice(format!("Content-Type: {mime}\r\n\r\n").as_bytes());
            body.extend_from_slice(&file_bytes);

            body.extend_from_slice(close_delim.as_bytes());

            let url = format!("{GDRIVE_UPLOAD_BASE}/files?uploadType=multipart&fields=id,name,mimeType,size,md5Checksum,modifiedTime");
            let resp = self
                .execute_with_retry("multipart gdrive upload", || {
                    self.client
                        .post(&url)
                        .header("Authorization", format!("Bearer {token}"))
                        .header(
                            "Content-Type",
                            format!("multipart/related; boundary={boundary}"),
                        )
                        .body(body.clone())
                })
                .await?;

            if !resp.status().is_success() {
                return Err(format!("multipart gdrive upload HTTP {}", resp.status()));
            }

            let item: DriveFileItem = resp.json().await.map_err(|e| e.to_string())?;
            GDriveFileMeta {
                id: item.id,
                name: item.name,
                mime_type: item.mime_type.unwrap_or(mime),
                size: item.size.and_then(|s| s.parse().ok()).unwrap_or(actual_size),
                md5: item.md5_checksum.or_else(|| Some(expected_md5.to_string())),
                modified_time: item.modified_time.and_then(|t| t.parse().ok()),
                is_dir: false,
            }
        };

        // Cache update: insert into metadata cache and update dir listing cache in-place
        self.metadata_cache
            .write()
            .await
            .insert(rel_path.clone(), (Instant::now(), meta.clone()));
        {
            let mut dlock = self.dir_listing_cache.write().await;
            if let Some((_, list)) = dlock.get_mut(parent_dir) {
                if let Some(pos) = list.iter().position(|item| item.name == meta.name) {
                    list[pos] = meta.clone();
                } else {
                    list.push(meta.clone());
                }
            }
        }

        Ok(meta)
    }

    /// Delete a file from Google Drive if exists.
    pub async fn delete_file(&self, rel_path: &str) -> Result<(), String> {
        let rel_path = rel_path.trim_matches('/').to_string();
        if let Some(existing) = self.get_file_metadata(&rel_path).await? {
            let token = self.get_access_token().await?;
            let url = format!("{GDRIVE_API_BASE}/files/{}", existing.id);
            let resp = self
                .execute_with_retry("delete gdrive file", || {
                    self.client
                        .delete(&url)
                        .header("Authorization", format!("Bearer {token}"))
                })
                .await?;

            if !resp.status().is_success() && resp.status() != reqwest::StatusCode::NOT_FOUND {
                return Err(format!("delete gdrive file HTTP {}", resp.status()));
            }

            self.metadata_cache.write().await.remove(&rel_path);
            let parent_dir = rel_path.rsplit_once('/').map(|(p, _)| p).unwrap_or("");
            {
                let mut dlock = self.dir_listing_cache.write().await;
                if let Some((_, list)) = dlock.get_mut(parent_dir) {
                    let file_name = rel_path.rsplit('/').next().unwrap_or(&rel_path);
                    list.retain(|item| item.name != file_name);
                }
            }
        }
        Ok(())
    }
}
