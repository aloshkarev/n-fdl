# ADR-002 — Confidence scale (resolves C2)

- **Status:** Accepted
- **Date:** 2026-06-28
- **Related:** [ADR-list](ADR-list-critical-decisions.md), [../spec/04-type-system.md](../spec/04-type-system.md) §2,
  AirPulse `airpulse://schema/verdict`

## Контекст

Prospect V1 использует confidence 0..100 с порогами Candidate(10–39) /
Probable(40–79) / Confirmed(80+). Существующий AirPulse verdict
(`airpulse://schema/verdict`) использует `confidence: number 0..1` (0 = unsure,
1 = certain). План миграции проспекта игнорирует конвертацию шкал — это блокер
для parallel-run и SARIF-совместимости.

## Решение

**0..100 внутри движка**; маппинг `/100` → 0..1 в legacy verdict и SARIF на
границе вывода.

### Форма
- `Confidence: u8` (0..100), `Weight: i8` (-100..+100) — newtype, не `Int` (04 §2).
- Threshold-псевдо-значения Candidate/Probable/Confirmed — предикаты, не типы (04 §2.1).
- Коммутативная формула: `C_new = clamp(0, 100, C_old + W)` (03 §3.3).
- Legacy mapping: `legacy_confidence = confidence / 100.0` (точность 0.01) в
  SARIF/JSON `verdict.confidence` (11).
- Thresholds в decision-anchor (`c.confidence >= 80`) — на 0..100 шкале.

## Последствия

- Float отсутствует в горячем пути (`#![deny(unsafe_code)]`, checked i64/i8).
- Legacy `confidence` 0..1 сохраняется в JSON/SARIF (миграционная совместимость).
- `fired_rule_ids` legacy → mapping в `sarif_id` (ADR-008).
- Тест: `confidence commutativity` (12 §3) + differential parity (12 §1.4).

## Отклонённые альтернативы

- **0..1 внутри**: float в hot-path, пороги нечитаемы (`0.8` vs `80`), float-округление.
- **Обе шкалы параллельно**: дублирование, risk рассогласования.
