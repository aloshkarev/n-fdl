//! Integration tests for `airpulse_dsl-store` against the runtime spec:
//! `07-runtime.md` §3–§4, `08-stream-watermarking.md` §3–§4/§6,
//! `03-semantics.md` §3.3–3.4, ADR-011, ADR-012.

use airpulse_dsl_ir::{FieldIdx, PendingMatch, ProvKey};
use airpulse_dsl_store::{
    EventNode, EventProvenance, GraphStore, Limits, RingBuffer, RuntimeProvKey, StoreDiagnostic,
    SubGraph, WaitQueue, window_id,
};
use airpulse_dsl_types::{
    CauseKind, DurationMs, EventId, EventTime, EventType, ProblemKind, RuleId, ScopeId,
};

fn t(ms: i64) -> EventTime {
    EventTime::from_millis(ms)
}

fn d(ms: i64) -> DurationMs {
    DurationMs::from_millis(ms).expect("non-negative duration in test")
}

fn scope() -> ScopeId {
    ScopeId::vlan(100)
}

fn evt(id: u64, time_ms: i64, sc: ScopeId) -> EventNode {
    EventNode::new(
        EventId::new(id),
        EventType::new("tcp.retransmission_burst"),
        t(time_ms),
        sc,
        vec![(FieldIdx(0), 1400)],
        EventProvenance::default(),
    )
}

#[test]
fn event_int_lists_are_sorted_deduplicated_and_bounded() {
    let event = EventNode::new(
        EventId::new(99),
        EventType::new("wifi.deauth_burst"),
        t(100),
        ScopeId::access_point(1),
        vec![(FieldIdx(2), 70)],
        EventProvenance::default(),
    )
    .with_int_list_field(FieldIdx(4), (0..70).rev().chain([2, 1, 2]).collect());

    assert_eq!(event.field(FieldIdx(2)), Some(70));
    let clients = event
        .int_list_field(FieldIdx(4))
        .expect("client_macs sidecar");
    assert_eq!(clients.len(), airpulse_dsl_store::MAX_EVENT_INT_LIST_VALUES);
    assert!(clients.windows(2).all(|w| w[0] < w[1]));
    assert_eq!(clients[0], 0);
    assert_eq!(
        clients[airpulse_dsl_store::MAX_EVENT_INT_LIST_VALUES - 1],
        (airpulse_dsl_store::MAX_EVENT_INT_LIST_VALUES - 1) as i64
    );
}

#[test]
fn ring_clone_preserves_int_list_sidecar() {
    let event = EventNode::new(
        EventId::new(100),
        EventType::new("wifi.deauth_burst"),
        t(200),
        ScopeId::access_point(2),
        vec![],
        EventProvenance::default(),
    )
    .with_int_list_field(FieldIdx(4), vec![3, 1, 3, 2]);
    let cloned = event.clone();
    assert_eq!(
        cloned.int_list_fields(),
        &[(FieldIdx(4), vec![1, 2, 3].into_boxed_slice())]
    );
    let mut ring = RingBuffer::new(ScopeId::access_point(2), 2);
    assert!(ring.push(cloned).is_none());
    assert_eq!(
        ring.iter()
            .next()
            .and_then(|e| e.int_list_field(FieldIdx(4))),
        Some(&[1, 2, 3][..])
    );
}

fn pending(rule: &str, anchor: u64, upper_ms: i64, sc: ScopeId) -> PendingMatch {
    PendingMatch {
        rule: RuleId::new(rule),
        anchor_event: EventId::new(anchor),
        upper_bound: t(upper_ms),
        scope: sc,
    }
}

// ─── RingBuffer ─────────────────────────────────────────────────────────

#[test]
fn ring_push_keeps_time_order_including_late_events() {
    // 07 §3: buf sorted by time; 08 §4 offline: late (out-of-order) events
    // are accepted and land at their time position.
    let mut ring = RingBuffer::new(scope(), 16);
    assert!(ring.push(evt(1, 100, scope())).is_none());
    assert!(ring.push(evt(2, 300, scope())).is_none());
    // Late event: time 200 arrives after 300 was buffered.
    assert!(ring.push(evt(3, 200, scope())).is_none());
    // Equal-time late event keeps arrival order (deterministic tie-break).
    assert!(ring.push(evt(4, 200, scope())).is_none());

    let order: Vec<(u64, i64)> = ring.iter().map(|e| (e.id.raw(), e.time.millis())).collect();
    assert_eq!(order, vec![(1, 100), (3, 200), (4, 200), (2, 300)]);
}

#[test]
fn ring_capacity_spill_drops_oldest_and_signals_adgl3005() {
    // ADR-011 spill policy: drop oldest (lowest time) + ADGL3005.
    let mut ring = RingBuffer::new(scope(), 2);
    assert!(ring.push(evt(1, 100, scope())).is_none());
    assert!(ring.push(evt(2, 200, scope())).is_none());

    let spill = ring
        .push(evt(3, 300, scope()))
        .expect("full ring must spill");
    assert_eq!(spill.code(), "ADGL3005");
    match spill {
        StoreDiagnostic::RingBufferSpill {
            scope: sc,
            dropped,
            capacity,
        } => {
            assert_eq!(sc, scope());
            assert_eq!(dropped, EventId::new(1), "oldest (lowest time) is dropped");
            assert_eq!(capacity, 2);
        }
        other => panic!("expected RingBufferSpill, got {other:?}"),
    }
    assert_eq!(ring.len(), 2);
    let ids: Vec<u64> = ring.iter().map(|e| e.id.raw()).collect();
    assert_eq!(ids, vec![2, 3]);
}

#[test]
fn ring_window_scan_is_inclusive_at_both_boundaries() {
    // 05 §3.2 (D4) via 03 §3.2: correlate window
    // [anchor.time - back, anchor.time + fwd] is inclusive at BOTH ends;
    // events one ms outside either bound are excluded.
    let mut ring = RingBuffer::new(scope(), 16);
    for (id, ms) in [(1, 99), (2, 100), (3, 150), (4, 200), (5, 201)] {
        assert!(ring.push(evt(id, ms, scope())).is_none());
    }
    // anchor = 150, back = 50, fwd = 50 → window [100, 200].
    let hits: Vec<u64> = ring
        .scan_window(t(150), d(50), d(50))
        .map(|e| e.id.raw())
        .collect();
    assert_eq!(
        hits,
        vec![2, 3, 4],
        "100 and 200 inclusive; 99 and 201 excluded"
    );
}

#[test]
fn ring_window_scan_yields_events_earliest_first() {
    // 03 §3.2: matches are earliest-time-first (binding takes matches[0]).
    let mut ring = RingBuffer::new(scope(), 16);
    assert!(ring.push(evt(1, 300, scope())).is_none());
    assert!(ring.push(evt(2, 100, scope())).is_none()); // late
    assert!(ring.push(evt(3, 200, scope())).is_none()); // late
    let hits: Vec<i64> = ring
        .scan_window(t(200), d(200), d(200))
        .map(|e| e.time.millis())
        .collect();
    assert_eq!(hits, vec![100, 200, 300]);
}

#[test]
fn ring_gc_evicts_strictly_older_than_cutoff() {
    // 07 §4 pseudocode: evict while front.time < watermark - max_lookback;
    // an event exactly AT the cutoff stays.
    let mut ring = RingBuffer::new(scope(), 16);
    for (id, ms) in [(1, 100), (2, 200), (3, 300)] {
        assert!(ring.push(evt(id, ms, scope())).is_none());
    }
    // wm = 500, lookback = 300 → cutoff = 200: evict 100, keep 200 and 300.
    let evicted = ring.gc(t(500), d(300));
    assert_eq!(evicted, 1);
    let ids: Vec<u64> = ring.iter().map(|e| e.id.raw()).collect();
    assert_eq!(
        ids,
        vec![2, 3],
        "event at exactly the cutoff (200) survives"
    );
}

// ─── WaitQueue ──────────────────────────────────────────────────────────

#[test]
fn waitqueue_pop_expired_pops_smallest_upper_bound_first() {
    // 07 §8: min-heap on upper_bound; PendingMatch Ord is ascending, so the
    // queue's Reverse wrapping must surface the nearest deadline first
    // (this doubles as the PendingMatch min-heap ordering test).
    let mut wq = WaitQueue::new(scope(), 8);
    assert!(wq.push(pending("r_c", 3, 300, scope())).is_none());
    assert!(wq.push(pending("r_a", 1, 100, scope())).is_none());
    assert!(wq.push(pending("r_b", 2, 200, scope())).is_none());

    let popped = wq.pop_expired(t(1_000));
    let uppers: Vec<i64> = popped.iter().map(|p| p.upper_bound.millis()).collect();
    assert_eq!(uppers, vec![100, 200, 300]);
    assert!(wq.is_empty());
}

#[test]
fn waitqueue_boundary_wm_equal_upper_does_not_pop_wm_greater_pops() {
    // 08 §3.2: resume STRICTLY when wm > upper (pop while upper_bound < wm).
    // wm == upper must NOT pop — the window is not yet provably closed.
    let mut wq = WaitQueue::new(scope(), 8);
    assert!(wq.push(pending("r", 1, 500, scope())).is_none());

    assert!(
        wq.pop_expired(t(499)).is_empty(),
        "wm < upper: still suspended"
    );
    assert!(
        wq.pop_expired(t(500)).is_empty(),
        "wm == upper: must NOT pop (08 §3.2)"
    );
    assert_eq!(wq.len(), 1);

    let popped = wq.pop_expired(t(501));
    assert_eq!(popped.len(), 1, "wm > upper: pops");
    assert_eq!(popped[0].upper_bound, t(500));
}

#[test]
fn waitqueue_capacity_spill_drops_largest_upper_bound() {
    // ADR-011 / 08 §3.3: overflow drops the pending with the LARGEST
    // upper_bound (least urgent) + ADGL3004.
    let mut wq = WaitQueue::new(scope(), 2);
    assert!(wq.push(pending("r1", 1, 100, scope())).is_none());
    assert!(wq.push(pending("r2", 2, 900, scope())).is_none());

    // New entry (500) is more urgent than the stored max (900): 900 spills.
    let spill = wq
        .push(pending("r3", 3, 500, scope()))
        .expect("full queue must spill");
    assert_eq!(spill.code(), "ADGL3004");
    match &spill {
        StoreDiagnostic::WaitQueueSpill {
            dropped, capacity, ..
        } => {
            assert_eq!(dropped.upper_bound, t(900), "least urgent is dropped");
            assert_eq!(*capacity, 2);
        }
        other => panic!("expected WaitQueueSpill, got {other:?}"),
    }
    assert_eq!(wq.len(), 2);

    // New entry (950) is itself the least urgent: it is the one dropped.
    let spill = wq.push(pending("r4", 4, 950, scope())).expect("must spill");
    match &spill {
        StoreDiagnostic::WaitQueueSpill { dropped, .. } => {
            assert_eq!(
                dropped.upper_bound,
                t(950),
                "incoming least-urgent entry is dropped"
            );
        }
        other => panic!("expected WaitQueueSpill, got {other:?}"),
    }
    let kept: Vec<i64> = wq
        .pop_expired(t(10_000))
        .iter()
        .map(|p| p.upper_bound.millis())
        .collect();
    assert_eq!(kept, vec![100, 500]);
}

// ─── GraphStore ─────────────────────────────────────────────────────────

#[test]
fn watermark_is_monotone_and_smaller_t_is_a_noop() {
    // 07 §10 / ADR-012: single AtomicI64 fetch_max — never regresses.
    let store = GraphStore::new(Limits::default());
    assert_eq!(store.advance_watermark(t(100)), t(100));
    assert_eq!(store.watermark(), t(100));
    assert_eq!(
        store.advance_watermark(t(50)),
        t(100),
        "smaller t is a no-op"
    );
    assert_eq!(store.watermark(), t(100));
    assert_eq!(store.advance_watermark(t(200)), t(200));
    assert_eq!(store.watermark(), t(200));
}

#[test]
fn store_pop_expired_respects_strict_boundary_across_scopes() {
    // 08 §3.2 at the store level, plus ADR-012: result ordering is
    // deterministic (sorted by PendingMatch order) regardless of shard
    // iteration order.
    let store = GraphStore::new(Limits::default());
    let s1 = ScopeId::vlan(1);
    let s2 = ScopeId::vlan(2);
    assert!(store.suspend(pending("r1", 1, 100, s1)).is_none());
    assert!(store.suspend(pending("r2", 2, 100, s2)).is_none());
    assert!(store.suspend(pending("r3", 3, 200, s1)).is_none());

    assert!(
        store.pop_expired(t(100)).is_empty(),
        "wm == upper: nothing pops"
    );
    let popped = store.pop_expired(t(101));
    assert_eq!(popped.len(), 2, "both upper=100 entries pop at wm=101");
    assert!(
        popped.windows(2).all(|w| w[0] <= w[1]),
        "deterministic sorted order"
    );
    assert_eq!(store.pending_len(s1), 1);
}

/// Deterministic xorshift64* PRNG — keeps the property test dependency-free
/// and reproducible (ADR-012 spirit).
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

#[test]
fn gc_never_evicts_events_an_unexpired_pending_could_reference() {
    // Property (08 §6 GC invariant): with max_lookback > max window forward,
    // every unexpired PendingMatch's anchor event is still in the ring after
    // gc(wm, max_lookback). Random events + pendings, advancing watermark.
    let max_forward = 1_000_i64;
    let max_lookback = d(1_500); // > max_forward (MAX_LOOKBACK invariant, 05 §3.1)
    let sc = scope();
    let limits = Limits {
        max_ringbuffer_events_per_scope: 4096,
        max_pending_per_scope: 4096,
        ..Limits::default()
    };
    let store = GraphStore::new(limits);
    let mut rng = Rng(0xDEAD_BEEF_CAFE_F00D);

    let mut next_id = 0_u64;
    let mut time_ms = 0_i64;
    for _step in 0..2_000 {
        time_ms += rng.range(0, 50); // non-decreasing arrival time
        next_id += 1;
        let anchor_time = time_ms;
        assert!(store.push_event(evt(next_id, anchor_time, sc)).is_none());

        // Some events become suspended anchors with a random forward window.
        if rng.range(0, 4) == 0 {
            let upper = anchor_time + rng.range(1, max_forward);
            assert!(store.suspend(pending("r", next_id, upper, sc)).is_none());
        }

        let wm = store.advance_watermark(t(time_ms));
        // Resume (drop) expired pendings first, then GC — 07 §5 order.
        let _resumed = store.pop_expired(wm);
        store.gc(wm, max_lookback);

        // Invariant: every still-suspended pending's anchor is in the ring.
        // Inspect suspended anchors via a full drain-and-restore sweep at a
        // sentinel watermark that pops everything, then re-suspend.
        let ring = store.ring(sc).expect("ring exists");
        let probe = store.pop_expired(t(i64::MAX));
        for p in &probe {
            assert!(
                ring.get(p.anchor_event).is_some(),
                "pending anchor {:?} (upper {}) evicted at wm {} — GC invariant violated",
                p.anchor_event,
                p.upper_bound.millis(),
                wm.millis(),
            );
        }
        drop(ring);
        for p in probe {
            assert!(store.suspend(p).is_none());
        }
    }
}

#[test]
fn flat_memory_ring_stays_bounded_under_10x_capacity_push() {
    // 07 §4 / 12 §property "flat memory": pushing 10x capacity through one
    // scope with an advancing watermark never grows the ring past capacity.
    let capacity = 64_usize;
    let limits = Limits {
        max_ringbuffer_events_per_scope: capacity,
        ..Limits::default()
    };
    let store = GraphStore::new(limits);
    let sc = scope();

    for i in 0..(capacity as u64 * 10) {
        let time_ms = i as i64 * 10;
        // Spills are allowed here (that's the bound working); just push.
        let _spill = store.push_event(evt(i, time_ms, sc));
        let wm = store.advance_watermark(t(time_ms));
        store.gc(wm, store.limits().max_lookback);
        let len = store.ring(sc).expect("ring exists").len();
        assert!(
            len <= capacity,
            "ring len {len} exceeded capacity {capacity} at step {i}"
        );
    }
}

#[test]
fn store_routes_ring_and_waitqueue_spill_diagnostics() {
    // ADR-011: spills surface as returned StoreDiagnostic values through the
    // GraphStore entry points (push_event → ADGL3005, suspend → ADGL3004).
    let limits = Limits {
        max_ringbuffer_events_per_scope: 1,
        max_pending_per_scope: 1,
        ..Limits::default()
    };
    let store = GraphStore::new(limits);
    let sc = scope();

    assert!(store.push_event(evt(1, 100, sc)).is_none());
    let spill = store
        .push_event(evt(2, 200, sc))
        .expect("full ring must spill via store");
    assert_eq!(spill.code(), "ADGL3005");
    match spill {
        StoreDiagnostic::RingBufferSpill {
            scope: s,
            dropped,
            capacity,
        } => {
            assert_eq!(s, sc);
            assert_eq!(dropped, EventId::new(1));
            assert_eq!(capacity, 1);
        }
        other => panic!("expected RingBufferSpill, got {other:?}"),
    }

    assert!(store.suspend(pending("r1", 1, 100, sc)).is_none());
    let spill = store
        .suspend(pending("r2", 2, 900, sc))
        .expect("full queue must spill via store");
    assert_eq!(spill.code(), "ADGL3004");
    match spill {
        StoreDiagnostic::WaitQueueSpill {
            scope: s,
            dropped,
            capacity,
        } => {
            assert_eq!(s, sc);
            assert_eq!(dropped.upper_bound, t(900), "least urgent dropped");
            assert_eq!(capacity, 1);
        }
        other => panic!("expected WaitQueueSpill, got {other:?}"),
    }
    assert_eq!(store.pending_len(sc), 1);
}

// ─── SubGraph ───────────────────────────────────────────────────────────

fn prov(rule: &str, time_ms: i64, dedup: DurationMs) -> RuntimeProvKey {
    RuntimeProvKey {
        key: ProvKey {
            rule: RuleId::new(rule),
            cause: CauseKind::new("PmtudBlackhole"),
            target_expr_hash: 42,
        },
        window_id: window_id(t(time_ms), dedup),
    }
}

#[test]
fn provenance_dedup_rejects_second_insert_in_same_window() {
    // 03 §3.3: prov = (rule, cause, target, window_id); a repeat within the
    // same dedup window is a no-op.
    let mut sg = SubGraph::new();
    let dedup = d(1_000); // spec default dedup_window = 1s (ADR-011)

    let first = prov("pmtud_hypothesis", 100, dedup);
    assert!(
        sg.try_insert_provenance(first.clone()),
        "first insert accepted"
    );
    assert!(sg.has_provenance(&first));
    // Same rule/cause/target, different event time but SAME window (100 and
    // 900 both floor to window 0) → rejected.
    assert!(
        !sg.try_insert_provenance(prov("pmtud_hypothesis", 900, dedup)),
        "same-window dup"
    );
    // Next dedup window (1100 → window 1) → accepted again.
    assert!(
        sg.try_insert_provenance(prov("pmtud_hypothesis", 1_100, dedup)),
        "new window"
    );
    // Different rule in the same window → distinct key, accepted.
    assert!(sg.try_insert_provenance(prov("other_rule", 100, dedup)));
}

#[test]
fn window_id_floors_including_negative_times() {
    // 03 §3.3 window_id = floor(Evt.time / dedup_window).
    let dedup = d(1_000);
    assert_eq!(window_id(t(0), dedup), 0);
    assert_eq!(window_id(t(999), dedup), 0);
    assert_eq!(window_id(t(1_000), dedup), 1);
    assert_eq!(window_id(t(-1), dedup), -1, "floor, not truncation");
}

#[test]
fn emitted_problems_cooldown_key_dedup() {
    // 03 §3.4 (F3): (rule, problem, target) within cooldown → no-op;
    // after cooldown expiry the same key emits again with the new wm.
    let mut sg = SubGraph::new();
    let rule = RuleId::new("pmtud_verdict");
    let problem = ProblemKind::new("XlIcmpTcpMss");
    let target = scope();
    let cooldown = d(5_000);

    assert!(
        sg.try_mark_emitted(&rule, &problem, target, t(1_000), cooldown),
        "first emit"
    );
    assert!(
        !sg.try_mark_emitted(&rule, &problem, target, t(3_000), cooldown),
        "within cooldown: no-op"
    );
    assert!(
        sg.try_mark_emitted(&rule, &problem, target, t(7_000), cooldown),
        "cooldown expired (7000 - 1000 > 5000): emits again"
    );
    // Different target is an independent key even within cooldown.
    assert!(sg.try_mark_emitted(&rule, &problem, ScopeId::vlan(999), t(1_500), cooldown));
}

#[test]
fn emitted_problems_do_not_grow_unbounded() {
    // Review fix: expired same-key tuples are replaced on re-emission, and
    // prune_emitted sweeps keys that stopped emitting.
    let mut sg = SubGraph::new();
    let rule = RuleId::new("pmtud_verdict");
    let problem = ProblemKind::new("XlIcmpTcpMss");
    let cooldown = d(1_000);

    // Re-emitting the same key after cooldown expiry keeps one live tuple,
    // not an ever-growing history.
    for i in 0..100_i64 {
        let wm = t(i * 2_000); // every emit is past the previous cooldown
        assert!(sg.try_mark_emitted(&rule, &problem, scope(), wm, cooldown));
    }
    // A stale tuple for a key that stopped emitting is swept by the explicit
    // prune (wm - max_cooldown horizon).
    let other = RuleId::new("one_shot_rule");
    assert!(sg.try_mark_emitted(&other, &problem, scope(), t(0), cooldown));
    let pruned = sg.prune_emitted(t(1_000_000), cooldown);
    assert_eq!(
        pruned, 2,
        "both remaining stale tuples pruned past the horizon"
    );
    // Cooldown still works after pruning: fresh emit, then suppressed.
    assert!(sg.try_mark_emitted(&rule, &problem, scope(), t(1_000_000), cooldown));
    assert!(!sg.try_mark_emitted(&rule, &problem, scope(), t(1_000_500), cooldown));
}

#[test]
fn provenance_pruned_only_below_gc_horizon() {
    // SubGraph::prune_provenance: keys with window_id below
    // window_id(wm - max_lookback) are unreachable (no live or pending event
    // can produce them, 07 §4 / 08 §6) and are dropped; newer keys survive
    // so same-window dedup still holds.
    let mut sg = SubGraph::new();
    let dedup = d(1_000);
    let lookback = d(10_000);

    let old = prov("pmtud_hypothesis", 0, dedup); // window 0
    let live = prov("pmtud_hypothesis", 95_000, dedup); // window 95
    assert!(sg.try_insert_provenance(old.clone()));
    assert!(sg.try_insert_provenance(live.clone()));

    // wm = 100_000 → horizon window = (100_000 - 10_000) / 1_000 = 90.
    let pruned = sg.prune_provenance(t(100_000), lookback, dedup);
    assert_eq!(pruned, 1);
    assert!(!sg.has_provenance(&old), "unreachable window key dropped");
    assert!(sg.has_provenance(&live), "key at/above horizon retained");
    // Dedup within the retained window still rejects.
    assert!(!sg.try_insert_provenance(live));
}

#[test]
fn store_gc_prunes_provenance_alongside_rings() {
    // GraphStore::gc drives SubGraph::prune_provenance with limits.dedup_window.
    let store = GraphStore::new(Limits::default()); // lookback 60s, dedup 1s
    let sc = scope();
    {
        let mut part = store.partition_mut(sc);
        assert!(part.try_insert_provenance(prov("r", 0, d(1_000)))); // window 0
        assert!(part.try_insert_provenance(prov("r", 200_000, d(1_000)))); // window 200
    }
    let wm = store.advance_watermark(t(300_000)); // horizon window = 240
    let _evicted = store.gc(wm, store.limits().max_lookback);
    let part = store.partition(sc).expect("partition exists");
    assert!(!part.has_provenance(&prov("r", 0, d(1_000))));
    assert!(
        !part.has_provenance(&prov("r", 200_000, d(1_000))),
        "window 200 < horizon 240"
    );
}

#[test]
fn problems_are_append_only_and_supersede_sets_flag() {
    // 03 §3.4 / C7: retraction is superseded=true, never removal.
    use airpulse_dsl_store::ProblemNode;
    use airpulse_dsl_types::{NodeId, SarifId, Severity};

    let mut sg = SubGraph::new();
    let kind = ProblemKind::new("XlIcmpTcpMss");
    sg.push_problem(ProblemNode {
        id: NodeId::new(1),
        kind: kind.clone(),
        target: scope(),
        time: t(1_000),
        severity: Severity::High,
        evidence: vec![],
        sarif_id: SarifId::new("l3_pmtud_blackhole"),
        superseded: false,
    });
    assert_eq!(sg.problems().len(), 1);
    assert_eq!(sg.supersede_problems(&kind, scope()), 1);
    assert_eq!(sg.problems().len(), 1, "append-only: node kept");
    assert!(sg.problems()[0].superseded);
    // Idempotent: already-superseded nodes are not re-marked.
    assert_eq!(sg.supersede_problems(&kind, scope()), 0);
}
