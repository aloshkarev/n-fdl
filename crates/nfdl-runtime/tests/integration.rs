use nfdl_runtime::{BytecodeVm, protocol_to_bytecode};
use nfdl_syntax::Parser;

#[test]
fn arp_protocol_to_bytecode() {
    let src = include_str!("../../../docs/examples/arp.nfdl");
    let mut p = Parser::new(src);
    let proto = p.parse_protocol().expect("parse failed");

    let program = protocol_to_bytecode(&proto);
    let mut vm = BytecodeVm::new(program.slot_count);
    // Real 28-byte Ethernet/IPv4 ARP packet so refinements (proto_type==0x0800,
    // hw_len>0, proto_len>0, opcode in 1..=4) are satisfied.
    let mut pkt = vec![
        0x00, 0x01, // hw_type = Ethernet
        0x08, 0x00, // proto_type = IPv4
        0x06, // hw_len = 6
        0x04, // proto_len = 4
        0x00, 0x01, // opcode = request
    ];
    pkt.extend_from_slice(&[0xAA; 6]); // sender_mac
    pkt.extend_from_slice(&[192, 168, 1, 1]); // sender_ip
    pkt.extend_from_slice(&[0x00; 6]); // target_mac
    pkt.extend_from_slice(&[192, 168, 1, 2]); // target_ip
    vm.load_input(&pkt);
    let result = vm.run(&program);

    assert!(result.is_ok(), "ARP bytecode run failed: {:?}", result);
}
