# ADGL Test Plan
Свойства привязаны к spec-контрактам.

## 1. Temporal GC profiling

- Ingest infinite looping PCAP; assert via heap profiler (например `jemalloc`
  stats / `dhat`) что memory **strictly flat** — watermark-driven RingBuffer GC
  (07 §4, 08 §6).
- Параметризация: `max_ringbuffer_events_per_scope` ∈ {256, 4096}; `MAX_LOOKBACK`
  ∈ {10s, 60s}; heap остаётся bounded в каждом случае.
- Gate: `flat-memory GC` property (12 §3).

## 2. WaitQueue correctness

- Feed `tcp.retransmission_burst` **без** `icmp.ptb`.
- Assert: engine **halts** diagnosis (PendingMatch suspended, upper =
  rtx.time + 1s); advances watermark past the 1s forward window
  (`wm > rtx.time+1s`, via a later event or end-of-stream flush 08 §3.4); then
  **deterministically** triggers the `absent`-branch action
  (`request_observation`) — no race (08 §5).
- Variant: with `icmp.ptb` arriving within window ⇒ `present` ⇒ infer +85.
- Gate: `WaitQueue correctness` property (12 §3).

## 3. Topology cycle isolation

- Mock `TopologyProvider` с circular `upstream_of` (A→B→C→A).
- Assert: graph resolves **without panic**; `upstream_of` returns `False` +
  `ADGL3006 TopologyCycleDetected`; `max_topology_hops` enforced (07 §6, ADR-010).
- Rule 8 (suppress_downstream) on cyclic topology ⇒ no infinite suppress.
- Gate: `topology cycle isolation` property (12 §3).

## 4. Privacy strict mode

- `$rtx.dst_ip` (PII, 10 §2) mapped to Scope resolves perfectly during graph
  evaluation (Session ScopeId = hash(5-tuple) includes dst_ip).
- Assert: `ProblemNode.evidence` JSON output **completely scrubbed**
  (`"<redacted>"`) when `strict=true`; raw dst_ip never appears in SARIF.
- Variant: `strict=false` ⇒ dst_ip visible (opt-in).
- Gate: `privacy strict mode` property (12 §3).

## 5. Differential vs legacy flat-TOML

- Corpus: AirPulse capture corpus (wired TCP + Wi-Fi + L3).
- Run legacy flat-TOML + ADGL parallel; compare SARIF `ruleId` + `partialFingerprints`.
- Gate: ≥95% stable-ID agreement (12 §1.4, plans/migration §2).
- Disagreements triage: документировать (legacy false-negative vs ADGL
  Unknown-handling improvement — expected, not regression).

## 6. Bipartite + provenance + commutativity (unit/property)

- Evidence rule with `emit` ⇒ AOT reject `ADGL0450` (05 §8).
- Decision rule with `infer` ⇒ AOT reject.
- Same `(rule, cause, target, window)` infer twice ⇒ second no-op (03 §3.3).
- Random order of infer applications ⇒ same final confidence (03 §3.3).
- 3 child ClientMac causes ⇒ parent Vlan = MAX, not 3× (09 §3.2).

## 7. Ambiguity synthesis

- Two exclusive causes at Probable (40-79) with Δ<15 ⇒ AmbiguityNode +
  `mark_ambiguous` ⇒ `Problem(AmbiguousDiagnosis)` in SARIF (03 §4).
- Lifecycle: one cause → Confirmed (≥80) ⇒ Ambiguity `Resolved` (append-only).
- Example: `10-ambiguity-demo.adgl`.

## 8. Late events + allowed lateness

- Offline: late event after resolved-absent ⇒ `ADGL3002 LateEvidence` audit,
  no retroactive re-apply (append-only provenance).
- Live: late event after wm ⇒ `ADGL3003 LateEventDropped` + side-output;
  with `allowed_lateness > 0` ⇒ re-fire pending (08 §4).

## 9. Fuzz

- DSL fuzz: random `.adgl` ⇒ no panic (lexer/parser/verifier).
- Bytecode fuzz: random `ProgramImage` + events ⇒ no panic/OOB in evaluator.
- Duration: 1h (M0), 24h (M6); `cargo geiger` clean.

## 10. Layout

```
tests/
├── golden/<ruleset>/{input.pcap, expected.sarif.json}
├── differential/{legacy_corpus/, adgl_corpus/, parity_report.json}
├── fuzz/{dsl_fuzz.rs, bytecode_fuzz.rs}
└── properties/{determinism.rs, gc_flat.rs, waitqueue.rs, topo_cycle.rs,
                privacy_strict.rs, bipartite.rs, commutativity.rs, provenance.rs,
                rollup_max.rs, ambiguity.rs, late_events.rs}
```

## Acceptance (link 13-roadmap)

- M0: §1 + §2 + golden PMTUD.
- M1: §3 + golden suppression.
- M2: §6 (roll-up MAX) + §7 + golden auth-outage.
- M3: §4 + §5 (≥95%).
- M4: §8 + full catalog.
- M5: live-mode ActionSink (Phase 3).
- M6: §9 24h clean + §5 full parity + migration complete.
