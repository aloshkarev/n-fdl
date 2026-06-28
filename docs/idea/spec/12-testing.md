# ADGL Testing v1

Определяет уровни тестов, CI-gates, свойства и acceptance по milestone. Аналог
N-FDL [../../spec/12-testing.md](../../spec/12-testing.md). Свойства
cross-reference spec-контрактам (03 §8, 05 §12, 07 §11, 08 §8, 09 §9).

## 1. Уровни тестов

- **1.1 unit (per crate)**: `airpulse_dsl::{syntax, types, verify, ir, store,
  evaluator, catalog, diag}`. Каждая фаза — изолированно.
- **1.2 golden**: `tests/golden/<ruleset>/{input.pcap, expected.sarif.json}` —
  end-to-end ingest→SARIF.
- **1.3 property**: proptest/insta на инварианты (§3).
- **1.4 differential**: ADGL output vs legacy flat-TOML AirPulse на corpus
  (≥95% согласованность stable IDs, plans/test-plan §4).
- **1.5 fuzz**: DSL-фаззинг (no-panic), bytecode-фаззинг, generative.
- **1.6 integration**: `airpulse_dsl_cli` harness (если добавлен) — полный pipeline.
- **1.7 migration**: parallel-run stable-ID parity (plans/migration).

## 2. CI gates

- `cargo test -p airpulse_dsl-syntax -p airpulse_dsl-verify -p airpulse_dsl-evaluator`
- `cargo test --test golden` (insta snapshot SARIF).
- `cargo test --test differential` (vs legacy corpus).
- `cargo geiger` чистый (no unsafe).
- `cargo test --test fuzz --release` (1h no-panic, M6 — 24h).
- `cargo miri` на trait-ABI (если unsafe добавлен в v1.5 — должен быть чист).

## 3. Свойства (named, link to contract)

1. **Deterministic output** — один `(PCAP, ProgramImage, catalog)` → идентичный
   SARIF. (03 §6, 09 §5, C12). proptest: random partition scheduling ⇒ same SARIF.
2. **Flat-memory GC** — infinite loop-PCAP ⇒ heap strictly flat (watermark-driven
   RingBuffer GC, 07 §4, 08 §6). heap-profiling test.
3. **WaitQueue correctness** — feed `tcp.retransmission_burst` без `icmp.ptb`;
   assert: engine halts diagnosis, advances watermark past the 1s forward window
   (`wm > rtx.time+1s`, via a later event or end-of-stream flush 08 §3.4),
   deterministically triggers `absent`-action (08 §5, plans/test-plan §2).
4. **Topology cycle isolation** — circular `upstream_of` в mock provider ⇒
   graph resolves without panic, `False` + `ADGL3006` (07 §6, 10 §6).
5. **Privacy strict mode** — `$rtx.dst_ip` resolves in graph (ScopeId, indexing)
   but completely scrubbed from `ProblemNode.evidence` JSON when `strict=true`
   (10 §11, ADR-009).
6. **Bipartite isolation** — evidence rule with `emit` → AOT reject
   `ADGL0450`; decision with `infer` → reject (05 §8, C6).
7. **Confidence commutativity** — order of `infer` applications doesn't change
   final `Cause.confidence` (03 §3.3, ADR-002). proptest.
8. **Provenance dedup** — same `(rule, cause, target, window)` infer twice ⇒
   second is no-op (03 §3.3, 06 §8). unit + proptest.
9. **MAX_LOOKBACK invariant** — no `PendingMatch` references evicted event
   (05 §3.1, 08 §6). property: random windows + GC ⇒ never dangling.
10. **Cross-scope roll-up = MAX** — 3 child ClientMac causes ⇒ parent Vlan
    confidence = max, not 3× (09 §3.2, ADR-003). unit.
11. **Ambiguity synthesis** — two exclusive causes at Probable with Δ<15 ⇒
    AmbiguityNode + `mark_ambiguous` (03 §4, C5). unit.
12. **Late-event audit** — late event never silently dropped; `ADGL3002/3003`
    in audit (08 §4). property.

## 4. Coverage metrics

- per-crate unit coverage ≥ 80% (tarpaulin).
- golden: 10 example `.adgl` (examples/) × ≥2 PCAP fixtures each.
- differential: ≥95% stable-ID agreement vs legacy on AirPulse corpus.
- fuzz: 1h no-panic (M0), 24h (M6).

## 5. Milestone acceptance (link 13-roadmap)

- **M0**: deterministic output + flat-memory GC + golden PMTUD (Rule 3).
- **M1**: WaitQueue correctness + topology cycle isolation + golden suppression (Rule 8).
- **M2**: cross-scope roll-up + ambiguity synthesis + golden auth-outage (Rules 2/5).
- **M3**: privacy strict mode + differential vs legacy ≥95%.
- **M4**: late-event audit + allowed-lateness + full catalog (12 events).
- **M5**: live-mode ActionSink (eBPF) + idle-source.
- **M6**: 24h fuzz clean + full differential + migration parallel-run parity.

## 6. Test layout

```
tests/
├── golden/<ruleset>/{input.pcap, expected.sarif.json}
├── differential/{legacy_corpus/, adgl_corpus/, parity_report.json}
├── fuzz/{dsl_fuzz.rs, bytecode_fuzz.rs}
└── properties/{determinism.rs, gc_flat.rs, waitqueue.rs, ...}
```

## 7. Контракт

1. Все свойства §3 — executable tests, привязаны к spec-контрактам.
2. Differential ≥95% — gate миграции (plans/migration).
3. Fuzz — no-panic на любом входе (lexer, parser, evaluator).
4. `cargo geiger` чистый (no unsafe).
5. Golden SARIF — insta-snapshot, reviewed on spec change.
