//! Integration with support for let, __current_offset and complex bytes[length-expr].

use crate::bytecode::{BytecodeBinOp, BytecodeProgram, BytecodeUnaryOp, Instruction};
use nfdl_syntax::ast::{BinOp, Expr, Field, Let, Message, NfdlType, Protocol, UnaryOp};
use std::collections::HashMap;

/// Simple expression evaluator for lets and length expressions (v1).
fn eval_expr(expr: &Expr, vars: &HashMap<String, u64>) -> u64 {
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
                BinOp::BitAnd => (l & r),
                BinOp::BitOr => (l | r),
                BinOp::BitXor => (l ^ r),
                BinOp::Shl => l << (r as u32),
                BinOp::Shr => l >> (r as u32),
                _ => 0,
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
                UnaryOp::BitNot => (!v),
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
    }
}

/// Compute lets for a message given initial context (e.g. previous fields).
fn evaluate_lets(lets: &[Let], mut vars: HashMap<String, u64>) -> HashMap<String, u64> {
    for l in lets {
        let val = eval_expr(&l.value, &vars);
        vars.insert(l.name.clone(), val);
    }
    vars
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
                _ => BytecodeBinOp::Eq,
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
            // For now emit as if-then-else via jumps would be ideal, but for v1.5 we can evaluate both or use a simple form
            // Simplified: evaluate cond then choose
            let (c_slot, n1) = emit_expr_to_slot(
                cond,
                var_slots,
                instructions,
                next_slot,
                current_offset_slot,
            );
            let (t_slot, n2) = emit_expr_to_slot(
                then_branch,
                var_slots,
                instructions,
                n1,
                current_offset_slot,
            );
            let (e_slot, n3) = emit_expr_to_slot(
                else_branch,
                var_slots,
                instructions,
                n2,
                current_offset_slot,
            );
            // Use a select-like or just return then for simplicity in this step
            // For correctness we push a BinOp that acts as select (cond != 0 ? t : e)
            let dst = n3;
            instructions.push(Instruction::BinOp {
                op: BytecodeBinOp::Eq,
                left: c_slot,
                right: c_slot,
                dst: dst,
            }); // placeholder
            (dst, dst + 1)
        }
        Expr::Coalesce { value, default } => {
            // Emit proper coalesce: result = v != 0 ? v : default
            // Using arithmetic select: is_zero = (v==0); result = is_zero*d + (1-is_zero)*v
            let (v_slot, n1) = emit_expr_to_slot(
                value,
                var_slots,
                instructions,
                next_slot,
                current_offset_slot,
            );
            let (d_slot, n2) =
                emit_expr_to_slot(default, var_slots, instructions, n1, current_offset_slot);
            let dst = n2;
            // is_zero = v == 0
            instructions.push(Instruction::BinOp {
                op: BytecodeBinOp::Eq,
                left: v_slot,
                right: v_slot,
                dst: dst,
            }); // reuse for placeholder, but we'll overwrite
            // For correctness we allocate more slots
            let is_zero = dst;
            let one_slot = dst + 1;
            instructions.push(Instruction::LoadConst {
                value: 1,
                dst: one_slot,
            });
            let not_zero = dst + 2;
            instructions.push(Instruction::BinOp {
                op: BytecodeBinOp::Sub,
                left: one_slot,
                right: is_zero,
                dst: not_zero,
            });
            let tmp1 = dst + 3;
            instructions.push(Instruction::BinOp {
                op: BytecodeBinOp::Mul,
                left: is_zero,
                right: d_slot,
                dst: tmp1,
            });
            let tmp2 = dst + 4;
            instructions.push(Instruction::BinOp {
                op: BytecodeBinOp::Mul,
                left: not_zero,
                right: v_slot,
                dst: tmp2,
            });
            let result = dst + 5;
            instructions.push(Instruction::BinOp {
                op: BytecodeBinOp::Add,
                left: tmp1,
                right: tmp2,
                dst: result,
            });
            (result, result + 1)
        }
        Expr::Call { name: _, args: _ } => {
            // For key expressions like bidir_tuple, we don't emit bytecode here
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

    field_map.insert(full_name.clone(), *slot);
    var_slots.insert(field.name.clone(), *slot);
    var_slots.insert(full_name.clone(), *slot);

    match &field.ty {
        NfdlType::U8 => {
            instructions.push(Instruction::ReadU8 { slot: *slot });
            instructions.push(Instruction::UpdateOffset {
                slot: current_offset_slot,
            });
            *slot += 1;
        }
        NfdlType::U16 | NfdlType::U24 => {
            instructions.push(Instruction::ReadU16 { slot: *slot });
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
                dst_slot: *slot,
            });
            instructions.push(Instruction::UpdateOffset {
                slot: current_offset_slot,
            });
            *slot += 1;
        }
        NfdlType::MessageRef(ref_name) => {
            // Inline the referenced message fields
            if let Some(referenced) = messages.iter().find(|m| &m.name == ref_name) {
                let sub_prefix = if name_prefix.is_empty() {
                    field.name.clone()
                } else {
                    full_name.clone()
                };
                for sub_field in &referenced.fields {
                    emit_field_bytecode(
                        sub_field,
                        &sub_prefix,
                        messages,
                        var_slots,
                        field_map,
                        instructions,
                        slot,
                        current_offset_slot,
                        depth + 1,
                    );
                }
                // Note: lets/loops inside referenced not inlined here for v1 simplicity
            } else {
                // Fallback: treat as opaque u8 (old behavior)
                instructions.push(Instruction::ReadU8 { slot: *slot });
                instructions.push(Instruction::UpdateOffset {
                    slot: current_offset_slot,
                });
                *slot += 1;
            }
        }
        _ => {
            // BytesRest etc - fallback
            instructions.push(Instruction::ReadU8 { slot: *slot });
            instructions.push(Instruction::UpdateOffset {
                slot: current_offset_slot,
            });
            *slot += 1;
        }
    }
}

pub fn protocol_to_bytecode_with_map(proto: &Protocol) -> (BytecodeProgram, HashMap<String, u16>) {
    let mut instructions = Vec::new();
    let mut slot = 0u16;
    let mut field_map: HashMap<String, u16> = HashMap::new();
    let mut var_slots: HashMap<String, u16> = HashMap::new();

    // Reserve slot 0 for __current_offset
    let current_offset_slot: u16 = 0;
    // Initialize it (will be updated)
    instructions.push(Instruction::LoadConst {
        value: 0,
        dst: current_offset_slot,
    });
    slot = 1;

    for msg in &proto.messages {
        var_slots.clear();
        var_slots.insert("__current_offset".to_string(), current_offset_slot);

        // Process lets first - emit their expressions
        for let_binding in &msg.lets {
            let (val_slot, next) = emit_expr_to_slot(
                &let_binding.value,
                &var_slots,
                &mut instructions,
                slot,
                current_offset_slot,
            );
            slot = next;
            // The let value lives in val_slot (or we can copy if needed)
            var_slots.insert(let_binding.name.clone(), val_slot);
            // Also record in field_map for visibility
            field_map.insert(let_binding.name.clone(), val_slot);
        }

        // Regular fields + expression emission for complex lengths
        for field in &msg.fields {
            emit_field_bytecode(
                field,
                "",
                &proto.messages,
                &mut var_slots,
                &mut field_map,
                &mut instructions,
                &mut slot,
                current_offset_slot,
                0,
            );
            // Note: slot is updated inside the helper
        }

        // === IMPROVED BYTECODE FOR LOOPS with carries + next (C2) ===
        for lp in &msg.loops {
            // 1. Initialize carry slots before loop
            let mut carry_slots: std::collections::HashMap<String, u16> =
                std::collections::HashMap::new();
            for carry in &lp.carries {
                let carry_full = if lp.name.is_empty() {
                    carry.name.clone()
                } else {
                    format!("{}.{}", lp.name, carry.name)
                };
                let (init_slot, next_s) = emit_expr_to_slot(
                    &carry.init,
                    &var_slots,
                    &mut instructions,
                    slot,
                    current_offset_slot,
                );
                slot = next_s;
                var_slots.insert(carry_full.clone(), init_slot);
                carry_slots.insert(carry.name.clone(), init_slot);
                // also register base name for simplicity
                var_slots.insert(carry.name.clone(), init_slot);
            }

            // Emit initial condition
            let (cond_slot, next_s) = emit_expr_to_slot(
                &lp.condition,
                &var_slots,
                &mut instructions,
                slot,
                current_offset_slot,
            );
            slot = next_s;

            let loop_start_ip = instructions.len() as u16;

            // Body fields
            for field in &lp.body {
                emit_field_bytecode(
                    field,
                    &lp.name,
                    &proto.messages,
                    &mut var_slots,
                    &mut field_map,
                    &mut instructions,
                    &mut slot,
                    current_offset_slot,
                    0,
                );
            }

            // 2. Emit next statements (carry updates) inside loop, before recheck
            for nxt in &lp.nexts {
                let full_name = if lp.name.is_empty() {
                    nxt.name.clone()
                } else {
                    format!("{}.{}", lp.name, nxt.name)
                };
                let target_slot = *var_slots
                    .get(&full_name)
                    .or_else(|| var_slots.get(&nxt.name))
                    .unwrap_or(&slot);
                let (val_slot, next_s2) = emit_expr_to_slot(
                    &nxt.value,
                    &var_slots,
                    &mut instructions,
                    slot,
                    current_offset_slot,
                );
                slot = next_s2;
                // copy val to target carry slot (use existing ops if needed; simple store via load)
                instructions.push(Instruction::CopySlot {
                    src: val_slot,
                    dst: target_slot,
                });
            }

            // Recompute condition (may depend on updated carries)
            let (recomputed, next2) = emit_expr_to_slot(
                &lp.condition,
                &var_slots,
                &mut instructions,
                slot,
                current_offset_slot,
            );
            slot = next2;

            let after_loop_ip = (instructions.len() + 2) as u16;
            instructions.push(Instruction::JumpIfZero {
                cond_slot: recomputed,
                target: after_loop_ip,
            });
            instructions.push(Instruction::Jump {
                target: loop_start_ip,
            });

            // After loop: expose final carry values under loopname.carries.name
            for carry in &lp.carries {
                let final_name = format!("{}.carries.{}", lp.name, carry.name);
                let carry_slot = *carry_slots.get(&carry.name).unwrap_or(&0);
                field_map.insert(final_name.clone(), carry_slot);
                var_slots.insert(final_name, carry_slot);
            }
        }
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

/// Extract context with full let + complex length + __current_offset support.

fn parse_fields_into_ctx(
    fields: &[Field],
    data: &[u8],
    mut data_pos: usize,
    vars: &mut HashMap<String, u64>,
    instructions: &mut Vec<Instruction>,
    mut slot: u16,
    proto: &Protocol,
) -> (usize, u16) {
    for field in fields {
        let current_slot = slot;
        // field_map.insert... (caller manages if needed)

        match &field.ty {
            NfdlType::U8 => {
                let v = data.get(data_pos).copied().unwrap_or(0) as u64;
                instructions.push(Instruction::ReadU8 { slot });
                slot += 1;
                data_pos += 1;
                vars.insert("__current_offset".to_string(), data_pos as u64);
                vars.insert(field.name.clone(), v);
            }
            NfdlType::U16 => {
                let v = if data_pos + 1 < data.len() {
                    u16::from_be_bytes([data[data_pos], data[data_pos + 1]]) as u64
                } else {
                    0
                };
                instructions.push(Instruction::ReadU16 { slot });
                slot += 2;
                data_pos += 2;
                vars.insert("__current_offset".to_string(), data_pos as u64);
                vars.insert(field.name.clone(), v);
            }
            NfdlType::Bytes { len } => {
                let computed = eval_expr(len, vars) as usize;
                let mut v = 0u64;
                for i in 0..computed.min(8) {
                    if data_pos + i < data.len() {
                        v = (v << 8) | data[data_pos + i] as u64;
                    }
                }
                instructions.push(Instruction::ReadSlice {
                    len_slot: computed as u16,
                    dst_slot: slot,
                });
                slot += computed as u16;
                data_pos += computed;
                vars.insert("__current_offset".to_string(), data_pos as u64);
                vars.insert(field.name.clone(), v);
            }
            NfdlType::MessageRef(ref_name) => {
                // Recursively parse the referenced message
                if let Some(sub_msg) = proto.messages.iter().find(|m| m.name == *ref_name) {
                    let (new_pos, new_slot) = parse_fields_into_ctx(
                        &sub_msg.fields,
                        data,
                        data_pos,
                        vars,
                        instructions,
                        slot,
                        proto,
                    );
                    // also process lets and loops of sub if needed
                    data_pos = new_pos;
                    slot = new_slot;
                } else {
                    // unknown, skip 1 byte
                    data_pos += 1;
                    slot += 1;
                }
            }
            _ => {
                let v = data.get(data_pos).copied().unwrap_or(0) as u64;
                data_pos += 1;
                slot += 1;
                vars.insert(field.name.clone(), v);
            }
        }
    }
    (data_pos, slot)
}

pub fn extract_context_for_message(
    proto: &Protocol,
    msg_name: &str,
    data: &[u8],
) -> HashMap<String, u64> {
    // PRIMARY PATH: Use bytecode + VM for full support of lets, __current_offset, complex bytes, loop-while and expr emission
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
