# N-FDL Correctness Refinements — Audit Trail

**Пространства имён:** finding-IDs `NFDL-G*`, `NFDL-P2-*`, `NFDL-P3-*`, `NFDL-P5-*`,
`NFDL-P6-*` — находки critical-analysis. Correction-IDs `C1..C10` — сквозные
исправления grammar/semantics/ADR (C9 = ADR-009, C10 = ADR-008 extension).

**Central claim (v1):** «каждый canonical example парсится per грамматикой,
все C-ID прослеживаемы через spec/ADR/examples, spec внутренне согласован;
implementation gaps явно отделены от spec contradictions».

**Метод:** декомпозиция central claim; evidentiary inventory (grammar × examples
× spec × ADR × crates); logic/bias audit; alternative hypotheses; integral verdict.

**Дата аудита:** 2026-06-30 (Passes 1–6). ADGL Pass 7 — см.
[`docs/idea/plans/correctness-refinements.md`](../idea/plans/correctness-refinements.md) §Pass 7.

---

## Pass 1 (2026-06-30): Gap analysis — C1–C10, stream/EFSM/plugin

| ID | Находка | Severity | Закрыто в |
|---|---|---|---|
| NFDL-G1 | `C10` использовался (grammar, 09, tcp.nfdl) без формального определения в ADR-002 (C1–C8 only) | spec-gap | [ADR-002 §C10](../adr/ADR-002-surface-syntax-corrections.md); [ADR-list ADR-008](../adr/ADR-list-critical-decisions.md) |
| NFDL-G2 | Grammar header: «C1–C8 only», но grammar кодирует C9 invoke + C10 bidir_tuple | wording | [02-grammar.ebnf](../spec/02-grammar.ebnf) header |
| NFDL-G3 | Cross-ref «testing §13» — секции не существует (12-testing имеет §1–4) | spec-gap | [03-semantics.md](../spec/03-semantics.md) §5; [06-ir-bytecode.md](../spec/06-ir-bytecode.md) §5.3 |
| NFDL-G4 | M1 acceptance «§10 appendix» — appendix отсутствует в 10-plugin-abi | spec-gap | [13-roadmap.md](../spec/13-roadmap.md) M1 |
| NFDL-G5 | `docs/plans/spec-correctness-blockers-2026-06.md` referenced but missing | spec-gap | этот документ + [examples/README.md](../examples/README.md) |
| NFDL-G6 | «DoS-вектор §12 плана» без именованного каталога | spec-gap | [dos-vectors.md](dos-vectors.md); [11-error-diagnostics.md](../spec/11-error-diagnostics.md) §7 |
| NFDL-G7 | 03-semantics ссылается на «§3 verification.md» вместо 05 §4 | typo | [03-semantics.md](../spec/03-semantics.md) §3.1 |
| NFDL-G8 | examples/README «declared initial state» vs grammar (first declared state) | wording | [examples/README.md](../examples/README.md); [09-efsm-sessions.md](../spec/09-efsm-sessions.md) §3 |
| NFDL-G9 | udp_dns: C9 record invoke, но `dec.name` не surfaced; field named `qname` as bytes | spec-gap | [udp_dns.nfdl](../examples/udp_dns.nfdl); [10-plugin-abi.md](../spec/10-plugin-abi.md) §7 |
| NFDL-G10 | `loop_result` type in 04 vs `Value::List` in 07 — mapping underspecified | spec-gap | [07-runtime.md](../spec/07-runtime.md) §4.1 |
| NFDL-G11 | `stream_bytes` in (T-BytesStream) but absent from τ grammar listing | spec-gap | [04-type-system.md](../spec/04-type-system.md) §1 |
| NFDL-G12 | Verifier errors in 05 без stable DiagId mapping in 11 | spec-gap | [11-error-diagnostics.md](../spec/11-error-diagnostics.md) §2.1 |
| NFDL-G13 | diameter.nfdl: `mode=stream` + unused `eof=on_close` without `bytes[EOF]` | wording | [diameter.nfdl](../examples/diameter.nfdl) comment |
| NFDL-G14 | Root protocol binding open (08 §7) — tracked to M1 | spec-gap (deferred) | [08-stream-reassembly.md](../spec/08-stream-reassembly.md) §7; [13-roadmap.md](../spec/13-roadmap.md) |
| NFDL-G15 | ADR-002 «C1-meta» label undefined | wording | [ADR-002](../adr/ADR-002-surface-syntax-corrections.md) §C1 |

---

## Pass 2 (2026-06-30): Spec ↔ Grammar ↔ Examples

| # | Дефект | Severity | Закрыто в |
|---|---|---|---|
| NFDL-P2-1 | `CarryDecl` grammar required `;`; parser + gtpu omit (optional) | blocker | [02-grammar.ebnf](../spec/02-grammar.ebnf) — `[ ";" ]` optional |
| NFDL-P2-2 | `??` in grammar/04/05, absent from 01-lexical | spec-gap | [01-lexical.md](../spec/01-lexical.md) §6, §6.1 |
| NFDL-P2-3 | `bidir`, `bidir_tuple`, meta literals not in keyword list | spec-gap | [01-lexical.md](../spec/01-lexical.md) §3.1, §3.3 |
| NFDL-P2-4 | `EOF`/`stream` slice-length tokens not documented | spec-gap | [01-lexical.md](../spec/01-lexical.md) §3.3 |
| NFDL-P2-5 | `__session(...)` in grammar, missing from builtins | spec-gap | [01-lexical.md](../spec/01-lexical.md) §3.2 |
| NFDL-P2-6 | udp_dns comment says `struct`, spec says `record` | wording | [udp_dns.nfdl](../examples/udp_dns.nfdl) |
| NFDL-P2-7 | Inconsistent C-id labels: radius C4 vs tcp C10 for session keys | wording | [radius.nfdl](../examples/radius.nfdl), [tcp.nfdl](../examples/tcp.nfdl) — unified C4/C10 |
| NFDL-P2-8 | Precedence: `??` vs `?:` only in grammar, not lexical | spec-gap | [01-lexical.md](../spec/01-lexical.md) §6.1 |
| NFDL-P2-9 | Typo «quailfied» in 03-semantics | typo | [03-semantics.md](../spec/03-semantics.md) §4.2 |
| NFDL-P2-10 | Duplicate §5.6 numbering in 04-type-system | typo | [04-type-system.md](../spec/04-type-system.md) — renumber 5.7+ |
| NFDL-P2-11 | examples/README «initial state» vs spec | wording | [examples/README.md](../examples/README.md) |

**Verdict Pass 2:** после правок — все 6 examples парсятся per grammar (carry `;`
optional); lexical completeness restored for `??`, session/meta keywords.

---

## Pass 3 (2026-06-30): Runtime / ABI / cross-cutting

| # | Дефект | Severity | Закрыто in spec | Impl note |
|---|---|---|---|---|
| NFDL-P3-001 | `LayerKind::Recursive(max_depth_ref)` in 05 vs bare `Recursive` in 06 | spec-gap | [06-ir-bytecode.md](../spec/06-ir-bytecode.md) §2.3 aligned | — |
| NFDL-P3-002 | `CarryProgressProven` vs `ProgressProven` naming | wording | [05-verification.md](../spec/05-verification.md) §5.2 | — |
| NFDL-P3-003 | Three byte forms collapsed to `ReadRest` in impl | impl-gap | documented in 07 §1 M0 status | M1+ |
| NFDL-P3-004 | Reassembly overlap: spec first-wins vs impl last-wins | **impl contradiction** | ADR-011 unchanged | fix in reassembly.rs (M4) |
| NFDL-P3-005 | `VmContinuation` undefined in code | impl-gap | 06 §5, 07 §5.3 | v1.5 |
| NFDL-P3-006 | `ProgramImage` absent | impl-gap | ADR-012, 06 §4.1 | M0+ |
| NFDL-P3-012 | Datagram underrun zero-fill vs spec `Truncated` | **impl contradiction** | 05 §10, 08 §1 | fix bytecode.rs |
| NFDL-P3-013 | Plugin subsystem absent | impl-gap (M1) | 10-plugin-abi, roadmap M1 | expected |
| NFDL-P3-019 | Crate topology: 10 vs 12 planned | wording | [07-runtime.md](../spec/07-runtime.md) §1 status banner | — |

**Verdict Pass 3:** spec internal drift (P3-001/002) closed in spec; impl
contradictions (P3-004, P3-012) tracked — not spec defects, documented as
implementation backlog in [traceability-matrix.md](traceability-matrix.md).

---

## Pass 4 (2026-06-30): Empirical verification + regression hunt

**Method:** `cargo test --workspace` (after edition fix 2024→2021 for Rust 1.83);
`cargo run -p nfdl-cli -- parse docs/examples/*.nfdl`.

| Check | Result |
|---|---|
| Workspace tests (core crates) | 64 passed (syntax, runtime, cli, verify, diag) |
| parse arp/udp_dns/gtpu/radius/tcp/diameter | all SUCCESS |
| carry semicolon optional | gtpu parses without `;` — grammar relaxed |
| spec↔impl overlap policy | **NOT VERIFIED** impl-side (P3-004) |

**Meta-урок:** empirical parse-trace обязателен — grammar strictness (P2-1) был
false positive относительно working parser.

---

## Pass 5 (2026-06-30): Verifier + ADR deep read

| # | Дефект | Severity | Закрыто в |
|---|---|---|---|
| NFDL-P5-001 | No stable DiagId registry; NFDL0412 vs NFDV01 vs NFD001 drift | spec-gap | [11-error-diagnostics.md](../spec/11-error-diagnostics.md) §2.1 |
| NFDL-P5-002 | 15+ VerificationError variants без DiagId | spec-gap | [11-error-diagnostics.md](../spec/11-error-diagnostics.md) §2.1 table |
| NFDL-P5-004 | Verifier advisory in M0 vs spec «AOT reject» | spec-gap | [05-verification.md](../spec/05-verification.md) §1 M0 subset note |
| NFDL-P5-005 | DoS table без DiagId | spec-gap | [11-error-diagnostics.md](../spec/11-error-diagnostics.md) §7; [dos-vectors.md](dos-vectors.md) |
| NFDL-P5-009 | `redefinition of binding` (C2) absent from 05 pipeline list | spec-gap | [05-verification.md](../spec/05-verification.md) §1 |
| NFDL-P5-010 | Inline ADRs lack Status/Date | wording | [ADR-list](../adr/ADR-list-critical-decisions.md) |
| NFDL-P5-011 | 08 §7 «Open Question до M0» stale | wording | [08-stream-reassembly.md](../spec/08-stream-reassembly.md) §7 → M1 |
| NFDL-P5-015 | `scan_crlf` manifest snippet missing | spec-gap | [10-plugin-abi.md](../spec/10-plugin-abi.md) §8 |

**ADR health report:**

| ADR | File | Status | Consistent |
|---|---|---|---|
| ADR-001 | inline | Accepted | yes |
| ADR-002 | detail | Accepted 2026-06-25 | yes (after C10 add) |
| ADR-003–008 | inline | Accepted | yes |
| ADR-009 | detail | Accepted 2026-06-25 | yes |
| ADR-010–012 | inline | Accepted | yes |

---

## Pass 6 (2026-06-30): Plans + testing/roadmap consistency

| # | Дефект | Закрыто в |
|---|---|---|
| NFDL-P6-1 | examples/README broken ref to missing blockers doc | [examples/README.md](../examples/README.md) → this file |
| NFDL-P6-2 | Golden count 6 protocols ↔ 12-testing §1.2 ↔ roadmap M0–M6 | verified consistent |
| NFDL-P6-3 | TShark differential ≥95% marked post-M6 | [12-testing.md](../spec/12-testing.md) §1.3 — no false claim |

---

## Интегральная оценка

- **Evidentiary strength:** strong for C1–C10 surface syntax; moderate for
  verifier/diagnostic completeness (closed in 11 §2.1).
- **Logical integrity:** no internal spec contradictions remain open (blockers closed).
- **Implementation parity:** documented separately — M0 subset explicit in 05 §1,
  07 §1; two impl contradictions (overlap, zero-fill) tracked for M4 fix.
- **Verdict:** **Strong / spec-ready** for v1 authoring; implementation catches up
  per roadmap milestones.

## Checkpoint (critical-analysis protocol)

- Contradictory evidence searched? — yes (overlap policy, verifier advisory).
- Regression from own fixes? — carry semicolon relaxed to match parser (Pass 4).
- External fact-check? — TShark parity deferred M6 (honest); ADGL legacy IDs in
  separate ADGL Pass 7.

---

## Open items (deferred, not blockers)

1. Root protocol binding (G14) — M1 ADR candidate
2. `VmContinuation` / resumable stream (P3-005) — v1.5
3. Full verifier pipeline (P5-013) — M0–M2 phased
4. Impl: first-wins reassembly (P3-004), fail-closed truncation (P3-012)
