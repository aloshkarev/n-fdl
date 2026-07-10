# ADR-002 — N-FDL Surface Syntax v1: Corrections (C1–C10)

- **Status:** Accepted (baseline for v1)
- **Date:** 2026-06-25
- **Supersedes:** original syntax sketch in project brief §4 / §6
- **Related:** ADR-006 (match typing, C6), ADR-008 (session keys, C4/C10); C1 (`bytes[EOF]` split) и C2 (`carry`/`next`) покрыты ниже в этом документе

## Контекст

Первоначальный набросок синтаксиса и шесть proof-by-example содержат
конструкции, которые расходятся с заявленной формальной моделью
(монадический завершаемый парсинг + ациклический граф зависимостей полей +
строгое разделение pure / effect). Если закодировать их «как есть», верификатор
окажется внутренне противоречивым. Данный ADR фиксирует исправления C1–C10 как
обязательный базис грамматики v1.

Принцип разрешения конфликтов: **формальная модель — источник истины.
Синтаксис подгоняется под модель, а не наоборот.**

Correction-ID **C9** — plugin record types ([ADR-009](ADR-009-plugin-record-types.md)).
Correction-ID **C10** — atomic endpoint session keys (см. §C10 ниже; детали в
[ADR-list ADR-008](ADR-list-critical-decisions.md)).

---

## C1 — Семантика `bytes[EOF]` неоднозначна между datagram и stream

**Проблема.** В GTP-U (`mode=datagram`) `payload: bytes[EOF]` означает «остаток
текущего среза». В TCP (`mode=stream`) — «до сигнала конца потока (FIN/pattern)».
Один синтаксис → две несовместимые семантики.

**Решение.** Расщепить на два конструкта:

- `bytes[..]` — **rest-of-message**: весь остаток текущего (известного по длине)
  среза. Разрешён в `datagram` и в `stream`, когда длина среза детерминирована.
- `bytes[EOF]` — **stream-end**: потребляет до сигнала EOF от runtime.
  Разрешён **только** в `mode=stream`; обязан быть **последним** полем
  сообщения; протокол обязан объявить источник EOF в `meta` (см. **C1-meta** ниже).
  Нарушение — `VerificationError`.

#### C1-meta — источник EOF в meta

```
meta {
    mode = stream;
    eof  = on_fin;            // on_fin | on_close | by_plugin("scan_crlf")
}
```

---

## C2 — `let curr_type = ...` внутри loop объявлен «мутацией» (ломает DAG/чистоту)

**Проблема.** GTP-U использует `let curr_type = ext.next_ext_type;` внутри тела
цикла, называя это «мутацией локальной переменной». Но `let` в модели —
иммутабельное связывание, узел ациклического DAG. Мутация = effect, что
противоречит чистоте выражений и завершаемости.

**Решение.** Ввести явный **loop-carried accumulator** `carry` / `next`.
`let` остаётся строго иммутабельным. Переиспользование `let` под shadowing/assignment
**запрещено** (`VerificationError: redefinition of binding`).

```
loop extensions
    carry curr_type: u8 = next_ext_type      // инициализация из C_loc
    while curr_type != 0
{
    ext: ExtHeader;
    next curr_type = ext.next_ext_type;      // обновление carry на след. итерацию
}
```

Семантика — fold/unfold: `carry` иммутабелен **внутри** итерации, пересчитывается
только на её границе через `next`. DAG остаётся ацикличным (внутри итерации),
завершаемость сохраняется (см. C2-progress).

**C2-progress.** Каждая итерация `loop` обязана потребить ≥ 1 байт. Где доказуемо
статически — проверяется компилятором; иначе — обязательный runtime guard
(`consumed >= 1`, иначе `RuntimeSafetyAbort::NonProgressLoop`). Плюс
конфигурируемый `max_loop_iterations`.

---

## C3 — FSM-ключ ссылается на поля родительского слоя (`IPv4.src`) без scope-правил

**Проблема.** TCP `key = (IPv4.src, IPv4.dst, src_port, dst_port)` требует доступа
к полям родительского слоя по имени протокола. Модель `C_loc` / `C_sess` не
содержит `C_parent`.

**Решение.** Ввести **layer-path scope**: qualified-имя `Proto.field` резолвится
по стеку активных родительских фреймов dispatcher (`layer_stack`). Доступ —
только чтение. Статически проверяется по bind-графу: если ни один путь исполнения
к данному слою не гарантирует присутствие `Proto` в стеке —
`VerificationError: parent layer 'Proto' not guaranteed in scope`.

---

## C4 — Session-ключ непригоден для request/response корреляции

**Проблема.** RADIUS `key = (UDP.src_port, UDP.dst_port, identifier)` и
TCP 4-tuple не нормализованы по направлению: request (client→server) и
response (server→client) получают разные ключи → разные сессии. Корреляция ломается.

**Решение.** Ввести две формы:
- `bidir(a, b)` — канонический направленно-независимый компонент для одиночных
  значений.
- `bidir_tuple((a1, a2, ...), (b1, b2, ...))` — атомарная сортировка endpoint
  tuple для TCP-like ключей, где IP и port должны меняться местами вместе.

```
state_machine AuthDialog {
    key = (bidir(UDP.src_port, UDP.dst_port), identifier);
}
state_machine Connection {
    key = bidir_tuple((IPv4.src, src_port), (IPv4.dst, dst_port));
}
```

Независимая нормализация `bidir(IPv4.src, IPv4.dst)` +
`bidir(src_port, dst_port)` запрещена для endpoint keys: она создаёт коллизию
между `A:8000 -> B:80` и `A:80 -> B:8000`.

---

## C5 — SMT/Z3 заявлен обязательным (конфликт с «прагматичным v1»)

**Проблема.** Z3 как hard-dependency усложняет сборку, дестабилизирует время
компиляции и избыточен для тривиальных линейных bounds (`length-8` при
`validate length>=8`).

**Решение.** v1 = **interval/range analysis** + propagation фактов из `validate`.
Невыводимые статически bounds **не отклоняют спеку**, а **понижаются до
runtime-проверки** (`BOUNDS_CHECK` → `ConstraintError` при нарушении) с
compile-warning. Z3 — опциональный verifier-backend за feature-flag в v1.5,
снимающий runtime-проверки для доказуемых случаев. Safety гарантирована всегда
(OOB невозможен), отличается лишь where проверка происходит.

---

## C6 — `match` с разными layout'ами веток требует sum-типа

**Проблема.** Diameter `match code { case 284 => grouped: ...  default => data: ... }`
производит ветки с разной структурой. Типовая система §3.3 не содержит sum-типов.

**Решение.** `match` производит **tagged union** в Typed AST:
`union { tag: <disc>, <branch_name>: <layout>, ... }`. Дискриминатор сохраняется
как тег. Ветки type-checked независимо; поля до/после `match` имеют единый тип.
Выбор ветки — ordered choice, lookahead = 0 (дискриминатор уже прочитан).

---

## C7 — Рекурсивная инкапсуляция (GTP-U → IPv4 → ...) без предела

**Проблема.** `bind IPv4 payload to TunnelMessage when type==255` допускает
бесконечную инкапсуляцию на злонамеренном пакете.

**Решение.** Циклы в **bind-графе разрешены** (легитимное туннелирование, в отличие
от field-DAG). Runtime-рекурсия ограничена двумя механизмами:
1. Глобальный `max_layer_depth` (config, default 16) → `Malformed::MaxDepthExceeded`.
2. **Payload-shrink invariant**: каждый bind обязан уменьшать payload-slice
   минимум на размер заголовка родителя (≥ 1 байт). Bind на пустой/нерастущий
   payload → abort. Ловит zero-length-инкапсуляцию до достижения лимита глубины.

---

## C8 — `__current_offset`: локальный или абсолютный?

**Проблема.** Loop-границы (`(__current_offset - start) < attrs_len`) требуют
локального смещения; DNS decompression требует абсолютного смещения в root-буфере.
Смешение → ошибки разбора.

**Решение.** Два различных builtin:
- `__current_offset` — смещение в **текущем сообщении** (локальное). Используется
  в loop-условиях, выражениях длины.
- `__root_offset` — **абсолютное** смещение в root-буфере. Передаётся плагинам
  (DNS decompress) вместе с `__root_buffer`.

---

## C10 — Atomic endpoint session keys (`bidir_tuple`, C4 extension)

**Проблема.** Независимая сортировка IP и портов в endpoint-ключе
(`sort(ip_a,ip_b)` + `sort(port_a,port_b)`) даёт рассогласованные ключи
(request/response не коррелируют).

**Решение.** Формализует расширение C4: `bidir_tuple((a1, a2, ...), (b1, b2, ...))`
с атомарной лексикографической сортировкой **целых endpoint-структур**.
Verifier отвергает split-sort паттерны (`NFDL0410`). См.
[ADR-008](ADR-list-critical-decisions.md).

---

## Последствия

- Грамматика v1 (`grammar.ebnf`) и формальная семантика (`semantics.md`)
  реализуют именно эти решения.
- Все шесть proof-by-example переписаны под исправленный синтаксис
  (`examples/*.nfdl`).
- M0 (ARP vertical slice) кодирует уже непротиворечивый синтаксис.

## Отклонённые альтернативы

- **`let`-shadowing для loop-state** — ломает видимость между итерациями и DAG.
- **Z3-mandatory в v1** — блокирует поставку, избыточен.
- **Единый `bytes[EOF]`** — неустранимая datagram/stream неоднозначность.
- **match с обязательным общим layout** — недостаточно выразителен для Diameter.
