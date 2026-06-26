use nfdl_runtime::{BytecodeVm, protocol_to_bytecode};
use nfdl_syntax::Parser;

#[test]
fn arp_protocol_to_bytecode() {
    let src = include_str!("../../../docs/examples/arp.nfdl");
    let mut p = Parser::new(src);
    let proto = p.parse_protocol().expect("parse failed");

    let program = protocol_to_bytecode(&proto);
    let mut vm = BytecodeVm::new(program.slot_count);
    let result = vm.run(&program);

    assert!(result.is_ok());
}
