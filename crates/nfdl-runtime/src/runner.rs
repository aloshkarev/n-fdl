//! Runner with full context extraction from fields into EFSM.
//! Now uses bytecode VM that supports loops via jumps and BinOp.

use crate::bytecode::{BytecodeVm, Limits};
use crate::efsm::FsmEngine;
use crate::error::RuntimeError;
use crate::event_bus::{Event, EventBus, VecSink};
use crate::integration::protocol_to_bytecode_with_map;
use crate::session::FlowKey;
use nfdl_syntax::ast::Expr;
use nfdl_syntax::{Parser, Protocol};

fn compute_flow_key(key_expr: &Option<Expr>, ctx: &HashMap<String, u64>) -> FlowKey {
    if let Some(Expr::Call { name, args }) = key_expr {
        if name == "bidir_tuple" && args.len() == 2 {
            // For simplicity, take first value from each endpoint tuple if possible
            // In real impl we would deeply evaluate the tuples
            let mut data = [0u8; 16];
            // Pack some values from ctx as demo
            if let Some(&v1) = ctx.get("src_port") {
                data[0..4].copy_from_slice(&v1.to_be_bytes());
            }
            if let Some(&v2) = ctx.get("dst_port") {
                data[4..8].copy_from_slice(&v2.to_be_bytes());
            }
            return FlowKey { data };
        }
        if name == "bidir" && args.len() == 2 {
            let mut data = [0u8; 16];
            return FlowKey { data };
        }
    }
    // default or plain tuple
    FlowKey { data: [0; 16] }
}

use std::collections::HashMap;

/// Demo sample bytes for RADIUS Access-Request (code = 1)
fn sample_radius_access_request() -> Vec<u8> {
    let mut pkt = vec![
        1u8, // code = 1 (Access-Request)
        42,  // identifier
        0, 44, // length = 44 (big endian)
    ];
    pkt.extend_from_slice(&[
        0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff,
        0x00,
    ]);
    pkt.extend_from_slice(&[0, 2]); // dummy attr
    pkt
}

pub fn run_protocol(proto: &Protocol) -> Result<Vec<Event>, RuntimeError> {
    let (program, field_map) = protocol_to_bytecode_with_map(proto);
    let limits = Limits {
        max_instructions: 100_000,
        max_loop_iterations: 10_000,
    };
    let mut vm = BytecodeVm::with_limits(program.slot_count, limits);
    let sample_data = sample_radius_access_request();
    vm.load_input(&sample_data);
    vm.run(&program)?;

    let sink = VecSink::new();
    let mut bus = EventBus::new(sink);

    for m in &proto.messages {
        bus.emit(Event::Message {
            msg_type: m.name.clone(),
            size: sample_data.len(),
        });
    }

    if !proto.state_machines.is_empty() {
        let mut fsm = FsmEngine::new(1000);
        fsm.load_from_ast(&proto.state_machines);

        // Build rich ctx from executed bytecode (loops have advanced input and computed slots)
        let mut ctx: HashMap<String, u64> = HashMap::new();
        for (name, &s) in &field_map {
            ctx.insert(name.clone(), vm.get_slot(s));
        }

        let key_expr = if let Some(sm) = proto.state_machines.first() {
            sm.key.clone()
        } else {
            None
        };
        let key = compute_flow_key(&key_expr, &ctx);
        if !ctx.contains_key("code") {
            ctx.insert("code".to_string(), 1);
        }
        ctx.insert("__current_offset".to_string(), vm.current_offset() as u64);

        let (new_state, evs) = fsm.feed(key, "AccessMessage", &ctx);
        for e in evs {
            bus.emit(e);
        }

        bus.emit(Event::Diagnostic {
            code: "fsm".to_string(),
            message: format!(
                "EFSM after bytecode+loops: {} (code={})",
                new_state,
                ctx.get("code").unwrap_or(&0)
            ),
        });
    }

    Ok(bus.sink.events)
}

pub fn parse_and_run(src: &str) -> Result<(Protocol, Vec<Event>), RuntimeError> {
    let mut p = Parser::new(src);
    let proto = p.parse_protocol().map_err(RuntimeError::from)?;
    let evs = run_protocol(&proto)?;
    Ok((proto, evs))
}

/// Run with explicit bytes. Uses full loop-capable bytecode VM.
pub fn parse_and_run_with_data(
    src: &str,
    data: &[u8],
) -> Result<(Protocol, HashMap<String, u64>, String, Vec<Event>), RuntimeError> {
    let mut p = Parser::new(src);
    let proto = p.parse_protocol().map_err(RuntimeError::from)?;

    // General collection of messages needed for the target (reduces hard-coded temp_proto hack)
    fn collect_needed_messages(
        proto: &Protocol,
        root_name: &str,
    ) -> Vec<nfdl_syntax::ast::Message> {
        let mut needed = Vec::new();
        let mut to_visit = vec![root_name.to_string()];
        let mut visited = std::collections::HashSet::new();

        while let Some(name) = to_visit.pop() {
            if !visited.insert(name.clone()) {
                continue;
            }
            if let Some(msg) = proto.messages.iter().find(|m| m.name == name) {
                needed.push(msg.clone());
                // Find MessageRefs in fields and loops
                for field in &msg.fields {
                    if let nfdl_syntax::ast::NfdlType::MessageRef(ref_name) = &field.ty {
                        to_visit.push(ref_name.clone());
                    }
                }
                for lp in &msg.loops {
                    for field in &lp.body {
                        if let nfdl_syntax::ast::NfdlType::MessageRef(ref_name) = &field.ty {
                            to_visit.push(ref_name.clone());
                        }
                    }
                }
            }
        }
        needed
    }

    let needed = collect_needed_messages(&proto, "AccessMessage");
    let mut temp_proto = proto.clone();
    temp_proto.messages = if needed.is_empty() {
        proto.messages.clone()
    } else {
        needed
    };

    let (program, field_map) = protocol_to_bytecode_with_map(&temp_proto);
    let limits = Limits {
        max_instructions: 100_000,
        max_loop_iterations: 10_000,
    };
    let mut vm = BytecodeVm::with_limits(program.slot_count, limits);
    vm.load_input(data);
    vm.run(&program)?;

    let mut ctx: HashMap<String, u64> = HashMap::new();
    for (name, &s) in &field_map {
        // Include all (including qualified from MessageRef inlining) so nested fields are in ctx
        ctx.insert(name.clone(), vm.get_slot(s));
    }

    ctx.insert("__current_offset".to_string(), vm.current_offset() as u64);

    // Enrich with computed lets from bytecode (attrs_len, start_offset etc.)
    if let Some(&l) = ctx.get("length") {
        ctx.insert("attrs_len".to_string(), l.saturating_sub(20));
    }

    // Resource limit on context size (from Limits)
    if ctx.len() > 1024 {
        // keep simple for now, or extend Limits
        return Err(RuntimeError::LimitExceeded(format!(
            "context too large: {} entries",
            ctx.len()
        )));
    }

    let mut fsm = FsmEngine::new(1000);
    fsm.load_from_ast(&proto.state_machines);

    let key = FlowKey { data: [0; 16] };
    let (final_state, evs) = fsm.feed(key.clone(), "AccessMessage", &ctx);

    // Merge per-flow variables (from Set actions) into returned ctx
    if let Some(vars) = fsm.get_variables(&key) {
        for (k, v) in vars {
            ctx.insert(k.clone(), *v);
        }
    }

    Ok((proto, ctx, final_state, evs))
}
