# Production checklist

Concise release gates for the N-FDL / ADGL workspace. Prefer these over ad-hoc local checks before tagging or integrating into AirPulse.

## CI gates (must pass)

Matching [`.github/workflows/ci.yml`](.github/workflows/ci.yml):

1. `cargo fmt --all -- --check`
2. `cargo clippy --workspace -- -D warnings`
3. `cargo test --workspace`
4. `./scripts/check_grammar_conformance.sh` (Rust ↔ tree-sitter IDE track; not used by verify/runtime)

## Planned local wrapper

`scripts/release-gate.sh` (Task 39) will run the four CI gates above in one command. Until it lands, run them manually or rely on CI.

## Tooling docs (spot-check with gates)

- Lints / `ndsl-clippy` IDs: [`docs/tooling/lints.md`](docs/tooling/lints.md)
- ADGL `include` loader: [`docs/tooling/includes.md`](docs/tooling/includes.md)
- Dual-track tree-sitter policy: [`docs/adr/ADR-013-dual-track-treesitter.md`](docs/adr/ADR-013-dual-track-treesitter.md)
- Tooling-wave notes (not M0–M6 completion): [`docs/spec/13-roadmap.md`](docs/spec/13-roadmap.md) appendix, [`docs/idea/spec/13-roadmap.md`](docs/idea/spec/13-roadmap.md) appendix

## Spec & examples (spot-check)

- N-FDL examples: [`docs/examples/`](docs/examples/)
- ADGL idea archive (historical): [`docs/idea/README.md`](docs/idea/README.md)

## Local smoke (optional)

```bash
cargo build -p nfdl-cli
cargo test -p nfdl-syntax
cargo test -p nfdl-runtime --test fsm_integration
cargo test -p airpulse_dsl-syntax
cargo test -p ndsl-clippy
```

## Dual-track reminder

Keep the recursive-descent / verify path and the tree-sitter IDE track separate. Grammar conformance checks editors; it must not feed verify or runtime.
