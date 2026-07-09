#[path = "golden/_stubs/stubs.rs"]
mod stubs;

use airpulse_dsl_evaluator::schema::EVENT_FIELD_TARGET;
use airpulse_dsl_evaluator::{
    Engine, OfflineAuditSink, ProblemView, RunMode, Snapshot, StaticTopology, fixtures, to_sarif,
};
use airpulse_dsl_ir::FieldIdx;
use airpulse_dsl_store::{EventNode, EventProvenance, Limits};
use airpulse_dsl_types::{EventId, EventTime, EventType, ProblemKind, SarifId, ScopeId, Severity};

fn t(ms: i64) -> EventTime {
    EventTime::from_millis(ms)
}

fn session_scope() -> ScopeId {
    ScopeId::session((0x0a00_0001, 443), (0x0a00_0002, 51234))
}

fn evt(id: u64, event_type: &str, time_ms: i64, scope: ScopeId, fields: Vec<(FieldIdx, i64)>) -> EventNode {
    EventNode::new(
        EventId::new(id),
        EventType::new(event_type),
        t(time_ms),
        scope,
        fields,
        EventProvenance::default(),
    )
}

fn rtx(id: u64, time_ms: i64, scope: ScopeId, segment_size: i64) -> EventNode {
    evt(id, "tcp.retransmission_burst", time_ms, scope, vec![(fixtures::F_SEGMENT_SIZE, segment_size)])
}

type OfflineEngine<'img> = Engine<'img, StaticTopology, OfflineAuditSink>;

fn offline_engine(img: &airpulse_dsl_ir::ProgramImage, topo: StaticTopology) -> OfflineEngine<'_> {
    Engine::new(img, topo, OfflineAuditSink::new(), Limits::default(), RunMode::Offline)
}

#[test]
fn g01_sarif_snapshot_contains_expected_rule_id() {
    let img = fixtures::rule3_pmtud();
    let topo = StaticTopology::new(Limits::default().max_topology_hops);
    let mut eng = offline_engine(&img, topo);
    let s = session_scope();

    eng.ingest(rtx(1, 10_000, s, 1500));
    eng.ingest(evt(2, "icmp.ptb", 10_400, s, vec![]));
    eng.finish();

    let snap = eng.snapshot();
    assert_eq!(
        snap.problems[0]
            .cause_kinds
            .iter()
            .map(|k| k.as_str())
            .collect::<Vec<_>>(),
        vec!["PmtudBlackhole"]
    );
    let sarif = to_sarif(&snap);
    assert!(sarif.contains("\"ruleId\":\"l3_pmtud_blackhole\""));
    assert!(sarif.contains("\"causes\":[\"PmtudBlackhole\"]"));
    insta::assert_snapshot!("g01_minimal_sarif", sarif);
}

#[test]
fn sarif_is_deterministic_for_same_snapshot() {
    let img = fixtures::rule3_pmtud();
    let s = session_scope();

    let mut eng_a = offline_engine(&img, StaticTopology::new(Limits::default().max_topology_hops));
    eng_a.ingest(rtx(1, 10_000, s, 1500));
    eng_a.ingest(evt(2, "icmp.ptb", 10_400, s, vec![]));
    eng_a.finish();
    let snap_a = eng_a.snapshot();

    let mut eng_b = offline_engine(&img, StaticTopology::new(Limits::default().max_topology_hops));
    eng_b.ingest(rtx(1, 10_000, s, 1500));
    eng_b.ingest(evt(2, "icmp.ptb", 10_400, s, vec![]));
    eng_b.finish();
    let snap_b = eng_b.snapshot();

    assert_eq!(to_sarif(&snap_a), to_sarif(&snap_b));
}

#[test]
fn superseded_problems_are_excluded_from_results() {
    let img = fixtures::rule8_suppression(stubs::device_unreachable_stub_rules());
    let (r1, r2) = (ScopeId::port(1, 1), ScopeId::port(2, 1));
    let mut topo = StaticTopology::new(Limits::default().max_topology_hops);
    topo.upstream_edge(r1, r2);
    let mut eng = offline_engine(&img, topo);
    let k1 = eng.intern_scope(r1);
    let k2 = eng.intern_scope(r2);

    eng.ingest(evt(1, stubs::STUB_EVENT_TYPE, 1_000, ScopeId::GLOBAL, vec![(EVENT_FIELD_TARGET, k1)]));
    eng.ingest(evt(2, stubs::STUB_EVENT_TYPE, 2_000, ScopeId::GLOBAL, vec![(EVENT_FIELD_TARGET, k2)]));
    eng.finish();

    let snap = eng.snapshot();
    assert_eq!(snap.problems.iter().filter(|p| p.superseded).count(), 1);

    let sarif = to_sarif(&snap);
    assert_eq!(sarif.matches("\"ruleId\":\"ap_device_unreachable\"").count(), 1);
}

#[test]
fn escaping_handles_quotes_backslashes_and_newlines() {
    let snapshot = Snapshot {
        causes: Vec::new(),
        problems: vec![ProblemView {
            scope: ScopeId::GLOBAL,
            kind: ProblemKind::new("kind\"slash\\line\nend"),
            target: ScopeId::GLOBAL,
            time: EventTime::from_millis(1),
            severity: Severity::Low,
            sarif_id: SarifId::new("id"),
            cause_kinds: vec![],
            superseded: false,
        }],
        audit: Vec::new(),
    };

    let sarif = to_sarif(&snapshot);
    assert!(sarif.contains("kind\\\"slash\\\\line\\nend"));
}
