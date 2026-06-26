use nfdl_runtime::{Event, EventBus, VecSink};

#[test]
fn basic_event_bus() {
    let sink = VecSink::new();
    let mut bus = EventBus::new(sink);

    bus.emit(Event::Message {
        msg_type: "ArpPacket".into(),
        size: 28,
    });
    bus.emit(Event::Emit {
        name: "TCP_SYN_SEEN".into(),
    });
    bus.emit(Event::FsmTransition {
        from: 0,
        to: 1,
        machine: "Connection".into(),
    });

    assert_eq!(bus.sink.events.len(), 3);
}
