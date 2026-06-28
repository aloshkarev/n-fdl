use nfdl_runtime::{BytecodeProgram, BytecodeVm, Instruction};

#[test]
fn simple_arp_bytecode() {
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
    // Minimal ARP-like prefix: hw_type=1 (Ethernet), proto_type=0x0800 (IPv4)
    vm.load_input(&[0x00, 0x01, 0x08, 0x00]);
    let result = vm.run(&program);
    assert!(result.is_ok());
}
