//! Path safety: resolve a client-supplied relative path to an absolute path
//! under the bound root, rejecting anything that could escape it.
//!
//! Rejected: absolute paths, `..` components, NUL bytes, and the reserved
//! `.ostore` meta name. `""`, `/` and `.` resolve to the root itself.
//! Note: axum percent-decodes path captures before they reach us, so
//! `%2e%2e%2f` arrives here as `../` and is rejected like any `..`.

use std::path::{Path, PathBuf};

use crate::error::ApiError;

/// Reserved meta directory name, never addressable through the API.
pub const META_DIR: &str = ".ostore";

/// Resolve `rel` against `root`. Returns `(absolute_path, normalized_rel)`
/// where `normalized_rel` uses `/` separators and is `""` for the root.
pub fn resolve(root: &Path, rel: &str) -> Result<(PathBuf, String), ApiError> {
    if rel.bytes().any(|b| b == 0) {
        return Err(ApiError::bad_path("NUL byte in path"));
    }
    let t = rel.trim();
    if t.is_empty() || t == "/" || t == "." {
        return Ok((root.to_path_buf(), String::new()));
    }
    if t.starts_with('/') {
        return Err(ApiError::bad_path("absolute paths are not allowed"));
    }
    let mut out = String::new();
    for seg in t.split('/') {
        match seg {
            "" | "." => continue,
            ".." => return Err(ApiError::bad_path("parent directory components are not allowed")),
            META_DIR => return Err(ApiError::bad_path(".ostore is reserved")),
            s => {
                if !out.is_empty() {
                    out.push('/');
                }
                out.push_str(s);
            }
        }
    }
    if out.is_empty() {
        return Ok((root.to_path_buf(), String::new()));
    }
    let abs = root.join(&out);
    // Belt-and-braces: after lexical normalization the result must stay under root.
    if !abs.starts_with(root) {
        return Err(ApiError::bad_path("path escapes root"));
    }
    Ok((abs, out))
}

/// Like [`resolve`] but rejects the root itself (for file operations).
pub fn resolve_nonroot(root: &Path, rel: &str) -> Result<(PathBuf, String), ApiError> {
    let (abs, norm) = resolve(root, rel)?;
    if norm.is_empty() {
        return Err(ApiError::bad_path("operation requires a path below the root"));
    }
    Ok((abs, norm))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> PathBuf {
        PathBuf::from("/srv/data")
    }

    fn rel_of(r: Result<(PathBuf, String), ApiError>) -> String {
        r.expect("expected path to resolve").1
    }

    #[test]
    fn rejects_parent_traversal() {
        assert!(resolve(&root(), "..").is_err());
        assert!(resolve(&root(), "../etc/passwd").is_err());
        assert!(resolve(&root(), "../../root").is_err());
        assert!(resolve(&root(), "a/../b").is_err());
        assert!(resolve(&root(), "a/../../b").is_err());
        // Percent-encoded traversal arrives decoded from axum: "../" here.
        assert!(resolve(&root(), "../evil").is_err());
    }

    #[test]
    fn rejects_absolute_paths() {
        assert!(resolve(&root(), "/abs").is_err());
        assert!(resolve(&root(), "/etc/passwd").is_err());
    }

    #[test]
    fn rejects_nul_bytes() {
        assert!(resolve(&root(), "a\0b").is_err());
        assert!(resolve(&root(), "\0").is_err());
    }

    #[test]
    fn rejects_reserved_meta_dir() {
        assert!(resolve(&root(), ".ostore").is_err());
        assert!(resolve(&root(), ".ostore/webhooks.json").is_err());
        assert!(resolve(&root(), "a/.ostore/x").is_err());
    }

    #[test]
    fn root_aliases_resolve_to_root() {
        for alias in ["", "/", "."] {
            let (abs, rel) = resolve(&root(), alias).expect("root alias");
            assert_eq!(abs, root());
            assert_eq!(rel, "");
        }
    }

    #[test]
    fn normalizes_dot_and_empty_segments() {
        assert_eq!(rel_of(resolve(&root(), "a/./b")), "a/b");
        assert_eq!(rel_of(resolve(&root(), "a//b")), "a/b");
        assert_eq!(rel_of(resolve(&root(), "./a/b/")), "a/b");
        assert_eq!(rel_of(resolve(&root(), "a/./")), "a");
    }

    #[test]
    fn resolves_nested_paths() {
        let (abs, rel) = resolve(&root(), "videos/2026/clip.mp4").expect("nested");
        assert_eq!(abs, PathBuf::from("/srv/data/videos/2026/clip.mp4"));
        assert_eq!(rel, "videos/2026/clip.mp4");
    }

    #[test]
    fn nonroot_rejects_root() {
        assert!(resolve_nonroot(&root(), "").is_err());
        assert!(resolve_nonroot(&root(), "/").is_err());
        assert!(resolve_nonroot(&root(), ".").is_err());
        assert!(resolve_nonroot(&root(), "a").is_ok());
    }
}
