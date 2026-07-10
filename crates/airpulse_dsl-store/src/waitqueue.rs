//! Bounded per-scope WaitQueue of suspended matches per
//! `docs/idea/spec/08-stream-watermarking.md` §3 and ADR-011.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use airpulse_dsl_ir::PendingMatch;
use airpulse_dsl_types::{EventTime, ScopeId};

use crate::diag::StoreDiagnostic;

/// Per-scope min-heap of [`PendingMatch`]es ordered by `upper_bound`
/// (`07` §3 `pending: BinaryHeap<PendingMatch>`; `08` §3.1).
///
/// `PendingMatch: Ord` is ascending by `upper_bound` (ir crate), so entries
/// are wrapped in [`std::cmp::Reverse`] to turn Rust's max-`BinaryHeap` into
/// the min-heap the resume loop needs (`07` §8 "O(log n) pop min
/// upper_bound").
///
/// Invariant: `len() <= capacity` (`max_pending_per_scope`, ADR-011);
/// overflow drops the pending with the **largest** `upper_bound` (least
/// urgent) + `ADGL3004 WaitQueueSpill` (`08` §3.3).
#[derive(Debug, Clone)]
pub struct WaitQueue {
    heap: BinaryHeap<Reverse<PendingMatch>>,
    scope: ScopeId,
    capacity: usize,
}

impl WaitQueue {
    /// Creates an empty queue for `scope` with
    /// `capacity = max_pending_per_scope` (ADR-011). Zero capacity is
    /// clamped to 1 so `push` stays total.
    #[must_use]
    pub fn new(scope: ScopeId, capacity: usize) -> WaitQueue {
        WaitQueue {
            heap: BinaryHeap::new(),
            scope,
            capacity: capacity.max(1),
        }
    }

    /// The partition this queue belongs to.
    #[must_use]
    pub const fn scope(&self) -> ScopeId {
        self.scope
    }

    /// Configured capacity (`max_pending_per_scope`).
    #[must_use]
    pub const fn capacity(&self) -> usize {
        self.capacity
    }

    /// Suspended match count; always `<= capacity()`.
    #[must_use]
    pub fn len(&self) -> usize {
        self.heap.len()
    }

    /// Whether no matches are suspended.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.heap.is_empty()
    }

    /// Suspends a match (`08` §3.1). On overflow the entry with the largest
    /// `upper_bound` — which may be `pending` itself — is dropped and an
    /// `ADGL3004 WaitQueueSpill` diagnostic is returned (ADR-011 spill
    /// policy: "drop pending with the largest upper_bound").
    #[must_use = "spill diagnostics must be routed by the caller (ADR-011)"]
    pub fn push(&mut self, pending: PendingMatch) -> Option<StoreDiagnostic> {
        if self.heap.len() < self.capacity {
            self.heap.push(Reverse(pending));
            return None;
        }
        // Find the least urgent entry currently held. BinaryHeap has no
        // O(log n) max-removal for a min-heap; capacity overflow is the
        // rare degraded path (ADR-011), so an O(n) scan is acceptable.
        let current_max = self.heap.iter().map(|Reverse(p)| p).max().cloned();
        match current_max {
            Some(max) if pending < max => {
                // The new entry is more urgent: evict the stored maximum.
                let mut entries: Vec<PendingMatch> =
                    self.heap.drain().map(|Reverse(p)| p).collect();
                if let Some(pos) = entries.iter().position(|p| *p == max) {
                    entries.swap_remove(pos);
                }
                entries.push(pending);
                self.heap = entries.into_iter().map(Reverse).collect();
                Some(StoreDiagnostic::WaitQueueSpill {
                    scope: self.scope,
                    dropped: max,
                    capacity: self.capacity,
                })
            }
            _ => Some(StoreDiagnostic::WaitQueueSpill {
                scope: self.scope,
                dropped: pending,
                capacity: self.capacity,
            }),
        }
    }

    /// The most urgent (smallest `upper_bound`) suspended match.
    #[must_use]
    pub fn peek(&self) -> Option<&PendingMatch> {
        self.heap.peek().map(|Reverse(p)| p)
    }

    /// Pops every match whose window has closed: `upper_bound < wm`, i.e.
    /// resume **strictly** when `wm > upper` (`08` §3.2 resume loop; the
    /// boundary `wm == upper` does *not* pop — `08` §3.1 shows it cannot
    /// arise for suspended entries, and popping it would race the window).
    /// Matches come out smallest-`upper_bound`-first (ADR-012 resume order).
    #[must_use = "popped matches are removed from the queue and must be resumed (08 §3.2)"]
    pub fn pop_expired(&mut self, wm: EventTime) -> Vec<PendingMatch> {
        let mut expired = Vec::new();
        while let Some(Reverse(top)) = self.heap.peek() {
            if top.upper_bound < wm {
                if let Some(Reverse(p)) = self.heap.pop() {
                    expired.push(p);
                }
            } else {
                break;
            }
        }
        expired
    }

    /// Drains *all* remaining matches in `upper_bound` order — the
    /// end-of-stream flush (`08` §3.4: `wm := +∞` sentinel pops everything).
    pub fn drain_all(&mut self) -> Vec<PendingMatch> {
        let mut all: Vec<PendingMatch> = self.heap.drain().map(|Reverse(p)| p).collect();
        all.sort();
        all
    }
}
