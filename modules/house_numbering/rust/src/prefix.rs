//! Exact Rust port of `modules/house_numbering/address_prefix.py` non-lossy alley-prefix syntax.
//!
//! Grammar: `_SEGMENT = (?:[A-Z]+[1-9][0-9]*[A-Z]*|[1-9][0-9]*[A-Z]*)`
//!          `_PREFIX  = _SEGMENT(?:/_SEGMENT)*`, ASCII only, length 1..=100.

use ovg_core::AppError;

fn is_valid_segment(seg: &str) -> bool {
    let bytes = seg.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    let mut i = 0;
    // Optional leading ASCII uppercase letters
    while i < bytes.len() && bytes[i].is_ascii_uppercase() {
        i += 1;
    }
    // Required first digit in '1'..='9'
    if i >= bytes.len() || !matches!(bytes[i], b'1'..=b'9') {
        return false;
    }
    i += 1;
    // Zero or more digits '0'..='9'
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    // Optional trailing ASCII uppercase letters
    while i < bytes.len() && bytes[i].is_ascii_uppercase() {
        i += 1;
    }
    i == bytes.len()
}

/// Preserve alphanumeric address components and canonicalize ASCII letters to uppercase.
pub fn canonical_prefix(value: &str) -> Result<String, AppError> {
    if !value.is_ascii() {
        return Err(AppError::validation("INVALID_PREFIX", "invalid_prefix"));
    }
    let normalized = value.trim().to_ascii_uppercase();
    if normalized.is_empty() || normalized.len() > 100 {
        return Err(AppError::validation("INVALID_PREFIX", "invalid_prefix"));
    }
    if !normalized.split('/').all(is_valid_segment) {
        return Err(AppError::validation("INVALID_PREFIX", "invalid_prefix"));
    }
    Ok(normalized)
}

/// Expand a prefix into its ancestor chain matching `prefix_recovery.py::address_chain`.
pub fn address_chain(prefix: &str) -> Result<Vec<String>, AppError> {
    if prefix.is_empty() {
        return Ok(vec![String::new()]);
    }
    let canon = canonical_prefix(prefix)?.to_ascii_lowercase();
    let parts: Vec<&str> = canon.split('/').collect();
    let mut chain = Vec::with_capacity(parts.len() + 1);
    chain.push(String::new());
    for i in 1..=parts.len() {
        chain.push(parts[..i].join("/"));
    }
    Ok(chain)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonicalizes_valid_vietnamese_alley_prefixes() {
        assert_eq!(canonical_prefix("  12/3a ").unwrap(), "12/3A");
        assert_eq!(canonical_prefix("b2/14/5c").unwrap(), "B2/14/5C");
        assert_eq!(canonical_prefix("100").unwrap(), "100");
        assert_eq!(
            address_chain("12/3A/2").unwrap(),
            vec!["", "12", "12/3a", "12/3a/2"]
        );
    }

    #[test]
    fn rejects_invalid_prefixes() {
        for bad in [
            "", "0", "01/2", "12/", "/12", "12//3", "ngõ 12", "A/1", "12 3",
        ] {
            assert!(canonical_prefix(bad).is_err(), "should reject {bad:?}");
        }
    }
}
