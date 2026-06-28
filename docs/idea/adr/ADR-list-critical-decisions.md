# ADGL Critical Design Decisions (ADR-style)

Полный список решений, принятых **до кода** на основе critical-analysis
исходного проспекта V1 (см. [plans/correctness-refinements.md](../plans/correctness-refinements.md)
для аудит-трейла). Каждый пункт: Options → Recommendation → Trade-offs.

Нотация correction-IDs `C1..C12` сквозная через grammar/semantics/ADRs (как
`C1..C10` в N-FDL).

## ADR-001 — Execution model (C1)

**Options**
- Streaming DAG graph engine (partitioned property graph + watermark)
- Flat TOML rules (текущий AirPulse, `airpulse://rules`)
- AOT rules-compiler в нативный Rust

**Recommendation**: Streaming DAG graph engine.

**Trade-offs**
- − сложность (WaitQueue, RingBuffer, watermark GC)
- + expressivity (многие-ко-многим, конкурирующие гипотезы, ambiguity)
- + детерминированное разрешение Missing Data Paradox
- + изоляция (спека = данные, ядро не перекомпилируется)
- Flat TOML не выражает temporal correlation и конкурирующие causes.

Принято в [ADR-001-execution-model.md](ADR-001-execution-model.md).

---

## ADR-002 — Confidence scale (C2)

**Options**
- 0..1 (legacy AirPulse verdict)
- 0..100 с порогами Candidate/Probable/Confirmed
- Обе шкалы параллельно

**Recommendation**: 0..100 внутри движка; маппинг `/100` → 0..1 в legacy
verdict и SARIF на границе вывода.

**Trade-offs**
- − конвертация на границе миграции (точность 0.01)
- + целочисленная арифметика без float в горячем пути (`#![deny(unsafe_code)]`)
- + пороги читаемы человеком (80 = Confirmed)
- + совместимость: legacy `confidence` 0..1 сохраняется в JSON/SARIF.

[ADR-002-confidence-scale.md](ADR-002-confidence-scale.md).

---

## ADR-003 — Scope vs target (C3)

**Options**
- `target` ≡ `scope` (слияние)
- `scope` = partition key, `target` = сущность Cause/Problem + иерархия
  агрегации
- Глобальный граф без partitioning

**Recommendation**: `scope` = partition key (где исполняется правило);
`target` = сущность, к которой относится гипотеза; иерархия
ClientMac → Vlan → Global с roll-up агрегацией Cause confidence.

**Trade-offs**
- − сложность routing/aggregation
- + cross-scope диагностика (Example 5: ClientMac → Vlan)
- + сохраняется lock-free внутри partition
- + Global = singleton, явная точка сериализации (не ломает lock-free-заявление,
  т.к. Global-правила редкие).

[ADR-003-scope-and-target.md](ADR-003-scope-and-target.md).

---

## ADR-004 — Watermark & deferred evaluation (C4)

**Options**
- Sync-обработка (нет future-window)
- Watermark + WaitQueue (deferred)
- Micro-batch windows

**Recommendation**: Event-time watermark + WaitQueue (BinaryHeap по upper
bound). Политика: PCAP-replay → watermark = max seen event time; live →
bounded-out-of-orderness `W = max_delay`. Late events → drop + side-output
audit (Flink-style). Idle source → min-watermark исключает простаивающие
партиции.

**Trade-offs**
- − live-латентность = max forward window (математическая необходимость)
- + детерминизм, нет race
- + explicit late-event аудит
- + idle-source не блокирует глобальный watermark.

[ADR-004-watermark-policy.md](ADR-004-watermark-policy.md).

---

## ADR-005 — Ambiguity & mutual exclusivity (C5)

**Options**
- Авто-вывод эксклюзивности (эвристика)
- Явное объявление `mutually_exclusive(...)`
- Без AmbiguityNode

**Recommendation**: Явное `mutually_exclusive(C1, C2, ...)` на ruleset/catalog
level; AmbiguityNode синтезируется когда два эксклюзивных Cause в одном scope
достигают Probable (40–79) с `Δconfidence < 15`; lifecycle: resolve/supersede
при расхождении; surfacing через action `mark_ambiguous` + Problem(AmbiguousDiagnosis).

**Trade-offs**
- − автор обязан объявлять эксклюзивность
- + движок не гадает
- + явная семантика для SARIF.

[ADR-005-ambiguity-and-exclusivity.md](ADR-005-ambiguity-and-exclusivity.md).

---

## ADR-006 — Bipartite isolation (C6)

**Options**
- Строгое: evidence только Cause, decision только Problem
- Refine: evidence → Cause + Action; decision → Problem + Action; isolation =
  evidence не эмиссит Problem, decision не создаёт Cause; decision-anchor ∈
  {Cause, Problem}
- Единый тип правил

**Recommendation**: Refine (третий вариант). Исходное заявление «evidence rules
only mutate internal graph state» ложно — evidence-правила эмитят
`request_observation` (Examples 5, 10).

**Trade-offs**
- − менее «чистая» бипартия
- + соответствует реальным примерам
- + decision на Problem (Example 8 suppression) вписывается.

[ADR-006-bipartite-isolation.md](ADR-006-bipartite-isolation.md).

---

## ADR-007 — Contradicts & suppression (C7)

**Options**
- Только `suppress_symptom` (Problem-level)
- Только negative weight (Cause-level Contradicts)
- Оба механизма

**Recommendation**: Оба. `weight: -N` → `EvidenceEdge::Contradicts`, декремент
confidence с floor 0. `action suppress_symptom(p)` → Problem-level suppression
(для топологической маскировки, Example 8). Problem-эмиссия append-only;
retraction через `superseded`-флаг при падении cause ниже порога.

**Trade-offs**
- − два механизма
- + Cause-level (доказательство против) и Problem-level (топо-маскировка)
  семантически различны
- + SARIF `superseded` даёт аудит-трейл.

[ADR-007-contradicts-and-suppression.md](ADR-007-contradicts-and-suppression.md).

---

## ADR-008 — SARIF mapping (C8)

**Options**
- `ruleId` = Problem-имя (`XlIcmpTcpMss`)
- `ruleId` = legacy строка (`l3_icmp_ptb_with_loss`)
- Явный `sarif_id` в `emit`

**Recommendation**: Явный `sarif_id` в `emit` (или catalog-default из
Problem-имя), символический стабильный (SARIF §3.27.5 NOTE: символический ID
стабильнее описательного). `partialFingerprints` (§3.27.17) =
`{scope, target, cause_ids}` для дедупликации между прогонами. Legacy
L3 `recommendation_id`s сохраняются как `sarif_id` (`l3_pmtud_blackhole`,
`l3_stp_spanning_tree`, `l3_dot1x_wired`); `ap_*` — новые стабильные IDs;
parallel-run parity на verdict level (many-to-one legacy rule ids → sarif_id,
plans/migration §2).

**Trade-offs**
- − явное поле в синтаксисе
- + стабильность + GitHub code-scanning fingerprinting
- + миграционная совместимость.

[ADR-008-sarif-mapping.md](ADR-008-sarif-mapping.md).

---

## ADR-009 — Privacy strict mode (C9)

**Options**
- Без privacy-режима
- Поле-level redaction в evidence JSON
- Hash-pseudonymization

**Recommendation**: Поле-level redaction (зеркало AirPulse `l3_privacy: strict`).
PII-поля: `dst_ip`, `src_ip`, `mac`, `client_mac`, `sni`, `bssid`. В strict
режиме `ProblemNode.evidence` JSON заменяет PII на `"<redacted>"`; внутри графа
(индексация, scope-key) значения сохраняются. Поле помечается `pii` в
catalog-схеме event.

**Trade-offs**
- − каталог помечает PII
- + reproducibility внутри движка + privacy снаружи
- + соответствие AirPulse strict.

[ADR-009-privacy-strict.md](ADR-009-privacy-strict.md).

---

## ADR-010 — Topology Unknown (C10)

**Options**
- Unknown ≡ absent (false)
- Unknown ≡ present (true)
- Three-valued: Unknown → defer / `request_topology` fallback

**Recommendation**: Three-valued. `TopologyProvider` возвращает `Bool | Unknown`.
В correlate `topo:` predicate: Unknown ≠ absent → правило не может решить;
fallback = `action request_topology` (не assuming false). Cycle-bound
(`max_topology_hops`) для `upstream_of`.

**Trade-offs**
- − трёхзначная логика в семантике
- + нет ложных negative при отсутствии LLDP/CDP
- + graceful degradation.

[ADR-010-topology-unknown.md](ADR-010-topology-unknown.md).

---

## ADR-011 — DoS limits (C11)

**Options**
- Без лимитов (доверие к спеке)
- Фиксированные лимиты everywhere
- Configurable лимиты с дефолтами

**Recommendation**: Configurable с дефолтами (mirror N-FDL §8). Lexer/parser:
max token 255B, source 4 MiB, nesting 64. Runtime: `max_pending_per_scope`,
`max_causes_per_scope`, `max_ringbuffer_events_per_scope`, `max_topology_hops`,
`max_rule_firings_per_event`, `MAX_LOOKBACK`. Spill: drop oldest pending +
diagnostic. `MAX_LOOKBACK > max(max_backward, max_forward)` — верификатор (C4).

**Trade-offs**
- − конфиг-поверхность
- + DoS-устойчивость, bounded memory
- + верификатор доказывает окно < MAX_LOOKBACK.

[ADR-011-dos-limits.md](ADR-011-dos-limits.md).

---

## ADR-012 — Determinism & ordering (C12)

**Options**
- Параллельный nondeterministic emission
- Deterministic ordering внутри partition

**Recommendation**: Deterministic. Внутри partition порядок firing =
`(anchor_event_time, rule_declaration_order)`; emission порядок Problems/Actions
= тот же + tie-break `scope_id`. Это обеспечивает дифференциальное тестирование
vs legacy и стабильные SARIF.

**Trade-offs**
- − slight serialization внутри partition
- + воспроизводимость, stable IDs, differential testing.

[ADR-012-determinism.md](ADR-012-determinism.md).

---

## Итог

Приняты все ADR-001..ADR-012 (этот документ + детальные файлы). Correction-IDs
`C1..C12` использованы в grammar/semantics/verification. Открытых вопросов на
момент spec v1 нет; всё из critical-analysis закрыто (см.
[plans/correctness-refinements.md](../plans/correctness-refinements.md)).
