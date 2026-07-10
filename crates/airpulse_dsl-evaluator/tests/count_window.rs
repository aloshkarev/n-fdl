//! Evaluator tests for bounded correlate `having: count >= N`.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use airpulse_dsl_catalog::{EventOrBindingType, resolve_metric_path};
use airpulse_dsl_evaluator::schema::EVENT_FIELD_TARGET;
use airpulse_dsl_evaluator::{Engine, OfflineAuditSink, RunMode, StaticTopology, TopologyProvider};
use airpulse_dsl_ir::{FieldIdx, ProgramImage};
use airpulse_dsl_store::{EdgeEndpoint, EventNode, EventProvenance, Limits};
use airpulse_dsl_syntax::parse_ruleset;
use airpulse_dsl_types::{EventId, EventTime, EventType, ScopeId, T3};
use airpulse_dsl_verify::verify;

const RULE_MIN_30: &str = r#"
ruleset "eval.count30" {
  version = "1.0"
  requires = ["wifi-ota", "topology"]
  evidence deauth_flood {
    scope: AccessPoint
    anchor storm: event(wifi.deauth_burst) { storm.count >= 5 }
    correlate hits: event(wifi.mgmt.deauth) {
      topo: same_ap(storm.target, hits.target)
      time: hits.time in [storm.time - 1s, storm.time]
      having: count >= 30
    }
    if present(hits) {
      infer Cause(RfInterference) { target: storm.target, weight: +90, evidence: [storm, hits] }
    }
  }
}
"#;

const RULE_MIN_1: &str = r#"
ruleset "eval.count1" {
  version = "1.0"
  requires = ["wifi-ota", "topology"]
  evidence earliest {
    scope: AccessPoint
    anchor storm: event(wifi.deauth_burst)
    correlate hits: event(wifi.mgmt.deauth) {
      topo: same_ap(storm.target, hits.target)
      time: hits.time in [storm.time - 1s, storm.time]
    }
    if present(hits) {
      infer Cause(RfInterference) { target: storm.target, weight: +50, evidence: [storm, hits] }
    }
  }
}
"#;

fn t(ms: i64) -> EventTime {
    EventTime::from_millis(ms)
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

fn offline_engine(
    img: &ProgramImage,
    topo: StaticTopology,
) -> Engine<'_, StaticTopology, OfflineAuditSink> {
    Engine::new(
        img,
        topo,
        OfflineAuditSink::new(),
        Limits::default(),
        RunMode::Offline,
    )
}

fn topo_with_ap(ap: ScopeId) -> StaticTopology {
    let mut topo = StaticTopology::new(Limits::default().max_topology_hops);
    topo.relate_ap(ap, ap);
    topo
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

#[test]
fn count_threshold_requires_n_matches_for_present() {
    let img = parse_verify_image(RULE_MIN_30);
    let ap = ScopeId::access_point(0x1122_3344_5566);

    let mut eng = offline_engine(&img, topo_with_ap(ap));
    let ap_key = eng.intern_scope(ap);

    for i in 0..29 {
        eng.ingest(evt(
            10 + i,
            "wifi.mgmt.deauth",
            9_500 + i as i64 * 10,
            ap,
            vec![(EVENT_FIELD_TARGET, ap_key)],
        ));
    }
    eng.ingest(evt(
        1,
        "wifi.deauth_burst",
        10_000,
        ap,
        vec![
            (EVENT_FIELD_TARGET, ap_key),
            (field_idx("wifi.deauth_burst", "count"), 40),
        ],
    ));
    eng.finish();
    assert!(
        !eng.snapshot()
            .causes
            .iter()
            .any(|c| c.kind.as_str() == "RfInterference"),
        "29 matches must not satisfy count >= 30"
    );

    let mut eng = offline_engine(&img, topo_with_ap(ap));
    let ap_key = eng.intern_scope(ap);
    for i in 0..30 {
        eng.ingest(evt(
            100 + i,
            "wifi.mgmt.deauth",
            9_500 + i as i64 * 10,
            ap,
            vec![(EVENT_FIELD_TARGET, ap_key)],
        ));
    }
    eng.ingest(evt(
        1,
        "wifi.deauth_burst",
        10_000,
        ap,
        vec![
            (EVENT_FIELD_TARGET, ap_key),
            (field_idx("wifi.deauth_burst", "count"), 40),
        ],
    ));
    eng.finish();
    assert!(
        eng.snapshot()
            .causes
            .iter()
            .any(|c| c.kind.as_str() == "RfInterference" && c.confidence.value() == 90),
        "30 matches must satisfy count >= 30"
    );
}

#[test]
fn omitted_having_equivalent_to_min_match_one() {
    let img = parse_verify_image(RULE_MIN_1);
    assert_eq!(img.rules[0].correlates[0].min_match, 1);

    let ap = ScopeId::access_point(0xAABB_CCDD_EEFF);
    let mut eng = offline_engine(&img, topo_with_ap(ap));
    let ap_key = eng.intern_scope(ap);

    eng.ingest(evt(
        2,
        "wifi.mgmt.deauth",
        9_900,
        ap,
        vec![(EVENT_FIELD_TARGET, ap_key)],
    ));
    eng.ingest(evt(
        1,
        "wifi.deauth_burst",
        10_000,
        ap,
        vec![(EVENT_FIELD_TARGET, ap_key)],
    ));
    eng.finish();
    assert!(
        eng.snapshot()
            .causes
            .iter()
            .any(|c| c.kind.as_str() == "RfInterference")
    );
}

#[test]
fn unknown_topology_with_insufficient_matches_yields_no_infer() {
    let img = parse_verify_image(RULE_MIN_30);
    let mut eng = offline_engine(
        &img,
        StaticTopology::new(Limits::default().max_topology_hops),
    );
    let ap = ScopeId::access_point(0x0102_0304_0506);
    let ap_key = eng.intern_scope(ap);

    eng.ingest(evt(
        1,
        "wifi.deauth_burst",
        10_000,
        ap,
        vec![(EVENT_FIELD_TARGET, ap_key)],
    ));
    for i in 0..5 {
        eng.ingest(evt(
            10 + i,
            "wifi.mgmt.deauth",
            9_500 + i as i64 * 10,
            ap,
            vec![(EVENT_FIELD_TARGET, ap_key)],
        ));
    }
    eng.finish();
    assert!(
        !eng.snapshot()
            .causes
            .iter()
            .any(|c| c.kind.as_str() == "RfInterference"),
        "Unknown topology with <N matches must not infer"
    );
}

#[derive(Clone)]
struct TrueThenUnknownTopology {
    same_ap_calls: Arc<AtomicUsize>,
    true_calls: usize,
}

impl TopologyProvider for TrueThenUnknownTopology {
    fn same_session(&self, _a: ScopeId, _b: ScopeId) -> T3 {
        T3::Unknown
    }

    fn same_client(&self, _a: ScopeId, _b: ScopeId) -> T3 {
        T3::Unknown
    }

    fn same_port(&self, _a: ScopeId, _b: ScopeId) -> T3 {
        T3::Unknown
    }

    fn same_ap(&self, _a: ScopeId, _b: ScopeId) -> T3 {
        if self.same_ap_calls.fetch_add(1, Ordering::SeqCst) < self.true_calls {
            T3::True
        } else {
            T3::Unknown
        }
    }

    fn same_vlan(&self, _a: ScopeId, _b: ScopeId) -> T3 {
        T3::Unknown
    }

    fn upstream_of(&self, _up: ScopeId, _down: ScopeId) -> T3 {
        T3::Unknown
    }
}

#[test]
fn n_minus_one_true_plus_unknown_resolves_unknown_not_absent() {
    let src = r#"
ruleset "eval.count_unknown" {
  version = "1.0"
  requires = ["wifi-ota", "topology"]
  evidence uncertain {
    scope: AccessPoint
    anchor storm: event(wifi.deauth_burst)
    correlate hits: event(wifi.mgmt.deauth) {
      topo: same_ap(storm.target, hits.target)
      time: hits.time in [storm.time - 1s, storm.time]
      having: count >= 3
    }
    if absent(hits) {
      infer Cause(RfInterference) { target: storm.target, weight: +40, evidence: [storm] }
    }
  }
}
"#;
    let img = parse_verify_image(src);
    let ap = ScopeId::access_point(0x0102_0304_0506);
    let calls = Arc::new(AtomicUsize::new(0));
    let topology = TrueThenUnknownTopology {
        same_ap_calls: Arc::clone(&calls),
        true_calls: 2,
    };
    let mut eng = Engine::new(
        &img,
        topology,
        OfflineAuditSink::new(),
        Limits::default(),
        RunMode::Offline,
    );
    let ap_key = eng.intern_scope(ap);
    for i in 0..3 {
        eng.ingest(evt(
            10 + i,
            "wifi.mgmt.deauth",
            9_500 + i as i64 * 10,
            ap,
            vec![(EVENT_FIELD_TARGET, ap_key)],
        ));
    }
    eng.ingest(evt(
        1,
        "wifi.deauth_burst",
        10_000,
        ap,
        vec![(EVENT_FIELD_TARGET, ap_key)],
    ));
    eng.finish();

    assert_eq!(calls.load(Ordering::SeqCst), 3);
    assert!(
        !eng.snapshot()
            .causes
            .iter()
            .any(|cause| cause.kind.as_str() == "RfInterference"),
        "Unknown binding must not satisfy absent(hits)"
    );
}

#[test]
fn inclusive_window_boundary_counts_edge_event() {
    let src = r#"
ruleset "eval.inclusive" {
  version = "1.0"
  requires = ["wifi-ota", "topology"]
  evidence edge {
    scope: AccessPoint
    anchor storm: event(wifi.deauth_burst)
    correlate hits: event(wifi.mgmt.deauth) {
      topo: same_ap(storm.target, hits.target)
      time: hits.time in [storm.time - 1000ms, storm.time]
      having: count >= 1
    }
    if present(hits) {
      infer Cause(RfInterference) { target: storm.target, weight: +40, evidence: [storm, hits] }
    }
  }
}
"#;
    let img = parse_verify_image(src);
    let ap = ScopeId::access_point(0x0A0B_0C0D_0E0F);
    let mut eng = offline_engine(&img, topo_with_ap(ap));
    let ap_key = eng.intern_scope(ap);

    eng.ingest(evt(
        2,
        "wifi.mgmt.deauth",
        9_000,
        ap,
        vec![(EVENT_FIELD_TARGET, ap_key)],
    ));
    eng.ingest(evt(
        1,
        "wifi.deauth_burst",
        10_000,
        ap,
        vec![(EVENT_FIELD_TARGET, ap_key)],
    ));
    eng.finish();
    assert!(
        eng.snapshot()
            .causes
            .iter()
            .any(|c| c.kind.as_str() == "RfInterference")
    );
}

#[derive(Clone)]
struct CountingTopology {
    same_ap_calls: Arc<AtomicUsize>,
}

impl TopologyProvider for CountingTopology {
    fn same_session(&self, _a: ScopeId, _b: ScopeId) -> T3 {
        T3::True
    }

    fn same_client(&self, _a: ScopeId, _b: ScopeId) -> T3 {
        T3::True
    }

    fn same_port(&self, _a: ScopeId, _b: ScopeId) -> T3 {
        T3::True
    }

    fn same_ap(&self, _a: ScopeId, _b: ScopeId) -> T3 {
        self.same_ap_calls.fetch_add(1, Ordering::SeqCst);
        T3::True
    }

    fn same_vlan(&self, _a: ScopeId, _b: ScopeId) -> T3 {
        T3::True
    }

    fn upstream_of(&self, _up: ScopeId, _down: ScopeId) -> T3 {
        T3::True
    }
}

#[test]
fn count_branch_stops_topology_evaluation_at_n_true_matches() {
    let img = parse_verify_image(&RULE_MIN_30.replace("count >= 30", "count >= 3"));
    let calls = Arc::new(AtomicUsize::new(0));
    let topology = CountingTopology {
        same_ap_calls: Arc::clone(&calls),
    };
    let mut eng = Engine::new(
        &img,
        topology,
        OfflineAuditSink::new(),
        Limits::default(),
        RunMode::Offline,
    );
    let ap = ScopeId::access_point(0x1234_5678_9ABC);
    let ap_key = eng.intern_scope(ap);
    for i in 0..8 {
        eng.ingest(evt(
            100 + i,
            "wifi.mgmt.deauth",
            9_000 + i as i64 * 10,
            ap,
            vec![(EVENT_FIELD_TARGET, ap_key)],
        ));
    }
    eng.ingest(evt(
        1,
        "wifi.deauth_burst",
        10_000,
        ap,
        vec![
            (EVENT_FIELD_TARGET, ap_key),
            (field_idx("wifi.deauth_burst", "count"), 40),
        ],
    ));
    eng.finish();

    assert_eq!(
        calls.load(Ordering::SeqCst),
        3,
        "no setup topology calls occur; candidate scanning must stop at N"
    );
}

#[test]
fn count_branch_binds_earliest_true_witness_for_fields_and_evidence() {
    let src = r#"
ruleset "eval.witness" {
  version = "1.0"
  requires = ["wifi-ota", "topology"]
  evidence witness {
    scope: AccessPoint
    anchor storm: event(wifi.deauth_burst)
    correlate hits: event(wifi.mgmt.deauth) {
      topo: same_ap(storm.target, hits.target)
      time: hits.time in [storm.time - 1s, storm.time]
      having: count >= 3
    }
    if hits.reason == 101 {
      infer Cause(RfInterference) { target: storm.target, weight: +60, evidence: [storm, hits] }
    }
  }
}
"#;
    let img = parse_verify_image(src);
    let ap = ScopeId::access_point(0x1122_AABB_CCDD);
    let mut eng = offline_engine(&img, topo_with_ap(ap));
    let ap_key = eng.intern_scope(ap);
    let reason = field_idx("wifi.mgmt.deauth", "reason");
    for (id, time, value) in [
        (101, 9_100, 101),
        (102, 9_200, 102),
        (103, 9_300, 103),
        (104, 9_400, 104),
        (105, 9_500, 105),
    ] {
        eng.ingest(evt(
            id,
            "wifi.mgmt.deauth",
            time,
            ap,
            vec![(EVENT_FIELD_TARGET, ap_key), (reason, value)],
        ));
    }
    eng.ingest(evt(
        1,
        "wifi.deauth_burst",
        10_000,
        ap,
        vec![(EVENT_FIELD_TARGET, ap_key)],
    ));
    eng.finish();

    assert!(
        eng.snapshot()
            .causes
            .iter()
            .any(|cause| cause.kind.as_str() == "RfInterference"),
        "field access must read the earliest witness (reason=101), not the Nth/latest"
    );
    let part = eng.store().partition(ap).expect("AP partition");
    assert!(
        part.edges
            .iter()
            .any(|edge| edge.src == EdgeEndpoint::Event(EventId::new(101))),
        "evidence must reference the earliest true witness"
    );
    assert!(
        !part.edges.iter().any(|edge| {
            matches!(
                edge.src,
                EdgeEndpoint::Event(id)
                    if id == EventId::new(103)
                        || id == EventId::new(104)
                        || id == EventId::new(105)
            )
        }),
        "Nth and later candidates must not become evidence witnesses"
    );
}
