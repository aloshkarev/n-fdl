//! Deterministic result extraction (`docs/idea/adr/ADR-012-determinism.md`):
//! final graph state read out in a stable order for golden assertions and
//! the later SARIF emitter (T-08).

use std::collections::BTreeMap;

use airpulse_dsl_types::{
    CauseKind, Confidence, EventTime, ProblemKind, SarifId, ScopeId, Severity,
};

use crate::sink::AuditEntry;

/// One cause with its final accumulated confidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CauseView {
    /// Partition the cause lives in.
    pub scope: ScopeId,
    /// Cause kind.
    pub kind: CauseKind,
    /// Hypothesis target.
    pub target: ScopeId,
    /// First-infer time (stable, `04` §3).
    pub time: EventTime,
    /// Final confidence.
    pub confidence: Confidence,
}

/// One emitted problem with its suppression flag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProblemView {
    /// Partition the problem lives in.
    pub scope: ScopeId,
    /// Problem kind.
    pub kind: ProblemKind,
    /// Problem target.
    pub target: ScopeId,
    /// Emission watermark (`03` §3.4).
    pub time: EventTime,
    /// Emission severity.
    pub severity: Severity,
    /// Stable SARIF id (C8).
    pub sarif_id: SarifId,
    /// Cause kinds referenced by this problem's evidence list, in stable order.
    pub cause_kinds: Vec<CauseKind>,
    /// Event field values linked via cause evidence edges (field name → value).
    pub evidence_fields: BTreeMap<String, String>,
    /// `suppress_symptom` flag (C7 — append-only retraction).
    pub superseded: bool,
}

/// Deterministically-ordered extraction of the final engine state
/// (ADR-012: merge order `(event_time, rule_decl_order, scope_id)`; here
/// causes sort by `(scope, kind, target)`, problems keep per-scope emission
/// order under a scope-sorted merge, and audit entries sort by
/// `(wm, rule, kind)` so scope-interleaving permutations of the input do not
/// change the snapshot).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Snapshot {
    /// All causes, sorted by `(scope, kind, target)`.
    pub causes: Vec<CauseView>,
    /// All problems, scopes in sorted order, per-scope emission order.
    pub problems: Vec<ProblemView>,
    /// Audited actions, sorted by `(wm, rule, kind, target)`.
    pub audit: Vec<AuditEntry>,
}
