# ADR-014: Wave-0 lint attribute directives (comment surface)

## Status

Accepted (Wave 0)

## Context

Task 18 needs `#[allow(lint_id)]` / `#[deny(lint_id)]` (or DSL-equivalent) so
authors can suppress or elevate `ndsl-clippy` findings without only using CLI
`--allow` / `--deny`. Adding real attribute productions to both N-FDL and ADGL
grammars (and plumbing them through trivia → AST → lint) is larger than this
wave and risks breaking existing examples.

## Decision

Ship a **pragmatic comment-directive surface** recognized by `ndsl-clippy`
before linting a file:

- `// #[allow(NFDL0001)]` / `// #[deny(ADGLS0100)]` / `// #[warn(...)]`
- `// #![…]` inner form (same file-wide effect in Wave 0)
- `// ndsl:allow(...)` / `// ndsl:deny(...)` as an explicit alias

Scope is **file-wide** only. CLI overrides still win over file directives.
Tree-sitter must not feed verify/runtime; this lives only in `ndsl-clippy`.

## Consequences

- Authors can suppress/elevate without grammar churn; examples stay attr-free.
- Decl-scoped attributes and true AST attrs remain follow-up work.
- Docs (`docs/tooling/lints.md`) own the user-facing vocabulary; this ADR
  records why comments were chosen for Wave 0.
