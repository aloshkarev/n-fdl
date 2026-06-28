# ADR-005 — Ambiguity & mutual exclusivity (resolves C5)

- **Status:** Accepted
- **Date:** 2026-06-28
- **Related:** [ADR-list](ADR-list-critical-decisions.md), [../spec/03-semantics.md](../spec/03-semantics.md) §4,
  [../spec/10-catalog-abi.md](../spec/10-catalog-abi.md) §7

## Контекст

Prospect V1: AmbiguityNode синтезируется «autonomously when two mutually
exclusive Causes on the same Scope reach Probable status with confidence Δ<15».
Но **механизм объявления эксклюзивности отсутствует** — движок не может знать,
что PmtudBlackhole и Congestion эксклюзивны. Также не определён поток
AmbiguityNode → SARIF и lifecycle (resolve/supersede).

## Решение

**Явное `mutually_exclusive(...)`** + Autonomous synthesis + lifecycle.

### Объявление
- Ruleset-level: `mutually_exclusive(K1, K2, ...)` ([02 §1](../spec/02-grammar.ebnf)).
- Catalog-defaults: `catalog.exclusivity_defaults` (10 §7) — `(Congestion,
  PmtudBlackhole)`, `(Congestion, TransientL2Disruption)`,
  `(PhysicalCableDamage, PhysicalLinkAbsent)`, и др.
- Ruleset может дополнять/переопределять.

### Синтез (03 §4)
```
on ConfidenceMutation в scope SG:
  для каждой пары (K1, K2) ∈ exclusivity[SG.ruleset]:
    c1, c2 = Causes(K1/K2, common_target).confidence
    если c1 ∈ [40,79] и c2 ∈ [40,79] и |c1-c2| < 15:
      create AmbiguityNode + action mark_ambiguous
```

### Lifecycle
- **Resolve**: существующая Ambiguity, если `max(c1,c2) >= 80` (один стал
  Confirmed) или `|c1-c2| >= 15` (расхождение) → `state = Resolved`; SARIF-запись
  сохраняется (append-only).
- **Surfacing**: `mark_ambiguous` → `Problem(AmbiguousDiagnosis)` в SARIF с
  `partialFingerprints = {scope, target_hash, causes}` (C8).

### Well-formedness (05 §7)
- Все Ki ∈ catalog.causes; пары имеют общий достижимый target-scope (иначе
  warning `RedundantExclusivity`).
- Не-overlapping группы (одна пара в одной группе) — иначе
  `ADGL0440 OverlappingExclusivity`.

## Последствия

- Грамматика: `mutually_exclusive` decl (01 §3.1, 02 §1).
- Catalog: `exclusivity_defaults` (10 §7).
- Тест: `ambiguity synthesis` (12 §3) + example 10-ambiguity-demo.

## Отклонённые альтернативы

- **Авто-вывод эксклюзивности (эвристика)**: движок гадает; недетерминированно.
- **Без AmbiguityNode**: конкурирующие гипотезы молча — пользователь не видит
  неоднозначность.
