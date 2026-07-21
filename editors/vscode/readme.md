# VS Code / editor pointers (IDE track)

N-FDL and ADGL use **tree-sitter grammars for editors only**. Per
[ADR-013](../../docs/adr/ADR-013-dual-track-treesitter.md), these CSTs must
**not** feed verify, bytecode, or runtime.

LSP is out of scope for Wave 1 (planned Wave 7). This note only explains how to
point an editor or extension at the grammars and query packs.

## Grammar locations

| Language | Extension | Grammar root |
|----------|-----------|--------------|
| N-FDL | `.nfdl` | `grammars/tree-sitter-nfdl/` |
| ADGL | `.adgl` | `grammars/tree-sitter-adgl/` |

Each grammar ships query packs under `queries/`:

- `highlights.scm` — syntax highlighting
- `folds.scm` — fold regions for protocol/message/ruleset/evidence/decision blocks
- `locals.scm` — minimal scopes and local definitions

## Pointing VS Code (or a tree-sitter extension) at the grammars

1. Install a tree-sitter-capable extension (for example one that loads local
   grammars / WASM, or a custom language extension you own).
2. Register the language scopes:
   - N-FDL: `source.nfdl`, file type `nfdl`
   - ADGL: `source.adgl`, file type `adgl`
3. Set the grammar path to the matching directory above (or to a built
   `*.wasm` / `src/parser.c` artifact from `tree-sitter build` / `generate`).
4. Load queries from that grammar’s `queries/` directory (`highlights`,
   `folds`, `locals` as supported by the host).

Example absolute layout from this repo:

```text
third_party/n-fdl/grammars/tree-sitter-nfdl/
  grammar.js
  queries/highlights.scm
  queries/folds.scm
  queries/locals.scm
third_party/n-fdl/grammars/tree-sitter-adgl/
  grammar.js
  queries/highlights.scm
  queries/folds.scm
  queries/locals.scm
```

## Develop / smoke-check queries

```bash
cd grammars/tree-sitter-nfdl   # or tree-sitter-adgl
tree-sitter generate
tree-sitter test
# optional: tree-sitter query queries/highlights.scm <example-file>
```

Neovim users can instead vendor the same `queries/*.scm` files under
`queries/nfdl/` / `queries/adgl/` and register parsers via
`nvim-treesitter`’s `parser_config` pointing at these grammar roots.
