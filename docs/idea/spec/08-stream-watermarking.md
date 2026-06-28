# ADGL Stream Watermarking & Deferred Evaluation v1

Определяет watermark-политику, отложенное вычисление correlate (Missing Data
Paradox), late events, idle-source, lifecycle WaitQueue и GC-инвариант. Аналог
N-FDL [../../spec/08-stream-reassembly.md](../../spec/08-stream-reassembly.md),
но вместо reassembly — event-time watermarking (Flink-style). Crate-владелец
`airpulse_dsl::store` + `evaluator`. Решает C4/D1..D5 (ADR-004).

## 1. Два режима источника

```
RunMode ::= Offline (PCAP replay) | Live (capture stream)
```

- **Offline**: события в capture-order (могут быть out-of-order по event-time,
  если capture таков). Watermark = max seen event-time (§2.1).
- **Live**: события real-time, возможны disorder/latency. Watermark =
  `max_seen - bounded_out_of_orderness` (§2.2).

Режим задаётся `RunMode` в `ActionSink` (07 §7) и определяет watermark-стратегию.

## 2. Watermark-политика

### 2.1 Offline (replay)

```
wm(t) = max(wm_prev, t)            для каждого ingested event time t
advance: wm monotone non-decreasing
```

Простая max-стратегия: в replay нет реальной lateness — «опоздание» =
out-of-order в capture. PendingMatch исполняется, когда `wm > upper_bound`.
Late events (§4) возможны только если capture сам out-of-order.

### 2.2 Live (bounded-out-of-orderness)

```
wm(t) = max(wm_prev, t - W)        W = max_disorder (config, ADR-011)
```

`W` = bounded-out-of-orderness (Flink `forBoundedOutOfOrderness`). События с
`time ≤ wm` после его продвижения — late (§4). Выбор `W` —
latency-vs-completeness tradeoff (Risks, §7): больший `W` ⇒ больше pending
(память), меньший `W` ⇒ больше false-absent в correlate.

### 2.3 Idle source (Flink-min)

В live multi-source: watermark = `min` по active sources. Источник без событий
`> idle_timeout` → excluded из min (не блокирует global watermark). При
возобновлении — re-include. Это предотвращает «застрявший» watermark, когда одна
партиция молчит (напр. icmp-источник редко шлёт PTB).

```
global_wm = min { src.wm | src.active }
src.active = (now - src.last_event_time) < idle_timeout
```

Offline: idle-source не применим (capture конечен, нет wall-clock).

## 3. Deferred evaluation & WaitQueue lifecycle (C4)

### 3.1 Suspend

При anchor-match (03 §3.1) движок вычисляет `upper_bound` каждого correlate:

```
upper = anchor.time + max_correlate_forward_window
```

Если `upper > wm` (есть незакрытый forward-окно) → `PendingMatch{ rule,
anchor_event, upper, scope }` → `WaitQueue[scope]` (BinaryHeap min по `upper`).
Иначе (`wm >= upper`, включая backward-only `upper == wm` — Example 8) —
немедленное исполнение. Resume (§3.2) — строго при `wm > upper_bound`; это
согласовано: suspended-pending всегда имеет `upper > wm_at_suspend`, поэтому
resume-условие `wm > upper` корректно его закрывает (никакой pending не suspended
при `upper == wm` ⇒ границы `wm == upper` для suspended не возникает).

### 3.2 Resume

```
on advance_watermark(wm):
  for scope in pending:
    while WaitQueue[scope].peek().upper_bound < wm:
      m = pop()
      // anchor_event обязан быть в RingBuffer (GC-инвариант §6)
      run_correlate_and_body(m.rule, m.anchor_event, m.scope)
```

Resume исполняет correlate против текущего RingBuffer (все события окна уже
прибыли, т.к. `wm > upper`). `present`/`absent`/`Unknown` — 03 §3.2/§3.7.

### 3.3 Bounded pending (D5)

`|WaitQueue[scope]| ≤ max_pending_per_scope` (ADR-011). При переполнении —
spill: drop pending с **наибольшим** `upper_bound` (наименее срочный) +
diagnostic `ADGL3004 WaitQueueSpill`. Это держит память bounded; заявление
«O(1) memory» из проспекта V1 корректируется: **amortized O(active windows)**,
не O(1); bounded константой `max_pending_per_scope`.

### 3.4 End-of-stream flush (offline / shutdown)

Capture конечен (offline): если последний event имеет `time ≤ upper` для
некоторого pending, `wm` никогда не превысит `upper` из новых событий ⇒
absent-branch не разрешится (strict resume §3.2 требует `wm > upper`). Поэтому
на закрытии capture движок flush'ит:

```
on end_of_stream:
  wm := +∞ (sentinel)        // либо last_event_time + max_forward_window + 1
  run resume-loop §3.2       // pop'ит все remaining pending (upper_bound < +∞)
  gc final                  // evict всё
```

Это корректно: после конца capture новых событий не прибудет, поэтому
absent-утверждения валидны (окно закрыто фактом конца потока, не watermark).
Late-event policy (§4) после flush — N/A (capture закрыт). Live-режим flush'ит
аналогично на shutdown. Без этого flush'а golden-тест PMTUD-absent (retrans без
PTB, PCAP заканчивается до `rtx.time + 1s`) никогда не триггерил бы
`absent`-action — регрессия determinism (12 §3.2).

## 4. Late events (D2)

Событие `e` с `e.time ≤ wm` (после продвижения wm):

```
Offline: accept (capture-order = truth); если уже было pending-resolved по
         absent-branch, e считается "late evidence":
         - если matching correlate уже resolved absent → audit `ADGL3002 LateEvidence`
           + НЕ ретроактивно переприменяет infer (append-only provenance).
         - если pending ещё не resolved → e попадает в RingBuffer до resume.
Live:   по умолчанию drop + side-output audit (`ADGL3003 LateEventDropped`).
         Опц. `allowed_lateness` (config) — окно после wm, в течение которого e
         принимается и может re-fire pending (как Flink allowed-lateness).
```

Late-event policy — explicit, не silent drop. Audit-side-output используется
для дифференциального тестирования (12 §differential) и для объяснения
«почему гипотеза не подтвердилась» в SARIF `incompleteFingerprints`.

## 5. Missing Data Paradox — разрешение (C4)

Парадокс: «assert a negative you must wait» — чтобы утверждать
`absent(ptb)`, нужно дождаться конца окна. Решение:

```
1. anchor rtx срабатывает.
2. correlate ptb имеет forward window [rtx.time, rtx.time + 1s].
3. Rule Instance suspended в WaitQueue (upper = rtx.time + 1s).
4. wm продвигается выше rtx.time + 1s (wm > upper; в replay — следующий пакет с
   time > upper, либо end-of-stream flush §3.4; в live — wall-clock + W).
5. Resume: scan RingBuffer[icmp.ptb] в окне.
   - found → present(ptb) = true → infer +85.
   - not found → absent(ptb) = true → infer +35 + action request_observation.
   - topology Unknown → Unknown → action request_topology (C10).
```

Без race conditions: решение принимается строго после `wm > upper`.
Детерминизм сохранён (03 §6).

## 6. GC-инвариант (D3)

```
MAX_LOOKBACK > max(max_backward_window, max_forward_window) + slack
slack ≥ 0 (config), по умолчанию = 0 (верификатор гарантирует strict, 05 §3.1)

anchor_event evict из RingBuffer когда: wm - ev.time > MAX_LOOKBACK
PendingMatch для ev активен пока: wm < ev.time + max_forward
                                       ≤ ev.time + MAX_LOOKBACK (т.к. forward < MAX_LOOKBACK)
⇒ ev не evicted, пока pending активно. QED.
```

Верификатор (05 §3.1) доказывает `forward ≤ MAX_LOOKBACK - slack` ⇒ инвариант
выполнен статически. Runtime GC (07 §4) выполняет eviction строго по правилу.

## 7. Risks & tradeoffs

- **Live latency**: correlate с forward window `F` ⇒ эмиссия задержана на `F + W`
  (W = bounded-out-of-orderness). Математическая необходимость (§5). UI помечает
  «Hypothesis Pending Data» (как в проспекте V1 §11).
- **`W` tuning**: малый W ⇒ false-absent (PTB пришёл позднее W после anchor).
  Mitigation: `allowed_lateness` (§4) + audit-side-output для re-fire.
- **Pending memory**: `max_pending_per_scope` spill может потерять диагноз на
  экстремальных burst-captures. Mitigation: config + diagnostic + дифференциальный
  тест vs legacy (12).
- **Idle timeout**: слишком короткий ⇒ false-idle (watermark прыгает), слишком
  длинный ⇒ stale watermark. Default 30s (config).

## 8. Контракт подсистемы

1. Watermark monotone (`fetch_max`); никогда не откатывается.
2. Offline: `wm = max_seen`; Live: `wm = max_seen - W`, idle-source excluded.
3. PendingMatch исполняется строго при `wm > upper_bound` (no race).
4. `|WaitQueue[scope]| ≤ max_pending_per_scope`; spill + audit (C11).
5. Late events: offline accept+audit, live drop+side-output (опц. allowed_lateness).
6. GC-инвариант: `MAX_LOOKBACK > max(back, forward)` ⇒ anchor живёт до resume.
7. Missing Data Paradox разрешается детерминированно (§5); `present`/`absent`/`Unknown`
   трёхзначны (C10).
