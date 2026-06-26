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

## ADR-002 — `let` mutation inside loop (C2)

**Options**
- Shadowing `let curr_type = ...` внутри итерации
- Assignment (`curr_type = ...`)
- Dedicated `carry` / `next` (loop-carried accumulator)

**Recommendation**: Dedicated `carry` / `next`.

**Trade-offs**
- Новый синтаксис (нужен в грамматике)
- Сохраняет DAG-чистоту и завершаемость (внутри итерации иммутабельно)
- Явно выражает fold-семантику связных списков (GTP-U)
- Shadowing ломает видимость между итерациями; assignment вводит effect в pure-выражение

Принято в ADR-002.

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

## ADR-009 — `bytes[EOF]` semantics split (C1)

**Options**
- Единый `bytes[EOF]` (datagram = rest, stream = end)
- Split: `bytes[..]` (rest-of-slice) + `bytes[EOF]` (stream-end)

**Recommendation**: Split + `meta eof = on_fin | on_close | by_plugin(...)`.

**Trade-offs**
- Два конструкта вместо одного
- Устраняет неоднозначность datagram/stream
- `bytes[EOF]` только в stream, только последнее поле

Принято в ADR-002.

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

Приняты: ADR-002, ADR-009 (детальные), ADR-001,003–008,010–012 (этот документ).

Остались open: root-binding, plugin-stall mitigation, resync-политика stream. Решить до соответствующих milestone.