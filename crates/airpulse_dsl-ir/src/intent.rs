//! Effect intents per `docs/idea/spec/06-ir-bytecode.md` §2.3 with the
//! semantics of `docs/idea/spec/03-semantics.md` §3.3–3.6.

use airpulse_dsl_types::{
    ActionKind, CauseKind, MetricPath, ProblemKind, RuleId, SarifId, Severity, Weight,
};

use crate::symbol::Symbol;

/// Static part of the provenance-dedup key
/// (`06-ir-bytecode.md` §2.3: `ProvKey = (rule_id, cause, target_expr_hash,
/// window_id)`).
///
/// The `window_id` component (`floor(Evt.time / dedup_window)`,
/// `03-semantics.md` §3.3) is runtime data — the evaluator joins it with this
/// static prefix to form the full O(1) dedup key.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProvKey {
    /// Inferring rule.
    pub rule: RuleId,
    /// Inferred cause kind.
    pub cause: CauseKind,
    /// Hash of the compiled `target` expression (stable across reloads of the
    /// same rule text).
    pub target_expr_hash: u64,
}

/// One effect directive dispatched over the GraphStore
/// (`06-ir-bytecode.md` §2.3 `Intent`; ADGL is graph-walk IR + intent-stream,
/// `06` §1). Intents inside a body execute in order (C12, `06` §3).
///
/// `target` expressions are scope-key metric paths (e.g. `rtx.target`,
/// `dhcp.vlan` — `03-semantics.md` §5.1); scope/target compatibility is
/// proven by the verifier (`05` §4).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Intent {
    /// `infer Cause(K) { target, weight, evidence }` — commutative clamped
    /// confidence mutation + EvidenceEdge (`03` §3.3). Evidence-rule bodies
    /// only (`05` §8).
    InferCause {
        /// Cause kind `K`.
        cause: CauseKind,
        /// Target scope-key expression, e.g. `rtx.target`.
        target: MetricPath,
        /// Confidence weight; negative = `Contradicts` edge (`03` §3.3, C7).
        weight: Weight,
        /// Evidence binding refs, e.g. `[rtx, ptb]`.
        evidence: Box<[Symbol]>,
        /// Static provenance-dedup prefix (`03` §3.3).
        provenance_key: ProvKey,
        /// PII field paths collected by the verifier from the evidence
        /// schemas, for strict-redaction (`05` §10, C9).
        evidence_pii: Box<[MetricPath]>,
    },
    /// `emit Problem(P) { target?, severity, evidence, sarif_id }` —
    /// append-only ProblemNode emission (`03` §3.4). Decision-rule bodies
    /// only (`05` §8).
    EmitProblem {
        /// Problem kind `P`.
        problem: ProblemKind,
        /// Optional target; `None` means the rule scope (`03` §3.4).
        target: Option<MetricPath>,
        /// Emission severity.
        severity: Severity,
        /// Evidence binding refs, e.g. `[c]`.
        evidence: Box<[Symbol]>,
        /// Stable symbolic SARIF id (C8); catalog default already resolved
        /// by the verifier when the rule omitted it.
        sarif_id: SarifId,
        /// PII field paths for strict-redaction (`05` §10).
        pii: Box<[MetricPath]>,
    },
    /// `action <kind>(arg?) { target?, reason?, evidence? }` — declarative
    /// side-effect intent for the ActionSink (`03` §3.6, G2). Allowed in both
    /// rule layers (C6).
    EmitAction {
        /// Action kind (closed set, types crate).
        kind: ActionKind,
        /// Kind-argument: catalog observation/check kind for
        /// `request_observation`/`run_check` (`05` §1.1), e.g.
        /// `icmp.visibility`; `None` for `request_topology`/`mark_ambiguous`.
        arg: Option<Symbol>,
        /// Optional target expression, e.g. `rtx.path`.
        target: Option<MetricPath>,
        /// Optional human-readable reason string.
        reason: Option<Box<str>>,
        /// Evidence binding refs.
        evidence: Box<[Symbol]>,
    },
    /// Lowered `suppress_symptom(binding)` (`05-verification.md` §1.1:
    /// `SupersedeProblem { problem = P, target = arg.target }`) — marks the
    /// Problem superseded, append-only, never deleted (C7, ADR-007;
    /// Example 8).
    SupersedeProblem {
        /// Problem kind of the suppressed binding.
        problem: ProblemKind,
        /// Target expression of the suppressed Problem, e.g.
        /// `downstream.target`.
        target: MetricPath,
    },
    /// `mark_ambiguous` over a mutually-exclusive cause pair — creates an
    /// AmbiguityNode (`03-semantics.md` §4, C5).
    MarkAmbiguous {
        /// The competing cause pair.
        causes: (CauseKind, CauseKind),
        /// Common target the causes compete over.
        target: MetricPath,
    },
}
