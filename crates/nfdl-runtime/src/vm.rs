//! Minimal Execution Engine (datagram mode)

use crate::context::ParserContext;
use crate::error::RuntimeError;
use nfdl_syntax::Protocol;

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
    pub fn execute_datagram(&mut self, spec: &Protocol) -> Result<(), RuntimeError> {
        const MAX_LAYER_DEPTH: usize = 16;
        const MAX_LOOP_ITERATIONS: usize = 10_000;

        // Simulate execution of messages
        for _ in &spec.messages {
            self.ctx.depth += 1;
            if self.ctx.depth > MAX_LAYER_DEPTH {
                return Err(RuntimeError::LimitExceeded("max_layer_depth".into()));
            }
        }

        if self.ctx.loop_count > MAX_LOOP_ITERATIONS {
            return Err(RuntimeError::LimitExceeded("max_loop_iterations".into()));
        }

        Ok(())
    }
}
