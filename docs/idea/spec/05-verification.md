# ADGL Static Verification v1

Фаза верификации работает над Typed AST + catalog и порождает Verified IR
([06-ir-bytecode.md](06-ir-bytecode.md)). Это последняя AOT-граница: **после неё
runtime-паника по логике DSL невозможна по построению** (остаются
`CorrelateError` / `LimitExceeded` / `ActionSinkError`). Реализуется в
`airpulse_dsl::verifier` (проектный); единственный источник `VerificationError`.
Зеркалирует N-FDL [../../spec/05-verification.md](../../spec/05-verification.md).

Принцип: дёшево и консервативно. Где не доказуемо статически — НЕ отклоняем, а
вставляем runtime-проверку (downgrade), сохраняя safety (C11).

## 1. Конвейер проверок (порядок важен)

```
1. Catalog resolution       metric-paths, event/cause/problem/action + arg-kind existence  (§1)
2. Capability check         requires ⊆ catalog.capabilities                     (§6)
3. Scope/target compat      rule.scope ⊑ target-scope, cause/problem валидны    (§4)
4. Temporal bound calc      time-окна вычислимы, MAX_LOOKBACK-инвариант          (§3)
5. Topology signature       topo-функции из каталога, arity, T3-возврат          (§5)
6. Rule-DAG acyclicity      нет циклов evidence→decision→evidence                (§2)
7. Exclusivity well-formed  mutually_exclusive ссылается на валидные Cause       (§7)
8. Bipartite isolation      evidence не emit Problem; decision не infer Cause    (§8)
9. DoS limits               окна, dedup, nesting, requires-count                 (§9)
10. Privacy annotations     pii-поля из каталога propagated в evidence           (§10)
```

Любая фаза может выдать `VerificationError` с `diag_id` (`ADGL####`) и span.
Несколько ошибок собираются и репортятся батчем (как rustc), а не на первой
([11-error-diagnostics.md](11-error-diagnostics.md)).

### 1.1 Action-arg resolution (per ActionKind)

`action`-аргумент имеет разный тип в зависимости от `ActionKind` (10 §5).
Синтаксически grammar (02 §9) допускает `KindIdent` (включая single `Ident` =
binding) для всех action-args — верификатор различает catalog-kind vs
Ref-binding по `ActionKind`, а не по синтаксису:

| ActionKind | Arg | Verifier check | Diagnostic |
|---|---|---|---|
| `request_observation` | catalog observation kind (`KindIdent`: `icmp.visibility`, `aaa.telemetry`, `wifi.rf_metrics`) | arg ∈ catalog.observation_kinds | `ADGL0207` |
| `run_check` | catalog check kind (`KindIdent`: `cable_loopback`, `lldp_poll`, `stp_root_check`) | arg ∈ catalog.check_kinds | `ADGL0208` |
| `suppress_symptom` | in-scope `ProblemRef` binding (напр. `downstream` — anchor/correlate binding типа `ProblemRef(P)`, Example 7) | arg ∈ Γ и Γ(arg) : ProblemRef(P) | `ADGL0209` |
| `mark_ambiguous`, `request_topology` | (no arg) | arg отсутствует (grammar: `[ "(" KindIdent ")" ]` опц.) | — |
| (unknown action) | — | action `Ident` ∈ catalog.actions | `ADGL0206` |

Это явно разрешает `suppress_symptom(downstream)` (Example 7): `downstream` —
`ProblemRef` binding, **не** catalog kind — `ADGL0207/0208` здесь неприменимы.
Lowering (06 §2.3): `suppress_symptom` → `SupersedeProblem { problem = P,
target = arg.target }`; kind-args (`request_observation`/`run_check`) →
`EmitAction { arg: Symbol }`. Этим гарантируется п.1 контракта (§12): все
action/arg refs разрешимы и типизированы.

## 2. Rule-DAG ацикличность

### 2.1 Построение

Узлы — правила (`EvidenceRule`, `DecisionRule`). Рёбра `R1 → R2`
(execution-dependency — R2 реагирует на output R1):
- `evidence R1 infer Cause(K)` → `decision R2` с `CauseAnchor(K)` (R2 re-fires
  на `ConfidenceMutation` от R1);
- `decision R2 emit Problem(P)` → `decision R3` с `ProblemAnchor(P)` (Example 8;
  R3 re-fires на `ProblemEmission` от R2).

AmbiguityNode synthesis (03 §4) **не создаёт** rule-DAG-рёбер: она read-only
читает Cause confidence и создаёт/resolves AmbiguityNode + `mark_ambiguous`, не
re-fire'ит правила. Поэтому mutually-exclusive seeding — нескольких causes в
одном правиле (Example 2) или в нескольких правилах (Example 10) — допустимо и не
образует цикл. Confidence clamp ∈ [0,100] ⇒ synthesis сходится.

### 2.2 Acyclicity (Tarjan SCC)

Любая SCC размера > 1 (или self-loop) →
`VerificationError::CyclicRuleDependency` со списком правил в цикле. Это сильнее,
чем «нет рекурсии»: правило не может (транзитивно) реагировать на свой же вывод.

Пример отвергаемого: `evidence R1 infer Cause(K) → decision D2 emit Problem(P) →
decision D3 ... → evidence R1` (через Problem-anchor, замкнуто).

## 3. Temporal bound analysis (C4/D3)

Для каждого `correlate` блока вычисляем back/forward окно:

```
window = [ ⟦a⟧, ⟦b⟧ ]   где time: x.time in [a, b]
back    = anchor.time - a_lo      (≥ 0; должно быть ≤ MAX_LOOKBACK)
forward = b_hi - anchor.time      (≥ 0; окно в будущее ⇒ Suspended, 03 §3.1)
```

`a`/`b` — выражения над `anchor.time` и `duration`-литералами
(`rtx.time - 500ms`, `rtx.time + 1s`). Верификатор **доказывает вычислимость**
границы: оба конца обязаны быть `anchor.time ± duration_lit` (линейная форма);
произвольные выражения над будущими событиями →
`VerificationError::NonCalculableWindow`.

### 3.1 MAX_LOOKBACK-инвариант (C4/D3)

```
для каждого correlate окна [a, b]:
  back    ≤ MAX_LOOKBACK - slack
  forward ≤ MAX_LOOKBACK - slack        (anchor должен пережить ожидание)
  slack   ≥ max_forward_window          (pending lifetime, 08 §3)
иначе -> VerificationError::WindowExceedsLookback
```

Это гарантирует, что anchor-event не будет GC'd из RingBuffer до исполнения
pending-instance. `MAX_LOOKBACK` — config (ADR-011); верификатор знает его из
manifest. Нарушение — hard error (иначе runtime потеряет данные).

### 3.2 Inclusive bounds (D4)

`[a, b]` inclusive both ends (02 §4, 01 §7). Верификатор проверяет, что
предикат `in` использует 2-элементный список; иной размер →
`VerificationError::MalformedWindow`.

## 4. Scope/target compatibility (C3)

Для каждого `infer Cause(K){ target: t }` / `emit Problem(P){ target: t }`:

```
σ_t = scope-type(t)        // статически из catalog: t = rtx.target ⇒ Session, t = dhcp.vlan ⇒ Vlan
σ_r = rule.scope
требуется:  σ_r ⊑ σ_t  (rule.scope равен или child target-scope; 04 §5 иерархия)
иначе: TypeError::ScopeTargetMismatch
```

Дополнительно: `catalog(σ_t, K)` — cause K валиден в target-scope σ_t (не каждый
cause имеет смысл в каждом scope; напр. `RfInterference` только в `AccessPoint`,
[10](10-catalog-abi.md) §3). Нарушение → `TypeError::CauseScopeInvalid` /
`ProblemScopeInvalid`.

Cross-scope roll-up (ClientMac→Vlan) допускается через `target`-путь: правило,
объявленное в `ClientMac`, может `infer` cause с `target: dhcp.vlan`, т.к.
`ClientMac ⊑ Vlan` (Vlan — ancestor, 04 §5); cause затем roll-up'ится в Vlan
partition (09 §3). `Vlan ⊑ ClientMac` ложно ⇒ rule-in-parent/target-in-child
отвергается (mismatch).

## 5. Topology signature check (C10)

```
topo ::= Ident "(" ExprList ")"
требуется:
  Ident ∈ catalog.topology_funcs (same_session, same_port, upstream_of, ...)
  arity(ExprList) == manifest(Ident).arity
  ∀ arg: type(arg) == ScopeId
  ret-type == T3
иначе: VerificationError::{UnknownTopology, TopoArity, TopoArgType}
```

`upstream_of` дополнительно проверяет, что оба аргумента — `ScopeId` одного
scope-типа (нельзя `upstream_of(Session, Port)`).

## 6. Capability check (E5)

```
ruleset.requires ⊆ catalog.capabilities
иначе: VerificationError::UnknownCapability { id }
```

Load-time. Runtime-недоступность (topology = Unknown) — НЕ capability-fail
(03 §3.7); capability = «движок поддерживает класс анализа», не «данные есть».

## 7. Exclusivity well-formedness (C5)

```
mutually_exclusive(K1, K2, ...):
  ∀ Ki: Ki ∈ catalog.causes
  ∀ pair (Ki, Kj): Ki и Kj имеют общий достижимый target-scope
                    (иначе Ambiguity никогда не сработает — warning)
  нет пересечения пар с другими mutually_exclusive-группами
                    (одна пара в одной группе) — иначе VerificationError::OverlappingExclusivity
```

Warning (не error): exclusivity-пара, чьи causes никогда не достижимы в одном
scope → `ADGL0502 RedundantExclusivity` (подсказка автору).

## 8. Bipartite isolation check (C6)

```
EvidenceRule:   body ∈ { InferStmt, ActionStmt }              // НЕ EmitStmt
DecisionRule:   body ∈ { EmitStmt, ActionStmt }               // НЕ InferStmt
иначе: VerificationError::BipartiteViolation { layer, offending_construct }
```

`action` допустим в обоих слоях (C6, ADR-006). `decision` с `ProblemAnchor`
допустим (Example 8) — это decision над агрегированным состоянием, не creation
of Cause.

## 9. DoS limits check (C11)

| Проверка | Лимит | Diagnostic |
|---|---|---|
| nesting depth | 64 | `ADGL0103` |
| correlate blocks per rule | 8 | `ADGL0204` |
| infer/emit per rule body | 16 | `ADGL0205` |
| `requires` entries | 32 | `ADGL0105` |
| forward window max | `MAX_LOOKBACK - slack` | `ADGL0412` (§3.1) |
| dedup window ≥ 1ms | 1ms | `ADGL0503` |
| mutually_exclusive group size | 8 | `ADGL0441` |

Превышение — hard `VerificationError` (спека = данные, не должна раздувать
runtime-память).

## 10. Privacy annotation propagation (C9)

Для каждого `evidence: [refs]` в `infer`/`emit`/`action`:

```
собрать PII-поля из catalog-схем всех referenced events
помечить Intent.evidence_pii = set(field_paths)
```

Это не error-фаза; оно аннотирует IR для runtime-redaction (04 §6.1, ADR-009).
В strict-режиме runtime заменит PII в evidence JSON на `"<redacted>"`.

## 11. Выход фазы: Verified IR аннотации

Каждый correlate-узел IR несёт:

```
WindowProof   = Calculable { back: Duration, forward: Duration } | RuntimeCheck
LookbackOk    = Proven | Violated(hard error)
ScopeCompat   = Proven (σ_r ⊑ σ_t) | TypeError
TopoSig       = Proven | TypeError
Bipartite     = Ok | Violated
```

`WindowProof.Calculable` ⇒ движок статически знает upper_bound ⇒
детерминированный WaitQueue-плейсмент (08 §2). `RuntimeCheck` (downgrade) —
для нелинейных окон (редкие; верификатор требует linear form, так что в v1
большинство Calculable).

## 12. Гарантии после verify (контракт)

1. Все metric-paths, event/cause/problem/action/topology refs разрешимы.
2. Все `requires` известны; scope/target совместимы; topology-сигнатуры корректны.
3. Rule-DAG ацикличен ⇒ завершаемость (03 §7).
4. Все temporal windows вычислимы и ≤ MAX_LOOKBACK ⇒ нет потерянных pending.
5. Bipartite isolation enforced; exclusivity well-formed.
6. DoS-лимиты удовлетворены ⇒ bounded memory.
7. PII-аннотации propagated ⇒ strict-redaction возможен без повторного анализа.

Эти гарантии — предпосылка свойств «deterministic output», «flat-memory GC»,
«topology cycle isolation» в [12-testing.md](12-testing.md).
