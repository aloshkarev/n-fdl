# ADGL Type System v1

Определяет типы узлов/рёбер графа, scope-типы, скалярные домены confidence/weight,
схемы event/cause/problem из каталога, сигнатуры topology-функций и
capability-типы. Crate-владелец: `airpulse_dsl::types` (проектный); единственный
источник `TypeError`. Зеркалирует N-FDL [../../spec/04-type-system.md](../../spec/04-type-system.md).

## 1. Синтаксис типов

```
τ ::=
  | Bool | Int | Duration | String | Severity               // скаляры
  | EventRef(T)    // binding anchor/correlate типа event T
  | CauseRef(K)    // binding decision-anchor cause K
  | ProblemRef(P)  // binding decision-anchor problem P
  | Confidence     // u8 0..100
  | Weight         // i8 -100..+100
  | ScopeId        // partition key
  | T3             // Bool | Unknown (topology)
  | List(τ)
  | Option<τ>      // conditional fields в event-схеме
```

## 2. Скалярные домены

| Тип | Rust | Домен | Примечание |
|---|---|---|---|
| `Bool` | `bool` | true/false | metric-предикаты |
| `Int` | `i64` | целое | metric-значения (`segment_size`, счётчики) |
| `Duration` | `i64_ms` | ≥ 0 | окна `time:`, dedup-окна |
| `String` | `&'a str` | zero-copy | `reason`, `sarif_id`, enum-значения |
| `Severity` | enum | Critical/High/Medium/Low/Recommended/Optional | `emit` |
| `Confidence` | `u8` | 0..100 | Cause.confidence (ADR-002) |
| `Weight` | `i8` | -100..+100 | rule weight; отрицательный = Contradicts (C7) |
| `T3` | enum | True/False/Unknown | topology (C10) |

Float отсутствует (ADR-002). `Confidence`/`Weight` — отдельные newtype-домены,
чтобы не путать с `Int` в типизаторе.

### 2.1 Threshold-псевдо-значения (не типы, для читаемости)

```
Candidate  = confidence ∈ [10, 39]
Probable   = confidence ∈ [40, 79]
Confirmed  = confidence ∈ [80, 100]
```

Используются в семантике Ambiguity (Probable, [03 §4](03-semantics.md)) и в
decision-anchor-предикатах (`c.confidence >= 80`). Это предикаты над `Confidence`,
не отдельные типы.

## 3. Типы узлов графа

```
NodeKind =
  | EventNode    { id: NodeId, type: EventType, time: Int, scope_key: ScopeId,
                   fields: EventFields(type), span: Span }              immutable
  | CauseNode    { id: NodeId, kind: CauseKind, target: ScopeId, time: Int,
                   confidence: Confidence, evidence: List<NodeId> }      stateful   // time = first-infer Evt.time (03 §3.3), stable
  | ProblemNode  { id: NodeId, kind: ProblemKind, target: ScopeId, time: Int,
                   severity: Severity, evidence: List<NodeId>,
                   sarif_id: String, superseded: Bool }                  append-only  // time = emission WM (03 §3.4)
  | AmbiguityNode{ id: NodeId, causes: (CauseKind, CauseKind), target: ScopeId,
                   state: Active | Resolved }
  | ActionNode   { id: NodeId, kind: ActionKind, intent: ActionIntent, wm: Int }
```

`EventNode` immutable (Ring Buffer, time-evicting). `CauseNode` stateful
(confidence мутируется коммутативно). `ProblemNode` append-only (`superseded`
вместо удаления, C7). `CauseKind`/`ProblemKind`/`ActionKind`/`EventType` —
catalog-типы (§6, [10](10-catalog-abi.md)).

Runtime carriage keeps scalar `Int` fields in the existing sorted
`(FieldIdx, i64)` array. `IntList` fields use a separate immutable sidecar,
sorted by `FieldIdx`; each value list is sorted, deduplicated, and
deterministically truncated to 64 entries. Scalar predicate opcodes never load
the sidecar. Until list-aware predicate IR exists, the verifier rejects every
predicate use of `IntList` fields with diagnostic `ADGL0213`; evidence
serialization still includes list values, subject to the same catalog PII
redaction as scalar fields.

## 4. Типы рёбер

```
EdgeKind =
  | Supports      CauseNode <- EventNode/CauseNode     weight > 0
  | Contradicts   CauseNode <- EventNode/CauseNode     weight < 0  (C7)
  | Explains      CauseNode <- EventNode               neutral correlation
  | Suppresses    ProblemNode <- ProblemNode           suppress_symptom (C7)
  | RollsUp       CauseNode(parent scope) <- CauseNode(child scope)  (C3, 09 §3)
```

Рёбра направленные. `EvidenceEdge ∈ {Supports, Contradicts, Explains}` создаётся
`infer` (03 §3.3). `Suppresses` создаётся `action suppress_symptom`. `RollsUp` —
cross-scope агрегация (09 §3).

## 5. Scope-типы (C3)

```
ScopeType ::= Session | Port | ClientMac | Vlan | AccessPoint | Global
```

Иерархия (для cross-scope roll-up, [09](09-scopes-sessions.md) §3):

```
ClientMac ⊂ Vlan ⊂ Global
Session   ⊂ Vlan          (по vlan-path события)
Port      ⊂ Global        (physical)
AccessPoint ⊂ Global
```

`Global` — singleton partition (один ScopeId = `GLOBAL`). Это явная точка
сериализации для cross-cutting правил (Example 8 suppression); lock-free
сохраняется между non-Global partitions (ADR-003).

`ScopeId` = детерминированный hash от (ScopeType, key-компонентов события),
например `Session -> hash(5-tuple)`, `Port -> hash(switch_id, port_id)`.

## 6. Catalog-схемы (типизируются верификатором, [05 §1](05-verification.md))

### 6.1 Event-схема

Каждый `EventType` в каталоге ([10](10-catalog-abi.md) §2) определяет record-тип:

```
tcp.retransmission_burst : {
    segment_size : Int,
    target       : ScopeId(Session),     // 5-tuple
    time         : Int,                   // event-time ms
    vlan         : ScopeId(Vlan),
    path         : List<ScopeId>,         // for request_observation targeting
    dst_ip       : Int  [pii],            // C9 privacy-маркер
    ...
}
```

Поля с `[pii]` — redact в strict-режиме (ADR-009). `.target` — canonical
scope-key данного event для своего scope; для cross-scope routing движок
извлекает ключи для всех заявленных правилами scope-типов (09 §2).

### 6.2 Cause/Problem-схема

```
Cause(PmtudBlackhole)   : { target: ScopeId, time: Int, confidence: Confidence, evidence: List<NodeId> }
                             // time = first-infer Evt.time (03 §3.3), stable; для correlate time-window (03 §3.2)
Problem(XlIcmpTcpMss)   : { target: ScopeId, time: Int, severity: Severity, evidence: List<NodeId>,
                            sarif_id: String = "l3_pmtud_blackhole" }   // C8 legacy-stable (= legacy recommendation_id); time = emission WM (03 §3.4)
```

`sarif_id` по умолчанию = catalog-default (символический, стабильный); может
переопределяться в `emit` ([02 §5](02-grammar.ebnf)).

### 6.3 Topology-функции (C10)

```
same_session(a: ScopeId, b: ScopeId) : T3
same_client (a: ScopeId, b: ScopeId) : T3
same_port   (a: ScopeId, b: ScopeId) : T3
same_ap     (a: ScopeId, b: ScopeId) : T3
same_vlan   (a: ScopeId, b: ScopeId) : T3
upstream_of (upstream: ScopeId, downstream: ScopeId) : T3    // cycle-bound, ADR-010
```

Возврат `T3` (`Bool | Unknown`); `Unknown` → `request_topology` fallback (03 §3.7).
`upstream_of` traversal ограничен `max_topology_hops` (ADR-011).

### 6.4 Action-виды

```
ActionKind ::= request_observation | run_check | suppress_symptom
             | mark_ambiguous | request_topology
```

Сигнатуры аргументов — в [10-catalog-abi.md](10-catalog-abi.md) §5.

### 6.5 Capability-типы (`requires`)

```
Capability ::= "l3-deep" | "topology" | "wifi-ota" | "radio-nemo" | ...
```

Load-time check: все `requires` ruleset должны быть объявлены в catalog; иначе
`VerificationError::UnknownCapability` ([05 §6](05-verification.md)). Runtime
availability (topology может быть Unknown) — отдельное от load-time
([03 §3.7](03-semantics.md)).

## 7. Правила типизации (selected)

```
(T-NodeField)
  Γ(x) : EventRef(T)        catalog(T).field(f) : τ_f
  ---------------------------------------------------- Γ ⊢ x.f : τ_f
  Γ(x) : CauseRef(K)        catalog(K).field(f) : τ_f      // f ∈ {target: ScopeId, time: Int, confidence: Confidence}
  ---------------------------------------------------- Γ ⊢ x.f : τ_f
  Γ(x) : ProblemRef(P)      catalog(P).field(f) : τ_f      // f ∈ {target: ScopeId, time: Int, severity: Severity}
  ---------------------------------------------------- Γ ⊢ x.f : τ_f

(T-Weight)
  lit ∈ -100..+100
  ----------------------- Γ ⊢ lit : Weight

(T-ConfCmp)
  Γ ⊢ c : CauseRef(K)      catalog(K).confidence : Confidence
  ----------------------------------------------------------- Γ ⊢ c.confidence >= 80 : Bool

(T-Infer)
  Γ ⊢ target : ScopeId(σ)   Γ ⊢ weight : Weight   rule.scope ⊑ σ
  K ∈ catalog(σ, K)                                              // cause валиден в target-scope
  ---------------------------------------------------------------------- Γ ⊢ infer Cause(K){...} ok

(T-Emit)
  Γ ⊢ target : ScopeId(σ)   Γ ⊢ severity : Severity   rule.scope ⊑ σ
  P ∈ catalog(σ, P)                                              // problem валиден в target-scope
  ---------------------------------------------------------------------- Γ ⊢ emit Problem(P){...} ok

(T-Action)
  Γ ⊢ target : τ_t   τ_t ∈ catalog_action(kind).target_types   // ScopeId; либо List<ScopeId> для request_observation(path)
  ---------------------------------------------------------------- Γ ⊢ action kind(arg){target,...} ok
```

`rule.scope ⊑ σ` — **rule.scope равен или child target-scope σ** (target —
ancestor-or-equal rule.scope в иерархии §5). Допускает same-scope
(`Session/Session`) и cross-scope roll-up (`ClientMac` rule → `Vlan` target,
09 §3), но запрещает rule-in-parent/target-in-child (не бьётся с partitioning).
Нарушение → `TypeError::ScopeTargetMismatch` (C3). `catalog(σ, K)` — cause K
валиден в scope σ (не каждый cause имеет смысл в каждом scope; напр.
`RfInterference` только `AccessPoint`).

## 8. Что НЕ входит в v1

- User-defined типы узлов/рёбер (catalog-only).
- Generic-типы, Higher-rank.
- Float (`Confidence`/`Weight` целочисленны, ADR-002).
- Подтипы кроме scope-иерархии `⊑`.
- User-defined topology-функции (catalog 6 функций + trait extension в v1.5).

## 9. Контракт

1. Каждый `EventType`/`CauseKind`/`ProblemKind`/`ActionKind` определён в каталоге.
2. `Confidence ∈ [0,100]`, `Weight ∈ [-100,100]` — newtype, не `Int`.
3. `rule.scope` ⊑ `target`-scope (target — равен или ancestor; roll-up — 09 §3).
4. Topology-функции возвращают `T3`; `Unknown` обрабатывается семантикой (C10).
5. PII-поля помечены `[pii]` для strict-redaction (C9).
6. Capability `requires` проверяются AOT (load-time).
