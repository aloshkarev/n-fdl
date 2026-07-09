//! Anchor/correlate binding values per `docs/idea/spec/03-semantics.md` §3.2:
//! a binding is `Some(bound value)`, `Absent`, or `Unknown` (three-valued,
//! C10). Bound values snapshot the anchor/candidate at resolution time —
//! never a borrowed store reference (`07-runtime.md` §2).

use airpulse_dsl_store::{EdgeEndpoint, EventNode};
use airpulse_dsl_types::{
    CauseKind, Confidence, EventTime, NodeId, ProblemKind, ScopeId,
};

use crate::interner::ScopeInterner;
use crate::schema::EVENT_FIELD_TARGET;

/// Snapshot of a [`airpulse_dsl_store::CauseNode`] taken when a decision
/// rule's Cause anchor fires (`03` §3.5 `ConfidenceMutation` re-eval) — the
/// `c` binding of Example 01 `pmtud_verdict`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CauseSnapshot {
    /// Stable node id of the snapshotted cause.
    pub node: NodeId,
    /// Cause kind.
    pub kind: CauseKind,
    /// Hypothesis target.
    pub target: ScopeId,
    /// First-infer time (stable, `04` §3).
    pub time: EventTime,
    /// Confidence at snapshot time.
    pub confidence: Confidence,
}

/// Snapshot of a [`airpulse_dsl_store::ProblemNode`] for Problem anchors and
/// Problem-source correlates (Example 07/08 `downstream`/`upstream`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProblemSnapshot {
    /// Stable node id of the snapshotted problem.
    pub node: NodeId,
    /// Problem kind.
    pub kind: ProblemKind,
    /// Problem target.
    pub target: ScopeId,
    /// Emission time (`03` §3.4 `time = WM`).
    pub time: EventTime,
}

/// A bound anchor/correlate value (`03` §3.2 `Some(...)` arm).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Bound {
    /// An event from the RingBuffer (evidence anchor / event correlate).
    Event(EventNode),
    /// A cause snapshot (decision Cause anchor / cause correlate).
    Cause(CauseSnapshot),
    /// A problem snapshot (decision Problem anchor / problem correlate).
    Problem(ProblemSnapshot),
}

impl Bound {
    /// The bound value's event-time — the anchor time for correlate windows
    /// (`03` §3.2 `[anchor.time - back, anchor.time + fwd]`).
    #[must_use]
    pub fn time(&self) -> EventTime {
        match self {
            Bound::Event(e) => e.time,
            Bound::Cause(c) => c.time,
            Bound::Problem(p) => p.time,
        }
    }

    /// Resolves `<binding>.target` (`03` §5.1):
    /// - events: the interned [`EVENT_FIELD_TARGET`] when present (target
    ///   differs from partition scope), else the partition scope itself;
    /// - causes/problems: the node's `target`.
    ///
    /// `None` only when an event carries an `EVENT_FIELD_TARGET` key the
    /// engine never interned — a fixture bug, surfaced as an
    /// unresolved-target diagnostic by the caller, never a panic (`07` §9).
    #[must_use]
    pub fn target(&self, interner: &ScopeInterner) -> Option<ScopeId> {
        match self {
            Bound::Event(e) => match e.field(EVENT_FIELD_TARGET) {
                Some(key) => interner.resolve(key),
                None => Some(e.scope),
            },
            Bound::Cause(c) => Some(c.target),
            Bound::Problem(p) => Some(p.target),
        }
    }

    /// Evidence-edge endpoint for this value (`04` §4: events by
    /// [`airpulse_dsl_types::EventId`], graph nodes by [`NodeId`]).
    #[must_use]
    pub fn endpoint(&self) -> EdgeEndpoint {
        match self {
            Bound::Event(e) => EdgeEndpoint::Event(e.id),
            Bound::Cause(c) => EdgeEndpoint::Node(c.node),
            Bound::Problem(p) => EdgeEndpoint::Node(p.node),
        }
    }
}

/// One binding's resolution state (`03` §3.2 binding rules):
/// `present(x) = Bound`, `absent(x) = Absent`, `Unknown` is neither (C10).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Binding {
    /// `Some(matches[0])` — earliest True match.
    Bound(Bound),
    /// No candidate matched and none was Unknown.
    Absent,
    /// At least one candidate's topo predicate was `Unknown` and none was
    /// `True` (C10 — must not collapse to Absent).
    Unknown,
}

impl Binding {
    /// The bound value, when present.
    #[must_use]
    pub fn bound(&self) -> Option<&Bound> {
        match self {
            Binding::Bound(b) => Some(b),
            Binding::Absent | Binding::Unknown => None,
        }
    }
}
