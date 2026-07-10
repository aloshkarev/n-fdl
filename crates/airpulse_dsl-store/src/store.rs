//! `GraphStore` — the sharded partition owner with the global monotone
//! watermark, per `docs/idea/spec/07-runtime.md` §3–§4 and §10.

use std::sync::atomic::{AtomicI64, Ordering};

use airpulse_dsl_ir::PendingMatch;
use airpulse_dsl_types::{DurationMs, EventTime, ScopeId};
use dashmap::DashMap;
use dashmap::mapref::one::{Ref, RefMut};

use crate::diag::StoreDiagnostic;
use crate::event::EventNode;
use crate::limits::Limits;
use crate::ring::RingBuffer;
use crate::subgraph::SubGraph;
use crate::waitqueue::WaitQueue;

/// The runtime state store (`07` §3 `GraphStore`).
///
/// `DashMap` gives lock-free parallel execution **between** partitions (C3);
/// within a partition processing is serial — one thread owns the entry guard
/// at a time (C12, `07` §10). The watermark is a single global
/// `AtomicI64` advanced with `fetch_max`, hence monotone (`07` §10,
/// ADR-012).
#[derive(Debug)]
pub struct GraphStore {
    /// Per-scope diagnostic subgraphs (`07` §3).
    partitions: DashMap<ScopeId, SubGraph>,
    /// Per-scope event rings (`07` §3).
    rings: DashMap<ScopeId, RingBuffer>,
    /// Global event-time watermark in ms (`08` §2). `i64::MIN` until the
    /// first advance.
    watermark: AtomicI64,
    /// Per-scope WaitQueues (`07` §3 `pending`).
    pending: DashMap<ScopeId, WaitQueue>,
    /// ADR-011 runtime bounds.
    limits: Limits,
}

impl GraphStore {
    /// Creates an empty store with the given ADR-011 limits.
    #[must_use]
    pub fn new(limits: Limits) -> GraphStore {
        GraphStore {
            partitions: DashMap::new(),
            rings: DashMap::new(),
            watermark: AtomicI64::new(i64::MIN),
            pending: DashMap::new(),
            limits,
        }
    }

    /// The configured limits.
    #[must_use]
    pub const fn limits(&self) -> &Limits {
        &self.limits
    }

    /// Current global watermark. `EventTime::from_millis(i64::MIN)` before
    /// the first advance.
    #[must_use]
    pub fn watermark(&self) -> EventTime {
        EventTime::from_millis(self.watermark.load(Ordering::Acquire))
    }

    /// Monotone watermark advance (`07` §5 / `08` §8.1): `fetch_max`, so a
    /// smaller `t` is a no-op and the watermark never regresses. Returns the
    /// new watermark `max(previous, t)` — the value the resume sweep and GC
    /// must use (`07` §5).
    #[must_use = "resume/GC must run against the returned (possibly larger) watermark (07 §5)"]
    pub fn advance_watermark(&self, t: EventTime) -> EventTime {
        let prev = self.watermark.fetch_max(t.millis(), Ordering::AcqRel);
        EventTime::from_millis(prev.max(t.millis()))
    }

    /// Pushes an event into its scope's ring (creating the ring on first
    /// use with `max_ringbuffer_events_per_scope` capacity). Returns the
    /// `ADGL3005` spill diagnostic when the ring was full (ADR-011).
    #[must_use = "spill diagnostics must be routed by the caller (ADR-011)"]
    pub fn push_event(&self, event: EventNode) -> Option<StoreDiagnostic> {
        let scope = event.scope;
        let mut ring = self
            .rings
            .entry(scope)
            .or_insert_with(|| RingBuffer::new(scope, self.limits.max_ringbuffer_events_per_scope));
        ring.push(event)
    }

    /// Read access to a scope's ring (window scans, anchor lookup).
    #[must_use]
    pub fn ring(&self, scope: ScopeId) -> Option<Ref<'_, ScopeId, RingBuffer>> {
        self.rings.get(&scope)
    }

    /// Mutable access to a scope's subgraph, created on first use. The
    /// returned guard serializes intra-partition access (C12).
    ///
    /// **Re-entrancy hazard (DashMap shard lock):** the guard holds a write
    /// lock on the partition's shard. While it is alive, do not call any
    /// other `GraphStore` method that touches `partitions` —
    /// [`GraphStore::partition_mut`], [`GraphStore::partition`],
    /// [`GraphStore::gc`], [`GraphStore::prune_emitted`] — even for a
    /// *different* scope: a same-shard collision deadlocks. Rings and
    /// pending live in separate maps and stay safe to touch. Drop the guard
    /// before cross-partition work (mirrors C12: one thread owns a
    /// partition shard at a time, `07` §10).
    pub fn partition_mut(&self, scope: ScopeId) -> RefMut<'_, ScopeId, SubGraph> {
        self.partitions.entry(scope).or_default()
    }

    /// Read access to a scope's subgraph.
    #[must_use]
    pub fn partition(&self, scope: ScopeId) -> Option<Ref<'_, ScopeId, SubGraph>> {
        self.partitions.get(&scope)
    }

    /// Suspends a match into its scope's WaitQueue (`08` §3.1), creating the
    /// queue on first use with `max_pending_per_scope` capacity. Returns the
    /// `ADGL3004` spill diagnostic on overflow (ADR-011).
    #[must_use = "spill diagnostics must be routed by the caller (ADR-011)"]
    pub fn suspend(&self, pending: PendingMatch) -> Option<StoreDiagnostic> {
        let scope = pending.scope;
        let mut wq = self
            .pending
            .entry(scope)
            .or_insert_with(|| WaitQueue::new(scope, self.limits.max_pending_per_scope));
        wq.push(pending)
    }

    /// Pops every suspended match with `upper_bound < wm` across all scopes
    /// — the resume sweep of `advance_watermark` (`07` §5, `08` §3.2:
    /// resume strictly when `wm > upper`). The result is sorted by
    /// `PendingMatch` order (upper_bound, then deterministic tie-breaks) so
    /// resume order is independent of DashMap shard iteration (ADR-012).
    #[must_use = "popped matches are removed from the queue and must be resumed (08 §3.2)"]
    pub fn pop_expired(&self, wm: EventTime) -> Vec<PendingMatch> {
        let mut expired = Vec::new();
        for mut entry in self.pending.iter_mut() {
            expired.extend(entry.value_mut().pop_expired(wm));
        }
        expired.sort();
        expired
    }

    /// Watermark GC over every ring (`07` §4): evicts events strictly older
    /// than `watermark - max_lookback`. Returns the total evicted count.
    ///
    /// Safe w.r.t. pending matches by the MAX_LOOKBACK invariant
    /// (`05` §3.1 / `08` §6): `max_lookback > max(back, forward) + slack`
    /// ⇒ an anchor event outlives every unexpired `PendingMatch` on it.
    pub fn gc(&self, watermark: EventTime, max_lookback: DurationMs) -> usize {
        let mut evicted = 0;
        for mut entry in self.rings.iter_mut() {
            evicted += entry.value_mut().gc(watermark, max_lookback);
        }
        // Prune unreachable provenance dedup keys alongside ring GC — same
        // horizon, same invariant (SubGraph::prune_provenance docs).
        for mut entry in self.partitions.iter_mut() {
            entry
                .value_mut()
                .prune_provenance(watermark, max_lookback, self.limits.dedup_window);
        }
        evicted
    }

    /// Sweeps stale problem-emission cooldown entries in every partition
    /// (see [`SubGraph::prune_emitted`]); `max_cooldown` is the maximum
    /// cooldown across the loaded ruleset, owned by the caller. Returns the
    /// total pruned count.
    pub fn prune_emitted(&self, wm: EventTime, max_cooldown: DurationMs) -> usize {
        let mut pruned = 0;
        for mut entry in self.partitions.iter_mut() {
            pruned += entry.value_mut().prune_emitted(wm, max_cooldown);
        }
        pruned
    }

    /// Number of suspended matches for `scope` (diagnostics/test support).
    #[must_use]
    pub fn pending_len(&self, scope: ScopeId) -> usize {
        self.pending.get(&scope).map_or(0, |wq| wq.len())
    }
}
