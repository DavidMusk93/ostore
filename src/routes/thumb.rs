//! GET /api/thumb/{*path}?w=<px> — JPEG thumbnail of an image object.
//!
//! The image is fitted inside a `w`×`w` box (aspect ratio preserved, never
//! cropped or upscaled), EXIF orientation applied, transparency flattened onto
//! white. Widths snap to 128/256/512/1024 so the cache stays small. Results are
//! cached under `<root>/.ostore/thumbs/` keyed by path + size + mtime + width,
//! so an overwritten object gets a fresh thumbnail automatically; entries
//! untouched for 30 days are swept at startup. Decoding runs on the blocking
//! pool behind a semaphore, with dimension/allocation limits against
//! decompression bombs.

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::time::{Duration, SystemTime};

use axum::extract::{Path as UrlPath, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, ImageDecoder, ImageReader, Limits, RgbImage};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::AppState;
use crate::error::ApiError;
use crate::paths;

const WIDTHS: [u32; 4] = [128, 256, 512, 1024];
const MAX_SOURCE_BYTES: u64 = 64 * 1024 * 1024;
const JPEG_QUALITY: u8 = 82;
const THUMB_TTL: Duration = Duration::from_secs(30 * 86400);

/// Extensions the thumbnailer understands (lowercase, no dot).
pub const IMAGE_EXTS: [&str; 7] = ["jpg", "jpeg", "png", "gif", "webp", "bmp", "jfif"];

#[derive(Deserialize, Default)]
pub struct ThumbQuery {
    w: Option<u32>,
}

fn thumbs_dir(root: &Path) -> PathBuf {
    root.join(paths::META_DIR).join("thumbs")
}

fn snap_width(w: Option<u32>) -> u32 {
    let want = w.unwrap_or(256);
    WIDTHS.iter().copied().find(|s| *s >= want).unwrap_or(WIDTHS[WIDTHS.len() - 1])
}

fn is_image(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| IMAGE_EXTS.contains(&e.to_ascii_lowercase().as_str()))
}

fn render(src: &Path, width: u32) -> Result<Vec<u8>, String> {
    let mut reader = ImageReader::open(src)
        .map_err(|e| e.to_string())?
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(16_384);
    limits.max_image_height = Some(16_384);
    limits.max_alloc = Some(512 * 1024 * 1024);
    reader.limits(limits);
    let mut decoder = reader.into_decoder().map_err(|e| e.to_string())?;
    let orientation = decoder.orientation().map_err(|e| e.to_string())?;
    let mut img = DynamicImage::from_decoder(decoder).map_err(|e| e.to_string())?;
    img.apply_orientation(orientation);

    let img = if img.width() > width || img.height() > width { img.thumbnail(width, width) } else { img };
    let rgb: RgbImage = if img.color().has_alpha() {
        let rgba = img.to_rgba8();
        RgbImage::from_fn(rgba.width(), rgba.height(), |x, y| {
            let p = rgba.get_pixel(x, y).0;
            let a = p[3] as u32;
            let blend = |c: u8| ((c as u32 * a + 255 * (255 - a)) / 255) as u8;
            image::Rgb([blend(p[0]), blend(p[1]), blend(p[2])])
        })
    } else {
        img.to_rgb8()
    };
    let mut out = Cursor::new(Vec::with_capacity(32 * 1024));
    JpegEncoder::new_with_quality(&mut out, JPEG_QUALITY)
        .encode_image(&rgb)
        .map_err(|e| e.to_string())?;
    Ok(out.into_inner())
}

fn jpeg_response(bytes: Vec<u8>, etag: &str) -> Response {
    (
        [
            (header::CONTENT_TYPE, HeaderValue::from_static("image/jpeg")),
            (header::CACHE_CONTROL, HeaderValue::from_static("private, max-age=86400")),
            (header::ETAG, HeaderValue::from_str(etag).unwrap_or(HeaderValue::from_static("\"t\""))),
        ],
        bytes,
    )
        .into_response()
}

pub async fn thumb(
    State(st): State<AppState>,
    UrlPath(path): UrlPath<String>,
    Query(q): Query<ThumbQuery>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let (abs, rel) = paths::resolve_nonroot(&st.root, &path)?;
    let meta = tokio::fs::metadata(&abs).await.map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => ApiError::not_found(&rel),
        _ => ApiError::internal(e),
    })?;
    if meta.is_dir() || !is_image(&abs) {
        return Err(ApiError::new(StatusCode::UNSUPPORTED_MEDIA_TYPE, "NOT_AN_IMAGE", format!("{rel} has no thumbnail")));
    }
    if meta.len() > MAX_SOURCE_BYTES {
        return Err(ApiError::new(StatusCode::PAYLOAD_TOO_LARGE, "TOO_LARGE", format!("{rel} is too large to thumbnail")));
    }
    let width = snap_width(q.w);
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut h = Sha256::new();
    h.update(rel.as_bytes());
    h.update(meta.len().to_be_bytes());
    h.update(mtime.to_be_bytes());
    h.update(width.to_be_bytes());
    let key = hex::encode(h.finalize())[..32].to_string();
    let etag = format!("\"{key}\"");

    let not_modified = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.split(',').any(|t| t.trim() == etag));
    if not_modified {
        return Ok((StatusCode::NOT_MODIFIED, [(header::ETAG, HeaderValue::from_str(&etag).map_err(ApiError::internal)?)]).into_response());
    }

    let cached = thumbs_dir(&st.root).join(format!("{key}.jpg"));
    if let Ok(bytes) = tokio::fs::read(&cached).await {
        st.metrics.thumbs_hit.fetch_add(1, Ordering::Relaxed);
        return Ok(jpeg_response(bytes, &etag));
    }

    let _permit = st.thumb_permits.acquire().await.map_err(ApiError::internal)?;
    // Another request may have produced it while we waited for a permit.
    if let Ok(bytes) = tokio::fs::read(&cached).await {
        st.metrics.thumbs_hit.fetch_add(1, Ordering::Relaxed);
        return Ok(jpeg_response(bytes, &etag));
    }
    let src = abs.clone();
    let bytes = match tokio::task::spawn_blocking(move || render(&src, width)).await.map_err(ApiError::internal)? {
        Ok(b) => b,
        Err(e) => {
            st.metrics.thumbs_failed.fetch_add(1, Ordering::Relaxed);
            tracing::warn!(path = %rel, error = %e, "thumbnail failed");
            return Err(ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, "DECODE_FAILED", format!("cannot decode {rel}: {e}")));
        }
    };
    st.metrics.thumbs_generated.fetch_add(1, Ordering::Relaxed);
    let dir = thumbs_dir(&st.root);
    let tmp = dir.join(format!("{key}{}{}", crate::TMP_MARKER, std::process::id()));
    let write = async {
        tokio::fs::create_dir_all(&dir).await?;
        tokio::fs::write(&tmp, &bytes).await?;
        tokio::fs::rename(&tmp, &cached).await
    };
    if let Err(e) = write.await {
        tracing::warn!(error = %e, "thumbnail cache write failed");
        let _ = tokio::fs::remove_file(&tmp).await;
    }
    Ok(jpeg_response(bytes, &etag))
}

/// Remove cached thumbnails not regenerated for [`THUMB_TTL`] (orphans of
/// deleted/moved/overwritten objects). Returns files removed.
pub fn sweep_thumbs(root: &Path) -> usize {
    let Ok(rd) = std::fs::read_dir(thumbs_dir(root)) else { return 0 };
    let now = SystemTime::now();
    let mut removed = 0;
    for de in rd.flatten() {
        let stale = de
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| now.duration_since(t).ok())
            .is_some_and(|age| age > THUMB_TTL);
        let tmp = de.file_name().to_string_lossy().contains(crate::TMP_MARKER);
        if (stale || tmp) && std::fs::remove_file(de.path()).is_ok() {
            removed += 1;
        }
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widths_snap_up_and_cap() {
        assert_eq!(snap_width(None), 256);
        assert_eq!(snap_width(Some(1)), 128);
        assert_eq!(snap_width(Some(200)), 256);
        assert_eq!(snap_width(Some(5000)), 1024);
    }
}
