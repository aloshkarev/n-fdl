# ADGL IR & Intent Bytecode v1

Определяет четыре уровня представления, Verified IR (`ProgramImage`), intent
opcodes (компактные директивы для evaluator) и контракт lowering. Crate-владелец
`airpulse_dsl::ir` (проектный). Зеркалирует N-FDL
[../../spec/06-ir-bytecode.md](../../spec/06-ir-bytecode.md), но вместо
bytecode-VM (N-FDL) ADGL использует graph-walk IR + intent-stream — движок
диспетчирует intents над GraphStore, а не интерпретирует IP-bytecode.

## 1. Четыре уровня представления

```
Parsed AST  ──parser──▶ Typed AST ──verifier──▶ Verified IR ──lowering──▶ Intent Bytecode
(winnow)               (04)           (05)            (§2)                (§4)
```

- **Parsed AST**: serde-structs, mirrors [02-grammar.ebnf](02-grammar.ebnf).
- **Typed AST**: + type annotations (04).
- **Verified IR** (`ProgramImage`): сериализуемый, versioned артефакт (ADR-012:
  hot-reload, кэш).
- **Intent Bytecode**: компактные opcodes для evaluator hot-path.

## 2. Verified IR — `ProgramImage`

```rust
pub struct ProgramImage {
    pub magic: [u8; 4],          // b"ADGL"
    pub version: u32,            // semver-packed (major<<16 | minor<<8 | patch)
    pub ruleset_id: Box<str>,    // "airpulse.tcp_diagnostics"
    pub requires: Box<[Box<str>]>,
    pub exclusivity: Box<[ExclusivityGroup]>,
    pub rules: Box<[RuleInstance]>,
    pub catalog_ref: CatalogRef, // указывает на catalog-версию
}
```

`ProgramImage` сериализуется (serde, versioned, backward-compat — как N-FDL
ADR-012). Magic `b"ADGL"` (vs N-FDL `b"NFDL"`). Hot-reload: сравнение
`(ruleset_id, version, catalog_ref)` решает, пересобирать ли partitions.

### 2.1 `RuleInstance`

```rust
pub struct RuleInstance {
    pub id: RuleId,              // stable symbolic, C12
    pub kind: RuleKind,          // Evidence | Decision
    pub scope: ScopeType,
    pub anchor: AnchorSpec,
    pub correlates: Box<[CorrelateSpec]>,
    pub branches: BranchTable,   // if/else over T3 cond (present/absent + metric preds), 03 §3.7
    pub body: Box<[Intent]>,     // ordered infer/emit/action
}

pub struct AnchorSpec {
    pub binding: Symbol,         // "rtx"
    pub event_type: EventType,
    pub predicate: Expr,         // compiled to predicate bytecode (§4)
}

pub struct CorrelateSpec {
    pub binding: Symbol,         // "ptb" / "upstream"
    pub source: CorrelateSource, // Event | Problem | Cause — 02 §4 CorrelateSource (Example 8 = Problem)
    pub topo: TopoCall,
    pub window: WindowProof,     // Calculable { back, forward } | RuntimeCheck (05 §11)
    pub min_match: u8,           // having: count >= N; default 1 (05 §8.2)
}

pub enum CorrelateSource { Event(EventType), Problem(ProblemKind), Cause(CauseKind) }
```

`min_match` is compact only in verified IR. Parsed AST retains the original
`i64` literal; verifier rejects values outside `1..=32` before conversion.

### 2.2 `PendingMatch` (WaitQueue entry, 08)

```rust
pub struct PendingMatch {
    pub rule: RuleId,
    pub anchor_event: EventId,   // ссылка на RingBuffer (must survive until wm > upper)
    pub upper_bound: i64,        // anchor.time + max forward (08 §2)
    pub scope: ScopeId,
    // BinaryHeap упорядочен по upper_bound (min-heap для ближайшего дедлайна)
}
```

### 2.3 `Intent`

```rust
pub enum Intent {
    InferCause  { cause: CauseKind, target: Expr, weight: i8,
                  evidence: Box<[Symbol]>, provenance_key: ProvKey },
    EmitProblem { problem: ProblemKind, target: Option<Expr>, severity: Severity,
                  evidence: Box<[Symbol]>, sarif_id: Box<str>, pii: Box<[FieldPath]> },
    EmitAction  { kind: ActionKind, arg: Option<Symbol>,
                  target: Option<Expr>, reason: Option<Box<str>>,
                  evidence: Box<[Symbol]> },
    SupersedeProblem { problem: ProblemKind, target: Expr },   // suppress_symptom (C7)
    MarkAmbiguous { causes: (CauseKind, CauseKind), target: Expr },
}
```

`ProvKey` = `(rule_id, cause, target_expr_hash, window_id)` — для O(1)
provenance-дедупликации (03 §3.3). `pii` собрано верификатором (05 §10).

## 3. Control-flow

ADGL не имеет IP-bytecode-control-flow как N-FDL VM. Control-flow выражен на
уровне `BranchTable` (if/else) и WaitQueue (deferred). Внутри тела правила
intents исполняются последовательно (ordered, C12). Циклов нет (rule-DAG
ацикличен, 05 §2).

### 3.1 BranchTable

```rust
pub struct BranchTable {
    pub cond: Expr,              // T3-valued: present/absent primaries + metric predicates, Kleene + short-circuit (03 §3.7)
    pub then_body: Box<[Intent]>,
    pub else_body: Option<Box<[Intent]>>,
    pub unknown_body: Box<[Intent]>,   // = [EmitAction{ request_topology }] автоген, C10
}
```

`unknown_body` генерируется автоматически верификатором, если cond может дать
Unknown (т.е. содержит `present`/`absent` над correlate с topo-Unknown-риском).
Это гарантирует, что движок никогда не «забывает» Unknown-ветку (C10).

## 4. Intent Bytecode (hot-path)

Predicate-выражения (`rtx.segment_size > 1400`, `c.confidence >= 80`) и
`target`/`weight`/`time`-окна компилируются в компактный bytecode для evaluator.
Группы opcode:

```
LOAD:   LOAD_EVENT_FIELD   binding, field_idx -> slot     // rtx.segment_size / rtx.vlan / rtx.path / rtx.time
        LOAD_CAUSE_FIELD   binding, field_idx -> slot     // c.confidence / c.target / c.time (04 §3)
        LOAD_PROBLEM_FIELD binding, field_idx -> slot     // upstream.target / upstream.time / downstream.target / downstream.time (Example 7/8, 04 §3)
        LOAD_CONST         imm -> slot
        LOAD_DURATION      imm_ms -> slot
        LOAD_SCOPE_KEY     -> slot                          // rtx.target

EXPR:   ADD SUB MUL DIV MOD        // i64 checked
        CMP_EQ CMP_NE CMP_LT CMP_LE CMP_GT CMP_GE
        AND OR NOT                 // Kleene over T3 (03 §3.7)

TOPO:   TOPO_CALL  func_idx, slot_a, slot_b -> slot_t3    // same_session etc (C10)

WIN:    WIN_BACK  anchor.time, dur -> slot
        WIN_FWD   anchor.time, dur -> slot
        WIN_IN    x.time, lo, hi -> slot_bool              // inclusive (D4)

CHECK:  PROV_CHECK  prov_key -> skip_if_seen              // dedup (03 §3.3)
        SCOPE_COMPAT ok | runtime

EMIT:   INFER_CAUSE   cause, target_slot, weight_imm, evidence_regs
        EMIT_PROBLEM  problem, target_slot, severity_imm, sarif_id_idx, pii_mask
        EMIT_ACTION   action_kind, arg_idx, target_slot, reason_idx
        SUPERSEDE     problem, target_slot
        MARK_AMBIG    cause_a, cause_b, target_slot

YIELD:  SUSPEND_PENDING  upper_bound_slot    // -> WaitQueue (08)
        EFFECT            effect_buf         // -> ActionSink / SARIF (11)
```

Opcodes `SCREAMING_SNAKE` с операндами (как N-FDL). `PROV_CHECK` —
zero-branching dedup (если seen → skip INFER_CAUSE). `TOPO_CALL` возвращает `T3`
в slot; `AND`/`OR` — Kleene.

### 4.1 Пример lowering (Rule 3 PMTUD, fragment)

```
; anchor rtx: event(tcp.retransmission_burst) { rtx.segment_size > 1400 }
LOAD_EVENT_FIELD rtx, F_SEGMENT_SIZE -> s0
LOAD_CONST 1400 -> s1
CMP_GT s0, s1 -> s2                      ; anchor predicate
; correlate ptb: same_session, time in [rtx.time-500ms, rtx.time+1s]
WIN_BACK rtx.time, 500ms -> lo
WIN_FWD  rtx.time, 1000ms -> hi          ; forward ⇒ SUSPEND_PENDING later
; if present(ptb) infer +85 else infer +35 + request_observation
PROV_CHECK (R1, PmtudBlackhole, rtx.target, window) -> skip
EMIT_PROBLEM ...
```

## 5. Continuation / Suspended (08)

ADGL не имеет `VmContinuation` (N-FDL) — вместо неё `PendingMatch` в WaitQueue
содержит `anchor_event` ref + `upper_bound`. Resume = pop из WaitQueue при
`watermark > upper_bound` и исполнить correlate + body. Anchor-event обязан
пережить ожидание (MAX_LOOKBACK-инвариант, 05 §3.1).

## 6. Zero-copy

`ProgramImage` хранит `Box<str>`/`Box<[..]>` (owned, сериализуемый). В
hot-path evaluator работает над `&EventNode` (borrowed из RingBuffer) и
`ScopeId` (Copy). Metric-paths — `field_idx` (u16) в opcodes, без строкового
lookup в hot-path. Topology-функции — `func_idx` (u8). Единственное копирование:
`Intent` в `Effect`-буфер (owned, для ActionSink/SARIF).

## 7. Что НЕ входит в v1

- JIT/codegen (graph-walk IR достаточно; JIT — v1 если profiling покажет need).
- Optimizer passes (constant folding, prov-dedup already at IR).
- Cross-ruleset linking (один ProgramImage на ruleset).
- User-defined opcodes (catalog-only topology/actions).

## 8. IR → Evaluator контракт

1. `ProgramImage` верифицирован (05 контракт выполнен) ⇒ evaluator не делает
   повторных статических проверок.
2. Каждый `CorrelateSpec.window` = `Calculable` ⇒ `upper_bound` известен
   статически ⇒ детерминированный WaitQueue-плейсмент.
3. `PROV_CHECK` гарантирует `infer` ≤ 1 раза на (rule, cause, target, window).
4. `unknown_body` всегда присутствует для Unknown-рисковых cond ⇒ C10 enforced.
5. `pii_mask` в `EMIT_PROBLEM`/`EMIT_ACTION` ⇒ strict-redaction без повторного
   анализа catalog.
6. Opcodes — checked-арифметика; переполнение (теоретически недостижимое) →
   `CorrelateError::ArithOverflow`, не паника (07 §7).
