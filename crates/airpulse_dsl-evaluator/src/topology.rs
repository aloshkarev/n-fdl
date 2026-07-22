//! `TopologyProvider` trait per `docs/idea/spec/07-runtime.md` §6 (C10) and
//! a deterministic test implementation (`StaticTopology`).
//!
//! All six functions return [`T3`]; `Unknown ≠ false` (ADR-010).
//! `upstream_of` is cycle-bounded: traversal uses a visited set and a
//! `max_hops` bound baked into the implementation (ADR-011, `10` §9); a
//! topology cycle resolves to `False` + diagnostic, never a panic
//! (`07` §6, G05 cycle isolation).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;

use airpulse_dsl_ir::TopoFuncIdx;
use airpulse_dsl_types::{ScopeId, T3};

/// The six catalog topology functions (`07-runtime.md` §6) — a closed set in
/// v1, addressed by `func_idx` in the hot path (`06` §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TopoFunc {
    /// `same_session(a, b)`.
    SameSession,
    /// `same_client(a, b)`.
    SameClient,
    /// `same_port(a, b)`.
    SamePort,
    /// `same_ap(a, b)`.
    SameAp,
    /// `same_vlan(a, b)`.
    SameVlan,
    /// `upstream_of(up, down)` — cycle-bounded (`07` §6).
    UpstreamOf,
}

impl TopoFunc {
    /// The canonical `func_idx` assignment (declaration order of `07` §6:
    /// `same_session = 0 … upstream_of = 5`). `None` for indices outside the
    /// closed set — surfaced as `CorrelateError::UnknownTopoFunction`, never
    /// a panic (`07` §9).
    #[must_use]
    pub const fn from_idx(idx: TopoFuncIdx) -> Option<TopoFunc> {
        match idx.0 {
            0 => Some(TopoFunc::SameSession),
            1 => Some(TopoFunc::SameClient),
            2 => Some(TopoFunc::SamePort),
            3 => Some(TopoFunc::SameAp),
            4 => Some(TopoFunc::SameVlan),
            5 => Some(TopoFunc::UpstreamOf),
            _ => None,
        }
    }

    /// Dispatches this function on a provider.
    #[must_use]
    pub fn call(self, topo: &dyn TopologyProvider, a: ScopeId, b: ScopeId) -> T3 {
        match self {
            TopoFunc::SameSession => topo.same_session(a, b),
            TopoFunc::SameClient => topo.same_client(a, b),
            TopoFunc::SamePort => topo.same_port(a, b),
            TopoFunc::SameAp => topo.same_ap(a, b),
            TopoFunc::SameVlan => topo.same_vlan(a, b),
            TopoFunc::UpstreamOf => topo.upstream_of(a, b),
        }
    }
}

/// Topology oracle (`07-runtime.md` §6). `Send + Sync` for cross-partition
/// lock-free execution (`07` §10); every answer is [`T3`] — `Unknown` is
/// handled by rule semantics (`03` §3.2/§3.7), never treated as `false`.
pub trait TopologyProvider: Send + Sync {
    /// Whether `a` and `b` belong to the same TCP session.
    fn same_session(&self, a: ScopeId, b: ScopeId) -> T3;
    /// Whether `a` and `b` belong to the same client.
    fn same_client(&self, a: ScopeId, b: ScopeId) -> T3;
    /// Whether `a` and `b` are on the same switch port.
    fn same_port(&self, a: ScopeId, b: ScopeId) -> T3;
    /// Whether `a` and `b` are on the same access point.
    fn same_ap(&self, a: ScopeId, b: ScopeId) -> T3;
    /// Whether `a` and `b` are on the same VLAN.
    fn same_vlan(&self, a: ScopeId, b: ScopeId) -> T3;
    /// Whether `up` is (transitively) upstream of `down`. Cycle-bounded:
    /// visited set + `max_hops`; a topology cycle yields `False` +
    /// diagnostic (`07` §6, ADR-011 `ADGL3006`).
    fn upstream_of(&self, up: ScopeId, down: ScopeId) -> T3;
}

/// A cycle / hop-bound signal from [`StaticTopology::upstream_of`]
/// (ADR-011 `ADGL3006`; `12-testing.md` §3.4 topology-cycle isolation).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TopologyDiagnostic {
    /// `ADGL3006` — a cycle was encountered while traversing upstream from
    /// `from`; the query resolved to `False` (cycle isolation, `07` §6).
    UpstreamCycle {
        /// The `down` argument of the failing query.
        from: ScopeId,
    },
    /// `ADGL3006` — traversal exceeded `max_hops` (ADR-011).
    HopBoundExceeded {
        /// The `down` argument of the failing query.
        from: ScopeId,
    },
}

impl TopologyDiagnostic {
    /// Stable diagnostic code (ADR-011 table).
    #[must_use]
    pub const fn code(&self) -> &'static str {
        "ADGL3006"
    }
}

const RELATION_COUNT: usize = 5; // same_session..same_vlan (upstream_of is directed)

/// Deterministic in-memory [`TopologyProvider`] for tests and offline
/// harnesses.
///
/// Semantics per relation query `same_*(a, b)`:
/// - `a == b` → `True` (trivially co-located);
/// - the pair was declared via the builder → `True`;
/// - both scopes are known to the topology but not related → `False`;
/// - either scope is unknown → `Unknown` (topology absent — ADR-010).
///
/// `upstream_of(up, down)` does a bounded DFS from `down` along declared
/// upstream edges. The traversal fully explores the reachable region so a
/// cycle anywhere in it is detected (on-path back-edge); a cycle or a hop
/// bound overrun records a [`TopologyDiagnostic`] (readable via
/// [`StaticTopology::diagnostics`]) and the query resolves to `False`.
///
/// Diagnostics are recorded behind a `Mutex` because the trait takes `&self`
/// (`Send + Sync`, `07` §10); contention is nil in the single-threaded v1
/// engine.
#[derive(Debug, Default)]
pub struct StaticTopology {
    known: BTreeSet<ScopeId>,
    /// Normalized (min, max) symmetric pairs, one set per relation.
    relations: [BTreeSet<(ScopeId, ScopeId)>; RELATION_COUNT],
    /// `down → direct upstream neighbors`.
    upstream: BTreeMap<ScopeId, BTreeSet<ScopeId>>,
    max_hops: usize,
    diagnostics: Mutex<Vec<TopologyDiagnostic>>,
}

fn norm(a: ScopeId, b: ScopeId) -> (ScopeId, ScopeId) {
    if a <= b { (a, b) } else { (b, a) }
}

impl StaticTopology {
    /// Empty topology (everything `Unknown`) with the given `max_hops`
    /// bound (ADR-011 `max_topology_hops`).
    #[must_use]
    pub fn new(max_hops: usize) -> StaticTopology {
        StaticTopology {
            max_hops: max_hops.max(1),
            ..StaticTopology::default()
        }
    }

    /// Declares a scope as known to the topology without any relation —
    /// queries against it answer `False` instead of `Unknown`.
    pub fn declare(&mut self, scope: ScopeId) -> &mut Self {
        self.known.insert(scope);
        self
    }

    fn relate(&mut self, rel: usize, a: ScopeId, b: ScopeId) -> &mut Self {
        self.known.insert(a);
        self.known.insert(b);
        self.relations[rel].insert(norm(a, b));
        self
    }

    /// Declares `same_session(a, b) = True` (symmetric).
    pub fn relate_session(&mut self, a: ScopeId, b: ScopeId) -> &mut Self {
        self.relate(0, a, b)
    }

    /// Declares `same_client(a, b) = True` (symmetric).
    pub fn relate_client(&mut self, a: ScopeId, b: ScopeId) -> &mut Self {
        self.relate(1, a, b)
    }

    /// Declares `same_port(a, b) = True` (symmetric).
    pub fn relate_port(&mut self, a: ScopeId, b: ScopeId) -> &mut Self {
        self.relate(2, a, b)
    }

    /// Declares `same_ap(a, b) = True` (symmetric).
    pub fn relate_ap(&mut self, a: ScopeId, b: ScopeId) -> &mut Self {
        self.relate(3, a, b)
    }

    /// Declares `same_vlan(a, b) = True` (symmetric).
    pub fn relate_vlan(&mut self, a: ScopeId, b: ScopeId) -> &mut Self {
        self.relate(4, a, b)
    }

    /// Declares a directed edge: `up` is *directly* upstream of `down`.
    pub fn upstream_edge(&mut self, up: ScopeId, down: ScopeId) -> &mut Self {
        self.known.insert(up);
        self.known.insert(down);
        self.upstream.entry(down).or_default().insert(up);
        self
    }

    /// Recorded cycle / hop-bound diagnostics, in query order.
    #[must_use]
    pub fn diagnostics(&self) -> Vec<TopologyDiagnostic> {
        self.diagnostics
            .lock()
            .map(|d| d.clone())
            .unwrap_or_default()
    }

    fn record(&self, d: TopologyDiagnostic) {
        if let Ok(mut v) = self.diagnostics.lock() {
            v.push(d);
        }
    }

    fn query(&self, rel: usize, a: ScopeId, b: ScopeId) -> T3 {
        if a == b {
            return T3::True;
        }
        if self.relations[rel].contains(&norm(a, b)) {
            return T3::True;
        }
        if self.known.contains(&a) && self.known.contains(&b) {
            T3::False
        } else {
            T3::Unknown
        }
    }

    /// Full DFS over the upstream region reachable from `start`, looking
    /// for `target`. Detects on-path back-edges (real cycles, not diamonds)
    /// and enforces the hop bound per path depth. Recursion depth is bounded
    /// by `max_hops` (`new` clamps it to ≥ 1; ADR-011 keeps it small).
    fn dfs(
        &self,
        node: ScopeId,
        target: ScopeId,
        hops_left: usize,
        on_path: &mut BTreeSet<ScopeId>,
        done: &mut BTreeSet<ScopeId>,
        outcome: &mut DfsOutcome,
    ) {
        on_path.insert(node);
        if let Some(nexts) = self.upstream.get(&node) {
            for &next in nexts {
                if next == target {
                    outcome.found = true;
                }
                if on_path.contains(&next) {
                    outcome.cycle = true;
                    continue;
                }
                if done.contains(&next) {
                    continue;
                }
                if hops_left == 0 {
                    outcome.hop_overrun = true;
                    continue;
                }
                self.dfs(next, target, hops_left - 1, on_path, done, outcome);
            }
        }
        on_path.remove(&node);
        done.insert(node);
    }
}

/// Accumulated DFS flags for `StaticTopology::dfs` (cycle detection, hop
/// bound overrun, and target reachability).
#[derive(Debug, Default, Clone, Copy)]
struct DfsOutcome {
    cycle: bool,
    hop_overrun: bool,
    found: bool,
}

impl TopologyProvider for StaticTopology {
    fn same_session(&self, a: ScopeId, b: ScopeId) -> T3 {
        self.query(0, a, b)
    }

    fn same_client(&self, a: ScopeId, b: ScopeId) -> T3 {
        self.query(1, a, b)
    }

    fn same_port(&self, a: ScopeId, b: ScopeId) -> T3 {
        self.query(2, a, b)
    }

    fn same_ap(&self, a: ScopeId, b: ScopeId) -> T3 {
        self.query(3, a, b)
    }

    fn same_vlan(&self, a: ScopeId, b: ScopeId) -> T3 {
        self.query(4, a, b)
    }

    fn upstream_of(&self, up: ScopeId, down: ScopeId) -> T3 {
        if up == down {
            // A node is never upstream of itself (no implicit self-loop).
            return T3::False;
        }
        if !self.known.contains(&up) || !self.known.contains(&down) {
            return T3::Unknown;
        }
        let mut on_path = BTreeSet::new();
        let mut done = BTreeSet::new();
        let mut outcome = DfsOutcome::default();
        self.dfs(
            down,
            up,
            self.max_hops,
            &mut on_path,
            &mut done,
            &mut outcome,
        );
        if outcome.cycle {
            // Cycle isolation (07 §6): resolve to False + ADGL3006.
            self.record(TopologyDiagnostic::UpstreamCycle { from: down });
            return T3::False;
        }
        if outcome.hop_overrun && !outcome.found {
            self.record(TopologyDiagnostic::HopBoundExceeded { from: down });
            return T3::False;
        }
        T3::from(outcome.found)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_relations_true_false_unknown() {
        let a = ScopeId::vlan(1);
        let b = ScopeId::vlan(2);
        let c = ScopeId::vlan(3);
        let mut topo = StaticTopology::new(16);
        topo.relate_session(a, b).declare(c);
        assert_eq!(topo.same_session(a, b), T3::True);
        assert_eq!(topo.same_session(b, a), T3::True, "symmetric");
        assert_eq!(topo.same_session(a, a), T3::True, "reflexive");
        assert_eq!(topo.same_session(a, c), T3::False, "both known, unrelated");
        assert_eq!(
            topo.same_session(a, ScopeId::vlan(99)),
            T3::Unknown,
            "unknown scope"
        );
        assert_eq!(topo.same_vlan(a, b), T3::False, "relations are independent");
    }

    #[test]
    fn unknown_must_not_collapse_to_false_adr010() {
        // ADR-010 / C10: topology absence is T3::Unknown, never False.
        let topo = StaticTopology::new(16);
        let a = ScopeId::vlan(1);
        let b = ScopeId::vlan(2);
        for ans in [
            topo.same_session(a, b),
            topo.same_client(a, b),
            topo.same_port(a, b),
            topo.same_ap(a, b),
            topo.same_vlan(a, b),
            topo.upstream_of(a, b),
        ] {
            assert_eq!(ans, T3::Unknown);
            assert!(
                ans.is_unknown() && !ans.is_false() && !ans.is_true(),
                "Unknown must not collapse to False/True: {ans:?}"
            );
        }
        assert!(topo.diagnostics().is_empty(), "Unknown is not a cycle/hop diagnostic");
    }

    #[test]
    fn upstream_of_transitive_and_bounded() {
        let (r1, r2, r3) = (
            ScopeId::port(1, 1),
            ScopeId::port(2, 1),
            ScopeId::port(3, 1),
        );
        let mut topo = StaticTopology::new(16);
        topo.upstream_edge(r1, r2).upstream_edge(r2, r3);
        assert_eq!(topo.upstream_of(r1, r3), T3::True, "transitive");
        assert_eq!(topo.upstream_of(r3, r1), T3::False, "directed");
        assert_eq!(
            topo.upstream_of(r1, r1),
            T3::False,
            "never upstream of itself"
        );
        assert_eq!(topo.upstream_of(r1, ScopeId::port(9, 9)), T3::Unknown);
        assert!(topo.diagnostics().is_empty());
    }

    #[test]
    fn upstream_cycle_resolves_false_with_adgl3006() {
        // 12 §3.4: circular upstream_of → no panic, False, diagnostic.
        let (r1, r2) = (ScopeId::port(1, 1), ScopeId::port(2, 1));
        let mut topo = StaticTopology::new(16);
        topo.upstream_edge(r1, r2).upstream_edge(r2, r1);
        assert_eq!(topo.upstream_of(r1, r2), T3::False, "cycle isolation");
        let diags = topo.diagnostics();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code(), "ADGL3006");
        assert!(matches!(diags[0], TopologyDiagnostic::UpstreamCycle { .. }));
    }

    #[test]
    fn hop_bound_is_enforced() {
        let scopes: Vec<ScopeId> = (0..6).map(|i| ScopeId::port(i, 0)).collect();
        let mut topo = StaticTopology::new(2);
        for w in scopes.windows(2) {
            topo.upstream_edge(w[1], w[0]); // chain: 5 upstream of 4 ... of 0
        }
        assert_eq!(
            topo.upstream_of(scopes[2], scopes[0]),
            T3::True,
            "within bound"
        );
        assert_eq!(
            topo.upstream_of(scopes[5], scopes[0]),
            T3::False,
            "beyond bound"
        );
        assert!(
            topo.diagnostics()
                .iter()
                .any(|d| matches!(d, TopologyDiagnostic::HopBoundExceeded { .. }))
        );
    }
}
