//! High-level datagram VM — re-exported from [`nfdl_vm`] for API compatibility.
//!
//! Prefer depending on `nfdl-vm` directly for new code; this module keeps
//! `nfdl_runtime::vm::…` and the crate-root `pub use` paths working.
//!
//! Note: [`nfdl_vm::VmState`] is the AST/datagram surface; the bytecode
//! interpreter lives in [`nfdl_bytecode::BytecodeVm`] (re-exported via
//! [`crate::bytecode`]).

pub use nfdl_vm::{ParserContext, VmError, VmState};
