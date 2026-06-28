# ADR-003 — Scope vs target (resolves C3)

- **Status:** Accepted
- **Date:** 2026-06-28
- **Related:** [ADR-list](ADR-list-critical-decisions.md), [../spec/09-scopes-sessions.md](../spec/09-scopes-sessions.md),
  [../spec/04-type-system.md](../spec/04-type-system.md) §5

## Контекст

Prospect V1 слитно использует `scope` и `target`. Example 5: `scope: ClientMac`,
`infer Cause(AuthServerOutage) { target: dhcp.vlan }`. Example 2: `scope: Vlan`,
`anchor c: Cause(AuthServerOutage)`. При partition-by-scope Cause в ClientMac
partition не виден decision-правилу в Vlan partition — cross-scope диагностика
молча ломается. Также `Global` scope (Example 8) не определён.

## Решение

**`scope` = partition key** (где rule исполняется); **`target` = сущность**
гипотезы/проблемы. Иерархия `ClientMac ⊂ Vlan ⊂ Global`, `Session ⊂ Vlan`,
`Port ⊂ Global`, `AccessPoint ⊂ Global` (04 §5). Cross-scope roll-up.

### Roll-up (09 §3.2)
- `rule.scope` ⊑ `target`-scope (target — ancestor-or-equal rule.scope; иначе `TypeError::ScopeTargetMismatch`, 05 §4).
- При `ConfidenceMutation` в child-scope с `target: t_parent` → parent-scope
  Cause confidence = **MAX** over children (не sum).
- `MAX` избегает double-counting: 3 клиента с одним AuthServerOutage ⇒ parent
  confidence = max child, не 3×.
- Roll-up → `ConfidenceMutation` в parent → trigger parent decision (03 §3.5).

### Global singleton (09 §6)
- `Global` = один partition `ScopeId(GLOBAL, ())`. Точка сериализации.
- Не ломает lock-free: non-Global partitions остаются cross-lock-free; Global
  cross-cutting правила (suppression, Example 8) редкие.

## Последствия

- Event routing fan-out по всем scope, заявленным правилами для типа (09 §2, B4).
- `ScopeId` = canonical hash; Session uses `bidir_tuple` (09 §7, mirror N-FDL C4/C10).
- Примеры 02/04 исправлены: cross-scope roll-up (examples/).

## Отклонённые альтернативы

- **`target` ≡ `scope`**: ломает cross-scope (Example 5→2).
- **Глобальный граф без partitioning**: теряется lock-free + O(1) lookup
  (ключевое требование V1).
- **Roll-up = SUM**: double-counting одинаковых сбоев; завышает confidence.
