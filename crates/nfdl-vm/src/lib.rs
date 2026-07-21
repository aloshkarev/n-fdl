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

    /// Execute datagram spec (minimal)
    pub fn execute_datagram(&mut self, spec: &Protocol) -> Result<(), VmError> {
        const MAX_LAYER_DEPTH: usize = 16;
        const MAX_LOOP_ITERATIONS: usize = 10_000;

        // Simulate execution of messages
        for _ in &spec.messages {
            self.ctx.depth += 1;
            if self.ctx.depth > MAX_LAYER_DEPTH {
                return Err(VmError::LimitExceeded("max_layer_depth".into()));
            }
        }

        if self.ctx.loop_count > MAX_LOOP_ITERATIONS {
            return Err(VmError::LimitExceeded("max_loop_iterations".into()));
        }

        Ok(())
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
    fn execute_datagram_ok_for_shallow_protocol() {
        let mut vm = VmState::new();
        let proto = empty_proto("T", 3);
        assert!(vm.execute_datagram(&proto).is_ok());
        assert_eq!(vm.ctx.depth, 3);
    }

    #[test]
    fn execute_datagram_hits_layer_depth_limit() {
        let mut vm = VmState::new();
        let proto = empty_proto("Deep", 17);
        match vm.execute_datagram(&proto) {
            Err(VmError::LimitExceeded(msg)) => assert_eq!(msg, "max_layer_depth"),
            other => panic!("expected LimitExceeded, got {other:?}"),
        }
    }
}
