# ADR-006 — Bipartite isolation (resolves C6)

- **Status:** Accepted
- **Date:** 2026-06-28
- **Related:** [ADR-list](ADR-list-critical-decisions.md), [../spec/03-semantics.md](../spec/03-semantics.md) §2,
  [../spec/05-verification.md](../spec/05-verification.md) §8

## Контекст

Prospect 1 заявляет «evidence rules only mutate internal graph state; decision
rules only act on aggregated graph states». Это **ложно**: evidence-правила
эмитят `action request_observation` (Examples 5, 10). Также Example 8
(`decision suppress_downstream { anchor downstream: Problem(...) }`) ставит
decision-anchor на Problem, что выходит за «Cause → Problem» бипартию.

## Решение

**Refined bipartite** — изоляция по типу эффекта, не по «только graph state».

### Правила
- **Evidence rule** body ∈ `{ InferStmt, ActionStmt }` — НЕ `EmitStmt`.
- **Decision rule** body ∈ `{ EmitStmt, ActionStmt }` — НЕ `InferStmt`.
- `action` допустим в **обоих** слоях (G1).
- **Decision-anchor ∈ { Cause, Problem }** (G3): Cause-anchor реагирует на
  `ConfidenceMutation` (03 §3.5); Problem-anchor реагирует на `ProblemEmission`
  (Example 8 suppression).

### Изоляция (что запрещено)
- Evidence rule → `emit Problem` ⇒ `ADGL0450 BipartiteViolation` (05 §8).
- Decision rule → `infer Cause` ⇒ `ADGL0450`.
- Это сохраняет суть бипартии: evidence не создаёт Problems (агрегированный
  вывод), decision не создаёт Causes (не интерпретирует сырые events).

### Many-to-many
- Один event → много causes (Rule 1, evidence).
- Один cause → много problems (Rule 2, decision).
- Decision на Problem → action suppression (Rule 8) — Problem-level, не Cause.

## Последствия

- Верификатор §8 проверяет bipartite (05 §8).
- Пример 8 валиден (Problem-anchor + `suppress_symptom`).
- `action` в evidence — первый-class (request_observation для active feedback,
  Phase 3, 13-roadmap M5).

## Отклонённые альтернативы

- **Строгое (evidence только Cause, decision только Problem)**: противоречит
  Examples 5/10 (request_observation) и Example 8 (Problem-anchor).
- **Единый тип правил**: теряется изоляция + Many-to-many декомпозиция.
