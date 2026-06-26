# N-FDL Stream & Reassembly Subsystem v1.5

Описывает datagram- и stream-пайплайны, TCP reassembly, разделение L4/L7,
семантику `NeedMoreBytes`/resume и управление памятью flow-буферов. Реализуется
в crate `nfdl-stream`. Это самый рискованный компонент проекта — детали ниже
намеренно подробны.

Зависит от: `06 §5` (континуация), `07 §2` (BufHandle::Shared/Bytes), `09` (FSM
управляет жизненным циклом flow).

## 1. Datagram pipeline (baseline, v1)

```
raw &'pkt [u8]
  -> Dispatcher: root = root-protocol (по link-type, см. §7); ctx.root = Borrowed
  -> VM.run(program[root], Slice(whole), ctx)              // синхронно, без YIELD
  -> bind: sub-Slice payload; layer_stack push; depth+1; VM.run(inner)
  -> Complete(msg): emit -> event bus; FSM.feed(msg)
  -> ConstraintError: mark Malformed; прервать ветку; diagnostic; arena сброс
```

В datagram нет reassembly, нет YIELD, нет континуаций. `bytes[EOF]` запрещён
верификатором (05). Нехватка байт = `ConstraintError::Truncated`.

## 2. Зачем разделять L4 и L7 (архитектурная необходимость)

TCP-сегмент **сам по себе** — datagram: его заголовок fixed-layout, парсится
синхронно как обычное сообщение. Stream-семантика относится ТОЛЬКО к payload
TCP, который образует непрерывный байтовый поток L7.

```
            ┌─────────────── L4: TCP segment (datagram-parse) ───────────────┐
segment ──► parse Segment header ──► FSM Connection transition ──► payload+seq ─┐
            └───────────────────────────────────────────────────────────────┘ │
                                                                                ▼
            ┌─────────────── L7: reassembled byte stream ────────────────────────┐
            │  per-flow Reassembler: order by seq, dedup, handle overlap          │
            │  contiguous Bytes from next-expected-seq ──► resume L7 VM            │
            └────────────────────────────────────────────────────────────────────┘
```

Следствие: TCP-протокол в DSL имеет **две роли** — `message Segment`
(datagram-разбор заголовка) и `mode=stream` payload (`bytes[EOF]`), которые
обрабатываются разными путями. Это отражено в `tcp.nfdl` комментарием.

## 3. Flow и его состояние

Flow = однонаправленный байтовый поток одного TCP-соединения (две стороны = два
flow, либо один bidir-flow с двумя half-streams — ADR ниже).

```
FlowState {
    key: FlowKey,                       // bidir 4-tuple (09 §key)
    dir: Direction,                     // c2s / s2c (два half-stream)
    next_seq: u32,                      // следующий ожидаемый seq (per half)
    reasm: Reassembler,                 // буфер + OOO-сегменты
    l7_cont: Option<VmContinuation>,    // замороженный L7-парсер (06 §5)
    l7_consumed: u64,                   // logically consumed by L7 (= cont.consumed_absolute)
    limits: FlowLimits,
}
```

## 4. Reassembler

### 4.1 Структура

```
Reassembler {
    base_seq: u32,                      // seq, соответствующий началу contiguous-буфера
    contiguous: BytesMut,               // упорядоченный непрерывный префикс
    ooo: BTreeMap<u32 /*seq*/, Bytes>,  // out-of-order сегменты, ждущие склейки
    ooo_bytes: usize,                   // суммарный размер OOO (для лимита)
}
```

### 4.2 Приём сегмента

```
fn accept(seg_seq, seg_data):
    1. relseq = wrapping_sub(seg_seq, base_seq)        // seq wraps mod 2^32
    2. если segment целиком левее next (relseq+len <= consumed): дубликат -> drop + anomaly
    3. overlap-resolution (см. 4.3) против contiguous и соседних ooo
    4. если seg_seq == next_seq: append в contiguous; next_seq += len;
         затем «подтянуть» из ooo все сегменты, ставшие contiguous
       иначе: вставить в ooo[seg_seq] (с учётом overlap); ooo_bytes += len
    5. enforce лимиты (§6); при превышении -> drop flow + diagnostic
    6. если contiguous вырос -> notify L7 resume (§5)
```

### 4.3 Overlap policy (ADR-011)

Перекрывающиеся сегменты (Teardrop-класс, разное поведение ОС) — источник
ambiguity и атак. v1.5 политика: **first-wins** (как Zeek по умолчанию) — байты,
уже принятые в contiguous, не перезаписываются; новые перекрывающие байты
игнорируются. При обнаружении overlap эмитится `anomaly: TcpOverlap` событие
(телеметрия), но не ошибка. Политика конфигурируема (`first-wins | last-wins`)
для differential-сверки с TShark. **IP-фрагментация overlap — вне scope v1.5**
(v2, отдельный L3-движок).

### 4.4 Seq wraparound

Все сравнения seq — модулярные (`wrapping_sub` + signed-сравнение разности), т.к.
32-битный seq оборачивается. Никаких абсолютных `<`/`>` на сырых seq.

## 5. NeedMoreBytes / resume (центральный механизм)

### 5.1 Когда возникает YIELD

В `mode=stream`, при исполнении байткода:

- `READ_*`/`BOUNDS_CHECK` требует `n` байт, а `__rem < n` (06 §5.4: обёрнуто в
  `JMP_IF __rem<req -> YIELD_NEED_BYTES`).
- `READ_EOF` (`bytes[EOF]`): потребляет всё доступное, затем YIELD до сигнала
  EOF (FIN/close/plugin-pattern по `meta.eof`).
- `READ_STREAM_CHUNK` (`bytes[stream]`): эмитит bounded `PAYLOAD_CHUNK` при
  появлении contiguous-байт и не ждёт FIN для построения единого AST-значения.

Если FIN/close наступил, а suspended read всё ещё ждёт объявленную длину
(`READ_*`/`BOUNDS_CHECK`) или delimiter plugin не нашёл границу, runtime
возвращает `ConstraintError::Truncated`, а не бесконечный `NeedMoreBytes`.

### 5.2 Что замораживается

`VmContinuation` (06 §5.1) сохраняется в `FlowState.l7_cont`. Инвариант
сохранности слотов (06 §5.2): bytes-слоты держат `Shared(Bytes)`-ссылки →
удерживают соответствующий префикс reassembly-буфера живым (ref-count не даёт
сжать/освободить занятые байты).
Агрегаты в continuation detached от stream scratch arena: либо owned/ref-counted
snapshot, либо deep clone на heap. Ссылка на per-packet/per-resume arena в
`FlowState` запрещена типами runtime.

### 5.3 Resume-цикл

```
fn on_contiguous_growth(flow):
    let Some(cont) = flow.l7_cont.take() else:
        # L7 ещё не стартовал -> запустить с начала доступного буфера
        cont = VmContinuation::fresh(l7_program, consumed=0)
    let view: Bytes = flow.reasm.contiguous_view_from(flow.l7_consumed)
    loop:
        match VM.resume(cont, view, ctx):
            Complete(msg, new_consumed):
                emit(msg); FSM.feed(msg, flow.key)
                flow.l7_consumed = new_consumed
                reasm.advance(new_consumed)        # можно compact-нуть префикс (§6.2)
                cont = VmContinuation::fresh(l7_program, consumed=new_consumed)
                # продолжить парсить следующее L7-сообщение, если есть байты
                if reasm.available_from(new_consumed) == 0: break
            NeedMoreBytes(new_cont):
                flow.l7_cont = Some(new_cont); break   # ждём ещё сегментов
            ConstraintError(e):
                mark Malformed; resync(flow);          # политика ресинхронизации (§5.5)
                break
```

### 5.4 Resume-equivalence (свойство корректности)

Для любого разбиения потока на сегменты результат L7-разбора идентичен разбору
цельного потока. Это property-тест (13 §7): фаззер бьёт поток на случайные
сегменты (включая OOO, дубли) → AST совпадает с baseline. Идемпотентность
resume гарантирует это.

### 5.5 Resync после ConstraintError в stream

Если L7-сообщение malformed в середине потока, простой drop теряет
синхронизацию. Политики (конфиг, ADR-кандидат):
- `drop-flow` (v1.5 default): пометить flow Malformed, прекратить L7-разбор,
  продолжать только FSM по L4-сегментам. Безопасно, теряет L7.
- `resync-by-plugin` (v2): использовать pattern-плагин (`scan_crlf`-класс) для
  поиска следующей валидной границы сообщения.

## 6. Управление памятью flow-буферов (DoS-критично)

### 6.1 Лимиты (config, enforced в accept)

```
FlowLimits {
    max_reassembly_buffer: usize,    // contiguous буфер на half-flow
    max_out_of_order_bytes: usize,   // суммарно в ooo
    max_out_of_order_segments: usize,
    max_pending_message_size: usize, // незавершённое L7-сообщение (ждёт NeedMoreBytes)
    flow_idle_timeout: Duration,
}
```

Превышение любого → `drop flow` + `Malformed::ReassemblyLimit` diagnostic.
Незавершённое сообщение, растущее без границы (never-completing) ловится
`max_pending_message_size`: если L7 потребил 0 и буфер достиг лимита — abort flow.

### 6.2 Compaction

После `Complete(msg)` префикс до `l7_consumed` больше не нужен L7. Compaction
сдвигает `contiguous`/`base_seq` вперёд, освобождая память — **но только если
нет живых `Bytes`-ссылок** на этот префикс. Если эмитированный AST ещё держит
срез (до flush в потребитель), ref-count удерживает память до его освобождения.
Это безопасно by design (07 §2): compaction физически не может освободить
читаемые данные.

### 6.3 Global pressure

Глобальный `max_total_reassembly_memory` поверх per-flow. При достижении —
LRU-эвикция наименее активных flow (drop + diagnostic). Backpressure
сигнализируется dispatcher'у.

### 6.4 Chunked emissions for large bodies

`bytes[EOF]` годится только для bounded сообщений, где удержание тела до EOF
приемлемо. Для HTTP file transfer, SMB payload и похожих больших тел используется
`bytes[stream]`:

```
payload: bytes[stream];
```

Runtime выбирает chunk size из `FlowLimits` и, когда есть contiguous-байты,
эмитит:

```
PAYLOAD_CHUNK { flow_key, dir, offset, bytes }
```

После подтверждения доставки события prefix может быть compacted без ожидания
FIN. `bytes[stream]` должно быть последним полем сообщения, не создаёт `bytes`
в AST и не может использоваться в выражениях.

## 7. Root protocol binding (Open Question, до M0)

Dispatcher выбирает корневой парсер по link-type источника:
- PCAP: link-type из глобального заголовка (Ethernet=1, raw IP=101, ...).
- Live: из конфигурации интерфейса.
- Явный override в запуске CLI (`--root Ethernet`).

Маппинг link-type → root-protocol — конфигурация runtime, НЕ часть DSL (спека
объявляет `bind`, но точка входа задаётся снаружи). Решение фиксируется ADR до
M0 dispatcher.

### 7.1 L3 fragmentation contract

N-FDL stream subsystem занимается L4/L7 reassembly, прежде всего TCP byte stream.
IPv4/IPv6 фрагментация находится ниже DSL-parser boundary. Dispatcher обязан
либо:

- передать в N-FDL уже дефрагментированный IP-пакет;
- либо пометить пакет как `Malformed::IpFragmentationUnsupported`/diagnostic и
  не запускать L4/L7 parser на неполном фрагменте.

Overlap policy для IP-фрагментов не наследует TCP overlap policy из §4.3; это
отдельный L3 engine/v2 scope.

## 8. Concurrency (v1.5)

Single-threaded event loop (07 §9): сегменты обрабатываются в порядке прихода;
reassembly/resume/FSM — в том же loop. Детерминированный порядок событий.
v2 sharded: flow партиционируется по `FlowKey`-hash, каждый shard владеет своими
flow share-nothing (09 §concurrency).

## 9. Контракт подсистемы

1. L7-парсер видит непрерывный in-order поток; НЕ видит границ сегментов, seq,
   ретрансмиссий, дублей (4.2 их поглощает).
2. `__rem` в stream = доступные contiguous-байты от текущего L7-offset; это не
   граница сообщения и не допустимое условие завершения stream-loop.
3. Resume идемпотентен и эквивалентен цельному разбору (5.4).
4. Memory bounded по всем осям (6); ни один вход не вызывает unbounded рост.
   Большие тела должны использовать `bytes[stream]`, а не unbounded `bytes[EOF]`.
5. Compaction не освобождает данные, на которые есть живые ссылки (6.2).
6. Overlap/дубли → anomaly-события, не паника, не OOB.
