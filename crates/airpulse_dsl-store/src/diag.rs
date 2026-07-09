//! Spill diagnostic signals per `docs/idea/adr/ADR-011-dos-limits.md`.
//!
//! The store never prints or logs — spill events are returned as values and
//! the caller (evaluator / diag crate) routes them into the diagnostics
//! stream (`11-diagnostics`). All spills are degrade + diagnostic, never a
//! panic (ADR-011 "Spill policy", `07` §9).

use airpulse_dsl_ir::PendingMatch;
use airpulse_dsl_types::{EventId, ScopeId};

/// A bounded-structure spill signal (ADR-011).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreDiagnostic {
    /// `ADGL3005 RingBufferSpill` — a RingBuffer exceeded
    /// `max_ringbuffer_events_per_scope`; the oldest (lowest-time) event was
    /// dropped (ADR-011 spill policy, `07` §4).
    RingBufferSpill {
        /// Partition whose ring spilled.
        scope: ScopeId,
        /// The evicted (oldest) event.
        dropped: EventId,
        /// The configured ring capacity.
        capacity: usize,
    },
    /// `ADGL3004 WaitQueueSpill` — a WaitQueue exceeded
    /// `max_pending_per_scope`; the pending with the **largest**
    /// `upper_bound` (least urgent) was dropped (ADR-011, `08` §3.3).
    WaitQueueSpill {
        /// Partition whose wait queue spilled.
        scope: ScopeId,
        /// The dropped (least urgent) pending match.
        dropped: PendingMatch,
        /// The configured queue capacity.
        capacity: usize,
    },
}

impl StoreDiagnostic {
    /// Stable diagnostic code (ADR-011 table; `11-diagnostics`).
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            StoreDiagnostic::RingBufferSpill { .. } => "ADGL3005",
            StoreDiagnostic::WaitQueueSpill { .. } => "ADGL3004",
        }
    }
}
