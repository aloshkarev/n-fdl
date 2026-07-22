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
| `NFDL0100`–`NFDL0199` | Unused declarations (messages, `let` bindings) |
| `NFDL0200`–`NFDL0299` | Redundant or stub `validate` blocks |
| `NFDL0900`–`NFDL0909` | Engine-smoke / driver demos (e.g. `NFDL0900` empty file) |
| `ADGLS0001`–`ADGLS0099` | Unused `correlate` / graph bindings |
| `ADGLS0100`–`ADGLS0199` | Literal and type hygiene (e.g. float warnings) |
| `ADGLS0200`–`ADGLS0299` | Empty or suspicious `having` clauses |
| `ADGLS0300`–`ADGLS0399` | Absence idioms (`present`/`absent` correlate suggestions) |

## Shipped N-FDL lints (Task 16)

Registered by `LintStore::register_builtin` → `nfdl::register_nfdl_pack`
(plus engine-smoke). Checks run only when `.nfdl` source is non-empty and the
canonical Rust parser (`nfdl-syntax`) succeeds; tree-sitter is not used.

| ID | Default | Message / rule |
|----|---------|----------------|
| `NFDL0001` | warn | Protocol and message names should be CamelCase (PascalCase: leading uppercase ASCII letter, alphanumeric only) |
| `NFDL0002` | warn | Field names should be snake_case (leading lowercase, `[a-z0-9_]+`, no `__` / trailing `_`) |
| `NFDL0100` | warn | Message is never referenced by `bind` layer/source, `MessageRef` types, or state-machine transitions (single entry-point messages without a dispatch graph are exempt) |
| `NFDL0101` | warn | `let` binding is never referenced in any expression. **Wire-layout message fields are not linted** — declaration is their use; payload fields that appear only on the wire are intentional |
| `NFDL0200` | warn | Validate is a constant (`true`/`false`/int), structural tautology (`x == x`), or has an empty message string |
| `NFDL0900` | warn | Source file is empty or whitespace-only (engine-smoke; kept for driver demos) |

## Shipped ADGL lints (Task 17)

Registered by `LintStore::register_builtin` → `adgl::register_adgl_pack`.
AST-backed checks use `airpulse_dsl_syntax::parse_ruleset` (canonical Rust
parser — not tree-sitter). Float hygiene scans `.adgl` source even when parse
fails (floats are rejected by the parser / units ABI).

| ID | Default | Message / rule |
|----|---------|----------------|
| `ADGLS0001` | warn | Correlate binding is never referenced outside its own `topo`/`time` — **`present()` / `absent()` and `infer`/`emit`/`action` evidence lists count as uses** (no false positive when the binding appears only there) |
| `ADGLS0100` | warn | Float literal in `.adgl` source (outside comments/strings). Units ABI is i64 — prefer per-mille / centi / ms integer thresholds |
| `ADGLS0200` | warn | `having: count >= 1` is redundant with the omitted default (empty / no-op having). **Does not** re-emit verify `ADGL0504` (`N = 0`) / `ADGL0505` (`N > 32`) |
| `ADGLS0300` | warn | Absence-named signal (heuristic: substring `contains` match on `unanswered`, `missing`, `absent`, `without`, `incomplete`, `no_response`/`noresponse`, `absence`) in rule/predicate/field paths on a rule that has **≥1 correlate** and never uses `present(...)` / `absent(...)`. Suggests considering those correlate idioms for peer-response absence; **no IR counterfactual**. Skips zero-correlate rules (counter-gap / metric / completeness / pure Cause→Problem verdicts). Expert counter-gap / metric thresholds (`absence_ms`, `*_without_*` counters) may intentionally omit `present`/`absent` — use `allow(ADGLS0300)` when appropriate |

## Suppress / deny attributes (Wave 0)

Full grammar attributes are deferred (no AST/`#![…]` parse yet). Until then,
`ndsl-clippy` honors **file-scoped line-comment directives** with the same
surface as rustc-style attrs:

| Form | Example |
|------|---------|
| Outer attr comment | `// #[allow(NFDL0001)]` |
| Inner attr comment | `// #![deny(ADGLS0100)]` |
| Explicit ndsl prefix | `// ndsl:allow(NFDL0001)` / `// ndsl:deny(ADGLS0100)` |

Rules:

- Directives apply to the **whole file** (Wave 0). Decl-scoped attachment is
  future work once real attribute nodes exist in the parsers.
- Comma-separated IDs are accepted: `// #[allow(NFDL0001, NFDL0002)]`.
- `forbid` is an alias for `deny`; `warning` is an alias for `warn`.
- Later directives for the same ID win.
- Malformed / non-directive comments are ignored (no parse error).
- Examples and shipped diagnostics do **not** require attributes.

**Precedence** (highest first):

1. CLI `--allow` / `--deny` (`LintStore::set_level`)
2. File comment directives (`attrs::parse_file_attrs`)
3. Lint default level

Real `#[allow(...)]` tokens in the grammar may replace this comment surface in
a later wave; the ID/level vocabulary will stay the same. See
[ADR-014](../adr/ADR-014-wave0-lint-attr-directives.md).

## Implementation

- Types: `crates/ndsl-clippy` (`LintId`, `LintLevel`, `LintDiagnostic`, `LintStore`)
- Context: `LintContext` carries optional `nfdl: Option<&Protocol>` and `adgl: Option<&Ruleset>` for style packs
- Driver: `LintStore::lint_paths` / `lint_file` walks `.nfdl` / `.adgl` (directories recurse)
- Levels: `LintStore::set_level` backs `ndsl-cli lint --allow` / `--deny`
- File attrs: `attrs::parse_file_attrs` (Wave-0 comment directives) applied per file in `lint_source`
- Rendering: human via ariadne (`RenderFormat::Human`) + JSON (`--json`)
- CLI: `ndsl-cli lint [--json] [--allow ID] [--deny ID] <paths...>`
