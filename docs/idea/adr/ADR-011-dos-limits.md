# ADR-011 — DoS limits (resolves C11)

- **Status:** Accepted
- **Date:** 2026-06-28
- **Related:** [ADR-list](ADR-list-critical-decisions.md), [../spec/05-verification.md](../spec/05-verification.md) §9,
  [../spec/07-runtime.md](../spec/07-runtime.md) §4, N-FDL [../../spec/01-lexical.md](../../spec/01-lexical.md) §8

## Контекст

Prospect V1 заявляет «O(1) memory bounding» и `#![deny(unsafe_code)]`, но не
определяет DoS-лимиты. Небаунденный WaitQueue, RingBuffer, cause-count,
topology-hops, rule-firings — все OOM-векторы. N-FDL имеет explicit лимиты
(§8 lexer, runtime DoS) — нужно зеркало.

## Решение

**Configurable лимиты с дефолтами** (mirror N-FDL §8 + runtime).

### Lexer/parser (01 §8, 05 §9)
| Лимит | Default | Diagnostic |
|---|---|---|
| max token length | 255 B | `ADGL0101` |
| max source size | 4 MiB | `ADGL0102` |
| max nesting | 64 | `ADGL0103` |
| max `requires` entries | 32 | `ADGL0105` |
| correlate blocks per rule | 8 | `ADGL0204` |
| correlate `having` min match | 1..32 | `ADGL0504` / `ADGL0505` |
| infer/emit per rule body | 16 | `ADGL0205` |

### Runtime (07, 08)
| Лимит | Default | Diagnostic | Spill |
|---|---|---|---|
| `max_ringbuffer_events_per_scope` | 4096 | `ADGL3005` | drop oldest |
| `max_pending_per_scope` | 1024 | `ADGL3004/3101` | drop largest upper_bound |
| `max_causes_per_scope` | 256 | `ADGL3102` | reject new + audit |
| `max_rule_firings_per_event` | 64 | `ADGL3103` | stop + audit |
| `max_topology_hops` | 16 | `ADGL3006` | `False` + audit |
| `MAX_LOOKBACK` | 60 s | `ADGL0412` (AOT) | hard error |
| `max_forward_window` | `MAX_LOOKBACK - slack` | `ADGL0412` | hard error |
| `dedup_window` | 1 s | — | — |

### Invariants (verifier-enforced, 05 §3.1/§9)
- `MAX_LOOKBACK > max(max_backward, max_forward) + slack`.
- `forward ≤ MAX_LOOKBACK - slack` (hard AOT error иначе).
- `dedup_window ≥ 1ms`.

### Spill policy
- RingBuffer: drop oldest (lowest time) + `ADGL3005`.
- WaitQueue: drop pending с **наибольшим** upper_bound (наименее срочный) +
  `ADGL3004`.
- Все spill — degrade + diagnostic, не паника (07 §9).

## Последствия

- Config-структура `Limits` (07 §3, 08).
- Верификатор §9 проверяет AOT-лимиты; runtime — spill.
- Тест: `flat-memory GC` (12 §3) на loop-PCAP с лимитами.

## Отклонённые альтернативы

- **Без лимитов (доверие к спеке)**: OOM на adversarial/burst captures.
- **Фиксированные hardcoded**: негибко (разные deployment: edge vs datacenter).
