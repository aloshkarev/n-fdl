//! Per-scope time-sorted event buffer per `docs/idea/spec/07-runtime.md`
//! §3–§4 and ADR-011.

use std::collections::VecDeque;

use airpulse_dsl_types::{DurationMs, EventTime, ScopeId};

use crate::diag::StoreDiagnostic;
use crate::event::EventNode;

/// Capacity-bounded, time-sorted per-scope event buffer
/// (`07` §3 `RingBuffer { buf: VecDeque<EventNode> // sorted by time, scope,
/// capacity }`).
///
/// Invariants:
/// - `buf` is non-decreasing in `EventNode.time` (late events are inserted
///   in time position, `08` §4 offline accept);
/// - `buf.len() <= capacity` — overflow drops the oldest event that no
///   caller-supplied keep-set names, and signals `ADGL3005 RingBufferSpill`
///   (ADR-011 spill policy). If every resident event is kept, the new event
///   is not inserted.
#[derive(Debug, Clone)]
pub struct RingBuffer {
    buf: VecDeque<EventNode>,
    scope: ScopeId,
    capacity: usize,
}

impl RingBuffer {
    /// Creates an empty ring for `scope` with
    /// `capacity = max_ringbuffer_events_per_scope` (ADR-011). A zero
    /// capacity is clamped to 1 so `push` stays total (no data-path panic,
    /// `07` §9).
    #[must_use]
    pub fn new(scope: ScopeId, capacity: usize) -> RingBuffer {
        RingBuffer {
            buf: VecDeque::new(),
            scope,
            capacity: capacity.max(1),
        }
    }

    /// The partition this ring buffers events for.
    #[must_use]
    pub const fn scope(&self) -> ScopeId {
        self.scope
    }

    /// Configured capacity (`max_ringbuffer_events_per_scope`).
    #[must_use]
    pub const fn capacity(&self) -> usize {
        self.capacity
    }

    /// Buffered event count; always `<= capacity()`.
    #[must_use]
    pub fn len(&self) -> usize {
        self.buf.len()
    }

    /// Whether the ring holds no events.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    /// Inserts `event` maintaining time order.
    ///
    /// In-order events (the common case) append at the back; late events
    /// (`08` §4 — offline accepts out-of-order capture events) are inserted
    /// at their time position, *after* any already-buffered event with the
    /// same time (arrival order is the deterministic tie-break).
    ///
    /// If the ring is full the oldest event is dropped and an
    /// `ADGL3005 RingBufferSpill` diagnostic is returned (ADR-011); the
    /// store itself never logs (`07` §9 / crate contract).
    #[must_use = "spill diagnostics must be routed by the caller (ADR-011)"]
    pub fn push(&mut self, event: EventNode) -> Option<StoreDiagnostic> {
        self.push_keeping(event, &std::collections::HashSet::new())
    }

    /// Like [`Self::push`], but an id in `keep` is not the spill victim.
    ///
    /// The victim is the oldest resident event whose id is not in `keep`.
    /// When every resident event is kept, `event` is not inserted and the
    /// diagnostic names that rejected id. `keep` is the unexpired wait
    /// anchors for this partition.
    #[must_use = "spill diagnostics must be routed by the caller (ADR-011)"]
    pub fn push_keeping(
        &mut self,
        event: EventNode,
        keep: &std::collections::HashSet<airpulse_dsl_types::EventId>,
    ) -> Option<StoreDiagnostic> {
        if self.buf.len() < self.capacity {
            self.insert_sorted(event);
            return None;
        }
        let dropped = if let Some(idx) = self.buf.iter().position(|e| !keep.contains(&e.id)) {
            let id = self
                .buf
                .remove(idx)
                .expect("position is inside the ring")
                .id;
            self.insert_sorted(event);
            id
        } else {
            event.id
        };
        Some(StoreDiagnostic::RingBufferSpill {
            scope: self.scope,
            dropped,
            capacity: self.capacity,
        })
    }

    /// First index whose time exceeds the new event's time: equal-time
    /// events keep arrival order (insert after them).
    fn insert_sorted(&mut self, event: EventNode) {
        let pos = self.buf.partition_point(|e| e.time <= event.time);
        self.buf.insert(pos, event);
    }

    /// Iterates events with `time ∈ [anchor - back, anchor + fwd]`,
    /// **inclusive both ends** (D4; `05-verification.md` §3.2, mirrored by
    /// the `WIN_IN` opcode contract `06` §4) — the correlate window scan
    /// `Ring.scan(T, time.window)` of `03-semantics.md` §3.2. Events come
    /// out in time order (earliest first — `03` §3.2 earliest-match).
    pub fn scan_window(
        &self,
        anchor: EventTime,
        back: DurationMs,
        fwd: DurationMs,
    ) -> impl Iterator<Item = &EventNode> {
        let lo = anchor.sub(back);
        let hi = anchor.add(fwd);
        self.scan_range(lo, hi)
    }

    /// Iterates events with `time ∈ [lo, hi]`, inclusive both ends.
    pub fn scan_range(&self, lo: EventTime, hi: EventTime) -> impl Iterator<Item = &EventNode> {
        let start = self.buf.partition_point(|e| e.time < lo);
        self.buf
            .iter()
            .skip(start)
            .take_while(move |e| e.time <= hi)
    }

    /// Looks up a buffered event by id (the `PendingMatch.anchor_event`
    /// resolution path, `07` §2 "anchor ref = EventId + RingBuffer-lookup").
    #[must_use]
    pub fn get(&self, id: airpulse_dsl_types::EventId) -> Option<&EventNode> {
        self.buf.iter().find(|e| e.id == id)
    }

    /// Watermark GC per `07-runtime.md` §4: evicts events **strictly older**
    /// than `watermark - max_lookback` (`front.time < cutoff`), front-first.
    /// Returns the number of evicted events.
    ///
    /// Safety of eviction relies on the MAX_LOOKBACK invariant
    /// (`05` §3.1 / `08` §6): `max_lookback > max(back, forward) + slack`
    /// ⇒ no unexpired `PendingMatch` references an evicted event.
    pub fn gc(&mut self, watermark: EventTime, max_lookback: DurationMs) -> usize {
        self.gc_keeping(watermark, max_lookback, &std::collections::HashSet::new())
    }

    /// Like [`Self::gc`], but an event id in `keep` stays even when it is
    /// older than the lookback. Callers pass anchors still named by an
    /// unexpired wait.
    pub fn gc_keeping(
        &mut self,
        watermark: EventTime,
        max_lookback: DurationMs,
        keep: &std::collections::HashSet<airpulse_dsl_types::EventId>,
    ) -> usize {
        let cutoff = watermark.sub(max_lookback);
        let before = self.buf.len();
        self.buf
            .retain(|event| event.time >= cutoff || keep.contains(&event.id));
        before - self.buf.len()
    }

    /// All buffered events in time order (diagnostics/test support).
    pub fn iter(&self) -> impl Iterator<Item = &EventNode> {
        self.buf.iter()
    }
}
