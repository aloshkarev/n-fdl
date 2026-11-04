//! `EventNode` — the owned, immutable event stored in RingBuffers, per
//! `docs/idea/spec/04-type-system.md` §3 and `docs/idea/spec/07-runtime.md`
//! §2 ("EventNode stored in RingBuffer (owned, evictable)").

use airpulse_dsl_ir::FieldIdx;
use airpulse_dsl_types::{
    BoundedList, EventId, EventTime, EventType, ListCoverage, ListKind, MAX_CATALOG_LIST_VALUES,
    ScopeId,
};

/// Maximum values carried by one event `IntList` field.
///
/// Input is sorted and deduplicated before this bound is applied, so truncation
/// is deterministic and retains the lowest values. A longer input is
/// [`ListCoverage::Partial`] and names the `IntList` cap.
pub const MAX_EVENT_INT_LIST_VALUES: usize = MAX_CATALOG_LIST_VALUES;

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
    /// Bounded typed `IntList` sidecar, sorted by field index. Scalar
    /// predicates continue to address only [`Self::fields`].
    int_list_fields: Box<[(FieldIdx, Box<[i64]>)]>,
    /// Coverage of each `IntList` sidecar, sorted by the same field index.
    int_list_coverage: Box<[(FieldIdx, ListCoverage)]>,
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
        EventNode {
            id,
            event_type,
            time,
            scope,
            fields: fields.into_boxed_slice(),
            int_list_fields: Box::new([]),
            int_list_coverage: Box::new([]),
            provenance,
        }
    }

    /// Adds or replaces one bounded `IntList` field.
    ///
    /// Values are sorted, deduplicated, and deterministically truncated to
    /// [`MAX_EVENT_INT_LIST_VALUES`]. A truncated field records
    /// [`ListCoverage::Partial`] with the `IntList` cap name.
    #[must_use]
    pub fn with_int_list_field(mut self, idx: FieldIdx, values: Vec<i64>) -> EventNode {
        let bounded = BoundedList::from_sorted_dedup(ListKind::IntList, values);
        let (values, coverage) = bounded.into_parts();

        let mut fields = self.int_list_fields.into_vec();
        match fields.binary_search_by_key(&idx, |(field_idx, _)| *field_idx) {
            Ok(position) => fields[position] = (idx, values),
            Err(position) => fields.insert(position, (idx, values)),
        }
        self.int_list_fields = fields.into_boxed_slice();

        let mut coverages = self.int_list_coverage.into_vec();
        match coverages.binary_search_by_key(&idx, |(field_idx, _)| *field_idx) {
            Ok(position) => coverages[position] = (idx, coverage),
            Err(position) => coverages.insert(position, (idx, coverage)),
        }
        self.int_list_coverage = coverages.into_boxed_slice();
        self
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

    /// Typed-list field lookup. Numeric predicates intentionally do not use
    /// this sidecar.
    #[must_use]
    pub fn int_list_field(&self, idx: FieldIdx) -> Option<&[i64]> {
        self.int_list_fields
            .binary_search_by_key(&idx, |(field_idx, _)| *field_idx)
            .ok()
            .map(|position| self.int_list_fields[position].1.as_ref())
    }

    /// All typed-list fields, sorted by field index.
    #[must_use]
    pub fn int_list_fields(&self) -> &[(FieldIdx, Box<[i64]>)] {
        &self.int_list_fields
    }

    /// Coverage of one `IntList` field. `None` when that field was not set.
    #[must_use]
    pub fn int_list_coverage(&self, idx: FieldIdx) -> Option<&ListCoverage> {
        self.int_list_coverage
            .binary_search_by_key(&idx, |(field_idx, _)| *field_idx)
            .ok()
            .map(|position| &self.int_list_coverage[position].1)
    }
}
