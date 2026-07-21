# tree-sitter-nfdl

IDE-track [tree-sitter](https://tree-sitter.github.io/) grammar for **N-FDL**.

Per [ADR-013](../../docs/adr/ADR-013-dual-track-treesitter.md), this grammar is
for editors only. It **must not** feed verify, bytecode, or runtime pipelines.

## Develop

```bash
cd grammars/tree-sitter-nfdl
tree-sitter generate
tree-sitter parse ../../docs/examples/arp.nfdl
tree-sitter test
```

Requires the `tree-sitter` CLI on `PATH`.
