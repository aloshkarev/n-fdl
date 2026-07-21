#![forbid(unsafe_code)]

pub mod bytecode;
pub mod context;
pub mod continuation;
pub mod efsm;
pub mod error;
pub mod event_bus;
pub mod integration;
pub mod reassembly;
pub mod runner;
pub mod session;
pub mod vm;

pub use bytecode::{BytecodeProgram, BytecodeVm, Instruction, Limits, StreamOutcome, VmContinuation};
pub use context::ParserContext;
pub use continuation::{
    parse_stream_start, parse_stream_start_with_limits, resume, CompleteParse, StreamContinuation,
    StreamParseStep,
};
pub use efsm::{FiredTimer, FsmEngine};
pub use error::RuntimeError;
pub use event_bus::{Event, EventBus, EventSink, VecSink};
pub use integration::{
    extract_context, extract_context_for_message, protocol_to_bytecode,
    protocol_to_bytecode_with_map,
};
pub use reassembly::Reassembler;
pub use runner::{
    parse_and_run, parse_and_run_stream, parse_and_run_with_data,
    parse_and_run_with_data_and_limits,
};
pub use session::{FlowKey, SessionContext, SessionDb};
pub use vm::{VmError, VmState};
