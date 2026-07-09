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
/// - `buf.len() <= capacity` — overflow drops the oldest event and signals
///   `ADGL3005 RingBufferSpill` (ADR-011 spill policy).
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
        RingBuffer { buf: VecDeque::new(), scope, capacity: capacity.max(1) }
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
    /// If the ring is full the oldest (lowest-time) event is dropped and an
    /// `ADGL3005 RingBufferSpill` diagnostic is returned (ADR-011); the
    /// store itself never logs (`07` §9 / crate contract).
    #[must_use = "spill diagnostics must be routed by the caller (ADR-011)"]
    pub fn push(&mut self, event: EventNode) -> Option<StoreDiagnostic> {
        let spill = if self.buf.len() == self.capacity {
            // Drop oldest (lowest time — front, by the sort invariant).
            self.buf.pop_front().map(|dropped| StoreDiagnostic::RingBufferSpill {
                scope: self.scope,
                dropped: dropped.id,
                capacity: self.capacity,
            })
        } else {
            None
        };
        // First index whose time exceeds the new event's time: equal-time
        // events keep arrival order (insert after them).
        let pos = self.buf.partition_point(|e| e.time <= event.time);
        self.buf.insert(pos, event);
        spill
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
        let start = self.buf.partition_point(|e| e.time < lo);
        self.buf.iter().skip(start).take_while(move |e| e.time <= hi)
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
        let cutoff = watermark.sub(max_lookback);
        let mut evicted = 0;
        while let Some(front) = self.buf.front() {
            if front.time < cutoff {
                self.buf.pop_front();
                evicted += 1;
            } else {
                break;
            }
        }
        evicted
    }

    /// All buffered events in time order (diagnostics/test support).
    pub fn iter(&self) -> impl Iterator<Item = &EventNode> {
        self.buf.iter()
    }
}
