//! `ActionSink` trait per `docs/idea/spec/07-runtime.md` §7 (G2) and the
//! offline audit / live stub implementations.
//!
//! Actions are declarative intents (`03-semantics.md` §3.6): the engine
//! resolves targets and forwards the intent; the *sink* decides the effect
//! by [`RunMode`].
//!
//! # RunMode contract (G2 / 07 §7 item 7)
//!
//! | Mode | Control-plane (eBPF filter, topology poll, external check) | Graph mutations |
//! |------|------------------------------------------------------------|-----------------|
//! | [`RunMode::Offline`] | **Never** executed — audit only. `request_observation` → `ADGL3001 ActionNoOpInReplay`. | Engine-owned (`suppress_symptom` → `SupersedeProblem`); sink only audits. |
//! | [`RunMode::Live`] | Host sink may drive controllers (AirPulse `AirPulseLiveActionSink`). | Same: engine mutates graph; sink audits / applies side effects. |
//!
//! This crate ships:
//! - [`OfflineAuditSink`] — PCAP replay backend (audit-only; owns no controllers).
//! - [`LiveActionSink`] — intentionally unimplemented stub that returns
//!   [`SinkOutcome::NotSupported`] for live control-plane kinds so the trait
//!   surface is ready for a host eBPF controller without embedding one here.
//!
//! Host eBPF / topology controllers are **out of scope** for n-fdl (design:
//! AirPulse P2 / ADGL M5).

use airpulse_dsl_ir::Symbol;
use airpulse_dsl_store::EdgeEndpoint;
use airpulse_dsl_types::MetricPath;
use airpulse_dsl_types::{ActionKind, CauseKind, EventTime, RuleId, ScopeId};

/// Execution mode (`08-stream-watermarking.md` §1). The spec sketches
/// `RunMode` carrying the audit-log / eBPF-controller handles (`07` §7);
/// this crate keeps the mode a plain discriminant and lets each sink own its
/// backend — [`OfflineAuditSink`] owns the audit log; a host live sink owns
/// eBPF/topology controllers. [`LiveActionSink`] is the in-crate stub that
/// refuses live control-plane work (`NotSupported`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RunMode {
    /// PCAP replay: actions are no-ops + audit (`07` §7).
    ///
    /// **Invariant:** no live control-plane side effect may run in this mode
    /// (no eBPF filter load, no topology poll, no external check enqueue).
    Offline,
    /// Live capture: host sinks may drive eBPF/topology controllers (v1.5, M5).
    ///
    /// The n-fdl stub [`LiveActionSink`] does **not** implement those hooks;
    /// it reports [`SinkOutcome::NotSupported`] until a host wires a real
    /// controller.
    Live,
}

/// How a sink handled an intent. Kept separate from [`ActionSink::emit`]'s
/// `()` return so existing host implementors stay source-compatible; sinks
/// that care expose outcomes via their own API (e.g. [`LiveActionSink::outcomes`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SinkOutcome {
    /// Intent recorded for audit; no control-plane side effect.
    Audited,
    /// Offline `request_observation` — audited as `ADGL3001 ActionNoOpInReplay`.
    NoOpInReplay,
    /// Live control-plane effect applied (host eBPF / topology / check).
    Applied,
    /// Live control-plane effect refused: this sink has no controller wired
    /// (n-fdl stub) or the host explicitly declined.
    NotSupported,
}

/// A fully-resolved runtime action (`03-semantics.md` §3.6
/// `ActionIntent{ kind, target, reason, evidence_refs }`, extended with the
/// emitting rule and the raw target path for audit fidelity).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionIntent {
    /// Action kind (closed set, `10` §5).
    pub kind: ActionKind,
    /// Rule that emitted the action (for deterministic audit ordering,
    /// ADR-012).
    pub rule: RuleId,
    /// Kind-argument, e.g. `icmp.visibility` (`05` §1.1).
    pub arg: Option<Symbol>,
    /// Resolved target scope, when the target path resolved.
    pub target: Option<ScopeId>,
    /// The raw target expression from the rule (e.g. `rtx.path`), kept for
    /// audit output — Phase 1 resolves all paths to the binding's target
    /// scope (no catalog field schema yet).
    pub target_path: Option<MetricPath>,
    /// Human-readable reason.
    pub reason: Option<Box<str>>,
    /// Evidence references.
    pub evidence: Vec<EdgeEndpoint>,
    /// The competing cause pair for `mark_ambiguous` (`03` §4).
    pub causes: Option<(CauseKind, CauseKind)>,
}

impl ActionIntent {
    /// Whether this kind is a live control-plane side effect under `07` §7 /
    /// `10` §10 (`request_observation` → eBPF, `request_topology` → poll,
    /// `run_check` → external enqueue). Graph-only kinds (`suppress_symptom`,
    /// `mark_ambiguous`) are engine-owned mutations plus audit.
    #[must_use]
    pub fn is_control_plane(&self) -> bool {
        matches!(
            self.kind,
            ActionKind::RequestObservation | ActionKind::RequestTopology | ActionKind::RunCheck
        )
    }
}

/// Consumes action intents (`07-runtime.md` §7). `Send` — the sink is
/// mutated per emission by the partition executor (`07` §10).
///
/// # Implementing a live / eBPF sink
///
/// Hosts (AirPulse) should:
/// 1. Own controllers outside this trait (filter load, topology poll, checks).
/// 2. In [`RunMode::Offline`], mirror [`OfflineAuditSink`]: audit only;
///    tag `request_observation` with `ADGL3001`.
/// 3. In [`RunMode::Live`], apply control-plane effects for
///    [`ActionIntent::is_control_plane`] kinds; never call eBPF from an
///    offline path.
///
/// See [`LiveActionSink`] for an in-crate stub that returns
/// [`SinkOutcome::NotSupported`] instead of loading controllers.
pub trait ActionSink: Send {
    /// Delivers one action. `wm` is the watermark at emission time.
    ///
    /// Offline mode must not execute live control-plane actions (audit only).
    fn emit(&mut self, intent: ActionIntent, mode: RunMode, wm: EventTime);
}

/// One audited action (`07` §7 offline table).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditEntry {
    /// Diagnostic code, when the audit is a warning:
    /// `ADGL3001 ActionNoOpInReplay` for `request_observation` in replay
    /// (`03` §3.6); `None` for plain audit records.
    pub code: Option<&'static str>,
    /// The audited intent.
    pub intent: ActionIntent,
    /// Watermark at emission.
    pub wm: EventTime,
}

/// Offline (PCAP replay) sink: records every action, executes none
/// (`07` §7 — "offline never calls eBPF", contract item 7).
///
/// Even if the engine is misconfigured with [`RunMode::Live`], this sink
/// still only audits — it owns no controllers and cannot perform live
/// control-plane work.
#[derive(Debug, Default)]
pub struct OfflineAuditSink {
    entries: Vec<AuditEntry>,
    outcomes: Vec<SinkOutcome>,
}

impl OfflineAuditSink {
    /// Empty audit log.
    #[must_use]
    pub fn new() -> OfflineAuditSink {
        OfflineAuditSink::default()
    }

    /// Audited actions in emission order.
    #[must_use]
    pub fn entries(&self) -> &[AuditEntry] {
        &self.entries
    }

    /// Per-emission outcomes in emission order (always audit-only variants).
    #[must_use]
    pub fn outcomes(&self) -> &[SinkOutcome] {
        &self.outcomes
    }

    fn record(&mut self, intent: ActionIntent, mode: RunMode, wm: EventTime) -> SinkOutcome {
        let (code, outcome) = match (mode, intent.kind) {
            // 07 §7: offline request_observation → ADGL3001 ActionNoOpInReplay.
            (RunMode::Offline, ActionKind::RequestObservation) => {
                (Some("ADGL3001"), SinkOutcome::NoOpInReplay)
            }
            _ => (None, SinkOutcome::Audited),
        };
        self.entries.push(AuditEntry { code, intent, wm });
        self.outcomes.push(outcome);
        outcome
    }
}

impl ActionSink for OfflineAuditSink {
    fn emit(&mut self, intent: ActionIntent, mode: RunMode, wm: EventTime) {
        let _ = self.record(intent, mode, wm);
    }
}

/// Stub live sink: trait surface for a future host eBPF / topology
/// controller without implementing one in this submodule.
///
/// - [`RunMode::Offline`]: same audit-only policy as [`OfflineAuditSink`]
///   (`request_observation` → `ADGL3001` / [`SinkOutcome::NoOpInReplay`]).
/// - [`RunMode::Live`]: control-plane kinds
///   ([`ActionIntent::is_control_plane`]) record
///   [`SinkOutcome::NotSupported`] and an audit entry; graph-audit kinds
///   record [`SinkOutcome::Audited`]. **Never** loads filters, polls
///   topology, or enqueues checks.
#[derive(Debug, Default)]
pub struct LiveActionSink {
    entries: Vec<AuditEntry>,
    outcomes: Vec<SinkOutcome>,
}

impl LiveActionSink {
    /// Empty stub (no controllers).
    #[must_use]
    pub fn new() -> LiveActionSink {
        LiveActionSink::default()
    }

    /// Audited actions in emission order.
    #[must_use]
    pub fn entries(&self) -> &[AuditEntry] {
        &self.entries
    }

    /// Per-emission outcomes in emission order.
    #[must_use]
    pub fn outcomes(&self) -> &[SinkOutcome] {
        &self.outcomes
    }

    fn record(&mut self, intent: ActionIntent, mode: RunMode, wm: EventTime) -> SinkOutcome {
        let (code, outcome) = match mode {
            RunMode::Offline => match intent.kind {
                ActionKind::RequestObservation => (Some("ADGL3001"), SinkOutcome::NoOpInReplay),
                _ => (None, SinkOutcome::Audited),
            },
            RunMode::Live => {
                if intent.is_control_plane() {
                    (None, SinkOutcome::NotSupported)
                } else {
                    (None, SinkOutcome::Audited)
                }
            }
        };
        self.entries.push(AuditEntry { code, intent, wm });
        self.outcomes.push(outcome);
        outcome
    }
}

impl ActionSink for LiveActionSink {
    fn emit(&mut self, intent: ActionIntent, mode: RunMode, wm: EventTime) {
        let _ = self.record(intent, mode, wm);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wm(ms: i64) -> EventTime {
        EventTime::from_millis(ms)
    }

    fn intent(kind: ActionKind) -> ActionIntent {
        ActionIntent {
            kind,
            rule: RuleId::new("test.rule"),
            arg: Some(Symbol::new("icmp.visibility")),
            target: Some(ScopeId::vlan(1)),
            target_path: Some(MetricPath::new("rtx.path")),
            reason: Some(Box::from("unit")),
            evidence: Vec::new(),
            causes: None,
        }
    }

    #[test]
    fn offline_request_observation_audits_adgl3001_noop_in_replay() {
        let mut sink = OfflineAuditSink::new();
        sink.emit(
            intent(ActionKind::RequestObservation),
            RunMode::Offline,
            wm(1),
        );

        assert_eq!(sink.entries().len(), 1);
        assert_eq!(sink.entries()[0].code, Some("ADGL3001"));
        assert_eq!(
            sink.entries()[0].intent.kind,
            ActionKind::RequestObservation
        );
        assert_eq!(sink.outcomes(), &[SinkOutcome::NoOpInReplay]);
    }

    #[test]
    fn offline_non_observation_actions_audit_without_adgl3001() {
        let kinds = [
            ActionKind::RunCheck,
            ActionKind::SuppressSymptom,
            ActionKind::MarkAmbiguous,
            ActionKind::RequestTopology,
        ];
        let mut sink = OfflineAuditSink::new();
        for (i, kind) in kinds.into_iter().enumerate() {
            sink.emit(intent(kind), RunMode::Offline, wm(i as i64));
        }

        assert_eq!(sink.entries().len(), kinds.len());
        assert!(sink.entries().iter().all(|e| e.code.is_none()));
        assert!(sink.outcomes().iter().all(|o| *o == SinkOutcome::Audited));
    }

    #[test]
    fn offline_sink_never_applies_even_when_engine_mode_is_live() {
        // Misconfigured Live on an OfflineAuditSink: still audit-only.
        let mut sink = OfflineAuditSink::new();
        for kind in [
            ActionKind::RequestObservation,
            ActionKind::RequestTopology,
            ActionKind::RunCheck,
            ActionKind::MarkAmbiguous,
            ActionKind::SuppressSymptom,
        ] {
            sink.emit(intent(kind), RunMode::Live, wm(0));
        }

        assert_eq!(sink.entries().len(), 5);
        assert!(sink.entries().iter().all(|e| e.code.is_none()));
        assert!(
            sink.outcomes().iter().all(|o| *o == SinkOutcome::Audited),
            "OfflineAuditSink must never report Applied/NotSupported — it has no controllers"
        );
        assert!(!sink.outcomes().contains(&SinkOutcome::Applied));
    }

    #[test]
    fn live_stub_offline_request_observation_is_adgl3001() {
        let mut sink = LiveActionSink::new();
        sink.emit(
            intent(ActionKind::RequestObservation),
            RunMode::Offline,
            wm(2),
        );

        assert_eq!(sink.entries()[0].code, Some("ADGL3001"));
        assert_eq!(sink.outcomes(), &[SinkOutcome::NoOpInReplay]);
    }

    #[test]
    fn live_stub_control_plane_kinds_return_not_supported() {
        let mut sink = LiveActionSink::new();
        for kind in [
            ActionKind::RequestObservation,
            ActionKind::RequestTopology,
            ActionKind::RunCheck,
        ] {
            sink.emit(intent(kind), RunMode::Live, wm(3));
        }

        assert_eq!(sink.entries().len(), 3);
        assert!(sink.entries().iter().all(|e| e.code.is_none()));
        assert!(
            sink.outcomes()
                .iter()
                .all(|o| *o == SinkOutcome::NotSupported)
        );
        assert!(
            !sink.outcomes().contains(&SinkOutcome::Applied),
            "stub must not pretend to apply eBPF/topology/check"
        );
    }

    #[test]
    fn live_stub_graph_audit_kinds_are_audited_not_applied() {
        let mut sink = LiveActionSink::new();
        sink.emit(intent(ActionKind::MarkAmbiguous), RunMode::Live, wm(4));
        sink.emit(intent(ActionKind::SuppressSymptom), RunMode::Live, wm(5));

        assert_eq!(
            sink.outcomes(),
            &[SinkOutcome::Audited, SinkOutcome::Audited]
        );
    }

    #[test]
    fn control_plane_classification_matches_spec() {
        assert!(intent(ActionKind::RequestObservation).is_control_plane());
        assert!(intent(ActionKind::RequestTopology).is_control_plane());
        assert!(intent(ActionKind::RunCheck).is_control_plane());
        assert!(!intent(ActionKind::MarkAmbiguous).is_control_plane());
        assert!(!intent(ActionKind::SuppressSymptom).is_control_plane());
    }
}
