; ADGL highlights (IDE track). Expand as editors adopt the grammar.

(line_comment) @comment
(block_comment) @comment

(string) @string
(integer) @number
(duration) @number
(signed_integer) @number
(boolean) @constant.builtin

[
  "{" "}" "(" ")" "[" "]"
  "," "."
] @punctuation.delimiter

[
  "ruleset"
  "version"
  "requires"
  "mutually_exclusive"
  "evidence"
  "decision"
  "scope"
  "anchor"
  "correlate"
  "infer"
  "emit"
  "action"
  "if"
  "else"
  "present"
  "absent"
  "and"
  "or"
  "not"
  "Cause"
  "Problem"
  "event"
  "topo"
  "time"
  "having"
  "count"
  "target"
  "weight"
  "evidence"
  "severity"
  "sarif_id"
  "reason"
  "in"
] @keyword

[
  "Session"
  "Port"
  "ClientMac"
  "Vlan"
  "AccessPoint"
  "Global"
] @type.builtin

[
  "Critical"
  "High"
  "Medium"
  "Low"
  "Recommended"
  "Optional"
] @constant

(ruleset_decl name: (string) @string.special)
(evidence_rule name: (identifier) @function)
(decision_rule name: (identifier) @function)
(anchor_block binding: (identifier) @variable)
(decision_anchor binding: (identifier) @variable)
(correlate_block binding: (identifier) @variable)
(cause_anchor kind: (identifier) @type)
(problem_anchor kind: (identifier) @type)
(infer_stmt kind: (identifier) @type)
(emit_stmt kind: (identifier) @type)
(action_stmt name: (identifier) @function)

[
  "==" "!=" "<" "<=" ">" ">="
  "&&" "||" "!"
  "+" "-" "*" "/" "%"
  "=" ":"
] @operator
