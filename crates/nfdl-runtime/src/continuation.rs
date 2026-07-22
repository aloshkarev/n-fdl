//! Stream `NeedMoreBytes` / continuation API (spec 06 §5, 08 §5).
//!
//! When a stream-mode parse suspends for more input, callers receive a
//! [`StreamContinuation`] handle and later call [`resume`] with additional
//! bytes. Bare [`RuntimeError::NeedMoreBytes`] remains for callers that only
//! need the unit error (see [`StreamParseStep::into_complete`]).

use crate::bytecode::{BytecodeProgram, BytecodeVm, Limits, StreamOutcome, VmContinuation};
use crate::efsm::FsmEngine;
use crate::error::RuntimeError;
use crate::event_bus::Event;
use crate::integration::protocol_to_bytecode_with_map;
use crate::runner::{compute_flow_key, root_message_name};
use nfdl_syntax::{Parser, Protocol};
use std::collections::HashMap;

/// Owned handle returned when stream parse yields for more bytes.
#[derive(Debug, Clone)]
pub struct StreamContinuation {
    vm_cont: VmContinuation,
    program: BytecodeProgram,
    field_map: HashMap<String, u16>,
    proto: Protocol,
    limits: Limits,
    root_msg: String,
}

impl StreamContinuation {
    /// Hint from the suspended read (may be a lower bound).
    pub fn bytes_needed(&self) -> usize {
        self.vm_cont.bytes_needed()
    }

    /// Bytes already consumed from the flow before suspension.
    pub fn consumed(&self) -> usize {
        self.vm_cont.consumed()
    }
}

/// One step of a resumable stream parse.
#[derive(Debug)]
#[allow(clippy::large_enum_variant)] // Done carries Protocol; NeedMore carries full VM snapshot.
pub enum StreamParseStep {
    Done {
        proto: Protocol,
        ctx: HashMap<String, u64>,
        final_state: String,
        events: Vec<Event>,
    },
    /// Suspended with a continuation handle (not only a bare error).
    NeedMoreBytes {
        needed: usize,
        continuation: Box<StreamContinuation>,
    },
}

/// Same tuple shape as [`crate::parse_and_run_with_data`] for the compat adapter.
pub type CompleteParse = (Protocol, HashMap<String, u64>, String, Vec<Event>);

impl StreamParseStep {
    /// Compat adapter: map yield to bare [`RuntimeError::NeedMoreBytes`].
    pub fn into_complete(self) -> Result<CompleteParse, RuntimeError> {
        match self {
            StreamParseStep::Done {
                proto,
                ctx,
                final_state,
                events,
            } => Ok((proto, ctx, final_state, events)),
            StreamParseStep::NeedMoreBytes { .. } => Err(RuntimeError::NeedMoreBytes),
        }
    }
}

/// Start a stream-mode parse. Short input yields [`StreamParseStep::NeedMoreBytes`].
pub fn parse_stream_start(src: &str, data: &[u8]) -> Result<StreamParseStep, RuntimeError> {
    parse_stream_start_with_limits(src, data, Limits::default())
}

/// Like [`parse_stream_start`] with caller-supplied limits.
pub fn parse_stream_start_with_limits(
    src: &str,
    data: &[u8],
    limits: Limits,
) -> Result<StreamParseStep, RuntimeError> {
    let mut p = Parser::new(src);
    let proto = p.parse_protocol().map_err(RuntimeError::from)?;
    let root_msg = root_message_name(&proto);
    let (program, field_map) = protocol_to_bytecode_with_map(&proto);
    let mut vm = BytecodeVm::with_limits(program.slot_count, limits.clone());
    vm.load_input(data);
    match vm.run_stream(&program)? {
        StreamOutcome::Complete => Ok(finalize(proto, field_map, &root_msg, &vm)?),
        StreamOutcome::NeedMoreBytes(vm_cont) => {
            let needed = vm_cont.bytes_needed();
            Ok(StreamParseStep::NeedMoreBytes {
                needed,
                continuation: Box::new(StreamContinuation {
                    vm_cont,
                    program,
                    field_map,
                    proto,
                    limits,
                    root_msg,
                }),
            })
        }
    }
}

/// Resume a suspended stream parse with additional bytes.
pub fn resume(
    continuation: StreamContinuation,
    more_bytes: &[u8],
) -> Result<StreamParseStep, RuntimeError> {
    let StreamContinuation {
        vm_cont,
        program,
        field_map,
        proto,
        limits,
        root_msg,
    } = continuation;
    let (vm, outcome) = BytecodeVm::resume(vm_cont, &program, more_bytes)?;
    match outcome {
        StreamOutcome::Complete => Ok(finalize(proto, field_map, &root_msg, &vm)?),
        StreamOutcome::NeedMoreBytes(vm_cont) => {
            let needed = vm_cont.bytes_needed();
            Ok(StreamParseStep::NeedMoreBytes {
                needed,
                continuation: Box::new(StreamContinuation {
                    vm_cont,
                    program,
                    field_map,
                    proto,
                    limits,
                    root_msg,
                }),
            })
        }
    }
}

fn finalize(
    proto: Protocol,
    field_map: HashMap<String, u16>,
    root_msg: &str,
    vm: &BytecodeVm,
) -> Result<StreamParseStep, RuntimeError> {
    let mut ctx: HashMap<String, u64> = HashMap::new();
    for (name, &s) in &field_map {
        if vm.slot_touched(s) {
            ctx.insert(name.clone(), vm.get_slot(s));
        }
    }
    ctx.insert("__current_offset".to_string(), vm.current_offset() as u64);

    let mut fsm = FsmEngine::new(1000);
    fsm.load_from_ast(&proto.state_machines);
    let key_expr = proto.state_machines.first().and_then(|sm| sm.key.clone());
    let key = compute_flow_key(&key_expr, &ctx);
    let (final_state, fsm_evs) = fsm.feed(key.clone(), root_msg, &ctx);
    if let Some(vars) = fsm.get_variables(&key) {
        for (k, v) in vars {
            ctx.insert(k.clone(), *v);
        }
    }

    let mut events = Vec::with_capacity(fsm_evs.len() + 1);
    events.push(Event::Message {
        msg_type: root_msg.to_string(),
        size: vm.input_len(),
    });
    events.extend(fsm_evs);

    Ok(StreamParseStep::Done {
        proto,
        ctx,
        final_state,
        events,
    })
}
