# N-FDL Lexical Specification v1

Лексическая структура N-FDL. Лексер — чистая функция `&str -> Vec<Token>`,
единственный (вместе с парсером) источник `SyntaxError`. Все токены несут
`Span { byte_start, byte_end, line, col }` для диагностики.

## 1. Кодировка и whitespace

- Исходник — UTF-8. Идентификаторы и ключевые слова — ASCII.
- Whitespace: `' '`, `'\t'`, `'\r'`, `'\n'` — разделители, незначимы кроме как
  разделители токенов. N-FDL **не** whitespace-sensitive.
- Конец строки — LF или CRLF (нормализуется).

## 2. Комментарии

```
line_comment   ::= "//" { любой символ кроме '\n' }
block_comment  ::= "/*" { любой символ } "*/"     (НЕ вложенные в v1)
```

Комментарии отбрасываются лексером (но span сохраняется для doc-инструментов
в будущем). Блочные комментарии не вкладываются — `/* /* */ */` завершается на
первом `*/` (ADR-зафиксировано для простоты; невложенность проверяется).

## 3. Идентификаторы и ключевые слова

```
ident   ::= (letter | '_') { letter | digit | '_' }
letter  ::= 'a'..'z' | 'A'..'Z'
digit   ::= '0'..'9'
```

- Максимальная длина идентификатора — 255 байт (DoS-guard на парсере).
- Регистр значим. Конвенция (не enforced лексером): протоколы/сообщения/
  состояния — `PascalCase`; поля/let/carry — `snake_case`; builtins — `__snake`.

### 3.1. Зарезервированные ключевые слова

```
protocol  message  meta  endian  mode  eof
field-уровень:  let  validate  match  case  default  loop  carry  while  next  if
типы:           u8 u16 u24 u32 u48 u64  i8 i16 i32 i64  bool  bitfield  bytes  str  opaque
layer/fsm:      bind  payload  to  when  state_machine  key  state  on  guard
actions:        emit  set  start_timer  cancel_timer
литералы:       true  false
```

Идентификатор, совпадающий с ключевым словом, — `SyntaxError`. `invoke` — **не**
ключевое слово, а встроенная форма вызова (lexer выдаёт ident; парсер распознаёт
контекстно), чтобы не занимать имя без нужды. *(Решение: оставить `invoke` контекстным.)*

### 3.2. Builtins (контекстные идентификаторы, не ключевые слова)

```
__root_buffer  __current_offset  __root_offset  __rem  __count
```

Лексически — обычные идентификаторы с префиксом `__`. Префикс `__` зарезервирован:
пользовательские идентификаторы **не могут** начинаться с `__`
(`SyntaxError: '__' prefix is reserved`). Это отделяет namespace builtins.

## 4. Числовые литералы

```
int_lit   ::= dec_lit | hex_lit | bin_lit
dec_lit   ::= digit { digit | '_' }
hex_lit   ::= ("0x" | "0X") hex_digit { hex_digit | '_' }
bin_lit   ::= ("0b" | "0B") bin_digit { bin_digit | '_' }
hex_digit ::= digit | 'a'..'f' | 'A'..'F'
bin_digit ::= '0' | '1'
```

- `_` — визуальный разделитель, игнорируется (`0xDE_AD`, `1_000`).
- Все целые литералы вычисляются в домене `i64` на этапе типизации; литерал,
  не влезающий в `i64`, — `SyntaxError: integer literal out of range`.
- Литералов с плавающей точкой в v1 **нет** (сетевые протоколы целочисленны).

## 5. Строковые литералы

```
string_lit ::= '"' { string_char } '"'
string_char ::= любой символ кроме '"' и '\' | escape
escape      ::= '\' ( '"' | '\' | 'n' | 't' | 'r' | '0' | "x" hex_digit hex_digit )
```

Используются в `validate ... -> "msg"`, `invoke("name", ...)`,
`by_plugin("scan_crlf")`. Многострочных строк в v1 нет.

## 6. Операторы и пунктуация

```
// арифметика
+   -   *   /   %
// битовые
<<  >>  &   |   ^   ~
// сравнение
==  !=  <   <=  >   >=
// логика
&&  ||  !
// тернарный
?   :
// структура
{   }   (   )   [   ]
;   ,   .   =   ->   =>   ..
:                              // разделитель field : type
```

Замечания:
- `->` — переход в `validate ... -> "msg"` и в FSM `on ... -> State`.
- `=>` — ветка `case expr => stmts`.
- `..` — rest-of-message в `bytes[..]`.
- `.` — qualified access (`IPv4.src`, `ext.next_ext_type`).
- `?` `:` — тернарный оператор `cond ? a : b` (как в Diameter `hdr_len`).
- `~` — побитовое НЕ (унарное).

### 6.1. Приоритет операторов (от низшего к высшему)

```
1  ? :            (тернарный, право-ассоц.)
2  ||
3  &&
4  |
5  ^
6  &
7  == !=
8  < <= > >=
9  << >>
10 + -
11 * / %
12  унарные: ! ~ - (унарный минус)
13 . (qualified)  ()-вызов  []-индекс/length-форма
```

Скобки `( )` переопределяют приоритет. Все бинарные операторы лево-ассоциативны,
кроме тернарного.

## 7. Максимальный munch и неоднозначности

- Лексер использует maximal-munch: `>>` — один токен (не два `>`); `<=` — один.
  Контекст «вложенные generic» отсутствует, поэтому `bitfield{9}` и `>>`
  не конфликтуют.
- `..` имеет приоритет над `.` `.` при maximal-munch (`bytes[..]`).
- `->` и `-` `>` различаются maximal-munch.

## 8. Лимиты лексера (DoS-guard)

| Лимит | Значение | Причина |
|---|---|---|
| max token length | 255 B | переполнение идентификатором |
| max source size | 4 MiB | спека — не данные |
| max nesting (скобки/блоки) | 64 | защита парсера от stack-blowup |
| block comment не вложен | — | детерминированный конец |

Превышение — `SyntaxError` с понятной диагностикой. Лексер не паникует ни на
каком входе (fuzz-target M0).

## 9. Token kinds (для реализации)

```
TokenKind =
  | Ident(Symbol) | Keyword(Kw) | Builtin(BuiltinId)
  | IntLit(i64) | StringLit(Box<str>) | BoolLit(bool)
  | Op(Op)              // + - * / % << >> & | ^ ~ == != < <= > >= && || ! ? :
  | Punct(Punct)        // { } ( ) [ ] ; , . = -> => .. :
  | Eof
```

Каждый токен: `Token { kind, span }`.
