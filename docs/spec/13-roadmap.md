# N-FDL Development Roadmap

Milestones с deliverables, acceptance criteria и рисками. Каждый milestone заканчивается вертикальным срезом, который можно запустить и протестировать.

## M0 — Vertical Slice: ARP datagram (end-to-end скелет)

**Deliverables**
- Workspace из 12 crate с `#![forbid(unsafe_code)]` (кроме nfdl-plugin).
- Lexer + parser (подмножество: message/field/validate/bytes[expr]/bind).
- Typed AST + минимальный verifier (DAG + interval-bounds).
- IR + bytecode compiler.
- Resumable VM (datagram path only).
- AST-вывод + golden harness.
- ARP.nfdl парсится end-to-end.

**Acceptance criteria**
- Golden-тест ARP проходит (input.hex → expected.json).
- Conservation-of-bytes property.
- No-panic fuzzing parser (1h).
- `cargo geiger` чистый.
- Время от `cargo run -- arp.nfdl packet.hex` до вывода AST < 50ms.

**Risks & mitigation**
- Преждевременная over-engineering IR → минимальный IR, расширяется позже.
- Lifetime-ошибки в zero-copy → strict `&'pkt` в datagram-пути.

**Duration**: 3–4 недели.

## M1 — UDP/DNS: bind + invoke + loop

**Deliverables**
- Bind-граф + dispatcher (нерекурсивный) + layer_stack.
- FFI ABI v1 + pure-плагин `dns_decompress` (с MAX_JUMPS + visited).
- `loop ... while` + `__count`/`__rem`/`__current_offset`/`__root_*`.
- `match` → tagged union.
- Plugin signature typecheck.
- UDP/DNS golden (Ethernet→IP→UDP→DNS трасса).

**Acceptance**
- Полная UDP/DNS трасса: [10-plugin-abi.md](10-plugin-abi.md) §7 + [udp_dns.nfdl](../examples/udp_dns.nfdl) + golden harness ([12-testing.md](12-testing.md) §1.2).
- Plugin-loop-guard (max-jumps) работает.
- Bounds-проверки на `length-8` (Proven где возможно).

**Risks**
- Offset-семантика (C8) — зафиксировать локальный vs root до M0-end.

## M2 — Bitfields + conditional fields + recursion guard

**Deliverables**
- `bitfield{k}` + bit-cursor + alignment-check (ADR-007).
- `field: T if cond` (Option-тип, flow-sensitive bounds).
- Recursive bind + `max_layer_depth` + payload-shrink invariant (C7).
- GTP-U datagram (с `carry`/`next`).

**Acceptance**
- GTP-U golden (chained ext-headers через carry).
- Recursion-bomb отбивается.
- Bitfield cross-byte корректен, misalignment → TypeError.

**Risks**
- Carry-семантика нова → property-тесты на loop-progress.

## M3 — EFSM + Sessions (datagram)

**Deliverables**
- FSM engine + Session DB.
- Canonical/bidir key extraction (C4, C10).
- `emit`/`set`.
- Multi-layer key (`IPv4.src`).
- RADIUS golden (request/response pairing).
- FSM liveness verifier (dead/unreachable states).

**Acceptance**
- RADIUS Access-Request/Accept корреляция работает.
- Bidir-key мапит пару в одну сессию.
- Deterministic ordering (09 §8).

**Risks**
- Key-нормализация — главный источник тонких багов → обширные directed-тесты.

## M4 — Stream + TCP reassembly + NeedMoreBytes (v1.5 core)

**Deliverables**
- `mode=stream` + reassembly subsystem (OOO/dup/overlap/first-wins).
- L4/L7 разделение.
- `VmContinuation` + yield/resume.
- `bytes::Bytes`-буферы.
- `bytes[EOF]`.
- TCP golden + FSM Connection.

**Acceptance**
- Resume-equivalence property (произвольная сегментация ≡ целое).
- Reassembly resource-limits enforced.
- TCP FSM (SYN/SYN-ACK/ESTABLISHED/FIN) работает.

**Risks** — **высший риск проекта**
- Continuation lifetime + буфер-compaction vs живые ссылки.
- Mitigation: ранний прототип continuation на M4-start, изолированные тесты.

## M5 — Diameter + stateful plugins + timers

**Deliverables**
- Diameter golden (grouped AVP, dynamic padding proven by modulo interval axiom).
- Stateful-plugin API (`open/feed/close`).
- FSM таймеры + expiration.
- `scan_crlf` reference.

**Acceptance**
- Diameter padding корректен.
- Idle-timeout эвикция.
- Timer-transitions детерминированы.

**Risks**
- Нелинейные bounds без Z3 → runtime-проверки кроме известных modulo/padding
  паттернов, покрытых interval axioms.

## M6 — Hardening: fuzzing + differential + Z3 (опц.)

**Deliverables**
- VM fuzzing (24h чисто).
- Differential vs TShark на корпусе (≥95% согласованность).
- Полный resource-limit enforcement.
- Z3-backend за feature-flag (убирает runtime-checks для доказуемых случаев).
- MIRI на FFI.

**Acceptance**
- Zero panics/OOB на N часов фаззинга.
- Differential-согласованность.
- Все DoS-векторы §12 покрыты тестами.

**Risks**
- Differential-расхождения с TShark → ручной triage + документирование.

## v2 (за рамками M6)

- QUIC/TLS 1.3 + shared secret store.
- IP fragmentation reassembly (L3 overlap).
- HPACK/QPACK stateful.
- Sharded runtime.
- JIT backend (cranelift поверх IR).
- ASN.1 BER/PER → N-FDL IR транслятор.
- Generative grammar fuzzing.
- WASM-изоляция плагинов.

## Release criteria v1

- Все M0–M6 acceptance criteria пройдены.
- 0 critical/high уязвимостей (DoS, OOB, panic).
- Документация (эта спека) + примеры.
- Performance: datagram ARP < 50µs, UDP/DNS < 200µs на типичном x86-64.