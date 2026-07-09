//! Deterministic scalar encodings for non-integer ADGL values.
//!
//! ADGL predicates and event fields use the `i64` slot domain in IR/runtime.
//! For stringly values (`FieldType::String`, string literals in predicates),
//! this module defines a single stable encoding used end-to-end.

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// Deterministic FNV-1a 64 hash over UTF-8 bytes.
#[must_use]
pub const fn stable_hash_u64(bytes: &[u8]) -> u64 {
    let mut h = FNV_OFFSET;
    let mut i = 0;
    while i < bytes.len() {
        h ^= bytes[i] as u64;
        h = h.wrapping_mul(FNV_PRIME);
        i += 1;
    }
    h
}

/// Stable `i64` encoding for string values.
///
/// This is the canonical ADGL string-to-slot encoding used by verifier
/// lowering and runtime fixture/event construction. Equal strings always map
/// to equal values; different strings map to different values except for rare
/// 64-bit hash collisions.
#[must_use]
pub fn stable_string_i64(value: &str) -> i64 {
    let hash = stable_hash_u64(value.as_bytes());
    i64::from_ne_bytes(hash.to_ne_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_string_encoding_is_deterministic() {
        let down_1 = stable_string_i64("DOWN");
        let down_2 = stable_string_i64("DOWN");
        let up = stable_string_i64("UP");
        assert_eq!(down_1, down_2);
        assert_ne!(down_1, up);
    }
}
