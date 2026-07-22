//! Runner with full context extraction from fields into EFSM.
//! Now uses bytecode VM that supports loops via jumps and BinOp.

use crate::bytecode::{BytecodeVm, Limits};
use crate::efsm::FsmEngine;
use crate::error::RuntimeError;
use crate::event_bus::{Event, EventBus, VecSink};
use crate::integration::protocol_to_bytecode_with_map;
use crate::reassembly::Reassembler;
use crate::session::FlowKey;
use nfdl_syntax::ast::Expr;
use nfdl_syntax::{Parser, Protocol};

pub(crate) fn compute_flow_key(key_expr: &Option<Expr>, ctx: &HashMap<String, u64>) -> FlowKey {
    // Render an endpoint expression (Tuple or scalar) into a canonical byte string
    // by concatenating each element's big-endian value. Used so that sorting two
    // endpoints is independent of direction (C4/C10, ADR-002 C4).
    fn eval_endpoint(e: &Expr, ctx: &HashMap<String, u64>) -> Vec<u8> {
        let mut vals: Vec<u64> = Vec::new();
        match e {
            Expr::Tuple(elems) => {
                for el in elems {
                    vals.push(crate::integration::eval_expr(el, ctx));
                }
            }
            other => vals.push(crate::integration::eval_expr(other, ctx)),
        }
        let mut bytes = Vec::new();
        for v in vals {
            bytes.extend_from_slice(&v.to_be_bytes());
        }
        bytes
    }

    if let Some(expr) = key_expr {
        match expr {
            Expr::Call { name, args } if name == "bidir_tuple" && args.len() == 2 => {
                let mut a = eval_endpoint(&args[0], ctx);
                let mut b = eval_endpoint(&args[1], ctx);
                if b < a {
                    std::mem::swap(&mut a, &mut b);
                }
                let mut data = [0u8; 16];
                let mut off = 0;
                for v in [a, b] {
                    for byt in v {
                        if off < 16 {
                            data[off] = byt;
                            off += 1;
                        }
                    }
                }
                return FlowKey { data };
            }
            Expr::Call { name, args } if name == "bidir" && args.len() == 2 => {
                let av = crate::integration::eval_expr(&args[0], ctx);
                let bv = crate::integration::eval_expr(&args[1], ctx);
                let (lo, hi) = if av <= bv { (av, bv) } else { (bv, av) };
                let mut data = [0u8; 16];
                data[0..8].copy_from_slice(&lo.to_be_bytes());
                data[8..16].copy_from_slice(&hi.to_be_bytes());
                return FlowKey { data };
            }
            _ => {}
        }
    }
    FlowKey { data: [0; 16] }
}

/// The root message is one never referenced by any other message's field/loop
/// `MessageRef` (i.e. the top-level PDU). Falls back to the first message.
pub fn root_message_name(proto: &Protocol) -> String {
    let mut referenced: std::collections::HashSet<String> = std::collections::HashSet::new();
    for m in &proto.messages {
        for f in &m.fields {
            if let nfdl_syntax::ast::NfdlType::MessageRef(r) = &f.ty {
                referenced.insert(r.clone());
            }
        }
        for lp in &m.loops {
            for f in &lp.body {
                if let nfdl_syntax::ast::NfdlType::MessageRef(r) = &f.ty {
                    referenced.insert(r.clone());
                }
            }
        }
        // A `MessageRef` in a `match` arm (e.g. `case N => { inner: Leaf; }`)
        // also marks the target as referenced — otherwise the referencing
        // message would be mistaken for unreferenced and picked as the root.
        for mt in &m.matches {
            collect_match_refs_name(mt, &mut referenced);
        }
    }
    for m in &proto.messages {
        if !referenced.contains(&m.name) {
            return m.name.clone();
        }
    }
    proto
        .messages
        .first()
        .map(|m| m.name.clone())
        .unwrap_or_default()
}

/// Recursive helper: collect `MessageRef` target names from a `match` and its
/// arms (mirrors `integration::collect_match_refs`).
fn collect_match_refs_name(
    m: &nfdl_syntax::ast::Match,
    out: &mut std::collections::HashSet<String>,
) {
    for arm in &m.arms {
        for f in &arm.fields {
            if let nfdl_syntax::ast::NfdlType::MessageRef(r) = &f.ty {
                out.insert(r.clone());
            }
        }
        for lp in &arm.loops {
            for f in &lp.body {
                if let nfdl_syntax::ast::NfdlType::MessageRef(r) = &f.ty {
                    out.insert(r.clone());
                }
            }
        }
        for nested in &arm.matches {
            collect_match_refs_name(nested, out);
        }
    }
}

use std::collections::HashMap;

/// Demo sample bytes for RADIUS Access-Request (code = 1)
fn sample_radius_access_request() -> Vec<u8> {
    let mut pkt = vec![
        1u8, // code = 1 (Access-Request)
        42,  // identifier
        0, 22, // length = 22 (big endian) — matches actual packet size (4+16+2)
    ];
    pkt.extend_from_slice(&[
        0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff,
        0x00,
    ]);
    pkt.extend_from_slice(&[0, 2]); // one Attribute: type=0, length=2, value=bytes[0]
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
            // Only include slots the VM actually wrote at runtime; compile-time
            // `MessageRef` unrolling registers phantom nested slots that never ran.
            if vm.slot_touched(s) {
                ctx.insert(name.clone(), vm.get_slot(s));
            }
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

        let root_msg = root_message_name(proto);
        let (new_state, evs) = fsm.feed(key, &root_msg, &ctx);
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

/// Reassemble a byte stream from (seq, data) segments, then run the root
/// message on the contiguous bytes. Wires `Reassembler` into stream-mode
/// protocols (spec 08-stream-reassembly). OOO segments are reassembled in-order.
pub fn parse_and_run_stream(
    src: &str,
    initial_seq: u32,
    segments: &[(u32, Vec<u8>)],
) -> Result<(Protocol, HashMap<String, u64>, String, Vec<Event>), RuntimeError> {
    let mut r = Reassembler::new(initial_seq);
    for (seq, data) in segments {
        r.accept_segment(*seq, data.clone())
            .map_err(|e| RuntimeError::LimitExceeded(format!("reassembly: {:?}", e)))?;
    }
    parse_and_run_with_data(src, r.get_contiguous())
}

/// Run with explicit bytes and the default `Limits`.
pub fn parse_and_run_with_data(
    src: &str,
    data: &[u8],
) -> Result<(Protocol, HashMap<String, u64>, String, Vec<Event>), RuntimeError> {
    parse_and_run_with_data_and_limits(src, data, Limits::default())
}

/// Run with explicit bytes and caller-supplied `Limits` (wires the configurable
/// resource limits from spec `07-runtime.md` / checklist task 3).
pub fn parse_and_run_with_data_and_limits(
    src: &str,
    data: &[u8],
    limits: Limits,
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
                // Find MessageRefs in fields, loops, and match arms (recursive).
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
                for m in &msg.matches {
                    visit_match(m, &mut to_visit);
                }
            }
        }
        needed
    }

    fn visit_match(m: &nfdl_syntax::ast::Match, to_visit: &mut Vec<String>) {
        for arm in &m.arms {
            for field in &arm.fields {
                if let nfdl_syntax::ast::NfdlType::MessageRef(ref_name) = &field.ty {
                    to_visit.push(ref_name.clone());
                }
            }
            for lp in &arm.loops {
                for field in &lp.body {
                    if let nfdl_syntax::ast::NfdlType::MessageRef(ref_name) = &field.ty {
                        to_visit.push(ref_name.clone());
                    }
                }
            }
            for nested in &arm.matches {
                visit_match(nested, to_visit);
            }
        }
    }

    /// Sub-parse a bound layer message from a payload slice (layered dispatch).
    fn run_layer(
        proto: &Protocol,
        layer: &str,
        data: &[u8],
        limits: Limits,
    ) -> Result<(HashMap<String, u64>, Vec<Event>), RuntimeError> {
        let needed = collect_needed_messages(proto, layer);
        if needed.is_empty() {
            return Ok((HashMap::new(), vec![]));
        }
        let mut tp = proto.clone();
        tp.messages = needed;
        let (program, field_map) = protocol_to_bytecode_with_map(&tp);
        let mut vm = BytecodeVm::with_limits(program.slot_count, limits);
        vm.load_input(data);
        vm.run(&program)?;
        let mut ctx: HashMap<String, u64> = HashMap::new();
        for (name, &s) in &field_map {
            if vm.slot_touched(s) {
                ctx.insert(name.clone(), vm.get_slot(s));
            }
        }
        ctx.insert("__current_offset".to_string(), vm.current_offset() as u64);
        let evs = vec![Event::Message {
            msg_type: layer.to_string(),
            size: data.len(),
        }];
        Ok((ctx, evs))
    }

    let root_msg = root_message_name(&proto);
    let needed = collect_needed_messages(&proto, &root_msg);
    let mut temp_proto = proto.clone();
    temp_proto.messages = if needed.is_empty() {
        proto.messages.clone()
    } else {
        needed
    };

    let (program, field_map) = protocol_to_bytecode_with_map(&temp_proto);
    let mut vm = BytecodeVm::with_limits(program.slot_count, limits.clone());
    vm.load_input(data);
    vm.run(&program)?;

    let mut ctx: HashMap<String, u64> = HashMap::new();
    for (name, &s) in &field_map {
        // Include all (including qualified from MessageRef inlining) so nested fields are in ctx,
        // but only slots the VM actually wrote — compile-time unrolling registers phantom slots
        // for recursion levels the data never reached (e.g. diameter grouped AVPs at depth > 1).
        if vm.slot_touched(s) {
            ctx.insert(name.clone(), vm.get_slot(s));
        }
    }

    ctx.insert("__current_offset".to_string(), vm.current_offset() as u64);

    // Note: `let` bindings (e.g. `attrs_len`, `start_offset`) are emitted by the
    // bytecode and appear in `field_map` / `ctx` above — no protocol-specific
    // post-processing is needed (the previous `attrs_len = length - 20` hack was
    // removed; let-snapshotting handles it generically).

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

    let key_expr = proto.state_machines.first().and_then(|sm| sm.key.clone());
    let key = compute_flow_key(&key_expr, &ctx);
    let (final_state, fsm_evs) = fsm.feed(key.clone(), &root_msg, &ctx);

    // Merge per-flow variables (from Set actions) into returned ctx
    if let Some(vars) = fsm.get_variables(&key) {
        for (k, v) in vars {
            ctx.insert(k.clone(), *v);
        }
    }

    // Always emit a Message-dispatch event first so callers can observe which
    // message was parsed even when the protocol has no state machine.
    let mut evs = Vec::with_capacity(fsm_evs.len() + 1);
    evs.push(Event::Message {
        msg_type: root_msg.clone(),
        size: data.len(),
    });
    evs.extend(fsm_evs);

    // Layered `bind` dispatch (C7): if the root message binds a payload field to
    // an in-protocol layer message and the `when` condition holds, sub-parse that
    // layer from the payload tail and merge its fields under `<layer>.<name>`.
    // Binds referencing external (not-in-protocol) layers are declarations only.
    for b in &proto.binds {
        if b.source != root_msg {
            continue;
        }
        if crate::integration::eval_expr(&b.when, &ctx) == 0 {
            continue;
        }
        let in_protocol = proto.messages.iter().any(|m| m.name == b.layer);
        if !in_protocol {
            continue;
        }
        let payload_start = vm
            .rest_starts()
            .iter()
            .find(|(n, _)| n == &b.field)
            .map(|(_, off)| *off);
        if let Some(start) = payload_start
            && let Ok((layer_ctx, layer_evs)) =
                run_layer(&proto, &b.layer, &data[start..], limits.clone())
        {
            for (k, v) in layer_ctx {
                ctx.insert(format!("{}.{}", b.layer, k), v);
            }
            evs.extend(layer_evs);
        }
    }

    Ok((proto, ctx, final_state, evs))
}

#[cfg(test)]
mod tests {
    use super::compute_flow_key;
    use nfdl_syntax::ast::Expr;
    use std::collections::HashMap;

    fn ctx_of(pairs: &[(&str, u64)]) -> HashMap<String, u64> {
        pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }

    #[test]
    fn bidir_key_is_direction_invariant() {
        // key = bidir(src_port, dst_port)
        let key = Some(Expr::Call {
            name: "bidir".into(),
            args: vec![
                Expr::Ident("src_port".into()),
                Expr::Ident("dst_port".into()),
            ],
        });
        let ab = ctx_of(&[("src_port", 100), ("dst_port", 200)]);
        let ba = ctx_of(&[("src_port", 200), ("dst_port", 100)]);
        let k1 = compute_flow_key(&key, &ab);
        let k2 = compute_flow_key(&key, &ba);
        assert_eq!(k1.data, k2.data, "bidir must canonicalize both directions");
        assert_ne!(k1.data, [0u8; 16], "key must be non-default");
    }

    #[test]
    fn bidir_tuple_key_is_direction_invariant() {
        // key = bidir_tuple((IPv4.src, src_port), (IPv4.dst, dst_port))
        let key = Some(Expr::Call {
            name: "bidir_tuple".into(),
            args: vec![
                Expr::Tuple(vec![
                    Expr::Ident("src_ip".into()),
                    Expr::Ident("src_port".into()),
                ]),
                Expr::Tuple(vec![
                    Expr::Ident("dst_ip".into()),
                    Expr::Ident("dst_port".into()),
                ]),
            ],
        });
        let ab = ctx_of(&[
            ("src_ip", 0x0a000001),
            ("src_port", 100),
            ("dst_ip", 0x0a000002),
            ("dst_port", 200),
        ]);
        let ba = ctx_of(&[
            ("src_ip", 0x0a000002),
            ("src_port", 200),
            ("dst_ip", 0x0a000001),
            ("dst_port", 100),
        ]);
        let k1 = compute_flow_key(&key, &ab);
        let k2 = compute_flow_key(&key, &ba);
        assert_eq!(
            k1.data, k2.data,
            "bidir_tuple must canonicalize both directions"
        );
    }
}
