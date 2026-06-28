# N-FDL Critical Design Decisions (ADR-style)

Полный список решений, которые нужно принять **до кода**. Каждый пункт в формате: Options → Recommendation → Trade-offs.

## ADR-001 — Execution model

**Options**
- Bytecode VM (интерпретатор верифицированного IR)
- Rust codegen (AOT генерация нативного кода)
- JIT (cranelift/LLVM поверх IR)

**Recommendation**: Bytecode VM.

**Trade-offs**
- − производительность (5–15×)
- + resumability (NeedMoreBytes = IP + frame snapshot)
- + memory-safety (one place to audit bounds)
- + isolation (спека = данные, ядро не перекомпилируется)
- + time-to-market (загрузил .nfdl → работает)

Codegen/JIT выигрывают perf, но проигрывают по главным требованиям v1 (resumability + safety + изоляция).

---

## ADR-002 — Surface Syntax v1: Corrections (C1–C8)

**Детальный ADR:** [ADR-002-surface-syntax-corrections.md](ADR-002-surface-syntax-corrections.md)
(Status: Accepted, 2026-06-25; supersedes original syntax sketch in project brief §4/§6).

**Кратко:** консолидированный набор исправлений синтаксиса v1, вытекающих из
формальной модели (ациклический DAG полей + pure/effect + завершаемость).
Формальная модель — источник истины, синтаксис подгоняется под неё.

- **C1** — split `bytes[..]` (rest-of-slice) vs `bytes[EOF]` (stream-end; только в
  `mode=stream`, последнее поле; источник EOF объявляется в `meta eof = ...`).
- **C2** — loop-carried accumulator `carry` / `next` вместо `let`-мутации внутри
  цикла (fold/unfold; DAG ацикличен внутри итерации; `C2-progress` — потребление
  ≥1 байта за итерацию + `max_loop_iterations`).
- **C3** — layer-path scope (`IPv4.src`) резолвится по `layer_stack` dispatcher’а.
- **C4** — `bidir` / `bidir_tuple` для канонической направленно-независимой
  request/response корреляции.
- **C5** — interval/range analysis + runtime-downgrade; Z3 — опциональный
  verifier-backend за feature-flag в v1.5.
- **C6** — `match` → tagged union в Typed AST.
- **C7** — рекурсивная инкапсуляция: циклы в bind-графе разрешены, runtime
  ограничено `max_layer_depth` + payload-shrink invariant.
- **C8** — `__current_offset` (локальное) vs `__root_offset` (абсолютное).

> Решение по `let`-мутации/`carry` (C2) ранее занимало этот слот как
> самостоятельная запись списка; теперь оно — секция C2 детального ADR-002.

---

## ADR-003 — Bounds verification backend (C5)

**Options**
- Z3-mandatory в v1
- Interval/range analysis only
- Interval + runtime-downgrade (RuntimeCheck при недоказуемости)

**Recommendation**: Interval + runtime-downgrade; Z3 — опциональный feature `z3` в v1.5.

**Trade-offs**
- Некоторые проверки остаются в рантайме (−perf на горячих путях)
- Не блокируем v1 на Z3 (стабильность сборки, время компиляции)
- Safety гарантирована всегда (OOB → ConstraintError)

---

## ADR-004 — Zero-copy buffer model

**Options**
- Только `&'pkt [u8]` (borrowed)
- Только `bytes::Bytes` (shared)
- Hybrid `BufHandle` (Borrowed | Shared)

**Recommendation**: Hybrid `BufHandle`.

**Trade-offs**
- Две ветки кода (dispatch по enum)
- Оптимальность каждого режима: datagram — ноль ref-count overhead; stream — переживает сегменты
- Единый API для VM

Принято в 07-runtime.

---

## ADR-005 — Concurrency model

**Options**
- Single-threaded event loop
- Sharded (per-flow-hash)
- Async (tokio)

**Recommendation**: Single-threaded v1; sharded v2.

**Trade-offs**
- − throughput
- + детерминизм (тотальный порядок)
- + простота Session DB (без блокировок)
- Sharded v2 сохраняет share-nothing внутри shard

---

## ADR-006 — `match` result typing (C6)

**Options**
- Только общий layout во всех ветках
- Tagged union (разные layout)

**Recommendation**: Tagged union.

**Trade-offs**
- Сложнее типовая система (union в 04)
- Выразительность (Diameter grouped AVP)
- Дискриминатор сохраняется как тег

---

## ADR-007 — Bitfield alignment

**Options**
- Разрешить misalignment (неявное padding)
- Требовать явное выравнивание
- VerificationError при sum % 8 != 0 перед байтовым полем

**Recommendation**: VerificationError при нарушении.

**Trade-offs**
- Строже к автору DSL
- Предсказуемость, отсутствие скрытого поведения
- Простая проверка в verifier

---

## ADR-008 — Session key normalization (C4/C10)

**Options**
- Literal key (как в исходных примерах)
- `bidir(a, b)` primitive
- `bidir_tuple(endpoint_a, endpoint_b)` primitive
- Directionality-аннотация

**Recommendation**: `bidir()` для одиночных компонентов, `bidir_tuple()` для
endpoint keys + опциональная directionality.

**Trade-offs**
- Новый builtin
- Корректная req/resp корреляция (иначе baseline сломан)
- Устраняет C10: независимая сортировка IP и портов рассогласовывает endpoint

---

## ADR-009 — Record Types for Plugin Results (C9)

**Детальный ADR:** [ADR-009-plugin-record-types.md](ADR-009-plugin-record-types.md)
(Status: Accepted, 2026-06-25; related: ADR-002 C8 offset model, `04-type-system.md`, `udp_dns.nfdl`).

**Кратко:** `invoke` может возвращать `record{...}` (плоский, именованные поля,
иммутабельный, по значению) — единственная user-видимая record-форма в v1,
порождаемая исключительно сигнатурой плагина из манифеста. Доступ к полям через
`.` (`dec.wire_len`); типизируется статически, несуществующее поле →
`TypeError::NoSuchField`. Реализует C9.

> Решение о `bytes[EOF]` semantics split (C1) ранее занимало этот слот, но оно
> полностью покрыто в **ADR-002 (Surface Syntax, секция C1)** как часть
> консолидированных corrections C1–C8. Отдельной ADR-009 для EOF-split нет.

---

## ADR-010 — Plugin isolation v1

**Options**
- Trusted in-process (текущий)
- Worker-thread + hard-timeout
- WASM sandbox

**Recommendation**: Trusted in-process + контракт + watchdog (v1); WASM в v2.

**Trade-offs**
- − изоляция (зависший pure-плагин может заблокировать single-thread)
- + производительность
- + простота
- Pure-плагины обязаны иметь внутренние лимиты; тяжёлые выносятся в worker (v1.5)

---

## ADR-011 — Reassembly overlap policy

**Options**
- first-wins
- last-wins
- both-emit-anomaly

**Recommendation**: first-wins + anomaly-event (как Zeek).

**Trade-offs**
- Совместимость с TShark может расходиться → документировать
- first-wins проще и безопаснее (не перезаписываем уже принятое)

---

## ADR-012 — ProgramImage serialization

**Options**
- In-memory only
- Serializable artifact (serde, versioned)

**Recommendation**: Serializable, versioned.

**Trade-offs**
- + hot-reload спек, кэширование
- + формат-стабильность
- − нагрузка на формат (нужен backward-compat)

Принято в 06-ir-bytecode.

---

## Итог

Детальные ADR (отдельные файлы):
- **ADR-002** — Surface Syntax v1: Corrections (C1–C8) → `ADR-002-surface-syntax-corrections.md`
- **ADR-009** — Record Types for Plugin Results (C9) → `ADR-009-plugin-record-types.md`

Inline-резюме в этом документе: ADR-001, ADR-003–008, ADR-010–012.

Остались open: root-binding, plugin-stall mitigation, resync-политика stream. Решить до соответствующих milestone.