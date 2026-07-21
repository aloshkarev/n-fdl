# tree-sitter-adgl

IDE-track [tree-sitter](https://tree-sitter.github.io/) grammar for **ADGL**.

Per [ADR-013](../../docs/adr/ADR-013-dual-track-treesitter.md), this grammar is
for editors only. It **must not** feed verify, bytecode, or runtime pipelines.

## Develop

```bash
cd grammars/tree-sitter-adgl
tree-sitter generate
tree-sitter parse ../../docs/idea/examples/01-pmtud-blackhole.adgl
tree-sitter test
```

Requires the `tree-sitter` CLI on `PATH`.

## Queries

IDE query packs live in `queries/`:

| File | Purpose |
|------|---------|
| `highlights.scm` | Syntax highlighting |
| `folds.scm` | Fold ruleset/evidence/decision/correlate blocks |
| `locals.scm` | Local scopes and definitions |

Editor wiring: see [`editors/vscode/readme.md`](../../editors/vscode/readme.md).
