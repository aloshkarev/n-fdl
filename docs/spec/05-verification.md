# N-FDL Static Verification v1

Фаза верификации работает над Typed AST и порождает Verified IR. Это последняя
AOT-граница: **после неё runtime-паника по логике DSL невозможна по построению**
(остаются только `ConstraintError` / `NeedMoreBytes` / `Malformed` / `PluginError`).
Реализуется в crate `nfdl-verify`. Единственный источник `VerificationError`.

Принцип: дёшево и консервативно. Где не можем доказать статически — НЕ отклоняем,
а вставляем runtime-проверку (downgrade, C5/ADR-003), сохраняя safety.

## 1. Конвейер проверок (порядок важен)

```
1. Name resolution / scope          (использует 03-semantics §4)
2. Field Dependency DAG + acyclicity (§2)
3. Forward-reference check           (§2.3)
4. Type checking                     (04-type-system; перемежается с 1)
5. Interval / bounds analysis        (§3)  -> аннотации Proven|RuntimeCheck
6. Conditional-field (Option) bounds (§4)
7. Stream boundary safety            (§4.5)
8. Loop progress analysis            (§5)
9. Plugin signature check            (§6)
10. Layer (bind) graph analysis       (§7)
11. FSM liveness/key analysis         (§8)
```

Любая фаза может выдать `VerificationError` с diagnostic-id и span. Несколько
ошибок собираются и репортятся батчем (как rustc), а не на первой.

> **M0 implementation subset:** в текущей реализации (`nfdl-verify` M0) активны
> фазы 5–6 (interval bounds, advisory notes) и частичная 7 (stream-rem в parser).
> Полный reject-on-error pipeline — целевое состояние; M0 эмитит warnings для
> не доказанных bounds (`NFDL0412` note) без отклонения спеки. Safety сохраняется
> runtime-downgrade (C5). Матрица фаз → milestone: [13-roadmap.md](13-roadmap.md).

### 1.1 Name redefinition (C2)

Повторное `let x` / shadowing `carry` в том же scope →
`VerificationError::RedefinitionOfBinding` (`NFDL0401`).

## 2. Field Dependency DAG

### 2.1 Построение

Узлы: каждое `field`, `let`, `carry`. Рёбра `a → b`: «вычисление `b` использует
`a`». Источники рёбер: свободные имена в `bytes[e]`, `if c`, `let = e`,
`validate p`, `match disc`, `while cond`, `next = e`, `key`, `guard`, `when`.

### 2.2 Acyclicity (Tarjan SCC)

Запускаем Tarjan на DAG. Любая SCC размера > 1 (или self-loop) →
`VerificationError::CyclicDependency` со списком имён в цикле. Топологическая
сортировка даёт порядок вычисления для lowering в IR.

Пример отвергаемого: `let a = b + 1; let b = a - 1;` → цикл `a↔b`.

### 2.3 Forward-reference

Дополнительно к ацикличности: имя в C_loc должно быть объявлено **текстуально и
физически раньше** (offset монотонен). Проверка: в топологическом порядке
позиция-источника во входном потоке < позиция-приёмника. Нарушение
(`bytes[future_len]` где `future_len` ниже) →
`VerificationError::ForwardReference`. Это сильнее ацикличности: `bytes[x]` где
`x` объявлен ниже без цикла всё равно запрещён (нельзя прочитать длину из ещё
не разобранных байт).

## 3. Interval / Bounds Analysis

Ядро bounds-safety. Для каждого выражения вычисляем интервал `[lo, hi]` (в i64),
распространяя:

- **Типовые интервалы** (04 §4): `u8 → [0,255]`, `bitfield{k} → [0, 2^k-1]`, ...
- **Refinement-факты** из `validate` (04 §6): `validate length >= 20` добавляет
  факт `length ≥ 20` в окружение от точки validate до конца scope.
- **Арифметику интервалов**: `[a,b] + [c,d] = [a+c, b+d]`, `-`, `*` (знаковая),
  и т.д., с **проверкой переполнения i64** (если границы выходят за i64 →
  `VerificationError::IntervalOverflow`, реально недостижимо для сетевых
  размеров, но защищает анализатор).

### 3.1 Проверка `bytes[e]`

Цель: доказать `0 ≤ e ≤ __rem` в точке чтения.

```
interval(e) = [lo, hi]
__rem в точке = [0, slice_len - cur_offset]   (cur_offset известен символически)

если lo ≥ 0  ∧  hi ≤ rem_lo:          -> Proven (zero runtime check)
если lo ≥ 0  ∧  hi не доказуемо:       -> RuntimeCheck (BOUNDS_CHECK в IR)
если lo < 0  доказуемо всегда:         -> VerificationError::NegativeLength (hard)
если lo < 0  возможно:                 -> RuntimeCheck (-> ConstraintError::NegativeLength)
```

`length - 8` при факте `length ≥ 8` → `lo = 0` → underflow исключён;
если `hi ≤ __rem` доказуемо → **Proven**. Это покрывает ARP, RADIUS, UDP.

### 3.2 Нелинейные выражения (Diameter padding, GTP ext)

`(4 - length%4)%4`, `(length*4)-2` — модулярная/нелинейная арифметика. Интервал
содержит встроенную аксиому modulo:

```
∀e, X > 0: interval(e % X) = [0, X-1]
```

Поэтому padding-паттерн `(4 - (length % 4)) % 4` доказывается как `[0,3]` без
SMT. Для составных выражений вроде
`(length*4)-2` при `length:u8 [1,255]` → `[2, 1018]`, нужно сравнить с `__rem`.
Где интервал не доказывает `≤ __rem` → **RuntimeCheck**. Z3-backend (v1.5,
feature `z3`) берёт те же выражения + факты и пытается доказать, снимая check.

### 3.3 Underflow / overflow

- Вычитание: если `lo` результата `< 0` возможен → underflow-риск → правило 3.1.
- Любая промежуточная операция вне i64 → hard error (защита анализатора).
- Runtime: VM использует checked-арифметику; переполнение в рантайме (теоретич.
  недостижимое после анализа) → `ConstraintError::ArithOverflow`, не паника.

## 4. Conditional-field (Option) bounds

Условное поле `f: τ if c` создаёт два пути: presence (`c=true`) и absence.
Анализатор раздваивает интервальное окружение по предикату `c`:

```
ветка c=true:  cur_offset += sizeof(τ);  f : τ доступно
ветка c=false: cur_offset без изменений; f отсутствует
```

Любое последующее `bytes[e]`, где `e` зависит от `f` или от `cur_offset`,
проверяется в **обеих** ветках; если хоть в одной не bounds-safe → RuntimeCheck
(или error при доказуемой негативности). Coalescing (`c ? f : 0`) сводит ветки
к одному интервалу. Пример: `gtpu` `seq_num: u16 if has_opt` + carry
`(has_opt ? next_ext_type : 0)` — анализатор видит обе ветки offset.

`??` считается явной формой coalescing для `Option[T]`: `ext_len ?? 0`
типизируется как `T`, а interval-анализ объединяет presence-ветку `ext_len` и
absence-ветку literal `0`.

### 4.5 Stream boundary safety

В `mode=stream` builtin `__rem` равен количеству уже доступных contiguous-байт,
а не длине логического сообщения. Поэтому verifier обязан отвергать:

- `loop ... while __rem ...`;
- `field: T if __rem ...`, если условие меняет layout stream-сообщения;
- `match __rem` или выражение match/branch, транзитивно зависящее от `__rem`.

Диагностика: `VerificationError::StreamRemControlFlow`. Разрешённые альтернативы:
length-prefixed framing, `bytes[EOF]`, `bytes[stream]`, либо plugin-framing
(`scan_crlf`), где отсутствие delimiter явно приводит к `NeedMoreBytes`.

## 5. Loop Progress Analysis (C2-progress, завершаемость)

Для каждого `loop` доказываем: **каждая итерация либо потребляет ≥ 1 байт, либо
имеет доказанный bounded carry-progress**.

### 5.1 Статическое доказательство (предпочтительно)

Тело loop содержит хотя бы одно **безусловное** чтение байт с доказуемо `≥ 1`
байт: `field: u8..u64` (фикс. размер ≥1), `field: bytes[e]` с доказанным
`e ≥ 1`, или вложенное сообщение с доказанным min-size ≥ 1.

```
min_consume(body) = сумма безусловных min-размеров полей пути
если min_consume ≥ 1 для всех путей -> ProgressProven
```

`Attribute` (RADIUS) min = `type:u8 + length:u8 = 2` → Proven.
`AVP` (Diameter) min = `code:u32+flags:u8+length:u24 = 8` → Proven.

### 5.2 Runtime guard (fallback)

Если статически не доказуемо (например, тело — только `bytes[e]` с `e`, могущим
быть 0) → IR помечает loop `progress = RuntimeGuard`. VM проверяет после каждой
итерации `consumed ≥ 1`; иначе допускает нулевую итерацию только при
`ProgressProven` (carry-progress variant) и активном `max_loop_iterations`.
`RuntimeSafetyAbort::NonProgressLoop`. Это гарантирует завершаемость даже для
TLV-паттернов с легальными zero-length элементами.

### 5.3 Дополнительные лимиты

- `max_loop_iterations` (config) — жёсткий потолок числа итераций →
  `Malformed::LoopLimit`. Ловит «прогрессирующие, но гигантские» циклы на
  раздутых длинах (DoS-вектор [dos-vectors.md](../plans/dos-vectors.md) DV-08–DV-10).
- Завершаемость по убыванию `__rem`: т.к. поток конечен и каждая итерация
  потребляет ≥1, число итераций ≤ `len(slice)`. Формальный аргумент
  завершаемости (termination measure = `__rem`, строго убывает).

## 6. Plugin Signature Check

Для каждого `invoke("p", args)`:

```
manifest(p) существует                   иначе VerificationError::UnknownPlugin
arity(args) == arity(manifest.args)      иначе ::PluginArity
∀i: type(arg_i) <: manifest.args[i]      иначе ::PluginArgType
purity(p) допустима в текущей позиции    иначе ::PluginPurityViolation
   (PURE в pure-expr OK; PURE в validate запрещён; STATEFUL только в не-pure поз.)
ret-тип регистрируется для дальнейшей типизации (record-доступ и т.д.)
```

## 7. Layer (bind) Graph Analysis

### 7.1 Построение bind-графа

Узлы — протоколы/сообщения; ребро `Outer →[pred] Inner` на каждый
`bind Outer payload to Inner when pred`. **Циклы допустимы** (туннелирование,
C7) — в отличие от field-DAG.

### 7.2 Проверки

- **Qualified-access reachability (C3):** для каждого `Proto.field` в сообщении
  M доказать, что на всех путях bind-графа, ведущих к M, `Proto` присутствует в
  стеке предков. Если существует путь без `Proto` →
  `VerificationError::ParentLayerNotInScope`. (Анализ достижимости по графу.)
- **Predicate purity:** `when`-предикат — pure над C_loc внешнего слоя.
- **Determinism warning:** если у одного Outer два `bind` с пересекающимися
  предикатами — ordered choice разрешает, но выдаётся warning о потенциальной
  неоднозначности.
- **Recursion bound (runtime, не статически):** циклы в графе → отметка
  «recursive»; runtime enforce `max_layer_depth` + payload-shrink (C7). Verifier
  лишь требует, чтобы рекурсивный bind имел убывающий payload по построению
  (родитель потребляет ≥1 байт заголовка) — иначе warning.

## 8. FSM Liveness Analysis

Для каждого `state_machine`:

- **Unreachable states:** обход из стартового состояния (первое объявленное или
  явно помеченное); недостижимые → `VerificationError::UnreachableState`
  (warning-уровень настраивается).
- **Sink states:** состояние без исходящих transitions И без таймера →
  потенциальный «застрявший» диалог. v1: warning `DeadEndState`. v1.5 с
  таймерами: sink без timer-выхода и не помеченный terminal →
  `VerificationError::SinkState`.
- **Transition determinism:** в одном state несколько `on SameMsg` с
  пересекающимися guard — ordered choice разрешает, warning о неоднозначности.
- **Key well-formedness:** `key`-выражение pure, типы компонентов хешируемы;
  `bidir(a,b)` — оба аргумента одного типа (C4). Для endpoint-ключей verifier
  требует `bidir_tuple((ip_src, port_src), (ip_dst, port_dst))`, чтобы IP и port
  сортировались атомарно; независимая нормализация IP и port выдаёт
  `VerificationError::BidirEndpointSplit` для TCP-like 4/5-tuples.
  Иначе `VerificationError`.
- **Message reachability:** `on Msg` ссылается на существующее message-имя в
  области видимости протокола; иначе `::UnknownMessage`.

## 9. Выход фазы: Verified IR аннотации

Каждый bounds-чувствительный узел IR несёт:
```
BoundsProof = Proven | RuntimeCheck(reason)
ProgressMode = ProgressProven | RuntimeGuard
LayerKind    = Flat | Recursive   // depth limit from runtime config max_layer_depth (C7)
```
Эти аннотации управляют генерацией `BOUNDS_CHECK` и `LOOP_BACK{consumed_check}`
инструкций в байткоде (`05`→bytecode lowering). Proven-узлы НЕ генерируют
runtime-проверок → zero-overhead на доказанных путях.

## 10. Гарантии после verify (контракт)

1. Нет циклических/forward зависимостей.
2. Каждый доступ к буферу либо Proven-safe, либо защищён RuntimeCheck → **OOB
   невозможен**.
3. Каждый loop завершается (Proven или RuntimeGuard + max_iter).
4. Все типы согласованы; все плагины существуют с корректными сигнатурами.
5. Все qualified-доступы к родителям разрешимы.
6. FSM не содержит явных мёртвых конфигураций (или они помечены).

Эти гарантии — предпосылка свойства «no panics / no OOB» в testing-фазе.
