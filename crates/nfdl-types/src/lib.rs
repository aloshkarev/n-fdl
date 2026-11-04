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
