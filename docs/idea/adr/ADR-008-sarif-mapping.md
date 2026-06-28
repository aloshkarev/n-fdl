# ADR-008 — SARIF mapping (resolves C8)

- **Status:** Accepted
- **Date:** 2026-06-28
- **Related:** [ADR-list](ADR-list-critical-decisions.md), [../spec/11-error-diagnostics.md](../spec/11-error-diagnostics.md),
  [../spec/10-catalog-abi.md](../spec/10-catalog-abi.md) §4, AirPulse `airpulse://rules`

## Контекст

Prospect V1 migration plan: «Ensure JSON/SARIF output stable IDs (AP/L3/XL-ICMP-PTB)
exactly match». Но DSL эмитит `Problem(XlIcmpTcpMss)` — не определено, как
Problem-имя → SARIF `ruleId`, и как обеспечить legacy 1:1 + dedup между прогонами.
SARIF 2.1.0 (OASIS) требует стабильный `ruleId` (§3.27.5) + `partialFingerprints`
(§3.27.17) для dedup; NOTE: «symbolic/numeric rule id more likely to remain stable
than descriptive string».

## Решение

**Явный `sarif_id`** + `partialFingerprints` + legacy mapping.

### ruleId (C8)
- `emit Problem(P) { sarif_id: "..." }` — явный stable symbolic ID (02 §5).
- Catalog-default: `sarif_id` из Problem-схемы (10 §4), напр.
  `XlIcmpTcpMss → "l3_pmtud_blackhole"`.
- Символический (не descriptive) — стабильнее (SARIF §3.27.5 NOTE).
- `ruleId` в SARIF = `sarif_id`; `reportingDescriptor` (§3.49) в `run.tool.driver.rules`.

### partialFingerprints (§3.27.17)
- `{ "scope": <scope_type>, "target": <target_hash>, "causes": [<cause_kinds>] }`.
- `target_hash` — hashed (privacy, C9) но стабильный для dedup между прогонами.
- GitHub code-scanning использует `partialFingerprints` для матчинга результатов
  (§3.27.17 + GitHub docs).

### Legacy mapping (migration)
- `sarif_id` для legacy-covered диагнозов = legacy `recommendation_id`
  (verified vs `airpulse://rules`): PMTUD `l3_pmtud_blackhole` (XlIcmpTcpMss,
  rec of `l3_icmp_tcp_blackhole_loss`), STP `l3_stp_spanning_tree`
  (SpanningTreeInstability), dot1x `l3_dot1x_wired` (ClientOnboardingFailure).
  Новые ADGL диагнозы (`ap_*`: CableDisconnected, WlanRadiusOutage,
  DeviceUnreachable, AmbiguousDiagnosis) — свежие стабильные symbolic IDs
  (SARIF §3.27.5), legacy-эквивалента нет (legacy flat-TOML — wired TCP/L3 only).
- **Many-to-one**: legacy имеет несколько `rule.id` на диагноз (PMTUD:
  `l3_icmp_ptb_with_loss`, `l3_icmp_tcp_mss_loss`, `l3_icmp_tcp_mss_rst`,
  `l3_icmp_tcp_blackhole_loss` → recs `l3_pmtud_investigate`/`l3_pmtud_blackhole`);
  ADGL консолидирует в один Problem. Migration adapter хранит mapping
  `legacy_rule_id → ADGL sarif_id`.
- Parallel-run: parity на **sarif_id/verdict level** (не raw `fired_rule_id`
  equality — невозможно при many-to-one) via adapter mapping; ≥95% agreement
  (plans/migration §2, 12 §1.7).
- Confidence: `legacy 0..1 = confidence/100` (ADR-002).

## Последствия

- `EmitStmt.sarif_id` field (02 §5, 04 §6.2).
- `ProblemNode.sarif_id` (04 §3), `Intent.EMIT_PROBLEM sarif_id_idx` (06 §4).
- `redact_evidence` + hashed target в fingerprints (10 §11).
- Тест: differential stable-ID parity (12 §1.4), M6 gate.

## Отклонённые альтернативы

- **`ruleId` = Problem-имя (`XlIcmpTcpMss`)**: не совпадает с legacy
  `l3_pmtud_blackhole` ⇒ ломает parallel-run.
- **`ruleId` = legacy строка без catalog-default**: каждый `emit` обязан
  повторять ID ⇒ дублирование, риск опечаток.
- **Без `partialFingerprints`**: GitHub code-scanning не сможет dedup между
  прогонами (§3.27.17).

## Внешние ссылки

- SARIF 2.1.0: https://docs.oasis-open.org/sarif/sarif/v2.1.0/sarif-v2.1.0.html
  (§3.27.5, §3.27.17, §3.49).
