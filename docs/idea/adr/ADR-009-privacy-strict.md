# ADR-009 — Privacy strict mode (resolves C9)

- **Status:** Accepted
- **Date:** 2026-06-28
- **Related:** [ADR-list](ADR-list-critical-decisions.md), [../spec/10-catalog-abi.md](../spec/10-catalog-abi.md) §11,
  [../spec/11-error-diagnostics.md](../spec/11-error-diagnostics.md) §5, AirPulse `l3_privacy: strict`

## Контекст

Prospect V1 test plan: «$rtx.dst_ip mapped to a Scope resolves perfectly during
graph evaluation, but is completely scrubbed from ProblemNode.evidence JSON
output when strict_privacy is flagged». Но модель redaction не определена: какие
поля PII, где redact, как сохранить reproducibility внутри движка. AirPulse
уже имеет `l3_privacy: strict` (redacts SNI, DNS↔TLS, cert subjects, per-flow UDP
IPs) — нужно зеркало.

## Решение

**Поле-level redaction** в evidence JSON; PII сохраняются внутри графа.

### PII-поля (catalog-маркировка, 10 §2)
- `dst_ip`, `src_ip` (IP), `client_mac`, `bssid` (MAC), `sni` (TLS), `vlan_id`
  (по config — может быть quasi-identifier).
- Каждое PII-поле в event-схеме помечено `[pii]` (04 §6.1).

### Redaction
- `redact_evidence(evidence, store, strict)` (10 §11): в strict режиме PII в
  evidence JSON → `"<redacted>"`.
- Внутри графа: `ScopeId` (hash от PII-компонентов), индексация, correlation —
  PII сохраняются (reproducibility). Redact **только** на выводе.
- `Intent.pii` mask (06 §2.3, 05 §10) — собрано AOT; runtime redact без
  повторного catalog-анализа.
- SARIF `partialFingerprints.target` = hashed (не raw IP) (ADR-008).

### Telemetry
- Runtime telemetry (если включена) — только non-PII агрегаты (counts,
  confidence distributions, scope-types). Никогда raw PII.

## Последствия

- Catalog: `[pii]` annotations (10 §2).
- `Intent.pii: Box<[FieldPath]>` (06 §2.3).
- runtime config: `strict: bool` (10 §11 `redact_evidence`; orthogonal to
  `RunMode`, applies to both Offline/Live — 07 §7 `RunMode` enum несёт только
  sink-dispatch, не privacy-флаг).
- Тест: `privacy strict mode` (12 §3) — dst_ip в graph, `<redacted>` в JSON.

## Отклонённые альтернативы

- **Без privacy-режима**: leak PII в SARIF/JSON.
- **Hash-pseudonymization everywhere**: ломает reproducibility (different hash
  per salt) + корреляция внутри графа сложнее.
- **Redaction внутри графа (не только вывод)**: теряется ability correlating
  по raw key; `ScopeId` уже hash — достаточно.
