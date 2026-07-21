//! Integration with support for let, __current_offset and complex bytes[length-expr].

use crate::bytecode::{BytecodeBinOp, BytecodeProgram, BytecodeUnaryOp, Instruction};
use nfdl_syntax::ast::{
    BinOp, Expr, Field, Let, Loop, Match, Message, NfdlType, Protocol, UnaryOp, Validate,
};
use std::collections::HashMap;

/// Simple expression evaluator for lets and length expressions (v1).
pub fn eval_expr(expr: &Expr, vars: &HashMap<String, u64>) -> u64 {
    match expr {
        Expr::Int(v) => *v as u64,
        Expr::Ident(name) => {
            if name == "__current_offset" {
                *vars.get("__current_offset").unwrap_or(&0)
            } else {
                *vars.get(name).unwrap_or(&0)
            }
        }
        Expr::Binary { op, left, right } => {
            let l = eval_expr(left, vars);
            let r = eval_expr(right, vars);
            match op {
                BinOp::Add => l + r,
                BinOp::Sub => l.saturating_sub(r),
                BinOp::Mul => l * r,
                BinOp::Div => {
                    if r != 0 {
                        l / r
                    } else {
                        0
                    }
                }
                BinOp::Mod => {
                    if r != 0 {
                        l % r
                    } else {
                        0
                    }
                }
                BinOp::Eq => {
                    if l == r {
                        1
                    } else {
                        0
                    }
                }
                BinOp::Ne => {
                    if l != r {
                        1
                    } else {
                        0
                    }
                }
                BinOp::Gt => {
                    if l > r {
                        1
                    } else {
                        0
                    }
                }
                BinOp::Lt => {
                    if l < r {
                        1
                    } else {
                        0
                    }
                }
                BinOp::Ge => {
                    if l >= r {
                        1
                    } else {
                        0
                    }
                }
                BinOp::Le => {
                    if l <= r {
                        1
                    } else {
                        0
                    }
                }
                BinOp::And => {
                    if l != 0 && r != 0 {
                        1
                    } else {
                        0
                    }
                }
                BinOp::Or => {
                    if l != 0 || r != 0 {
                        1
                    } else {
                        0
                    }
                }
                BinOp::BitAnd => l & r,
                BinOp::BitOr => l | r,
                BinOp::BitXor => l ^ r,
                BinOp::Shl => l << r as u32,
                BinOp::Shr => l >> r as u32,
            }
        }
        Expr::Unary { op, expr } => {
            let v = eval_expr(expr, vars);
            match op {
                UnaryOp::Not => {
                    if v == 0 {
                        1
                    } else {
                        0
                    }
                }
                UnaryOp::BitNot => !v,
                UnaryOp::Neg => (0u64).wrapping_sub(v),
            }
        }
        Expr::Ternary {
            cond,
            then_branch,
            else_branch,
        } => {
            if eval_expr(cond, vars) != 0 {
                eval_expr(then_branch, vars)
            } else {
                eval_expr(else_branch, vars)
            }
        }
        Expr::Coalesce { value, default } => {
            let v = eval_expr(value, vars);
            if v != 0 { v } else { eval_expr(default, vars) }
        }
        Expr::Call { name: _, args: _ } => 0, // bidir_tuple etc handled specially for keys
        Expr::Tuple(_) => 0, // tuples are structural (used in keys), not scalar values
        Expr::Field(_, _) => 0, // resolved at key computation via dotted names
        Expr::Str(_) => 0, // string literals are not scalar values
    }
}

fn emit_expr_to_slot(
    expr: &Expr,
    var_slots: &HashMap<String, u16>,
    instructions: &mut Vec<Instruction>,
    next_slot: u16,
    current_offset_slot: u16,
) -> (u16, u16) {
    // Returns (result_slot, new_next_slot)
    match expr {
        Expr::Int(v) => {
            instructions.push(Instruction::LoadConst {
                value: *v as u64,
                dst: next_slot,
            });
            (next_slot, next_slot + 1)
        }
        Expr::Ident(name) => {
            if name == "__current_offset" {
                (current_offset_slot, next_slot)
            } else if let Some(&s) = var_slots.get(name) {
                // Value already lives in slot s, reuse it directly for BinOp
                (s, next_slot)
            } else {
                // Unknown ident - load 0 as fallback
                instructions.push(Instruction::LoadConst {
                    value: 0,
                    dst: next_slot,
                });
                (next_slot, next_slot + 1)
            }
        }
        Expr::Binary { op, left, right } => {
            let (l_slot, next1) = emit_expr_to_slot(
                left,
                var_slots,
                instructions,
                next_slot,
                current_offset_slot,
            );
            let (r_slot, next2) =
                emit_expr_to_slot(right, var_slots, instructions, next1, current_offset_slot);
            let dst = next2;
            let bc_op = match op {
                BinOp::Add => BytecodeBinOp::Add,
                BinOp::Sub => BytecodeBinOp::Sub,
                BinOp::Mul => BytecodeBinOp::Mul,
                BinOp::Div => BytecodeBinOp::Div,
                BinOp::Mod => BytecodeBinOp::Mod,
                BinOp::Eq => BytecodeBinOp::Eq,
                BinOp::Ne => BytecodeBinOp::Ne,
                BinOp::Gt => BytecodeBinOp::Gt,
                BinOp::Lt => BytecodeBinOp::Lt,
                BinOp::Ge => BytecodeBinOp::Ge,
                BinOp::Le => BytecodeBinOp::Le,
                BinOp::And => BytecodeBinOp::And,
                BinOp::Or => BytecodeBinOp::Or,
                BinOp::BitAnd => BytecodeBinOp::BitAnd,
                BinOp::BitOr => BytecodeBinOp::BitOr,
                BinOp::BitXor => BytecodeBinOp::BitXor,
                BinOp::Shl => BytecodeBinOp::Shl,
                BinOp::Shr => BytecodeBinOp::Shr,
            };
            instructions.push(Instruction::BinOp {
                op: bc_op,
                left: l_slot,
                right: r_slot,
                dst,
            });
            (dst, dst + 1)
        }
        Expr::Unary { op, expr } => {
            let (val_slot, next1) = emit_expr_to_slot(
                expr,
                var_slots,
                instructions,
                next_slot,
                current_offset_slot,
            );
            let dst = next1;
            let bc_op = match op {
                UnaryOp::Not => BytecodeUnaryOp::Not,
                UnaryOp::BitNot => BytecodeUnaryOp::BitNot,
                UnaryOp::Neg => BytecodeUnaryOp::Neg,
            };
            instructions.push(Instruction::UnaryOp {
                op: bc_op,
                src: val_slot,
                dst,
            });
            (dst, dst + 1)
        }
        Expr::Ternary {
            cond,
            then_branch,
            else_branch,
        } => {
            // cond; JumpIfZero(cond, else); <then> -> dst; Jump(end); else: <else> -> dst; end:
            let (c_slot, n1) = emit_expr_to_slot(
                cond,
                var_slots,
                instructions,
                next_slot,
                current_offset_slot,
            );
            let dst = n1;
            let jmp_to_else_idx = instructions.len();
            instructions.push(Instruction::JumpIfZero {
                cond_slot: c_slot,
                target: 0,
            });
            let (t_slot, n2) = emit_expr_to_slot(
                then_branch,
                var_slots,
                instructions,
                dst,
                current_offset_slot,
            );
            let after_then = n2;
            instructions.push(Instruction::CopySlot { src: t_slot, dst });
            let jmp_to_end_idx = instructions.len();
            instructions.push(Instruction::Jump { target: 0 });
            let else_ip = instructions.len() as u16;
            if let Instruction::JumpIfZero { target, .. } = &mut instructions[jmp_to_else_idx] {
                *target = else_ip;
            }
            let (e_slot, n3) = emit_expr_to_slot(
                else_branch,
                var_slots,
                instructions,
                after_then,
                current_offset_slot,
            );
            instructions.push(Instruction::CopySlot { src: e_slot, dst });
            let end_ip = instructions.len() as u16;
            if let Instruction::Jump { target } = &mut instructions[jmp_to_end_idx] {
                *target = end_ip;
            }
            (dst, n3)
        }
        Expr::Coalesce { value, default } => {
            // v ?? d  ==  (v != 0) ? v : d
            let (v_slot, n1) = emit_expr_to_slot(
                value,
                var_slots,
                instructions,
                next_slot,
                current_offset_slot,
            );
            let dst = n1;
            let jmp_to_default_idx = instructions.len();
            instructions.push(Instruction::JumpIfZero {
                cond_slot: v_slot,
                target: 0,
            });
            instructions.push(Instruction::CopySlot { src: v_slot, dst });
            let jmp_to_end_idx = instructions.len();
            instructions.push(Instruction::Jump { target: 0 });
            let default_ip = instructions.len() as u16;
            if let Instruction::JumpIfZero { target, .. } = &mut instructions[jmp_to_default_idx] {
                *target = default_ip;
            }
            let (d_slot, n2) =
                emit_expr_to_slot(default, var_slots, instructions, dst, current_offset_slot);
            instructions.push(Instruction::CopySlot { src: d_slot, dst });
            let end_ip = instructions.len() as u16;
            if let Instruction::Jump { target } = &mut instructions[jmp_to_end_idx] {
                *target = end_ip;
            }
            (dst, n2)
        }
        Expr::Call { name: _, args: _ } => {
            // For key expressions like bidir_tuple, we don't emit bytecode here
            instructions.push(Instruction::LoadConst {
                value: 0,
                dst: next_slot,
            });
            (next_slot, next_slot + 1)
        }
        Expr::Tuple(_) => {
            instructions.push(Instruction::LoadConst {
                value: 0,
                dst: next_slot,
            });
            (next_slot, next_slot + 1)
        }
        Expr::Field(expr, field) => {
            if let Expr::Ident(base) = expr.as_ref() {
                let full = format!("{}.{}", base, field);
                if let Some(&s) = var_slots.get(&full) {
                    return (s, next_slot);
                }
                if let Some(&s) = var_slots.get(field) {
                    return (s, next_slot);
                }
                if let Some(&s) = var_slots.get(base) {
                    return (s, next_slot);
                }
            }
            instructions.push(Instruction::LoadConst {
                value: 0,
                dst: next_slot,
            });
            (next_slot, next_slot + 1)
        }
        Expr::Str(_) => {
            instructions.push(Instruction::LoadConst {
                value: 0,
                dst: next_slot,
            });
            (next_slot, next_slot + 1)
        }
    }
}

fn emit_field_bytecode(
    field: &Field,
    name_prefix: &str,
    messages: &[Message],
    var_slots: &mut HashMap<String, u16>,
    field_map: &mut HashMap<String, u16>,
    instructions: &mut Vec<Instruction>,
    slot: &mut u16,
    current_offset_slot: u16,
    depth: usize,
    little_endian: bool,
) {
    const MAX_REF_DEPTH: usize = 8; // TODO: wire from Limits in runner
    if depth > MAX_REF_DEPTH {
        // Stop inlining to prevent stack overflow / resource exhaustion on bad refs
        instructions.push(Instruction::ReadU8 { slot: *slot });
        instructions.push(Instruction::UpdateOffset {
            slot: current_offset_slot,
        });
        *slot += 1;
        return;
    }
    let full_name = if name_prefix.is_empty() {
        field.name.clone()
    } else {
        format!("{}.{}", name_prefix, field.name)
    };

    // Conditional field: skip the read when cond == 0 (slot stays 0 = None-ish for v1).
    let cond_jump_idx = if let Some(cond) = &field.conditional {
        let (c_slot, next_s) =
            emit_expr_to_slot(cond, var_slots, instructions, *slot, current_offset_slot);
        *slot = next_s;
        let idx = instructions.len();
        instructions.push(Instruction::JumpIfZero {
            cond_slot: c_slot,
            target: 0,
        });
        Some(idx)
    } else {
        None
    };

    let read_slot = *slot;
    // Register the field's slot AFTER emitting the (optional) condition: the
    // condition expression may have allocated intermediate slots, so recording
    // `*slot` earlier would point at a cond-temp instead of the read destination.
    field_map.insert(full_name.clone(), read_slot);
    var_slots.insert(field.name.clone(), read_slot);
    var_slots.insert(full_name.clone(), read_slot);

    let is_ref = matches!(field.ty, NfdlType::MessageRef(_));

    match &field.ty {
        NfdlType::U8 => {
            instructions.push(Instruction::ReadU8 { slot: read_slot });
            instructions.push(Instruction::UpdateOffset {
                slot: current_offset_slot,
            });
            *slot += 1;
        }
        NfdlType::U16 => {
            instructions.push(Instruction::ReadU16 {
                slot: read_slot,
                le: little_endian,
            });
            instructions.push(Instruction::UpdateOffset {
                slot: current_offset_slot,
            });
            *slot += 1;
        }
        NfdlType::U24 => {
            instructions.push(Instruction::ReadU24 {
                slot: read_slot,
                le: little_endian,
            });
            instructions.push(Instruction::UpdateOffset {
                slot: current_offset_slot,
            });
            *slot += 1;
        }
        NfdlType::U32 => {
            instructions.push(Instruction::ReadU32 {
                slot: read_slot,
                le: little_endian,
            });
            instructions.push(Instruction::UpdateOffset {
                slot: current_offset_slot,
            });
            *slot += 1;
        }
        NfdlType::Bytes { len } => {
            let (len_slot, next_s) =
                emit_expr_to_slot(len, var_slots, instructions, *slot, current_offset_slot);
            *slot = next_s;
            instructions.push(Instruction::ReadSlice {
                len_slot,
                dst_slot: read_slot,
            });
            instructions.push(Instruction::UpdateOffset {
                slot: current_offset_slot,
            });
            *slot += 1;
        }
        NfdlType::MessageRef(ref_name) => {
            // Inline the referenced message's **full body** (fields + lets +
            // loops + validates + matches) in source order, so constructs like
            // diameter AVP's `match code { … }` and its `let v_bit`/`let pad_len`
            // are actually emitted (not just the flat field list).
            //
            // `var_slots` is a flat bare-name table shared across all inlining
            // levels. Without scoping, an inlined message's `let` bindings
            // (e.g. `payload_len`) overwrite the parent's, and — because a
            // `match` emits its case arms in source order — a sibling `default`
            // arm emitted *after* a recursively-inlined case arm would resolve
            // a shared name to the deepest nested slot (uninitialized). Snapshot
            // and restore `var_slots` around the inlining so each instance's
            // bindings stay local. `field_map` (output) and `slot` (global
            // counter) are NOT restored.
            if let Some(referenced) = messages.iter().find(|m| &m.name == ref_name) {
                let sub_prefix = if name_prefix.is_empty() {
                    field.name.clone()
                } else {
                    full_name.clone()
                };
                let saved_var_slots = var_slots.clone();
                emit_body_block(
                    &referenced.fields,
                    &referenced.lets,
                    &referenced.loops,
                    &referenced.validates,
                    &referenced.matches,
                    messages,
                    var_slots,
                    field_map,
                    instructions,
                    slot,
                    current_offset_slot,
                    little_endian,
                    &sub_prefix,
                    depth + 1,
                );
                *var_slots = saved_var_slots;
            } else {
                // Fallback: treat as opaque u8 (old behavior)
                instructions.push(Instruction::ReadU8 { slot: read_slot });
                instructions.push(Instruction::UpdateOffset {
                    slot: current_offset_slot,
                });
                *slot += 1;
            }
        }
        NfdlType::Bitfield { bits } => {
            instructions.push(Instruction::ReadBits {
                bits: *bits,
                slot: read_slot,
            });
            // Bit reads advance a bit-cursor that carries into input_pos; the
            // byte offset is only settled once aligned, so refresh __current_offset.
            instructions.push(Instruction::UpdateOffset {
                slot: current_offset_slot,
            });
            *slot += 1;
        }
        NfdlType::BytesRest | NfdlType::BytesEof | NfdlType::BytesStream => {
            // Consume the remainder of the current slice / stream (terminal).
            instructions.push(Instruction::ReadRest {
                name: field.name.clone(),
                slot: read_slot,
            });
            instructions.push(Instruction::UpdateOffset {
                slot: current_offset_slot,
            });
            *slot += 1;
        }
    }

    // Per-field refinement: validate expr -> message
    if let Some(v) = &field.validate {
        let (v_slot, next_s) =
            emit_expr_to_slot(&v.expr, var_slots, instructions, *slot, current_offset_slot);
        *slot = next_s;
        instructions.push(Instruction::Validate {
            pred_slot: v_slot,
            message: v.message.clone(),
        });
    }

    // Record emitted field value (leaf fields only; MessageRef inlines its own leaves)
    if !is_ref {
        instructions.push(Instruction::EmitField {
            name: field.name.clone(),
            slot: read_slot,
        });
    }

    // Patch the conditional-skip jump to land after the read/validate/emit block
    if let Some(idx) = cond_jump_idx {
        let after = instructions.len() as u16;
        if let Instruction::JumpIfZero { target, .. } = &mut instructions[idx] {
            *target = after;
        }
    }
}

/// Emit a message/arm body block in **source order** (fields, lets, loops,
/// validates, and matches interleaved via their `order` field). A field may
/// reference a preceding `let`, and a `let` may reference preceding fields, so
/// the original source order is the only correct emission order.
///
/// Shared by the root message, `match` arms, and `MessageRef` inlining so all
/// three stay consistent. `prefix` is the dotted path for inlined sub-messages
/// (e.g. `avps.a`); `depth` guards against recursive `MessageRef` inlining.
#[allow(clippy::too_many_arguments)]
fn emit_body_block(
    fields: &[Field],
    lets: &[Let],
    loops: &[Loop],
    validates: &[Validate],
    matches: &[Match],
    proto_messages: &[Message],
    var_slots: &mut HashMap<String, u16>,
    field_map: &mut HashMap<String, u16>,
    instructions: &mut Vec<Instruction>,
    slot: &mut u16,
    current_offset_slot: u16,
    little_endian: bool,
    prefix: &str,
    depth: usize,
) {
    enum Item<'a> {
        F(&'a Field),
        L(&'a Let),
        V(&'a Validate),
        P(&'a Loop),
        M(&'a Match),
    }
    let mut ordered: Vec<(u32, Item)> = Vec::new();
    for f in fields {
        ordered.push((f.order, Item::F(f)));
    }
    for l in lets {
        ordered.push((l.order, Item::L(l)));
    }
    for v in validates {
        ordered.push((v.order, Item::V(v)));
    }
    for p in loops {
        ordered.push((p.order, Item::P(p)));
    }
    for m in matches {
        ordered.push((m.order, Item::M(m)));
    }
    ordered.sort_by_key(|(o, _)| *o);

    for (_, item) in ordered {
        match item {
            Item::F(field) => emit_field_bytecode(
                field,
                prefix,
                proto_messages,
                var_slots,
                field_map,
                instructions,
                slot,
                current_offset_slot,
                depth,
                little_endian,
            ),
            Item::L(let_binding) => {
                let (val_slot, next) = emit_expr_to_slot(
                    &let_binding.value,
                    var_slots,
                    instructions,
                    *slot,
                    current_offset_slot,
                );
                let snap = next;
                instructions.push(Instruction::CopySlot {
                    src: val_slot,
                    dst: snap,
                });
                *slot = snap + 1;
                // Register the bare name so later fields/conditions can reference
                // it (e.g. `vendor_id if v_bit == 1`), plus a dotted entry.
                var_slots.insert(let_binding.name.clone(), snap);
                let full = if prefix.is_empty() {
                    let_binding.name.clone()
                } else {
                    format!("{}.{}", prefix, let_binding.name)
                };
                field_map.insert(full, snap);
            }
            Item::V(v) => {
                let (v_slot, next_s) =
                    emit_expr_to_slot(&v.expr, var_slots, instructions, *slot, current_offset_slot);
                *slot = next_s;
                instructions.push(Instruction::Validate {
                    pred_slot: v_slot,
                    message: v.message.clone(),
                });
            }
            Item::P(lp) => {
                let loop_prefix = if prefix.is_empty() {
                    lp.name.clone()
                } else {
                    format!("{}.{}", prefix, lp.name)
                };
                let mut carry_slots: std::collections::HashMap<String, u16> =
                    std::collections::HashMap::new();
                for carry in &lp.carries {
                    let carry_full = format!("{}.{}", loop_prefix, carry.name);
                    let (init_slot, next_s) = emit_expr_to_slot(
                        &carry.init,
                        var_slots,
                        instructions,
                        *slot,
                        current_offset_slot,
                    );
                    *slot = next_s;
                    var_slots.insert(carry_full.clone(), init_slot);
                    carry_slots.insert(carry.name.clone(), init_slot);
                    var_slots.insert(carry.name.clone(), init_slot);
                }

                let loop_start_ip = instructions.len() as u16;
                let (cond_slot, next_s) = emit_expr_to_slot(
                    &lp.condition,
                    var_slots,
                    instructions,
                    *slot,
                    current_offset_slot,
                );
                *slot = next_s;
                let exit_jump_idx = instructions.len();
                instructions.push(Instruction::JumpIfZero {
                    cond_slot,
                    target: 0,
                });

                for field in &lp.body {
                    emit_field_bytecode(
                        field,
                        &loop_prefix,
                        proto_messages,
                        var_slots,
                        field_map,
                        instructions,
                        slot,
                        current_offset_slot,
                        depth,
                        little_endian,
                    );
                }

                for nxt in &lp.nexts {
                    let full_name = format!("{}.{}", loop_prefix, nxt.name);
                    let target_slot = *var_slots
                        .get(&full_name)
                        .or_else(|| var_slots.get(&nxt.name))
                        .unwrap_or(slot);
                    let (val_slot, next_s2) = emit_expr_to_slot(
                        &nxt.value,
                        var_slots,
                        instructions,
                        *slot,
                        current_offset_slot,
                    );
                    *slot = next_s2;
                    instructions.push(Instruction::CopySlot {
                        src: val_slot,
                        dst: target_slot,
                    });
                }

                instructions.push(Instruction::Jump {
                    target: loop_start_ip,
                });

                let after_loop_ip = instructions.len() as u16;
                if let Instruction::JumpIfZero { target, .. } = &mut instructions[exit_jump_idx] {
                    *target = after_loop_ip;
                }

                for carry in &lp.carries {
                    let final_name = format!("{}.carries.{}", loop_prefix, carry.name);
                    let carry_slot = *carry_slots.get(&carry.name).unwrap_or(&0);
                    field_map.insert(final_name.clone(), carry_slot);
                    var_slots.insert(final_name, carry_slot);
                }
            }
            Item::M(m) => emit_match(
                m,
                proto_messages,
                var_slots,
                field_map,
                instructions,
                slot,
                current_offset_slot,
                little_endian,
                prefix,
                depth,
            ),
        }
    }
}

/// Emit a tagged-union `match`: evaluate the tag, then for each `case N` arm
/// compare and execute its body on equality; the `default` arm runs if no case
/// matched. All arm bodies jump to a common end label. `prefix`/`depth` are
/// threaded to arm bodies so an inlined `MessageRef`'s `match` fields get the
/// right dotted names and recursion depth.
#[allow(clippy::too_many_arguments)]
fn emit_match(
    m: &Match,
    proto_messages: &[Message],
    var_slots: &mut HashMap<String, u16>,
    field_map: &mut HashMap<String, u16>,
    instructions: &mut Vec<Instruction>,
    slot: &mut u16,
    current_offset_slot: u16,
    little_endian: bool,
    prefix: &str,
    depth: usize,
) {
    let (tag_slot, next) =
        emit_expr_to_slot(&m.tag, var_slots, instructions, *slot, current_offset_slot);
    *slot = next;

    let mut end_jumps: Vec<usize> = Vec::new();
    let mut default_arm: Option<&nfdl_syntax::ast::MatchArm> = None;

    for arm in &m.arms {
        if let Some(cv) = arm.case {
            let const_slot = *slot;
            instructions.push(Instruction::LoadConst {
                value: cv as u64,
                dst: const_slot,
            });
            *slot += 1;
            let cmp_slot = *slot;
            instructions.push(Instruction::BinOp {
                op: BytecodeBinOp::Eq,
                left: tag_slot,
                right: const_slot,
                dst: cmp_slot,
            });
            *slot += 1;
            let skip_idx = instructions.len();
            instructions.push(Instruction::JumpIfZero {
                cond_slot: cmp_slot,
                target: 0,
            });
            emit_body_block(
                &arm.fields,
                &arm.lets,
                &arm.loops,
                &arm.validates,
                &arm.matches,
                proto_messages,
                var_slots,
                field_map,
                instructions,
                slot,
                current_offset_slot,
                little_endian,
                prefix,
                depth,
            );
            let end_idx = instructions.len();
            instructions.push(Instruction::Jump { target: 0 });
            end_jumps.push(end_idx);
            let next_arm_ip = instructions.len() as u16;
            if let Instruction::JumpIfZero { target, .. } = &mut instructions[skip_idx] {
                *target = next_arm_ip;
            }
        } else {
            default_arm = Some(arm);
        }
    }

    if let Some(arm) = default_arm {
        emit_body_block(
            &arm.fields,
            &arm.lets,
            &arm.loops,
            &arm.validates,
            &arm.matches,
            proto_messages,
            var_slots,
            field_map,
            instructions,
            slot,
            current_offset_slot,
            little_endian,
            prefix,
            depth,
        );
    }

    let end_ip = instructions.len() as u16;
    for ej in end_jumps {
        if let Instruction::Jump { target } = &mut instructions[ej] {
            *target = end_ip;
        }
    }
}

/// Index of the root message: the first message not referenced by any other
/// message's field/loop/match-arm `MessageRef`. Falls back to 0. Nested messages
/// are inlined at their `MessageRef` sites, so only the root is emitted top-level.
fn root_message_index(proto: &Protocol) -> usize {
    let mut referenced: std::collections::HashSet<String> = std::collections::HashSet::new();
    for m in &proto.messages {
        collect_msg_refs(m, &mut referenced);
    }
    for (i, m) in proto.messages.iter().enumerate() {
        if !referenced.contains(&m.name) {
            return i;
        }
    }
    0
}

/// Walk a message body (fields, loops, and `match` arms recursively) collecting
/// every `MessageRef` target. Shared by `root_message_index` and the runner's
/// `collect_needed_messages` so both stay consistent.
fn collect_msg_refs(msg: &Message, out: &mut std::collections::HashSet<String>) {
    for f in &msg.fields {
        if let NfdlType::MessageRef(r) = &f.ty {
            out.insert(r.clone());
        }
    }
    for lp in &msg.loops {
        for f in &lp.body {
            if let NfdlType::MessageRef(r) = &f.ty {
                out.insert(r.clone());
            }
        }
    }
    for m in &msg.matches {
        collect_match_refs(m, out);
    }
}

/// Recursively collect `MessageRef` targets from a `match` and its arms.
fn collect_match_refs(m: &Match, out: &mut std::collections::HashSet<String>) {
    for arm in &m.arms {
        for f in &arm.fields {
            if let NfdlType::MessageRef(r) = &f.ty {
                out.insert(r.clone());
            }
        }
        for lp in &arm.loops {
            for f in &lp.body {
                if let NfdlType::MessageRef(r) = &f.ty {
                    out.insert(r.clone());
                }
            }
        }
        for nested in &arm.matches {
            collect_match_refs(nested, out);
        }
    }
}

pub fn protocol_to_bytecode_with_map(proto: &Protocol) -> (BytecodeProgram, HashMap<String, u16>) {
    let mut instructions = Vec::new();
    let mut slot: u16;
    let mut field_map: HashMap<String, u16> = HashMap::new();
    let mut var_slots: HashMap<String, u16> = HashMap::new();
    let little_endian = proto.endian == "little";

    // Reserve slot 0 for __current_offset
    let current_offset_slot: u16 = 0;
    // Initialize it (will be updated)
    instructions.push(Instruction::LoadConst {
        value: 0,
        dst: current_offset_slot,
    });
    slot = 1;

    let root_idx = root_message_index(proto);
    for (i, msg) in proto.messages.iter().enumerate() {
        if i != root_idx {
            continue;
        }
        var_slots.clear();
        var_slots.insert("__current_offset".to_string(), current_offset_slot);

        // Emit the root message body in source order via the shared helper
        // (same path used by `match` arms and `MessageRef` inlining).
        emit_body_block(
            &msg.fields,
            &msg.lets,
            &msg.loops,
            &msg.validates,
            &msg.matches,
            &proto.messages,
            &mut var_slots,
            &mut field_map,
            &mut instructions,
            &mut slot,
            current_offset_slot,
            little_endian,
            "",
            0,
        );
    }

    instructions.push(Instruction::Return);

    let program = BytecodeProgram {
        instructions,
        slot_count: (slot as usize).max(32) + 16,
    };

    (program, field_map)
}
pub fn protocol_to_bytecode(proto: &Protocol) -> BytecodeProgram {
    protocol_to_bytecode_with_map(proto).0
}

pub fn extract_context_for_message(
    proto: &Protocol,
    _msg_name: &str,
    data: &[u8],
) -> HashMap<String, u64> {
    // PRIMARY PATH: Use bytecode + VM for full support of lets, __current_offset, complex bytes, loop-while and expr emission.
    // NOTE: the bytecode builder emits the root message (see `root_message_index`);
    // `_msg_name` is retained for API compatibility / future per-message dispatch.
    let (program, field_map) = protocol_to_bytecode_with_map(proto);

    let mut vm = crate::bytecode::BytecodeVm::new(program.slot_count);
    vm.load_input(data);
    let _ = vm.run(&program);

    let mut ctx: HashMap<String, u64> = HashMap::new();

    // Build ctx from executed bytecode slots (this captures all emitted expressions, loop effects, lets, offsets)
    for (name, &s) in &field_map {
        ctx.insert(name.clone(), vm.get_slot(s));
    }

    // Ensure key special values
    ctx.insert("__current_offset".to_string(), vm.current_offset() as u64);

    ctx
}
pub fn extract_context(proto: &Protocol, data: &[u8]) -> HashMap<String, u64> {
    // fallback to first message
    if let Some(first) = proto.messages.first() {
        extract_context_for_message(proto, &first.name, data)
    } else {
        HashMap::new()
    }
}
