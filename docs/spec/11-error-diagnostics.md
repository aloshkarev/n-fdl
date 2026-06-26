# N-FDL Error & Diagnostics Model v1

Унифицирует модель ошибок всех фаз, диагностику уровня rustc для автора DSL,
event bus и телеметрию без утечки чувствительных данных. Реализуется в crate
`nfdl-diag` (типы ошибок, шина) с участием каждой фазы как источника.

## 1. Таксономия ошибок

Две большие категории: **AOT** (на загрузке спеки, спека отвергается целиком) и
**Runtime** (на разборе пакета, локальная деградация без падения ядра).

```
Error =
  // ── AOT (load-time): спека некорректна, ядро не загружает её ──
  | SyntaxError        // лексер/парсер: грамматика нарушена
  | TypeError          // типизация: несовпадение типов, plugin-sig, scope
  | VerificationError  // верификация: cycle, unprovable-strict bounds, FSM dead-state, ...
  // ── Runtime (per-packet): спека верна, данные/условия проблемны ──
  | ConstraintError    // validate=false, runtime bounds-check, underflow, div0
  | NeedMoreBytes      // stream: данных недостаточно (НЕ ошибка — управляющий сигнал)
  | Malformed          // агрегат: пакет/ветка не разобраны (constraint/depth/limit)
  | PluginError        // FFI вернул error / нарушил контракт / timeout
  | RuntimeSafetyAbort // non-progress loop, max_iter, max_depth — защита, не падение
```

### 1.1 Контракт фаз

| Ошибка | Фаза | Эффект | Паника ядра? |
|---|---|---|---|
| SyntaxError | parse | спека не загружена | нет |
| TypeError | typecheck | спека не загружена | нет |
| VerificationError | verify | спека не загружена | нет |
| ConstraintError | VM | ветка/сообщение → Malformed | **никогда** |
| NeedMoreBytes | VM (stream) | yield континуации | нет |
| Malformed | VM/dispatcher | пакет помечен, частичный AST | **никогда** |
| PluginError | FFI | ветка → Malformed, плагин изолирован | **никогда** |
| RuntimeSafetyAbort | VM | разбор прерван безопасно | **никогда** |

**Инвариант:** после verify-границы (05 §10) ни одна runtime-ситуация не вызывает
panic. Все runtime-ошибки recoverable и локализованы. `catch_unwind` в harness —
страховка, а не штатный путь (любой panic = баг, ловится fuzzing 13).

## 2. NeedMoreBytes — не ошибка

Подчёркнуто отдельно: `NeedMoreBytes` — управляющий сигнал потоковой машины
(06 §5, 08 §5), а не ошибка. Возникает только в `mode=stream`. Несёт
`PendingRead` (сколько байт / ожидание EOF). Никогда не достигает пользователя
как «ошибка» — обрабатывается reassembly-оркестрацией.

## 3. Диагностика для автора DSL (AOT)

Цель — качество rustc: точный span, подсветка, объяснение, подсказка.

```
Diagnostic {
    id: DiagId,            // стабильный код, напр. "NFDL0412" (для документации/подавления)
    severity: Error | Warning | Note,
    span: Span,            // file:line:col + byte-range из 01-lexical
    message: String,       // основное сообщение
    labels: Vec<(Span, String)>,   // вторичные подсветки ("здесь объявлено length")
    help: Option<String>,  // как исправить
}
```

Пример (bounds, 05 §3.1):
```
error[NFDL0412]: `bytes[length - 20]` may underflow
  ┌─ radius.nfdl:15:16
  │
15│         value: bytes[length - 20];
  │                      ^^^^^^^^^^^ `length` has range [0, 65535]; `length - 20`
  │                                  can be negative
  │
help: add `validate length >= 20 -> "..."` before this field, or the access will
      be runtime-checked (ConstraintError on violation)
```

Несколько ошибок собираются батчем (не падаем на первой) и сортируются по span.
`DiagId` стабилен → можно документировать каждый код и подавлять warnings адресно.

## 4. Runtime-диагностика (per-packet)

При `Malformed`/`ConstraintError`/`PluginError` ядро НЕ падает, а эмитит
структурированную runtime-диагностику в event bus:

```
RuntimeDiag {
    diag_id: DiagId,             // тот же код, что в validate "msg" или встроенный
    kind: ConstraintError | Malformed | PluginError | RuntimeSafetyAbort,
    layer_path: Vec<ProtoId>,    // где случилось: Ethernet→IPv4→UDP→DNS
    offset: u64,                 // позиция в пакете (root_offset)
    message: String,             // из validate "msg" или системное
    partial_ast: Option<MsgRef>, // что успели разобрать до ошибки
}
```

Частичный AST сохраняется — анализ malformed-пакетов важнее, чем «всё или ничего».

## 5. Event Bus

Однонаправленный sink; **без обратного влияния на парсинг** (события не могут
изменить ход разбора → детерминизм сохраняется).

```
Event =
  | Message(MsgRef)                      // успешно разобранное сообщение (AST)
  | FsmTransition { key_hash, from, to, machine }
  | Emit { name, key_hash }              // из FSM action `emit`
  | SessionExpired { key_hash, machine }
  | Diagnostic(RuntimeDiag)
  | Anomaly(AnomalyKind)                 // TcpOverlap, Retransmit, DepthExceeded, ...
EventBus { sink: trait EventSink }       // pluggable: stdout/JSON, file, SIEM, тест-collector
```

Порядок событий тотально детерминирован (09 §8) → differential-тесты
воспроизводимы. `EventSink` — trait: golden-тесты используют in-memory collector,
prod — JSON/PDML-сериализатор.

## 6. Телеметрия без утечки чувствительных данных

### 6.1 Принцип privacy-by-default

События несут **метаданные**, не сырой payload:
- Ключи сессий — **хешированные** (`key_hash`), не сырые IP/порты (в privacy-режиме).
- Чувствительные поля (authenticator, ключи, токены, payload) — **handle/длина/
  hash**, не значение.
- Сырые байты в событиях — только при явном `--debug-payloads` флаге.

### 6.2 Маркировка чувствительности

DSL-кандидат (v1.5): аннотация `@sensitive` на поле → ядро никогда не помещает
его значение в телеметрию, только длину/hash. Без аннотации — консервативно:
bytes-поля и str по умолчанию не сериализуются в события (только метаданные),
скаляры — можно (порты/коды нечувствительны). Точная политика — конфиг.

### 6.3 Что всегда безопасно эмитить

Тип сообщения, layer-path, state-transitions, диагностик-коды, timestamps,
размеры, anomaly-типы. Этого достаточно для мониторинга/IDS без PII.

## 7. Маппинг runtime-ошибок на DoS-защиты (связь с 12 плана)

| RuntimeSafetyAbort/Malformed подвид | Триггер | Защита |
|---|---|---|
| NonProgressLoop | loop потребил 0 байт | 05 §5.2, VM consumed-check |
| LoopLimit | > max_loop_iterations | 08/конфиг |
| MaxDepthExceeded | bind-рекурсия > max_layer_depth | 07 §10, C7 |
| ReassemblyLimit | flow-буфер > лимита | 08 §6 |
| SessionLimit | > max_sessions | 09 §6.2 LRU |
| PluginTimeout | FFI > time-budget | 10 §6.2 |
| CompressionBomb | output > ratio cap | 10 §6.2 |

Все → безопасное прерывание + diagnostic/anomaly-событие, никогда не паника/OOM.

## 8. Контракт подсистемы

1. AOT-ошибки отвергают спеку с диагностикой уровня rustc (span+help+stable id).
2. Runtime-ошибки никогда не паникуют ядро; деградация локальна, AST частичен.
3. NeedMoreBytes — сигнал, не ошибка.
4. Event bus однонаправлен; не влияет на разбор; порядок детерминирован.
5. Телеметрия privacy-by-default: метаданные и hash, не сырой payload/PII.
6. Каждая DoS-защита порождает явный диагностик/anomaly, а не тихий сбой.
