//! Content-addressed hardlink deduplication with kernel `st_nlink` reference counting.
//!
//! Layout:
//!   `<root>/.ostore/cas/<sha256[0..2]>/<sha256>`
//!
//! Every non-empty committed object is hardlinked to its SHA-256 entry in `.ostore/cas/`.
//! Because `ostore` never mutates files in-place (every write streams to a unique temp
//! file and atomically `rename`s it into place), sharing an inode across identical object
//! paths is completely safe:
//!
//! - `nlink == 1`: only the CAS anchor remains (all object paths deleted/overwritten) -> GC target.
//! - `nlink == 2`: 1 live object path + CAS anchor.
//! - `nlink >= 3`: `nlink - 1` live object paths sharing 1 physical copy (`nlink - 2` copies saved).
//!
//! On filesystems where hard links are unsupported or an inode hits `EMLINK`, operations
//! degrade gracefully to an ordinary atomic rename without failing the write.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::SystemTime;

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

use serde::Serialize;
use tokio::sync::RwLock;

use crate::ChecksumCache;
use crate::paths::META_DIR;

const CAS_DIR: &str = "cas";

#[derive(Debug, Clone, Default, Serialize)]
pub struct DedupStats {
    /// Total canonical blobs currently anchored in `.ostore/cas/`.
    pub cas_objects: u64,
    /// Total unique physical bytes stored in `.ostore/cas/`.
    pub cas_bytes: u64,
    /// Number of CAS blobs shared by 2 or more live object paths (`nlink >= 3`).
    pub shared_objects: u64,
    /// Total extra object path references deduplicated (`sum(nlink - 2)` for shared blobs).
    pub extra_references: u64,
    /// Physical bytes currently saved by hardlink deduplication.
    pub reclaimed_bytes: u64,
    /// Unreferenced CAS blobs (`nlink <= 1`) awaiting garbage collection.
    pub orphaned_objects: u64,
    /// Bytes held by orphaned CAS blobs.
    pub orphaned_bytes: u64,
    /// Cumulative online write-time dedup hits since process start.
    pub online_hits: u64,
    /// Cumulative bytes saved on write since process start.
    pub online_bytes_saved: u64,
    /// Cumulative CAS entries removed by GC since process start.
    pub gc_removed: u64,
    /// Cumulative bytes freed by GC since process start.
    pub gc_bytes_freed: u64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ScanReport {
    pub scanned_files: u64,
    pub anchored_new: u64,
    pub already_linked: u64,
    pub deduplicated: u64,
    pub bytes_saved: u64,
    pub errors: u64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct GcReport {
    pub removed: u64,
    pub bytes_freed: u64,
}

pub struct DedupStore {
    root: PathBuf,
    /// Shared read lock for linking/anchoring; exclusive write lock for GC unlink.
    lock: RwLock<()>,
    scan_in_progress: AtomicBool,
    pub online_hits: AtomicU64,
    pub online_bytes_saved: AtomicU64,
    pub gc_removed: AtomicU64,
    pub gc_bytes_freed: AtomicU64,
}

impl DedupStore {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            lock: RwLock::new(()),
            scan_in_progress: AtomicBool::new(false),
            online_hits: AtomicU64::new(0),
            online_bytes_saved: AtomicU64::new(0),
            gc_removed: AtomicU64::new(0),
            gc_bytes_freed: AtomicU64::new(0),
        }
    }

    pub fn cas_root(&self) -> PathBuf {
        self.root.join(META_DIR).join(CAS_DIR)
    }

    fn cas_path(&self, sha256: &str) -> Option<PathBuf> {
        if sha256.len() != 64 || !sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        Some(self.cas_root().join(&sha256[..2]).join(sha256))
    }

    /// Commit a freshly written and fsync'd temp file `tmp` to destination `abs`
    /// with online content-addressed hardlink deduplication.
    ///
    /// Returns `Ok(true)` if `abs` was deduplicated against an existing CAS blob,
    /// or `Ok(false)` if committed as a new unique blob.
    pub async fn commit_temp(
        &self,
        tmp: &Path,
        abs: &Path,
        size: u64,
        sha256: &str,
    ) -> std::io::Result<bool> {
        // Empty files consume zero data blocks; skip CAS to avoid hitting link limits.
        if size == 0 {
            tokio::fs::rename(tmp, abs).await?;
            return Ok(false);
        }
        let Some(cas_path) = self.cas_path(sha256) else {
            tokio::fs::rename(tmp, abs).await?;
            return Ok(false);
        };

        let _guard = self.lock.read().await;

        // Fast path: CAS blob already exists.
        if let Some(deduped) = self.try_link_existing(&cas_path, tmp, abs, size).await? {
            if deduped {
                self.online_hits.fetch_add(1, Ordering::Relaxed);
                self.online_bytes_saved.fetch_add(size, Ordering::Relaxed);
                return Ok(true);
            }
        }

        // Slow path: anchor `tmp` into CAS, then atomically rename `tmp` -> `abs`.
        if let Some(parent) = cas_path.parent() {
            let _ = tokio::fs::create_dir_all(parent).await;
        }
        match tokio::fs::hard_link(tmp, &cas_path).await {
            Ok(()) => {
                tokio::fs::rename(tmp, abs).await?;
                Ok(false)
            }
            Err(e) if e.kind() == ErrorKind::AlreadyExists => {
                // Lost a race with a concurrent writer of the same hash; link to theirs.
                if let Some(true) = self.try_link_existing(&cas_path, tmp, abs, size).await? {
                    self.online_hits.fetch_add(1, Ordering::Relaxed);
                    self.online_bytes_saved.fetch_add(size, Ordering::Relaxed);
                    return Ok(true);
                }
                tokio::fs::rename(tmp, abs).await?;
                Ok(false)
            }
            Err(_) => {
                // Filesystem does not support hard links or hit limit; graceful fallback.
                tokio::fs::rename(tmp, abs).await?;
                Ok(false)
            }
        }
    }

    /// Attempt to publish `abs` as a hardlink to `cas_path` and remove `tmp`.
    /// Returns `Ok(Some(true))` on dedup success, `Ok(Some(false))` if `cas_path`
    /// is missing/mismatched or link limit was hit, `Err` only if final rename failed.
    async fn try_link_existing(
        &self,
        cas_path: &Path,
        tmp: &Path,
        abs: &Path,
        expected_size: u64,
    ) -> std::io::Result<Option<bool>> {
        let cas_meta = match tokio::fs::metadata(cas_path).await {
            Ok(m) if m.is_file() && m.len() == expected_size => m,
            _ => return Ok(None),
        };
        let _ = cas_meta;
        let tmp_link = PathBuf::from(format!("{}.link", tmp.display()));
        match tokio::fs::hard_link(cas_path, &tmp_link).await {
            Ok(()) => {
                if let Err(e) = tokio::fs::rename(&tmp_link, abs).await {
                    let _ = tokio::fs::remove_file(&tmp_link).await;
                    return Err(e);
                }
                let _ = tokio::fs::remove_file(tmp).await;
                // Refresh mtime to now so incremental sync tools (e.g. `aws s3 sync`)
                // never treat a newly uploaded deduplicated object as older than local.
                touch_mtime(abs, SystemTime::now());
                Ok(Some(true))
            }
            Err(_) => {
                let _ = tokio::fs::remove_file(&tmp_link).await;
                Ok(Some(false))
            }
        }
    }

    /// Opportunistically reclaim a single CAS blob if its link count dropped to 1
    /// (called after deleting a file whose sha256 was already known in cache).
    pub async fn maybe_gc_hash(&self, sha256: &str) -> (u64, u64) {
        #[cfg(not(unix))]
        {
            let _ = sha256;
            return (0, 0);
        }
        #[cfg(unix)]
        {
            let Some(cas_path) = self.cas_path(sha256) else {
                return (0, 0);
            };
            let _guard = self.lock.write().await;
            let Ok(meta) = tokio::fs::symlink_metadata(&cas_path).await else {
                return (0, 0);
            };
            if meta.is_file() && meta.nlink() <= 1 {
                let size = meta.len();
                if tokio::fs::remove_file(&cas_path).await.is_ok() {
                    self.gc_removed.fetch_add(1, Ordering::Relaxed);
                    self.gc_bytes_freed.fetch_add(size, Ordering::Relaxed);
                    return (1, size);
                }
            }
            (0, 0)
        }
    }

    /// Garbage-collect all orphaned CAS blobs (`nlink <= 1`).
    pub async fn gc(&self) -> GcReport {
        #[cfg(not(unix))]
        {
            return GcReport::default();
        }
        #[cfg(unix)]
        {
            let cas_root = self.cas_root();
            let mut report = GcReport::default();
            let Ok(mut buckets) = tokio::fs::read_dir(&cas_root).await else {
                return report;
            };
            while let Ok(Some(bucket)) = buckets.next_entry().await {
                let bpath = bucket.path();
                if !bucket.file_type().await.map(|t| t.is_dir()).unwrap_or(false) {
                    continue;
                }
                let Ok(mut entries) = tokio::fs::read_dir(&bpath).await else {
                    continue;
                };
                let mut remaining = 0usize;
                while let Ok(Some(ent)) = entries.next_entry().await {
                    let epath = ent.path();
                    let Ok(meta) = tokio::fs::symlink_metadata(&epath).await else {
                        continue;
                    };
                    if meta.is_file() && meta.nlink() <= 1 {
                        let _guard = self.lock.write().await;
                        // Re-check under exclusive write lock so a concurrent upload cannot race.
                        if let Ok(m2) = tokio::fs::symlink_metadata(&epath).await {
                            if m2.is_file() && m2.nlink() <= 1 {
                                let sz = m2.len();
                                if tokio::fs::remove_file(&epath).await.is_ok() {
                                    report.removed += 1;
                                    report.bytes_freed += sz;
                                    continue;
                                }
                            }
                        }
                    }
                    remaining += 1;
                }
                if remaining == 0 {
                    let _ = tokio::fs::remove_dir(&bpath).await;
                }
            }
            if report.removed > 0 {
                self.gc_removed.fetch_add(report.removed, Ordering::Relaxed);
                self.gc_bytes_freed.fetch_add(report.bytes_freed, Ordering::Relaxed);
            }
            report
        }
    }

    /// Inspect current CAS deduplication state directly from kernel `st_nlink` counts.
    pub async fn stats(&self) -> DedupStats {
        let mut s = DedupStats {
            online_hits: self.online_hits.load(Ordering::Relaxed),
            online_bytes_saved: self.online_bytes_saved.load(Ordering::Relaxed),
            gc_removed: self.gc_removed.load(Ordering::Relaxed),
            gc_bytes_freed: self.gc_bytes_freed.load(Ordering::Relaxed),
            ..Default::default()
        };
        #[cfg(unix)]
        {
            let cas_root = self.cas_root();
            let Ok(mut buckets) = tokio::fs::read_dir(&cas_root).await else {
                return s;
            };
            while let Ok(Some(bucket)) = buckets.next_entry().await {
                if !bucket.file_type().await.map(|t| t.is_dir()).unwrap_or(false) {
                    continue;
                }
                let Ok(mut entries) = tokio::fs::read_dir(bucket.path()).await else {
                    continue;
                };
                while let Ok(Some(ent)) = entries.next_entry().await {
                    let Ok(meta) = tokio::fs::symlink_metadata(ent.path()).await else {
                        continue;
                    };
                    if !meta.is_file() {
                        continue;
                    }
                    let nlink = meta.nlink();
                    let sz = meta.len();
                    s.cas_objects += 1;
                    s.cas_bytes += sz;
                    if nlink <= 1 {
                        s.orphaned_objects += 1;
                        s.orphaned_bytes += sz;
                    } else if nlink >= 3 {
                        let extra = nlink - 2;
                        s.shared_objects += 1;
                        s.extra_references += extra;
                        s.reclaimed_bytes += sz.saturating_mul(extra);
                    }
                }
            }
        }
        s
    }

    /// Retroactively scan `OSTORE_ROOT` and hardlink-deduplicate all existing files
    /// with identical SHA-256 hashes into `.ostore/cas/`.
    pub async fn scan_and_dedup(
        &self,
        checksum_cache: &ChecksumCache,
    ) -> Result<ScanReport, &'static str> {
        if self
            .scan_in_progress
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err("dedup scan already in progress");
        }
        struct Reset<'a>(&'a AtomicBool);
        impl Drop for Reset<'_> {
            fn drop(&mut self) {
                self.0.store(false, Ordering::SeqCst);
            }
        }
        let _reset = Reset(&self.scan_in_progress);

        let mut report = ScanReport::default();
        let mut stack = vec![self.root.clone()];

        while let Some(dir) = stack.pop() {
            let Ok(mut rd) = tokio::fs::read_dir(&dir).await else {
                continue;
            };
            while let Ok(Some(ent)) = rd.next_entry().await {
                let name = ent.file_name().to_string_lossy().into_owned();
                if name == META_DIR || name.contains(crate::TMP_MARKER) {
                    continue;
                }
                let path = ent.path();
                let Ok(meta) = tokio::fs::symlink_metadata(&path).await else {
                    continue;
                };
                if meta.file_type().is_symlink() {
                    continue;
                }
                if meta.is_dir() {
                    stack.push(path);
                    continue;
                }
                if !meta.is_file() {
                    continue;
                }
                report.scanned_files += 1;
                let size = meta.len();
                if size == 0 {
                    continue;
                }
                let orig_mtime = meta.modified().ok();
                let (md5, sha256) = match checksum_cache.get_or_compute_both(&path, &meta).await {
                    Ok(v) => v,
                    Err(_) => {
                        report.errors += 1;
                        continue;
                    }
                };
                let Some(cas_path) = self.cas_path(&sha256) else {
                    continue;
                };

                let _guard = self.lock.read().await;
                if let Ok(cas_meta) = tokio::fs::symlink_metadata(&cas_path).await {
                    #[cfg(unix)]
                    if cas_meta.dev() == meta.dev() && cas_meta.ino() == meta.ino() {
                        report.already_linked += 1;
                        continue;
                    }
                    if cas_meta.is_file() && cas_meta.len() == size {
                        // Verify file hasn't changed while hashing.
                        let Ok(cur_meta) = tokio::fs::symlink_metadata(&path).await else {
                            continue;
                        };
                        if cur_meta.len() != size || cur_meta.modified().ok() != orig_mtime {
                            continue;
                        }
                        let tmp_link = PathBuf::from(format!(
                            "{}{}dedup-{}",
                            path.display(),
                            crate::TMP_MARKER,
                            std::process::id()
                        ));
                        match tokio::fs::hard_link(&cas_path, &tmp_link).await {
                            Ok(()) => {
                                if tokio::fs::rename(&tmp_link, &path).await.is_ok() {
                                    // Preserve the newer mtime so sync clients never see mtime go backwards.
                                    if let (Some(a), Some(b)) = (orig_mtime, cas_meta.modified().ok()) {
                                        if a > b {
                                            touch_mtime(&path, a);
                                        }
                                    }
                                    if let Ok(new_m) = tokio::fs::metadata(&path).await {
                                        let mtime_ns = new_m
                                            .modified()
                                            .ok()
                                            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                                            .map(|d| d.as_nanos())
                                            .unwrap_or(0);
                                        checksum_cache.insert_both(&path, size, mtime_ns, md5, sha256).await;
                                    }
                                    report.deduplicated += 1;
                                    report.bytes_saved += size;
                                } else {
                                    let _ = tokio::fs::remove_file(&tmp_link).await;
                                    report.errors += 1;
                                }
                            }
                            Err(_) => {
                                let _ = tokio::fs::remove_file(&tmp_link).await;
                                report.errors += 1;
                            }
                        }
                    }
                } else {
                    if let Some(parent) = cas_path.parent() {
                        let _ = tokio::fs::create_dir_all(parent).await;
                    }
                    match tokio::fs::hard_link(&path, &cas_path).await {
                        Ok(()) => {
                            report.anchored_new += 1;
                        }
                        Err(e) if e.kind() == ErrorKind::AlreadyExists => {
                            report.already_linked += 1;
                        }
                        Err(_) => {
                            report.errors += 1;
                        }
                    }
                }
            }
        }

        Ok(report)
    }
}

fn touch_mtime(path: &Path, t: SystemTime) {
    if let Ok(f) = std::fs::File::open(path) {
        let _ = f.set_modified(t);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    fn sha_hex(data: &[u8]) -> String {
        hex::encode(Sha256::digest(data))
    }

    #[tokio::test]
    async fn online_dedup_shares_inode_and_gc_reclaims() {
        let dir = tempfile::tempdir().unwrap();
        let store = DedupStore::new(dir.path().to_path_buf());
        let data = b"identical backup payload across two hosts";
        let sha = sha_hex(data);

        let tmp1 = dir.path().join("a.bin.ostore-tmp-1");
        let dst1 = dir.path().join("a.bin");
        tokio::fs::write(&tmp1, data).await.unwrap();
        assert!(!store.commit_temp(&tmp1, &dst1, data.len() as u64, &sha).await.unwrap());

        let tmp2 = dir.path().join("b.bin.ostore-tmp-2");
        let dst2 = dir.path().join("b.bin");
        tokio::fs::write(&tmp2, data).await.unwrap();
        assert!(store.commit_temp(&tmp2, &dst2, data.len() as u64, &sha).await.unwrap());

        #[cfg(unix)]
        {
            let m1 = tokio::fs::metadata(&dst1).await.unwrap();
            let m2 = tokio::fs::metadata(&dst2).await.unwrap();
            assert_eq!(m1.ino(), m2.ino());
            assert_eq!(m1.nlink(), 3); // a.bin + b.bin + cas
        }

        let stats = store.stats().await;
        assert_eq!(stats.cas_objects, 1);
        assert_eq!(stats.shared_objects, 1);
        assert_eq!(stats.extra_references, 1);
        assert_eq!(stats.reclaimed_bytes, data.len() as u64);
        assert_eq!(stats.online_hits, 1);

        // Overwrite a.bin with new data -> b.bin must remain untouched!
        let new_data = b"modified host a file";
        let new_sha = sha_hex(new_data);
        let tmp3 = dir.path().join("a.bin.ostore-tmp-3");
        tokio::fs::write(&tmp3, new_data).await.unwrap();
        assert!(!store.commit_temp(&tmp3, &dst1, new_data.len() as u64, &new_sha).await.unwrap());
        assert_eq!(tokio::fs::read(&dst1).await.unwrap(), new_data);
        assert_eq!(tokio::fs::read(&dst2).await.unwrap(), data);

        // Delete b.bin -> original CAS blob becomes orphaned (nlink == 1) and GC removes it.
        tokio::fs::remove_file(&dst2).await.unwrap();
        let gc = store.gc().await;
        assert_eq!(gc.removed, 1);
        assert_eq!(gc.bytes_freed, data.len() as u64);
        assert_eq!(store.stats().await.cas_objects, 1);
    }

    #[tokio::test]
    async fn scan_deduplicates_preexisting_files() {
        let dir = tempfile::tempdir().unwrap();
        let store = DedupStore::new(dir.path().to_path_buf());
        let cache = ChecksumCache::new();
        let data = b"pre-existing duplicate blob content";

        let p1 = dir.path().join("host1/blob.bin");
        let p2 = dir.path().join("host2/blob.bin");
        tokio::fs::create_dir_all(p1.parent().unwrap()).await.unwrap();
        tokio::fs::create_dir_all(p2.parent().unwrap()).await.unwrap();
        tokio::fs::write(&p1, data).await.unwrap();
        tokio::fs::write(&p2, data).await.unwrap();

        let report = store.scan_and_dedup(&cache).await.unwrap();
        assert_eq!(report.scanned_files, 2);
        assert_eq!(report.anchored_new, 1);
        assert_eq!(report.deduplicated, 1);
        assert_eq!(report.bytes_saved, data.len() as u64);

        #[cfg(unix)]
        {
            let m1 = tokio::fs::metadata(&p1).await.unwrap();
            let m2 = tokio::fs::metadata(&p2).await.unwrap();
            assert_eq!(m1.ino(), m2.ino());
            assert_eq!(m1.nlink(), 3);
        }
    }
}
