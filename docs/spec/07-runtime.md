# N-FDL Runtime Architecture v1 (Rust 2024)

Описывает crate-структуру, ownership/lifetime-модель буферов и срезов,
безопасный дизайн ключевых типов (`Slice`, `Value`, `Frame`, `ParserContext`,
`SessionContext`, `VmState`), стратегию аллокаций (arena/Arc/Bytes/Cow) и
политику `unsafe`. Это фундамент, на который ссылаются `06`, `08`, `09`, `10`.

Псевдокод иллюстрирует структуру и lifetime-связи, не финальные сигнатуры.

## 1. Crate-структура (workspace)

```
nfdl-syntax     лексер, парсер, Parsed AST, span/диагностика-спаны
nfdl-types      Typed AST, типовая система, scope-резолюция, inference
nfdl-verify     DAG, interval-анализ, FSM liveness; feature "z3" (v1.5)
nfdl-ir         Verified IR, lowering, CFG
nfdl-bytecode   компилятор байткода, ProgramImage (serde), дизассемблер
nfdl-vm         resumable VM, Slice, Value, Frame, VmState, VmContinuation
nfdl-runtime    Layer dispatcher, ParserContext, recursion guard, root-binding
nfdl-stream     TCP reassembly, flow-буферы, NeedMoreBytes-оркестрация   (v1.5)
nfdl-fsm        EFSM-движок, SessionContext, Session DB, таймеры
nfdl-plugin     реестр плагинов, FFI ABI, манифест, safe-обёртки  (ЕДИНСТВ. unsafe)
nfdl-diag       типы ошибок, Event bus, телеметрия
nfdl-cli        load/run/inspect, golden/diff harness
nfdl-fuzz       fuzz-таргеты (parser, vm, generative)
```

Зависимости строго однонаправленные: `syntax → types → verify → ir → bytecode →
vm → runtime → {stream, fsm}`. `plugin`, `diag` — листовые (зависят максимум от
`vm`-типов значений). Циклов нет.

Все crate: `#![forbid(unsafe_code)]`, кроме `nfdl-plugin` (см. §7).

> **Implementation status (M0, 2026-06).** Схема выше — целевая архитектура v1.
> Текущая реализация: бо́льшая часть логики (bytecode-компилятор, VM, EFSM,
> sessions, reassembly, runner) живёт в **`nfdl-runtime`**; парсер/AST — в
> **`nfdl-syntax`**; `nfdl-verify` существует обособленно (interval-анализ + Z3
> stub) и **не подключён** к пайплайну. Крейты `nfdl-types`, `nfdl-ir`,
> `nfdl-bytecode`, `nfdl-vm`, `nfdl-diag` — пустые placeholder’ы (`M0
> placeholder`). `nfdl-stream`/`nfdl-fsm`/`nfdl-plugin` как отдельные крейты ещё
> не выделены. Расщепление монолита по целевой схеме — работа milestone’ов
> M1–M6 (`13-roadmap.md`), не часть M0.

## 2. Lifetime-модель буферов: два режима

Центральное решение (ADR-004). Zero-copy срез не может пережить буфер. Два
непересекающихся режима жизни буфера → единый абстрагирующий handle.

### 2.1 BufHandle

```rust
enum BufHandle<'pkt> {
    /// datagram: буфер живёт ровно в течение обработки одного пакета.
    Borrowed(&'pkt [u8]),
    /// stream: буфер переживает отдельные TCP-сегменты (resume).
    Shared(bytes::Bytes),
}
```

- **Datagram** → `Borrowed(&'pkt [u8])`: ноль накладных расходов, никакого
  ref-count. Всё, что от него производно (`Slice`, `Value::Bytes`, выходной AST),
  параметризовано `'pkt` и не может пережить пакет. Компилятор Rust гарантирует
  это статически.
- **Stream** → `Shared(Bytes)`: ref-counted immutable buffer. Дешёвый clone
  (счётчик), под-срезы без копий, переживает сегменты и континуации (06 §5.2).

Один и тот же код VM работает над `BufHandle` обобщённо; режим выбирается
dispatcher'ом на входе по `meta.mode`.

### 2.2 Почему не один режим

- Только `&'pkt` — не работает для stream (срез должен пережить сегмент).
- Только `Bytes` — навязывает ref-count и atomic-счётчики на горячий datagram-
  путь (большинство трафика), теряя главное преимущество zero-copy.
- Hybrid — оптимален для каждого режима ценой одной enum-диспетчеризации
  (предсказуемая ветка, ничтожна на фоне разбора).

## 3. Slice

```rust
struct Slice<'pkt> {
    buf: BufHandle<'pkt>,   // Borrowed или Shared
    offset: usize,          // от начала buf
    len: usize,
}
```

Инварианты (поддерживаются конструктором):
- `offset + len <= buf.len()` — проверяется ОДИН раз при создании (safe
  `slice::get`). Дальнейшие чтения внутри `Slice` уже in-bounds.
- Создание `Slice` — единственная точка bounds-проверки для срезов; bytecode
  `READ_SLICE` идёт через неё → OOB невозможен без unsafe.

Операции: `sub(offset, len) -> Option<Slice>` (под-срез с проверкой),
`as_bytes() -> &[u8]` (для плагинов/сериализации), `len()`, `is_empty()`.
Никакого `unsafe` внутри.

## 4. Value

```rust
enum Value<'pkt> {
    U(u64),                      // все uN расширены до u64
    I(i64),                      // все iN расширены до i64
    Bool(bool),
    Bytes(Slice<'pkt>),          // zero-copy view
    Str(Box<str>),               // owned — только результат invoke (DNS-имя)
    Opaque(OpaqueHandle),        // __root_buffer, plugin state handle
    List(ArenaVec<'pkt, Value<'pkt>>),     // результат loop (arena-backed)
    Option(Option<Box<Value<'pkt>>>),      // условное поле
    Union { tag: u64, inner: Box<Value<'pkt>> },   // match
    Record(ArenaVec<'pkt, (Sym, Value<'pkt>)>),    // invoke record (C9)
    Message(Box<MessageValue<'pkt>>),      // вложенный агрегат
}
```

**Mapping `loop_result` (04 §5.6):** Typed AST хранит
`loop_result{ items: list[τ], carries: record{...} }`. Runtime представление —
`Value::List` для `items` плюс carry-слоты во frame state (`carries: HashMap<Sym, Value>`).
Human-readable вывод может сворачивать в `name.items`; verifier использует полную форму.

```rust
// conceptual — not a separate Value variant in v1
struct LoopResult<'pkt> {
    items: ArenaVec<'pkt, Value<'pkt>>,
    carries: HashMap<Sym, Value<'pkt>>,
}
```

- Скаляры — по значению (u64/i64/bool); arith-checked на уровне VM.
- `Bytes` несёт `'pkt` → не переживёт буфер (для stream `'pkt` фактически
  привязан к жизни `Bytes` через `Shared`-вариант — handle живёт пока жив clone).
- `Str` — единственный owned-аллоцированный вариант значения от парсинга
  (плагин аллоцировал, ядро владеет и освобождает).
- Агрегаты (`List`/`Record`) — arena-backed только на datagram/synchronous пути.
  В `mode=stream` любое агрегатное значение, попадающее в `VmContinuation`,
  переносится в flow-owned snapshot store или deep-clone'ится в owned heap перед
  возвратом `NeedMoreBytes` (06 §5.1). Per-packet arena никогда не переживает
  пакет.

## 5. Frame, ParserContext, VmState, SessionContext

### 5.1 Frame (на одно message-template / loop-итерацию)

```rust
struct Frame<'pkt, 'arena> {
    msg_id: MsgId,
    slots: &'arena mut [Value<'pkt>],  // slot file, размер из ProgramImage
    bit_cursor: BitCursor,             // под-байтовая позиция для bitfield
    loop_state: Option<LoopState<'pkt>>,
}
struct LoopState<'pkt> {
    carries: SmallVec<[Value<'pkt>; 4]>,
    count: u64,
    iter_start_offset: usize,          // для consumed-check (loop progress)
}
```

### 5.2 ParserContext (на разбор одного пакета/стека слоёв)

```rust
struct ParserContext<'pkt> {
    root: BufHandle<'pkt>,             // __root_buffer
    layer_stack: Vec<LayerFrame<'pkt>>,// C_parent: для IPv4.src + recursion depth (C3/C7)
    cur_offset: usize,                 // __current_offset (локальный, в текущем срезе)
    root_offset: usize,                // __root_offset (абсолютный)
    depth: u16,                        // текущая глубина инкапсуляции (C7)
    limits: &'pkt ResourceLimits,
}
struct LayerFrame<'pkt> {
    proto: ProtoId,
    fields: Box<MessageValue<'pkt>>,   // разобранные поля родителя (read-only для child)
}
```

### 5.3 VmState (исполнение байткода)

```rust
struct VmState<'pkt, 'arena> {
    program: Arc<ProgramImage>,        // иммутабельный, разделяемый
    ip: ProgramCounter,
    frames: Vec<Frame<'pkt, 'arena>>,  // стек фреймов (CALL_MSG + loop)
    operand_stack: Vec<Value<'pkt>>,   // глубина ограничена статически
    cursor: Cursor,                    // byte+bit offset в текущем срезе
    arena: &'arena Arena,              // datagram per-packet или stream scratch arena
    ctx: &'arena mut ParserContext<'pkt>,
}
```

`'pkt`: жизнь входного буфера. `'arena`: жизнь текущей scratch-арены. В datagram
это per-packet arena; в stream это arena текущего resume-вызова. Значения,
сохраняемые в `FlowState.l7_cont`, обязаны быть detached от scratch arena
(owned/ref-counted), иначе Rust lifetime не позволит сохранить continuation.
`program: Arc` — переживает оба, разделяется между пакетами/потоками.

### 5.4 SessionContext (C_sess — переживает пакеты, НЕ zero-copy)

```rust
struct SessionContext {
    state: StateId,
    vars: HashMap<Sym, OwnedValue>,    // 'static — owned, без 'pkt!
    timers: TimerSet,                  // v1.5
    last_seen: Instant,
}
enum OwnedValue { U(u64), I(i64), Bool(bool), Bytes(Box<[u8]>), Str(Box<str>) }
```

**Критично:** `C_sess` не может содержать `'pkt`-срезы — он переживает пакет.
`set req_auth = authenticator` (RADIUS) **копирует** 16 байт в `Box<[u8]>`. Это
единственная точка копирования в FSM, документирована (06 §6). Тип `OwnedValue`
структурно запрещает zero-copy ссылки в сессии → висячих ссылок нет by design.

## 6. Стратегия аллокаций

| Механизм | Где | Зачем |
|---|---|---|
| **Arena** (`bumpalo`) | datagram per-packet; stream per-resume scratch для `Frame.slots`, временных `Value::List/Record`, узлов выходного AST | убирает per-field malloc; сброс O(1) после пакета/resume |
| **Flow snapshot store** | stream `VmContinuation` агрегаты и slot snapshots | owned/ref-counted значения, переживающие сегменты |
| **`Arc<ProgramImage>`** | байткод | иммутабелен, разделяется между пакетами/потоками, read-only |
| **`bytes::Bytes`** | stream-буферы и stream-срезы | ref-count переживает сегменты/континуации |
| **`Cow<'static, str>`** | диагностика, имена | статические строки спеки vs форматированные сообщения |
| **`Box<[u8]>` / `Box<str>`** | `OwnedValue` в `C_sess`, `Str` от плагина | owned-данные, переживающие пакет |
| **`SmallVec`** | operand stack, carries, мелкие slot-векторы | избегаем heap для типичных малых размеров |

Принцип: горячий datagram-путь не делает heap-аллокаций на чтение поля (всё в
арене/слотах/стеке). Аллокации — только на границах (str от плагина,
session-copy, stream ref-count, stream continuation snapshot).

## 7. Политика unsafe

- **`#![forbid(unsafe_code)]`** во ВСЕХ crate, кроме `nfdl-plugin`.
- Все bounds-проверки `Slice` — safe Rust (`get`/`get_mut` → `Option`).
- VM-арифметика — `checked_*` / `wrapping_*` с явной обработкой (06 §4.3).
- **Единственная зона unsafe — `nfdl-plugin`** (C-ABI FFI к нативным плагинам).
  Требования к ней:
  1. Инкапсуляция в safe-обёртки; unsafe не «протекает» наружу.
  2. Отдельный justification-файл (`nfdl-plugin/UNSAFE.md`) на каждый unsafe-блок.
  3. CI: MIRI + ASAN на FFI-границе; fuzzing с санитайзерами.
  4. Контракт: плагин получает read-only view, не сохраняет указатели за
     пределы вызова, не пишет в буфер (см. `10-plugin-abi`).
- Цель-метрика: `cargo geiger` показывает unsafe только в `nfdl-plugin`.

## 8. Поток данных (datagram, один пакет) — связывание lifetimes

```
1. dispatcher получает &'pkt [u8]; arena = Arena::new(); ctx = ParserContext{ root: Borrowed(buf), ... }
2. root parser: VmState{ program: Arc.clone, frames: [Frame in arena], ctx: &mut ctx }
3. READ_* пишут Value<'pkt> в slots (срезы — view в buf, скаляры — по значению)
4. bind: dispatcher создаёт sub-Slice payload, push layer_stack, depth+=1,
   проверяет max_layer_depth + payload-shrink (C7); рекурсивно VM.run(inner)
5. EMIT_* строят MessageValue<'pkt> в арене -> event bus (потребитель сериализует/копирует синхронно)
6. FSM.feed(msg, &mut session_db): key extraction (копии скаляров), δ, set (owned-copy), emit
7. конец пакета: arena.reset(); buf можно освободить (ничто 'pkt не пережило)
```

Stream-вариант отличается шагами 1 (`Shared(Bytes)`) и наличием
YIELD/континуации (06 §5, детали в `08-stream-reassembly`). При YIELD VM сначала
детачит все slot/operand/frame значения, которые попадут в continuation, от
scratch arena; после этого arena может быть безопасно сброшена.

## 9. Детерминизм и concurrency (v1)

- **Single-threaded event loop** (ADR-005): один пакет за раз, FSM/таймеры тикают
  в том же loop. Нет гонок в session DB, тотальный детерминированный порядок.
- `Arc<ProgramImage>` — единственное разделяемое состояние; read-only, потому
  thread-safe тривиально (готово к sharded v2 без изменения типов значений).
- Per-packet арена — thread-local в v2 sharded-модели (share-nothing).

## 10. Контракт runtime (инварианты)

1. Никакой `Value::Bytes`/`Slice` не переживает свой буфер (гарантия Rust
   lifetime + `Shared`-ref-count для stream).
2. Никакой arena-backed `List`/`Record`/`Message` не сохраняется в
   `VmContinuation`; stream snapshots owned/ref-counted.
3. `C_sess` структурно не содержит `'pkt`-ссылок (`OwnedValue`).
4. Все доступы к буферу проходят через `Slice`-конструктор → bounds-safe.
5. Горячий datagram-путь — без heap-аллокаций на чтение поля.
6. unsafe локализован в `nfdl-plugin`, покрыт MIRI/ASAN.
7. `max_layer_depth` + payload-shrink enforced в dispatcher до рекурсивного bind.
