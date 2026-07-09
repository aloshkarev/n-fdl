# airpulse_dsl-fuzz

No-panic fuzz/property harness for the ADGL parser and verifier.

## CI smoke (deterministic + bounded)

```bash
cargo test -p airpulse_dsl-fuzz
```

This runs:

- property-style parser no-panic checks over arbitrary byte inputs
- property-style parse->verify no-panic checks over arbitrary strings
- deterministic seed-corpus mutation fuzzing (10 ADGL example files, bounded iterations)
- explicit oversize-source guard check (`ADGL0102`)

## Long manual run

```bash
cargo test -p airpulse_dsl-fuzz mutation_seed_corpus_no_panic_long -- --ignored
```

Use this long run for extended no-panic confidence (for example, a 1-hour local campaign).
