//! Wave 6 topology T3 / ADR-010 engine locks:
//! - Unknown topology must not collapse to False / absent-branch;
//! - Unknown drives `request_topology` (`unknown_body`, C10);
//! - Live stub soft-fails control-plane `request_topology` (`NotSupported`);
//! - Cross-scope roll-up parent confidence = MAX over children (`09` §3.2).

use airpulse_dsl_evaluator::{
    Engine, LiveActionSink, OfflineAuditSink, RunMode, SinkOutcome, StaticTopology, fixtures,
    schema::EVENT_FIELD_TARGET,
};
use airpulse_dsl_store::{EventNode, EventProvenance, EvidenceEdgeKind, Limits};
use airpulse_dsl_types::{
    ActionKind, CauseKind, DurationMs, EventId, EventTime, EventType, ScopeId,
};

fn t(ms: i64) -> EventTime {
    EventTime::from_millis(ms)
}

fn session() -> ScopeId {
    ScopeId::session((0x0a00_0001, 443), (0x0a00_0002, 51234))
}

fn session_b() -> ScopeId {
    ScopeId::session((0x0a00_0003, 443), (0x0a00_0004, 51235))
}

fn rtx(id: u64, time_ms: i64, scope: ScopeId, target_key: Option<i64>) -> EventNode {
    let mut fields = vec![(fixtures::F_SEGMENT_SIZE, 1500)];
    if let Some(k) = target_key {
        fields.push((EVENT_FIELD_TARGET, k));
    }
    EventNode::new(
        EventId::new(id),
        EventType::new("tcp.retransmission_burst"),
        t(time_ms),
        scope,
        fields,
        EventProvenance::default(),
    )
}

fn ptb(id: u64, time_ms: i64, scope: ScopeId, target_key: Option<i64>) -> EventNode {
    let mut fields = vec![];
    if let Some(k) = target_key {
        fields.push((EVENT_FIELD_TARGET, k));
    }
    EventNode::new(
        EventId::new(id),
        EventType::new("icmp.ptb"),
        t(time_ms),
        scope,
        fields,
        EventProvenance::default(),
    )
}

fn offline(
    topo: StaticTopology,
) -> Engine<'static, StaticTopology, OfflineAuditSink> {
    let img = Box::leak(Box::new(fixtures::rule3_pmtud()));
    Engine::new(
        img,
        topo,
        OfflineAuditSink::new(),
        Limits::default(),
        RunMode::Offline,
    )
}

fn live(topo: StaticTopology) -> Engine<'static, StaticTopology, LiveActionSink> {
    let img = Box::leak(Box::new(fixtures::rule3_pmtud()));
    Engine::new(
        img,
        topo,
        LiveActionSink::new(),
        Limits {
            max_disorder: DurationMs::from_millis(0).unwrap(),
            allowed_lateness: DurationMs::from_millis(0).unwrap(),
            ..Limits::default()
        },
        RunMode::Live,
    )
}

#[test]
fn unknown_topo_drives_request_topology_not_absent_branch() {
    // Candidate PTB exists, but same_session(rtx.target, ptb.target) is
    // Unknown (targets never declared) → Binding::Unknown → present() Unknown
    // → unknown_body request_topology. Must NOT take else (False/absent):
    // no +35 infer, no request_observation (ADR-010).
    let mut eng = offline(StaticTopology::new(16));
    let s = session();
    let ta = ScopeId::vlan(11);
    let tb = ScopeId::vlan(22);
    let ka = eng.intern_scope(ta);
    let kb = eng.intern_scope(tb);

    eng.ingest(rtx(1, 10_000, s, Some(ka)));
    eng.ingest(ptb(2, 10_400, s, Some(kb)));
    eng.finish();

    let snap = eng.snapshot();
    assert!(
        snap.causes.is_empty(),
        "Unknown must not collapse to absent (+35) or present (+85); causes={:?}",
        snap.causes
    );
    assert_eq!(
        snap.audit.len(),
        1,
        "only request_topology expected, audit={:?}",
        snap.audit
    );
    assert_eq!(snap.audit[0].intent.kind, ActionKind::RequestTopology);
    assert_eq!(snap.audit[0].code, None);
    assert!(
        !snap
            .audit
            .iter()
            .any(|e| e.intent.kind == ActionKind::RequestObservation),
        "False/absent else-branch must not run under Unknown"
    );
}

#[test]
fn known_unrelated_topo_takes_absent_else_not_request_topology() {
    // Contrast: both targets known but unrelated → topo False → Absent →
    // else-branch (+35 + request_observation), not request_topology.
    let ta = ScopeId::vlan(11);
    let tb = ScopeId::vlan(22);
    let mut topo = StaticTopology::new(16);
    topo.declare(ta).declare(tb);

    let mut eng = offline(topo);
    let s = session();
    let ka = eng.intern_scope(ta);
    let kb = eng.intern_scope(tb);

    eng.ingest(rtx(1, 10_000, s, Some(ka)));
    eng.ingest(ptb(2, 10_400, s, Some(kb)));
    eng.finish();

    let snap = eng.snapshot();
    let session_cause = snap
        .causes
        .iter()
        .find(|c| c.scope == s)
        .expect("absent-branch infer on session partition");
    assert_eq!(session_cause.confidence.value(), 35);
    assert_eq!(session_cause.target, ta);
    assert!(
        snap.audit
            .iter()
            .any(|e| e.intent.kind == ActionKind::RequestObservation),
        "absent else should request_observation, audit={:?}",
        snap.audit
    );
    assert!(
        snap.audit
            .iter()
            .all(|e| e.intent.kind != ActionKind::RequestTopology),
        "False topo must not take unknown_body"
    );
}

#[test]
fn live_unknown_request_topology_soft_fails_not_supported() {
    let mut eng = live(StaticTopology::new(16));
    let s = session();
    let ta = ScopeId::vlan(11);
    let tb = ScopeId::vlan(22);
    let ka = eng.intern_scope(ta);
    let kb = eng.intern_scope(tb);

    eng.ingest(rtx(1, 10_000, s, Some(ka)));
    eng.ingest(ptb(2, 10_400, s, Some(kb)));
    eng.finish();

    assert!(eng.graph_snapshot().causes.is_empty());
    assert_eq!(eng.sink().entries().len(), 1);
    assert_eq!(
        eng.sink().entries()[0].intent.kind,
        ActionKind::RequestTopology
    );
    assert_eq!(eng.sink().outcomes(), &[SinkOutcome::NotSupported]);
}

#[test]
fn cross_scope_rollup_parent_confidence_is_max() {
    // Two session children infer PmtudBlackhole targeting the same VLAN with
    // different weights; parent VLAN confidence must be MAX (09 §3.2 / ADR-003).
    let vlan = ScopeId::vlan(42);
    let mut eng = offline(StaticTopology::new(16));
    let kv = eng.intern_scope(vlan);
    let s1 = session();
    let s2 = session_b();

    // Child A: present path → +85 at session, rolls up to vlan.
    eng.ingest(rtx(1, 10_000, s1, Some(kv)));
    eng.ingest(ptb(2, 10_400, s1, Some(kv)));
    eng.advance_watermark(t(11_001));

    // Child B: absent path → +35 at session, rolls up; MAX stays 85.
    eng.ingest(rtx(3, 20_000, s2, Some(kv)));
    eng.finish();

    let snap = eng.snapshot();
    let parent = snap
        .causes
        .iter()
        .find(|c| c.scope == vlan && c.target == vlan)
        .expect("rolled-up cause on VLAN parent");
    assert_eq!(
        parent.confidence.value(),
        85,
        "parent confidence must be MAX(85, 35), snap={:?}",
        snap.causes
    );

    let part = eng
        .store()
        .partition(vlan)
        .expect("vlan partition after roll-up");
    assert!(
        part.edges
            .iter()
            .any(|e| e.kind == EvidenceEdgeKind::RollsUp),
        "RollsUp provenance edge required on parent"
    );
    assert!(
        part.causes
            .contains_key(&(CauseKind::new("PmtudBlackhole"), vlan)),
        "parent cause key (PmtudBlackhole, vlan)"
    );
}
