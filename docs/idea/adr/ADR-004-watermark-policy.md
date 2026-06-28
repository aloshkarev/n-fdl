# ADR-004 — Watermark & deferred evaluation (resolves C4)

- **Status:** Accepted
- **Date:** 2026-06-28
- **Related:** [ADR-list](ADR-list-critical-decisions.md), [../spec/08-stream-watermarking.md](../spec/08-stream-watermarking.md),
  [../spec/05-verification.md](../spec/05-verification.md) §3

## Контекст

Prospect V1 описывает deferred evaluation (watermarking) для Missing Data
Paradox, но не определяет: watermark-политику (replay vs live), late events,
idle source, MAX_LOOKBACK vs windows, WaitQueue bound. Заявление «O(1) memory»
ложно для небаунденного pending-множества.

## Решение

**Event-time watermark + WaitQueue** (BinaryHeap min по `upper_bound`).

### Политика (08 §2)
- **Offline (PCAP replay)**: `wm = max(wm_prev, t)`. Capture-order = truth.
- **Live**: `wm = max(wm_prev, t - W)`, `W = bounded_out_of_orderness` (Flink
  `forBoundedOutOfOrderness`).
- **Idle source**: `global_wm = min` по active sources; source без событий
  `> idle_timeout` excluded (Flink idle-source).

### Late events (08 §4)
- Offline: accept + audit `ADGL3002 LateEvidence`; append-only provenance ⇒
  не ретроактивно переприменяет resolved-absent.
- Live: drop + side-output `ADGL3003`; опц. `allowed_lateness` для re-fire.

### MAX_LOOKBACK invariant (05 §3.1, 08 §6)
- `MAX_LOOKBACK > max(max_backward, max_forward) + slack`.
- Верификатор доказывает `forward ≤ MAX_LOOKBACK - slack` статически (hard error
  иначе). ⇒ anchor-event живёт в RingBuffer до resume.

### WaitQueue bound (D5)
- `|WaitQueue[scope]| ≤ max_pending_per_scope` (ADR-011); spill drop oldest
  pending с **наибольшим** upper_bound + `ADGL3004`.
- Корректировка «O(1) memory» проспекта: **amortized O(active windows)**,
  bounded `max_pending_per_scope`.

## Последствия

- Live latency = `forward + W` (математическая необходимость, 08 §7); UI
  «Hypothesis Pending Data».
- `W`/`idle_timeout`/`allowed_lateness` — config (ADR-011).
- Тесты: `WaitQueue correctness`, `flat-memory GC`, `late-event audit` (12 §3).

## Отклонённые альтернативы

- **Sync (no future-window)**: нельзя выразить `absent(ptb)` корректно (Missing
  Data Paradox).
- **Micro-batch windows**: менее точно, latency batch-period.
- **Unbounded WaitQueue**: DoS / OOM на burst-captures.
