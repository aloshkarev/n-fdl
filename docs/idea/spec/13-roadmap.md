# ADGL Development Roadmap v1

Milestones с deliverables, acceptance criteria и рисками. Каждый milestone —
вертикальный срез, runnable + тестируемый. Зеркалирует N-FDL
[../../spec/13-roadmap.md](../../spec/13-roadmap.md). Migration —
[plans/migration-from-flat-toml.md](../plans/migration-from-flat-toml.md);
phases — [plans/implementation-phases.md](../plans/implementation-phases.md).

## M0 — Vertical Slice: PMTUD blackhole (end-to-end скелет)

**Deliverables**
- Workspace `airpulse_dsl` (crates: syntax/types/verify/ir/store/evaluator/catalog/diag),
  `#![deny(unsafe_code)]`.
- Hand-coded AST для Rule 3 (PMTUD) и Rule 8 (suppression) — без winnow пока.
- `GraphStore` (DashMap) + `RingBuffer` + watermark-GC + `WaitQueue` (BinaryHeap).
- Evaluator: ingest→route→advance→correlate→infer/emit. Offline `ActionSink`.
- SARIF-вывод (stable `ruleId` + `partialFingerprints`, ADR-008).

**Acceptance**
- Golden PMTUD: PCAP с retrans+PTB ⇒ `Problem(XlIcmpTcpMss)` confidence≥80.
- Golden PMTUD-absent: retrans без PTB ⇒ после +1s `absent`-action + audit.
- `deterministic output` + `flat-memory GC` свойства (12 §3).
- `cargo geiger` чистый.

**Risks**
- WaitQueue lifetime vs GC ⇒ ранний прототип MAX_LOOKBACK-инварианта (05 §3.1).

**Duration**: 3–4 недели.

## M1 — Suppression + topology

**Deliverables**
- `TopologyProvider` trait (6 функций, `T3`).
- Decision на Problem-anchor (Rule 8), `suppress_symptom`, `upstream_of`
  cycle-bound.
- `Global` singleton partition.
- Golden suppression (Rule 8).

**Acceptance**
- `WaitQueue correctness` + `topology cycle isolation` (12 §3).
- `upstream_of` цикл ⇒ `False` + `ADGL3006`, no panic.

**Risks**
- Topology Unknown handling (C10) ⇒ three-valued semantics тесты.

## M2 — Cross-scope + ambiguity

**Deliverables**
- Scope hierarchy + roll-up (ClientMac→Vlan→Global, MAX не sum).
- `mutually_exclusive` + AmbiguityNode + `mark_ambiguous`.
- Golden auth-outage (Rules 2/5), tcp-retrans-seed (Rule 1).

**Acceptance**
- `cross-scope roll-up = MAX` + `ambiguity synthesis` свойства (12 §3).
- 3 child causes ⇒ parent = max, не 3×.

**Risks**
- Roll-up triggering decision re-eval — ordering determinism (C12).

## M3 — winnow parser + verifier + privacy

**Deliverables**
- winnow parser (02-grammar) — zero-copy (`&str`/`Partial`).
- `airpulse_dsl::verify` — все 12 фаз (05 §1), ariadne `ADGL####` errors.
- Privacy strict-redaction (10 §11, ADR-009).
- Differential harness vs legacy flat-TOML.

**Acceptance**
- 10 example `.adgl` парсятся + verify чисто.
- `privacy strict mode` свойство (12 §3).
- Differential ≥95% stable-ID agreement (12 §4).

**Risks**
- winnow streaming `Partial` + zero-copy lifetimes (winnow docs, см. Внешние ссылки).

## M4 — Full catalog + late events

**Deliverables**
- Все 12 events, 8 causes, 6+Ambiguous problems, 5 actions, 6 topo (10).
- Late-event policy (offline accept+audit, live drop+side-output), `allowed_lateness`.
- Idle-source watermark (08 §2.3).

**Acceptance**
- `late-event audit` свойство (12 §3).
- Все 10 examples golden pass.

**Risks**
- `allowed_lateness` re-fire vs append-only provenance — семантические тонкости.

## M5 — Live mode + ActionSink eBPF

**Deliverables**
- `RunMode::Live` + `EbpfController` ActionSink (Phase 3 active feedback).
- `request_observation` → eBPF filter load/unload по graph ambiguity.
- `request_topology` → LLDP/CDP poll.

**Acceptance**
- Live demo: PTB-absent ⇒ eBPF ICMP-filter loaded ⇒ PTB captured ⇒ hypothesis
  upgrades +85.
- `ADGL3001 ActionNoOpInReplay` в offline.

**Risks**
- eBPF latency vs watermark interplay (08 §7).

## M6 — Hardening + migration

**Deliverables**
- 24h fuzz clean (DSL + bytecode + evaluator).
- Migration parallel-run: legacy flat-TOML + ADGL side-by-side, stable-ID parity.
- Deprecation `l3_tcp_correlation.rs` (plans/migration §4).
- Full differential ≥95%.

**Acceptance**
- 0 panics/OOB на 24h фаззинга.
- Parallel-run: identical SARIF stable IDs vs legacy на corpus.
- All M0–M5 acceptance passed.

**Risks**
- Differential расхождения ⇒ triage + документ (12 §1.4).

## v2 (за рамками M6)

- User-defined catalog extension (events/cause/topology).
- ML-fused verdict (AirPulse `diagnosis_source: Fused`) — bridge confidence→ML.
- Sharded runtime (cross-process partitions).
- WASM-изоляция внешних `run_check` actions.
- Generative grammar fuzzing (ADGL-spec-aware).

## Release criteria v1

- Все M0–M6 acceptance criteria пройдены.
- 0 critical/high дефектов (panic, OOB, data loss, privacy leak).
- Документация (spec 01–13, ADR-001..012, examples, plans) + differential report.
- Performance: ingest throughput ≥ legacy flat-TOML на corpus; latency ≤
  `max_forward_window + W` (08 §7).

## Appendix — Tooling / runtime hardening (shipped on `feature/nfdl-adgl-tooling-runtime`)

ADGL milestones **M0–M6 above are unchanged**. This appendix notes ADGL-side tooling and runtime hardening shipped in the same effort as N-FDL tooling waves (mirror: [N-FDL 13-roadmap](../../spec/13-roadmap.md) appendix).

**Shipped (ADGL-focused):**

- `airpulse_dsl-syntax` recovery + parse suggestions; `include` loader
- `ndsl-clippy` ADGLS rules (incl. ADGLS0300 absence idiom on correlate)
- Docs/`///`; SARIF / diagnostics alignment
- Late-event policy, privacy strict-redaction path, ActionSink / topology plumbing
- Shared dual-track tree-sitter IDE track (not on verify/evaluator path)

**Policy:** [ADR-013 dual-track tree-sitter](../../adr/ADR-013-dual-track-treesitter.md). Gates: [PRODUCTION_CHECKLIST.md](../../../PRODUCTION_CHECKLIST.md).

## Внешние ссылки (Exa-verified)

- SARIF 2.1.0 — https://docs.oasis-open.org/sarif/sarif/v2.1.0/sarif-v2.1.0.html
  (§3.27.5 `ruleId`, §3.27.17 `partialFingerprints`, §3.49 `reportingDescriptor`).
- winnow — https://docs.rs/winnow/latest/winnow/ (zero-copy, `Partial` streaming).
- Flink watermarks — https://nightlies.apache.org/flink/flink-docs-master/docs/dev/datastream/event-time/generating_watermarks/
  (bounded-out-of-orderness, late events, idle source).
