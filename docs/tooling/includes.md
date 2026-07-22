# ADGL multi-file `include` (loader-level)

Wave 7 first cut: composition lives in `airpulse_dsl_syntax::load_ruleset`, **not** in the EBNF grammar or tree-sitter grammars.

## Surface

Leading directives only, before `ruleset`:

```adgl
include "shared.adgl"

ruleset "main" {
  version = "1.0"
  // ...
}
```

- Paths are relative to the including file.
- Included files are full rulesets (and may `include` further).
- Entry ruleset keeps name / version / header decls; included **rules** are prepended (depth-first).
- Included header decls (`requires`, …) are ignored in this cut.
- Cycles (`A → B → A`) fail with `ADGL0200`.

## API

- `parse_ruleset(&str)` — unchanged, single-file; rejects `include`.
- `load_ruleset(path) -> LoadedRuleset` then `LoadedRuleset::parse()`.

## Follow-up

Promote `include` into the grammar + tree-sitter ADGL grammar when dual-track editors need it. Do **not** feed tree-sitter into verify/runtime.
