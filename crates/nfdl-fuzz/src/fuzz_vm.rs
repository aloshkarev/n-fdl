//! Fuzz target for Bytecode VM (M6)

use nfdl_runtime::{BytecodeProgram, BytecodeVm, Instruction};

pub fn fuzz_bytecode_vm(data: &[u8]) {
    if data.len() < 4 { return; }
    
    let mut instructions = vec![];
    for chunk in data.chunks(4) {
        if chunk.len() == 4 {
            let opcode = chunk[0];
            let slot = u16::from_le_bytes([chunk[1], chunk[2]]);
            
            let instr = match opcode % 6 {
                0 => Instruction::ReadU16 { slot },
                1 => Instruction::ReadU8 { slot },
                2 => Instruction::ReadSlice { len_slot: slot, dst_slot: slot + 1 },
                3 => Instruction::Validate { pred_slot: slot },
                4 => Instruction::EmitField { name: slot, slot: slot + 1 },
                _ => Instruction::Return,
            };
            instructions.push(instr);
        }
    }
    
    if instructions.is_empty() { return; }
    instructions.push(Instruction::Return);
    
    let program = BytecodeProgram {
        instructions,
        slot_count: 16,
    };
    
    let mut vm = BytecodeVm::new(16);
    let _ = vm.run(&program); // should never panic
}

#[cfg(fuzzing)]
fn main() {
    // cargo fuzz run fuzz_vm
}
