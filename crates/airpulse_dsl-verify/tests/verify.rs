use airpulse_dsl_evaluator::{Engine, OfflineAuditSink, RunMode, StaticTopology};
use airpulse_dsl_ir::{
    AnchorSource, AnchorSpec, BindingIdx, Intent, PredOp, Predicate, ProgramImage, ProvKey, RuleInstance, RuleKind,
    SlotIdx, Symbol, VerifiedAnnotations, WindowProof,
};
use airpulse_dsl_store::{EventNode, EventProvenance, Limits};
use airpulse_dsl_types::{
    ActionKind, CauseKind, EventId, EventTime, EventType, MetricPath, ProblemKind, RuleId, SarifId, ScopeId,
    ScopeType, Severity, Weight,
};
use airpulse_dsl_verify::{render_diagnostics, verify_source, verify_source_with_config, VerifyConfig};

fn t(ms: i64) -> EventTime {
    EventTime::from_millis(ms)
}

fn session_scope() -> ScopeId {
    ScopeId::session((0x0a00_0001, 443), (0x0a00_0002, 51234))
}

fn evt(id: u64, event_type: &str, time_ms: i64, scope: ScopeId, fields: Vec<(airpulse_dsl_ir::FieldIdx, i64)>) -> EventNode {
    EventNode::new(
        EventId::new(id),
        EventType::new(event_type),
        t(time_ms),
        scope,
        fields,
        EventProvenance::default(),
    )
}

fn example(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/../../docs/idea/examples/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("example fixture")
}

fn slot(i: u8) -> SlotIdx {
    SlotIdx::new(i).unwrap_or(Predicate::always_true().result)
}

fn stub_device_unreachable_rules() -> Vec<RuleInstance> {
    let evidence = RuleInstance {
        id: RuleId::new("stub_device_unreachable_evidence"),
        kind: RuleKind::Evidence,
        scope: ScopeType::Global,
        anchor: AnchorSpec {
            binding: Symbol::new("ev"),
            source: AnchorSource::Event(EventType::new("net.device_unreachable")),
            predicate: Predicate::always_true(),
        },
        correlates: Box::new([]),
        branches: None,
        body: Box::new([Intent::InferCause {
            cause: CauseKind::new("DeviceDown"),
            target: MetricPath::new("ev.target"),
            weight: Weight::new(100).expect("valid weight"),
            evidence: Box::new([Symbol::new("ev")]),
            provenance_key: ProvKey {
                rule: RuleId::new("stub_device_unreachable_evidence"),
                cause: CauseKind::new("DeviceDown"),
                target_expr_hash: 0x5f0d_11e5,
            },
            evidence_pii: Box::new([]),
        }]),
        annotations: VerifiedAnnotations::from_correlates(&[], None),
    };
    let verdict = RuleInstance {
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
                        field: airpulse_dsl_evaluator::schema::CAUSE_FIELD_CONFIDENCE,
                        dst: slot(0),
                    },
                    PredOp::LoadConst { imm: 80, dst: slot(1) },
                    PredOp::CmpGe { lhs: slot(0), rhs: slot(1), dst: slot(2) },
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
            sarif_id: SarifId::new("device_unreachable"),
            pii: Box::new([]),
        }]),
        annotations: VerifiedAnnotations::from_correlates(&[], None),
    };
    vec![evidence, verdict]
}

#[test]
fn all_examples_verify_cleanly() {
    let files = [
        "01-pmtud-blackhole.adgl",
        "02-tcp-retrans-seed.adgl",
        "03-auth-outage-impact.adgl",
        "04-dhcp-missing-auth.adgl",
        "05-crc-link-flap.adgl",
        "06-link-absent.adgl",
        "07-suppress-downstream.adgl",
        "08-stp-tcp-burst.adgl",
        "09-ap-deauth-missing-rf.adgl",
        "10-ambiguity-demo.adgl",
    ];
    for file in files {
        let src = example(file);
        let verified = verify_source(&src);
        assert!(verified.is_ok(), "{file} failed verification");
    }
}

#[test]
fn lowering_example_01_matches_key_fixture_shape() {
    let src = example("01-pmtud-blackhole.adgl");
    let verified = verify_source(&src).expect("example 01 verifies");
    assert_eq!(verified.image.rules.len(), 2);
    let evidence = &verified.image.rules[0];
    assert_eq!(evidence.id.as_str(), "pmtud_hypothesis");
    let branches = evidence.branches.as_ref().expect("if/else lowered");
    assert_eq!(branches.unknown_body.len(), 1);
    assert!(matches!(
        branches.unknown_body[0],
        Intent::EmitAction {
            kind: ActionKind::RequestTopology,
            ..
        }
    ));
    match evidence.correlates[0].window {
        WindowProof::Calculable { back, forward } => {
            assert_eq!(back.millis(), 500);
            assert_eq!(forward.millis(), 1000);
        }
        WindowProof::RuntimeCheck => panic!("expected calculable window"),
    }
    if let Intent::InferCause { weight, .. } = &branches.then_body[0] {
        assert_eq!(weight.value(), 85);
    } else {
        panic!("expected infer in then branch");
    }
    if let Some(else_body) = &branches.else_body {
        if let Intent::InferCause { weight, .. } = &else_body[0] {
            assert_eq!(weight.value(), 35);
        } else {
            panic!("expected infer in else branch");
        }
    } else {
        panic!("expected else branch");
    }
    assert_eq!(evidence.annotations.max_backward.millis(), 500);
    assert_eq!(evidence.annotations.max_forward.millis(), 1000);
}

#[test]
fn end_to_end_example_01_emits_problem() {
    let src = example("01-pmtud-blackhole.adgl");
    let verified = verify_source(&src).expect("example 01 verifies");
    let img = &verified.image;
    let mut eng = Engine::new(
        img,
        StaticTopology::new(Limits::default().max_topology_hops),
        OfflineAuditSink::new(),
        Limits::default(),
        RunMode::Offline,
    );
    let s = session_scope();
    eng.ingest(evt(1, "tcp.retransmission_burst", 10_000, s, vec![(airpulse_dsl_ir::FieldIdx(0), 1500)]));
    eng.ingest(evt(2, "icmp.ptb", 10_400, s, vec![]));
    eng.finish();
    let snap = eng.snapshot();
    assert!(
        snap.causes
            .iter()
            .any(|c| c.kind.as_str() == "PmtudBlackhole" && c.confidence.value() == 85)
    );
    assert!(snap.problems.iter().any(|p| p.kind.as_str() == "XlIcmpTcpMss"));
}

#[test]
fn end_to_end_confidence_threshold_uses_confidence_not_time() {
    let src = r#"
ruleset "verify-confidence-threshold" {
    version = "1.0"
    requires = ["l3-deep"]

    evidence low_seed {
        scope: Session
        anchor rtx: event(tcp.retransmission_burst)
        infer Cause(PmtudBlackhole) { target: rtx.target, weight: +35, evidence: [rtx] }
    }

    evidence high_seed {
        scope: Session
        anchor ptb: event(icmp.ptb)
        infer Cause(PmtudBlackhole) { target: ptb.target, weight: +85, evidence: [ptb] }
    }

    decision verdict {
        scope: Session
        anchor c: Cause(PmtudBlackhole) { c.confidence >= 80 }
        emit Problem(XlIcmpTcpMss) { severity: High, evidence: [c], sarif_id: "l3_pmtud_blackhole" }
    }
}"#;

    let verified = verify_source(src).expect("confidence ruleset verifies");
    let img = &verified.image;
    let s = session_scope();

    let mut low = Engine::new(
        img,
        StaticTopology::new(Limits::default().max_topology_hops),
        OfflineAuditSink::new(),
        Limits::default(),
        RunMode::Offline,
    );
    low.ingest(evt(1, "tcp.retransmission_burst", 1_250_000, s, vec![(airpulse_dsl_ir::FieldIdx(0), 1500)]));
    low.finish();
    let low_snap = low.snapshot();
    assert_eq!(low_snap.causes.len(), 1);
    assert_eq!(low_snap.causes[0].confidence.value(), 35);
    assert!(
        low_snap.problems.is_empty(),
        "large timestamps must not make c.confidence >= 80 true when confidence is 35"
    );

    let mut high = Engine::new(
        img,
        StaticTopology::new(Limits::default().max_topology_hops),
        OfflineAuditSink::new(),
        Limits::default(),
        RunMode::Offline,
    );
    high.ingest(evt(2, "icmp.ptb", 1_250_100, s, vec![]));
    high.finish();
    let high_snap = high.snapshot();
    assert_eq!(high_snap.causes.len(), 1);
    assert_eq!(high_snap.causes[0].confidence.value(), 85);
    assert!(
        high_snap.problems.iter().any(|p| p.kind.as_str() == "XlIcmpTcpMss"),
        "confidence 85 must trigger the thresholded decision"
    );
}

#[test]
fn emits_expected_negative_codes() {
    let cases = [
        (
            r#"
            ruleset "x" {
                version = "1.0"
                decision d {
                    scope: Session
                    anchor c: Cause(Congestion) { c.confidence >= 1 }
                    infer Cause(Congestion) { target: c.target, weight: +1, evidence: [c] }
                }
            }"#,
            "ADGL0450",
        ),
        (
            r#"
            ruleset "x" {
                version = "1.0"
                evidence e {
                    scope: Session
                    anchor r: event(tcp.retransmission_burst) { same_session(r.target, r.target) }
                    infer Cause(Congestion) { target: r.target, weight: +1, evidence: [r] }
                }
            }"#,
            "ADGL0501",
        ),
        (
            r#"
            ruleset "x" {
                version = "1.0"
                requires = ["missing-cap"]
                evidence e {
                    scope: Session
                    anchor r: event(tcp.retransmission_burst)
                    infer Cause(Congestion) { target: r.target, weight: +1, evidence: [r] }
                }
            }"#,
            "ADGL0430",
        ),
        (
            r#"
            ruleset "x" {
                version = "1.0"
                evidence e {
                    scope: Session
                    anchor r: event(unknown.event)
                    infer Cause(Congestion) { target: r.target, weight: +1, evidence: [r] }
                }
            }"#,
            "ADGL0201",
        ),
        (
            r#"
            ruleset "x" {
                version = "1.0"
                mutually_exclusive(Congestion, NopeCause)
                evidence e {
                    scope: Session
                    anchor r: event(tcp.retransmission_burst)
                    infer Cause(Congestion) { target: r.target, weight: +1, evidence: [r] }
                }
            }"#,
            "ADGL0202",
        ),
        (
            r#"
            ruleset "x" {
                version = "1.0"
                evidence e {
                    scope: Global
                    anchor r: event(tcp.retransmission_burst)
                    infer Cause(Congestion) { target: r.target, weight: +1, evidence: [r] }
                }
            }"#,
            "ADGL0210",
        ),
        (
            r#"
            ruleset "x" {
                version = "1.0"
                evidence e {
                    scope: Session
                    anchor r: event(tcp.retransmission_burst)
                    correlate ptb: event(icmp.ptb) {
                        topo: same_session(r.target, ptb.target)
                        time: ptb.time in [ptb.time, r.time + 1s]
                    }
                    if present(ptb) {
                        infer Cause(Congestion) { target: r.target, weight: +1, evidence: [r, ptb] }
                    }
                }
            }"#,
            "ADGL0411",
        ),
        (
            r#"
            ruleset "x" {
                version = "1.0"
                evidence e {
                    scope: Session
                    anchor r: event(tcp.retransmission_burst)
                    infer Cause(Congestion) { target: r.target, weight: +1, evidence: [r] }
                }
                decision d1 {
                    scope: Session
                    anchor c: Cause(Congestion) { c.confidence >= 1 }
                    emit Problem(XlIcmpTcpMss) { severity: High, evidence: [c] }
                }
                decision d2 {
                    scope: Session
                    anchor p: Problem(XlIcmpTcpMss)
                    emit Problem(AmbiguousDiagnosis) { severity: Medium, evidence: [p] }
                }
                decision d3 {
                    scope: Session
                    anchor q: Problem(AmbiguousDiagnosis)
                    emit Problem(XlIcmpTcpMss) { severity: Medium, evidence: [q] }
                }
            }"#,
            "ADGL0410",
        ),
        (
            r#"
            ruleset "x" {
                version = "1.0"
                evidence e {
                    scope: Session
                    anchor r: event(tcp.retransmission_burst)
                    correlate ptb: event(icmp.ptb) {
                        topo: same_session(r.target, ptb.target)
                        time: ptb.time in [r.time - 70s, r.time + 70s]
                    }
                }
            }"#,
            "ADGL0412",
        ),
        (
            r#"
            ruleset "x" {
                version = "1.0"
                mutually_exclusive(Congestion, PmtudBlackhole)
                mutually_exclusive(PmtudBlackhole, Congestion)
                evidence e {
                    scope: Session
                    anchor r: event(tcp.retransmission_burst)
                    infer Cause(Congestion) { target: r.target, weight: +1, evidence: [r] }
                }
            }"#,
            "ADGL0440",
        ),
        (
            r#"
            ruleset "x" {
                version = "1.0"
                evidence e {
                    scope: Session
                    anchor r: event(tcp.retransmission_burst)
                    correlate ptb: event(icmp.ptb) {
                        topo: made_up_topo(r.target, ptb.target)
                        time: ptb.time in [r.time - 1s, r.time + 1s]
                    }
                }
            }"#,
            "ADGL0420",
        ),
        (
            r#"
            ruleset "x" {
                version = "1.0"
                evidence e {
                    scope: Session
                    anchor r: event(tcp.retransmission_burst)
                    infer Cause(RfInterference) { target: r.target, weight: +10, evidence: [r] }
                }
            }"#,
            "ADGL0211",
        ),
        (
            r#"
            ruleset "x" {
                version = "1.0"
                decision d {
                    scope: Session
                    anchor c: Cause(Congestion) { c.confidence >= 1 }
                    emit Problem(WlanRadiusOutage) { target: c.target, severity: High, evidence: [c] }
                }
            }"#,
            "ADGL0212",
        ),
        (
            r#"
            ruleset "x" {
                version = "1.0"
                evidence e {
                    scope: Session
                    anchor r: event(tcp.retransmission_burst)
                    correlate ptb: event(icmp.ptb) {
                        topo: same_session(r.target, ptb.target)
                        time: ptb.time in [same_session(r.target, ptb.target), r.time + 1s]
                    }
                    if present(ptb) {
                        infer Cause(Congestion) { target: r.target, weight: +1, evidence: [r, ptb] }
                    }
                }
            }"#,
            "ADGL0501",
        ),
        (
            r#"
            ruleset "x" {
                version = "1.0"
                decision d {
                    scope: Session
                    anchor c: Cause(Congestion) { c.confidence >= 1 }
                    action suppress_symptom(c) {}
                }
            }"#,
            "ADGL0209",
        ),
    ];
    for (src, code) in cases {
        let err = verify_source(src).expect_err("must fail");
        assert!(
            err.iter().any(|d| d.code == code),
            "expected {code}, got {:?}",
            err.iter().map(|d| d.code).collect::<Vec<_>>()
        );
    }
}

#[test]
fn warns_on_redundant_exclusivity() {
    let src = r#"
    ruleset "x" {
        version = "1.0"
        requires = ["missing-cap"]
        mutually_exclusive(PhysicalCableDamage, RfInterference)
        evidence e {
            scope: Session
            anchor r: event(tcp.retransmission_burst)
            infer Cause(Congestion) { target: r.target, weight: +1, evidence: [r] }
        }
    }"#;
    let err = verify_source(src).expect_err("warning + error batch expected");
    assert!(err.iter().any(|d| d.code == "ADGL0502"));
}

#[test]
fn render_diagnostics_includes_code_and_message() {
    let src = r#"
    ruleset "x" {
        version = "1.0"
        requires = ["missing-cap"]
    }"#;
    let err = verify_source(src).expect_err("must fail");
    let rendered = render_diagnostics(src, "x.adgl", &err);
    assert!(rendered.contains("ADGL0430"), "{rendered}");
    assert!(rendered.contains("unknown capability"), "{rendered}");
}

#[test]
fn example_07_matches_fixture_shape() {
    let src = example("07-suppress-downstream.adgl");
    let verified = verify_source(&src).expect("example 07 verifies");
    let lowered = &verified.image.rules[0];
    let fixture = airpulse_dsl_evaluator::fixtures::suppress_downstream_rule();
    assert_eq!(lowered.correlates[0].topo.func_idx.0, 5);
    match lowered.correlates[0].window {
        WindowProof::Calculable { back, forward } => {
            assert_eq!(back.millis(), 30_000);
            assert_eq!(forward.millis(), 5_000);
        }
        WindowProof::RuntimeCheck => panic!("expected calculable window"),
    }
    match fixture.correlates[0].window {
        WindowProof::Calculable { back, forward } => {
            assert_eq!(back.millis(), 30_000);
            assert_eq!(forward.millis(), 5_000);
        }
        WindowProof::RuntimeCheck => panic!("fixture should be calculable"),
    }
    let branches = lowered.branches.as_ref().expect("if branch lowered");
    let then = &branches.then_body;
    assert_eq!(then.len(), 2, "must lower to SupersedeProblem + EmitAction");
    assert!(matches!(then[0], Intent::SupersedeProblem { .. }));
    assert!(matches!(
        then[1],
        Intent::EmitAction {
            kind: ActionKind::SuppressSymptom,
            ..
        }
    ));
    if let Intent::EmitAction { reason, .. } = &then[1] {
        assert_eq!(reason.as_deref(), Some("Masked by upstream topology failure"));
    }
}

#[test]
fn end_to_end_example_07_suppresses_downstream() {
    let src = example("07-suppress-downstream.adgl");
    let verified = verify_source(&src).expect("example 07 verifies");
    let mut rules = stub_device_unreachable_rules();
    rules.extend(verified.image.rules.iter().cloned());
    let img = ProgramImage::new(
        verified.image.version,
        "verify-example-07",
        verified.image.requires.clone(),
        verified.image.exclusivity.clone(),
        rules.into_boxed_slice(),
        verified.image.catalog_ref.clone(),
    );
    let mut topo = StaticTopology::new(Limits::default().max_topology_hops);
    let upstream = ScopeId::port(1, 1);
    let downstream = ScopeId::port(2, 1);
    topo.upstream_edge(upstream, downstream);
    let mut eng = Engine::new(
        &img,
        topo,
        OfflineAuditSink::new(),
        Limits::default(),
        RunMode::Offline,
    );
    let up_key = eng.intern_scope(upstream);
    let down_key = eng.intern_scope(downstream);
    eng.ingest(evt(
        1,
        "net.device_unreachable",
        1_000,
        ScopeId::GLOBAL,
        vec![(airpulse_dsl_evaluator::schema::EVENT_FIELD_TARGET, up_key)],
    ));
    eng.ingest(evt(
        2,
        "net.device_unreachable",
        2_000,
        ScopeId::GLOBAL,
        vec![(airpulse_dsl_evaluator::schema::EVENT_FIELD_TARGET, down_key)],
    ));
    eng.finish();
    let snap = eng.snapshot();
    assert!(snap.problems.iter().any(|p| p.target == downstream && p.superseded));
    assert!(snap.audit.iter().any(|a| a.intent.kind == ActionKind::SuppressSymptom));
}

#[test]
fn dedup_window_config_validation_emits_adgl0503() {
    let src = example("01-pmtud-blackhole.adgl");
    let err = verify_source_with_config(
        &src,
        VerifyConfig {
            max_lookback_ms: 60_000,
            dedup_window_ms: 0,
        },
    )
    .expect_err("invalid dedup window must fail");
    assert!(err.iter().any(|d| d.code == "ADGL0503"));
}

