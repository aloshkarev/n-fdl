//! Per-partition diagnostic subgraph per `docs/idea/spec/07-runtime.md` §3,
//! node/edge shapes per `docs/idea/spec/04-type-system.md` §3–§4, dedup
//! semantics per `docs/idea/spec/03-semantics.md` §3.3–3.4 and §4.

use std::collections::{HashMap, HashSet};

use airpulse_dsl_ir::ProvKey;
use airpulse_dsl_types::{
    CauseKind, Confidence, DurationMs, EventId, EventTime, NodeId, ProblemKind, RuleId, SarifId,
    ScopeId, Severity,
};

/// Full runtime provenance-dedup key (`03` §3.3
/// `prov = (R.id, K, T, window_id)`): the static [`ProvKey`] prefix from the
/// IR joined with the runtime `window_id = floor(Evt.time / dedup_window)`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RuntimeProvKey {
    /// Static prefix `(rule_id, cause, target_expr_hash)` (ir crate).
    pub key: ProvKey,
    /// Dedup window index, `floor(Evt.time / dedup_window)` (`03` §3.3).
    pub window_id: i64,
}

/// Computes `window_id = floor(Evt.time / dedup_window)` (`03` §3.3).
///
/// ADR-011 requires `dedup_window ≥ 1ms`; a zero duration is clamped to 1ms
/// here so the division is total (no data-path panic, `07` §9). Uses
/// euclidean division so negative event-times still floor.
#[must_use]
pub fn window_id(time: EventTime, dedup_window: DurationMs) -> i64 {
    let w = dedup_window.millis().max(1);
    time.millis().div_euclid(w)
}

/// A hypothesis node (`04` §3 `CauseNode`): stateful — confidence mutates
/// commutatively (`03` §3.3), `time` is the first-infer event time and stays
/// stable across later infers (`04` §3 comment).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CauseNode {
    /// Stable engine-assigned node id (`07` §2).
    pub id: NodeId,
    /// Catalog cause kind.
    pub kind: CauseKind,
    /// Entity the hypothesis is about (`03` §5.1 target vs scope).
    pub target: ScopeId,
    /// First-infer event time, stable (`04` §3).
    pub time: EventTime,
    /// Accumulated confidence, `clamp(0,100)` commutative (`03` §3.3).
    pub confidence: Confidence,
    /// Evidence node references (`04` §3 `evidence: List<NodeId>`).
    pub evidence: Vec<NodeId>,
    /// Provenance keys that contributed to this cause — the per-node view of
    /// the dedup set, for SARIF explanation (`03` §3.3).
    pub provenance: Vec<RuntimeProvKey>,
}

/// An emitted problem (`04` §3 `ProblemNode`): append-only — never removed,
/// retraction is `superseded = true` (C7, ADR-007; `03` §3.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProblemNode {
    /// Stable engine-assigned node id.
    pub id: NodeId,
    /// Catalog problem kind.
    pub kind: ProblemKind,
    /// Entity the problem is about.
    pub target: ScopeId,
    /// Emission watermark (`03` §3.4 `time = WM`).
    pub time: EventTime,
    /// Emission severity.
    pub severity: Severity,
    /// Evidence node references.
    pub evidence: Vec<NodeId>,
    /// Stable symbolic SARIF id (C8).
    pub sarif_id: SarifId,
    /// Suppression flag — set instead of deleting (C7).
    pub superseded: bool,
}

/// Ambiguity lifecycle state (`04` §3 `state: Active | Resolved`;
/// lifecycle in `03` §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AmbiguityState {
    /// Both causes are Probable and too close to call (`03` §4).
    Active,
    /// One cause won (≥ 80) or they separated (Δ ≥ 15); the SARIF record is
    /// kept (`03` §4 "superseded; SARIF entry preserved").
    Resolved,
}

/// A competing-hypotheses node (`04` §3 `AmbiguityNode`; created by the
/// `mark_ambiguous` synthesis over `mutually_exclusive` pairs, `03` §4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AmbiguityNode {
    /// Stable engine-assigned node id.
    pub id: NodeId,
    /// The competing mutually-exclusive cause pair.
    pub causes: (CauseKind, CauseKind),
    /// Common target the causes compete over.
    pub target: ScopeId,
    /// Active / Resolved lifecycle state.
    pub state: AmbiguityState,
}

/// Evidence-edge kind — the `infer`-creatable subset of `04` §4 `EdgeKind`
/// (`EvidenceEdge ∈ {Supports, Contradicts, Explains}`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EvidenceEdgeKind {
    /// `weight > 0` support (`04` §4).
    Supports,
    /// `weight < 0` contradiction (C7).
    Contradicts,
    /// Neutral correlation.
    Explains,
}

/// Edge source: an event in the ring (referenced by [`EventId`] — rings
/// evict, so no owned reference) or another graph node (`04` §4
/// "CauseNode <- EventNode/CauseNode").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EdgeEndpoint {
    /// A ring event.
    Event(EventId),
    /// A subgraph node (Cause/Problem/Ambiguity).
    Node(NodeId),
}

/// A directed evidence edge created by `infer`
/// (`03` §3.3 `Sub.Edges.add(EvidenceEdge{ kind, src=Evt, dst=Cause })`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceEdge {
    /// Supports / Contradicts / Explains.
    pub kind: EvidenceEdgeKind,
    /// Evidence source.
    pub src: EdgeEndpoint,
    /// Destination node (typically the inferred Cause).
    pub dst: NodeId,
}

/// One partition's diagnostic graph (`07` §3 `SubGraph`).
///
/// The store owns containers and dedup invariants; *when* to insert (rule
/// evaluation, confidence math, ambiguity synthesis) is the evaluator's job
/// (`07` §1 crate split).
#[derive(Debug, Clone, Default)]
pub struct SubGraph {
    /// Hypotheses, keyed by `(kind, target)` (`07` §3).
    pub causes: HashMap<(CauseKind, ScopeId), CauseNode>,
    /// Emitted problems — append-only (C7): only [`SubGraph::push_problem`]
    /// and the `superseded` flag ever change this.
    problems: Vec<ProblemNode>,
    /// Competing-hypothesis nodes, keyed by `(pair, target)` (`07` §3).
    pub ambiguities: HashMap<((CauseKind, CauseKind), ScopeId), AmbiguityNode>,
    /// Evidence edges (`04` §4).
    pub edges: Vec<EvidenceEdge>,
    /// Provenance dedup set (`03` §3.3; C12 "once per window").
    ///
    /// Prunable: this is only the *dedup* index — the append-only provenance
    /// record (C7) lives on each [`CauseNode::provenance`] list, so dropping
    /// unreachable window keys here loses no explanation data. See
    /// [`SubGraph::prune_provenance`] for the safety argument.
    seen_provenance: HashSet<RuntimeProvKey>,
    /// Problem-emission cooldown set
    /// (`07` §3 `HashSet<(RuleId, ProblemKind, ScopeId, i64)>`; `03` §3.4 F3).
    ///
    /// Bounded: [`SubGraph::try_mark_emitted`] replaces the expired entry
    /// for its own key, and [`SubGraph::prune_emitted`] sweeps stale keys.
    emitted_problems: HashSet<(RuleId, ProblemKind, ScopeId, i64)>,
}

impl SubGraph {
    /// Empty partition graph.
    #[must_use]
    pub fn new() -> SubGraph {
        SubGraph::default()
    }

    /// Registers an infer provenance key (`03` §3.3): returns `true` if the
    /// key was new (the infer proceeds), `false` if it was already seen this
    /// dedup window (the infer is a no-op — "one (rule, cause, target) pair
    /// applies at most once per window").
    pub fn try_insert_provenance(&mut self, key: RuntimeProvKey) -> bool {
        self.seen_provenance.insert(key)
    }

    /// Whether an infer provenance key has been seen (`03` §3.3).
    #[must_use]
    pub fn has_provenance(&self, key: &RuntimeProvKey) -> bool {
        self.seen_provenance.contains(key)
    }

    /// Problem-emission cooldown check + record (`03` §3.4 F3): if
    /// `(rule, problem, target)` was already emitted and the cooldown has
    /// not expired (`wm - previous_wm <= cooldown`), returns `false` (the
    /// emit is a no-op). Otherwise records `(rule, problem, target, wm)` and
    /// returns `true`.
    ///
    /// Bounded growth: expired entries for the *same* key are removed on
    /// re-emission (they can never suppress again), so each
    /// `(rule, problem, target)` holds at most one live tuple. Keys that
    /// stop emitting are swept by [`SubGraph::prune_emitted`].
    pub fn try_mark_emitted(
        &mut self,
        rule: &RuleId,
        problem: &ProblemKind,
        target: ScopeId,
        wm: EventTime,
        cooldown: DurationMs,
    ) -> bool {
        let within_cooldown = self.emitted_problems.iter().any(|(r, p, t, prev_wm)| {
            r == rule
                && p == problem
                && *t == target
                && wm.millis().saturating_sub(*prev_wm) <= cooldown.millis()
        });
        if within_cooldown {
            return false;
        }
        // Expired same-key entries are dead — same rule ⇒ same cooldown, and
        // wm is monotone, so they can never veto a future emit. Drop them
        // before recording the new timestamp.
        self.emitted_problems
            .retain(|(r, p, t, _)| !(r == rule && p == problem && *t == target));
        self.emitted_problems.insert((rule.clone(), problem.clone(), target, wm.millis()))
    }

    /// Sweeps cooldown entries whose timestamp is older than
    /// `wm - max_cooldown` — they can never suppress an emit again, because
    /// the watermark is monotone (`08` §8.1) and every rule's cooldown is
    /// `≤ max_cooldown`. Returns the number of pruned entries.
    ///
    /// `max_cooldown` must be the maximum cooldown across the loaded
    /// ruleset; the caller (evaluator/GC driver) owns that aggregate —
    /// per-rule cooldowns are not visible to the store.
    pub fn prune_emitted(&mut self, wm: EventTime, max_cooldown: DurationMs) -> usize {
        let before = self.emitted_problems.len();
        let horizon = wm.sub(max_cooldown).millis();
        self.emitted_problems.retain(|(_, _, _, emitted_wm)| *emitted_wm >= horizon);
        before - self.emitted_problems.len()
    }

    /// Sweeps provenance dedup keys whose `window_id` can never be produced
    /// again. Returns the number of pruned keys.
    ///
    /// Safety of pruning (`03` §3.3 + `07` §4): an infer's `window_id` is
    /// `floor(Evt.time / dedup_window)`, and every infer is driven by an
    /// event that must still be in a RingBuffer — GC evicts events with
    /// `time < wm - max_lookback`, and the MAX_LOOKBACK invariant (`08` §6)
    /// keeps pending anchors above that horizon too. So no future infer can
    /// carry `Evt.time < wm - max_lookback`, and any key with
    /// `window_id < window_id(wm - max_lookback)` is unreachable — dropping
    /// it cannot re-open a dedup window, even for late events (`08` §4
    /// offline-late evidence is audited, never retroactively re-inferred).
    ///
    /// This does *not* violate C7 append-only provenance: the durable
    /// explanation record is each [`CauseNode::provenance`] list; this set
    /// is only the O(1) dedup index.
    pub fn prune_provenance(
        &mut self,
        wm: EventTime,
        max_lookback: DurationMs,
        dedup_window: DurationMs,
    ) -> usize {
        let before = self.seen_provenance.len();
        let horizon = window_id(wm.sub(max_lookback), dedup_window);
        self.seen_provenance.retain(|k| k.window_id >= horizon);
        before - self.seen_provenance.len()
    }

    /// Appends a problem (`03` §3.4 — Problems are append-only, C7).
    pub fn push_problem(&mut self, problem: ProblemNode) {
        self.problems.push(problem);
    }

    /// Emitted problems in emission order (append-only view).
    #[must_use]
    pub fn problems(&self) -> &[ProblemNode] {
        &self.problems
    }

    /// Marks every problem of `(kind, target)` superseded — the
    /// `suppress_symptom` graph mutation (`03` §3.4/C7: retraction is a flag,
    /// never a removal). Returns how many nodes were newly marked.
    pub fn supersede_problems(&mut self, kind: &ProblemKind, target: ScopeId) -> usize {
        let mut marked = 0;
        for p in &mut self.problems {
            if !p.superseded && p.kind == *kind && p.target == target {
                p.superseded = true;
                marked += 1;
            }
        }
        marked
    }
}
