; Minimal locals / scopes for N-FDL (IDE track).

; ----- scopes -----
(protocol_decl) @local.scope
(message_decl) @local.scope
(session_decl) @local.scope
(state_decl) @local.scope
(match_arm) @local.scope
(default_arm) @local.scope
(loop_stmt) @local.scope

; ----- definitions -----
(protocol_decl name: (identifier) @local.definition.type)
(message_decl name: (identifier) @local.definition.type)
(session_decl name: (identifier) @local.definition.type)
(state_decl name: (identifier) @local.definition.type)

(field_stmt name: (identifier) @local.definition.field)
(let_stmt name: (identifier) @local.definition.var)
(carry_decl name: (identifier) @local.definition.var)
(loop_stmt name: (identifier) @local.definition.var)

(bind_decl
  outer: (identifier) @local.definition.type
  inner: (identifier) @local.definition.type)

; ----- references -----
(identifier) @local.reference
