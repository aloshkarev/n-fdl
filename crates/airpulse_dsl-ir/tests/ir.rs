//! IR construction/introspection tests: `rules_for` routing (07 §5),
//! predicate opcode round-trip (06 §4), and hand-coded shapes of Example 01
//! Rule 3 (PMTUD blackhole) and Example 07/08 (suppress downstream) — the
//! Phase 1 targets. Construction and introspection only; execution is the
//! evaluator crate's job (07 §1).

use airpulse_dsl_ir::{
    AnchorKey, AnchorSource, AnchorSpec, BindingIdx, BranchTable, CatalogRef, CorrelateSource,
    CorrelateSpec, FieldIdx, Intent, PredOp, Predicate, ProgramImage, ProvKey, RuleInstance,
    RuleKind, SlotIdx, Symbol, TopoCall, TopoFuncIdx, VerifiedAnnotations, WindowProof,
};
use airpulse_dsl_types::{
    ActionKind, CauseKind, DurationMs, EventType, MetricPath, ProblemKind, RuleId, SarifId,
    ScopeType, Severity, Weight,
};

fn slot(i: u8) -> SlotIdx {
    SlotIdx::new(i).expect("test slot within MAX_SLOTS")
}

fn dur(ms: i64) -> DurationMs {
    DurationMs::from_millis(ms).expect("non-negative test duration")
}

fn catalog_ref() -> CatalogRef {
    CatalogRef {
        id: "airpulse.catalog".into(),
        version: "1.0".into(),
    }
}

/// `rtx.segment_size > 1400` (Example 01 anchor predicate; 06 §4.1 lowering).
fn segment_size_gt_1400() -> Predicate {
    Predicate {
        ops: Box::new([
            PredOp::LoadEventField {
                binding: BindingIdx(0),
                field: FieldIdx(0),
                dst: slot(0),
            },
            PredOp::LoadConst {
                imm: 1400,
                dst: slot(1),
            },
            PredOp::CmpGt {
                lhs: slot(0),
                rhs: slot(1),
                dst: slot(2),
            },
        ]),
        result: slot(2),
    }
}

/// Example 01 Rule 3 `pmtud_hypothesis` — evidence rule: event anchor with
/// metric predicate, one correlate with backward+forward window, present/else
/// branch with infer-cause intents.
fn pmtud_hypothesis() -> RuleInstance {
    let correlates: Box<[CorrelateSpec]> = Box::new([CorrelateSpec {
        binding: Symbol::new("ptb"),
        source: CorrelateSource::Event(EventType::new("icmp.ptb")),
        topo: TopoCall {
            func: "same_session".into(),
            func_idx: TopoFuncIdx(0),
            args: Box::new([MetricPath::new("rtx.target"), MetricPath::new("ptb.target")]),
        },
        // time: ptb.time in [rtx.time - 500ms, rtx.time + 1s] (05 §11 Calculable)
        window: WindowProof::Calculable {
            back: dur(500),
            forward: dur(1000),
        },
        min_match: 1,
    }]);
    let prov = ProvKey {
        rule: RuleId::new("pmtud_hypothesis"),
        cause: CauseKind::new("PmtudBlackhole"),
        target_expr_hash: 0x9e37_79b9,
    };
    let branches = BranchTable {
        // if present(ptb)
        cond: Predicate {
            ops: Box::new([PredOp::Present {
                binding: BindingIdx(1),
                dst: slot(0),
            }]),
            result: slot(0),
        },
        then_body: Box::new([Intent::InferCause {
            cause: CauseKind::new("PmtudBlackhole"),
            target: MetricPath::new("rtx.target"),
            weight: Weight::new(85).expect("weight in domain"),
            evidence: Box::new([Symbol::new("rtx"), Symbol::new("ptb")]),
            provenance_key: prov.clone(),
            evidence_pii: Box::new([]),
        }]),
        else_body: Some(Box::new([
            Intent::InferCause {
                cause: CauseKind::new("PmtudBlackhole"),
                target: MetricPath::new("rtx.target"),
                weight: Weight::new(35).expect("weight in domain"),
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
        // C10 auto-generated Unknown branch (06 §3.1).
        unknown_body: Box::new([Intent::EmitAction {
            kind: ActionKind::RequestTopology,
            arg: None,
            target: None,
            reason: None,
            evidence: Box::new([]),
        }]),
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

/// Example 01 `pmtud_verdict` — decision rule: Cause anchor with
/// `c.confidence >= 80`, unconditional emit-Problem body.
fn pmtud_verdict() -> RuleInstance {
    RuleInstance {
        id: RuleId::new("pmtud_verdict"),
        kind: RuleKind::Decision,
        scope: ScopeType::Session,
        anchor: AnchorSpec {
            binding: Symbol::new("c"),
            source: AnchorSource::Cause(CauseKind::new("PmtudBlackhole")),
            predicate: Predicate {
                ops: Box::new([
                    PredOp::LoadCauseField {
                        binding: BindingIdx(0),
                        field: FieldIdx(0),
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
            problem: ProblemKind::new("XlIcmpTcpMss"),
            target: None,
            severity: Severity::High,
            evidence: Box::new([Symbol::new("c")]),
            sarif_id: SarifId::new("l3_pmtud_blackhole"),
            pii: Box::new([]),
        }]),
        annotations: VerifiedAnnotations::from_correlates(&[], None),
    }
}

fn tcp_diagnostics_image() -> ProgramImage {
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
        Box::new([pmtud_hypothesis(), pmtud_verdict()]),
        catalog_ref(),
    )
}

#[test]
fn rules_for_routes_by_anchor_and_class() {
    let img = tcp_diagnostics_image();
    assert_eq!(img.magic, *b"ADGL"); // 06 §2 magic
    assert_eq!(img.version, (1 << 16));

    // Evidence lookup by event type (07 §5 img.rules_for(evt.type, sg, Evidence)).
    let evt = EventType::new("tcp.retransmission_burst");
    let hits: Vec<_> = img
        .rules_for(
            AnchorKey::Event(&evt),
            ScopeType::Session,
            RuleKind::Evidence,
        )
        .map(|r| r.id.as_str())
        .collect();
    assert_eq!(hits, ["pmtud_hypothesis"]);

    // Decision lookup by cause kind (03 §3.5 ConfidenceMutation re-eval).
    let cause = CauseKind::new("PmtudBlackhole");
    let hits: Vec<_> = img
        .rules_for(
            AnchorKey::Cause(&cause),
            ScopeType::Session,
            RuleKind::Decision,
        )
        .map(|r| r.id.as_str())
        .collect();
    assert_eq!(hits, ["pmtud_verdict"]);

    // Wrong class, wrong scope, unknown anchor — all route to nothing.
    assert_eq!(
        img.rules_for(
            AnchorKey::Event(&evt),
            ScopeType::Session,
            RuleKind::Decision
        )
        .count(),
        0
    );
    assert_eq!(
        img.rules_for(
            AnchorKey::Event(&evt),
            ScopeType::Global,
            RuleKind::Evidence
        )
        .count(),
        0
    );
    let other = EventType::new("icmp.ptb");
    assert_eq!(
        img.rules_for(
            AnchorKey::Event(&other),
            ScopeType::Session,
            RuleKind::Evidence
        )
        .count(),
        0
    );
}

#[test]
fn predicate_opcode_round_trip() {
    // Construct the 06 §4.1 anchor-predicate fragment and inspect it op-by-op.
    let p = segment_size_gt_1400();
    assert_eq!(p.ops.len(), 3);
    assert_eq!(
        p.ops[0],
        PredOp::LoadEventField {
            binding: BindingIdx(0),
            field: FieldIdx(0),
            dst: slot(0)
        }
    );
    assert_eq!(
        p.ops[1],
        PredOp::LoadConst {
            imm: 1400,
            dst: slot(1)
        }
    );
    assert_eq!(
        p.ops[2],
        PredOp::CmpGt {
            lhs: slot(0),
            rhs: slot(1),
            dst: slot(2)
        }
    );
    assert_eq!(p.result, slot(2));
    // Value semantics: an identical reconstruction compares equal.
    assert_eq!(p, segment_size_gt_1400());
    assert_ne!(p, Predicate::always_true());

    // Window opcodes (06 §4 WIN group) shape-check.
    let win = Predicate {
        ops: Box::new([
            PredOp::LoadEventField {
                binding: BindingIdx(0),
                field: FieldIdx(3),
                dst: slot(0),
            },
            PredOp::LoadDuration {
                dur: dur(500),
                dst: slot(1),
            },
            PredOp::WinBack {
                time: slot(0),
                dur: slot(1),
                dst: slot(2),
            },
            PredOp::LoadDuration {
                dur: dur(1000),
                dst: slot(3),
            },
            PredOp::WinFwd {
                time: slot(0),
                dur: slot(3),
                dst: slot(4),
            },
            PredOp::LoadEventField {
                binding: BindingIdx(1),
                field: FieldIdx(3),
                dst: slot(5),
            },
            PredOp::WinIn {
                x: slot(5),
                lo: slot(2),
                hi: slot(4),
                dst: slot(6),
            },
        ]),
        result: slot(6),
    };
    assert!(matches!(win.ops[6], PredOp::WinIn { dst, .. } if dst == win.result));
}

#[test]
fn rule3_pmtud_shape_is_expressible() {
    // Phase 1 target: Example 01 Rule 3 — anchor event, correlate with a
    // forward window (absent-handling via the else branch), infer-cause with
    // weight.
    let rule = pmtud_hypothesis();

    // Anchor: event type + compiled metric predicate.
    assert!(matches!(
        &rule.anchor.source,
        AnchorSource::Event(t) if t.as_str() == "tcp.retransmission_burst"
    ));
    assert_eq!(rule.anchor.binding, Symbol::new("rtx"));
    assert_eq!(rule.anchor.predicate.ops.len(), 3);

    // Correlate: forward window forces WaitQueue suspension (03 §3.1);
    // max_forward drives upper_bound (07 §5).
    assert_eq!(rule.correlates.len(), 1);
    assert_eq!(
        rule.correlates[0].window,
        WindowProof::Calculable {
            back: dur(500),
            forward: dur(1000)
        }
    );
    assert_eq!(rule.max_forward(), dur(1000));
    assert_eq!(rule.annotations.max_backward, dur(500));
    assert_eq!(rule.annotations.target_scope, Some(ScopeType::Session));

    // Branches: present(ptb) → +85; else → +35 + request_observation;
    // Unknown → request_topology (C10).
    let branches = rule.branches.as_ref().expect("rule 3 has an if/else");
    assert!(matches!(
        branches.cond.ops[0],
        PredOp::Present {
            binding: BindingIdx(1),
            ..
        }
    ));
    match &branches.then_body[0] {
        Intent::InferCause {
            cause,
            target,
            weight,
            evidence,
            ..
        } => {
            assert_eq!(cause.as_str(), "PmtudBlackhole");
            assert_eq!(target.as_str(), "rtx.target");
            assert_eq!(weight.value(), 85);
            assert_eq!(evidence.len(), 2);
        }
        other => panic!("expected InferCause, got {other:?}"),
    }
    let else_body = branches.else_body.as_ref().expect("rule 3 has else");
    assert!(matches!(
        &else_body[0],
        Intent::InferCause { weight, evidence, .. } if weight.value() == 35 && evidence.len() == 1
    ));
    assert!(matches!(
        &else_body[1],
        Intent::EmitAction { kind: ActionKind::RequestObservation, arg: Some(a), .. }
            if a.as_str() == "icmp.visibility"
    ));
    assert!(matches!(
        &branches.unknown_body[0],
        Intent::EmitAction {
            kind: ActionKind::RequestTopology,
            ..
        }
    ));

    // Both InferCause intents share the same static provenance prefix —
    // provenance dedup key per (rule, cause, target) (03 §3.3).
    let key_of = |i: &Intent| match i {
        Intent::InferCause { provenance_key, .. } => provenance_key.clone(),
        other => panic!("expected InferCause, got {other:?}"),
    };
    assert_eq!(key_of(&branches.then_body[0]), key_of(&else_body[0]));
}

#[test]
fn rule8_suppress_downstream_shape_is_expressible() {
    // Example 07/08: decision rule with Problem anchor (no predicate),
    // Problem-source correlate with upstream_of topo, suppress_symptom
    // lowered to SupersedeProblem (05 §1.1).
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
        // time: upstream.time in [downstream.time - 30s, downstream.time + 5s]
        window: WindowProof::Calculable {
            back: dur(30_000),
            forward: dur(5_000),
        },
        min_match: 1,
    }]);
    let rule = RuleInstance {
        id: RuleId::new("suppress_downstream"),
        kind: RuleKind::Decision,
        scope: ScopeType::Global,
        anchor: AnchorSpec {
            binding: Symbol::new("downstream"),
            source: AnchorSource::Problem(ProblemKind::new("DeviceUnreachable")),
            predicate: Predicate::always_true(),
        },
        correlates,
        branches: Some(BranchTable {
            cond: Predicate {
                ops: Box::new([PredOp::Present {
                    binding: BindingIdx(1),
                    dst: slot(0),
                }]),
                result: slot(0),
            },
            then_body: Box::new([Intent::SupersedeProblem {
                problem: ProblemKind::new("DeviceUnreachable"),
                target: MetricPath::new("downstream.target"),
            }]),
            else_body: None,
            unknown_body: Box::new([Intent::EmitAction {
                kind: ActionKind::RequestTopology,
                arg: None,
                target: None,
                reason: None,
                evidence: Box::new([]),
            }]),
        }),
        body: Box::new([]),
        annotations: VerifiedAnnotations {
            max_backward: dur(30_000),
            max_forward: dur(5_000),
            target_scope: None,
        },
    };

    let img = ProgramImage::new(
        ProgramImage::pack_version(1, 0, 0),
        "airpulse.topo_suppression",
        Box::new(["topology".into()]),
        Box::new([]),
        Box::new([rule]),
        catalog_ref(),
    );

    // ProblemEmission re-eval lookup (03 §3.5, Example 8).
    let p = ProblemKind::new("DeviceUnreachable");
    let hits: Vec<_> = img
        .rules_for(
            AnchorKey::Problem(&p),
            ScopeType::Global,
            RuleKind::Decision,
        )
        .collect();
    assert_eq!(hits.len(), 1);
    let rule = hits[0];
    assert!(matches!(
        rule.correlates[0].source,
        CorrelateSource::Problem(_)
    ));
    assert_eq!(rule.correlates[0].topo.func.as_ref(), "upstream_of");
    let then_body = &rule.branches.as_ref().expect("has branch").then_body;
    assert!(matches!(
        &then_body[0],
        Intent::SupersedeProblem { target, .. } if target.as_str() == "downstream.target"
    ));
}
