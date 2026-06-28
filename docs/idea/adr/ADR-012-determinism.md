# ADR-012 — Determinism & ordering (resolves C12)

- **Status:** Accepted
- **Date:** 2026-06-28
- **Related:** [ADR-list](ADR-list-critical-decisions.md), [../spec/03-semantics.md](../spec/03-semantics.md) §6,
  [../spec/09-scopes-sessions.md](../spec/09-scopes-sessions.md) §5, N-FDL [../../spec/09-efsm-sessions.md](../../spec/09-efsm-sessions.md) §8

## Контекст

Prospect V1: «lock-free parallel execution» per partition, но не определяет
порядок firing/эмиссии внутри partition. Коммутативность confidence защищает
состояние, но `action`/`Problem` emission order влияет на SARIF stable IDs и
differential testing vs legacy. N-FDL meticulous о детерминизме (ordered
choice, total order) — нужно зеркало.

## Решение

**Deterministic ordering** внутри partition; cross-partition merge.

### Внутри partition (03 §6, 09 §5)
```
firing order      = (anchor_event.time, rule_declaration_index)
correlate matches = earliest-time-first (03 §3.2)
emission order    = (anchor_event.time, rule_declaration_index, scope_id_hash)
```
- Правила обрабатываются в порядке объявления ruleset для matching event.
- `scope_id_hash` — детерминированный hash (одинаковая функция на всех запусках).

### Cross-partition
- Parallel lock-free **между** partitions (DashMap, 07 §3).
- Serial **внутри** partition (один поток owns shard в момент processing).
- Финальный SARIF: merge всех partition Effects по
  `(event_time, rule_decl_index, scope_id_hash)`.

### Watermark
- Single global `AtomicI64::fetch_max` — monotone non-decreasing.
- Resume order из WaitQueue — по `upper_bound` (min-heap).

### Свойство
- Один `(PCAP, ProgramImage, catalog)` → идентичный SARIF (12 §3
  `deterministic output`).
- Это gate для differential testing vs legacy (12 §1.4) и migration parallel-run
  (plans/migration).

## Последствия

- `RuleId` = stable symbolic (decl index + ruleset id) (06 §2.1).
- `Effect` buffer per-partition, ordered; merge deterministic.
- Тест: `deterministic output` (12 §3) — random partition scheduling ⇒ same SARIF.

## Отклонённые альтернативы

- **Parallel nondeterministic emission**: SARIF differs run-to-run; differential
  testing невозможно; stable IDs unstable.
- **Global serial (no partition parallelism)**: теряется lock-free throughput
  (ключевое требование V1).
