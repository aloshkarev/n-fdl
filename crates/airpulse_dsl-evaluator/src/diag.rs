//! Engine-level diagnostics (`docs/idea/spec/11-diagnostics` codes where
//! assigned by ADR-011; degrade + diagnostic, never a panic — `07` §9).

use airpulse_dsl_store::StoreDiagnostic;
use airpulse_dsl_types::{EventId, EventTime, EventType, MetricPath, RuleId, ScopeId};

use crate::error::CorrelateError;

/// A non-fatal engine diagnostic. The engine records these and continues;
/// nothing on the data path panics (`07-runtime.md` §9).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineDiagnostic {
    /// A store spill surfaced through ingest/suspend
    /// (`ADGL3004`/`ADGL3005`, ADR-011).
    Store(StoreDiagnostic),
    /// A predicate/correlate evaluation error; the rule instance was
    /// skipped (`06` §8 item 6 — errors are values).
    ///
    /// Code: `ADGL3007` in v1. The runtime diagnostics spec names
    /// `ADGL3007` as `ArithOverflow`; the engine currently reports every
    /// predicate/correlate runtime error through this single diagnostic
    /// surface (`PredicateError`) while preserving the concrete
    /// [`CorrelateError`] payload for downstream inspection.
    PredicateError {
        /// The rule whose predicate failed.
        rule: RuleId,
        /// The error value.
        error: CorrelateError,
    },
    /// An intent's `target` expression did not resolve to a scope (unbound
    /// binding or un-interned key); the intent was skipped.
    UnresolvedTarget {
        /// The rule owning the intent.
        rule: RuleId,
        /// The unresolved target expression.
        path: MetricPath,
    },
    /// A target expression referenced an unsupported metric-path tail for the
    /// runtime bound value type (for example `<cause>.vlan`).
    UnsupportedTargetTail {
        /// The rule owning the intent.
        rule: RuleId,
        /// The original target expression.
        path: MetricPath,
        /// The binding segment in the path head.
        binding: Box<str>,
        /// The unsupported tail segment.
        tail: Box<str>,
    },
    /// `ADGL3102` — `max_causes_per_scope` reached; the new cause was
    /// rejected (ADR-011).
    CauseCapacity {
        /// The partition at capacity.
        scope: ScopeId,
    },
    /// `ADGL3103` — `max_rule_firings_per_event` reached; further firings
    /// for this ingest/resume root were stopped (ADR-011).
    RuleFiringsExceeded {
        /// The partition where the budget ran out.
        scope: ScopeId,
    },
    /// A `WindowProof::RuntimeCheck` correlate was encountered; Phase 1
    /// executes only `Calculable` windows (`06` §8 item 2), so the binding
    /// resolved `Absent`.
    RuntimeCheckWindow {
        /// The rule with the unsupported window proof.
        rule: RuleId,
    },
    /// A resumed `PendingMatch` referenced an anchor event no longer in the
    /// ring (possible only after a ring *spill*; watermark GC alone cannot
    /// cause this — MAX_LOOKBACK invariant, `08` §6).
    MissingAnchor {
        /// The suspended rule.
        rule: RuleId,
    },
    /// A resumed `PendingMatch` referenced a rule id not present in the loaded
    /// [`ProgramImage`]. This should not happen in normal single-image replay;
    /// it indicates store/image inconsistency and the pending match is skipped.
    RuleNotInImage {
        /// The missing suspended rule id.
        rule: RuleId,
    },
    /// `ADGL3002 LateEvidence` — offline late event arrived after a matching
    /// correlate had already resolved absent (`08` §4 / ADR-004). Accepted
    /// into the ring for audit fidelity; infer is not re-applied.
    LateEvidence {
        /// Partition where the late evidence arrived.
        scope: ScopeId,
        /// Rule whose absent correlate the late event would have matched.
        rule: RuleId,
        /// Late event id.
        event: EventId,
        /// Late event type (correlate source).
        event_type: EventType,
        /// Watermark at ingest.
        wm: EventTime,
    },
    /// `ADGL3003 LateEventDropped` — live late event dropped to the side-output
    /// (`08` §4 / ADR-004).
    LateEventDropped {
        /// Partition of the dropped event.
        scope: ScopeId,
        /// Dropped event id.
        event: EventId,
        /// Event time.
        time: EventTime,
        /// Watermark that the event was late relative to.
        wm: EventTime,
    },
}

impl EngineDiagnostic {
    /// Stable diagnostic code where ADR-011 / `11-error-diagnostics` assigns one.
    #[must_use]
    pub const fn code(&self) -> Option<&'static str> {
        match self {
            EngineDiagnostic::Store(s) => Some(s.code()),
            EngineDiagnostic::PredicateError { .. } => Some("ADGL3007"),
            EngineDiagnostic::UnsupportedTargetTail { .. } => Some("ADGL3008"),
            EngineDiagnostic::CauseCapacity { .. } => Some("ADGL3102"),
            EngineDiagnostic::RuleFiringsExceeded { .. } => Some("ADGL3103"),
            EngineDiagnostic::LateEvidence { .. } => Some("ADGL3002"),
            EngineDiagnostic::LateEventDropped { .. } => Some("ADGL3003"),
            EngineDiagnostic::UnresolvedTarget { .. }
            | EngineDiagnostic::RuntimeCheckWindow { .. }
            | EngineDiagnostic::MissingAnchor { .. }
            | EngineDiagnostic::RuleNotInImage { .. } => None,
        }
    }
}
