//! Bytecode IR + VM — re-exported from [`nfdl_bytecode`] for API compatibility.
//!
//! Prefer depending on `nfdl-bytecode` directly for new code; this module keeps
//! `nfdl_runtime::bytecode::…` and the crate-root `pub use` paths working.

pub use nfdl_bytecode::{
    BytecodeBinOp, BytecodeError, BytecodeProgram, BytecodeUnaryOp, BytecodeVm, DatagramOutcome,
    Instruction, Limits, ShortRead, StreamOutcome, VmContinuation,
};
