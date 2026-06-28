# ADR-001 — Execution model (resolves C1)

- **Status:** Accepted
- **Date:** 2026-06-28
- **Related:** [ADR-list-critical-decisions.md](ADR-list-critical-decisions.md),
  [../spec/01-lexical.md](../spec/01-lexical.md), N-FDL [ADR-001](../../adr/ADR-list-critical-decisions.md)

## Контекст

Исходный проспект V1 предполагает замену плоского TOML-движка AirPulse
(`airpulse://rules`, priority-based matching) на streaming DAG property-graph
engine. Альтернативы: оставить flat TOML, либо AOT-компиляция правил в нативный
Rust. Решение влияет на всю архитектуру ([07-runtime.md](../spec/07-runtime.md)).

## Решение

**Streaming DAG graph engine** — partitioned property graph (`GraphStore` =
`DashMap<ScopeId, SubGraph>`), event-time watermark, WaitQueue deferred
evaluation, commutative confidence accumulation. Правила = данные
(`ProgramImage`, [06](../spec/06-ir-bytecode.md)); ядро не перекомпилируется
между ruleset-ами.

### Свойства модели
- Bipartite rules (evidence Event→Cause, decision Cause/Problem→Problem/Action).
- Many-to-many: один event → много causes (Rule 1); один cause → много problems (Rule 2).
- Temporal correlation via watermark (C4), Missing Data Paradox resolved (08 §5).
- AmbiguityNode autonomous synthesis (C5).

## Последствия

- `airpulse_dsl` crate-tree (07 §1): syntax/types/verify/ir/store/evaluator/catalog/diag.
- `#![deny(unsafe_code)]` (AirPulse-мандат); нет FFI в v1 (catalog встроен).
- IR = graph-walk intents, не bytecode-VM как N-FDL (06 §1).
- Migration parallel-run (plans/migration) — legacy TOML + ADGL side-by-side.

## Отклонённые альтернативы

- **Flat TOML**: не выражает temporal correlation, конкурирующие causes, ambiguity;
  priority-based matching nondeterministic при tied priority.
- **AOT rules-compiler в нативный Rust**: +perf, −safety-audit-surface, −hot-reload,
  −изоляция (ядро перекомпилируется). Не нужно для v1 throughput (ingest дешёвый, 07 §8).
