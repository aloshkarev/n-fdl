//! Stub `Problem(DeviceUnreachable)` emitter for the Example 07 golden tests
//! (G05, `12-testing.md` §3.4).
//!
//! `07-suppress-downstream.adgl` notes: "Problem(DeviceUnreachable) is
//! emitted by a separate verdict rule (not shown here)" and "in v1 the
//! emitter must live in the same ruleset". Per the Phase 1 plan decision
//! ("stubs for 07/08"), this module hand-codes that emitter as Rust
//! constructor code: a synthetic `net.device_unreachable` event drives an
//! evidence rule (`infer Cause(DeviceDown) +100`), whose decision verdict
//! emits `Problem(DeviceUnreachable)` at the event's target — which is what
//! `suppress_downstream` reacts to (`03` §3.5 ProblemEmission).

use airpulse_dsl_evaluator::schema::CAUSE_FIELD_CONFIDENCE;
use airpulse_dsl_ir::{
    AnchorSource, AnchorSpec, BindingIdx, Intent, PredOp, Predicate, ProvKey, RuleInstance,
    RuleKind, SlotIdx, Symbol, VerifiedAnnotations,
};
use airpulse_dsl_types::{
    CauseKind, EventType, MetricPath, ProblemKind, RuleId, SarifId, ScopeType, Severity, Weight,
};

/// The synthetic event type the stub evidence rule anchors on.
pub const STUB_EVENT_TYPE: &str = "net.device_unreachable";

fn slot(i: u8) -> SlotIdx {
    SlotIdx::new(i).expect("stub slot within MAX_SLOTS")
}

/// Evidence: `anchor ev: event(net.device_unreachable)` →
/// `infer Cause(DeviceDown) { target: ev.target, weight: +100 }`.
fn device_unreachable_evidence() -> RuleInstance {
    RuleInstance {
        id: RuleId::new("stub_device_unreachable_evidence"),
        kind: RuleKind::Evidence,
        scope: ScopeType::Global,
        anchor: AnchorSpec {
            binding: Symbol::new("ev"),
            source: AnchorSource::Event(EventType::new(STUB_EVENT_TYPE)),
            predicate: Predicate::always_true(),
        },
        correlates: Box::new([]),
        branches: None,
        body: Box::new([Intent::InferCause {
            cause: CauseKind::new("DeviceDown"),
            target: MetricPath::new("ev.target"),
            weight: Weight::new(100).expect("weight in domain"),
            evidence: Box::new([Symbol::new("ev")]),
            provenance_key: ProvKey {
                rule: RuleId::new("stub_device_unreachable_evidence"),
                cause: CauseKind::new("DeviceDown"),
                target_expr_hash: 0x5f0d_11e5,
            },
            evidence_pii: Box::new([]),
        }]),
        annotations: VerifiedAnnotations::from_correlates(&[], None),
    }
}

/// Decision: `anchor c: Cause(DeviceDown) { c.confidence >= 80 }` →
/// `emit Problem(DeviceUnreachable) { target: c.target, severity: High }`.
fn device_unreachable_verdict() -> RuleInstance {
    RuleInstance {
        id: RuleId::new("stub_device_unreachable_verdict"),
        kind: RuleKind::Decision,
        scope: ScopeType::Global,
        anchor: AnchorSpec {
            binding: Symbol::new("c"),
            source: AnchorSource::Cause(CauseKind::new("DeviceDown")),
            predicate: Predicate {
                ops: Box::new([
                    PredOp::LoadCauseField {
                        binding: BindingIdx(0),
                        field: CAUSE_FIELD_CONFIDENCE,
                        dst: slot(0),
                    },
                    PredOp::LoadConst {
                        imm: 80,
                        dst: slot(1),
                    },
                    PredOp::CmpGe {
                        lhs: slot(0),
                        rhs: slot(1),
                        dst: slot(2),
                    },
                ]),
                result: slot(2),
            },
        },
        correlates: Box::new([]),
        branches: None,
        body: Box::new([Intent::EmitProblem {
            problem: ProblemKind::new("DeviceUnreachable"),
            target: Some(MetricPath::new("c.target")),
            severity: Severity::High,
            evidence: Box::new([Symbol::new("c")]),
            sarif_id: SarifId::new("ap_device_unreachable"),
            pii: Box::new([]),
        }]),
        annotations: VerifiedAnnotations::from_correlates(&[], None),
    }
}

/// The stub emitter rules, in declaration order (evidence before verdict —
/// C12 ordered firing).
pub fn device_unreachable_stub_rules() -> Vec<RuleInstance> {
    vec![device_unreachable_evidence(), device_unreachable_verdict()]
}
