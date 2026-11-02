//! Streaming directory archive download (POSIX ustar tar format).
//!
//! Streams files directly from disk without creating a temporary archive on disk,
//! enabling instant downloads of directories regardless of size.

use axum::body::{Body, Bytes};
use axum::extract::{Query, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::Response;
use serde::Deserialize;
use tokio::io::AsyncReadExt;

use crate::AppState;
use crate::error::ApiError;
use crate::paths;

#[derive(Deserialize, Default)]
pub struct ArchiveQuery {
    #[serde(default)]
    pub path: String,
}

fn ustar_header(name: &str, size: u64, mtime: u64, is_dir: bool) -> [u8; 512] {
    let mut header = [0u8; 512];

    // 0..100: name
    let name_bytes = name.as_bytes();
    let name_len = name_bytes.len().min(100);
    header[..name_len].copy_from_slice(&name_bytes[..name_len]);

    // 100..108: mode (8 octal bytes with NUL)
    let mode = if is_dir { b"0000755\0" } else { b"0000644\0" };
    header[100..108].copy_from_slice(mode);

    // 108..116: uid, 116..124: gid
    header[108..116].copy_from_slice(b"0000000\0");
    header[116..124].copy_from_slice(b"0000000\0");

    // 124..136: size (11 octal digits + NUL)
    let size_octal = if is_dir {
        format!("{:011o}\0", 0)
    } else {
        format!("{:011o}\0", size)
    };
    let size_bytes = size_octal.as_bytes();
    let copy_len = size_bytes.len().min(12);
    header[124..124 + copy_len].copy_from_slice(&size_bytes[..copy_len]);

    // 136..148: mtime (11 octal digits + NUL)
    let mtime_octal = format!("{:011o}\0", mtime);
    let mtime_bytes = mtime_octal.as_bytes();
    let mtime_len = mtime_bytes.len().min(12);
    header[136..136 + mtime_len].copy_from_slice(&mtime_bytes[..mtime_len]);

    // 156: typeflag ('5' for dir, '0' for regular file)
    header[156] = if is_dir { b'5' } else { b'0' };

    // 257..263: magic ("ustar\0"), 263..265: version ("00")
    header[257..263].copy_from_slice(b"ustar\0");
    header[263..265].copy_from_slice(b"00");

    // 148..156: chksum (sum treated as spaces initially)
    header[148..156].copy_from_slice(b"        ");
    let sum: u32 = header.iter().map(|&b| b as u32).sum();
    let chksum_octal = format!("{:06o}\0 ", sum);
    let chk_bytes = chksum_octal.as_bytes();
    let chk_len = chk_bytes.len().min(8);
    header[148..148 + chk_len].copy_from_slice(&chk_bytes[..chk_len]);

    header
}

/// GET /api/archive?path=<dir_rel> — download directory as streaming tar archive.
pub async fn download_archive(
    State(st): State<AppState>,
    Query(q): Query<ArchiveQuery>,
) -> Result<Response, ApiError> {
    let (abs, rel) = paths::resolve(&st.root, &q.path)?;
    let meta = tokio::fs::metadata(&abs)
        .await
        .map_err(|_| ApiError::not_found(if rel.is_empty() { "/" } else { &rel }))?;
    if !meta.is_dir() {
        return Err(ApiError::bad_request("NOT_A_DIR", format!("{rel} is not a directory")));
    }

    let dir_name = if rel.is_empty() {
        "root-archive".to_string()
    } else {
        rel.rsplit('/').next().unwrap_or("archive").to_string()
    };

    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Bytes, std::io::Error>>(16);

    let abs_clone = abs.clone();
    let rel_clone = rel.clone();

    tokio::spawn(async move {
        let mut stack = vec![(abs_clone, rel_clone)];
        while let Some((dir_abs, dir_rel)) = stack.pop() {
            let Ok(mut rd) = tokio::fs::read_dir(&dir_abs).await else { continue };
            while let Ok(Some(entry)) = rd.next_entry().await {
                let name = entry.file_name().to_string_lossy().to_string();
                if crate::routes::list::hidden(&name) {
                    continue;
                }
                let child_abs = entry.path();
                let child_rel = if dir_rel.is_empty() {
                    name.clone()
                } else {
                    format!("{dir_rel}/{name}")
                };

                let Ok(m) = entry.metadata().await else { continue };
                let mtime_sec = m
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0);

                if m.is_dir() {
                    let dir_tar_name = format!("{child_rel}/");
                    let header = ustar_header(&dir_tar_name, 0, mtime_sec, true);
                    if tx.send(Ok(Bytes::copy_from_slice(&header))).await.is_err() {
                        return;
                    }
                    stack.push((child_abs, child_rel));
                } else if m.is_file() {
                    let file_size = m.len();
                    let header = ustar_header(&child_rel, file_size, mtime_sec, false);
                    if tx.send(Ok(Bytes::copy_from_slice(&header))).await.is_err() {
                        return;
                    }

                    if let Ok(mut f) = tokio::fs::File::open(&child_abs).await {
                        let mut buf = vec![0u8; 64 * 1024];
                        let mut bytes_sent = 0u64;
                        while bytes_sent < file_size {
                            let to_read = (file_size - bytes_sent).min(buf.len() as u64) as usize;
                            match f.read(&mut buf[..to_read]).await {
                                Ok(0) => break,
                                Ok(n) => {
                                    bytes_sent += n as u64;
                                    if tx.send(Ok(Bytes::copy_from_slice(&buf[..n]))).await.is_err() {
                                        return;
                                    }
                                }
                                Err(e) => {
                                    let _ = tx.send(Err(e)).await;
                                    return;
                                }
                            }
                        }

                        // Pad file data to 512 bytes boundary
                        let remainder = (file_size % 512) as usize;
                        if remainder > 0 {
                            let pad = 512 - remainder;
                            let zeros = vec![0u8; pad];
                            if tx.send(Ok(Bytes::from(zeros))).await.is_err() {
                                return;
                            }
                        }
                    }
                }
            }
        }

        // Two 512-byte zero blocks signify the end of the archive
        let end_blocks = vec![0u8; 1024];
        let _ = tx.send(Ok(Bytes::from(end_blocks))).await;
    });

    let stream = tokio_stream::wrappers::ReceiverStream::new(rx);
    let disposition = format!("attachment; filename=\"{dir_name}.tar\"");

    let resp = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/x-tar")
        .header(header::CONTENT_DISPOSITION, HeaderValue::from_str(&disposition).unwrap_or_else(|_| HeaderValue::from_static("attachment; filename=\"archive.tar\"")))
        .body(Body::from_stream(stream))
        .map_err(ApiError::internal)?;

    Ok(resp)
}
