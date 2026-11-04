#![forbid(unsafe_code)]
#![warn(clippy::all)]

// M0 placeholder — will be filled during vertical slice.
// This crate does not import an ADGL cause. Coverage stays absent until a
// bounded decode report exists. That report is not a cause.

/// Packet-format coverage. `Absent` until a decode report is produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeCoverage {
    /// No decode report.
    Absent,
    /// A cap was hit while decoding.
    Partial {
        /// Cap name.
        cap: &'static str,
        /// Units kept.
        kept: usize,
        /// Units dropped.
        dropped: usize,
    },
    /// The decode report covered the buffer.
    Complete,
}

/// Current decode report: none.
pub const DECODE_COVERAGE: DecodeCoverage = DecodeCoverage::Absent;

/// Map a buffer walk to coverage.
///
/// Empty input is [`DecodeCoverage::Absent`]. A named cap, or bytes left after
/// `consumed`, is [`DecodeCoverage::Partial`]. `consumed == buffer_len` with no
/// cap is [`DecodeCoverage::Complete`]. This function does not decode fields
/// and it is not a cause.
pub fn decode_coverage(
    buffer_len: usize,
    consumed: usize,
    cap: Option<&'static str>,
) -> DecodeCoverage {
    if buffer_len == 0 {
        return DecodeCoverage::Absent;
    }
    if let Some(cap) = cap {
        let kept = consumed.min(buffer_len);
        let dropped = buffer_len
            .saturating_sub(kept)
            .max(consumed.saturating_sub(buffer_len));
        return DecodeCoverage::Partial { cap, kept, dropped };
    }
    if consumed == buffer_len {
        return DecodeCoverage::Complete;
    }
    if consumed > buffer_len {
        return DecodeCoverage::Partial {
            cap: "unread-tail",
            kept: buffer_len,
            dropped: consumed - buffer_len,
        };
    }
    DecodeCoverage::Partial {
        cap: "unread-tail",
        kept: consumed,
        dropped: buffer_len - consumed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_buffer_is_absent_even_with_a_cap() {
        assert_eq!(decode_coverage(0, 0, None), DecodeCoverage::Absent);
        assert_eq!(
            decode_coverage(0, 0, Some("short-read")),
            DecodeCoverage::Absent
        );
    }

    #[test]
    fn leftover_or_named_cap_is_partial_and_exact_consume_is_complete() {
        assert_eq!(decode_coverage(4, 4, None), DecodeCoverage::Complete);
        assert_eq!(
            decode_coverage(4, 1, None),
            DecodeCoverage::Partial {
                cap: "unread-tail",
                kept: 1,
                dropped: 3,
            }
        );
        assert_eq!(
            decode_coverage(4, 4, Some("short-read")),
            DecodeCoverage::Partial {
                cap: "short-read",
                kept: 4,
                dropped: 0,
            }
        );
    }
}
