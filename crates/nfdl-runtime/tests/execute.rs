use nfdl_runtime::{VmState, parse_and_run};
use nfdl_syntax::Parser;

#[test]
fn execute_arp_datagram() {
    let src = include_str!("../../../docs/examples/arp.nfdl");
    let mut p = Parser::new(src);
    let proto = p.parse_protocol().expect("parse failed");

    let mut vm = VmState::new();
    let result = vm.execute_datagram(&proto);
    assert!(result.is_ok(), "ARP execution should succeed");
}

#[test]
fn execute_arp_real_protocol() {
    let src = include_str!("../../../docs/examples/arp.nfdl");
    let (proto, events) = parse_and_run(src).expect("real protocol run failed");

    assert_eq!(proto.name, "ARP");
    assert!(!proto.messages.is_empty());
    let first_msg = &proto.messages[0];
    assert!(first_msg.fields.len() >= 8, "ARP should have many fields");

    // Check that we parsed a bytes[len] field
    let has_bytes_len = first_msg
        .fields
        .iter()
        .any(|f| matches!(f.ty, nfdl_syntax::ast::NfdlType::Bytes { .. }));
    assert!(has_bytes_len, "should have dependent length bytes field");

    // Should have emitted events
    assert!(!events.is_empty());
}

#[test]
fn execute_gtpu_with_depth_limit() {
    let src = include_str!("../../../docs/examples/gtpu.nfdl");
    let mut p = Parser::new(src);
    let proto = p.parse_protocol().expect("parse failed");

    let mut vm = VmState::new();
    // Should succeed because depth is low
    let result = vm.execute_datagram(&proto);
    assert!(result.is_ok());
}
