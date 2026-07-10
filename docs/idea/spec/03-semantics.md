# ADGL Formal Semantics & Scope Rules v1

Определяет точную семантику каждой конструкции, разделение pure/effect,
watermark-отложенное вычисление, коммутативное накопление confidence и
инварианты завершаемости. Контракт между грамматикой
([02-grammar.ebnf](02-grammar.ebnf)) и верификатором/движком. Crate-владелец:
`airpulse_dsl::evaluator` (проектный). Зеркалирует N-FDL
[../../spec/03-semantics.md](../../spec/03-semantics.md).

Нотация вычисления выражения `e` в окружении `Γ`: `⟦e⟧(Γ)`.

## 1. Семантические домены

```
Evt         EventNode = { id, type, time, scope_key, fields, span }   immutable
Ring        time-evicting buffer событий partition (07 §3)
Sub         SubGraph = { Causes, Problems, Ambiguities, Edges }       partition
SG          ScopeId (partition key)
WM          watermark (event-time, ms)
WQ          WaitQueue = BinaryHeap<PendingMatch> по upper_bound         (08)
Prov        provenance = (rule_id, cause_type, target, window_id)       дедуп
C           Cause.confidence ∈ 0..100 (u8)
W           rule weight ∈ -100..+100 (i8)
T3          Bool | Unknown                                               three-valued (C10)
Err         SyntaxError | TypeError | VerificationError                 (AOT)
            | CorrelateError | LimitExceeded | ActionSinkError          (runtime)
```

Сигнатура обработки события (монадическая по partition):

```
ingest : Evt × GraphStore  ->  GraphStore' × Effects  ∪  { Suspended }
```

`Suspended` достижимо, когда anchor сработал, но correlate имеет future-window
и правило помещено в WaitQueue до watermark > upper_bound
([08-stream-watermarking.md](08-stream-watermarking.md)).

## 2. Pure vs Effect (строгая граница)

| Класс | Конструкции | Где разрешено |
|---|---|---|
| **Pure** | metric-предикаты anchor `{ rtx.segment_size > 1400 }`, `topo`-аргументы, `time`-окна, `target`/`weight`/`severity` выражения | anchor/correlate/if/infer/emit |
| **Topo-effect** | `topo: same_session(...)` (чтение TopologyProvider) | correlate |
| **Graph-effect** | `infer Cause(...)` (мутация Cause confidence + EvidenceEdge) | evidence rule body |
| **Output-effect** | `emit Problem(...)` (создание ProblemNode) | decision rule body |
| **Side-effect** | `action request_observation/suppress_symptom/...` | оба слоя правил (C6) |

**Правило:** graph/output/side-effect в pure-позиции (внутри metric-предиката
anchor) → `VerificationError::EffectInPurePosition`. Topo-effect допустим только
в `correlate.topo` (трёхзначный результат, §3.2).

## 3. Семантика конструкций (operational)

Обозначение: `⟨rule, Evt, Sub, WM⟩ ⇓ ⟨Sub', Effects'⟩` — исполнение правила
обновляет subgraph и эффекты.

### 3.1 Anchor `anchor rtx: event(T) { p }`

```
ingest Evt of type T into scope SG(Evt):
  для каждого EvidenceRule R с scope == SG(Evt.type) и anchor type == T (по порядку объявления):
    Γ = { rtx := Evt }
    если ⟦p⟧(Γ) == true:
      upper = max over correlate blocks of (Evt.time + correlate.forward_bound)   (08 §2)
      если upper > WM:   Suspended -> WQ.push(PendingMatch{R, Evt, upper})   // есть незакрытый forward-окно (wm < upper); resume строго при wm > upper (08 §3.2)
      иначе (wm >= upper): немедленное исполнение §3.2–3.4    // backward-only (upper == wm) ⇒ immediate (Example 8); late anchor (wm > upper) ⇒ immediate
```

Anchor срабатывает **раз на matching event per rule**. Повторное срабатывание
того же правила на том же событии невозможно (event immutable, route — once).

### 3.2 Correlate `correlate x: Source { topo, time [, having] }`

`Source ∈ { event(T), Problem(K), Cause(K) }` ([02 §4](02-grammar.ebnf)
`CorrelateSource`). Optional `having: count >= N` (`N` — целый литерал
`1..=32`; опущен ≡ `N = 1`, прежнее earliest-match поведение). При исполнении
(после watermark > upper, либо немедленно для pure-backward окон) для каждого
correlate-блока:

```
candidates =
  event(T)   -> Ring.scan(T, time.window)                    // [anchor.time - back, anchor.time + fwd], inclusive
  Problem(K) -> Sub.Problems.filter(kind==K, time.window)    // decision-rule correlate (Example 8)
  Cause(K)   -> Sub.Causes.filter(kind==K, time.window)
matches = scan candidates in deterministic order:
            count True where ⟦topo⟧(anchor, c) == True
            stop when count == N (work cap; witness = earliest True)
binding x =
  если count >= N:  Some(witness)                   // earliest True among counted matches
  иначе если ∃ c: ⟦topo⟧(anchor, c) == Unknown:     Unknown          // C10
  иначе:                                                Absent
```

Для `event`-source `topo`-Unknown возможен (topology absent); для
`Problem`/`Cause`-source candidates локальны в SubGraph, но `topo`-predicate
(напр. `upstream_of`) всё равно может вернуть `Unknown` ⇒ та же трёхзначная
логика. `Problem`/`Cause` correlates не требуют WaitQueue-suspend (нет future
window — они читают уже-эмиссированные узлы), но `time`-окно проверяется.

`present(ptb)` = `binding ∈ {Some}` (т.е. `count >= N`); `absent(ptb)` =
`binding == Absent`; `Unknown`-binding → ни present, ни absent не истинны →
`else` не выполняется, вместо него `action request_topology` (C10, §3.6).
Evidence/provenance ссылается только на witness (earliest True), не на все
совпавшие кандидаты.

### 3.3 Infer `infer Cause(K) { target, weight ±, evidence }`

```
T = ⟦target⟧(Γ ∪ bindings)
W = ⟦weight⟧  ∈ -100..+100
prov = (R.id, K, T, window_id)                       window_id = floor(Evt.time / dedup_window)
если prov ∈ Sub.seen_provenance:  no-op              (C12 дедуп, один раз за окно)
иначе:
    C_old = Sub.Causes.get(K, T).confidence  или  0
    C_new = clamp(0, 100, C_old + W)                    commutative, ADR-002
    // Cause.time = Evt.time при создании (first-infer), стабилен при последующих infer (04 §3; correlate time-window §3.2)
    Sub.Causes.upsert(K, T, C_new, time = Sub.Causes.has(K,T) ? existing.time : Evt.time)
  Sub.Edges.add(EvidenceEdge{ kind = W>=0 ? Supports : Contradicts, src=Evt, dst=Cause(K,T) })
  Sub.seen_provenance.insert(prov)
  Effects += ConfidenceMutation(K, T)                // триггер decision re-eval (§3.5)
```

`window_id` (дедуп-окно) — настраиваемый период (по умолчанию 1s); одна
пара (rule, cause, target) применяется не более раза за окно. Negative `weight`
→ `EvidenceEdge::Contradicts`, декремент с floor 0 (C7).

### 3.4 Emit `emit Problem(P) { target, severity, evidence, sarif_id? }`

```
T = ⟦target⟧  (опц. для Problem; если опущено — target = scope)
если (R.id, P, T) ∈ Sub.emitted_problems и не истёк cooldown:  no-op   (F3)
иначе:
  Sub.Problems.upsert(P, T, time = WM, severity, evidence_refs, sarif_id, superseded=false)   // time = emission watermark (04 §3; correlate §3.2, Example 7)
  Sub.emitted_problems.insert((R.id, P, T, WM))
  Effects += ProblemEmission(P, T, sarif_id)          // -> SARIF/JSON, 11
```

Problem-эмиссия **append-only**: при падении cause ниже порога Problem не
удаляется, а помечается `superseded=true` (C7, ADR-007). `sarif_id` —
стабильный symbolic ID (C8); если опущен — catalog-default из Problem-имени.

### 3.5 Decision re-evaluation (F5)

```
on ConfidenceMutation(K, T, SG):
  для каждого DecisionRule D с scope == SG и CauseAnchor(K) (по порядку):
    если ⟦D.anchor.predicate⟧({ c := Cause(K,T) }) == true:   // c.confidence >= 80
      исполнить D body (emit/action)  с dedup §3.4

on ProblemEmission(P, T, SG):
  для каждого DecisionRule D с scope == SG и ProblemAnchor(P) (по порядку):
    исполнить D body (correlate + if/else + emit/action)         // Example 8 suppression
```

Decision на Cause-anchor исполняется на `ConfidenceMutation`; на Problem-anchor
(Example 8) — на `ProblemEmission`. Это сохраняет бипартию: decisions реагируют
на агрегированное состояние (Cause confidence или Problem emission), не на сырые
события (C6). Problem-anchor decision может реагировать на Problems,
эмиссированные другим правилом того же ruleset — верификатор (05 §2) гарантирует
ацикличность зависимости `emit Problem(P) → ProblemAnchor(P)`.

### 3.6 Action `action request_observation(kind) { target, reason?, evidence? }` (G2)

```
intent = ActionIntent{ kind, target, reason, evidence_refs }
Effects += ActionEmission(intent)                   // -> ActionSink, 10
```

`ActionSink` (trait, [10-catalog-abi.md](10-catalog-abi.md)):
- **offline (PCAP replay)**: no-op + audit-log (нельзя ретроактивно загрузить
  eBPF-фильтр). Diagnostic `ADGL3001 ActionNoOpInReplay` (warning).
- **live**: `request_observation` → load eBPF capture filter для целевого
  scope; `suppress_symptom` → mark Problem superseded; `run_check` →
  enqueue внешнюю проверку; `request_topology` → enqueue LLDP/CDP poll;
  `mark_ambiguous` → создать AmbiguityNode (§4).

Action — declarative intent; тело не имеет stateful-логики (no user functions,
[02 §10](02-grammar.ebnf)).

### 3.7 If/Else условие (C10)

Условие `if` — `Expr` ([02 §6](02-grammar.ebnf)), вычисляемое в T3:
`present(x)`/`absent(x)` → T3 (по binding §3.2); metric-сравнения
(`oper.state == "DOWN"`, `rtx.segment_size > 1400`) → Bool, lifted до T3
(True/False). `and`/`or`/`not` — Kleene.

```
cond = ⟦if_cond⟧(bindings)   // T3
если cond == True:   исполнить then-body
иначе если cond == False и есть else:  исполнить else-body
иначе если cond == Unknown:   emit action request_topology; then/else пропустить
```

**Short-circuit** (обязательно): `and` не вычисляет RHS если LHS ∈ {False,
Unknown}; `or` не вычисляет RHS если LHS ∈ {True}. Это критично для паттерна
`present(oper) and oper.state == "DOWN"` (Example 7 / [06-link-absent](../examples/06-link-absent.adgl)):
`oper.state` определено только когда `oper` bound (Some); если `present(oper)` не
True, RHS не вычисляется ⇒ нет доступа к unbound полю.

Kleene: `True and Unknown = Unknown`; `False and Unknown = False`;
`Unknown or True = True`; `Unknown or Unknown = Unknown`. Это гарантирует, что
отсутствие topology не даёт ложных negative-выводов (C10).

## 4. AmbiguityNode lifecycle (C5)

```
on ConfidenceMutation в scope SG:
  для каждой пары (K1, K2) ∈ mutually_exclusive[SG.ruleset]:
    c1 = Causes(K1, common_target).confidence
    c2 = Causes(K2, common_target).confidence
    если c1 ∈ [40,79] и c2 ∈ [40,79] и |c1 - c2| < 15:
      если Ambiguity(K1,K2,target) не существует:
        Sub.Ambiguities.create(K1, K2, target)
        Effects += ActionEmission(mark_ambiguous{ target, causes:[K1,K2] })
    иначе если существующая Ambiguity(K1,K2,target) и (max(c1,c2) >= 80 или |c1-c2| >= 15):
        Sub.Ambiguities.resolve(...)   // superseded; SARIF-запись сохраняется
```

`common_target` = target, по которому K1 и K2 конкурируют (обычно = scope target).
Surfacing: `mark_ambiguous` action → Problem(AmbiguousDiagnosis) в SARIF с
`partialFingerprints` = `{scope, target, causes}` (C8).

## 5. Scope-правила (резолюция имён, C3)

Поиск имени `x` — от внутреннего scope к внешнему; **первое совпадение**.

```
1. Bindings scope : имена anchor/correlate (rtx, ptb, dhcp, ...)
2. Cause/Problem ctx : c.confidence (в decision-anchor predicate)
3. Builtins      : __watermark __scope __confidence
4. Catalog       : event/cause/problem/topology fields (через .-access)
```

### 5.1 `target` vs `scope` (C3)

- `scope` — partition key, определяет **куда** routed событие и **где** живёт
  SubGraph. Задаётся `scope:` в правиле.
- `target` — сущность, к которой относится гипотеза/проблема. Может отличаться
  от scope (Example 5: `scope: ClientMac`, `target: dhcp.vlan`).
- Cross-scope агрегация: Cause confidence в child-scope (ClientMac) roll-up в
  parent-scope (Vlan) по `target`-пути. Алгоритм — [09-scopes-sessions.md](09-scopes-sessions.md) §3.

### 5.2 Qualified access `rtx.segment_size`

Резолвится по catalog-схеме event-типа (`tcp.retransmission_burst` → поля
`segment_size`, `target`, `time`, `vlan`, `path`, ...). Верификатор
([05 §1](05-verification.md)) проверяет существование path. Read-only.

## 6. Детерминизм (C12)

- **Ordered firing** внутри partition: правила обрабатываются в порядке
  объявления ruleset для matching event; correlate-matches — earliest-time-first.
- **Ordered emission**: Effects упорядочены `(anchor_event_time, rule_decl_order)`;
  tie-break `scope_id` (детерминированный hash).
- Один `(B*, catalog)` → идентичный SARIF (property `deterministic_output`,
  [12-testing.md](12-testing.md)).
- Параллельность только **между** partitions (share-nothing); внутри partition —
  serial (C12).

## 7. Завершаемость (termination)

Гарантируется тремя механизмами:

1. **Rule-DAG ацикличен**: верификатор ([05 §2](05-verification.md)) — нет
   циклов `evidence R1 infer Cause K → decision D2 → ... → evidence R1`.
2. **WaitQueue bounded**: pending-instances ≤ `max_pending_per_scope` (ADR-011);
   каждый PendingMatch имеет upper_bound ≤ `Evt.time + max_forward_window`, после
   watermark — исполняется или drop+spill.
3. **Нет общей рекурсии**: нет user-functions, нет loops; action = declarative
   intent (не вызывает правила). Confidence clamp ∈ [0,100] ⇒ накопление сходится.

ADGL **не Тьюринг-полон**: нет неограниченных циклов, нет рекурсии, нет
изменяемого общего состояния в pure-позиции. Все программы завершаются за
`O(events × rules × max_correlate_window)`.

## 8. Контракт

1. Anchor срабатывает ≤ 1 раза на (rule, event).
2. `infer` с тем же provenance в одном dedup-окне — no-op.
3. `present`/`absent` трёхзначны; Unknown → `request_topology`, не false.
4. Confidence ∈ [0,100], коммутативен, clamp.
5. Problem append-only; retraction = `superseded`-флаг.
6. Action = intent; offline → no-op+log, live → eBPF/external.
7. Внутри partition — детерминированный порядок (C12).
8. Завершаемость: rule-DAG ацикличен + WaitQueue bounded + no recursion.
