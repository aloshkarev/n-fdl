//! Bytecode VM with support for loops (jumps + condition evaluation) for production v1.

use crate::error::RuntimeError;

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
    },
    ReadU32 {
        slot: u16,
    },
    ReadSlice {
        len_slot: u16,
        dst_slot: u16,
    },
    Validate {
        pred_slot: u16,
    },
    EmitField {
        name: u16,
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
    current_offset: usize,
    limits: Limits,
    instructions_executed: usize,
    loop_iterations: usize,
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
            current_offset: 0,
            limits,
            instructions_executed: 0,
            loop_iterations: 0,
        }
    }

    pub fn load_input(&mut self, data: &[u8]) {
        self.input = data.to_vec();
        self.input_pos = 0;
    }

    pub fn get_slot(&self, slot: u16) -> u64 {
        self.slots.get(slot as usize).copied().unwrap_or(0)
    }

    pub fn set_slot(&mut self, slot: u16, val: u64) {
        if (slot as usize) < self.slots.len() {
            self.slots[slot as usize] = val;
        }
    }

    pub fn current_offset(&self) -> usize {
        self.input_pos
    }

    pub fn run(&mut self, program: &BytecodeProgram) -> Result<(), RuntimeError> {
        self.instructions_executed = 0;
        let mut ip: usize = 0;
        let instrs = &program.instructions;

        self.loop_iterations = 0;
        while ip < instrs.len() {
            if self.instructions_executed > self.limits.max_instructions {
                return Err(RuntimeError::LimitExceeded(format!(
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
                    self.slots[*dst as usize] = val;
                    ip += 1;
                }
                Instruction::CopySlot { src, dst } => {
                    let val = self.slots[*src as usize];
                    self.slots[*dst as usize] = val;
                    ip += 1;
                }
                Instruction::ReadU8 { slot } => {
                    if self.input_pos < self.input.len() {
                        self.slots[*slot as usize] = self.input[self.input_pos] as u64;
                        self.input_pos += 1;
                    } else {
                        self.slots[*slot as usize] = 0;
                    }
                    ip += 1;
                }
                Instruction::ReadU16 { slot } => {
                    if self.input_pos + 1 < self.input.len() {
                        let val = u16::from_be_bytes([
                            self.input[self.input_pos],
                            self.input[self.input_pos + 1],
                        ]) as u64;
                        self.slots[*slot as usize] = val;
                        self.input_pos += 2;
                    } else {
                        self.slots[*slot as usize] = 0;
                    }
                    ip += 1;
                }
                Instruction::ReadU32 { slot } => {
                    if self.input_pos + 3 < self.input.len() {
                        let val = u32::from_be_bytes([
                            self.input[self.input_pos],
                            self.input[self.input_pos + 1],
                            self.input[self.input_pos + 2],
                            self.input[self.input_pos + 3],
                        ]) as u64;
                        self.slots[*slot as usize] = val;
                        self.input_pos += 4;
                    } else {
                        self.slots[*slot as usize] = 0;
                    }
                    ip += 1;
                }
                Instruction::ReadSlice { len_slot, dst_slot } => {
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
                    self.slots[*dst_slot as usize] = val;
                    self.input_pos += len;
                    ip += 1;
                }
                Instruction::Validate { pred_slot } => {
                    let pred = self.slots[*pred_slot as usize];
                    if pred == 0 {
                        return Err(RuntimeError::Constraint("validation failed".into()));
                    }
                    ip += 1;
                }
                Instruction::EmitField { .. } => {
                    ip += 1;
                }
                Instruction::LoadConst { value, dst } => {
                    self.slots[*dst as usize] = *value;
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
                    self.slots[*dst as usize] = val;
                    ip += 1;
                }
                Instruction::UpdateOffset { slot } => {
                    self.slots[*slot as usize] = self.input_pos as u64;
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
                                return Err(RuntimeError::LimitExceeded(format!(
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
