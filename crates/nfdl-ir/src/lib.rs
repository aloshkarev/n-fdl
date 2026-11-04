#![forbid(unsafe_code)]
#![warn(clippy::all)]

// M0 placeholder — will be filled during vertical slice.
// This crate does not import an ADGL cause or a graph edge.

/// NFDL IR does not mint a cause. Stays `None` until a decode report exists,
/// and that report is still not an ADGL cause.
pub const DECODE_CAUSE: Option<()> = None;
