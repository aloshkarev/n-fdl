//! Minimal Event Bus (one-directional sink)

#[derive(Debug, Clone)]
pub enum Event {
    Message { msg_type: String, size: usize },
    FsmTransition { from: u32, to: u32, machine: String },
    Emit { name: String },
    SessionExpired { key_hash: u64 },
    Diagnostic { code: String, message: String },
    Anomaly { kind: String },
}

pub trait EventSink {
    fn emit(&mut self, event: Event);
}

pub struct VecSink {
    pub events: Vec<Event>,
}

impl VecSink {
    pub fn new() -> Self {
        Self { events: Vec::new() }
    }
}

impl Default for VecSink {
    fn default() -> Self {
        Self::new()
    }
}

impl EventSink for VecSink {
    fn emit(&mut self, event: Event) {
        self.events.push(event);
    }
}

pub struct EventBus<S: EventSink> {
    pub sink: S,
}

impl<S: EventSink> EventBus<S> {
    pub fn new(sink: S) -> Self {
        Self { sink }
    }

    pub fn emit(&mut self, event: Event) {
        self.sink.emit(event);
    }
}
