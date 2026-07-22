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
- Each canonical path is expanded **once** — diamond includes (`A→S`, `B→S`) do not duplicate `S` rules.
- Included header decls (`requires`, …) are ignored in this cut.
- Cycles (`A → B → A`) fail with `ADGL4000`.

### Loader diagnostics (`ADGL40xx`)

| Code | Name | When |
| --- | --- | --- |
| `ADGL4000` | IncludeCycle | Include graph has a cycle on the active DFS stack |
| `ADGL4001` | IncludeIoError | Missing / unreadable include path |
| `ADGL4002` | MalformedInclude | Bad `include` directive form |
| `ADGL4003` | IncludeInternalError | Span/composition invariant failure while splicing |

These IDs live in the loader band (`ADGL40xx`), not `ADGL02xx` (TypeError).

## API

- `parse_ruleset(&str)` — unchanged, single-file; rejects `include`.
- `load_ruleset(path) -> LoadedRuleset` then `LoadedRuleset::parse()`.
- `airpulse_dsl_verify::verify_path(path)` — load (with includes) then AOT verify/lower.
- `ndsl-cli parse|check` — ADGL paths use `load_ruleset`.
- `ndsl-cli verify` — semantic verify (`verify_path`); **not** the same as `check` (parse+lint).

## Follow-up

Promote `include` into the grammar + tree-sitter ADGL grammar when dual-track editors need it. Do **not** feed tree-sitter into verify/runtime.
