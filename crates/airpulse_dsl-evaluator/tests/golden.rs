//! Golden acceptance tests (M0–M1 gate) per `docs/idea/spec/12-testing.md`
//! §3.1–§3.4 against the hand-coded Example 01 / Example 07 images.

#[path = "golden/_stubs/stubs.rs"]
mod stubs;

use airpulse_dsl_evaluator::schema::EVENT_FIELD_TARGET;
use airpulse_dsl_evaluator::{
    Engine, OfflineAuditSink, RunMode, StaticTopology, TopologyDiagnostic, fixtures, to_sarif,
};
use airpulse_dsl_ir::FieldIdx;
use airpulse_dsl_store::{EventNode, EventProvenance, Limits};
use airpulse_dsl_types::{ActionKind, EventId, EventTime, EventType, ScopeId, Severity};

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

fn pmtud_engine(img: &airpulse_dsl_ir::ProgramImage) -> OfflineEngine<'_> {
    Engine::new(
        img,
        StaticTopology::new(Limits::default().max_topology_hops),
        OfflineAuditSink::new(),
        Limits::default(),
        RunMode::Offline,
    )
}

// ─── G01 — PMTUD blackhole (12 §3.1–3.3) ─────────────────────────────────

#[test]
fn g01_present_ptb_in_window_confirms_and_emits_problem() {
    // 12 §3.1: tcp.retransmission_burst(segment_size > 1400) followed by
    // icmp.ptb in-window → present branch (+85); decision emits the Problem
    // once the 80-threshold is crossed.
    let img = fixtures::rule3_pmtud();
    let mut eng = pmtud_engine(&img);
    let s = session_scope();

    eng.ingest(rtx(1, 10_000, s, 1500));
    // Forward window [10_000, 11_000] not closed (upper 11_000 > wm 10_000)
    // → suspended (08 §3.1).
    assert_eq!(eng.store().pending_len(s), 1);
    assert_eq!(eng.suspended(), 1);

    // PTB inside [rtx.time - 500ms, rtx.time + 1s]; same partition → the
    // same_session(rtx.target, ptb.target) call sees identical targets.
    eng.ingest(evt(2, "icmp.ptb", 10_400, s, vec![]));
    // Still pending: wm = 10_400 ≤ upper.
    assert_eq!(eng.store().pending_len(s), 1);

    // End-of-stream flush (08 §3.4) closes the window.
    eng.finish();
    assert_eq!(eng.resumed(), 1);
    assert_eq!(eng.store().pending_len(s), 0);

    let snap = eng.snapshot();
    // Exact confidence: 0 + 85 (03 §3.3).
    assert_eq!(snap.causes.len(), 1);
    let cause = &snap.causes[0];
    assert_eq!(cause.kind.as_str(), "PmtudBlackhole");
    assert_eq!(cause.target, s);
    assert_eq!(cause.confidence.value(), 85);
    assert_eq!(cause.time, t(10_000), "Cause.time = first-infer anchor time (04 §3)");

    // Decision fired on ConfidenceMutation (85 ≥ 80, 03 §3.5).
    assert_eq!(snap.problems.len(), 1);
    let p = &snap.problems[0];
    assert_eq!(p.kind.as_str(), "XlIcmpTcpMss");
    assert_eq!(p.sarif_id.as_str(), "l3_pmtud_blackhole");
    assert_eq!(p.severity, Severity::High);
    assert_eq!(p.target, s, "omitted target = rule scope (03 §3.4)");
    assert!(!p.superseded);
    // time = emission watermark (03 §3.4) = flush watermark
    // last_event 10_400 + image max_forward 1_000 + 1.
    assert_eq!(p.time, t(11_401));

    // Present branch emits no actions.
    assert!(snap.audit.is_empty());
    assert!(eng.diagnostics().is_empty());
}

#[test]
fn g01_absent_ptb_fires_absent_branch_after_window_closes() {
    // 12 §3.2/§3.3 (WaitQueue correctness): retrans without PTB → after the
    // watermark passes rtx.time + 1s, the absent branch fires (+35) and
    // request_observation is audited as ADGL3001.
    let img = fixtures::rule3_pmtud();
    let mut eng = pmtud_engine(&img);
    let s = session_scope();

    eng.ingest(rtx(1, 10_000, s, 1500));
    // Suspend actually happened: upper (11_000) > wm-at-ingest (10_000).
    assert_eq!(eng.store().pending_len(s), 1);
    assert_eq!(eng.suspended(), 1);
    assert_eq!(eng.resumed(), 0);

    // Boundary check (08 §3.2): wm == upper must NOT resume.
    eng.advance_watermark(t(11_000));
    assert_eq!(eng.store().pending_len(s), 1, "wm == upper: window not provably closed");
    assert_eq!(eng.resumed(), 0);

    // Strictly past the upper bound → resume.
    eng.advance_watermark(t(11_001));
    assert_eq!(eng.store().pending_len(s), 0);
    assert_eq!(eng.resumed(), 1, "resumed exactly when wm > upper (08 §3.2)");

    let snap = eng.snapshot();
    assert_eq!(snap.causes.len(), 1);
    assert_eq!(snap.causes[0].confidence.value(), 35, "absent branch weight +35");
    assert!(snap.problems.is_empty(), "35 < 80: no verdict");

    // request_observation audited as ADGL3001 ActionNoOpInReplay (07 §7).
    assert_eq!(snap.audit.len(), 1);
    let entry = &snap.audit[0];
    assert_eq!(entry.code, Some("ADGL3001"));
    assert_eq!(entry.intent.kind, ActionKind::RequestObservation);
    assert_eq!(entry.intent.rule.as_str(), "pmtud_hypothesis");
    assert_eq!(entry.intent.arg.as_ref().map(|a| a.as_str()), Some("icmp.visibility"));
    assert_eq!(
        entry.intent.target_path.as_ref().map(|p| p.as_str()),
        Some("rtx.path"),
        "raw target path preserved for audit"
    );
    assert_eq!(entry.wm, t(11_001), "audited at the resume watermark");
}

#[test]
fn g01_anchor_predicate_rejects_small_segments() {
    // Anchor predicate rtx.segment_size > 1400 (03 §3.1): a 1200-byte burst
    // never matches — nothing suspends, nothing is inferred.
    let img = fixtures::rule3_pmtud();
    let mut eng = pmtud_engine(&img);
    let s = session_scope();

    eng.ingest(rtx(1, 10_000, s, 1200));
    assert_eq!(eng.store().pending_len(s), 0);
    assert_eq!(eng.suspended(), 0);
    eng.finish();
    let snap = eng.snapshot();
    assert!(snap.causes.is_empty());
    assert!(snap.problems.is_empty());
    assert!(snap.audit.is_empty());
}

#[test]
fn c10_topology_unknown_fires_unknown_branch_request_topology() {
    // 03 §3.7: Unknown binding → neither then nor else; unknown_body fires
    // (request_topology), no infer at all (C10 — Unknown is not false).
    let img = fixtures::rule3_pmtud();
    let mut eng = pmtud_engine(&img); // empty topology: unknown scopes
    let s = session_scope();
    // A PTB whose target is a different, topology-unknown scope: the
    // same_session(rtx.target = s, ptb.target = other) call yields Unknown.
    let other = ScopeId::session((0x0a00_0009, 80), (0x0a00_000a, 40000));
    let other_key = eng.intern_scope(other);

    eng.ingest(rtx(1, 10_000, s, 1500));
    eng.ingest(evt(2, "icmp.ptb", 10_400, s, vec![(EVENT_FIELD_TARGET, other_key)]));
    eng.finish();

    let snap = eng.snapshot();
    assert!(snap.causes.is_empty(), "Unknown skips then AND else bodies (03 §3.7)");
    assert!(snap.problems.is_empty());
    assert_eq!(snap.audit.len(), 1);
    assert_eq!(snap.audit[0].intent.kind, ActionKind::RequestTopology);
    assert_eq!(snap.audit[0].code, None, "request_topology is a plain audit record");
}

// ─── G05 — suppression + topology cycle (12 §3.4) ────────────────────────

fn routers() -> (ScopeId, ScopeId) {
    (ScopeId::port(1, 1), ScopeId::port(2, 1))
}

fn suppression_engine(
    img: &airpulse_dsl_ir::ProgramImage,
    topo: StaticTopology,
) -> (OfflineEngine<'_>, i64, i64) {
    let mut eng = Engine::new(img, topo, OfflineAuditSink::new(), Limits::default(), RunMode::Offline);
    let (r1, r2) = routers();
    let k1 = eng.intern_scope(r1);
    let k2 = eng.intern_scope(r2);
    (eng, k1, k2)
}

#[test]
fn g05_upstream_failure_supersedes_downstream_problem() {
    // 12 §3.4 golden suppression: the stub emits DeviceUnreachable for the
    // upstream router, then for the downstream one; rule 8 marks the
    // downstream Problem superseded (C7 append-only flag).
    let img = fixtures::rule8_suppression(stubs::device_unreachable_stub_rules());
    let (r1, r2) = routers();
    let mut topo = StaticTopology::new(Limits::default().max_topology_hops);
    topo.upstream_edge(r1, r2); // r1 directly upstream of r2
    let (mut eng, k1, k2) = suppression_engine(&img, topo);

    eng.ingest(evt(1, stubs::STUB_EVENT_TYPE, 1_000, ScopeId::GLOBAL, vec![(EVENT_FIELD_TARGET, k1)]));
    eng.ingest(evt(2, stubs::STUB_EVENT_TYPE, 2_000, ScopeId::GLOBAL, vec![(EVENT_FIELD_TARGET, k2)]));
    eng.finish();

    let snap = eng.snapshot();
    // Stub path: two DeviceDown causes at +100.
    assert_eq!(snap.causes.len(), 2);
    assert!(snap.causes.iter().all(|c| c.kind.as_str() == "DeviceDown"));
    assert!(snap.causes.iter().all(|c| c.confidence.value() == 100));

    // Two problems; only the downstream one (r2) superseded.
    assert_eq!(snap.problems.len(), 2);
    let by_target = |target: ScopeId| {
        snap.problems.iter().find(|p| p.target == target).expect("problem for target")
    };
    assert!(!by_target(r1).superseded, "upstream problem stays live");
    assert!(by_target(r2).superseded, "downstream problem masked (C7 superseded flag)");
    assert!(snap.problems.iter().all(|p| p.kind.as_str() == "DeviceUnreachable"));

    // suppress_symptom audited with the example's reason string.
    let suppress: Vec<_> = snap
        .audit
        .iter()
        .filter(|e| e.intent.kind == ActionKind::SuppressSymptom)
        .collect();
    assert_eq!(suppress.len(), 1);
    assert_eq!(suppress[0].intent.rule.as_str(), "suppress_downstream");
    assert_eq!(suppress[0].intent.target, Some(r2));
    assert_eq!(
        suppress[0].intent.reason.as_deref(),
        Some("Masked by upstream topology failure")
    );
    assert!(eng.topo().diagnostics().is_empty(), "acyclic topology: no ADGL3006");

    // SARIF should only include the live (non-superseded) upstream problem.
    let sarif = to_sarif(&snap);
    assert_eq!(sarif.matches("\"ruleId\":\"ap_device_unreachable\"").count(), 1);
}

#[test]
fn g05_topology_cycle_isolates_no_suppression_with_diagnostic() {
    // 12 §3.4 topology-cycle isolation: circular upstream_of graph → no
    // panic, upstream_of resolves False (never upstream), no suppression,
    // ADGL3006 diagnostic present.
    let img = fixtures::rule8_suppression(stubs::device_unreachable_stub_rules());
    let (r1, r2) = routers();
    let mut topo = StaticTopology::new(Limits::default().max_topology_hops);
    topo.upstream_edge(r1, r2).upstream_edge(r2, r1); // cycle
    let (mut eng, k1, k2) = suppression_engine(&img, topo);

    eng.ingest(evt(1, stubs::STUB_EVENT_TYPE, 1_000, ScopeId::GLOBAL, vec![(EVENT_FIELD_TARGET, k1)]));
    eng.ingest(evt(2, stubs::STUB_EVENT_TYPE, 2_000, ScopeId::GLOBAL, vec![(EVENT_FIELD_TARGET, k2)]));
    eng.finish();

    let snap = eng.snapshot();
    assert_eq!(snap.problems.len(), 2);
    assert!(
        snap.problems.iter().all(|p| !p.superseded),
        "cycle isolation: nothing suppressed"
    );
    assert!(
        !snap.audit.iter().any(|e| e.intent.kind == ActionKind::SuppressSymptom),
        "no suppress action emitted"
    );
    let diags = eng.topo().diagnostics();
    assert!(!diags.is_empty(), "cycle diagnostic present");
    assert!(diags.iter().all(|d| d.code() == "ADGL3006"));
    assert!(diags.iter().any(|d| matches!(d, TopologyDiagnostic::UpstreamCycle { .. })));
}
