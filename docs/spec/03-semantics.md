# N-FDL Formal Semantics & Scope Rules v1

Определяет точную семантику каждой конструкции, правила видимости, разделение
pure/effect и инварианты завершаемости. Это контракт между грамматикой
(`02-grammar.ebnf`) и верификатором/VM.

## 1. Семантические домены

```
B*          поток байт (входной буфер)
Slice       = { buf, offset, len }            zero-copy view, всегда in-bounds
M           типизированное AST сообщения (результат разбора)
S           состояния EFSM
C_loc       локальный контекст: имя -> Value (поля, let, carry текущего сообщения)
C_parent    стек родительских слоёв (layer_stack): Proto -> M
C_sess      контекст сессии: имя -> OwnedValue (переживает пакеты)
Env         = (C_loc, C_parent, C_sess, builtins)
Err         SyntaxError | TypeError | VerificationError      (AOT)
            | ConstraintError | NeedMoreBytes | Malformed | PluginError   (runtime)
```

Сигнатура разбора (монадическая, зависимые Π-типы по C_loc):

```
decode : B* × Env  ->  Either< Err, (M, B*_rest, C_loc') >  ∪  { NeedMoreBytes }
```

`NeedMoreBytes` достижим только при `mode = stream`.

## 2. Pure vs Effect (строгая граница)

Верификатор присваивает каждому выражению/конструкту эффект-класс.

| Класс | Конструкции | Где разрешено |
|---|---|---|
| **Pure** | арифметика, сравнения, логика, тернарный, чтение field/let/carry/builtin, qualified `Proto.f`, `invoke` PURE-плагина | `let`, `validate`, `if`, `match` disc, `bytes[expr]`, `while`, `guard`, `key`, `next`, bind `when` |
| **Read-effect** | `field: T` (двигает offset) | только как `FieldStmt` |
| **Session-effect** | `set v = e` | только FSM `Action` |
| **Event-effect** | `emit`, `start_timer`, `cancel_timer` | только FSM `Action` |
| **Layer-effect** | `bind` | только top-level декларация |
| **Stateful-FFI** | `invoke` STATEFUL-плагина (v1.5) | выделенная не-pure позиция (не внутри pure-expr) |

**Правило:** effect в pure-позиции → `VerificationError`. В частности
`validate invoke("crc")` запрещён, даже если плагин PURE (validate обязан быть
детерминированно-без-внешних-вызовов для воспроизводимости диагностики).
*(Решение: invoke в validate запрещён в v1; пересмотр — v1.5 для PURE-плагинов.)*

## 3. Семантика конструкций (operational)

Обозначение: `⟨stmt, Env, off⟩ ⇓ ⟨Env', off'⟩` — исполнение stmt сдвигает
offset `off -> off'` и обновляет окружение.

### 3.1 FieldStmt `f : T [if c] ;`

```
eval(c, Env) = false   =>   C_loc' = C_loc[f := None];  off' = off      (поле отсутствует)
eval(c, Env) = true    =>   читаем T:
    n = sizeof_or_lenexpr(T, Env)                    // для bytes[e]: n = eval(e)
    требуем off + n <= len(slice)                    // bounds
        иначе: datagram -> ConstraintError(OOB)
               stream   -> yield NeedMoreBytes
    v = decode_value(T, slice[off .. off+n])
    C_loc' = C_loc[f := Some(v)]; off' = off + n
```

Для `bytes[e]`: требуется неотрицательная длина (доказано статически или runtime
`ConstraintError::NegativeLength`). Для `bytes[..]`: `n = len(slice) - off`.
Для `bytes[EOF]` (stream): читает до EOF-сигнала; до него — `NeedMoreBytes`.
Для `bytes[stream]` (stream): поле не строит единый AST-срез; VM периодически
эмитит `PAYLOAD_CHUNK` в event bus и освобождает уже переданный contiguous-префикс
согласно лимитам reassembly. Это конструкция для больших тел без bounded EOF.

Тип условного поля — `Option<T>`; последующие выражения, использующие `f`,
типизируются с учётом возможного отсутствия (verifier требует, чтобы зависящие
длины были корректны в обеих ветках, см. §3 verification.md).

### 3.2 LetStmt `let x = e ;`

```
v = eval(e, Env)            // pure, не двигает offset
C_loc' = C_loc[x := v]; off' = off
```
`x` иммутабелен. Повторное `let x` в том же scope → `VerificationError`.
`x` участвует в DAG зависимостей (узел, рёбра = свободные имена в `e`).

### 3.3 ValidateStmt `validate p -> "msg" ;`

```
eval(p, Env) = true   =>   no-op
eval(p, Env) = false  =>   ConstraintError("msg")  — текущая ветка/сообщение Malformed
```
Не двигает offset. Факт `p == true` добавляется в knowledge base verifier для
последующего interval-анализа (`validate len>=20` доказывает `len-20 >= 0`).

### 3.4 MatchStmt (tagged union, C6)

```
d = eval(disc, Env)
выбрать первую case Ci: eval(Ci, Env) == d   (ordered choice, lookahead 0)
    нет совпадения -> default-ветка (если есть), иначе ConstraintError(NoMatch)
исполнить тело выбранной ветки как вложенный scope
результат: union{ tag := d, <branch> := <layout> }
```
Дискриминатор `disc` обязан быть уже прочитанным значением (pure, без чтения
байт). Ветки добавляют поля в C_loc под тегом; имена веток не конфликтуют.

### 3.5 LoopStmt (C2: carry/next, fold-семантика)

```
loop name carry c:T = init  while cond { body; next c = upd; }

state_0:   c_0 = eval(init, Env);  acc = []
iter k:    Env_k = Env + { c := c_k, __count := k }
           если eval(cond, Env_k) == false -> stop
           ⟨body, Env_k, off⟩ ⇓ ⟨Env_k', off_k'⟩         // body — вложенный scope
           PROGRESS: требуем off_k' - off >= 1            // C2-progress
               иначе RuntimeSafetyAbort::NonProgressLoop
           acc.push( item_k )
           c_{k+1} = eval(upd, Env_k')                    // next: пересчёт carry
           off = off_k'
результат: C_loc[name := record{ items := list(acc), carries := final_carries }]
лимит: k <= max_loop_iterations  иначе Malformed::LoopLimit
```

`c` иммутабелен **внутри** итерации; меняется только через `next` на границе.
Несколько `carry` допустимы; `next` обязан существовать для каждого `carry`,
который меняется (отсутствие `next` => carry константен — допустимо, warning).
Итоговые значения carry видимы после цикла как `loop_name.carries.<name>`, а
элементы — как `loop_name.items`. Это нужно для протоколов, где связный список
несёт тип следующего заголовка/слоя за пределы самой коллекции.

Правило progress по умолчанию требует потребления ≥ 1 байта на итерацию. Нулевая
итерация допустима только если verifier докажет изменение хотя бы одного carry
и цикл имеет жёсткий конечный bound (`max_loop_iterations` или статический
эквивалент). Иначе — `RuntimeSafetyAbort::NonProgressLoop`.

### 3.6 InvokeExpr / InvokeType `invoke("p", args...)`

```
sig = manifest(p)                              // arity, типы — проверены AOT
проверить purity(p) допустима в текущей позиции
vals = map(eval, args)                         // args pure
out = ffi_call(p, __root_buffer, __root_offset, vals)
    ok(v)        -> v : sig.ret
    plugin_err   -> PluginError -> текущая ветка Malformed
```
Плагин получает root-буфер + абсолютный offset (C8). Не двигает локальный offset
(плагин читает произвольно, но не «потребляет» поток — потребление выражается
последующим `bytes[...]`, длина которого может зависеть от результата invoke).

## 4. Scope-правила (резолюция имён)

Поиск имени `x` идёт от внутреннего scope к внешнему; **первое совпадение**.

```
1. Loop-iteration scope   : поля итерации + carry-переменные + __count
2. Message-local C_loc     : field/let, объявленные ТЕКСТУАЛЬНО ВЫШЕ точки исп.
3. Builtins                : __root_buffer __current_offset __root_offset __rem __count
4. Parent layer C_parent   : ТОЛЬКО quailfied form  Proto.field  (не bare)
5. Session C_sess           : ТОЛЬКО внутри FSM guard/action (set/чтение)
```

### 4.1 Forward-reference запрещён

В C_loc имя резолвится только если объявлено **текстуально и физически раньше**
(offset монотонен). `bytes[future_len]` где `future_len` объявлено ниже →
`VerificationError::ForwardReference`. Это гарантирует ацикличность DAG и
завершаемость (см. verification.md §2).

### 4.2 Qualified access `Proto.field` (C3)

Резолвится по `layer_stack`. Статическая проверка: для каждого пути в bind-графе,
ведущего к текущему сообщению, `Proto` обязан присутствовать в стеке. Иначе
`VerificationError::ParentLayerNotInScope`. Доступ read-only; скаляры копируются.

### 4.3 Разделение data plane / control plane

`C_sess` **не виден** в парсере сообщения (FieldStmt/LetStmt/...). Только FSM
guard/action читают/пишут `C_sess`. Парсер не может читать состояние сессии —
это сохраняет чистоту и завершаемость разбора (разбор не зависит от истории).

Исключение для криптографических протоколов — **session projection**:
`__session("name")` возвращает read-only `OwnedValue` из заранее разрешённого
набора session-переменных. Projection:

- разрешён только в `let`/аргументах `invoke` и только для immutable чтения;
- не может участвовать в `while`, `if`, `match`, `bytes[e]`, `key` или `bind when`,
  чтобы структура разбора и потребление байт не зависели от истории;
- типизируется по декларации FSM/plugin manifest; отсутствующее значение даёт
  recoverable `NeedMoreSessionValue`/`ConstraintError`, не панику.

Так data plane остаётся детерминированным по байтам и read-only projection, но
может передать согласованные в FSM секреты в pure-плагин дешифровки:
`let pt = invoke("aes_gcm", __session("cipher_key"), payload);`.

### 4.4 Builtins — точная семантика (C8)

| Builtin | Тип | Значение |
|---|---|---|
| `__root_buffer` | opaque(ptr,len) | весь исходный буфер, read-only, для плагинов |
| `__current_offset` | u64 | смещение в **текущем сообщении** (локальное), от 0 |
| `__root_offset` | u64 | **абсолютное** смещение в root-буфере |
| `__rem` | u64 | байт осталось в текущем срезе = `len(slice) - __current_offset` |
| `__count` | u64 | индекс текущей итерации loop (0-based); вне loop — `TypeError` |
| `__session("k")` | τ | read-only projection из C_sess; verifier-gated |

В `mode=stream` `__rem` означает только уже доступные contiguous-байты, а не
границу логического сообщения. Поэтому `__rem` запрещён в условиях `loop while`,
`field if` и `match`, если эти условия определяют завершение/форму stream-разбора.
Для чтения до закрытия используется `bytes[EOF]`; для больших потоковых тел —
`bytes[stream]` или plugin-framing (`scan_crlf` и аналоги).

## 5. Детерминизм

- **Ordered choice** везде: `match` (по порядку case), `bind` (по порядку
  объявления), FSM transitions (по порядку `on` в state).
- **lookahead = 0**: ветвление только по уже вычисленным значениям; парсер
  никогда не «заглядывает вперёд» в непрочитанные байты.
- Один и тот же `(B*, Env)` всегда даёт идентичный результат (property:
  deterministic parse, см. testing §13).

## 6. Завершаемость (termination)

Гарантируется тремя независимыми механизмами:

1. **Field-DAG ацикличен** (Tarjan SCC на этапе verify) — нет циклических
   зависимостей длин.
2. **Loop progress** — каждая итерация потребляет ≥ 1 байт (статически или
   runtime guard), либо имеет доказанное carry-progress + жёсткий лимит итераций.
   В `stream` запрещено использовать `__rem` как единственный критерий конца
   логического сообщения.
3. **Нет общей рекурсии** в data plane: вложенные сообщения вызываются по
   фиксированной структуре; рекурсия слоёв (`bind`) ограничена `max_layer_depth`
   + payload-shrink invariant (C7).

Базовый DSL **не Тьюринг-полон**: нет неограниченных циклов, нет общей рекурсии,
нет изменяемого общего состояния в data plane. Все программы завершаются.

## 7. EFSM семантика

```
δ : S × M × C_sess  ->  S' × C_sess' × Effects
```

После успешного разбора сообщения `M`:
```
key = eval(KeyExpr, C_loc ∪ C_parent)        // bidir() нормализует (C4)
sess = session_db.get_or_create(key)
для каждого Transition "on M guard g -> S'" в state sess.state (по порядку):
    если msg_type == M и eval(g, C_loc ∪ C_sess) == true:
        исполнить actions:  emit/set/timer  (по порядку)
        sess.state := S'
        STOP (первый матч)
нет матча -> сессия не меняется (опц. diagnostic)
```
Эффекты (`emit`, таймеры) применяются в порядке появления → детерминированный
порядок событий на event bus.
