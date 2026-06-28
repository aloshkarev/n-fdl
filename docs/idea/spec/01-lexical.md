# ADGL Lexical Specification v1

Лексическая структура ADGL. Лексер — чистая функция `&str -> Vec<Token>`,
единственный (вместе с парсером) источник `SyntaxError`. Все токены несут
`Span { byte_start, byte_end, line, col }` для ariadne-диагностики
([11-error-diagnostics.md](11-error-diagnostics.md)). Crate-владелец:
`airpulse_dsl::syntax` (проектный).

Конвенции зеркалируют N-FDL [../../spec/01-lexical.md](../../spec/01-lexical.md).

## 1. Кодировка и whitespace

- Исходник — UTF-8. Идентификаторы и ключевые слова — ASCII.
- Whitespace: `' '`, `'\t'`, `'\r'`, `'\n'` — незначимы кроме как разделители
  токенов. ADGL **не** whitespace-sensitive.
- Конец строки — LF или CRLF (нормализуется).

## 2. Комментарии

```
line_comment   ::= "//" { любой символ кроме '\n' }
block_comment  ::= "/*" { любой символ } "*/"     (НЕ вложенные в v1)
```

Блочные комментарии не вкладываются (`/* /* */ */` завершается на первом
`*/`). Span сохраняется для doc-инструментов.

## 3. Идентификаторы и ключевые слова

```
ident   ::= (letter | '_') { letter | digit | '_' }
letter  ::= 'a'..'z' | 'A'..'Z'
digit   ::= '0'..'9'
```

- Максимальная длина идентификатора — 255 байт (DoS-guard).
- Регистр значим. Конвенция (не enforced): ruleset/scope-типы — `PascalCase`;
  rule-имена/anchor-связывания — `snake_case`; builtins — `__snake`.

### 3.1. Зарезервированные ключевые слова

```
top-level:    ruleset  version  requires  mutually_exclusive
rule kinds:   evidence  decision
rule body:    scope  anchor  correlate  infer  emit  action
control:      if  else  present  absent  and  or  not
node types:   Cause  Problem  event
```

Идентификатор, совпадающий с ключевым словом, — `SyntaxError`. `event`,
`Cause`, `Problem` — контекстные ключевые слова (допустимы как ident вне
позиции объявления узла, чтобы не занимать имена без нужды).

### 3.2. Builtins (контекстные идентификаторы, не ключевые слова)

```
__watermark  __scope  __confidence  __ruleset_version
```

Лексически — обычные идентификаторы с префиксом `__`. Префикс `__` зарезервирован:
пользовательские идентификаторы **не могут** начинаться с `__`
(`SyntaxError: '__' prefix is reserved`).

## 4. Числовые и временные литералы

ADGL работает с целочисленными метриками и временными интервалами. Float
отсутствует (диагностика целочисленна; confidence 0..100 — `u8`,
[04-type-system.md](04-type-system.md), ADR-002).

```
int_lit    ::= dec_lit | hex_lit | bin_lit
dec_lit    ::= digit { digit | '_' }
hex_lit    ::= ("0x" | "0X") hex_digit { hex_digit | '_' }
bin_lit    ::= ("0b" | "0B") bin_digit { bin_digit | '_' }

duration   ::= int_lit unit
unit       ::= "ms" | "s" | "min"
time_lit   ::= int_lit                       // абсолютная event-time (мс, epoch)
```

- `_` — визуальный разделитель (`1_000`, `0xDE_AD`).
- Целые литералы вычисляются в `i64`; переполнение —
  `SyntaxError: integer literal out of range`.
- `duration` (`500ms`, `1s`, `2min`) — тип `Duration` ([04](04-type-system.md)).
  Используется в `time:` окнах и `weight`-окнах дедупликации.
- `1s` = `1000ms`; `1min` = `60s`. Смешанные единицы в одном литерале запрещены
  (`1s500ms` → `SyntaxError`, используйте `1500ms`).

## 5. Строковые литералы

```
string_lit ::= '"' { string_char } '"'
string_char ::= любой символ кроме '"' и '\' | escape
escape      ::= '\' ( '"' | '\' | 'n' | 't' | 'r' | '0' | "x" hex_digit hex_digit )
```

Используются в `ruleset "name"`, `version = "1.0"`, `requires = ["..."]`,
`reason: "..."`, severity-enum значениях. Многострочных строк в v1 нет.

## 6. Операторы и пунктуация

```
// арифметика/сравнение (для metric-предикатов)
+  -  *  /  %        ==  !=  <  <=  >  >=
// логика
&&  ||  !   (и синонимы and or not — ключевые слова)
// принадлежность окну
in
// структура
{  }  (  )  [  ]
;  ,  .  :  =  ->
```

Замечания:
- `in` — ключевое слово для `time: x.time in [a, b]` (и `metric in [...]`
  если добавится).
- `:` — разделитель `anchor rtx: event(...)`, `scope: Session`,
  `target: rtx.target`.
- `->` — не используется в v1 (зарезервирован для будущих transition-form).
- `.` — qualified access (`rtx.segment_size`, `c.confidence`,
  `dhcp.vlan`, `rtx.path`).

### 6.1. Приоритет операторов (от низшего к высшему)

```
1  ||  or
2  &&  and
3  ==  !=  <  <=  >  >=  in
4  +  -
5  *  /  %
6  унарные: !  not
7  . (qualified)  ()-вызов  []-индекс
```

Скобки переопределяют приоритет. Все бинарные операторы лево-ассоциативны.
`in` — неассоциативный (только в `time:`/явных окнах).

## 7. Максимальный munch и неоднозначности

- Лексер использует maximal-munch: `>=` — один токен (не `>` `=`); `&&` — один.
- `..` (диапазон) в v1 **не вводится** — окна задаются `[a, b]` (список из двух
  границ), чтобы избежать конфликта с `.` qualified-access.
- Число с unit: `1s` лексируется как `int_lit` `1` + `ident` `s` только если `s`
  не является валидным unit — лексер распознаёт `duration` цельным токеном
  (`500ms`, `1s`, `2min`). `s`/`ms`/`min` вне числового контекста — обычные
  идентификаторы.

## 8. Лимиты лексера (DoS-guard, ADR-011)

| Лимит | Значение | Причина |
|---|---|---|
| max token length | 255 B | переполнение идентификатором |
| max source size | 4 MiB | спека — не данные |
| max nesting (скобки/блоки) | 64 | защита парсера от stack-blowup |
| max `requires` entries | 32 | DoS на capability-check |
| block comment не вложен | — | детерминированный конец |

Превышение — `SyntaxError` с понятной диагностикой. Лексер не паникует ни на
каком входе (fuzz-target, [12-testing.md](12-testing.md)).

## 9. Token kinds (для реализации)

```
TokenKind =
  | Ident(Symbol) | Keyword(Kw) | Builtin(BuiltinId)
  | IntLit(i64) | DurationLit(i64_ms) | StringLit(Box<str>)
  | Op(Op)          // + - * / % == != < <= > >= && || ! .
  | Punct(Punct)    // { } ( ) [ ] ; , : = -> in
  | Eof
```

Каждый токен: `Token { kind, span }`. `DurationLit` хранит значение в
миллисекундах (`i64_ms`) — единая внутренняя единица времени; верификатор
проверяет, что окна `time:` вычислимы в этой единице
([05-verification.md](05-verification.md) §3).

## 10. Контракт

1. Лексер — total function: любой `&str` → `Vec<Token>` или `SyntaxError`,
   никогда не паникует.
2. Префикс `__` — только builtins; пользовательское `__x` — `SyntaxError`.
3. `duration` — цельный токен; `ms`/`s`/`min` вне числового контекста — idents.
4. Окна `[a, b]` (не `a..b`) — избегаем `..` конфликта с `.`.
5. Все лимиты §8 enforced с `ADGL01xx` диагностикой ([11](11-error-diagnostics.md)).
