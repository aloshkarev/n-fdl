# N-FDL IR & Bytecode Design v1

Описывает четыре уровня представления и переход между ними, control-flow модель,
семейства инструкций байткода, модель континуации для `NeedMoreBytes` и
сохранение zero-copy. Реализуется в crates `nfdl-ir` и `nfdl-bytecode`.

Псевдокод здесь — для уточнения структуры, не финальные Rust-типы.

## 1. Четыре уровня

```
Parsed AST   — 1:1 с грамматикой, span-info, без типов
   │  name resolution + type inference (04) + verification (05)
Typed AST     — типизирован, scope разрешён, union/option/record выведены
   │  lowering: топологический порядок DAG -> явный control-flow
Verified IR   — DAG узлов-инструкций + CFG; аннотации BoundsProof/ProgressMode/LayerKind
   │  linearization + slot allocation + jump-table resolution
Bytecode      — линейный поток инструкций + constant pool + tables; ProgramImage
```

Граница «Typed AST → Verified IR» — там, где доказаны инварианты (05 §10).
Граница «IR → Bytecode» — чисто механическая (линеаризация), не меняет семантику.

## 2. Verified IR

### 2.1 Структура

IR — это **CFG из basic blocks**, где каждый блок принадлежит одному
**message-template**. Узлы внутри блока линейны; ветвление — на границах блоков.

```
IrProgram {
    protocols:   Map<ProtoId, ProtoIr>,
    messages:    Map<MsgId, MsgTemplate>,
    plugins:     Map<PluginId, PluginSig>,
    binds:       Vec<BindEdge>,          // bind-граф (циклы допустимы, C7)
    machines:    Map<FsmId, FsmIr>,
    consts:      ConstPool,
}

MsgTemplate {
    id: MsgId,
    entry: BlockId,
    blocks: Map<BlockId, Block>,
    slots: SlotLayout,                   // имя -> SlotId (поля/let/carry)
    min_size: u32,                       // для loop-progress / bind-shrink
}

Block {
    id: BlockId,
    ops: Vec<IrNode>,
    term: Terminator,                    // как блок завершается (см. §3)
}
```

### 2.2 IrNode (операции внутри блока)

```
IrNode =
  | ReadScalar  { ty, endian, dst: SlotId }
  | ReadBits    { bits: u8, dst: SlotId }
  | Align                                          // выравнивание bit-cursor до байта
  | ReadSlice   { len: IrExpr, bounds: BoundsProof, dst: SlotId }
  | ReadRest    { dst: SlotId }                    // bytes[..]
  | ReadEof     { dst: SlotId }                    // bytes[EOF] (stream) -> может YIELD
  | ReadStream  { event: EventId }                 // bytes[stream] -> PAYLOAD_CHUNK, no AST slot
  | LetBind     { expr: IrExpr, dst: SlotId }
  | Validate    { pred: IrExpr, diag: DiagId }     // -> ConstraintError
  | BoundsCheck { len: IrExpr, diag: DiagId }      // вставляется когда BoundsProof=RuntimeCheck
  | Invoke      { plugin: PluginId, args: Vec<IrExpr>, dst: SlotId }
  | CallMsg     { msg: MsgId, dst: SlotId }        // вложенное сообщение (не bind!)
  | EmitField   { name: Sym, src: SlotId }         // строит выходной AST
  | EmitMsgBegin{ msg: MsgId } | EmitMsgEnd
```

`IrExpr` — дерево чистых выражений (арифметика/сравнения/логика/слот-ref/builtin/
invoke-pure/record-field-access), вычисляется на operand-стеке.

### 2.3 Аннотации (из фазы 05)

```
BoundsProof  = Proven | RuntimeCheck(reason)     // Proven -> нет BOUNDS_CHECK в bytecode
ProgressMode = ProgressProven | RuntimeGuard     // RuntimeGuard -> consumed-check на back-edge
LayerKind    = Flat | Recursive   // Recursive: runtime enforces max_layer_depth + shrink (C7)
```

## 3. Control-flow модель

`Terminator` завершает блок. CFG **reducible by construction** (DSL не имеет
произвольных goto) → тривиальная верификация и JIT-дружелюбность (v2).

```
Terminator =
  | Goto    { target: BlockId }
  | Branch  { disc: IrExpr, table: Vec<(CaseVal, BlockId)>, default: BlockId }  // match
  | CondField { cond: IrExpr, present: BlockId, absent: BlockId }              // field: T if c
  | LoopHeader { cond: IrExpr, body: BlockId, exit: BlockId,
                 carries: Vec<(SlotId, /*init done in pred*/)>, progress: ProgressMode }
  | LoopLatch  { updates: Vec<(SlotId, IrExpr)>, header: BlockId }   // next c = e; back-edge
  | Return                                                          // конец message
```

### 3.1 Loop-форма (C2 carry/next)

```
   pred:    eval carry_init -> carry slots ; Goto header
   header:  LoopHeader{ cond, body, exit }       // eval cond; false -> exit
   body:    <statements> ; Goto latch
   latch:   LoopLatch{ updates=[next c=e], header } // прогресс-check здесь
            -> если progress=RuntimeGuard: проверить consumed>=1, иначе abort
   exit:    list собран в slot ; продолжение
```

`carry` живёт в выделенных слотах header-фрейма; иммутабельен в body,
обновляется только в latch — точное отражение fold-семантики (03 §3.5).

### 3.2 match-форма

`Branch` строит jump-table по дискриминатору (плотный switch если значения
компактны, иначе бинарный поиск). Каждая ветка — блок, добавляющий поля в
union-слот под тегом; сходятся в join-блок (поля после match).

## 4. Bytecode

### 4.1 ProgramImage (сериализуемый артефакт, ADR-012)

```
ProgramImage {
    magic: b"NFDL", version: u16,
    const_pool: Bytes,                  // строки, большие константы
    plugin_table: Vec<PluginRef>,       // имя+сигнатура для late-binding к registry
    msg_table: Vec<CompiledMsg>,        // байткод каждого message-template
    bind_table: Vec<CompiledBind>,
    fsm_table: Vec<CompiledFsm>,
    limits: ResourceLimits,             // запечённые лимиты (могут override-ться)
}
CompiledMsg { entry_pc, code: Bytes, slot_count, min_size }
```

Иммутабельный, версионированный, разделяется `Arc`. Не содержит указателей в
ядро → полная изоляция спеки.

### 4.2 Исполнительная модель

- **Slot file** (на фрейм): регистро-подобное хранилище значений полей/let/carry.
  Индексируется `SlotId` (u16). Слот хранит `Value` (см. `07-runtime`).
- **Operand stack** (на фрейм): для вычисления `IrExpr`. Глубина ограничена
  статически (вычислена при компиляции) → стек не растёт неограниченно.
- **Frame stack**: вложенные `CallMsg` + loop-фреймы. Глубина ≤ статической
  оценки + `max_layer_depth` для bind.
- **Cursor**: `byte_offset` + `bit_offset` в текущем срезе.

### 4.3 Семейства инструкций

```
READ:
  READ_U8  READ_U16 READ_U24 READ_U32 READ_U48 READ_U64    // endian запечён в opcode-variant
  READ_BITS imm:u8                                         // 1..=64
  ALIGN                                                    // bit-cursor -> byte boundary
  READ_SLICE  src_len:reg   -> dst:slot                    // zero-copy, см. §6
  READ_REST   -> dst:slot
  READ_EOF    -> dst:slot                                  // может эмитить YIELD
  READ_STREAM_CHUNK event_id                                // chunked bytes[stream]

EXPR (operand stack):
  PUSH_CONST  idx | PUSH_IMM imm | PUSH_SLOT slot | PUSH_BUILTIN id
  PUSH_FIELD_OF slot, field_idx                            // record .field access (C9)
  PUSH_SESSION idx                                          // read-only session projection
  ADD SUB MUL DIV MOD  SHL SHR  BAND BOR BXOR  BNOT NEG     // checked арифметика
  AND OR NOT
  CMP_EQ CMP_NE CMP_LT CMP_LE CMP_GT CMP_GE
  COALESCE                                                  // Option -> value | default

CONTROL:
  JMP pc | JMP_IF pc | JMP_IFNOT pc
  BRANCH_TABLE disc:reg, table_idx                          // match
  LOOP_ENTER  progress_slot                                 // фиксирует offset-снимок
  LOOP_BACK   header_pc [, CONSUMED_CHECK]                  // back-edge + опц. progress-guard
  CALL_MSG msg_id -> dst:slot
  RET

CHECK (-> recoverable errors, не паника):
  VALIDATE pred:reg, diag_id        // false -> ConstraintError
  BOUNDS_CHECK len:reg, diag_id     // len<0 || len>__rem -> ConstraintError

FFI:
  INVOKE plugin_id, argc -> dst:slot   // args с operand-стека

EMIT (строит выходной AST):
  EMIT_MSG_BEGIN msg_id | EMIT_MSG_END
  EMIT_FIELD name_idx, src:slot

YIELD (stream resume point):
  YIELD_NEED_BYTES hint:u32         // сохраняет континуацию, возвращает NeedMoreBytes
```

Кодировка: 1-байтовый opcode + varint-операнды. `Proven`-узлы НЕ генерируют
`BOUNDS_CHECK` → zero overhead на доказанных путях (05 §9).

### 4.4 Пример: ARP ArpPacket (фрагмент, иллюстративно)

```
EMIT_MSG_BEGIN ArpPacket
READ_U16 -> s_hw_type        ; EMIT_FIELD hw_type, s_hw_type
READ_U16 -> s_proto_type     ; EMIT_FIELD proto_type, s_proto_type
PUSH_SLOT s_proto_type ; PUSH_IMM 0x0800 ; CMP_EQ ; VALIDATE diag#1
READ_U8  -> s_hw_len         ; PUSH_SLOT s_hw_len ; PUSH_IMM 0 ; CMP_GT ; VALIDATE diag#2
...
PUSH_SLOT s_hw_len ; READ_SLICE -> s_sender_mac      ; EMIT_FIELD sender_mac, s_sender_mac
; (bounds Proven: hw_len:u8 ∈[1,255], проверено против __rem статически где возможно)
...
RET ; EMIT_MSG_END
```

## 5. Континуация для NeedMoreBytes (stream resume)

Критическая часть. Возникает только в `mode=stream`, когда `READ_*`/`BOUNDS_CHECK`
обнаруживает `__rem < required`, или `READ_EOF` ждёт сигнала FIN.

### 5.1 Что сохраняется

```
VmContinuation {
    msg_pc: ProgramCounter,            // где остановились
    frame_stack: Vec<FrameSnapshot>,   // вложенные CALL_MSG + loop фреймы
    operand_stack: Vec<Value>,         // снимок стека выражений
    consumed_absolute: u64,            // логически потреблено из flow до этой точки
    cursor: { byte_offset, bit_offset },// позиция внутри текущего доступного буфера
    pending: PendingRead,              // что именно ждём: N байт / EOF
}
FrameSnapshot {
    msg_id, slots: SlotValues,         // уже прочитанные значения ЭТОГО фрейма
    loop_state: Option<{ carries: SlotValues, count: u64, iter_start_offset: u64 }>,
}
```

`SlotValues` внутри `VmContinuation` не может содержать arena-backed borrow из
per-packet arena. Snapshot хранит только:

- скаляры по значению;
- `Bytes` stable handles (`Shared(Bytes), offset, len`);
- owned/ref-counted агрегаты (`List`/`Record`/`Message`) из stream snapshot store
  или deep-clone на heap при создании continuation.

Иначе per-packet arena reset после `NeedMoreBytes` превратил бы continuation в
висячий снимок. Это обязательный IR→runtime контракт, а не оптимизация.

### 5.2 Инвариант сохранности слотов (zero-copy + resume)

**Проблема:** слот может содержать `bytes`-дескриптор, указывающий в
reassembly-буфер, который при resume может быть перевыделен/сжат.

**Решение:**

- Скалярные слоты сохраняются **по значению** (u64/bool/str-owned) — переживают.
- `bytes`-слоты в stream хранят **stable handle** = `(BufHandle::Shared(Bytes),
  offset, len)`, где `Bytes` — ref-counted (см. `07-runtime` §ownership). Пока
  континуация жива, она удерживает `Bytes`-ссылку → reassembly не освободит этот
  префикс. При resume slices материализуются от актуального flow-буфера, который
  логически непрерывен с сохранённым `consumed_absolute`.
- `consumed_absolute` — единственная «истина» о позиции в потоке; cursor в
  конкретном буфере пересчитывается при resume.

### 5.3 Цикл resume

```
on new_contiguous_bytes(flow):
    cont = flow.continuation.take()
    buf  = flow.reassembly.contiguous_view()        // Bytes, от next-expected-seq
    vm.restore(cont, buf)                            // восстановить frames/stack/cursor
    loop:
        result = vm.run_from(cont.msg_pc)
        match result:
            Complete(msg) -> emit(msg); FSM.feed(msg); continue с след. сообщения
            NeedMoreBytes(new_cont) -> flow.continuation = Some(new_cont); break
            ConstraintError(e) -> mark Malformed; resync-политика (ADR); break
```

Идемпотентность: повторный resume с тем же буфером даёт тот же результат
(детерминизм). Свойство `resume-equivalence` (`12-testing.md` §1.5 property 5):
произвольная сегментация потока ≡ цельный разбор.

### 5.4 Где YIELD вставляется компилятором

- Перед каждым `READ_*` в `mode=stream` — НЕ безусловно, а через проверку
  `__rem >= required`; если хватает — fast path без yield. Компилятор
  оборачивает потенциально-блокирующие чтения в `JMP_IF (__rem<req) -> YIELD`.
- `datagram`-сообщения вообще не содержат YIELD (verifier гарантирует отсутствие
  `bytes[EOF]` и трактует нехватку как `ConstraintError`).

## 6. Сохранение zero-copy в байткоде

- `READ_SLICE`/`READ_REST`/`READ_EOF` пишут в слот **дескриптор**
  `Slice{ buf_handle, offset, len }` — НЕ копируют байты.
- `READ_STREAM_CHUNK` не пишет bytes-значение в слот; он эмитит bounded chunk в
  event bus, обновляет consumed offset и разрешает compaction reassembly-буфера.
- `EMIT_FIELD` для bytes-поля эмитит тот же дескриптор в выходной AST → вывод
  тоже zero-copy (материализация строк — только на сериализации в PDML/JSON
  потребителем).
- Единственные точки копирования (документированы):
  1. `INVOKE`, если плагин возвращает owned `str` (DNS-имя) — аллокация плагина.
  2. FSM `set v = e` для bytes — owned-копия в `C_sess` (переживает пакет).
  3. Пересечение границы reassembly-буфера в stream — через `Bytes` ref-count
     (не копия данных, копия указателя+счётчика).
- `datagram`-путь — строго zero-copy от входа до выходного AST.

## 7. Что НЕ входит в IR/bytecode v1

- JIT-компиляция IR (v2; CFG спроектирован reducible под это).
- Оптимизации уровня IR (constant folding, dead-slot elimination) — v1.5,
  не влияют на семантику.
- Инкрементальная перекомпиляция отдельных сообщений — v2.
- Bytecode verifier отдельным проходом (в v1 доверяем нашему компилятору +
  fuzzing VM на произвольном байткоде как защита; формальный bytecode-verifier — v1.5).

## 8. Контракт IR→Bytecode

1. Линеаризация сохраняет семантику CFG (reducible → корректный порядок блоков).
2. Все slot/jump-индексы разрешены и в границах (проверяется при компиляции).
3. Глубина operand-стека и фреймов статически ограничена и записана в образ.
4. Proven-узлы не порождают runtime-проверок; RuntimeCheck/RuntimeGuard —
   порождают ровно одну инструкцию проверки.
5. ProgramImage детерминированно воспроизводим из одного Typed AST (для
   golden-тестов на байткод).
