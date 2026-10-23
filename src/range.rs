//! HTTP Range header parsing (`Range: bytes=...`).
//!
//! Supports single ranges only: `bytes=start-end`, `bytes=start-`,
//! `bytes=-suffix`. Returns inclusive `(start, end)` byte offsets, clamped
//! to `size - 1`. Malformed or unsatisfiable ranges yield `Err(())` which
//! callers map to 416. A header with a non-bytes unit is ignored (`Ok(None)`).

/// Parse a Range header value against an object of `size` bytes.
///
/// - `Ok(None)` — no bytes range requested (ignore header, send full body)
/// - `Ok(Some((start, end)))` — satisfiable inclusive byte range
/// - `Err(())` — malformed or unsatisfiable (respond 416)
pub fn parse_range(header: &str, size: u64) -> Result<Option<(u64, u64)>, ()> {
    let h = header.trim();
    let Some(rest) = h.strip_prefix("bytes=") else {
        return Ok(None); // unknown unit: ignore per RFC 9110
    };
    // Only single ranges are supported; extra ranges are ignored.
    let first = rest.split(',').next().ok_or(())?.trim();
    let (a, b) = first.split_once('-').ok_or(())?;
    let a = a.trim();
    let b = b.trim();
    if size == 0 {
        return Err(());
    }
    if a.is_empty() {
        // Suffix range: last N bytes.
        let n: u64 = b.parse().map_err(|_| ())?;
        if n == 0 {
            return Err(());
        }
        let start = size.saturating_sub(n);
        Ok(Some((start, size - 1)))
    } else {
        let start: u64 = a.parse().map_err(|_| ())?;
        let end: u64 = if b.is_empty() {
            size - 1
        } else {
            b.parse().map_err(|_| ())?
        };
        if start > end || start >= size {
            return Err(());
        }
        Ok(Some((start, end.min(size - 1))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_end() {
        assert_eq!(parse_range("bytes=0-99", 1000), Ok(Some((0, 99))));
        assert_eq!(parse_range("bytes=100-199", 1000), Ok(Some((100, 199))));
    }

    #[test]
    fn open_end() {
        assert_eq!(parse_range("bytes=100-", 1000), Ok(Some((100, 999))));
        assert_eq!(parse_range("bytes=0-", 1000), Ok(Some((0, 999))));
    }

    #[test]
    fn suffix() {
        assert_eq!(parse_range("bytes=-200", 1000), Ok(Some((800, 999))));
        // Suffix larger than the object clamps to the whole object.
        assert_eq!(parse_range("bytes=-5000", 1000), Ok(Some((0, 999))));
    }

    #[test]
    fn end_clamped_to_eof() {
        assert_eq!(parse_range("bytes=0-9999", 1000), Ok(Some((0, 999))));
        assert_eq!(parse_range("bytes=999-9999", 1000), Ok(Some((999, 999))));
    }

    #[test]
    fn beyond_eof_is_unsatisfiable() {
        assert_eq!(parse_range("bytes=1000-2000", 1000), Err(()));
        assert_eq!(parse_range("bytes=5000-", 1000), Err(()));
    }

    #[test]
    fn inverted_range_is_invalid() {
        assert_eq!(parse_range("bytes=20-10", 1000), Err(()));
    }

    #[test]
    fn malformed_is_invalid() {
        assert_eq!(parse_range("bytes=abc", 1000), Err(()));
        assert_eq!(parse_range("bytes=", 1000), Err(()));
        assert_eq!(parse_range("bytes=-", 1000), Err(()));
        assert_eq!(parse_range("bytes=-0", 1000), Err(()));
        assert_eq!(parse_range("bytes=1-2-3", 1000), Err(()));
    }

    #[test]
    fn empty_object_is_unsatisfiable() {
        assert_eq!(parse_range("bytes=0-0", 0), Err(()));
        assert_eq!(parse_range("bytes=-1", 0), Err(()));
    }

    #[test]
    fn non_bytes_unit_is_ignored() {
        assert_eq!(parse_range("items=0-10", 1000), Ok(None));
    }

    #[test]
    fn extra_ranges_are_ignored() {
        assert_eq!(parse_range("bytes=0-9,20-29", 1000), Ok(Some((0, 9))));
    }
}
