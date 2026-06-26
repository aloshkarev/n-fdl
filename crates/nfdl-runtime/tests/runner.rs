use nfdl_runtime::parse_and_run;

#[test]
fn end_to_end_arp() {
    let src = include_str!("../../../docs/examples/arp.nfdl");
    let (proto, events) = parse_and_run(src).expect("run failed");
    assert_eq!(proto.name, "ARP");
    assert!(!events.is_empty());
}
