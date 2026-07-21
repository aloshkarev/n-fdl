//! Bytecode program IR and interpreter (Instruction / BytecodeVm).
//!
//! Extracted from `nfdl-runtime` so compile/session/EFSM can depend on a thin
//! bytecode crate without pulling the full runtime. `nfdl-runtime` re-exports
//! these types for API compatibility.

#![forbid(unsafe_code)]
#![warn(clippy::all)]

/// Errors produced while interpreting a [`BytecodeProgram`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BytecodeError {
    Constraint(String),
    LimitExceeded(String),
}

impl std::fmt::Display for BytecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BytecodeError::Constraint(s) => write!(f, "constraint: {s}"),
            BytecodeError::LimitExceeded(s) => write!(f, "limit exceeded: {s}"),
        }
    }
}

impl std::error::Error for BytecodeError {}

#[derive(Debug, Clone)]
pub struct Limits {
    pub max_instructions: usize,
    pub max_loop_iterations: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_instructions: 100_000,
            max_loop_iterations: 10_000,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BytecodeBinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Ne,
    Gt,
    Lt,
    Ge,
    Le,
    And,
    Or,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BytecodeUnaryOp {
    Not,
    BitNot,
    Neg,
}

#[derive(Debug, Clone)]
pub enum Instruction {
    ReadU8 {
        slot: u16,
    },
    ReadU16 {
        slot: u16,
        le: bool,
    },
    ReadU24 {
        slot: u16,
        le: bool,
    },
    ReadU32 {
        slot: u16,
        le: bool,
    },
    ReadSlice {
        len_slot: u16,
        dst_slot: u16,
    },
    /// Read `bits` bits from the current bit position (bitfield{k}); advances
    /// the bit cursor, carrying into `input_pos` across byte boundaries.
    ReadBits {
        bits: u8,
        slot: u16,
    },
    /// Consume the rest of the input (`bytes[EOF]` / `bytes[..]` / `bytes[stream]`).
    /// Stores the remaining byte count in `slot` and records the payload start
    /// offset (for layered `bind` dispatch) under `name`.
    ReadRest {
        name: String,
        slot: u16,
    },
    Validate {
        pred_slot: u16,
        message: String,
    },
    EmitField {
        name: String,
        slot: u16,
    },
    // New for expressions and control flow
    LoadConst {
        value: u64,
        dst: u16,
    },
    BinOp {
        op: BytecodeBinOp,
        left: u16,
        right: u16,
        dst: u16,
    },
    UnaryOp {
        op: BytecodeUnaryOp,
        src: u16,
        dst: u16,
    },
    CopySlot {
        src: u16,
        dst: u16,
    },
    // __current_offset management
    UpdateOffset {
        slot: u16,
    },
    Jump {
        target: u16,
    },
    JumpIfZero {
        cond_slot: u16,
        target: u16,
    },
    Return,
}

#[derive(Debug, Clone)]
pub struct BytecodeProgram {
    pub instructions: Vec<Instruction>,
    pub slot_count: usize,
}

pub struct BytecodeVm {
    slots: Vec<u64>,
    input: Vec<u8>,
    input_pos: usize,
    /// Bit offset (0..7) within the current byte for `bitfield{k}` reads.
    bit_offset: u8,
    limits: Limits,
    instructions_executed: usize,
    loop_iterations: usize,
    emitted: Vec<(String, u64)>,
    /// `(field_name, offset_before)` for each `ReadRest` (bytes[EOF]/bytes[..]/bytes[stream]),
    /// so the runner can sub-dispatch a bound layer message from the payload tail.
    rest_starts: Vec<(String, usize)>,
    /// Per-slot "was written at runtime" flag. Compile-time `MessageRef` inlining
    /// (bounded by `MAX_REF_DEPTH`) registers slots for every unrolled recursion
    /// level, but only the levels actually reached by the data execute. The
    /// runner uses this to exclude phantom nested fields/lets from the output
    /// context (e.g. diameter's `avps.a.grouped.inner.*` when no grouped AVP ran).
    slot_touched: Vec<bool>,
}

impl BytecodeVm {
    pub fn new(slot_count: usize) -> Self {
        Self::with_limits(slot_count, Limits::default())
    }

    pub fn with_limits(slot_count: usize, limits: Limits) -> Self {
        Self {
            slots: vec![0; slot_count.max(32)],
            input: vec![],
            input_pos: 0,
            bit_offset: 0,
            limits,
            instructions_executed: 0,
            loop_iterations: 0,
            emitted: vec![],
            rest_starts: vec![],
            slot_touched: vec![false; slot_count.max(32)],
        }
    }

    pub fn load_input(&mut self, data: &[u8]) {
        self.input = data.to_vec();
        self.input_pos = 0;
        self.bit_offset = 0;
    }

    pub fn get_slot(&self, slot: u16) -> u64 {
        self.slots.get(slot as usize).copied().unwrap_or(0)
    }

    pub fn set_slot(&mut self, slot: u16, val: u64) {
        if (slot as usize) < self.slots.len() {
            self.slots[slot as usize] = val;
        }
    }

    /// Write a slot and mark it touched at runtime (used for output filtering).
    fn write_slot(&mut self, slot: u16, val: u64) {
        let i = slot as usize;
        if i < self.slots.len() {
            self.slots[i] = val;
            self.slot_touched[i] = true;
        }
    }

    /// Whether `slot` was written by an executed instruction (vs. a
    /// compile-time-registered but runtime-unreached phantom slot).
    pub fn slot_touched(&self, slot: u16) -> bool {
        self.slot_touched
            .get(slot as usize)
            .copied()
            .unwrap_or(false)
    }

    pub fn current_offset(&self) -> usize {
        self.input_pos
    }

    /// Fields recorded by `EmitField` instructions, in emission order.
    pub fn emitted(&self) -> &[(String, u64)] {
        &self.emitted
    }

    /// `(field_name, offset_before)` for each terminal `bytes[EOF]/bytes[..]/bytes[stream]`
    /// read — the start of the payload tail, for layered `bind` dispatch.
    pub fn rest_starts(&self) -> &[(String, usize)] {
        &self.rest_starts
    }

    fn read_bytes<const N: usize>(&self, pos: usize) -> Option<[u8; N]> {
        if pos + N <= self.input.len() {
            let mut out = [0u8; N];
            out.copy_from_slice(&self.input[pos..pos + N]);
            Some(out)
        } else {
            None
        }
    }

    /// Align the cursor to the next byte boundary after `bitfield` reads.
    fn align_to_byte(&mut self) {
        if self.bit_offset != 0 {
            self.input_pos += 1;
            self.bit_offset = 0;
        }
    }

    /// Read `bits` bits from the bit stream at (input_pos, bit_offset).
    fn read_bits(&mut self, bits: u8) -> u64 {
        let mut value: u64 = 0;
        let mut remaining = bits as usize;
        while remaining > 0 {
            if self.input_pos >= self.input.len() {
                break;
            }
            let byte = self.input[self.input_pos];
            let avail = 8 - self.bit_offset as usize;
            let take = avail.min(remaining);
            let mask = (1u16 << take) - 1;
            let extracted = (byte >> (avail - take)) as u16 & mask;
            value = (value << take) | extracted as u64;
            remaining -= take;
            self.bit_offset += take as u8;
            if self.bit_offset == 8 {
                self.input_pos += 1;
                self.bit_offset = 0;
            }
        }
        value
    }

    pub fn run(&mut self, program: &BytecodeProgram) -> Result<(), BytecodeError> {
        self.instructions_executed = 0;
        let mut ip: usize = 0;
        let instrs = &program.instructions;

        self.loop_iterations = 0;
        while ip < instrs.len() {
            if self.instructions_executed > self.limits.max_instructions {
                return Err(BytecodeError::LimitExceeded(format!(
                    "exceeded max instructions {}",
                    self.limits.max_instructions
                )));
            }
            self.instructions_executed += 1;

            let instr = &instrs[ip];
            match instr {
                Instruction::UnaryOp { op, src, dst } => {
                    let v = self.slots[*src as usize];
                    let val = match op {
                        BytecodeUnaryOp::Not => {
                            if v == 0 {
                                1
                            } else {
                                0
                            }
                        }
                        BytecodeUnaryOp::BitNot => !v,
                        BytecodeUnaryOp::Neg => 0u64.wrapping_sub(v),
                    };
                    self.write_slot(*dst, val);
                    ip += 1;
                }
                Instruction::CopySlot { src, dst } => {
                    let val = self.slots[*src as usize];
                    self.write_slot(*dst, val);
                    ip += 1;
                }
                Instruction::ReadU8 { slot } => {
                    self.align_to_byte();
                    if self.input_pos < self.input.len() {
                        self.write_slot(*slot, self.input[self.input_pos] as u64);
                        self.input_pos += 1;
                    } else {
                        self.write_slot(*slot, 0);
                    }
                    ip += 1;
                }
                Instruction::ReadU16 { slot, le } => {
                    self.align_to_byte();
                    if let Some(b) = self.read_bytes::<2>(self.input_pos) {
                        let val = if *le {
                            u16::from_le_bytes(b) as u64
                        } else {
                            u16::from_be_bytes(b) as u64
                        };
                        self.write_slot(*slot, val);
                        self.input_pos += 2;
                    } else {
                        self.write_slot(*slot, 0);
                    }
                    ip += 1;
                }
                Instruction::ReadU24 { slot, le } => {
                    self.align_to_byte();
                    if let Some(b) = self.read_bytes::<3>(self.input_pos) {
                        let val = if *le {
                            ((b[2] as u64) << 16) | ((b[1] as u64) << 8) | (b[0] as u64)
                        } else {
                            ((b[0] as u64) << 16) | ((b[1] as u64) << 8) | (b[2] as u64)
                        };
                        self.write_slot(*slot, val);
                        self.input_pos += 3;
                    } else {
                        self.write_slot(*slot, 0);
                    }
                    ip += 1;
                }
                Instruction::ReadU32 { slot, le } => {
                    self.align_to_byte();
                    if let Some(b) = self.read_bytes::<4>(self.input_pos) {
                        let val = if *le {
                            u32::from_le_bytes(b) as u64
                        } else {
                            u32::from_be_bytes(b) as u64
                        };
                        self.write_slot(*slot, val);
                        self.input_pos += 4;
                    } else {
                        self.write_slot(*slot, 0);
                    }
                    ip += 1;
                }
                Instruction::ReadSlice { len_slot, dst_slot } => {
                    self.align_to_byte();
                    let len = if *len_slot == u16::MAX {
                        (self.input.len() - self.input_pos).min(16)
                    } else if (*len_slot as usize) < self.slots.len() {
                        self.slots[*len_slot as usize] as usize
                    } else {
                        0usize
                    };
                    let mut val = 0u64;
                    for i in 0..len.min(8) {
                        if self.input_pos + i < self.input.len() {
                            val = (val << 8) | (self.input[self.input_pos + i] as u64);
                        }
                    }
                    self.write_slot(*dst_slot, val);
                    self.input_pos += len;
                    ip += 1;
                }
                Instruction::ReadBits { bits, slot } => {
                    let val = self.read_bits(*bits);
                    self.write_slot(*slot, val);
                    ip += 1;
                }
                Instruction::ReadRest { name, slot } => {
                    self.align_to_byte();
                    let before = self.input_pos;
                    let remaining = self.input.len().saturating_sub(self.input_pos);
                    self.write_slot(*slot, remaining as u64);
                    self.input_pos = self.input.len();
                    self.rest_starts.push((name.clone(), before));
                    ip += 1;
                }
                Instruction::Validate { pred_slot, message } => {
                    let pred = self.slots[*pred_slot as usize];
                    if pred == 0 {
                        return Err(BytecodeError::Constraint(message.clone()));
                    }
                    ip += 1;
                }
                Instruction::EmitField { name, slot } => {
                    let val = self.slots[*slot as usize];
                    self.emitted.push((name.clone(), val));
                    ip += 1;
                }
                Instruction::LoadConst { value, dst } => {
                    self.write_slot(*dst, *value);
                    ip += 1;
                }
                Instruction::BinOp {
                    op,
                    left,
                    right,
                    dst,
                } => {
                    let l = self.slots[*left as usize];
                    let r = self.slots[*right as usize];
                    let val = match op {
                        BytecodeBinOp::Add => l + r,
                        BytecodeBinOp::Sub => l.saturating_sub(r),
                        BytecodeBinOp::Mul => l * r,
                        BytecodeBinOp::Div => {
                            if r != 0 {
                                l / r
                            } else {
                                0
                            }
                        }
                        BytecodeBinOp::Mod => {
                            if r != 0 {
                                l % r
                            } else {
                                0
                            }
                        }
                        BytecodeBinOp::Eq => {
                            if l == r {
                                1
                            } else {
                                0
                            }
                        }
                        BytecodeBinOp::Ne => {
                            if l != r {
                                1
                            } else {
                                0
                            }
                        }
                        BytecodeBinOp::Gt => {
                            if l > r {
                                1
                            } else {
                                0
                            }
                        }
                        BytecodeBinOp::Lt => {
                            if l < r {
                                1
                            } else {
                                0
                            }
                        }
                        BytecodeBinOp::Ge => {
                            if l >= r {
                                1
                            } else {
                                0
                            }
                        }
                        BytecodeBinOp::Le => {
                            if l <= r {
                                1
                            } else {
                                0
                            }
                        }
                        BytecodeBinOp::And => {
                            if l != 0 && r != 0 {
                                1
                            } else {
                                0
                            }
                        }
                        BytecodeBinOp::Or => {
                            if l != 0 || r != 0 {
                                1
                            } else {
                                0
                            }
                        }
                        BytecodeBinOp::BitAnd => l & r,
                        BytecodeBinOp::BitOr => l | r,
                        BytecodeBinOp::BitXor => l ^ r,
                        BytecodeBinOp::Shl => l << (r as u32),
                        BytecodeBinOp::Shr => l >> (r as u32),
                    };
                    self.write_slot(*dst, val);
                    ip += 1;
                }
                Instruction::UpdateOffset { slot } => {
                    self.write_slot(*slot, self.input_pos as u64);
                    ip += 1;
                }
                Instruction::Jump { target } => {
                    ip = *target as usize;
                }
                Instruction::JumpIfZero { cond_slot, target } => {
                    if self.slots[*cond_slot as usize] == 0 {
                        if (*target as usize) < ip {
                            self.loop_iterations += 1;
                            if self.loop_iterations > self.limits.max_loop_iterations {
                                return Err(BytecodeError::LimitExceeded(format!(
                                    "exceeded max loop iterations {}",
                                    self.limits.max_loop_iterations
                                )));
                            }
                        }
                        ip = *target as usize;
                    } else {
                        ip += 1;
                    }
                }
                Instruction::Return => {
                    return Ok(());
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_read_u16_and_return() {
        let program = BytecodeProgram {
            instructions: vec![
                Instruction::ReadU16 { slot: 0, le: false },
                Instruction::ReadU16 { slot: 1, le: false },
                Instruction::Validate {
                    pred_slot: 0,
                    message: "hw_type must be 1".to_string(),
                },
                Instruction::Return,
            ],
            slot_count: 4,
        };

        let mut vm = BytecodeVm::new(4);
        vm.load_input(&[0x00, 0x01, 0x08, 0x00]);
        assert!(vm.run(&program).is_ok());
        assert_eq!(vm.get_slot(0), 1);
        assert_eq!(vm.get_slot(1), 0x0800);
    }

    #[test]
    fn limit_exceeded_on_infinite_jump() {
        let program = BytecodeProgram {
            instructions: vec![Instruction::Jump { target: 0 }],
            slot_count: 1,
        };
        let limits = Limits {
            max_instructions: 3,
            max_loop_iterations: 1,
        };
        let mut vm = BytecodeVm::with_limits(1, limits);
        let err = vm.run(&program).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("limit exceeded"), "got: {msg}");
    }
}
