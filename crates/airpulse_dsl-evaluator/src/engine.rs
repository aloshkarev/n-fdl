//! The ADGL execution engine per `docs/idea/spec/07-runtime.md` §5 pipeline:
//! `ingest → route → anchor match → suspend if upper > wm → advance_watermark
//! → resume → correlate → exec intents`, with the rule semantics of
//! `docs/idea/spec/03-semantics.md` §3–§4 and the watermark lifecycle of
//! `docs/idea/spec/08-stream-watermarking.md`.
//!
//! # Concurrency model (C12, `07` §10)
//!
//! The v1 engine is single-threaded (`&mut self`): intra-partition execution
//! is serial by construction, which is exactly the spec's requirement. The
//! design does not preclude cross-partition parallelism: all graph state
//! lives in the `DashMap`-sharded [`GraphStore`] behind `&self` entry
//! guards, partition guards are never held across nested rule execution
//! (each store mutation acquires and releases its own guard), and the
//! watermark is a global atomic. Lifting to one-worker-per-partition-shard
//! only requires splitting the diagnostics/sink accumulation per partition
//! and merging by `(event_time, rule_decl_order, scope_id)` (ADR-012).
//!
//! # Watermark policy (`08` §2 / ADR-004)
//!
//! - **Offline:** [`Engine::ingest`] advances `wm = max(seen event-time)`,
//!   then resume / GC / anchor-match. Out-of-order events with
//!   `time ≤ wm` are accepted; if a matching correlate already resolved
//!   absent, the engine audits `ADGL3002 LateEvidence` and does **not**
//!   retroactively re-infer (append-only provenance, `08` §4).
//! - **Live:** `wm = max(prev, t - W)` per source with idle-source
//!   exclusion (`08` §2.2–§2.3). Late events beyond `allowed_lateness`
//!   are dropped to [`Engine::late_side_output`] with `ADGL3003`.

use std::collections::{BTreeSet, HashMap};

use airpulse_dsl_catalog::{
    EventOrBindingType, FieldType, exclusivity_defaults, resolve_event, resolve_metric_path,
    resolve_problem,
};
use airpulse_dsl_ir::{
    AnchorKey, CorrelateSource, CorrelateSpec, ExclusivityGroup, Intent, PendingMatch, Predicate,
    ProgramImage, ProvKey, RuleInstance, RuleKind, Symbol, WindowProof,
};
use airpulse_dsl_store::{
    AmbiguityNode, AmbiguityState, CauseNode, EdgeEndpoint, EventNode, EvidenceEdge,
    EvidenceEdgeKind, GraphStore, Limits, ProblemNode, RuntimeProvKey, window_id,
};
use airpulse_dsl_types::{
    ActionKind, CauseKind, Confidence, DurationMs, EventTime, EventType, MetricPath, NodeId,
    ProblemKind, RuleId, SarifId, ScopeId, Severity, T3, Weight,
};

use crate::binding::{Binding, Bound, CauseSnapshot, ProblemSnapshot};
use crate::diag::EngineDiagnostic;
use crate::evidence::collect_problem_evidence;
use crate::extract::{CauseView, ProblemView, Snapshot};
use crate::interner::ScopeInterner;
use crate::predicate::{PredCtx, eval_predicate};
use crate::sarif::{SarifOptions, to_sarif_with_options};
use crate::sink::{ActionIntent, ActionSink, OfflineAuditSink, RunMode};
use crate::topology::{TopoFunc, TopologyProvider};

/// Deterministic mix of the static `target_expr_hash` with the *evaluated*
/// target scope, realizing the full `(R.id, K, T, window_id)` provenance
/// dedup key of `03-semantics.md` §3.3 — the IR's [`ProvKey`] carries only
/// the expression hash, so two different evaluated targets of the same
/// expression must not collide.
const fn mix_target(expr_hash: u64, target_key: u64) -> u64 {
    // splitmix64-style finalization — deterministic across runs/platforms.
    let mut z = expr_hash ^ target_key.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// A correlate that resolved `absent` and whose window is closed — used to
/// detect offline late evidence (`08` §4 `ADGL3002`).
#[derive(Debug, Clone)]
struct ResolvedAbsent {
    scope: ScopeId,
    rule: RuleId,
    event_type: EventType,
    lo: EventTime,
    hi: EventTime,
}

/// Per-source live watermark state (`08` §2.2–§2.3).
#[derive(Debug, Clone, Copy)]
struct SourceWm {
    local_wm: EventTime,
    last_seen_wall: EventTime,
}

/// The ADGL execution engine (`07-runtime.md` §5).
///
/// Owns a [`GraphStore`] plus a verified [`ProgramImage`], a topology
/// oracle, and an action sink. See the module docs for the concurrency and
/// watermark models.
#[derive(Debug)]
pub struct Engine<'img, T, S> {
    store: GraphStore,
    image: &'img ProgramImage,
    topo: T,
    sink: S,
    mode: RunMode,
    interner: ScopeInterner,
    diagnostics: Vec<EngineDiagnostic>,
    /// Every partition this engine created — snapshot iteration order
    /// (`BTreeSet`: deterministic, ADR-012).
    touched: BTreeSet<ScopeId>,
    next_node_id: u64,
    last_event_time: Option<EventTime>,
    /// Max forward window across the image — the end-of-stream flush bound
    /// (`08` §3.4 `last_event_time + max_forward_window + 1`).
    image_max_forward: DurationMs,
    /// Problem-emission cooldown (`03` §3.4 F3). The spec does not yet fix
    /// where the per-rule cooldown lives (see crate-level spec notes);
    /// Phase 1 uses one engine-wide value, defaulting to
    /// `limits.dedup_window`.
    problem_cooldown: DurationMs,
    /// Strict privacy for SARIF evidence emission (ADR-009).
    strict_privacy: bool,
    finished: bool,
    suspended: u64,
    resumed: u64,
    /// Live late-event side-output (`08` §4 `ADGL3003`).
    late_side_output: Vec<EventNode>,
    /// Offline resolved-absent correlate windows for late-evidence audit.
    resolved_absent: Vec<ResolvedAbsent>,
    /// Live per-source watermark / idle tracking (`08` §2.3).
    sources: HashMap<ScopeId, SourceWm>,
    /// Wall-clock used for idle-source detection (live). Advanced by
    /// ingest event-time and [`Engine::advance_wall_clock`].
    wall_clock: EventTime,
}

impl<'img, T: TopologyProvider, S: ActionSink> Engine<'img, T, S> {
    /// Builds an engine over a fresh [`GraphStore`] with the given ADR-011
    /// limits.
    #[must_use]
    pub fn new(image: &'img ProgramImage, topo: T, sink: S, limits: Limits, mode: RunMode) -> Self {
        let image_max_forward = image
            .rules
            .iter()
            .map(RuleInstance::max_forward)
            .max()
            .unwrap_or_default();
        let problem_cooldown = limits.dedup_window;
        Engine {
            store: GraphStore::new(limits),
            image,
            topo,
            sink,
            mode,
            interner: ScopeInterner::new(),
            diagnostics: Vec::new(),
            touched: BTreeSet::new(),
            next_node_id: 0,
            last_event_time: None,
            image_max_forward,
            problem_cooldown,
            strict_privacy: false,
            finished: false,
            suspended: 0,
            resumed: 0,
            late_side_output: Vec::new(),
            resolved_absent: Vec::new(),
            sources: HashMap::new(),
            wall_clock: EventTime::from_millis(i64::MIN),
        }
    }

    /// Overrides the engine-wide problem-emission cooldown (F3, `03` §3.4).
    #[must_use]
    pub fn with_problem_cooldown(mut self, cooldown: DurationMs) -> Self {
        self.problem_cooldown = cooldown;
        self
    }

    /// Enables strict-privacy redaction for SARIF evidence (`10` §11, ADR-009).
    #[must_use]
    pub fn with_strict_privacy(mut self, strict_privacy: bool) -> Self {
        self.strict_privacy = strict_privacy;
        self
    }

    /// Whether strict-privacy redaction is enabled for SARIF emission.
    #[must_use]
    pub fn strict_privacy(&self) -> bool {
        self.strict_privacy
    }

    /// Interns a scope for target-key resolution (see
    /// [`crate::schema::EVENT_FIELD_TARGET`]); harnesses intern every scope
    /// their synthetic events reference as a target. Returns the `i64` key
    /// to store in the event field.
    pub fn intern_scope(&mut self, scope: ScopeId) -> i64 {
        self.interner.intern(scope)
    }

    // ── read-out ─────────────────────────────────────────────────────────

    /// The underlying store (test/diagnostic access, e.g. `pending_len`).
    #[must_use]
    pub fn store(&self) -> &GraphStore {
        &self.store
    }

    /// The topology provider.
    #[must_use]
    pub fn topo(&self) -> &T {
        &self.topo
    }

    /// The action sink.
    #[must_use]
    pub fn sink(&self) -> &S {
        &self.sink
    }

    /// Engine diagnostics in occurrence order.
    #[must_use]
    pub fn diagnostics(&self) -> &[EngineDiagnostic] {
        &self.diagnostics
    }

    /// Total anchor matches suspended into WaitQueues (`08` §3.1).
    #[must_use]
    pub fn suspended(&self) -> u64 {
        self.suspended
    }

    /// Total suspended matches resumed (`08` §3.2).
    #[must_use]
    pub fn resumed(&self) -> u64 {
        self.resumed
    }

    /// Live late-event side-output (`08` §4): events dropped with
    /// `ADGL3003 LateEventDropped`. Empty in offline mode.
    #[must_use]
    pub fn late_side_output(&self) -> &[EventNode] {
        &self.late_side_output
    }

    /// Advances the live wall-clock used for idle-source detection
    /// (`08` §2.3). Monotone: a smaller `t` is a no-op. Offline ignores this.
    pub fn advance_wall_clock(&mut self, t: EventTime) {
        if self.mode != RunMode::Live {
            return;
        }
        if self.wall_clock.millis() == i64::MIN || t > self.wall_clock {
            self.wall_clock = t;
        }
    }

    /// Final graph state (causes + problems) in deterministic order
    /// (ADR-012); audit is left empty — sinks own their logs (see
    /// [`Engine::snapshot`] for the offline-audit engine).
    #[must_use]
    pub fn graph_snapshot(&self) -> Snapshot {
        let mut causes = Vec::new();
        let mut problems = Vec::new();
        for &scope in &self.touched {
            let Some(part) = self.store.partition(scope) else {
                continue;
            };
            for ((kind, target), node) in &part.causes {
                causes.push(CauseView {
                    scope,
                    kind: kind.clone(),
                    target: *target,
                    time: node.time,
                    confidence: node.confidence,
                });
            }
            let cause_kinds_by_node: HashMap<_, _> = part
                .causes
                .values()
                .map(|node| (node.id, node.kind.clone()))
                .collect();
            for p in part.problems() {
                let mut cause_kinds = Vec::new();
                for node in &p.evidence {
                    let Some(kind) = cause_kinds_by_node.get(node) else {
                        continue;
                    };
                    if !cause_kinds.contains(kind) {
                        cause_kinds.push(kind.clone());
                    }
                }
                let evidence_fields = collect_problem_evidence(&self.store, scope, p, &part);
                problems.push(ProblemView {
                    scope,
                    kind: p.kind.clone(),
                    target: p.target,
                    time: p.time,
                    severity: p.severity,
                    sarif_id: p.sarif_id.clone(),
                    cause_kinds,
                    evidence_fields,
                    superseded: p.superseded,
                });
            }
        }
        causes.sort_by(|a, b| (a.scope, &a.kind, a.target).cmp(&(b.scope, &b.kind, b.target)));
        // Problems: scopes already in sorted (BTreeSet) order; per-scope
        // emission (append) order preserved.
        Snapshot {
            causes,
            problems,
            audit: Vec::new(),
        }
    }

    // ── pipeline (07 §5) ─────────────────────────────────────────────────

    /// Ingests one event under the configured [`RunMode`] watermark policy
    /// (`08` §2 / §4): offline max-seen + late-evidence audit; live
    /// bounded-out-of-orderness with idle-source and late drop to
    /// side-output. Then resume sweep + GC + anchor matching.
    pub fn ingest(&mut self, event: EventNode) {
        let scope = event.scope;
        self.touched.insert(scope);
        self.interner.intern(scope);
        self.last_event_time = Some(self.last_event_time.unwrap_or(event.time).max(event.time));

        let prev_wm = self.store.watermark();
        let watermark_initialized = prev_wm.millis() != i64::MIN;
        let is_late = watermark_initialized && event.time <= prev_wm;

        if self.mode == RunMode::Live && is_late {
            let cutoff = prev_wm.sub(self.store.limits().allowed_lateness);
            if event.time <= cutoff {
                self.diagnostics.push(EngineDiagnostic::LateEventDropped {
                    scope,
                    event: event.id,
                    time: event.time,
                    wm: prev_wm,
                });
                self.late_side_output.push(event);
                return;
            }
            // Within allowed_lateness: fall through and accept.
        }

        if self.mode == RunMode::Offline && is_late {
            self.audit_offline_late_evidence(&event, prev_wm);
        }

        let wm = match self.mode {
            RunMode::Offline => self.store.advance_watermark(event.time),
            RunMode::Live => self.advance_live_watermark(scope, event.time),
        };

        if let Some(d) = self.store.push_event(event.clone()) {
            self.diagnostics.push(EngineDiagnostic::Store(d));
        }
        self.resume_expired(wm);
        self.gc_sweep(wm);
        self.match_anchors(&event, wm);
    }

    /// Live per-source watermark advance with idle-source exclusion
    /// (`08` §2.2–§2.3).
    fn advance_live_watermark(&mut self, scope: ScopeId, event_time: EventTime) -> EventTime {
        // Wall-clock tracks the live stream by default (event-time), and may
        // be pushed ahead via `advance_wall_clock` for idle tests.
        if self.wall_clock.millis() == i64::MIN || event_time > self.wall_clock {
            self.wall_clock = event_time;
        }
        let wall = self.wall_clock;
        let w = self.store.limits().max_disorder;
        let candidate = event_time.sub(w);
        let entry = self.sources.entry(scope).or_insert(SourceWm {
            local_wm: candidate,
            last_seen_wall: wall,
        });
        if candidate > entry.local_wm {
            entry.local_wm = candidate;
        }
        entry.last_seen_wall = wall;

        let idle = self.store.limits().idle_timeout;
        let mut global: Option<EventTime> = None;
        for src in self.sources.values() {
            let idle_for = wall.millis().saturating_sub(src.last_seen_wall.millis());
            if idle_for >= idle.millis() {
                continue; // excluded from min
            }
            global = Some(match global {
                Some(g) if src.local_wm < g => src.local_wm,
                Some(g) => g,
                None => src.local_wm,
            });
        }
        // If every source is idle, keep the prior watermark (no regression).
        let target = global.unwrap_or_else(|| self.store.watermark());
        self.store.advance_watermark(target)
    }

    /// Offline `ADGL3002`: late event matches a previously resolved-absent
    /// correlate window — audit only, no re-infer (`08` §4).
    fn audit_offline_late_evidence(&mut self, event: &EventNode, wm: EventTime) {
        for ra in &self.resolved_absent {
            if ra.scope != event.scope || ra.event_type != event.event_type {
                continue;
            }
            if event.time >= ra.lo && event.time <= ra.hi {
                self.diagnostics.push(EngineDiagnostic::LateEvidence {
                    scope: event.scope,
                    rule: ra.rule.clone(),
                    event: event.id,
                    event_type: event.event_type.clone(),
                    wm,
                });
                // One audit per late event is enough for the differential /
                // SARIF incompleteFingerprints use case (08 §4).
                break;
            }
        }
    }

    /// Records absent correlate windows after bindings resolve so later
    /// offline late evidence can be audited (`08` §4). Filters to
    /// [`Binding::Absent`] only — safe for `present`/`absent` branches and
    /// branchless rules.
    fn record_resolved_absent(
        &mut self,
        rule: &RuleInstance,
        bindings: &[Binding],
        anchor: &Bound,
        scope: ScopeId,
    ) {
        let anchor_time = anchor.time();
        for (idx, spec) in rule.correlates.iter().enumerate() {
            let binding = bindings.get(idx + 1);
            if !matches!(binding, Some(Binding::Absent)) {
                continue;
            }
            let CorrelateSource::Event(event_type) = &spec.source else {
                continue;
            };
            let (back, fwd) = match spec.window {
                WindowProof::Calculable { back, forward } => (back, forward),
                WindowProof::RuntimeCheck => continue,
            };
            self.resolved_absent.push(ResolvedAbsent {
                scope,
                rule: rule.id.clone(),
                event_type: event_type.clone(),
                lo: anchor_time.sub(back),
                hi: anchor_time.add(fwd),
            });
        }
    }

    /// Explicit watermark advance (e.g. punctuation from the capture
    /// driver): monotone `fetch_max`, then resume sweep + GC (`07` §5).
    pub fn advance_watermark(&mut self, t: EventTime) {
        let wm = self.store.advance_watermark(t);
        self.resume_expired(wm);
        self.gc_sweep(wm);
    }

    /// End-of-stream flush (`08` §3.4): pushes the watermark past every
    /// possible pending `upper_bound` (`last_event_time + max_forward + 1`)
    /// so absent-branches resolve deterministically, then GCs. Idempotent.
    pub fn finish(&mut self) {
        if self.finished {
            return;
        }
        self.finished = true;
        let Some(last) = self.last_event_time else {
            return;
        };
        let one_ms = DurationMs::from_millis(1).unwrap_or_default();
        let flush = last.add(self.image_max_forward).add(one_ms);
        self.advance_watermark(flush);
    }

    fn gc_sweep(&mut self, wm: EventTime) {
        let _ = self.store.gc(wm, self.store.limits().max_lookback);
        let _ = self.store.prune_emitted(wm, self.problem_cooldown);
    }

    /// Resume sweep (`08` §3.2): pop every pending with `upper < wm`
    /// (strict) in deterministic order and re-run correlate + body against
    /// the now-complete window.
    fn resume_expired(&mut self, wm: EventTime) {
        let expired: Vec<PendingMatch> = self.store.pop_expired(wm);
        for m in expired {
            let anchor_event = self
                .store
                .ring(m.scope)
                .and_then(|ring| ring.get(m.anchor_event).cloned());
            let Some(evt) = anchor_event else {
                self.diagnostics.push(EngineDiagnostic::MissingAnchor {
                    rule: m.rule.clone(),
                });
                continue;
            };
            let img = self.image;
            let Some(rule) = img.rules.iter().find(|r| r.id == m.rule) else {
                // Only possible if the image changed under a live store —
                // never silently drop the pending match.
                self.diagnostics.push(EngineDiagnostic::RuleNotInImage {
                    rule: m.rule.clone(),
                });
                continue;
            };
            self.resumed += 1;
            let mut budget = self.store.limits().max_rule_firings_per_event;
            self.run_rule(rule, Bound::Event(evt), m.scope, &mut budget);
        }
    }

    /// Anchor matching for one routed event (`03` §3.1; `07` §5 step 2).
    fn match_anchors(&mut self, event: &EventNode, wm: EventTime) {
        let img = self.image;
        let scope = event.scope;
        let rules: Vec<&'img RuleInstance> = img
            .rules_for(
                AnchorKey::Event(&event.event_type),
                scope.scope_type(),
                RuleKind::Evidence,
            )
            .collect();
        // One firings budget per ingested event (ADR-011
        // max_rule_firings_per_event), covering decision cascades.
        let mut budget = self.store.limits().max_rule_firings_per_event;
        for rule in rules {
            let bindings = [Binding::Bound(Bound::Event(event.clone()))];
            match self.eval_pred(rule, &rule.anchor.predicate, &bindings, scope) {
                Some(t) if t.is_true() => {}
                _ => continue, // False/Unknown/no anchor match (03 §3.1)
            }
            let upper = event.time.add(rule.max_forward());
            if upper > wm {
                // Forward window not closed → suspend (08 §3.1).
                self.suspended += 1;
                let pending = PendingMatch {
                    rule: rule.id.clone(),
                    anchor_event: event.id,
                    upper_bound: upper,
                    scope,
                };
                if let Some(d) = self.store.suspend(pending) {
                    self.diagnostics.push(EngineDiagnostic::Store(d));
                }
            } else {
                // Backward-only (upper == wm) or late anchor → immediate.
                self.run_rule(rule, Bound::Event(event.clone()), scope, &mut budget);
            }
        }
    }

    // ── correlate + body (03 §3.2–3.7) ───────────────────────────────────

    /// Runs one rule instance: resolve correlate bindings, select the
    /// branch (`03` §3.7), execute intents in order (C12).
    ///
    /// # Body vs. branches
    ///
    /// `branches` and `body` are *mutually exclusive*: when a
    /// [`BranchTable`] is present, only the selected branch body executes;
    /// `rule.body` executes only when `branches` is `None`. The spec is
    /// silent on the interaction (`06` §2.1 lists both fields;
    /// `03` §3.7 describes only branch dispatch), but the IR crate resolved
    /// the ambiguity by making `branches` optional — `RuleInstance::body`
    /// is documented as "executed when there is no branch table", and every
    /// example rule uses exactly one of the two. Executing both would
    /// double-fire intents for branched rules.
    fn run_rule(
        &mut self,
        rule: &'img RuleInstance,
        anchor: Bound,
        scope: ScopeId,
        budget: &mut usize,
    ) {
        if *budget == 0 {
            self.diagnostics
                .push(EngineDiagnostic::RuleFiringsExceeded { scope });
            return;
        }
        *budget -= 1;

        let bindings = self.resolve_bindings(rule, &anchor, scope);
        // Spec §4 keys off resolved-absent correlates (window closed), not
        // which branch fired — record for present/absent/branchless alike.
        self.record_resolved_absent(rule, &bindings, &anchor, scope);
        match &rule.branches {
            Some(bt) => match self.eval_pred(rule, &bt.cond, &bindings, scope) {
                Some(T3::True) => {
                    self.exec_intents(&bt.then_body, rule, &bindings, &anchor, scope, budget);
                }
                Some(T3::False) => {
                    if let Some(else_body) = &bt.else_body {
                        self.exec_intents(else_body, rule, &bindings, &anchor, scope, budget);
                    }
                }
                Some(T3::Unknown) => {
                    // C10: Unknown → unknown_body (request_topology).
                    self.exec_intents(&bt.unknown_body, rule, &bindings, &anchor, scope, budget);
                }
                None => {} // predicate error already recorded
            },
            None => {
                self.exec_intents(&rule.body, rule, &bindings, &anchor, scope, budget);
            }
        }
    }

    /// Resolves every correlate binding (`03` §3.2): scan candidates in the
    /// inclusive window and topology-filter in deterministic order. Count
    /// mode stops after `min_match` true candidates and binds the earliest
    /// true candidate as its witness. Fewer than `min_match` true candidates
    /// plus any `Unknown` resolves `Unknown` (C10); otherwise it is `Absent`.
    fn resolve_bindings(
        &mut self,
        rule: &'img RuleInstance,
        anchor: &Bound,
        scope: ScopeId,
    ) -> Vec<Binding> {
        let mut bindings = vec![Binding::Bound(anchor.clone())];
        for spec in &rule.correlates {
            let (back, fwd) = match spec.window {
                WindowProof::Calculable { back, forward } => (back, forward),
                WindowProof::RuntimeCheck => {
                    self.diagnostics.push(EngineDiagnostic::RuntimeCheckWindow {
                        rule: rule.id.clone(),
                    });
                    bindings.push(Binding::Absent);
                    continue;
                }
            };
            let anchor_time = anchor.time();
            let lo = anchor_time.sub(back);
            let hi = anchor_time.add(fwd);

            let mut candidates: Vec<Bound> = Vec::new();
            match &spec.source {
                CorrelateSource::Event(event_type) => {
                    if let Some(ring) = self.store.ring(scope) {
                        for e in ring.scan_window(anchor_time, back, fwd) {
                            let self_match = matches!(anchor, Bound::Event(a) if a.id == e.id);
                            if &e.event_type == event_type && !self_match {
                                candidates.push(Bound::Event(e.clone()));
                            }
                        }
                    }
                    // Ring order is already (time, arrival) — deterministic.
                }
                CorrelateSource::Problem(kind) => {
                    if let Some(part) = self.store.partition(scope) {
                        for p in part.problems() {
                            let self_match = matches!(anchor, Bound::Problem(a) if a.node == p.id);
                            if &p.kind == kind && p.time >= lo && p.time <= hi && !self_match {
                                candidates.push(Bound::Problem(ProblemSnapshot {
                                    node: p.id,
                                    kind: p.kind.clone(),
                                    target: p.target,
                                    time: p.time,
                                }));
                            }
                        }
                    }
                    // Emission (append) order; ids ascend — sort by
                    // (time, id) for earliest-first (03 §3.2).
                    candidates.sort_by_key(bound_sort_key);
                }
                CorrelateSource::Cause(kind) => {
                    if let Some(part) = self.store.partition(scope) {
                        for ((k, target), node) in &part.causes {
                            let self_match = matches!(anchor, Bound::Cause(a) if a.node == node.id);
                            if k == kind && node.time >= lo && node.time <= hi && !self_match {
                                candidates.push(Bound::Cause(CauseSnapshot {
                                    node: node.id,
                                    kind: k.clone(),
                                    target: *target,
                                    time: node.time,
                                    confidence: node.confidence,
                                }));
                            }
                        }
                    }
                    // HashMap iteration order is arbitrary — sort (ADR-012).
                    candidates.sort_by_key(bound_sort_key);
                }
            }

            // Resolve the topo function once per correlate spec (not per
            // candidate) so an invalid func_idx yields exactly one
            // diagnostic per rule evaluation.
            let func = TopoFunc::from_idx(spec.topo.func_idx);
            if func.is_none() {
                self.diagnostics.push(EngineDiagnostic::PredicateError {
                    rule: rule.id.clone(),
                    error: crate::error::CorrelateError::UnknownTopoFunction {
                        func: spec.topo.func_idx.0,
                    },
                });
            }
            let mut match_count = 0u8;
            let mut witness: Option<Bound> = None;
            let mut saw_unknown = false;
            for cand in candidates {
                match self.eval_topo(spec, func, rule, &bindings, &cand) {
                    T3::True => {
                        match_count = match_count.saturating_add(1);
                        if witness.is_none() {
                            witness = Some(cand);
                        }
                        if match_count >= spec.min_match {
                            break; // earliest witness retained; stop at N (03 §3.2)
                        }
                    }
                    T3::Unknown => saw_unknown = true,
                    T3::False => {}
                }
            }
            let binding = match (match_count >= spec.min_match, witness, saw_unknown) {
                (true, Some(witness), _) => Binding::Bound(witness),
                // Defensive degradation: every true match sets `witness`, but
                // runtime paths must remain panic-free if that invariant ever
                // changes. This crate has no logging dependency; diagnostics
                // are reserved for externally actionable runtime failures.
                (true, None, _) => Binding::Unknown,
                (false, _, true) => Binding::Unknown,
                (false, _, false) => Binding::Absent,
            };
            bindings.push(binding);
        }
        bindings
    }

    /// Evaluates a correlate's topo call against one candidate
    /// (`03` §3.2 `⟦topo⟧(anchor, c)`). Unresolvable arguments (unbound
    /// binding, un-interned key) yield `Unknown` — topology identity is
    /// unknowable, which is exactly the C10 branch. `func` is pre-resolved
    /// by the caller (`None` = invalid func_idx, diagnosed once per rule
    /// evaluation, resolves `Unknown` for every candidate).
    fn eval_topo(
        &mut self,
        spec: &CorrelateSpec,
        func: Option<TopoFunc>,
        rule: &RuleInstance,
        partial: &[Binding],
        candidate: &Bound,
    ) -> T3 {
        let Some(func) = func else {
            return T3::Unknown;
        };
        if spec.topo.args.len() != 2 {
            return T3::Unknown;
        }
        let resolve = |this: &mut Self, path: &MetricPath| -> Option<ScopeId> {
            let head = path.segments().next()?;
            let bound = if head == spec.binding.as_str() {
                candidate
            } else {
                let idx = binding_index_of(rule, head)?;
                partial.get(idx)?.bound()?
            };
            this.resolve_scope_path_from_bound(rule, path, head, bound)
        };
        match (
            resolve(self, &spec.topo.args[0]),
            resolve(self, &spec.topo.args[1]),
        ) {
            (Some(a), Some(b)) => func.call(&self.topo, a, b),
            _ => T3::Unknown,
        }
    }

    fn eval_pred(
        &mut self,
        rule: &RuleInstance,
        pred: &Predicate,
        bindings: &[Binding],
        scope: ScopeId,
    ) -> Option<T3> {
        let ctx = PredCtx {
            bindings,
            scope,
            interner: &self.interner,
            topo: &self.topo,
        };
        match eval_predicate(pred, &ctx) {
            Ok(t) => Some(t),
            Err(error) => {
                self.diagnostics.push(EngineDiagnostic::PredicateError {
                    rule: rule.id.clone(),
                    error,
                });
                None
            }
        }
    }

    /// Resolves an intent `target` expression (`<binding>.target`) and, for
    /// event bindings, catalog-typed routing paths (`<binding>.<field>`).
    fn resolve_target(
        &mut self,
        rule: &RuleInstance,
        path: &MetricPath,
        bindings: &[Binding],
    ) -> Option<ScopeId> {
        let head = path.segments().next()?;
        let idx = binding_index_of(rule, head)?;
        let bound = bindings.get(idx)?.bound()?;
        self.resolve_scope_path_from_bound(rule, path, head, bound)
    }

    fn resolve_scope_path_from_bound(
        &mut self,
        rule: &RuleInstance,
        path: &MetricPath,
        binding_name: &str,
        bound: &Bound,
    ) -> Option<ScopeId> {
        let mut segments = path.segments();
        let _head = segments.next()?;
        let tail = segments.next();
        if segments.next().is_some() {
            // Current IR/lowering only emits two-segment metric paths.
            self.diagnostics
                .push(EngineDiagnostic::UnsupportedTargetTail {
                    rule: rule.id.clone(),
                    path: path.clone(),
                    binding: binding_name.to_string().into_boxed_str(),
                    tail: tail.unwrap_or_default().to_string().into_boxed_str(),
                });
            return None;
        }

        match bound {
            Bound::Event(event) => match tail {
                None | Some("target") => bound.target(&self.interner),
                Some(tail_name) => {
                    let schema = resolve_event(event.event_type.as_str())?;
                    let Some((field_idx, field_ty)) = resolve_metric_path(
                        EventOrBindingType::Event(&event.event_type),
                        tail_name,
                    ) else {
                        self.diagnostics
                            .push(EngineDiagnostic::UnsupportedTargetTail {
                                rule: rule.id.clone(),
                                path: path.clone(),
                                binding: binding_name.to_string().into_boxed_str(),
                                tail: tail_name.to_string().into_boxed_str(),
                            });
                        return None;
                    };
                    let is_scope_field = matches!(field_ty, FieldType::ScopeId(_));
                    let is_routing_path = schema
                        .routing_paths
                        .iter()
                        .any(|route| route.path == tail_name);
                    if !(is_routing_path || is_scope_field) {
                        self.diagnostics
                            .push(EngineDiagnostic::UnsupportedTargetTail {
                                rule: rule.id.clone(),
                                path: path.clone(),
                                binding: binding_name.to_string().into_boxed_str(),
                                tail: tail_name.to_string().into_boxed_str(),
                            });
                        return None;
                    }
                    let key = event.field(field_idx)?;
                    self.interner.resolve(key)
                }
            },
            Bound::Cause(_) | Bound::Problem(_) => match tail {
                None | Some("target") => bound.target(&self.interner),
                Some(tail_name) => {
                    self.diagnostics
                        .push(EngineDiagnostic::UnsupportedTargetTail {
                            rule: rule.id.clone(),
                            path: path.clone(),
                            binding: binding_name.to_string().into_boxed_str(),
                            tail: tail_name.to_string().into_boxed_str(),
                        });
                    None
                }
            },
        }
    }

    fn evidence_endpoints(
        &self,
        rule: &RuleInstance,
        evidence: &[Symbol],
        bindings: &[Binding],
    ) -> Vec<airpulse_dsl_store::EdgeEndpoint> {
        evidence
            .iter()
            .filter_map(|sym| {
                let idx = binding_index_of(rule, sym.as_str())?;
                Some(bindings.get(idx)?.bound()?.endpoint())
            })
            .collect()
    }

    // ── intent execution (03 §3.3–3.6) ───────────────────────────────────

    #[allow(clippy::too_many_arguments)]
    fn exec_intents(
        &mut self,
        intents: &[Intent],
        rule: &'img RuleInstance,
        bindings: &[Binding],
        anchor: &Bound,
        scope: ScopeId,
        budget: &mut usize,
    ) {
        for intent in intents {
            match intent {
                Intent::InferCause {
                    cause,
                    target,
                    weight,
                    evidence,
                    provenance_key,
                    ..
                } => {
                    self.exec_infer(
                        rule,
                        cause,
                        target,
                        *weight,
                        evidence,
                        provenance_key,
                        bindings,
                        anchor,
                        scope,
                        budget,
                    );
                }
                Intent::EmitProblem {
                    problem,
                    target,
                    severity,
                    evidence,
                    sarif_id,
                    ..
                } => {
                    self.exec_emit_problem(
                        rule, problem, target, *severity, evidence, sarif_id, bindings, scope,
                        budget,
                    );
                }
                Intent::EmitAction {
                    kind,
                    arg,
                    target,
                    reason,
                    evidence,
                } => {
                    self.exec_action(rule, *kind, arg, target, reason, evidence, bindings);
                }
                Intent::SupersedeProblem { problem, target } => {
                    self.exec_supersede(rule, problem, target, bindings, scope);
                }
                Intent::MarkAmbiguous { causes, target } => {
                    self.exec_mark_ambiguous(rule, causes, target, bindings, scope);
                }
            }
        }
    }

    /// `infer Cause(K)` (`03` §3.3): provenance dedup, commutative clamped
    /// confidence mutation, evidence edge, then `ConfidenceMutation`
    /// re-evaluation (`03` §3.5) and ambiguity synthesis (`03` §4).
    #[allow(clippy::too_many_arguments)]
    fn exec_infer(
        &mut self,
        rule: &'img RuleInstance,
        cause: &CauseKind,
        target_path: &MetricPath,
        weight: Weight,
        evidence: &[Symbol],
        provenance_key: &ProvKey,
        bindings: &[Binding],
        anchor: &Bound,
        scope: ScopeId,
        budget: &mut usize,
    ) {
        let Some(target) = self.resolve_target(rule, target_path, bindings) else {
            self.diagnostics.push(EngineDiagnostic::UnresolvedTarget {
                rule: rule.id.clone(),
                path: target_path.clone(),
            });
            return;
        };
        let anchor_time = anchor.time();
        let dedup_window = self.store.limits().dedup_window;
        let key = RuntimeProvKey {
            key: ProvKey {
                rule: provenance_key.rule.clone(),
                cause: provenance_key.cause.clone(),
                target_expr_hash: mix_target(provenance_key.target_expr_hash, target.hash_key()),
            },
            window_id: window_id(anchor_time, dedup_window),
        };
        let edge_kind = if weight.is_supporting() {
            EvidenceEdgeKind::Supports
        } else {
            EvidenceEdgeKind::Contradicts
        };
        let endpoints = self.evidence_endpoints(rule, evidence, bindings);
        let max_causes = self.store.limits().max_causes_per_scope;
        let candidate_node_id = self.next_node_id + 1;

        let mutation: CauseSnapshot;
        {
            let mut part = self.store.partition_mut(scope);
            let exists = part.causes.contains_key(&(cause.clone(), target));
            if !exists && part.causes.len() >= max_causes {
                drop(part);
                self.diagnostics
                    .push(EngineDiagnostic::CauseCapacity { scope });
                return;
            }
            if !part.try_insert_provenance(key.clone()) {
                return; // dedup no-op (03 §3.3, C12)
            }
            let mut created = false;
            let node = part
                .causes
                .entry((cause.clone(), target))
                .or_insert_with(|| {
                    created = true;
                    CauseNode {
                        id: NodeId::new(candidate_node_id),
                        kind: cause.clone(),
                        target,
                        // Cause.time = Evt.time at first infer, stable after
                        // (03 §3.3 comment, 04 §3).
                        time: anchor_time,
                        confidence: Confidence::MIN,
                        evidence: Vec::new(),
                        provenance: Vec::new(),
                    }
                });
            node.confidence = node.confidence.apply(weight);
            node.provenance.push(key);
            let dst = node.id;
            mutation = CauseSnapshot {
                node: dst,
                kind: cause.clone(),
                target,
                time: node.time,
                confidence: node.confidence,
            };
            for src in endpoints {
                part.edges.push(EvidenceEdge {
                    kind: edge_kind,
                    src,
                    dst,
                });
            }
            if created {
                self.next_node_id = candidate_node_id;
            }
        } // partition guard dropped before nested execution (store re-entrancy contract)

        self.on_confidence_mutation(&mutation, scope, budget);
        if scope != target && scope.scope_type().is_subsumed_by(target.scope_type()) {
            self.rollup_cause_to_parent(cause, target, scope, budget);
        }
    }

    /// Cross-scope roll-up (`09` §3.2, ADR-003): parent confidence = MAX over
    /// child partitions that inferred the same `(K, target)`.
    fn rollup_cause_to_parent(
        &mut self,
        cause: &CauseKind,
        target: ScopeId,
        child_scope: ScopeId,
        budget: &mut usize,
    ) {
        let parent_scope = target;
        // PERF: O(P) scan over every touched partition per infer. Fine for
        // offline per-session replay (few partitions), but a live engine with
        // many child scopes rolling into one parent should maintain a
        // children(parent_sg) index (`09` §3.2 `children(parent_sg)`) keyed by
        // (parent_scope, cause, target) so the MAX is an O(children) lookup —
        // do not pay an O(all-partitions) walk on every ConfidenceMutation.
        let mut max_conf = Confidence::MIN;
        for &scope in &self.touched {
            if scope == parent_scope {
                continue;
            }
            if !scope.scope_type().is_subsumed_by(parent_scope.scope_type()) {
                continue;
            }
            let Some(part) = self.store.partition(scope) else {
                continue;
            };
            if let Some(node) = part.causes.get(&(cause.clone(), target)) {
                max_conf = std::cmp::max(max_conf, node.confidence);
            }
        }
        // Child cause node for the RollsUp provenance edge (`09` §3.2).
        let child_node = self.store.partition(child_scope).and_then(|part| {
            part.causes
                .get(&(cause.clone(), target))
                .map(|node| node.id)
        });

        self.touched.insert(parent_scope);
        self.interner.intern(parent_scope);
        let max_causes = self.store.limits().max_causes_per_scope;
        let candidate_node_id = self.next_node_id + 1;
        let mutation: CauseSnapshot;
        {
            let mut part = self.store.partition_mut(parent_scope);
            let exists = part.causes.contains_key(&(cause.clone(), target));
            if !exists && part.causes.len() >= max_causes {
                drop(part);
                self.diagnostics.push(EngineDiagnostic::CauseCapacity {
                    scope: parent_scope,
                });
                return;
            }
            let mut created = false;
            let node = part
                .causes
                .entry((cause.clone(), target))
                .or_insert_with(|| {
                    created = true;
                    CauseNode {
                        id: NodeId::new(candidate_node_id),
                        kind: cause.clone(),
                        target,
                        time: self.store.watermark(),
                        confidence: Confidence::MIN,
                        evidence: Vec::new(),
                        provenance: Vec::new(),
                    }
                });
            node.confidence = max_conf;
            let dst = node.id;
            mutation = CauseSnapshot {
                node: dst,
                kind: cause.clone(),
                target,
                time: node.time,
                confidence: node.confidence,
            };
            // RollsUp{ child → parent } provenance edge (`09` §3.2,
            // ADR-003); deduped so repeat roll-ups from the same child add
            // one edge.
            if let Some(src_node) = child_node {
                let edge = EvidenceEdge {
                    kind: EvidenceEdgeKind::RollsUp,
                    src: EdgeEndpoint::Node(src_node),
                    dst,
                };
                if !part.edges.contains(&edge) {
                    part.edges.push(edge);
                }
            }
            if created {
                self.next_node_id = candidate_node_id;
            }
        }
        self.on_confidence_mutation(&mutation, parent_scope, budget);
    }

    /// `ConfidenceMutation(K, T)` → decision re-evaluation (`03` §3.5) +
    /// ambiguity synthesis (`03` §4).
    fn on_confidence_mutation(
        &mut self,
        snapshot: &CauseSnapshot,
        scope: ScopeId,
        budget: &mut usize,
    ) {
        let img = self.image;
        let rules: Vec<&'img RuleInstance> = img
            .rules_for(
                AnchorKey::Cause(&snapshot.kind),
                scope.scope_type(),
                RuleKind::Decision,
            )
            .collect();
        for rule in rules {
            let bindings = [Binding::Bound(Bound::Cause(snapshot.clone()))];
            match self.eval_pred(rule, &rule.anchor.predicate, &bindings, scope) {
                Some(t) if t.is_true() => {
                    self.run_rule(rule, Bound::Cause(snapshot.clone()), scope, budget);
                }
                _ => {}
            }
        }
        self.synthesize_ambiguity(snapshot, scope);
    }

    /// Ambiguity synthesis over `mutually_exclusive` groups (`03` §4).
    /// `common_target` = the mutated cause's target.
    fn synthesize_ambiguity(&mut self, snapshot: &CauseSnapshot, scope: ScopeId) {
        let img = self.image;
        let target = snapshot.target;
        let wm = self.store.watermark();
        for group in effective_exclusivity_groups(img) {
            if !group.causes.contains(&snapshot.kind) {
                continue;
            }
            for other in group.causes.iter().filter(|k| **k != snapshot.kind) {
                let pair = if snapshot.kind <= *other {
                    (snapshot.kind.clone(), other.clone())
                } else {
                    (other.clone(), snapshot.kind.clone())
                };
                let c1 = snapshot.confidence;
                let c2 = {
                    let part = self.store.partition(scope);
                    part.and_then(|p| p.causes.get(&(other.clone(), target)).map(|n| n.confidence))
                        .unwrap_or(Confidence::MIN)
                };
                let close = c1.value().abs_diff(c2.value()) < 15;
                let both_probable = c1.is_probable() && c2.is_probable();
                if both_probable && close {
                    let amb_node_id = self.next_node_id + 1;
                    let problem_node_id = amb_node_id + 1;
                    let created = {
                        let mut part = self.store.partition_mut(scope);
                        let other_node = part.causes.get(&(other.clone(), target)).map(|n| n.id);
                        match part.ambiguities.entry((pair.clone(), target)) {
                            std::collections::hash_map::Entry::Occupied(_) => false,
                            std::collections::hash_map::Entry::Vacant(slot) => {
                                slot.insert(AmbiguityNode {
                                    id: NodeId::new(amb_node_id),
                                    causes: pair.clone(),
                                    target,
                                    state: AmbiguityState::Active,
                                });
                                let (sarif_id, severity) = resolve_problem("AmbiguousDiagnosis")
                                    .map(|schema| {
                                        (
                                            schema.default_sarif_id.clone(),
                                            schema.severity.unwrap_or(Severity::Medium),
                                        )
                                    })
                                    .unwrap_or_else(|| {
                                        (SarifId::new("ap_ambiguous"), Severity::Medium)
                                    });
                                let mut evidence = vec![snapshot.node];
                                if let Some(node) = other_node.filter(|n| !evidence.contains(n)) {
                                    evidence.push(node);
                                }
                                part.push_problem(ProblemNode {
                                    id: NodeId::new(problem_node_id),
                                    kind: ProblemKind::new("AmbiguousDiagnosis"),
                                    target,
                                    time: wm,
                                    severity,
                                    evidence,
                                    sarif_id,
                                    superseded: false,
                                });
                                true
                            }
                        }
                    };
                    if created {
                        self.next_node_id = problem_node_id;
                        // Surface via mark_ambiguous action (03 §4).
                        let intent = ActionIntent {
                            kind: ActionKind::MarkAmbiguous,
                            // Synthesis is ruleset-level, not rule-level
                            // (03 §4) — attribute to the ruleset id.
                            rule: RuleId::new(img.ruleset_id.as_ref()),
                            arg: None,
                            target: Some(target),
                            target_path: None,
                            reason: None,
                            evidence: Vec::new(),
                            causes: Some(pair),
                        };
                        self.sink.emit(intent, self.mode, wm);
                    }
                } else {
                    // Resolution: one confirmed or clearly separated (03 §4).
                    let resolvable = c1.max(c2).is_confirmed() || !close;
                    if resolvable {
                        let mut part = self.store.partition_mut(scope);
                        if let Some(a) = part.ambiguities.get_mut(&(pair, target)) {
                            if a.state == AmbiguityState::Active {
                                a.state = AmbiguityState::Resolved;
                            }
                        }
                    }
                }
            }
        }
    }

    /// `emit Problem(P)` (`03` §3.4): cooldown dedup (F3), append-only
    /// emission with `time = WM`, then `ProblemEmission` re-evaluation
    /// (`03` §3.5, Example 8).
    #[allow(clippy::too_many_arguments)]
    fn exec_emit_problem(
        &mut self,
        rule: &'img RuleInstance,
        problem: &ProblemKind,
        target_path: &Option<MetricPath>,
        severity: Severity,
        evidence: &[Symbol],
        sarif_id: &SarifId,
        bindings: &[Binding],
        scope: ScopeId,
        budget: &mut usize,
    ) {
        let target = match target_path {
            Some(path) => match self.resolve_target(rule, path, bindings) {
                Some(t) => t,
                None => {
                    self.diagnostics.push(EngineDiagnostic::UnresolvedTarget {
                        rule: rule.id.clone(),
                        path: path.clone(),
                    });
                    return;
                }
            },
            // Omitted target = the rule scope (03 §3.4).
            None => scope,
        };
        let wm = self.store.watermark();
        // Node-evidence only (04 §3 ProblemNode.evidence: List<NodeId>);
        // event evidence is carried by evidence edges instead.
        let evidence_nodes: Vec<NodeId> = evidence
            .iter()
            .filter_map(|sym| {
                let idx = binding_index_of(rule, sym.as_str())?;
                match bindings.get(idx)?.bound()? {
                    Bound::Cause(c) => Some(c.node),
                    Bound::Problem(p) => Some(p.node),
                    Bound::Event(_) => None,
                }
            })
            .collect();
        let cooldown = self.problem_cooldown;
        let node_id = self.next_node_id + 1;
        let snapshot: ProblemSnapshot;
        {
            let mut part = self.store.partition_mut(scope);
            if !part.try_mark_emitted(&rule.id, problem, target, wm, cooldown) {
                return; // cooldown no-op (F3)
            }
            part.push_problem(ProblemNode {
                id: NodeId::new(node_id),
                kind: problem.clone(),
                target,
                time: wm, // emission watermark (03 §3.4)
                severity,
                evidence: evidence_nodes,
                sarif_id: sarif_id.clone(),
                superseded: false,
            });
            snapshot = ProblemSnapshot {
                node: NodeId::new(node_id),
                kind: problem.clone(),
                target,
                time: wm,
            };
        }
        self.next_node_id = node_id;
        self.on_problem_emission(&snapshot, scope, budget);
    }

    /// `ProblemEmission(P, T)` → Problem-anchored decision re-evaluation
    /// (`03` §3.5, Example 8 suppression).
    fn on_problem_emission(
        &mut self,
        snapshot: &ProblemSnapshot,
        scope: ScopeId,
        budget: &mut usize,
    ) {
        let img = self.image;
        let rules: Vec<&'img RuleInstance> = img
            .rules_for(
                AnchorKey::Problem(&snapshot.kind),
                scope.scope_type(),
                RuleKind::Decision,
            )
            .collect();
        for rule in rules {
            let bindings = [Binding::Bound(Bound::Problem(snapshot.clone()))];
            match self.eval_pred(rule, &rule.anchor.predicate, &bindings, scope) {
                Some(t) if t.is_true() => {
                    self.run_rule(rule, Bound::Problem(snapshot.clone()), scope, budget);
                }
                _ => {}
            }
        }
    }

    /// `action <kind>(...)` (`03` §3.6): resolve, forward to the sink — the
    /// sink decides the effect per [`RunMode`] (`07` §7).
    #[allow(clippy::too_many_arguments)]
    fn exec_action(
        &mut self,
        rule: &RuleInstance,
        kind: ActionKind,
        arg: &Option<Symbol>,
        target_path: &Option<MetricPath>,
        reason: &Option<Box<str>>,
        evidence: &[Symbol],
        bindings: &[Binding],
    ) {
        let target = target_path
            .as_ref()
            .and_then(|p| self.resolve_target(rule, p, bindings));
        let intent = ActionIntent {
            kind,
            rule: rule.id.clone(),
            arg: arg.clone(),
            target,
            target_path: target_path.clone(),
            reason: reason.clone(),
            evidence: self.evidence_endpoints(rule, evidence, bindings),
            causes: None,
        };
        let wm = self.store.watermark();
        self.sink.emit(intent, self.mode, wm);
    }

    /// Lowered `suppress_symptom` (`05` §1.1 → `SupersedeProblem`): the
    /// *engine* performs the graph mutation (mark superseded — C7,
    /// append-only); the sink never mutates the graph (`07` §7). The audit
    /// record is the responsibility of the lowering, which pairs the
    /// `SupersedeProblem` with an `EmitAction(SuppressSymptom)` carrying the
    /// rule's `reason` (see `fixtures::suppress_downstream_rule`) — the
    /// intent itself has no reason field to audit.
    fn exec_supersede(
        &mut self,
        rule: &RuleInstance,
        problem: &ProblemKind,
        target_path: &MetricPath,
        bindings: &[Binding],
        scope: ScopeId,
    ) {
        let Some(target) = self.resolve_target(rule, target_path, bindings) else {
            self.diagnostics.push(EngineDiagnostic::UnresolvedTarget {
                rule: rule.id.clone(),
                path: target_path.clone(),
            });
            return;
        };
        let mut part = self.store.partition_mut(scope);
        let _marked = part.supersede_problems(problem, target);
    }

    /// Explicit `mark_ambiguous` intent (distinct from the automatic
    /// synthesis of `03` §4, which the engine drives on
    /// `ConfidenceMutation`).
    fn exec_mark_ambiguous(
        &mut self,
        rule: &RuleInstance,
        causes: &(CauseKind, CauseKind),
        target_path: &MetricPath,
        bindings: &[Binding],
        scope: ScopeId,
    ) {
        let Some(target) = self.resolve_target(rule, target_path, bindings) else {
            self.diagnostics.push(EngineDiagnostic::UnresolvedTarget {
                rule: rule.id.clone(),
                path: target_path.clone(),
            });
            return;
        };
        let pair = if causes.0 <= causes.1 {
            causes.clone()
        } else {
            (causes.1.clone(), causes.0.clone())
        };
        let node_id = self.next_node_id + 1;
        let created = {
            let mut part = self.store.partition_mut(scope);
            match part.ambiguities.entry((pair.clone(), target)) {
                std::collections::hash_map::Entry::Occupied(_) => false,
                std::collections::hash_map::Entry::Vacant(slot) => {
                    slot.insert(AmbiguityNode {
                        id: NodeId::new(node_id),
                        causes: pair.clone(),
                        target,
                        state: AmbiguityState::Active,
                    });
                    true
                }
            }
        };
        if created {
            self.next_node_id = node_id;
        }
        let intent = ActionIntent {
            kind: ActionKind::MarkAmbiguous,
            rule: rule.id.clone(),
            arg: None,
            target: Some(target),
            target_path: Some(target_path.clone()),
            reason: None,
            evidence: Vec::new(),
            causes: Some(pair),
        };
        let wm = self.store.watermark();
        self.sink.emit(intent, self.mode, wm);
    }
}

impl<T: TopologyProvider> Engine<'_, T, OfflineAuditSink> {
    /// Full deterministic snapshot including the sorted audit log
    /// (ADR-012 merge ordering; see [`Snapshot`] docs).
    #[must_use]
    pub fn snapshot(&self) -> Snapshot {
        let mut snap = self.graph_snapshot();
        let mut audit: Vec<_> = self.sink().entries().to_vec();
        audit.sort_by(|a, b| {
            (a.wm, a.intent.rule.as_str(), a.intent.kind, a.intent.target).cmp(&(
                b.wm,
                b.intent.rule.as_str(),
                b.intent.kind,
                b.intent.target,
            ))
        });
        snap.audit = audit;
        snap
    }

    /// Emits SARIF for the current graph state, honoring [`Engine::strict_privacy`].
    #[must_use]
    pub fn sarif(&self) -> String {
        let options = SarifOptions {
            strict_privacy: self.strict_privacy,
        };
        to_sarif_with_options(&self.snapshot(), options)
    }
}

/// Binding-name → binding-index resolution (`06` §2.1: anchor is binding 0,
/// correlates follow in declaration order).
fn binding_index_of(rule: &RuleInstance, name: &str) -> Option<usize> {
    if rule.anchor.binding.as_str() == name {
        return Some(0);
    }
    rule.correlates
        .iter()
        .position(|c| c.binding.as_str() == name)
        .map(|i| i + 1)
}

/// Ruleset exclusivity merged with catalog defaults (`10` §7, ADR-005).
fn effective_exclusivity_groups(image: &ProgramImage) -> Vec<ExclusivityGroup> {
    let mut groups: Vec<ExclusivityGroup> = image.exclusivity.to_vec();
    for pair in exclusivity_defaults() {
        let catalog_group = ExclusivityGroup {
            causes: Box::new([pair.left.clone(), pair.right.clone()]),
        };
        let duplicate = groups.iter().any(|group| {
            group.causes.len() == 2
                && ((group.causes[0] == pair.left && group.causes[1] == pair.right)
                    || (group.causes[0] == pair.right && group.causes[1] == pair.left))
        });
        if !duplicate {
            groups.push(catalog_group);
        }
    }
    groups
}

/// Deterministic candidate ordering: earliest time first, node id
/// tie-break (ids ascend with creation order — ADR-012).
fn bound_sort_key(b: &Bound) -> (EventTime, u64) {
    match b {
        Bound::Event(e) => (e.time, e.id.raw()),
        Bound::Cause(c) => (c.time, c.node.raw()),
        Bound::Problem(p) => (p.time, p.node.raw()),
    }
}
