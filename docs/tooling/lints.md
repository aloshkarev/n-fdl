# Lint identifier scheme

N-FDL and ADGL share the `ndsl-clippy` driver. Each lint has a stable
identifier, a default level (`allow` / `warn` / `deny`), and a human-readable
message rendered with source spans (ariadne in later waves).

## Namespace rules

| Track | Prefix | Range | Purpose |
|-------|--------|-------|---------|
| N-FDL style | `NFDL` | `NFDL0001`–`NFDL0999` | Protocol DSL style and hygiene lints |
| ADGL style | `ADGLS` | `ADGLS0001`–`ADGLS9999` | Diagnostic-graph DSL style lints (`S` = style) |

**Do not** reuse verify/compiler diagnostic codes as clippy IDs:

| Track | Prefix | Example | Owner |
|-------|--------|---------|-------|
| N-FDL verify | `NFD` | `NFD001` | `nfdl-verify`, `ndsl-diag` |
| ADGL verify | `ADGL` | `ADGL0042` | `airpulse_dsl-verify` |

The `ADGLS` prefix deliberately inserts `S` so style lints never collide with
four-digit ADGL verify codes (`ADGL####`).

## Levels

`LintLevel` mirrors rustc/clippy:

| Level | CLI / attribute | Effect |
|-------|-----------------|--------|
| `allow` | `--allow`, `#[allow(lint_id)]` | Suppress |
| `warn` | default for most style lints | Emit warning |
| `deny` | `--deny`, `#[deny(lint_id)]` | Treat as error (non-zero exit in `ndsl lint`) |

Parsing accepts case-insensitive `allow`, `warn`/`warning`, and `deny`/`forbid`.

## Reserved blocks (Wave 0)

| Block | Reserved for |
|-------|----------------|
| `NFDL0001`–`NFDL0099` | Naming conventions (CamelCase, snake_case) |
| `NFDL0100`–`NFDL0199` | Unused declarations (messages, fields) |
| `NFDL0200`–`NFDL0299` | Redundant or stub `validate` blocks |
| `NFDL0900`–`NFDL0909` | Engine-smoke / driver demos (e.g. `NFDL0900` empty file) |
| `ADGLS0001`–`ADGLS0099` | Unused `correlate` / graph bindings |
| `ADGLS0100`–`ADGLS0199` | Literal and type hygiene (e.g. float warnings) |
| `ADGLS0200`–`ADGLS0299` | Empty or suspicious `having` clauses |

## Shipped N-FDL lints (Task 16)

Registered by `LintStore::register_builtin` → `nfdl::register_nfdl_pack`
(plus engine-smoke). Checks run only when `.nfdl` source is non-empty and the
canonical Rust parser (`nfdl-syntax`) succeeds; tree-sitter is not used.

| ID | Default | Message / rule |
|----|---------|----------------|
| `NFDL0001` | warn | Protocol and message names should be CamelCase (PascalCase: leading uppercase ASCII letter, alphanumeric only) |
| `NFDL0002` | warn | Field names should be snake_case (leading lowercase, `[a-z0-9_]+`, no `__` / trailing `_`) |
| `NFDL0100` | warn | Message is never referenced by `bind` layer/source, `MessageRef` types, or state-machine transitions (single entry-point messages without a dispatch graph are exempt) |
| `NFDL0101` | warn | Field is never referenced in any expression (skipped when the protocol has no expression idents at all, to avoid pure wire-layout noise) |
| `NFDL0200` | warn | Validate is a constant (`true`/`false`/int), structural tautology (`x == x`), or has an empty message string |
| `NFDL0900` | warn | Source file is empty or whitespace-only (engine-smoke; kept for driver demos) |

ADGL style packs (`ADGLS####`) ship in Task 17.

## Implementation

- Types: `crates/ndsl-clippy` (`LintId`, `LintLevel`, `LintDiagnostic`, `LintStore`)
- Context: `LintContext` carries optional `nfdl: Option<&Protocol>` for style packs
- Driver: `LintStore::lint_paths` / `lint_file` walks `.nfdl` / `.adgl` (directories recurse)
- Levels: `LintStore::set_level` backs `ndsl-cli lint --allow` / `--deny`
- Rendering: human via ariadne (`RenderFormat::Human`) + JSON (`--json`)
- CLI: `ndsl-cli lint [--json] [--allow ID] [--deny ID] <paths...>`
