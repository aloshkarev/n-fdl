use airpulse_dsl_catalog::{EventOrBindingType, resolve_metric_path};
use airpulse_dsl_evaluator::schema::EVENT_FIELD_TARGET;
use airpulse_dsl_evaluator::{
    Engine, EngineDiagnostic, OfflineAuditSink, RunMode, StaticTopology, to_sarif,
};
use airpulse_dsl_ir::{FieldIdx, ProgramImage};
use airpulse_dsl_store::{EventNode, EventProvenance, EvidenceEdgeKind, Limits};
use airpulse_dsl_syntax::parse_ruleset;
use airpulse_dsl_types::{
    ActionKind, CauseKind, EventId, EventTime, EventType, ScopeId, stable_string_i64,
};
use airpulse_dsl_verify::verify;

type OfflineEngine<'img> = Engine<'img, StaticTopology, OfflineAuditSink>;

const EX02: &str = include_str!("../../../docs/idea/examples/02-tcp-retrans-seed.adgl");
const EX03: &str = include_str!("../../../docs/idea/examples/03-auth-outage-impact.adgl");
const EX04: &str = include_str!("../../../docs/idea/examples/04-dhcp-missing-auth.adgl");
const EX05: &str = include_str!("../../../docs/idea/examples/05-crc-link-flap.adgl");
const EX06: &str = include_str!("../../../docs/idea/examples/06-link-absent.adgl");
const EX08: &str = include_str!("../../../docs/idea/examples/08-stp-tcp-burst.adgl");
const EX09: &str = include_str!("../../../docs/idea/examples/09-ap-deauth-missing-rf.adgl");
const EX10: &str = include_str!("../../../docs/idea/examples/10-ambiguity-demo.adgl");
const STUB_EX08_DECISION: &str = include_str!("golden/_stubs/08-stp-companion-decision.adgl");
const EX01: &str = include_str!("../../../docs/idea/examples/01-pmtud-blackhole.adgl");

fn t(ms: i64) -> EventTime {
    EventTime::from_millis(ms)
}

fn session_scope() -> ScopeId {
    ScopeId::session((0x0a00_0001, 443), (0x0a00_0002, 51234))
}

fn field_idx(event_type: &str, field: &str) -> FieldIdx {
    let et = EventType::new(event_type);
    resolve_metric_path(EventOrBindingType::Event(&et), field)
        .unwrap_or_else(|| panic!("missing catalog path {event_type}.{field}"))
        .0
}

fn parse_verify_image(src: &str) -> ProgramImage {
    let ast = parse_ruleset(src).expect("ruleset parses");
    verify(&ast).expect("ruleset verifies").image
}

fn offline_engine(img: &ProgramImage) -> OfflineEngine<'_> {
    Engine::new(
        img,
        StaticTopology::new(Limits::default().max_topology_hops),
        OfflineAuditSink::new(),
        Limits::default(),
        RunMode::Offline,
    )
}

fn evt(
    id: u64,
    event_type: &str,
    time_ms: i64,
    scope: ScopeId,
    fields: Vec<(FieldIdx, i64)>,
) -> EventNode {
    EventNode::new(
        EventId::new(id),
        EventType::new(event_type),
        t(time_ms),
        scope,
        fields,
        EventProvenance::default(),
    )
}

fn merge_rulesets(
    name: &str,
    requires: &[&str],
    sources: &[&str],
    appended_snippets: &[&str],
) -> String {
    let mut merged = String::new();
    merged.push_str(&format!("ruleset \"{name}\" {{\n"));
    merged.push_str("    version = \"1.0\"\n");
    merged.push_str("    requires = [");
    for (idx, req) in requires.iter().enumerate() {
        if idx > 0 {
            merged.push_str(", ");
        }
        merged.push('"');
        merged.push_str(req);
        merged.push('"');
    }
    merged.push_str("]\n\n");
    for src in sources {
        merged.push_str(&extract_ruleset_body(src));
        merged.push('\n');
    }
    for snippet in appended_snippets {
        merged.push_str(snippet);
        merged.push('\n');
    }
    merged.push_str("}\n");
    merged
}

fn extract_ruleset_body(src: &str) -> String {
    let start = src.find('{').expect("ruleset open brace");
    let mut depth = 0_i32;
    let mut end = None;
    for (off, ch) in src[start..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(start + off);
                    break;
                }
            }
            _ => {}
        }
    }
    let end = end.expect("ruleset close brace");
    let inner = &src[start + 1..end];
    let mut out = String::new();
    for line in inner.lines() {
        let trim = line.trim();
        if trim.starts_with("version") || trim.starts_with("requires") {
            continue;
        }
        if trim.is_empty() {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

#[test]
fn g01_present_ptb_in_window_matches_fixture_outcome() {
    let img = parse_verify_image(EX01);
    let mut eng = offline_engine(&img);
    let s = session_scope();

    eng.ingest(evt(
        1,
        "tcp.retransmission_burst",
        10_000,
        s,
        vec![(field_idx("tcp.retransmission_burst", "segment_size"), 1500)],
    ));
    assert_eq!(
        eng.suspended(),
        1,
        "forward +1s window must suspend on anchor"
    );
    eng.ingest(evt(2, "icmp.ptb", 10_400, s, vec![]));
    eng.finish();

    let snap = eng.snapshot();
    assert_eq!(snap.causes.len(), 1);
    assert_eq!(snap.causes[0].kind.as_str(), "PmtudBlackhole");
    assert_eq!(snap.causes[0].confidence.value(), 85);
    assert_eq!(snap.problems.len(), 1);
    assert_eq!(snap.problems[0].kind.as_str(), "XlIcmpTcpMss");
    assert_eq!(snap.problems[0].sarif_id.as_str(), "l3_pmtud_blackhole");
    assert!(snap.audit.is_empty(), "present branch emits no actions");

    insta::assert_debug_snapshot!("g01_present_snapshot", snap);
    insta::assert_snapshot!("g01_present_sarif", to_sarif(&eng.snapshot()));
}

#[test]
fn g01_absent_ptb_matches_fixture_outcome() {
    let img = parse_verify_image(EX01);
    let mut eng = offline_engine(&img);
    let s = session_scope();

    eng.ingest(evt(
        1,
        "tcp.retransmission_burst",
        10_000,
        s,
        vec![(field_idx("tcp.retransmission_burst", "segment_size"), 1500)],
    ));
    assert_eq!(eng.suspended(), 1);
    eng.advance_watermark(t(11_000));
    assert_eq!(eng.resumed(), 0, "wm == upper must not resume");
    eng.advance_watermark(t(11_001));
    assert_eq!(eng.resumed(), 1, "wm > upper resumes the absent branch");
    eng.finish();

    let snap = eng.snapshot();
    assert_eq!(snap.causes.len(), 1);
    assert_eq!(snap.causes[0].confidence.value(), 35);
    assert!(
        snap.problems.is_empty(),
        "35 < 80 should not emit verdict problem"
    );
    assert_eq!(snap.audit.len(), 1);
    assert_eq!(snap.audit[0].intent.kind, ActionKind::RequestObservation);
    assert_eq!(snap.audit[0].code, Some("ADGL3001"));

    insta::assert_debug_snapshot!("g01_absent_snapshot", snap);
    insta::assert_snapshot!("g01_absent_sarif", to_sarif(&eng.snapshot()));
}

#[test]
fn g02_tcp_retrans_seed_only_no_ambiguity() {
    let img = parse_verify_image(EX02);
    let mut eng = offline_engine(&img);
    let s = session_scope();

    eng.ingest(evt(
        1,
        "tcp.retransmission_burst",
        10_000,
        s,
        vec![(field_idx("tcp.retransmission_burst", "segment_size"), 1500)],
    ));
    eng.finish();

    let snap = eng.snapshot();
    assert_eq!(snap.causes.len(), 3);
    assert!(
        snap.problems.is_empty(),
        "seed-only ruleset emits no problems"
    );
    assert!(
        !snap
            .audit
            .iter()
            .any(|a| a.intent.kind == ActionKind::MarkAmbiguous),
        "all seeds are below Probable threshold, ambiguity must not synthesize"
    );
    assert_eq!(
        snap.causes
            .iter()
            .map(|c| c.confidence.value())
            .collect::<Vec<_>>(),
        vec![35, 35, 20]
    );

    insta::assert_debug_snapshot!("g02_snapshot", snap);
    insta::assert_snapshot!("g02_sarif", to_sarif(&eng.snapshot()));
}

#[test]
fn g03_merged_auth_rules_route_dhcp_vlan_target() {
    let merged = merge_rulesets(
        "airpulse.aaa_diagnostics",
        &["l3-deep", "topology"],
        &[EX03, EX04],
        &[],
    );
    let img = parse_verify_image(&merged);
    let mut eng = offline_engine(&img);
    let client = ScopeId::client_mac(0x00_11_22_33_44_55);
    let vlan = ScopeId::vlan(42);
    let vlan_key = eng.intern_scope(vlan);

    eng.ingest(evt(
        1,
        "dhcp.timeout",
        20_000,
        client,
        vec![
            (field_idx("dhcp.timeout", "vlan"), vlan_key),
            (field_idx("dhcp.timeout", "client_mac"), 0x0011_2233_4455),
        ],
    ));
    eng.finish();

    let snap = eng.snapshot();
    assert_eq!(snap.causes.len(), 2, "child + rolled-up parent cause");
    assert_eq!(snap.causes[0].kind.as_str(), "AuthServerOutage");
    assert_eq!(snap.causes[0].confidence.value(), 50);
    assert_eq!(snap.causes[0].target, vlan);
    assert!(
        snap.problems.is_empty(),
        "decision in Example 03 does not fire without roll-up"
    );
    assert!(
        snap.audit
            .iter()
            .any(|a| a.intent.kind == ActionKind::RequestObservation),
        "absence branch requests AAA telemetry"
    );

    insta::assert_debug_snapshot!("g03_snapshot", snap);
    insta::assert_snapshot!("g03_sarif", to_sarif(&eng.snapshot()));
}

#[test]
fn g03_cross_scope_rollup_max_expected_behavior() {
    let merged = merge_rulesets(
        "airpulse.aaa_diagnostics",
        &["l3-deep", "topology"],
        &[EX03, EX04],
        &[],
    );
    let img = parse_verify_image(&merged);
    let mut eng = offline_engine(&img);
    let vlan = ScopeId::vlan(42);
    let vlan_key = eng.intern_scope(vlan);

    for (id, mac) in [(1, 0x1000_u64), (2, 0x1001_u64), (3, 0x1002_u64)] {
        eng.ingest(evt(
            id,
            "dhcp.timeout",
            20_000 + (id as i64),
            ScopeId::client_mac(mac),
            vec![(field_idx("dhcp.timeout", "vlan"), vlan_key)],
        ));
    }
    eng.finish();

    let snap = eng.snapshot();
    // Target behavior from 09 §3/ADR-003: one VLAN-level MAX roll-up (50).
    // Example 03 requires c.confidence >= 80, so no decision problem fires.
    let rolled = snap
        .causes
        .iter()
        .find(|c| c.kind.as_str() == "AuthServerOutage" && c.target == vlan && c.scope == vlan)
        .expect("vlan-partition roll-up cause");
    assert_eq!(
        rolled.confidence.value(),
        50,
        "MAX roll-up must not sum child weights"
    );
    assert_eq!(
        snap.causes
            .iter()
            .filter(|c| c.kind.as_str() == "AuthServerOutage" && c.target == vlan)
            .count(),
        4,
        "three child-scope causes plus one parent roll-up"
    );
    assert!(
        !snap.problems.iter().any(|p| p.target == vlan),
        "roll-up to 50 alone must not satisfy c.confidence >= 80 decision threshold"
    );

    // RollsUp{ child → parent } provenance edges (09 §3.2, ADR-003):
    // one per contributing child partition.
    let parent_part = eng.store().partition(vlan).expect("vlan partition exists");
    let rollup_edges = parent_part
        .edges
        .iter()
        .filter(|e| e.kind == EvidenceEdgeKind::RollsUp)
        .count();
    assert_eq!(
        rollup_edges, 3,
        "one RollsUp edge per child-scope contribution"
    );
}

#[test]
fn g03_cross_scope_rollup_single_child_matches_parent_max() {
    let merged = merge_rulesets(
        "airpulse.aaa_diagnostics",
        &["l3-deep", "topology"],
        &[EX03, EX04],
        &[],
    );
    let img = parse_verify_image(&merged);
    let mut eng = offline_engine(&img);
    let vlan = ScopeId::vlan(7);
    let vlan_key = eng.intern_scope(vlan);
    let client = ScopeId::client_mac(0x00_22_33_44_55_66);

    eng.ingest(evt(
        1,
        "dhcp.timeout",
        30_000,
        client,
        vec![(field_idx("dhcp.timeout", "vlan"), vlan_key)],
    ));
    eng.finish();

    let snap = eng.snapshot();
    let child = snap
        .causes
        .iter()
        .find(|c| c.scope == client && c.target == vlan)
        .expect("child-scope cause");
    let parent = snap
        .causes
        .iter()
        .find(|c| c.scope == vlan && c.target == vlan)
        .expect("parent roll-up cause");
    assert_eq!(child.confidence.value(), 50);
    assert_eq!(parent.confidence.value(), 50);

    let parent_part = eng.store().partition(vlan).expect("vlan partition exists");
    assert_eq!(
        parent_part
            .edges
            .iter()
            .filter(|e| e.kind == EvidenceEdgeKind::RollsUp)
            .count(),
        1,
        "single child contributes exactly one RollsUp edge"
    );
}

#[test]
fn g04_port_crc_flap_present_and_link_absent_short_circuit() {
    let merged = merge_rulesets(
        "airpulse.port_diagnostics",
        &["topology"],
        &[EX05, EX06],
        &[],
    );
    let img = parse_verify_image(&merged);
    let mut eng = offline_engine(&img);
    let p1 = ScopeId::port(1, 1);
    let p2 = ScopeId::port(2, 1);

    eng.ingest(evt(
        1,
        "port.crc_errors",
        10_000,
        p1,
        vec![(field_idx("port.crc_errors", "count"), 20)],
    ));
    eng.ingest(evt(
        2,
        "port.link_flap",
        10_100,
        p1,
        vec![(field_idx("port.link_flap", "count"), 1)],
    ));
    eng.ingest(evt(
        3,
        "port.admin_state",
        20_000,
        p2,
        vec![(
            field_idx("port.admin_state", "state"),
            stable_string_i64("UP"),
        )],
    ));
    eng.finish();

    let snap = eng.snapshot();
    assert!(
        snap.causes
            .iter()
            .any(|c| c.kind.as_str() == "PhysicalCableDamage" && c.confidence.value() == 90),
        "scenario should cross cable verdict threshold via confidence (+90)"
    );
    assert!(
        !snap
            .causes
            .iter()
            .any(|c| c.kind.as_str() == "PhysicalLinkAbsent"),
        "no oper correlate => present(oper)=false, short-circuit prevents infer"
    );
    assert_eq!(snap.problems.len(), 1);
    assert_eq!(snap.problems[0].kind.as_str(), "CableDisconnected");

    insta::assert_debug_snapshot!("g04_snapshot", snap);
    insta::assert_snapshot!("g04_sarif", to_sarif(&eng.snapshot()));
}

#[test]
fn g05_link_absent_string_predicate_distinguishes_up_down() {
    let img = parse_verify_image(EX06);
    let port = ScopeId::port(7, 9);

    let mut eng_up = offline_engine(&img);
    eng_up.ingest(evt(
        1,
        "port.admin_state",
        60_000,
        port,
        vec![(
            field_idx("port.admin_state", "state"),
            stable_string_i64("UP"),
        )],
    ));
    eng_up.ingest(evt(
        2,
        "port.oper_state",
        60_100,
        port,
        vec![(
            field_idx("port.oper_state", "state"),
            stable_string_i64("UP"),
        )],
    ));
    eng_up.finish();
    let snap_up = eng_up.snapshot();
    assert!(
        !snap_up
            .causes
            .iter()
            .any(|c| c.kind.as_str() == "PhysicalLinkAbsent"),
        "oper.state == \"UP\" must not match oper.state == \"DOWN\""
    );

    let mut eng_down = offline_engine(&img);
    eng_down.ingest(evt(
        11,
        "port.admin_state",
        70_000,
        port,
        vec![(
            field_idx("port.admin_state", "state"),
            stable_string_i64("UP"),
        )],
    ));
    eng_down.ingest(evt(
        12,
        "port.oper_state",
        70_100,
        port,
        vec![(
            field_idx("port.oper_state", "state"),
            stable_string_i64("DOWN"),
        )],
    ));
    eng_down.finish();
    let snap_down = eng_down.snapshot();
    assert!(
        snap_down
            .causes
            .iter()
            .any(|c| c.kind.as_str() == "PhysicalLinkAbsent"),
        "oper.state == \"DOWN\" must match and infer PhysicalLinkAbsent"
    );
}

#[test]
fn g06_stp_backward_window_runs_immediately() {
    let merged = merge_rulesets(
        "airpulse.l2_diagnostics",
        &["l3-deep", "topology"],
        &[EX08],
        &[STUB_EX08_DECISION],
    );
    let img = parse_verify_image(&merged);
    let mut eng = offline_engine(&img);
    let s = session_scope();
    let vlan = ScopeId::vlan(100);
    let vlan_key = eng.intern_scope(vlan);

    eng.ingest(evt(
        1,
        "stp.topology_change",
        28_500,
        s,
        vec![(field_idx("stp.topology_change", "vlan"), vlan_key)],
    ));
    eng.ingest(evt(
        2,
        "tcp.retransmission_burst",
        30_000,
        s,
        vec![
            (field_idx("tcp.retransmission_burst", "segment_size"), 1500),
            (field_idx("tcp.retransmission_burst", "vlan"), vlan_key),
        ],
    ));
    assert_eq!(eng.suspended(), 0, "backward-only window must not suspend");
    eng.finish();

    let snap = eng.snapshot();
    assert!(
        snap.causes
            .iter()
            .any(|c| c.kind.as_str() == "TransientL2Disruption")
    );
    assert!(
        snap.causes
            .iter()
            .any(|c| c.kind.as_str() == "TransientL2Disruption" && c.confidence.value() == 80),
        "this scenario must cross c.confidence >= 80 by confidence, not by time"
    );
    assert!(
        snap.problems
            .iter()
            .any(|p| p.kind.as_str() == "AmbiguousDiagnosis")
    );

    insta::assert_debug_snapshot!("g06_snapshot", snap);
    insta::assert_snapshot!("g06_sarif", to_sarif(&eng.snapshot()));
}

#[test]
fn regression_cause_confidence_threshold_uses_catalog_field_index() {
    const CONFIDENCE_RULESET: &str = r#"
ruleset "airpulse.confidence_regression" {
    version = "1.0"
    requires = ["l3-deep"]

    evidence seed_low {
        scope: Session
        anchor rtx: event(tcp.retransmission_burst)
        infer Cause(PmtudBlackhole) { target: rtx.target, weight: +35, evidence: [rtx] }
    }

    evidence seed_high {
        scope: Session
        anchor rtx: event(icmp.ptb)
        infer Cause(PmtudBlackhole) { target: rtx.target, weight: +85, evidence: [rtx] }
    }

    decision verdict {
        scope: Session
        anchor c: Cause(PmtudBlackhole) { c.confidence >= 80 }
        emit Problem(XlIcmpTcpMss) { severity: High, evidence: [c], sarif_id: "l3_pmtud_blackhole" }
    }
}
"#;

    let img = parse_verify_image(CONFIDENCE_RULESET);
    let s = session_scope();

    let mut low = offline_engine(&img);
    low.ingest(evt(
        1,
        "tcp.retransmission_burst",
        900_000,
        s,
        vec![(field_idx("tcp.retransmission_burst", "segment_size"), 1500)],
    ));
    low.finish();
    let low_snap = low.snapshot();
    assert!(
        low_snap.problems.is_empty(),
        "35-confidence cause at large timestamp must not satisfy c.confidence >= 80"
    );
    assert_eq!(low_snap.causes[0].confidence.value(), 35);

    let mut high = offline_engine(&img);
    high.ingest(evt(2, "icmp.ptb", 900_100, s, vec![]));
    high.finish();
    let high_snap = high.snapshot();
    assert!(
        high_snap
            .problems
            .iter()
            .any(|p| p.kind.as_str() == "XlIcmpTcpMss"),
        "85-confidence cause must satisfy c.confidence >= 80"
    );
    assert_eq!(high_snap.causes[0].confidence.value(), 85);
}

#[test]
fn g07_ap_deauth_missing_rf_forward_window_absent_branch() {
    let img = parse_verify_image(EX09);
    let mut eng = offline_engine(&img);
    let ap = ScopeId::access_point(0x1122_3344_5566);
    let ap_key = eng.intern_scope(ap);

    eng.ingest(evt(
        1,
        "wifi.deauth_burst",
        40_000,
        ap,
        vec![(EVENT_FIELD_TARGET, ap_key)],
    ));
    assert_eq!(
        eng.suspended(),
        1,
        "forward +5s window suspends until closure"
    );

    eng.advance_watermark(t(45_000));
    assert_eq!(eng.resumed(), 0, "wm == upper must not resume");
    eng.advance_watermark(t(45_001));
    assert_eq!(eng.resumed(), 1, "resume happens when wm > upper");
    eng.finish();

    let snap = eng.snapshot();
    assert!(
        snap.causes
            .iter()
            .any(|c| c.kind.as_str() == "RfInterference" && c.confidence.value() == 40)
    );
    assert!(
        snap.audit
            .iter()
            .any(|a| a.intent.kind == ActionKind::RequestObservation && a.code == Some("ADGL3001"))
    );
    assert!(
        snap.problems.is_empty(),
        "example 09 is seed+action without a verdict rule"
    );

    insta::assert_debug_snapshot!("g07_snapshot", snap);
    insta::assert_snapshot!("g07_sarif", to_sarif(&eng.snapshot()));
}

#[test]
fn g08_ambiguity_demo_synthesizes_mark_ambiguous() {
    let img = parse_verify_image(EX10);
    let mut eng = offline_engine(&img);
    let s = session_scope();

    eng.ingest(evt(
        1,
        "tcp.retransmission_burst",
        50_000,
        s,
        vec![(field_idx("tcp.retransmission_burst", "segment_size"), 1501)],
    ));
    eng.finish();

    let snap = eng.snapshot();
    assert_eq!(snap.causes.len(), 2);
    assert!(snap.causes.iter().all(|c| c.confidence.value() == 45));
    let ambiguity_problem = snap
        .problems
        .iter()
        .find(|p| p.kind.as_str() == "AmbiguousDiagnosis")
        .expect("ambiguity synthesis must emit AmbiguousDiagnosis");
    assert_eq!(ambiguity_problem.target, s);
    assert_eq!(ambiguity_problem.sarif_id.as_str(), "ap_ambiguous");
    let mark = snap
        .audit
        .iter()
        .find(|a| a.intent.kind == ActionKind::MarkAmbiguous)
        .expect("ambiguity synthesis must emit mark_ambiguous");
    assert_eq!(mark.intent.target, Some(s));
    assert_eq!(
        mark.intent
            .causes
            .as_ref()
            .map(|(a, b)| (a.as_str(), b.as_str())),
        Some(("Congestion", "PmtudBlackhole"))
    );
    assert_eq!(
        snap.causes
            .iter()
            .map(|c| c.kind.clone())
            .collect::<std::collections::BTreeSet<_>>(),
        [
            CauseKind::new("Congestion"),
            CauseKind::new("PmtudBlackhole")
        ]
        .into_iter()
        .collect()
    );

    insta::assert_debug_snapshot!("g08_snapshot", snap);
    insta::assert_snapshot!("g08_sarif", to_sarif(&eng.snapshot()));
}

#[test]
fn catalog_defaults_synthesize_packet_loss_spurious_ambiguity() {
    const RULESET: &str = r#"
ruleset "airpulse.catalog_exclusivity" {
    version = "1.0"
    requires = ["l3-deep", "topology"]

    evidence seed_packet_loss {
        scope: Session
        anchor rtx: event(tcp.retransmission_burst)
        infer Cause(PacketLossPath) { target: rtx.target, weight: +45, evidence: [rtx] }
    }

    evidence seed_spurious {
        scope: Session
        anchor rtx: event(tcp.retransmission_burst) { rtx.segment_size > 1400 }
        infer Cause(SpuriousRetransmission) { target: rtx.target, weight: +44, evidence: [rtx] }
    }
}
"#;
    let img = parse_verify_image(RULESET);
    let mut eng = offline_engine(&img);
    let s = session_scope();

    eng.ingest(evt(
        1,
        "tcp.retransmission_burst",
        60_000,
        s,
        vec![(field_idx("tcp.retransmission_burst", "segment_size"), 1501)],
    ));
    eng.finish();

    let snap = eng.snapshot();
    assert!(
        snap.problems
            .iter()
            .any(|p| p.kind.as_str() == "AmbiguousDiagnosis"
                && p.sarif_id.as_str() == "ap_ambiguous"),
        "catalog-default exclusivity must synthesize ambiguity when Δ < 15"
    );
}

#[test]
fn g09_unsupported_target_tail_emits_adgl3008() {
    const BAD_TARGET_RULESET: &str = r#"
ruleset "airpulse.bad_target_tail" {
    version = "1.0"
    requires = ["l3-deep", "topology"]

    evidence bad_tail {
        scope: Session
        anchor rtx: event(tcp.retransmission_burst)
        infer Cause(Congestion) { target: rtx.segment_size, weight: +10, evidence: [rtx] }
    }
}
"#;

    let img = parse_verify_image(BAD_TARGET_RULESET);
    let mut eng = offline_engine(&img);
    let s = session_scope();

    eng.ingest(evt(
        1,
        "tcp.retransmission_burst",
        90_000,
        s,
        vec![(field_idx("tcp.retransmission_burst", "segment_size"), 1600)],
    ));
    eng.finish();

    assert!(
        eng.diagnostics().iter().any(|d| {
            matches!(d, EngineDiagnostic::UnsupportedTargetTail { .. })
                && d.code() == Some("ADGL3008")
        }),
        "unsupported target tails must surface a stable ADGL3008 diagnostic"
    );
}
