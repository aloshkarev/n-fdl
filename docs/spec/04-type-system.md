# N-FDL Type System v1

Типовая система — монотонная, без вывода полиморфизма (фиксированные типы),
flow-sensitive по условным полям. Цель: статически отвергать некорректные спеки
и снабжать верификатор (`05-verification.md`) фактами для bounds-анализа.
Реализуется в crate `nfdl-types`. Единственный источник `TypeError`.

## 1. Синтаксис типов

```
τ ::=
   | u8 | u16 | u24 | u32 | u48 | u64           // беззнаковые целые
   | i8 | i16 | i32 | i64                        // знаковые целые
   | bool
   | bitfield{k}                                 // 1 ≤ k ≤ 64; value-type uN (см. §3)
   | bytes[e] | bytes[..] | bytes[EOF] | bytes[stream] // Slice or chunk stream
   | str                                          // owned, только результат invoke
   | opaque                                       // непрозрачный handle (root_buffer, plugin state)
   | Option[τ]                                    // условное поле  (field: τ if c)
   | list[τ]                                      // loop items
   | loop_result{ items: list[τ], carries: record{...} } // результат loop
   | union{ tag: τ_d, b1: Layout1, ... }          // результат match (ADR-006)
   | record{ f1: τ1, ..., fn: τn }                // результат invoke (ADR-009/C9)
   | message Ident                                // вложенное сообщение (агрегат)
```

Внутренний домен вычисления выражений — `int` (= i64 с overflow-проверкой) и
`bool`. Все целочисленные типы при использовании в выражениях расширяются до
`int`; результат, попадающий в `bytes[...]`/поле, проверяется на диапазон.

## 2. Endianness

`meta endian = big|little` задаёт представление всех многобайтовых скаляров в
модуле по умолчанию. Для протоколов с per-packet или per-field byte order
разрешены явные формы `u32le`/`u32be` и динамический модификатор
`u32(endian = expr)`, где `expr: bool|int` мапится на little/big по правилу
манифеста протокола. Endian — атрибут **представления**, не части значения:
`u16` в big- и little-модуле имеет один value-type `u16`, различается только
декодер. Bitfield читаются MSB-first внутри своей битовой последовательности
независимо от endian (сетевой порядок бит).

## 3. Bitfield

- `bitfield{k}` имеет value-тип: наименьший `uN ≥ k` округлённый до {8,16,32,64}
  для хранения; семантически — беззнаковое `int` в `[0, 2^k - 1]`.
- Последовательность смежных `bitfield` пакуется MSB-first, может пересекать
  байтовые границы.
- **Alignment-правило (ADR-007):** перед любым НЕ-bitfield полем (или концом
  сообщения) сумма непрерывно-предшествующих bitfield-битов обязана быть кратна 8.
  Иначе `TypeError::BitfieldMisaligned`. Это исключает скрытое неявное
  выравнивание.

## 4. Целочисленные литералы и диапазоны

- Литерал типизируется как `int`; при присваивании/сравнении с полем типа `uN`
  проверяется попадание в диапазон `uN` (например `validate opcode <= 4` —
  `4` влезает в `u16` ✓).
- Каждый тип несёт статический интервал:
  `u8 → [0,255]`, `u16 → [0,65535]`, `i8 → [-128,127]`, `bitfield{4} → [0,15]`,
  и т.д. Эти интервалы — вход для bounds-анализа (`05-verification.md §3`).

## 5. Правила типизации (selected, нотация `Γ ⊢ e : τ`)

`Γ` — типовое окружение (имя → τ), строится по scope-правилам (`03-semantics §4`).

### 5.1 Поля и связывания

```
(T-Field)        Γ ⊢ τ ok      [ Γ ⊢ c : bool ]
                 ──────────────────────────────────────
                 Γ ⊢ (f : τ [if c]) ⊣ Γ[f := if c then Option[τ] else τ]

(T-Let)          Γ ⊢ e : τ      f ∉ dom(Γ_local)
                 ────────────────────────────────
                 Γ ⊢ (let f = e) ⊣ Γ[f := τ]

(T-Carry)        Γ ⊢ init : τ'      τ' <: τ
                 ─────────────────────────────────
                 в теле loop:  Γ ⊢ carry c : τ  ⊣ Γ[c := τ]
```

`<:` — отношение подтипа: `int`-литерал/узкий `uN` <: более широкий числовой;
`τ <: Option[τ]` неявно НЕ выполняется (Option вводится только `if`).

### 5.2 Арифметика и сравнения

```
(T-Arith)  Γ ⊢ e1 : int   Γ ⊢ e2 : int        op ∈ {+,-,*,/,%,<<,>>,&,|,^}
           ─────────────────────────────────────────────────────────────
           Γ ⊢ (e1 op e2) : int

(T-Cmp)    Γ ⊢ e1 : int   Γ ⊢ e2 : int        op ∈ {==,!=,<,<=,>,>=}
           ─────────────────────────────────────────────
           Γ ⊢ (e1 op e2) : bool

(T-Logic)  Γ ⊢ e1 : bool  Γ ⊢ e2 : bool       op ∈ {&&,||}
           ────────────────────────────────────────
           Γ ⊢ (e1 op e2) : bool

(T-Coalesce) Γ ⊢ e1 : Option[τ]  Γ ⊢ e2 : τ
             ────────────────────────────────
             Γ ⊢ (e1 ?? e2) : τ

(T-Tern)   Γ ⊢ c : bool   Γ ⊢ a : τ   Γ ⊢ b : τ
           ──────────────────────────────────────
           Γ ⊢ (c ? a : b) : τ
```

`==`/`!=` допустимы и над `bool`. Деление/`%` на статический 0 → `TypeError`;
на runtime 0 → `ConstraintError::DivByZero`.

### 5.3 Option (условные поля), flow-sensitivity

Использование `f : Option[τ]` в выражении, требующем `τ`, легально **только**
если verifier докажет presence на данном пути, иначе `TypeError::MaybeAbsent`.
Практика v1: значение Option-поля, используемое в длине последующего поля,
требует, чтобы это последующее поле имело то же presence-условие, ЛИБО
coalescing через `??` или тернарный (`next_ext_type ?? 0`,
`has_opt ? next_ext_type : 0` в `gtpu.nfdl`).
Подробный bounds-учёт обеих веток — `05-verification §4`.

### 5.4 bytes / длины

```
(T-BytesExpr)  Γ ⊢ e : int       (verifier: e ≥ 0 ∧ e ≤ __rem)
               ─────────────────────────────────────────────
               Γ ⊢ bytes[e] : bytes

(T-BytesRest)  mode ∈ {datagram, stream(len-known)}
               ───────────────────────────────────
               Γ ⊢ bytes[..] : bytes

(T-BytesEof)   mode = stream ∧ это последнее поле ∧ meta.eof объявлен
               ──────────────────────────────────────────────────────
               Γ ⊢ bytes[EOF] : bytes

(T-BytesStream) mode = stream ∧ это последнее поле
                ──────────────────────────────────
                Γ ⊢ bytes[stream] : stream_bytes
```

Нарушение условий `bytes[EOF]` — `VerificationError` (фаза verify), не TypeError.
`bytes[stream]` не является `bytes`-значением в AST; это event-emitting sink,
который производит `PAYLOAD_CHUNK` и не может участвовать в последующих
выражениях.

### 5.5 match → union (ADR-006/C6)

```
(T-Match)  Γ ⊢ disc : int
           ∀ i: Γ ⊢ case_i_pat : int        Γ_i = Γ ⊢ body_i ⊣ Γ_i
           ─────────────────────────────────────────────────────────
           Γ ⊢ match ⊣ Γ[m := union{ tag: int, b_i: Layout(Γ_i \ Γ) }]
```

Поля, объявленные внутри ветки, видны только в этой ветке и инкапсулируются в
union под её тегом. Поля до/после match — общие, в Γ. Дискриминатор — `int`,
вычислен из уже прочитанных значений (lookahead 0).

### 5.6 loop → list

```
(T-Loop)  тело типизируется с Γ + carries + __count:int
          элемент итерации имеет агрегатный тип τ_item
          ──────────────────────────────────────────────
          Γ ⊢ loop name ... ⊣ Γ[name := loop_result{
              items: list[τ_item],
              carries: record{ carry_name: carry_type, ... }
          }]
```

Для обратной совместимости human-readable вывод может отображать `name.items`
как `name`, но Typed AST и verifier обязаны хранить carries явно.

### 5.6.1 session projection

```
(T-SessionProj)  declared_session_var(k) : τ
                 position ∈ {let, invoke-arg}
                 ─────────────────────────
                 Γ ⊢ __session("k") : τ
```

Projection read-only и запрещён в выражениях, определяющих форму разбора
(`if`, `while`, `match`, `bytes[e]`, `key`, `bind when`).

### 5.7 invoke → record / скаляр (ADR-009/C9)

```
(T-Invoke) manifest(p) = { args: (a1..an), ret: τ_r, purity }
           Γ ⊢ arg_i : a_i'      a_i' <: a_i      purity допустима в позиции
           ────────────────────────────────────────────────────────────────
           Γ ⊢ invoke("p", arg_1..arg_n) : τ_r

(T-Field-Access)   Γ ⊢ e : record{ ..., f: τ, ... }
                   ──────────────────────────────────
                   Γ ⊢ e.f : τ
```

`τ_r` ∈ { скаляр, bool, str, opaque, record{...} }. Доступ к несуществующему
полю record → `TypeError::NoSuchField`. Arity/type mismatch с манифестом →
`TypeError::PluginSignature`.

### 5.8 qualified parent access (C3)

```
(T-Qual)   layer-graph гарантирует Proto в стеке на всех путях к текущему msg
           Proto.field : τ_field  (по типу поля в Proto)
           ──────────────────────────────────────────────
           Γ ⊢ Proto.field : τ_field
```

Если присутствие не гарантировано → `VerificationError::ParentLayerNotInScope`
(проверка в verify-фазе по bind-графу, не в локальном typecheck).

## 6. Refinement types

`validate p -> "msg"` сужает тип последующего использования через факт `p`.
Формально: после `validate p`, в типовом окружении регистрируется предикат `p`
как **факт** для interval-анализа. Тип поля `length : u16 { length >= 20 }`
после `validate length >= 20`. Refinement не меняет представление — только
сужает доказуемый интервал (используется bounds-проверками). Refinement-факты
flow-sensitive: действуют от точки `validate` до конца текущего scope.

## 7. Что НЕ входит в типовую систему v1

- Параметрический полиморфизм / generics.
- User-defined struct/enum (сообщения и union/record покрывают нужды).
- Float-типы.
- Зависимые типы в полном смысле (длины — через verifier, не через типы-Π).
- Subtyping сложнее числового расширения.

Прагматика: типовая система намеренно «скучная и монотонная» — вся «умность»
(зависимые длины, bounds) вынесена в отдельную фазу верификации, что упрощает и
типизацию, и доказуемость.
