# N-FDL / ADGL Traceability Matrix — C-ID ↔ ADR ↔ Spec ↔ Example ↔ Test

Сквозная прослеживаемость correction-IDs. **Status:** `closed` = spec+ADR aligned;
`impl-gap` = spec OK, code deferred; `adgl-only` = ADGL namespace.

## N-FDL (C1–C10)

| C-ID | ADR | Spec § | Example | Test / property | Status |
|------|-----|--------|---------|-----------------|--------|
| C1 | ADR-002 §C1 | 02 §BytesLen, 03 §3.1, 04 §5.4, 08 §1–2 | tcp (`bytes[EOF]`), gtpu (`bytes[..]`) | golden tcp; conservation-of-bytes | closed |
| C2 | ADR-002 §C2 | 02 §LoopStmt, 03 §3.5, 05 §5 | gtpu (carry/next) | phase4/5 runtime tests | closed |
| C3 | ADR-002 §C3 | 03 §4.2, 04 §5.9, 05 §7.2 | radius (`IPv4.src`), arp bind | radius golden | closed |
| C4 | ADR-002 §C4, ADR-008 | 05 §8, 09 §2.2 | radius (`bidir`) | FSM transition tests | closed |
| C5 | ADR-003 | 05 §3, 03 §3.3 | all `validate` examples | interval bounds NFDV01→NFDL0412 | closed (M0 advisory verify) |
| C6 | ADR-006 | 03 §3.4, 04 §5.5 | diameter (`match`) | phase5 MessageRef scope | closed |
| C7 | ADR-002 §C7 | 03 §6.3, 05 §7, 07 §10 | gtpu, arp (`bind`) | bind declaration-only M0 | closed (spec); impl-gap cross-protocol |
| C8 | ADR-002 §C8 | 03 §4.4, 10 §3 | udp_dns, radius | offset in loop bounds | closed |
| C9 | ADR-009 | 04 §5.8, 10 §2–7 | udp_dns (`invoke`) | plugin ABI M1 | closed (spec); impl-gap invoke |
| C10 | ADR-002 §C10, ADR-008 | 02 §SessionKey, 09 §2.2 | tcp (`bidir_tuple`) | tcp FSM key tests | closed |

## N-FDL ADR index (inline vs detail)

| ADR | Topic | Detail file | Status |
|-----|-------|-------------|--------|
| ADR-001 | Bytecode VM | inline | Accepted |
| ADR-002 | Surface C1–C10 | ADR-002-surface-syntax-corrections.md | Accepted |
| ADR-003 | Bounds backend C5 | inline | Accepted |
| ADR-004 | BufHandle | inline → 07-runtime | Accepted |
| ADR-005 | Concurrency | inline | Accepted |
| ADR-006 | Match union C6 | inline → 04 | Accepted |
| ADR-007 | Bitfield align | inline | Accepted |
| ADR-008 | Session keys C4/C10 | inline → 09 | Accepted |
| ADR-009 | Plugin record C9 | ADR-009-plugin-record-types.md | Accepted |
| ADR-010 | Plugin isolation | inline → 10 | Accepted |
| ADR-011 | Reassembly overlap | inline → 08 | Accepted |
| ADR-012 | ProgramImage serde | inline → 06 | Accepted |

## ADGL (C1–C12) — summary

Full matrix in [`docs/idea/plans/correctness-refinements.md`](../idea/plans/correctness-refinements.md)
Passes 1–6. Pass 7 (2026-06-30): no new defects; migration §6 count (12 events)
consistent with catalog §2.

| C-ID | ADR | Primary example | Status |
|------|-----|-----------------|--------|
| C1 | ADR-001 | 01-pmtud-blackhole.adgl | adgl-only closed |
| C2 | ADR-002 | confidence thresholds | adgl-only closed |
| C3 | ADR-003 | 05-clientmac-to-vlan.adgl | adgl-only closed |
| C4 | ADR-004 | WaitQueue / 08-watermarking | adgl-only closed |
| C5 | ADR-005 | 10-ambiguity-demo.adgl | adgl-only closed |
| C6 | ADR-006 | bipartite rules | adgl-only closed |
| C7 | ADR-007 | contradicts/suppress | adgl-only closed |
| C8 | ADR-008 | SARIF mapping | adgl-only closed |
| C9 | ADR-009 | privacy strict | adgl-only closed |
| C10 | ADR-010 | topology Unknown | adgl-only closed |
| C11 | ADR-011 | DoS limits | adgl-only closed |
| C12 | ADR-012 | determinism | adgl-only closed |

## Implementation gaps (spec ≠ code, tracked not fixed in spec audit)

| Finding | Spec says | Code (M0) | Milestone |
|---------|-----------|-------------|-----------|
| NFDL-P3-004 | first-wins overlap | last-wins in reassembly.rs | M4 |
| NFDL-P3-012 | Truncated on underrun | zero-fill | M0 hardening |
| NFDL-P3-013 | invoke/FFI | not wired | M1 |
| NFDL-P3-005 | VmContinuation | placeholder VM | v1.5 |
