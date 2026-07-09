//! `EventNode` — the owned, immutable event stored in RingBuffers, per
//! `docs/idea/spec/04-type-system.md` §3 and `docs/idea/spec/07-runtime.md`
//! §2 ("EventNode stored in RingBuffer (owned, evictable)").

use airpulse_dsl_ir::FieldIdx;
use airpulse_dsl_types::{EventId, EventTime, EventType, ScopeId};

/// Provenance basics for an event — where in the capture/stream it came from
/// (the `span` component of `04` §3 `EventNode`; used for SARIF evidence
/// locations and late-event audit, `08` §4).
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct EventProvenance {
    /// Capture frame index (PCAP frame number), when known.
    pub frame: Option<u64>,
    /// Source identifier for live multi-source captures (`08` §2.3).
    pub source: Option<Box<str>>,
}

/// One owned event (`04` §3 `EventNode { id, type, time, scope_key, fields,
/// span }`) — immutable once constructed; rings evict but never mutate it.
///
/// Metric fields are stored as `(FieldIdx, i64)` pairs sorted by field index,
/// matching how the evaluator addresses them:
/// [`PredOp::LoadEventField`](airpulse_dsl_ir::PredOp::LoadEventField) loads
/// `field: FieldIdx` into an `i64` slot register (`06` §4/§6 — "field_idx
/// (u16) in opcodes, no string lookup in hot-path"). Catalog resolution
/// (`05` §1) interns non-integer field values (enum strings, scope keys via
/// their deterministic hash) into the `i64` slot domain.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EventNode {
    /// Engine-assigned event id — the `PendingMatch.anchor_event` lookup key
    /// (`07` §2).
    pub id: EventId,
    /// Catalog event type, e.g. `tcp.retransmission_burst`.
    pub event_type: EventType,
    /// Event-time in ms — the RingBuffer sort key (`07` §3).
    pub time: EventTime,
    /// Partition this copy of the event belongs to (`07` §8 — fan-out clones
    /// one `EventNode` per matching scope).
    pub scope: ScopeId,
    /// Metric fields, sorted by `FieldIdx` (see type-level docs).
    fields: Box<[(FieldIdx, i64)]>,
    /// Capture provenance (`04` §3 `span`).
    pub provenance: EventProvenance,
}

impl EventNode {
    /// Builds an event, sorting `fields` by index so lookups are O(log n).
    /// Duplicate indices keep the first occurrence.
    #[must_use]
    pub fn new(
        id: EventId,
        event_type: EventType,
        time: EventTime,
        scope: ScopeId,
        mut fields: Vec<(FieldIdx, i64)>,
        provenance: EventProvenance,
    ) -> EventNode {
        fields.sort_by_key(|(idx, _)| *idx);
        fields.dedup_by_key(|(idx, _)| *idx);
        EventNode { id, event_type, time, scope, fields: fields.into_boxed_slice(), provenance }
    }

    /// Field lookup by catalog index — the store-side counterpart of
    /// `LOAD_EVENT_FIELD` (`06` §4). `None` for fields absent on this event
    /// (`Option<τ>` conditional fields, `04` §1).
    #[must_use]
    pub fn field(&self, idx: FieldIdx) -> Option<i64> {
        self.fields
            .binary_search_by_key(&idx, |(i, _)| *i)
            .ok()
            .map(|pos| self.fields[pos].1)
    }

    /// All fields, sorted by index.
    #[must_use]
    pub fn fields(&self) -> &[(FieldIdx, i64)] {
        &self.fields
    }
}
