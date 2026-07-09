//! `ActionSink` trait per `docs/idea/spec/07-runtime.md` §7 (G2) and the
//! offline audit implementation.
//!
//! Actions are declarative intents (`03-semantics.md` §3.6): the engine
//! resolves targets and forwards the intent; the *sink* decides the effect
//! by [`RunMode`]. Offline replay never touches eBPF — `request_observation`
//! becomes an `ADGL3001 ActionNoOpInReplay` audit entry (`07` §7).
//! `suppress_symptom`'s graph mutation (mark superseded) is executed by the
//! engine via the `SupersedeProblem` intent; the sink only audits it.

use airpulse_dsl_store::EdgeEndpoint;
use airpulse_dsl_types::{ActionKind, CauseKind, EventTime, RuleId, ScopeId};
use airpulse_dsl_ir::Symbol;
use airpulse_dsl_types::MetricPath;

/// Execution mode (`08-stream-watermarking.md` §1). The spec sketches
/// `RunMode` carrying the audit-log / eBPF-controller handles (`07` §7);
/// Phase 1 keeps the mode a plain discriminant and lets each sink own its
/// backend — `OfflineAuditSink` owns the audit log, a future live sink will
/// own the eBPF/topology controllers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RunMode {
    /// PCAP replay: actions are no-ops + audit (`07` §7).
    Offline,
    /// Live capture: actions drive eBPF/topology controllers (v1.5, M5).
    Live,
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

/// Consumes action intents (`07-runtime.md` §7). `Send` — the sink is
/// mutated per emission by the partition executor (`07` §10).
pub trait ActionSink: Send {
    /// Delivers one action. `wm` is the watermark at emission time.
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
#[derive(Debug, Default)]
pub struct OfflineAuditSink {
    entries: Vec<AuditEntry>,
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
}

impl ActionSink for OfflineAuditSink {
    fn emit(&mut self, intent: ActionIntent, mode: RunMode, wm: EventTime) {
        // This sink is the offline backend; in (misconfigured) Live mode it
        // still only audits — it owns no controllers.
        let code = match (mode, intent.kind) {
            // 07 §7: offline request_observation → ADGL3001 ActionNoOpInReplay.
            (RunMode::Offline, ActionKind::RequestObservation) => Some("ADGL3001"),
            _ => None,
        };
        self.entries.push(AuditEntry { code, intent, wm });
    }
}
