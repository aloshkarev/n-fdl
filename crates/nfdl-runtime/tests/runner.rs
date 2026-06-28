use nfdl_runtime::parse_and_run_with_data;

#[test]
fn end_to_end_arp() {
    let src = include_str!("../../../docs/examples/arp.nfdl");
    // Real 28-byte Ethernet/IPv4 ARP packet.
    let mut pkt = vec![
        0x00, 0x01, // hw_type
        0x08, 0x00, // proto_type = IPv4
        0x06, // hw_len
        0x04, // proto_len
        0x00, 0x01, // opcode
    ];
    pkt.extend_from_slice(&[0xAA; 6]);
    pkt.extend_from_slice(&[192, 168, 1, 1]);
    pkt.extend_from_slice(&[0x00; 6]);
    pkt.extend_from_slice(&[192, 168, 1, 2]);

    let (proto, _ctx, _final_state, events) =
        parse_and_run_with_data(src, &pkt).expect("run failed");
    assert_eq!(proto.name, "ARP");
    assert!(!events.is_empty());
}
