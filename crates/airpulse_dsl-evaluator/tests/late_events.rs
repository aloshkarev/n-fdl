//! Late-event policy tests per `docs/idea/spec/08-stream-watermarking.md` §4,
//! ADR-004, and `docs/idea/plans/test-plan.md` §8:
//! - Offline: late evidence after resolved-absent → `ADGL3002` audit, no
//!   retroactive re-infer (append-only provenance).
//! - Live: late events drop to side-output → `ADGL3003`.
//! - Idle-source watermark (live): idle partitions excluded from global min.
//! - Lookback / GC: pending anchors survive until resume (`08` §6).

use airpulse_dsl_evaluator::{
    Engine, EngineDiagnostic, OfflineAuditSink, RunMode, StaticTopology, fixtures,
};
use airpulse_dsl_store::{EventNode, EventProvenance, Limits};
use airpulse_dsl_types::{DurationMs, EventId, EventTime, EventType, ScopeId};

fn t(ms: i64) -> EventTime {
    EventTime::from_millis(ms)
}

fn d(ms: i64) -> DurationMs {
    DurationMs::from_millis(ms).expect("non-negative duration in test")
}

fn session() -> ScopeId {
    ScopeId::session((0x0a00_0001, 443), (0x0a00_0002, 51234))
}

fn session_b() -> ScopeId {
    ScopeId::session((0x0a00_0003, 443), (0x0a00_0004, 51235))
}

fn rtx(id: u64, time_ms: i64, scope: ScopeId) -> EventNode {
    EventNode::new(
        EventId::new(id),
        EventType::new("tcp.retransmission_burst"),
        t(time_ms),
        scope,
        vec![(fixtures::F_SEGMENT_SIZE, 1500)],
        EventProvenance::default(),
    )
}

fn ptb(id: u64, time_ms: i64, scope: ScopeId) -> EventNode {
    EventNode::new(
        EventId::new(id),
        EventType::new("icmp.ptb"),
        t(time_ms),
        scope,
        vec![],
        EventProvenance::default(),
    )
}

fn offline_engine(limits: Limits) -> Engine<'static, StaticTopology, OfflineAuditSink> {
    // Leak the image so the engine can borrow `'static` for the test helper.
    // Tests are short-lived; this avoids threading a local `ProgramImage`.
    let img = Box::leak(Box::new(fixtures::rule3_pmtud()));
    Engine::new(
        img,
        StaticTopology::new(limits.max_topology_hops),
        OfflineAuditSink::new(),
        limits,
        RunMode::Offline,
    )
}

fn live_engine(limits: Limits) -> Engine<'static, StaticTopology, OfflineAuditSink> {
    let img = Box::leak(Box::new(fixtures::rule3_pmtud()));
    Engine::new(
        img,
        StaticTopology::new(limits.max_topology_hops),
        OfflineAuditSink::new(),
        limits,
        RunMode::Live,
    )
}

#[test]
fn offline_late_evidence_after_absent_audits_adgl3002_without_retroactive_infer() {
    // test-plan §8 / 08 §4: absent branch resolves, then a PTB arrives that
    // would have fallen inside the correlate window → LateEvidence audit,
    // confidence stays on the absent-branch weight (+35), not present (+85).
    let mut eng = offline_engine(Limits::default());
    let s = session();

    eng.ingest(rtx(1, 10_000, s));
    assert_eq!(eng.suspended(), 1);
    eng.advance_watermark(t(11_001));
    assert_eq!(eng.resumed(), 1);

    let snap_before = eng.snapshot();
    assert_eq!(snap_before.causes.len(), 1);
    assert_eq!(snap_before.causes[0].confidence.value(), 35);

    // Late PTB: time inside [rtx-500ms, rtx+1s] but wm already past upper.
    eng.ingest(ptb(2, 10_400, s));

    let diags: Vec<_> = eng
        .diagnostics()
        .iter()
        .filter(|d| d.code() == Some("ADGL3002"))
        .collect();
    assert_eq!(
        diags.len(),
        1,
        "offline late evidence must audit ADGL3002, got: {:?}",
        eng.diagnostics()
    );
    assert!(
        matches!(
            diags[0],
            EngineDiagnostic::LateEvidence { event_type, .. }
                if event_type.as_str() == "icmp.ptb"
        ),
        "LateEvidence should name the late correlate event type"
    );

    let snap = eng.snapshot();
    assert_eq!(snap.causes.len(), 1);
    assert_eq!(
        snap.causes[0].confidence.value(),
        35,
        "append-only: must not retroactively re-apply present-branch infer"
    );
    // Late event is accepted into the ring (offline accept), not dropped.
    assert!(
        eng.store()
            .ring(s)
            .is_some_and(|r| r.iter().any(|e| e.id == EventId::new(2))),
        "offline late evidence stays in the ring"
    );
    assert!(
        eng.late_side_output().is_empty(),
        "offline does not use the live drop side-output"
    );
}

#[test]
fn offline_late_evidence_after_if_absent_true_audits_adgl3002() {
    // Review fix: record_resolved_absent must run after resolve_bindings for
    // `if absent(...)` (True = Absent), not only the `present()` else path.
    let img = Box::leak(Box::new(fixtures::rule3_pmtud_if_absent()));
    let mut eng = Engine::new(
        img,
        StaticTopology::new(Limits::default().max_topology_hops),
        OfflineAuditSink::new(),
        Limits::default(),
        RunMode::Offline,
    );
    let s = session();

    eng.ingest(rtx(1, 10_000, s));
    eng.advance_watermark(t(11_001));
    assert_eq!(eng.resumed(), 1);

    let snap_before = eng.snapshot();
    assert_eq!(snap_before.causes.len(), 1);
    assert_eq!(snap_before.causes[0].confidence.value(), 35);

    eng.ingest(ptb(2, 10_400, s));

    let diags: Vec<_> = eng
        .diagnostics()
        .iter()
        .filter(|d| d.code() == Some("ADGL3002"))
        .collect();
    assert_eq!(
        diags.len(),
        1,
        "if absent(...) True path must still record resolved-absent for ADGL3002, got: {:?}",
        eng.diagnostics()
    );
    assert_eq!(eng.snapshot().causes[0].confidence.value(), 35);
}

#[test]
fn offline_late_event_while_pending_is_accepted_without_3002() {
    // 08 §4: if pending is not yet resolved, late/out-of-order correlate
    // evidence just joins the ring until resume — no LateEvidence audit.
    let mut eng = offline_engine(Limits::default());
    let s = session();

    eng.ingest(rtx(1, 10_000, s));
    // Out-of-order relative to capture max so far? After rtx, wm=10000.
    // PTB at 9900 is ≤ wm → late by definition, but pending still open.
    eng.ingest(ptb(2, 9_900, s));

    assert!(
        eng.diagnostics()
            .iter()
            .all(|d| d.code() != Some("ADGL3002")),
        "pending-open late evidence must not emit ADGL3002"
    );
    eng.finish();
    let snap = eng.snapshot();
    // PTB at 9900 is inside [10000-500, 10000+1000] → present → +85.
    assert_eq!(snap.causes[0].confidence.value(), 85);
}

#[test]
fn live_late_event_dropped_to_side_output_adgl3003() {
    // 08 §4 live default: drop + side-output ADGL3003.
    let limits = Limits {
        max_disorder: d(0),
        allowed_lateness: d(0),
        ..Limits::default()
    };
    let mut eng = live_engine(limits);
    let s = session();

    eng.ingest(rtx(1, 10_000, s));
    assert_eq!(eng.store().watermark(), t(10_000));

    eng.ingest(ptb(2, 9_500, s));

    assert_eq!(
        eng.diagnostics()
            .iter()
            .filter(|d| d.code() == Some("ADGL3003"))
            .count(),
        1,
        "live late drop must audit ADGL3003, got: {:?}",
        eng.diagnostics()
    );
    assert_eq!(eng.late_side_output().len(), 1);
    assert_eq!(eng.late_side_output()[0].id, EventId::new(2));
    assert!(
        eng.store()
            .ring(s)
            .is_none_or(|r| !r.iter().any(|e| e.id == EventId::new(2))),
        "dropped late event must not enter the ring"
    );
}

#[test]
fn live_allowed_lateness_accepts_event_inside_grace_window() {
    let limits = Limits {
        max_disorder: d(0),
        allowed_lateness: d(1_000),
        ..Limits::default()
    };
    let mut eng = live_engine(limits);
    let s = session();

    eng.ingest(rtx(1, 10_000, s));
    // 9500 is late vs wm=10000, but within allowed_lateness=1000 (cutoff 9000).
    eng.ingest(ptb(2, 9_500, s));

    assert!(
        eng.diagnostics()
            .iter()
            .all(|d| d.code() != Some("ADGL3003")),
        "within allowed_lateness must not drop"
    );
    assert!(eng.late_side_output().is_empty());
    assert!(
        eng.store()
            .ring(s)
            .is_some_and(|r| r.iter().any(|e| e.id == EventId::new(2)))
    );
}

#[test]
fn live_idle_source_excluded_from_global_watermark_min() {
    // 08 §2.3: global_wm = min over active sources; idle sources excluded.
    let limits = Limits {
        max_disorder: d(0),
        idle_timeout: d(1_000),
        ..Limits::default()
    };
    let mut eng = live_engine(limits);
    let a = session();
    let b = session_b();

    eng.ingest(rtx(1, 10_000, a));
    eng.ingest(rtx(2, 10_000, b));
    assert_eq!(eng.store().watermark(), t(10_000));

    // Wall clock advances past idle_timeout for both; then only A produces.
    eng.advance_wall_clock(t(12_000));
    eng.ingest(rtx(3, 12_000, a));

    // B is idle → excluded. Global wm follows active source A (12000), not
    // stuck at B's stale 10000.
    assert_eq!(
        eng.store().watermark(),
        t(12_000),
        "idle source B must not pin global watermark at 10000"
    );
}

#[test]
fn lookback_gc_keeps_pending_anchor_until_resume() {
    // 08 §6 / 05 §3.1: max_lookback > max_forward ⇒ anchor survives until
    // wm > upper, then GC may reclaim after resume.
    let limits = Limits {
        max_lookback: d(1_200),
        ..Limits::default()
    };
    let mut eng = offline_engine(limits);
    let s = session();

    eng.ingest(rtx(1, 10_000, s));
    assert_eq!(eng.suspended(), 1);
    assert_eq!(eng.store().pending_len(s), 1);

    // Advance wm within lookback of the anchor but past enough that a naïve
    // GC without the invariant would be tempting — anchor must remain.
    eng.advance_watermark(t(10_500));
    assert!(
        eng.store()
            .ring(s)
            .is_some_and(|r| r.iter().any(|e| e.id == EventId::new(1))),
        "pending anchor must not be GC'd while WaitQueue entry is live"
    );
    assert_eq!(eng.store().pending_len(s), 1);

    eng.advance_watermark(t(11_001));
    assert_eq!(eng.resumed(), 1);
    assert_eq!(eng.store().pending_len(s), 0);
}
