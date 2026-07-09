//! `RuleInstance` and its parts per `docs/idea/spec/06-ir-bytecode.md`
//! §2.1/§3.1 with the verified annotations of
//! `docs/idea/spec/05-verification.md` §11–12.

use airpulse_dsl_types::{
    CauseKind, DurationMs, EventType, MetricPath, ProblemKind, RuleId, ScopeType,
};

use crate::intent::Intent;
use crate::predicate::{Predicate, TopoFuncIdx};
use crate::symbol::Symbol;

/// Rule class: the bipartite evidence/decision split
/// (`06-ir-bytecode.md` §2.1 `RuleInstance.kind`; isolation enforced by the
/// verifier, `05-verification.md` §8 — evidence never emits Problems,
/// decisions never infer Causes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RuleKind {
    /// Evidence rule: event anchor, body of infer/action intents
    /// (`03-semantics.md` §3.1).
    Evidence,
    /// Decision rule: Cause- or Problem-anchor, body of emit/action intents
    /// (`03-semantics.md` §3.5).
    Decision,
}

/// What the anchor matches on.
///
/// `06-ir-bytecode.md` §2.1 spells only `event_type: EventType` for
/// `AnchorSpec`, but decision rules anchor on aggregated state — a Cause
/// (Example 01 `anchor c: Cause(PmtudBlackhole)`) or a Problem (Example 07/08
/// `anchor downstream: Problem(DeviceUnreachable)`), per `03-semantics.md`
/// §3.5 re-evaluation triggers. The IR therefore carries all three anchor
/// sources.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AnchorSource {
    /// Evidence anchor on a raw event (`03` §3.1).
    Event(EventType),
    /// Decision anchor on a Cause — fires on `ConfidenceMutation` (`03` §3.5).
    Cause(CauseKind),
    /// Decision anchor on a Problem — fires on `ProblemEmission` (Example 8).
    Problem(ProblemKind),
}

/// Anchor clause of a rule (`06-ir-bytecode.md` §2.1 `AnchorSpec`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AnchorSpec {
    /// Binding name, e.g. `"rtx"`, `"c"`, `"downstream"`.
    pub binding: Symbol,
    /// What the anchor matches on (event type / cause kind / problem kind).
    pub source: AnchorSource,
    /// Compiled anchor predicate (`06` §4), e.g.
    /// `rtx.segment_size > 1400`, `c.confidence >= 80`. Use
    /// [`Predicate::always_true`] when the anchor has no predicate block
    /// (Example 8).
    pub predicate: Predicate,
}

/// Correlate candidate source (`06-ir-bytecode.md` §2.1 `CorrelateSource`;
/// grammar `02` §4 — Example 8 correlates on a Problem).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CorrelateSource {
    /// Scan the partition RingBuffer for events of this type (`03` §3.2).
    Event(EventType),
    /// Filter already-emitted Problems in the SubGraph (Example 8).
    Problem(ProblemKind),
    /// Filter Causes in the SubGraph.
    Cause(CauseKind),
}

/// A topology-function call in a correlate clause, e.g.
/// `same_session(rtx.target, ptb.target)` (`06-ir-bytecode.md` §2.1
/// `CorrelateSpec.topo`; signature checked per `05-verification.md` §5,
/// returns `T3` — C10).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TopoCall {
    /// Catalog topology function name, e.g. `same_session`, `upstream_of`.
    pub func: Box<str>,
    /// Catalog function index used by the hot-path opcodes (`06` §6).
    pub func_idx: TopoFuncIdx,
    /// Scope-key argument expressions, e.g. `rtx.target`, `ptb.target`.
    pub args: Box<[MetricPath]>,
}

/// Verifier-proven window bound for a correlate clause
/// (`05-verification.md` §11 `WindowProof`).
///
/// `Calculable` means both window ends are linear `anchor.time ± duration`
/// forms (`05` §3), so the engine statically knows the WaitQueue
/// `upper_bound` (`06` §8 item 2). `RuntimeCheck` is the conservative
/// downgrade for non-linear windows (rare in v1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WindowProof {
    /// `[anchor.time - back, anchor.time + forward]`, inclusive both ends
    /// (D4, `05` §3.2).
    Calculable {
        /// Backward extent from the anchor time (≥ 0).
        back: DurationMs,
        /// Forward extent from the anchor time (≥ 0); > 0 forces WaitQueue
        /// suspension until `watermark > upper_bound` (`03` §3.1).
        forward: DurationMs,
    },
    /// Window must be checked at runtime (downgrade, `05` §11).
    RuntimeCheck,
}

/// One `correlate` clause (`06-ir-bytecode.md` §2.1 `CorrelateSpec`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CorrelateSpec {
    /// Binding name, e.g. `"ptb"`, `"upstream"`.
    pub binding: Symbol,
    /// Where candidates come from (Event / Problem / Cause).
    pub source: CorrelateSource,
    /// Topology filter; `Unknown` results make the binding `Unknown`, never
    /// a non-match (`03` §3.2, C10).
    pub topo: TopoCall,
    /// Verified window bound (`05` §11).
    pub window: WindowProof,
}

/// The if/else branch table of a rule body (`06-ir-bytecode.md` §3.1;
/// semantics `03-semantics.md` §3.7).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BranchTable {
    /// `T3`-valued condition: `present`/`absent` primaries plus metric
    /// predicates, Kleene with short-circuit (`03` §3.7).
    pub cond: Predicate,
    /// Intents executed when `cond == True`.
    pub then_body: Box<[Intent]>,
    /// Intents executed when `cond == False` (optional `else`).
    pub else_body: Option<Box<[Intent]>>,
    /// Intents executed when `cond == Unknown` — auto-generated by the
    /// verifier as `[EmitAction{ request_topology }]` for Unknown-risky
    /// conditions so the engine never forgets the Unknown branch (C10,
    /// `06` §3.1).
    pub unknown_body: Box<[Intent]>,
}

/// Annotations attached by the verifier (`05-verification.md` §11–12) that
/// the engine consumes without re-deriving (`06` §8 item 1).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VerifiedAnnotations {
    /// Max backward extent over all correlate windows; proven
    /// `≤ MAX_LOOKBACK - slack` (`05` §3.1).
    pub max_backward: DurationMs,
    /// Max forward extent over all correlate windows; determines the
    /// WaitQueue `upper_bound = anchor.time + max_forward` (`07-runtime.md`
    /// §5, `08` §2). Zero means backward-only ⇒ immediate execution
    /// (Example 8).
    pub max_forward: DurationMs,
    /// Proven scope-compatibility: the scope type of the intents' `target`
    /// expressions, with `rule.scope ⊑ target_scope` (`05` §4, `05` §11
    /// `ScopeCompat = Proven`). `None` when every intent targets the rule
    /// scope itself.
    pub target_scope: Option<ScopeType>,
}

impl VerifiedAnnotations {
    /// Recomputes `max_backward`/`max_forward` from `Calculable` correlate
    /// windows (the same fold as `07-runtime.md` §5
    /// `max_forward(rule.correlates)`); `RuntimeCheck` windows contribute
    /// nothing. A helper for hand-coding images in Phase 1 — the real
    /// verifier attaches these during lowering.
    #[must_use]
    pub fn from_correlates(
        correlates: &[CorrelateSpec],
        target_scope: Option<ScopeType>,
    ) -> VerifiedAnnotations {
        let mut max_backward = DurationMs::default();
        let mut max_forward = DurationMs::default();
        for c in correlates {
            if let WindowProof::Calculable { back, forward } = c.window {
                max_backward = max_backward.max(back);
                max_forward = max_forward.max(forward);
            }
        }
        VerifiedAnnotations { max_backward, max_forward, target_scope }
    }
}

/// One verified rule (`06-ir-bytecode.md` §2.1 `RuleInstance`).
///
/// Rules keep their `ruleset` declaration order inside
/// [`crate::ProgramImage::rules`] — ordered firing is part of determinism
/// (C12, `03-semantics.md` §6).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RuleInstance {
    /// Stable symbolic id (C12), e.g. `pmtud_hypothesis`.
    pub id: RuleId,
    /// Evidence or Decision (`05` §8 bipartite isolation).
    pub kind: RuleKind,
    /// Partition scope this rule runs in (`03` §5.1).
    pub scope: ScopeType,
    /// Anchor clause.
    pub anchor: AnchorSpec,
    /// Correlate clauses (≤ 8, `05` §9).
    pub correlates: Box<[CorrelateSpec]>,
    /// If/else over the correlate bindings (`06` §3.1). `None` for rules
    /// whose body is unconditional (Example 01 `pmtud_verdict`).
    pub branches: Option<BranchTable>,
    /// Unconditional ordered body intents, executed when there is no branch
    /// table (`06` §2.1 `body`; ordered — C12).
    pub body: Box<[Intent]>,
    /// Verifier-attached temporal/scope annotations (`05` §11–12).
    pub annotations: VerifiedAnnotations,
}

impl RuleInstance {
    /// The proven max forward window — the engine computes
    /// `upper = evt.time + rule.max_forward()` for WaitQueue placement
    /// (`07-runtime.md` §5 `max_forward(rule.correlates)`).
    #[must_use]
    pub const fn max_forward(&self) -> DurationMs {
        self.annotations.max_forward
    }
}
