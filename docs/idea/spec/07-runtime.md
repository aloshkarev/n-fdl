# ADGL Runtime v1

Определяет crate-tree, `GraphStore`, RingBuffer + watermark GC, WaitQueue,
Engine-pipeline, `TopologyProvider`/`ActionSink` traits, allocation-стратегию,
unsafe-политику, детерминизм/concurrency. Crate-владелец `airpulse_dsl` (проектный).
Зеркалирует N-FDL [../../spec/07-runtime.md](../../spec/07-runtime.md); псевдокод
иллюстрирует структуру, не финальные сигнатуры.

## 1. Crate-tree

```
airpulse_dsl
├── syntax    (lexer + winnow parser -> Parsed AST)         02-grammar
├── types     (node/edge/scope/confidence types)            04
├── verify    (AOT static analysis -> Verified IR)          05
├── ir        (ProgramImage, RuleInstance, Intent, opcodes) 06
├── store     (GraphStore, RingBuffer, WaitQueue, GC)       §3, §4
├── evaluator (Engine: ingest/route/advance/correlate/exec) §5
├── catalog   (universal network troubleshooting catalog)  10
└── diag      (ariadne diagnostics, event bus)              11
```

Все crates `#![deny(unsafe_code)]` (AirPulse-мандат). Нет `unsafe` вообще
(в отличие от N-FDL, где `nfdl-plugin` — единственное исключение; ADGL не имеет
FFI-плагинов в v1, catalog — встроенный).

## 2. Lifetime/buffer модель

```
'pcap   lifetime исходного capture / live stream
'img    lifetime ProgramImage (static после load)
Sub     owned per-partition (GraphStore владеет)
EventNode  stored в RingBuffer (owned, evictable)
Cause/Problem/Ambiguity  owned Sub, stable NodeId
&EventNode  borrowed в hot-path (zero-copy из RingBuffer)
```

Никаких arena-backed значений в PendingMatch (anchor ref = `EventId` +
RingBuffer-lookup, не `&'pcap`). Это избегает lifetime-проблем N-FDL
`VmContinuation` (07-runtime N-FDL §5) — ADGL не имеет continuations, только
PendingMatch с EventId.

## 3. GraphStore + RingBuffer

```rust
pub struct GraphStore {
    partitions: DashMap<ScopeId, SubGraph>,    // lock-free cross-partition (C3)
    rings: DashMap<ScopeId, RingBuffer>,       // per-partition event buffer
    watermark: AtomicI64,                       // global event-time (08)
    pending: DashMap<ScopeId, BinaryHeap<PendingMatch>>,  // WaitQueue per scope
}

pub struct SubGraph {
    causes: HashMap<(CauseKind, ScopeId), CauseNode>,
    problems: Vec<ProblemNode>,                 // append-only (C7)
    ambiguities: HashMap<((CauseKind,CauseKind), ScopeId), AmbiguityNode>,
    edges: Vec<EvidenceEdge>,
    seen_provenance: HashSet<ProvKey>,          // dedup (03 §3.3)
    emitted_problems: HashSet<(RuleId, ProblemKind, ScopeId, i64)>,  // cooldown (F3)
}

pub struct RingBuffer {
    buf: VecDeque<EventNode>,                   // sorted by time
    scope: ScopeId,
    capacity: usize,                            // max_ringbuffer_events_per_scope (ADR-011)
}
```

`DashMap` обеспечивает lock-free parallel execution **между** partitions (C3).
Внутри partition — serial (C12). RingBuffer — `VecDeque` с time-order;
GC- eviction в §4.

## 4. Watermark GC (C4/D3)

```
fn gc(ring: &mut RingBuffer, watermark: i64, max_lookback: i64) {
    let cutoff = watermark - max_lookback;
    while let Some(front) = ring.buf.front() {
        if front.time < cutoff { ring.buf.pop_front(); }
        else { break; }
    }
    // invariant (05 §3.1): max_lookback > max(max_backward, max_forward) + slack
    // ⇒ никакой PendingMatch не ссылается на evicted event
}
```

GC запускается после `advance_watermark` (§5). Memory strictly bounded:
`|ring| ≤ capacity` (spill: drop oldest + diagnostic `ADGL3005 RingBufferSpill`,
ADR-011). На infinite loop-PCAP heap остаётся плоским (12 §property).

## 5. Engine pipeline

```
fn ingest(evt: EventNode, store: &GraphStore, img: &ProgramImage, topo: &dyn TopologyProvider, sink: &mut dyn ActionSink) {
    // 1. Route to partitions (fan-out by scope, 09 §2)
    for sg in route_scopes(evt, img) {
        let ring = store.rings.get_mut(sg); ring.push(evt.clone_for_partition(sg));
        // 2. Anchor match (03 §3.1) — для каждого evidence-rule в sg
        for rule in img.rules_for(evt.type, sg, Evidence) {
            if predicate_holds(rule.anchor, evt) {
                let upper = evt.time + max_forward(rule.correlates);
                if upper > store.watermark { suspend(rule, evt, upper, sg); }   // forward-window not closed (wm < upper) ⇒ suspend; backward-only (upper == wm) ⇒ immediate (Example 8); resume строго при wm > upper (08 §3.2)
                else { run_correlate_and_body(rule, evt, sg, store, topo, sink); }
            }
        }
    }
}

fn advance_watermark(store: &GraphStore, t: i64) {
    let wm = store.watermark.fetch_max(t, ..).max(t);   // fetch_max returns prev; new wm = max(prev, t)
    // pop expired PendingMatch (08 §3)
    for (sg, wq) in store.pending.iter() {
        while let Some(top) = wq.peek() {
            if top.upper_bound < wm { let m = wq.pop(); resume(m, store, topo, sink); }
            else { break; }
        }
    }
    // GC (§4)
    for ring in store.rings.iter_mut() { gc(ring, wm, max_lookback); }
}
```

`resume` = `run_correlate_and_body` для suspended-правила: scan RingBuffer в
окне, resolve bindings (03 §3.2), выполнить intents (03 §3.3–3.6),
`ConfidenceMutation` → decision re-eval (03 §3.5), Ambiguity synthesis (03 §4).

## 6. TopologyProvider trait (C10)

```rust
pub trait TopologyProvider: Send + Sync {               // bounds mirror 10 §9; Send+Sync для cross-partition lock-free (07 §10)
    fn same_session(&self, a: ScopeId, b: ScopeId) -> T3;
    fn same_client (&self, a: ScopeId, b: ScopeId) -> T3;
    fn same_port   (&self, a: ScopeId, b: ScopeId) -> T3;
    fn same_ap     (&self, a: ScopeId, b: ScopeId) -> T3;
    fn same_vlan   (&self, a: ScopeId, b: ScopeId) -> T3;
    fn upstream_of (&self, up: ScopeId, down: ScopeId) -> T3;  // cycle-bound; max_hops baked in impl (ADR-011, 10 §9)
}
```

`T3 = Bool | Unknown`. `Unknown` ≠ `false` (ADR-010). `upstream_of` —
BFS/DFS с `visited`-set, `max_hops` bound; цикл в topology → `False` (не
считается upstream) + diagnostic. Реализация указывает на AirPulse adjacency hashes
(`wifi_analysis`, `l3_cross_diagnostics`) — plans/migration §3.

## 7. ActionSink trait (G2)

```rust
pub trait ActionSink: Send {                              // bounds mirror 10 §10; Send — sink mutable per emission (07 §10)
    fn emit(&mut self, intent: ActionIntent, mode: RunMode, wm: i64);
}
pub enum RunMode { Offline { audit: &mut AuditLog }, Live { ebpf: &mut EbpfController, topo: &mut TopologyController } }   // Live topo — для request_topology (10 §10)
```

- Offline: `request_observation` → audit-log `ADGL3001 ActionNoOpInReplay`;
  `suppress_symptom` → mark superseded (graph mutation OK); `request_topology` →
  audit.
- Live: `request_observation` → `ebpf.load_filter(scope)`; `request_topology` →
  `topology.poll(scope)`; `run_check` → external check enqueue.

Action = declarative; sink решает эффект по `RunMode` (03 §3.6).

## 8. Allocation strategy

| Mechanism | Where | Зачем |
|---|---|---|
| `DashMap` sharded | partitions/rings/pending | lock-free cross-partition (C3) |
| `VecDeque` | RingBuffer | O(1) push/pop front, time-ordered |
| `BinaryHeap` | WaitQueue per scope | O(log n) pop min upper_bound |
| `HashMap`/`HashSet` | SubGraph indices | O(1) cause/provenance lookup |
| owned `Box<[Intent]>` | Effect buffer | survives hot-path, -> sink |
| `EventNode` clone на fan-out | per-scope ring | изоляция partition; shallow clone |

Hot-path (ingest) — zero allocation кроме `EventNode` clone для fan-out (по
числу matching scopes). Predicate bytecode — `slot`-registers (stack-allocated
array, no heap). Это держит ingest дешёвым.

## 9. unsafe policy

`#![deny(unsafe_code)]` во всех crates. `DashMap`/`BinaryHeap` — safe wrappers.
Нет FFI в v1 (catalog встроен, topology/action — Rust traits). Checked-арифметика
в opcodes; переполнение → `CorrelateError::ArithOverflow`, не паника. Никаких
`unwrap`/`expect` в hot-path на данных, которые могут прийти из capture (только
на invariant-assertions debug-build).

## 10. Determinism & concurrency (C12)

- **Cross-partition**: parallel, lock-free (DashMap shards).
- **Intra-partition**: serial (один поток owns partition shard в момент
  processing; DashMap entry-guard). Порядок = (event_time, rule_decl_order).
- **Emission**: Effects собираются per-partition в упорядоченный buffer;
  merge по `(event_time, rule_decl_order, scope_id)` в финальном SARIF.
- **Watermark**: single global `AtomicI64::fetch_max` — monotone.
- Один `(PCAP, ProgramImage, catalog)` → идентичный SARIF (12 §property).

## 11. Контракт runtime

1. `#![deny(unsafe_code)]`; нет FFI в v1; checked-арифметика; нет panic в hot-path.
2. Cross-partition lock-free; intra-partition serial + deterministic order.
3. RingBuffer bounded (`capacity`); GC по watermark; flat-memory на loop-PCAP.
4. WaitQueue bounded (`max_pending_per_scope`); spill + diagnostic (ADR-011).
5. PendingMatch ссылается на event, гарантированно живущий до `wm > upper`
   (MAX_LOOKBACK-инвариант, 05 §3.1).
6. `TopologyProvider` → `T3`; `Unknown` обрабатывается семантикой, не падает.
7. `ActionSink` эффект зависит от `RunMode`; offline никогда не вызывает eBPF.
8. Watermark monotone (`fetch_max`); late events — 08 §4.
