# N-FDL FFI Plugin System v1 / v1.5

Описывает ABI/API плагинов, разделение pure-stateless / stateful, типизацию
сигнатур, sandbox/изоляцию, передачу буферов без копий и два эталонных плагина
(`dns_decompress`, `scan_crlf`). Реализуется в crate `nfdl-plugin` — **единственная
зона `unsafe`** в проекте (07 §7).

Плагины выполняют то, что декларативный DSL не должен: контекстно-зависимое
сжатие (DNS pointer), криптографию (CRC, AES-GCM), pattern-scan, stateful
декодеры (HPACK v1.5). Спека вызывает их через `invoke`; ядро их не знает по
имени до загрузки манифеста.

## 1. Принципы

1. **Read-only доступ к буферу.** Плагин получает указатель + длину + offset в
   root-буфер; писать не может.
2. **Без сохранения указателей.** Плагин не имеет права удерживать указатель на
   буфер за пределами вызова (буфер живёт `'pkt`).
3. **Owned-результаты освобождает ядро** через объявленный в манифесте free-cb —
   чёткий ownership-контракт через FFI-границу.
4. **Сигнатура статически типизирована** (04 §5.7, 05 §6): arity/типы/purity
   проверяются при компиляции DSL.
5. **Детерминизм для pure-плагинов** — обязателен (кэшируемость, fuzzing,
   resume-equivalence). Один вход → один выход.

## 2. Манифест плагина

Объявляется при регистрации в реестре; компилятор DSL читает его для typecheck.

```
PluginManifest {
    abi_version: u32,                  // совместимость ABI; mismatch -> отказ загрузки
    name: &str,                        // "dns_decompress", "scan_crlf", "crc32", ...
    purity: Purity,                    // PureStateless | Stateful
    args:  Vec<AbiType>,               // типы аргументов (для проверки invoke)
    ret:   AbiType,                    // тип результата (скаляр | str | record{...} | opaque)
    free:  Option<FreeFn>,             // освобождение owned-результата (если ret owned)
    flags: PluginFlags,                // напр. MAY_READ_ROOT, NEEDS_ROOT_OFFSET
}
enum Purity { PureStateless, Stateful }
enum AbiType { U8..U64, I8..I64, Bool, Str, Bytes, Opaque, Record(Vec<(Sym,AbiType)>) }
```

`AbiType::Record` поддерживает C9 (составной результат `dns_decompress`).

## 3. C-ABI v1 (стабильная граница)

Все функции — `extern "C"`, версионированы через `abi_version`.

### 3.1 Передача буфера и значений (zero-copy ввод)

```c
// неизменяемый view в root-буфер; НЕ владеется плагином
typedef struct {
    const uint8_t* ptr;     // __root_buffer
    size_t         len;
    size_t         offset;  // __root_offset (абсолютный, C8)
} nfdl_buf_view;

// тегированное значение через границу (вход-аргументы и выход)
typedef struct {
    uint8_t  tag;           // NFDL_U64 | NFDL_I64 | NFDL_BOOL | NFDL_STR | NFDL_BYTES | NFDL_OPAQUE | NFDL_REC
    union {
        uint64_t        u;
        int64_t         i;
        bool            b;
        nfdl_owned_str  s;      // { ptr, len }  (owned by plugin, freed via free-cb)
        nfdl_buf_view   bytes;  // view (НЕ owned)
        void*           opaque; // handle (напр. stateful state)
        nfdl_record*    rec;    // массив полей (owned)
    } v;
} nfdl_value;

typedef enum {
    NFDL_OK = 0,
    NFDL_ERR_MALFORMED = 1,     // вход некорректен -> ConstraintError/Malformed
    NFDL_ERR_LIMIT     = 2,     // превышен внутренний лимит (напр. max pointer jumps)
    NFDL_ERR_INTERNAL  = 3,     // баг плагина -> PluginError
    NFDL_NEED_MORE     = 4      // только stateful/stream: нужно больше данных
} nfdl_status;
```

### 3.2 Pure stateless вызов

```c
nfdl_status nfdl_invoke(
    nfdl_buf_view        root,        // буфер + абсолютный offset
    const nfdl_value*    args,
    size_t               argc,
    nfdl_value*          out          // заполняется плагином
);
```

### 3.3 Stateful жизненный цикл (v1.5)

```c
nfdl_status nfdl_open (const nfdl_open_params*, void** out_state);   // на старте flow/session
nfdl_status nfdl_feed (void* state, nfdl_buf_view chunk,
                       const nfdl_value* args, size_t argc, nfdl_value* out);
void        nfdl_close(void* state);                                 // на завершении flow
```

Handle `void* state` хранится в `SessionContext`/`FlowState` ядром; передаётся
как `Opaque`-аргумент в `invoke`. Жизненный цикл привязан к session/flow.

### 3.4 Освобождение owned-результатов

```c
void nfdl_free(nfdl_value* v);   // ядро вызывает после копирования/использования str/rec
```

Ядро: получил `out` → использовал/скопировал в арену → вызвал `free`. Owned `str`
от плагина — единственная аллокация на pure-пути (07 §6).

## 4. Pure-stateless vs Stateful

| Аспект | PureStateless (v1) | Stateful (v1.5) |
|---|---|---|
| Примеры | CRC32, AES-GCM, dns_decompress, scan_crlf | HPACK/QPACK, Zlib-inflate-stream |
| Состояние между вызовами | нет | per-session/flow handle |
| Детерминизм | обязателен | в пределах своей истории |
| Позиция в DSL | pure-expr (кроме validate) | выделенная не-pure позиция |
| Кэшируемость | да | нет |
| Жизненный цикл | нет | open/feed/close, привязан к flow |
| Fuzzing | прямой (вход→выход) | stateful-fuzzing с историей |

## 5. Типизация сигнатур (связь с 04/05)

```
invoke("p", a1..an):
    manifest(p) существует              else VerificationError::UnknownPlugin (05 §6)
    n == |manifest.args|                else ::PluginArity
    type(ai) <: manifest.args[i]        else ::PluginArgType
    purity допустима в позиции          else ::PluginPurityViolation
    тип результата = manifest.ret       (record-доступ типизируется по ret, C9)
```

`dns_decompress` манифест:
```
{ name:"dns_decompress", purity:PureStateless,
  args:[Opaque /*root*/, U64 /*root_offset*/],
  ret: Record[("name",Str),("wire_len",U16)],
  flags: MAY_READ_ROOT | NEEDS_ROOT_OFFSET }
```

## 6. Sandbox / изоляция

### 6.1 v1: trusted in-process + жёсткий контракт

Плагины — нативный код в адресном пространстве ядра (производительность). Контракт
(документирован, проверяется инструментами, НЕ языком):
- read-only буфер; запись = UB-нарушение контракта → ловится ASAN в CI.
- запрет сохранять указатели за пределы вызова.
- детерминизм для pure.
- внутренние лимиты (max pointer jumps и т.п.) — ответственность плагина.

CI-страховка: MIRI + ASAN на FFI-границе; fuzzing плагинов с санитайзерами
(13 §8). `cargo geiger` подтверждает локализацию unsafe.

### 6.2 Resource guards

- **Time budget**: ядро ставит deadline на вызов. В single-thread прервать
  зависший C-код нельзя кооперативно без поддержки плагина → варианты (ADR-010):
  - pure-плагины обязаны иметь внутренние лимиты (jumps/iterations) и не блокировать;
  - тяжёлые/недоверенные плагины (v1.5) выносятся в worker-поток с hard-timeout.
- **Output-size cap**: для decompression (HPACK/Zlib) — лимит размера выхода +
  decompression-ratio cap (защита от compression-bomb, 12 §плана).

### 6.3 v2: WASM-изоляция

Опционально — плагины как WASM-модули (полный sandbox: нет произвольного доступа
к памяти ядра, прерываемость, детерминизм по построению). Цена —
производительность. Тот же манифест/ABI поверх WASM-импортов.

## 7. Эталонный плагин: dns_decompress

Решает Known Gap «DNS pointer compression» — абсолютные смещения ломают
потоковый разбор.

```
вход:  root (буфер DNS-сообщения), root_offset (начало имени)
выход: record{ name: str, wire_len: u16 }
алгоритм:
    cur = root_offset; jumps = 0; visited = {}; name_parts = []
    wire_len считается ТОЛЬКО до первого pointer-прыжка (on-wire длина в исходной позиции)
    loop:
        bounds-check cur < root.len                    else NFDL_ERR_MALFORMED
        b = root[cur]
        if b == 0: terminate (root label)
        elif (b & 0xC0) == 0xC0:                        # compression pointer
            jumps += 1
            if jumps > MAX_JUMPS: return NFDL_ERR_LIMIT  # анти-DoS (compression loop)
            target = ((b & 0x3F)<<8 | root[cur+1])
            if target in visited: return NFDL_ERR_LIMIT  # loop detection
            visited.insert(target)
            if wire_len not yet fixed: wire_len = (cur+2) - root_offset
            cur = target                                 # прыжок (НЕ увеличивает wire_len)
        else:                                            # обычный label длины b
            bounds-check cur+1+b <= root.len            else NFDL_ERR_MALFORMED
            name_parts.push(root[cur+1 .. cur+1+b])
            cur += 1 + b
    name = join(name_parts, ".")                         # owned str -> ядро освободит
    if wire_len not set (no pointer): wire_len = cur+1 - root_offset
```

Ключевые защиты: bounds-check каждого доступа, `MAX_JUMPS` + visited-set против
compression-loop DoS. `wire_len` фиксируется на on-wire длине (до первого
прыжка) → DSL потребляет именно столько (`bytes[dec.wire_len]`, C9).

## 8. Эталонный плагин: scan_crlf

Решает Known Gap «EOF/pattern-bound messages» (HTTP/1.x line framing).

**Manifest (M1 reference):**

```toml
name = "scan_crlf"
abi_version = 1
purity = "pure"
args = [
  { name = "root", type = "opaque" },
  { name = "root_offset", type = "u64" },
  { name = "limit", type = "u16" },
]
ret = { kind = "record", fields = [
  { name = "found", type = "bool" },
  { name = "line_len", type = "u16" },
]}
```

```
вход:  root, root_offset, limit (макс. сколько сканировать)
выход: record{ found: bool, line_len: u16 }    # длина до и включая CRLF
алгоритм:
    end = min(root.len, root_offset + limit)
    for i in root_offset .. end-1:
        if root[i]=='\r' && root[i+1]=='\n':
            return { found:true, line_len: (i+2)-root_offset }
    return { found:false, line_len: 0 }          # в stream -> вызвать NeedMoreBytes
```

В `mode=stream`: если `found==false` и достигнут конец доступных байт — DSL
эмитит `NeedMoreBytes` (ждёт продолжения строки в следующем сегменте). Так
pattern-bound framing интегрируется с reassembly (08 §5.1).

## 9. Реестр плагинов

```
PluginRegistry {
    by_name: HashMap<&str, RegisteredPlugin>,
    abi_version: u32,
}
RegisteredPlugin { manifest, vtable: PluginVTable }   // late-binding к ProgramImage.plugin_table
```

Загрузка спеки: каждый `PluginRef` из `ProgramImage` резолвится в реестре по
имени; manifest-сигнатура сверяется с тем, что ожидал компилятор (защита от
version-skew). Отсутствие плагина → отказ загрузки (не runtime-сюрприз).

## 10. Контракт подсистемы

1. Вход — zero-copy view; плагин не пишет и не сохраняет указатели.
2. Owned-выход освобождается ядром через free-cb.
3. Сигнатуры типизированы статически; version-skew ловится при загрузке.
4. Pure-плагины детерминированы и имеют внутренние лимиты (анти-DoS).
5. unsafe локализован здесь; MIRI/ASAN/geiger в CI.
6. dns_decompress/scan_crlf защищены от loop/overflow и интегрированы с C9/stream.
