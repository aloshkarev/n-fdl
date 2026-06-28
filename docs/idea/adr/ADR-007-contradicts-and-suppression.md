# ADR-007 — Contradicts & suppression (resolves C7)

- **Status:** Accepted
- **Date:** 2026-06-28
- **Related:** [ADR-list](ADR-list-critical-decisions.md), [../spec/03-semantics.md](../spec/03-semantics.md) §3.3–3.4,
  [../spec/04-type-system.md](../spec/04-type-system.md) §4

## Контекст

Prospect V1 декларирует `EvidenceEdge` типы `Supports, Contradicts, Explains`, но
синтаксис `infer` показывает только `+weight`. Нет способа выразить
противоречащее доказательство. Также Example 8 использует `action
suppress_symptom` (Problem-level suppression) — отдельный механизм от
Cause-level Contradicts. Не определена Problem retraction при падении cause.

## Решение

**Два механизма** + append-only retraction.

### 1. Cause-level: `weight: -N` → Contradicts
- `infer Cause(K) { weight: -30, ... }` → `EvidenceEdge::Contradicts`,
  `C_new = clamp(0, C_old + (-30))` (03 §3.3).
- Декремент с floor 0. Семантика: «доказательство против гипотезы K».
- `Explains` edge — нейтральная корреляция (weight 0); v1.5 может добавить
  явный `explains` clause.

### 2. Problem-level: `action suppress_symptom(p)`
- Для топологической маскировки (Example 8: upstream Problem маскирует
  downstream) — не декремент confidence, а mark Problem `superseded=true`.
- `Suppresses` edge: `ProblemNode <- ProblemNode` (04 §4).

### 3. Problem retraction (F4)
- Problem-эмиссия **append-only** (03 §3.4).
- При падении cause ниже порога (после Contradicts) — Problem не удаляется, а
  помечается `superseded=true` (SARIF `baselineState: absent` в следующем run,
  C8).
- Аудит-трейл сохраняется; differential testing стабилен.

## Последствия

- Грамматика: `weight: "+" IntLit | "-" IntLit` (02 §5).
- `ProblemNode.superseded: Bool` (04 §3).
- Тест: `confidence commutativity` с negative weights (12 §3).

## Отклонённые альтернативы

- **Только `suppress_symptom`**: нет способа ослабить гипотезу противоречащим
  доказательством (Cause-level).
- **Только negative weight**: нет топологической маскировки (upstream/downstream
  — разные Problems, не один Cause).
- **Problem deletion**: ломает append-only + SARIF stable IDs + differential.
