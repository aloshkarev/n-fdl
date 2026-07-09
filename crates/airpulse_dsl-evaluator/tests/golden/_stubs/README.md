# Golden Stub Snippets

These ADGL snippets are appended to canonical examples in `golden_pipeline.rs`
to keep runtime goldens on the parse -> verify -> evaluate pipeline.

- `08-stp-companion-decision.adgl`: companion decision for Example 08 so the
  backward-only window scenario also emits a concrete problem/SARIF record.
- `stubs.rs` remains for Example 07 legacy suppression tests that currently use
  a Rust-built emitter fixture.
