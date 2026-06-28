# Example Protocols

The example `.nfdl` protocol definitions live in this directory:

- `arp.nfdl`
- `udp_dns.nfdl`
- `tcp.nfdl`
- `radius.nfdl`
- `diameter.nfdl`
- `gtpu.nfdl`

These are referenced from tests using relative paths like `include_str!("../../../../docs/examples/radius.nfdl")`.

This is the single source of truth for examples.

## Mapping: example → spec / ADR / milestone / support

`Support` reflects the **current implementation** after the Phase 2–4 fixes
(audit 2026-06, see `docs/plans/spec-correctness-blockers-2026-06.md`). `parse=ok`
means `nfdl-cli parse` returns SUCCESS with a correct AST; `run=ok` means
`nfdl-cli run <file> --hex <hex>` executes the root message end-to-end. The legacy
`nfdl-cli <file>` (no `--hex`) path feeds a hardcoded RADIUS sample to every
protocol, so `arp`/`diameter` report a constraint/limit there **by data mismatch,
not by bug** — use `run --hex` with a real packet.

| File | Milestone | Demonstrates | Spec / ADR / C-ids | Support (current) |
|---|---|---|---|---|
| `arp.nfdl` | M0 | scalars u8/u16, hex literals, `validate -> "msg"`, dependent `bytes[expr]`, `bind` | `02-grammar`; ADR-002 C1, C5 (validate→bounds), C7 (bind), C8 (`__current_offset`) | parse=ok (hex literals + validate + bind parsed); run=ok with real hex (`run --hex`, golden test in `phase4_hardening.rs`); `bind` layer is external → declaration only |
| `udp_dns.nfdl` | M1 | Ethernet→IP→UDP→DNS, `invoke("dns_decompress")`, record result, `loop` | `02-grammar`; ADR-002 C3; ADR-009 (C9) | parse=ok (3 messages); `invoke`/plugin ABI not implemented (M1) |
| `tcp.nfdl` | M4 | `bitfield{k}`, conditional options, `mode=stream`, `bytes[EOF]`, EFSM `bidir_tuple` | `02-grammar`, `08-stream-reassembly`; ADR-002 C1, C4; ADR-007 (bitfield) | parse=ok (bitfield + `meta{mode=stream}` + `bytes[EOF]` produced); run=ok (stream VM, FSM `CLOSED→SYN_SENT` on SYN); reassembly wired via `parse_and_run_stream` |
| `radius.nfdl` | M3 | `loop while`, MessageRef, `state_machine`, `bidir` key, `set`/`emit` | `02-grammar`, `09-efsm-sessions`; ADR-002 C4, C8 | parse=ok; run=end-to-end green (best-supported example) |
| `diameter.nfdl` | M5 | `loop`, `match` (tagged union), `u24`, conditional `vendor_id`, ternary, padding (modulo) | `02-grammar`, `04-type-system`; ADR-002 C6, C5 | parse=ok (match + u24 + ternary); run=ok with real hex (`match` default arm + padding exercised) |
| `gtpu.nfdl` | M2 | `carry`/`next`, recursive `bind`, `bitfield` | `02-grammar`, `03-semantics`; ADR-002 C2, C7 | parse=ok (bitfield + bind parsed); run=ok; `bind` layer is external → declaration only |

## Remaining gaps (after Phase 2–4)

Tracked in `docs/plans/spec-correctness-blockers-2026-06.md` and `PRODUCTION_CHECKLIST.md`:

- **`invoke("…")` / plugin ABI** — `udp_dns` references `invoke("dns_decompress")`; not
  implemented (M1 roadmap). The `invoke` call parses structurally; the plugin record result
  (ADR-009 / C9) is not wired.
- **Cross-protocol `bind` layering** — `bind` parsing + in-protocol layer dispatch landed
  (Phase 3.5). `arp`/`gtpu` bind to layers defined *outside* their own protocol file, which
  remain declarations (no dispatch target in-protocol). Full cross-protocol layering is v1.5.
- **`VmContinuation` / resumable parsing** — v1 runs each message to completion; `NeedMoreBytes`
  is not surfaced to callers (Phase 4.4, deferred to v1.5).
- **`__session("key")` projection** — not wired into FSM actions (Phase 4.5, deferred to v1.5).
- **Real Z3** — optional per ADR-002 C5; interval bounds analysis is the v1 verifier (v1.5/v2).
- **SARIF 2.1.0 export** — `nfdl-diag` carries structured `Diagnostic`s; serializer is v2.

### Reproducing the support table

```bash
cargo test --workspace                       # 66 passed, 0 failed
cargo run -p nfdl-cli -- parse   docs/examples/<file>.nfdl
cargo run -p nfdl-cli -- run     docs/examples/arp.nfdl      --hex "00010800060400010011223344550a0000010000000000000a000002"
cargo run -p nfdl-cli -- run     docs/examples/tcp.nfdl      --hex "04d200500000000000000000500220000000000048454c4c4f"
cargo run -p nfdl-cli -- run     docs/examples/diameter.nfdl --hex "0100001c000000000000000000000000000000000000000100000008"
cargo run -p nfdl-cli -- validate docs/examples/arp.nfdl
cargo run -p nfdl-cli -- dump     docs/examples/arp.nfdl
```

## Phase 5 finalization (audit 2026-06)

Four end-to-end correctness fixes landed while running the example suite, with
regression tests in `crates/nfdl-runtime/tests/phase5_finalization.rs`:

- **EFSM initial state** — a fresh flow now starts at the machine's declared
  `initial` state (e.g. TCP `CLOSED`), not the generic `IDLE`; the TCP
  `Connection` FSM now transitions `CLOSED → SYN_SENT` and emits `TCP_SYN_SEEN`
  on a SYN segment.
- **`MessageRef` lexical scoping** — `var_slots` is snapshot/restored around
  each `MessageRef` inlining, so a `let` binding in a recursively-inlined
  message (diameter grouped AVP) no longer leaks to a sibling `match` arm.
  Without this, the `default => data: bytes[payload_len]` arm resolved
  `payload_len` to a phantom nested slot and read 0 bytes.
- **Phantom-slot filtering** — the VM tracks which slots were written at
  runtime; the runner excludes compile-time-registered but runtime-unreached
  slots (e.g. `avps.a.grouped.inner.*` when no grouped AVP ran) from the output
  context.
- **Root detection for match-arm references** — `root_message_name` now scans
  `match` arms for `MessageRef` targets, so a message whose only sub-message
  reference is inside a `match` arm is no longer mistaken for the root.

`match` arm grammar follows `02-grammar.ebnf`: `case <expr> => { … }` and
`default => { … }` (the `case` keyword and braces are required). Recursive
`MessageRef` (grouped AVPs) is compile-time-unrolled up to `MAX_REF_DEPTH = 8`;
deeper runtime nesting is a v1.5 item (a `Call`/`Return` subroutine model would
remove the bound).
