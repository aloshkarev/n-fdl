//! Property tests per `docs/idea/spec/12-testing.md` §3: deterministic
//! output (property 1), flat-memory GC (property 2), and the MAX_LOOKBACK
//! no-dangling invariant at engine level (property 9). Hand-rolled
//! deterministic pseudo-randomness — no external test deps (ADR-012 spirit).

use airpulse_dsl_evaluator::{
    CorrelateError, Engine, EngineDiagnostic, OfflineAuditSink, RunMode, Snapshot, StaticTopology,
    fixtures,
};
use airpulse_dsl_ir::{PredOp, Predicate, ProgramImage, SlotIdx};
use airpulse_dsl_store::{EventNode, EventProvenance, Limits, StoreDiagnostic};
use airpulse_dsl_types::{DurationMs, EventId, EventTime, EventType, ScopeId};

fn t(ms: i64) -> EventTime {
    EventTime::from_millis(ms)
}

fn d(ms: i64) -> DurationMs {
    DurationMs::from_millis(ms).expect("non-negative duration in test")
}

fn rtx(id: u64, time_ms: i64, scope: ScopeId, segment_size: i64) -> EventNode {
    EventNode::new(
        EventId::new(id),
        EventType::new("tcp.retransmission_burst"),
        t(time_ms),
        scope,
        vec![(fixtures::F_SEGMENT_SIZE, segment_size)],
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

/// Runs a stream through a fresh engine and extracts
/// (snapshot, diagnostics).
fn run(img: &ProgramImage, limits: Limits, stream: &[EventNode]) -> (Snapshot, Vec<EngineDiagnostic>) {
    let mut eng = Engine::new(
        img,
        StaticTopology::new(limits.max_topology_hops),
        OfflineAuditSink::new(),
        limits,
        RunMode::Offline,
    );
    for e in stream {
        eng.ingest(e.clone());
    }
    eng.finish();
    (eng.snapshot(), eng.diagnostics().to_vec())
}

/// Deterministic xorshift64* PRNG (same pattern as the store tests).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn range(&mut self, lo: i64, hi: i64) -> i64 {
        lo + (self.next() % (hi - lo).unsigned_abs()) as i64
    }
}

fn sessions() -> [ScopeId; 2] {
    [
        ScopeId::session((0x0a00_0001, 443), (0x0a00_0002, 51234)),
        ScopeId::session((0x0a00_0003, 443), (0x0a00_0004, 51235)),
    ]
}

fn slot(i: u8) -> SlotIdx {
    SlotIdx::new(i).expect("test slot index must be within MAX_SLOTS")
}

fn overflow_predicate() -> Predicate {
    Predicate {
        ops: Box::new([
            PredOp::LoadConst { imm: i64::MAX, dst: slot(0) },
            PredOp::LoadConst { imm: 1, dst: slot(1) },
            PredOp::Add { lhs: slot(0), rhs: slot(1), dst: slot(2) },
        ]),
        result: slot(2),
    }
}

#[test]
fn deterministic_output_same_stream_twice() {
    // 12 §3 property 1 / 03 §6: same (stream, image, limits) → identical
    // extracted results, including audit and diagnostics.
    let img = fixtures::rule3_pmtud();
    let [s1, s2] = sessions();
    let mut rng = Rng(0xA5A5_5A5A_DEAD_BEEF);
    let mut stream = Vec::new();
    let mut time_ms = 0_i64;
    for i in 0..300_u64 {
        time_ms += rng.range(0, 400);
        let scope = if rng.range(0, 2) == 0 { s1 } else { s2 };
        let ev = if rng.range(0, 3) == 0 {
            ptb(i, time_ms, scope)
        } else {
            rtx(i, time_ms, scope, rng.range(1000, 2000))
        };
        stream.push(ev);
    }

    let (snap_a, diag_a) = run(&img, Limits::default(), &stream);
    let (snap_b, diag_b) = run(&img, Limits::default(), &stream);
    assert_eq!(snap_a, snap_b, "identical stream → identical snapshot");
    assert_eq!(diag_a, diag_b, "identical stream → identical diagnostics");
    // Sanity: the stream actually produced graph state.
    assert!(!snap_a.causes.is_empty());
}

#[test]
fn deterministic_output_invariant_under_scope_interleaving() {
    // ADR-012 / 03 §6: emission is merged by (event_time, rule_decl_order,
    // scope_id); swapping the arrival order of *equal-time* events in
    // different partitions must not change the extracted results
    // (cross-partition scheduling invariance for a serial engine).
    let img = fixtures::rule3_pmtud();
    let [s1, s2] = sessions();

    let order_a = vec![
        rtx(1, 10_000, s1, 1500),
        rtx(2, 10_000, s2, 1600),
        ptb(3, 10_400, s1),
        ptb(4, 10_400, s2),
    ];
    let order_b = vec![
        rtx(2, 10_000, s2, 1600),
        rtx(1, 10_000, s1, 1500),
        ptb(4, 10_400, s2),
        ptb(3, 10_400, s1),
    ];

    let (snap_a, _) = run(&img, Limits::default(), &order_a);
    let (snap_b, _) = run(&img, Limits::default(), &order_b);
    assert_eq!(snap_a, snap_b, "scope interleaving must not change results");
    assert_eq!(snap_a.causes.len(), 2);
    assert!(snap_a.causes.iter().all(|c| c.confidence.value() == 85));
    assert_eq!(snap_a.problems.len(), 2);
}

#[test]
fn flat_memory_gc_bounds_ring_and_drains_pending() {
    // 12 §3 properties 2 + 9 at engine level: a long stream (10× ring
    // capacity) with an advancing watermark keeps the ring bounded by GC
    // (no capacity spills), leaves no pending after finish, and never
    // resumes against an evicted anchor (no dangling — MAX_LOOKBACK
    // invariant, 08 §6).
    let capacity = 64_usize;
    let limits = Limits {
        max_ringbuffer_events_per_scope: capacity,
        // max_lookback (1200) > max forward window (1000) — 05 §3.1; at
        // 20ms spacing GC retains ≤ 60 events < capacity, so the ring is
        // bounded by GC, not by spill.
        max_lookback: d(1_200),
        ..Limits::default()
    };
    let img = fixtures::rule3_pmtud();
    let [s1, _] = sessions();
    let mut eng = Engine::new(
        &img,
        StaticTopology::new(limits.max_topology_hops),
        OfflineAuditSink::new(),
        limits,
        RunMode::Offline,
    );

    let steps = (capacity as u64) * 10;
    for i in 0..steps {
        let time_ms = i as i64 * 20;
        eng.ingest(rtx(i, time_ms, s1, 1500));
        let len = eng.store().ring(s1).expect("ring exists").len();
        assert!(len <= capacity, "ring len {len} exceeded capacity {capacity} at step {i}");
    }
    eng.finish();

    assert_eq!(eng.store().pending_len(s1), 0, "pending drained at end-of-stream");
    assert_eq!(eng.suspended(), steps, "every anchor match suspended (forward window)");
    assert_eq!(eng.resumed(), steps, "every suspension resumed exactly once");
    for diag in eng.diagnostics() {
        assert!(
            !matches!(diag, EngineDiagnostic::MissingAnchor { .. }),
            "resume must never dangle (08 §6): {diag:?}"
        );
        assert!(
            !matches!(diag, EngineDiagnostic::Store(StoreDiagnostic::RingBufferSpill { .. })),
            "ring must be bounded by GC, not by spill: {diag:?}"
        );
    }

    // All absent-branch infers landed (dedup allows one per 1s window; the
    // stream spans 12.8s → confidence saturates at 100 well before the end).
    let snap = eng.snapshot();
    assert_eq!(snap.causes.len(), 1);
    assert_eq!(snap.causes[0].confidence.value(), 100);
}

#[test]
fn provenance_dedup_one_infer_per_window() {
    // 12 §3 property 8 at engine level: two matching anchors in the same
    // dedup window infer once (+35), not twice (03 §3.3).
    let img = fixtures::rule3_pmtud();
    let [s1, _] = sessions();
    // Both rtx events in dedup window 10 (10_000..11_000, default 1s).
    let stream = vec![rtx(1, 10_000, s1, 1500), rtx(2, 10_600, s1, 1500)];
    let (snap, _) = run(&img, Limits::default(), &stream);
    assert_eq!(snap.causes.len(), 1);
    assert_eq!(
        snap.causes[0].confidence.value(),
        35,
        "second same-window infer is a no-op (03 §3.3)"
    );
    // Both anchors fired and audited their absent-branch action, though —
    // dedup applies to the infer, not the action.
    assert_eq!(snap.audit.len(), 2);
}

#[test]
fn predicate_overflow_surfaces_engine_diagnostic_code() {
    // Runtime correlate/predicate errors are non-fatal diagnostics; overflow
    // maps to ADGL3007 (11-error-diagnostics.md runtime range).
    let mut img = fixtures::rule3_pmtud();
    img.rules[0].anchor.predicate = overflow_predicate();

    let [s1, _] = sessions();
    let mut eng = Engine::new(
        &img,
        StaticTopology::new(Limits::default().max_topology_hops),
        OfflineAuditSink::new(),
        Limits::default(),
        RunMode::Offline,
    );
    eng.ingest(rtx(1, 10_000, s1, 1500));
    eng.finish();

    let diag = eng
        .diagnostics()
        .iter()
        .find(|d| {
            matches!(
                d,
                EngineDiagnostic::PredicateError {
                    error: CorrelateError::ArithOverflow,
                    ..
                }
            )
        })
        .expect("overflow predicate should emit PredicateError");
    assert_eq!(diag.code(), Some("ADGL3007"));
}
