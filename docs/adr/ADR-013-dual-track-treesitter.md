# ADR-013 — Dual-track parsing: tree-sitter (IDE) vs Rust (canon)

**Date:** 2026-07-21  
**Status:** accepted  
**Scope:** N-FDL (`.nfdl`) and ADGL (`.adgl`) in `third_party/n-fdl/`

## Context

Editor integrations need fast incremental parsing, syntax highlighting, folds,
and local symbol queries. Verify, bytecode compilation, and runtime evaluation
need a single authoritative AST with accurate spans, trivia, and semantic
errors.

Tree-sitter excels at IDE ergonomics but produces a concrete syntax tree that
can disagree with the Rust parser on edge cases, recovery, and trivia attachment.

## Decision

Adopt a **dual-track** architecture:

| Track | Technology | Consumers |
|-------|------------|-----------|
| IDE | `tree-sitter-nfdl`, `tree-sitter-adgl` grammars + query packs | Neovim, VS Code, Zed, etc. |
| Canon | Rust lexer (+ `ndsl-trivia`) → AST | `ndsl-fmt`, `ndsl-clippy`, `*-verify`, runtimes |

**Invariant:** tree-sitter output MUST NOT feed verify, bytecode, or runtime
pipelines.

## Rationale

- **Correctness:** Verify and runtime share one parser; no drift between “what
  the IDE sees” and “what ships.”
- **Trivia:** Formatters and clippy lints require leading/trailing comment and
  whitespace attachment from the Rust lexer, not tree-sitter’s lossy CST.
- **Velocity:** Grammar authors can iterate highlighting queries without
  blocking semantic work on the Rust side.
- **Conformance:** CI compares corpora so both tracks stay roughly aligned on
  supported constructs (`scripts/check_grammar_conformance` in later waves).

## Consequences

### Positive

- Clear ownership: IDE grammars vs semantic toolchain.
- `ndsl-fmt` and `ndsl-clippy` depend only on Rust AST + trivia.
- AirPulse path-deps (`airpulse_dsl-*`, `nfdl-runtime`) remain stable.

### Negative

- Two parsers to maintain; conformance CI is mandatory.
- Occasional mismatch on error-recovery paths — IDE may parse what verify
  rejects; document as expected.

## Alternatives considered

1. **Tree-sitter only** — rejected: weak trivia, no shared verify AST.
2. **Rust parser only** — rejected: poor incremental IDE experience.
3. **Tree-sitter → Rust via conversion** — rejected: duplicates recovery logic
   and reintroduces drift.

## References

- Tooling plan: `docs/superpowers/plans/2026-07-21-nfdl-adgl-tooling-runtime.md`
- Lint IDs: `docs/tooling/lints.md`
- Wave 1 grammars: `grammars/tree-sitter-nfdl`, `grammars/tree-sitter-adgl`
