; N-FDL highlights (IDE track). Expand as editors adopt the grammar.

(line_comment) @comment
(block_comment) @comment

(string) @string
(integer) @number
(boolean) @constant.builtin

[
  "{" "}" "(" ")" "[" "]"
  ";" "," "."
] @punctuation.delimiter

[
  "protocol"
  "message"
  "meta"
  "endian"
  "mode"
  "eof"
  "let"
  "validate"
  "match"
  "case"
  "default"
  "loop"
  "carry"
  "while"
  "next"
  "if"
  "bind"
  "payload"
  "to"
  "when"
  "state_machine"
  "key"
  "state"
  "on"
  "guard"
  "emit"
  "set"
  "start_timer"
  "cancel_timer"
  "bitfield"
  "bytes"
  "invoke"
  "big"
  "little"
  "stream"
  "datagram"
  "on_fin"
  "on_close"
  "by_plugin"
  "bidir"
  "bidir_tuple"
  "EOF"
] @keyword

[
  "u8" "u16" "u24" "u32" "u48" "u64"
  "i8" "i16" "i32" "i64"
  "u16le" "u24le" "u32le" "u48le" "u64le"
  "u16be" "u24be" "u32be" "u48be" "u64be"
  "i16le" "i32le" "i64le"
  "i16be" "i32be" "i64be"
  "bool" "str" "opaque"
] @type.builtin

(builtin) @variable.builtin

(protocol_decl name: (identifier) @type)
(message_decl name: (identifier) @type)
(session_decl name: (identifier) @type)
(state_decl name: (identifier) @type)

(field_stmt name: (identifier) @property)
(let_stmt name: (identifier) @variable)
(bind_decl outer: (identifier) @type inner: (identifier) @type)

[
  "==" "!=" "<" "<=" ">" ">="
  "&&" "||" "!"
  "+" "-" "*" "/" "%"
  "<<" ">>" "&" "|" "^" "~"
  "->" "=>" ".." "??"
  "=" "?" ":"
] @operator
