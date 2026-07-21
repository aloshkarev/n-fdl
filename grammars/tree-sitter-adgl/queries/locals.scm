; Minimal locals / scopes for ADGL (IDE track).

; ----- scopes -----
(ruleset_decl) @local.scope
(evidence_rule) @local.scope
(decision_rule) @local.scope
(correlate_block) @local.scope
(if_else_block) @local.scope

; ----- definitions -----
(evidence_rule name: (identifier) @local.definition.function)
(decision_rule name: (identifier) @local.definition.function)

(anchor_block binding: (identifier) @local.definition.var)
(decision_anchor binding: (identifier) @local.definition.var)
(correlate_block binding: (identifier) @local.definition.var)

(action_stmt name: (identifier) @local.definition.function)

; ----- references -----
(identifier) @local.reference
