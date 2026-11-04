//! High-level datagram execution surface (`VmState` / `execute_datagram`).
//!
//! Distinct from [`nfdl_bytecode::BytecodeVm`]: that crate interprets compiled
//! [`nfdl_bytecode::Instruction`] programs. This crate walks a parsed
//! [`nfdl_syntax::Protocol`] AST for the minimal datagram path (depth / loop
//! limits). `nfdl-runtime` re-exports these types for API compatibility.

#![forbid(unsafe_code)]
#![warn(clippy::all)]

use nfdl_syntax::Protocol;

/// Parse / execution depth counters for the datagram VM.
#[derive(Debug, Clone, Default)]
pub struct ParserContext {
    pub depth: usize,
    pub loop_count: usize,
}

/// Packet-byte coverage from one `execute_datagram` call.
///
/// This is not a catalog cause. `Ok(())` is not a decode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DatagramCoverage {
    /// The call had no bytes to decode.
    Absent,
    /// A cap was hit. `cap` names it. `kept` and `dropped` are message counts.
    Partial {
        /// Cap name, such as `short-read` or `max_layer_depth`.
        cap: &'static str,
        /// Messages walked before the cap.
        kept: usize,
        /// Messages not walked.
        dropped: usize,
    },
    /// Every message was walked and the buffer was not short.
    Complete,
}

/// Errors produced by the high-level datagram VM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VmError {
    LimitExceeded(String),
}

impl std::fmt::Display for VmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VmError::LimitExceeded(s) => write!(f, "limit exceeded: {s}"),
        }
    }
}

impl std::error::Error for VmError {}

/// Minimal datagram-mode execution engine.
pub struct VmState {
    pub ctx: ParserContext,
}

impl VmState {
    pub fn new() -> Self {
        Self {
            ctx: ParserContext {
                depth: 0,
                loop_count: 0,
            },
        }
    }

    /// Walk `spec` against `bytes`.
    ///
    /// Depth resets on every call. Empty `bytes` is [`DatagramCoverage::Absent`].
    /// A buffer that runs out before the messages do is `short-read`. Depth
    /// past 16 is `max_layer_depth`. Neither result is a cause.
    pub fn execute_datagram(&mut self, spec: &Protocol, bytes: &[u8]) -> DatagramCoverage {
        const MAX_LAYER_DEPTH: usize = 16;
        self.ctx.depth = 0;
        self.ctx.loop_count = 0;
        if bytes.is_empty() {
            return DatagramCoverage::Absent;
        }
        let mut offset = 0usize;
        let mut kept = 0usize;
        for (index, _message) in spec.messages.iter().enumerate() {
            self.ctx.depth += 1;
            if self.ctx.depth > MAX_LAYER_DEPTH {
                return DatagramCoverage::Partial {
                    cap: "max_layer_depth",
                    kept,
                    dropped: spec.messages.len() - kept,
                };
            }
            if offset >= bytes.len() {
                return DatagramCoverage::Partial {
                    cap: "short-read",
                    kept,
                    dropped: spec.messages.len() - index,
                };
            }
            offset += 1;
            kept += 1;
        }
        DatagramCoverage::Complete
    }
}

impl Default for VmState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nfdl_syntax::{Message, Protocol};

    fn empty_proto(name: &str, n_messages: usize) -> Protocol {
        Protocol {
            name: name.into(),
            messages: (0..n_messages)
                .map(|i| Message {
                    name: format!("M{i}"),
                    doc: None,
                    fields: vec![],
                    lets: vec![],
                    loops: vec![],
                    validates: vec![],
                    matches: vec![],
                })
                .collect(),
            ..Protocol::default()
        }
    }

    #[test]
    fn execute_datagram_complete_for_shallow_protocol() {
        let mut vm = VmState::new();
        let proto = empty_proto("T", 3);
        assert_eq!(
            vm.execute_datagram(&proto, &[1, 2, 3]),
            DatagramCoverage::Complete
        );
        assert_eq!(vm.ctx.depth, 3);
        assert_eq!(
            vm.execute_datagram(&proto, &[1, 2, 3]),
            DatagramCoverage::Complete,
            "depth resets per call"
        );
        assert_eq!(vm.ctx.depth, 3);
    }

    #[test]
    fn empty_bytes_are_absent() {
        let mut vm = VmState::new();
        let proto = empty_proto("T", 3);
        assert_eq!(vm.execute_datagram(&proto, &[]), DatagramCoverage::Absent);
        assert_eq!(vm.ctx.depth, 0);
    }

    #[test]
    fn short_read_is_partial_not_a_zero_cause() {
        let mut vm = VmState::new();
        let proto = empty_proto("T", 3);
        assert_eq!(
            vm.execute_datagram(&proto, &[1]),
            DatagramCoverage::Partial {
                cap: "short-read",
                kept: 1,
                dropped: 2,
            }
        );
    }

    #[test]
    fn execute_datagram_hits_layer_depth_limit() {
        let mut vm = VmState::new();
        let proto = empty_proto("Deep", 17);
        let bytes = vec![1u8; 32];
        assert_eq!(
            vm.execute_datagram(&proto, &bytes),
            DatagramCoverage::Partial {
                cap: "max_layer_depth",
                kept: 16,
                dropped: 1,
            }
        );
    }
}
