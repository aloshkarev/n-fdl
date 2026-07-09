//! Runtime DoS limits per `docs/idea/adr/ADR-011-dos-limits.md`.

use airpulse_dsl_types::DurationMs;

/// Non-panicking duration constructor for the spec-default literals below
/// (all non-negative, so the fallback branch is dead).
fn dur(ms: i64) -> DurationMs {
    DurationMs::from_millis(ms).unwrap_or_default()
}

/// Configurable runtime DoS limits (ADR-011 "Runtime" table).
///
/// Lexer/parser and AOT limits (max token length, `MAX_LOOKBACK` vs window
/// proofs, …) are owned by the syntax/verify crates; this struct carries only
/// the bounds the *store* and evaluator enforce at runtime. All spills are
/// degrade + diagnostic, never a panic (ADR-011 "Spill policy", `07` §9).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Limits {
    /// `max_ringbuffer_events_per_scope` — RingBuffer capacity; overflow
    /// drops the oldest event + `ADGL3005 RingBufferSpill` (ADR-011).
    pub max_ringbuffer_events_per_scope: usize,
    /// `max_pending_per_scope` — WaitQueue bound; overflow drops the pending
    /// with the **largest** `upper_bound` (least urgent) +
    /// `ADGL3004 WaitQueueSpill` (ADR-011, `08` §3.3).
    pub max_pending_per_scope: usize,
    /// `max_causes_per_scope` — SubGraph cause bound; overflow rejects the
    /// new cause + audit `ADGL3102` (ADR-011). Enforced by the evaluator's
    /// infer path.
    pub max_causes_per_scope: usize,
    /// `max_rule_firings_per_event` — evaluator bound, `ADGL3103` (ADR-011).
    pub max_rule_firings_per_event: usize,
    /// `max_topology_hops` — `upstream_of` traversal bound, `ADGL3006`
    /// (ADR-011); consumed by TopologyProvider implementations.
    pub max_topology_hops: usize,
    /// `MAX_LOOKBACK` — GC horizon: events with
    /// `time < watermark - max_lookback` are evicted (`07` §4). The verifier
    /// proves `MAX_LOOKBACK > max(max_backward, max_forward) + slack`
    /// (`05` §3.1, `08` §6), which is what keeps GC safe for pending matches.
    pub max_lookback: DurationMs,
    /// `slack` in the `MAX_LOOKBACK` invariant (`08` §6; default 0 — the
    /// verifier guarantees strict inequality).
    pub slack: DurationMs,
    /// `dedup_window` — provenance dedup period for
    /// `window_id = floor(Evt.time / dedup_window)` (`03` §3.3, ADR-011;
    /// invariant `dedup_window ≥ 1ms`).
    pub dedup_window: DurationMs,
}

/// Spec defaults from the ADR-011 "Runtime" table.
impl Default for Limits {
    fn default() -> Limits {
        Limits {
            max_ringbuffer_events_per_scope: 4096,
            max_pending_per_scope: 1024,
            max_causes_per_scope: 256,
            max_rule_firings_per_event: 64,
            max_topology_hops: 16,
            max_lookback: dur(60_000),
            slack: dur(0),
            dedup_window: dur(1_000),
        }
    }
}
