//! Hand-coded `ProgramImage` fixtures used as ground-truth evaluator tests.
//!
//! - [`rule3_pmtud`] — `docs/idea/examples/01-pmtud-blackhole.adgl`
//!   (`pmtud_hypothesis` evidence + `pmtud_verdict` decision);
//! - [`rule8_suppression`] — `docs/idea/examples/07-suppress-downstream.adgl`
//!   (`suppress_downstream` decision), composed with caller-supplied
//!   `DeviceUnreachable` emitter rules (the example's emitter is "not shown";
//!   golden tests provide a stub — `tests/golden/_stubs/`).
//!
//! Parser + verifier now generate equivalent `ProgramImage` values (Phase 2),
//! and these fixtures remain as explicit runtime baselines.

use airpulse_dsl_ir::{
    AnchorSource, AnchorSpec, BindingIdx, BranchTable, CatalogRef, CorrelateSource, CorrelateSpec,
    FieldIdx, Intent, PredOp, Predicate, ProgramImage, ProvKey, RuleInstance, RuleKind, SlotIdx,
    Symbol, TopoCall, TopoFuncIdx, VerifiedAnnotations, WindowProof,
};
use airpulse_dsl_types::{
    ActionKind, CauseKind, DurationMs, EventType, MetricPath, ProblemKind, RuleId, SarifId,
    ScopeType, Severity, Weight,
};

/// Fixture field index of `tcp.retransmission_burst.segment_size`.
/// This mirrors the catalog mapping for event metrics.
pub const F_SEGMENT_SIZE: FieldIdx = FieldIdx(0);

fn slot(i: u8) -> SlotIdx {
    // Fixture-only constructor: indices are compile-time constants.
    SlotIdx::new(i)
        .expect("fixture authoring error: slot index must be within airpulse_dsl_ir::MAX_SLOTS")
}

fn dur(ms: i64) -> DurationMs {
    DurationMs::from_millis(ms)
        .expect("fixture authoring error: duration literal must be non-negative")
}

fn weight(v: i8) -> Weight {
    Weight::new(v).expect("fixture authoring error: weight literal must be in [-100, 100]")
}

fn catalog_ref() -> CatalogRef {
    CatalogRef { id: "airpulse.catalog".into(), version: "1.0".into() }
}

/// `rtx.segment_size > 1400` (Example 01 anchor predicate; `06` §4.1
/// lowering).
fn segment_size_gt_1400() -> Predicate {
    Predicate {
        ops: Box::new([
            PredOp::LoadEventField { binding: BindingIdx(0), field: F_SEGMENT_SIZE, dst: slot(0) },
            PredOp::LoadConst { imm: 1400, dst: slot(1) },
            PredOp::CmpGt { lhs: slot(0), rhs: slot(1), dst: slot(2) },
        ]),
        result: slot(2),
    }
}

/// `c.confidence >= 80` decision anchor predicate.
fn confidence_ge_80() -> Predicate {
    Predicate {
        ops: Box::new([
            PredOp::LoadCauseField {
                binding: BindingIdx(0),
                field: crate::schema::CAUSE_FIELD_CONFIDENCE,
                dst: slot(0),
            },
            PredOp::LoadConst { imm: 80, dst: slot(1) },
            PredOp::CmpGe { lhs: slot(0), rhs: slot(1), dst: slot(2) },
        ]),
        result: slot(2),
    }
}

/// The auto-generated Unknown branch body (`06` §3.1, C10).
fn request_topology_body() -> Box<[Intent]> {
    Box::new([Intent::EmitAction {
        kind: ActionKind::RequestTopology,
        arg: None,
        target: None,
        reason: None,
        evidence: Box::new([]),
    }])
}

/// Example 01 `pmtud_hypothesis` — evidence rule:
///
/// ```adgl
/// anchor rtx: event(tcp.retransmission_burst) { rtx.segment_size > 1400 }
/// correlate ptb: event(icmp.ptb) {
///     topo: same_session(rtx.target, ptb.target)
///     time: ptb.time in [rtx.time - 500ms, rtx.time + 1s]
/// }
/// if present(ptb) { infer +85 [rtx, ptb] }
/// else { infer +35 [rtx]; action request_observation(icmp.visibility) { target: rtx.path } }
/// ```
#[must_use]
pub fn pmtud_hypothesis_rule() -> RuleInstance {
    let correlates: Box<[CorrelateSpec]> = Box::new([CorrelateSpec {
        binding: Symbol::new("ptb"),
        source: CorrelateSource::Event(EventType::new("icmp.ptb")),
        topo: TopoCall {
            func: "same_session".into(),
            func_idx: TopoFuncIdx(0),
            args: Box::new([MetricPath::new("rtx.target"), MetricPath::new("ptb.target")]),
        },
        window: WindowProof::Calculable { back: dur(500), forward: dur(1000) },
    }]);
    let prov = ProvKey {
        rule: RuleId::new("pmtud_hypothesis"),
        cause: CauseKind::new("PmtudBlackhole"),
        target_expr_hash: 0x9e37_79b9,
    };
    let branches = BranchTable {
        cond: Predicate {
            ops: Box::new([PredOp::Present { binding: BindingIdx(1), dst: slot(0) }]),
            result: slot(0),
        },
        then_body: Box::new([Intent::InferCause {
            cause: CauseKind::new("PmtudBlackhole"),
            target: MetricPath::new("rtx.target"),
            weight: weight(85),
            evidence: Box::new([Symbol::new("rtx"), Symbol::new("ptb")]),
            provenance_key: prov.clone(),
            evidence_pii: Box::new([]),
        }]),
        else_body: Some(Box::new([
            Intent::InferCause {
                cause: CauseKind::new("PmtudBlackhole"),
                target: MetricPath::new("rtx.target"),
                weight: weight(35),
                evidence: Box::new([Symbol::new("rtx")]),
                provenance_key: prov,
                evidence_pii: Box::new([]),
            },
            Intent::EmitAction {
                kind: ActionKind::RequestObservation,
                arg: Some(Symbol::new("icmp.visibility")),
                target: Some(MetricPath::new("rtx.path")),
                reason: None,
                evidence: Box::new([]),
            },
        ])),
        unknown_body: request_topology_body(),
    };
    let annotations = VerifiedAnnotations::from_correlates(&correlates, Some(ScopeType::Session));
    RuleInstance {
        id: RuleId::new("pmtud_hypothesis"),
        kind: RuleKind::Evidence,
        scope: ScopeType::Session,
        anchor: AnchorSpec {
            binding: Symbol::new("rtx"),
            source: AnchorSource::Event(EventType::new("tcp.retransmission_burst")),
            predicate: segment_size_gt_1400(),
        },
        correlates,
        branches: Some(branches),
        body: Box::new([]),
        annotations,
    }
}

/// Example 01 `pmtud_verdict` — decision rule:
///
/// ```adgl
/// anchor c: Cause(PmtudBlackhole) { c.confidence >= 80 }
/// emit Problem(XlIcmpTcpMss) { severity: High, evidence: [c],
///                              sarif_id: "l3_pmtud_blackhole" }
/// ```
#[must_use]
pub fn pmtud_verdict_rule() -> RuleInstance {
    RuleInstance {
        id: RuleId::new("pmtud_verdict"),
        kind: RuleKind::Decision,
        scope: ScopeType::Session,
        anchor: AnchorSpec {
            binding: Symbol::new("c"),
            source: AnchorSource::Cause(CauseKind::new("PmtudBlackhole")),
            predicate: confidence_ge_80(),
        },
        correlates: Box::new([]),
        branches: None,
        body: Box::new([Intent::EmitProblem {
            problem: ProblemKind::new("XlIcmpTcpMss"),
            target: None, // omitted target = rule scope (03 §3.4)
            severity: Severity::High,
            evidence: Box::new([Symbol::new("c")]),
            sarif_id: SarifId::new("l3_pmtud_blackhole"),
            pii: Box::new([]),
        }]),
        annotations: VerifiedAnnotations::from_correlates(&[], None),
    }
}

/// Example 01 ruleset `airpulse.tcp_diagnostics` (Rule 3), incl. its
/// `mutually_exclusive(PmtudBlackhole, Congestion, TransientL2Disruption)`
/// group.
#[must_use]
pub fn rule3_pmtud() -> ProgramImage {
    ProgramImage::new(
        ProgramImage::pack_version(1, 0, 0),
        "airpulse.tcp_diagnostics",
        Box::new(["l3-deep".into(), "topology".into()]),
        Box::new([airpulse_dsl_ir::ExclusivityGroup {
            causes: Box::new([
                CauseKind::new("PmtudBlackhole"),
                CauseKind::new("Congestion"),
                CauseKind::new("TransientL2Disruption"),
            ]),
        }]),
        Box::new([pmtud_hypothesis_rule(), pmtud_verdict_rule()]),
        catalog_ref(),
    )
}

/// Example 07 `suppress_downstream` — decision rule:
///
/// ```adgl
/// anchor downstream: Problem(DeviceUnreachable)
/// correlate upstream: Problem(DeviceUnreachable) {
///     topo: upstream_of(upstream.target, downstream.target)
///     time: upstream.time in [downstream.time - 30s, downstream.time + 5s]
/// }
/// if present(upstream) { action suppress_symptom(downstream) { ... } }
/// ```
///
/// `suppress_symptom(downstream)` lowers to `SupersedeProblem`
/// (`05-verification.md` §1.1); the reason string is preserved in a
/// companion audit action.
#[must_use]
pub fn suppress_downstream_rule() -> RuleInstance {
    let correlates: Box<[CorrelateSpec]> = Box::new([CorrelateSpec {
        binding: Symbol::new("upstream"),
        source: CorrelateSource::Problem(ProblemKind::new("DeviceUnreachable")),
        topo: TopoCall {
            func: "upstream_of".into(),
            func_idx: TopoFuncIdx(5),
            args: Box::new([
                MetricPath::new("upstream.target"),
                MetricPath::new("downstream.target"),
            ]),
        },
        window: WindowProof::Calculable { back: dur(30_000), forward: dur(5_000) },
    }]);
    RuleInstance {
        id: RuleId::new("suppress_downstream"),
        kind: RuleKind::Decision,
        scope: ScopeType::Global,
        anchor: AnchorSpec {
            binding: Symbol::new("downstream"),
            source: AnchorSource::Problem(ProblemKind::new("DeviceUnreachable")),
            predicate: Predicate::always_true(),
        },
        correlates: correlates.clone(),
        branches: Some(BranchTable {
            cond: Predicate {
                ops: Box::new([PredOp::Present { binding: BindingIdx(1), dst: slot(0) }]),
                result: slot(0),
            },
            then_body: Box::new([
                Intent::SupersedeProblem {
                    problem: ProblemKind::new("DeviceUnreachable"),
                    target: MetricPath::new("downstream.target"),
                },
                Intent::EmitAction {
                    kind: ActionKind::SuppressSymptom,
                    arg: Some(Symbol::new("downstream")),
                    target: Some(MetricPath::new("downstream.target")),
                    reason: Some("Masked by upstream topology failure".into()),
                    evidence: Box::new([Symbol::new("upstream")]),
                },
            ]),
            else_body: None,
            unknown_body: request_topology_body(),
        }),
        body: Box::new([]),
        annotations: VerifiedAnnotations::from_correlates(&correlates, None),
    }
}

/// Example 07 ruleset `airpulse.topo_suppression` (Rule 8).
///
/// The `Problem(DeviceUnreachable)` emitter the rule reacts to is "a
/// separate verdict rule (not shown)" in the example, and in v1 it must
/// live in the same ruleset (single `ProgramImage` per file, `02` §10) —
/// pass stub emitter rules via `emitter_rules` (golden tests keep them in
/// `tests/golden/_stubs/`). Emitters precede the suppressor in declaration
/// order (C12 ordered firing).
#[must_use]
pub fn rule8_suppression(emitter_rules: impl IntoIterator<Item = RuleInstance>) -> ProgramImage {
    let mut rules: Vec<RuleInstance> = emitter_rules.into_iter().collect();
    rules.push(suppress_downstream_rule());
    ProgramImage::new(
        ProgramImage::pack_version(1, 0, 0),
        "airpulse.topo_suppression",
        Box::new(["topology".into()]),
        Box::new([]),
        rules.into_boxed_slice(),
        catalog_ref(),
    )
}
