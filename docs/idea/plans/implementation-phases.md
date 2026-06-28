# Implementation Phases — ADGL

Из проспекта V1 §9, формализовано под milestones ([../spec/13-roadmap.md](../spec/13-roadmap.md)).
Каждая phase — вертикальный срез, runnable + тестируемый.

## Phase 1 — Headless Core (Pure Rust API)

**Без парсера**. Build the runtime primitives; hand-code AST для Rule 3 (PMTUD)
и Rule 8 (suppression), чтобы доказать O(1)-ish memory bounding на PCAPs.

- `airpulse_dsl::store`: `GraphStore` (`DashMap<ScopeId, SubGraph>`), `RingBuffer`
  (time-evicting), `WaitQueue` (`BinaryHeap` по upper_bound).
- `airpulse_dsl::evaluator`: ingest→route→advance_watermark→resume→correlate→exec.
- Watermark GC (07 §4); MAX_LOOKBACK invariant (05 §3.1).
- Offline `ActionSink` (audit-log).
- Hand-coded `ProgramImage` для Rule 3 и Rule 8 (без winnow).

**Acceptance** (13-roadmap M0+M1):
- `deterministic output` + `flat-memory GC` (12 §3).
- `WaitQueue correctness`: retrans без PTB ⇒ после +1s absent-action.
- `topology cycle isolation`: circular `upstream_of` ⇒ no panic.
- Golden PMTUD + Golden suppression.

## Phase 2 — Parser & Verifier

Ввести winnow ([../spec/02-grammar.ebnf](../spec/02-grammar.ebnf)); hook
верификатор к typed metric-paths.

- `airpulse_dsl::syntax`: winnow zero-copy parser (`&str` + `Partial`).
- `airpulse_dsl::verify`: все 10 фаз (05 §1), ariadne `ADGL####` errors.
- `airpulse_dsl::ir`: `ProgramImage` lowering (06).
- `airpulse_dsl::catalog`: universal catalog (10).

**Acceptance** (M2–M4):
- 10 example `.adgl` парсятся + verify чисто (12 §1.2 golden).
- Privacy strict-redaction (M3).
- Full catalog (12 events) + late events + idle-source (M4).
- Differential vs legacy ≥95% (M3).

## Phase 3 — Active Feedback Loop

Bind `ActionNode(request_observation)` к AirPulse `live/` subsystem — dynamic
load/unload eBPF capture filters based on graph ambiguity.

- `RunMode::Live` + `EbpfController` `ActionSink` (07 §7).
- `request_observation` → `ebpf.load_filter(scope)`; unload after window.
- `request_topology` → LLDP/CDP poll.
- Ambiguity-driven: если AmbiguityNode active, load filter для разрешения.

**Acceptance** (M5):
- Live demo: PTB-absent ⇒ eBPF ICMP-filter loaded ⇒ PTB captured ⇒ hypothesis
  upgrades +85.
- `ADGL3001 ActionNoOpInReplay` в offline (regression guard).

## Cross-cutting

- `#![deny(unsafe_code)]` с Phase 1; `cargo geiger` чистый каждый milestone.
- Fuzz: Phase 1 — no-panic на hand-coded AST; Phase 2 — DSL/bytecode fuzz;
  Phase 3 — live-mode integration tests.
- Migration parallel-run (plans/migration) стартует в Phase 2, gate в Phase 3/M6.

## Risks (per phase)

- **Phase 1**: WaitQueue lifetime vs GC — ранний прототип MAX_LOOKBACK-инварианта.
- **Phase 2**: winnow `Partial` streaming + zero-copy lifetimes; verifier
  completeness (10 фаз).
- **Phase 3**: eBPF latency vs watermark interplay (08 §7); live `W` tuning.
