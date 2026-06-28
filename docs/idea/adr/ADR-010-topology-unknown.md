# ADR-010 — Topology Unknown (resolves C10)

- **Status:** Accepted
- **Date:** 2026-06-28
- **Related:** [ADR-list](ADR-list-critical-decisions.md), [../spec/03-semantics.md](../spec/03-semantics.md) §3.2/§3.7,
  [../spec/10-catalog-abi.md](../spec/10-catalog-abi.md) §6/§9

## Контекст

Prospect V1 §11 Risks: «TopologyProvider returns Unknown. The runtime routes
Unknown as absent, cleanly falling back to request_topology actions instead of
assuming false». Но это противоречие: «routes Unknown as absent» vs «not
assuming false». Если Unknown ≡ absent, то `present(ptb) == false` при
Unknown-topology ⇒ ложный negative (false-absent). Нужно формальная
трёхзначная логика.

## Решение

**Three-valued logic** (`T3 = Bool | Unknown`); `Unknown ≠ absent`.

### TopologyProvider (07 §6, 10 §9)
```rust
fn same_session(&self, a, b) -> T3;   // Bool | Unknown
fn upstream_of(&self, up, down) -> T3;  // cycle-bound, max_topology_hops
```

### Correlate binding (03 §3.2)
```
matches = [ c | ⟦topo⟧(rtx, c) == True ]          // Unknown не match
binding =
  |Some(matches[0])|  если |matches| >= 1
  |Unknown|           если ∃ c: ⟦topo⟧ == Unknown и нет True-match
  |Absent|            иначе
```

### If/Else (03 §3.7, Kleene)
- `present(ptb)` = `binding == Some`; `absent(ptb)` = `binding == Absent`.
- `Unknown`-binding ⇒ ни present, ни absent не истинны ⇒ `else` НЕ выполняется;
  вместо него **`action request_topology`** (auto-generated `unknown_body`,
  06 §3.1).
- Kleene: `True and Unknown = Unknown`; `False and Unknown = False`;
  `Unknown or True = True`; `Unknown or Unknown = Unknown`.

### Cycle-bound (07 §6)
- `upstream_of` — BFS с `visited`-set, `max_topology_hops` (ADR-011).
- Цикл в topology ⇒ `False` (не upstream) + `ADGL3006 TopologyCycleDetected`.

## Последствия

- `T3` тип (04 §2); `TOPO_CALL` opcode → `slot_t3` (06 §4).
- `BranchTable.unknown_body` auto-generated (06 §3.1) — верификатор гарантирует
  presence для Unknown-рисковых cond.
- Тест: `topology cycle isolation` (12 §3).

## Отклонённые альтернативы

- **Unknown ≡ absent (false)**: ложные negative при отсутствии LLDP/CDP
  (prospect противоречит сам себе в §11).
- **Unknown ≡ present (true)**: ложные positive; `present(ptb)` без topology.
- **Бесконечное ожидание (ждать topology)**: deadlock, если topology никогда не прибудет.
