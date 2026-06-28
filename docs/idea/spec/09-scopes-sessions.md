# ADGL Scopes & Sessions v1

Определяет scope-partitioning, иерархию и cross-scope агрегацию, event
routing/fan-out, `target` vs `scope`, Global singleton и детерминированный
порядок. Аналог N-FDL [../../spec/09-efsm-sessions.md](../../spec/09-efsm-sessions.md),
но вместо EFSM-session-DB — partitioned GraphStore. Crate-владелец
`airpulse_dsl::store`. Решает C3/B1..B4 (ADR-003).

## 1. Архитектурное размещение

```
Event ──route──▶ partitions (ScopeId) ──▶ SubGraph + RingBuffer + WaitQueue
                                          (per partition, lock-free cross, 07)
```

Scope = partition key (C3). Внутри partition — share-nothing subgraph с Causes,
Problems, Ambiguities, Edges. Между partitions — lock-free (`DashMap`, 07 §3).

## 2. Event routing (fan-out, B4)

Событие `Evt` типа `T` несёт ключи для нескольких scope-типов (catalog, 04 §6.1):

```
tcp.retransmission_burst:
  Session key  = hash(5-tuple)
  Vlan key     = hash(vlan_id)
  Port key     = hash(egress_switch, egress_port)   // если известно
```

Routing:

```
route_scopes(evt, img) =
  { ScopeId(σ, key_σ(evt))  для σ ∈ scopes_declared_by_rules_for(evt.type) }
```

Только scope-типы, заявленные хотя бы одним правилом для `evt.type`, получают
partition. Это avoids фантомных partitions. `Global`-правила (Example 8) routed
в singleton `GLOBAL` partition. Fan-out — shallow clone `EventNode` per
partition (07 §8).

### 2.1 Route-once

Каждое событие ingested один раз; fan-out создаёт partition-local copies. Anchor
срабатывает ≤ 1 раза на (rule, event, partition) (03 §3.1, контракт).

## 3. Cross-scope агрегация (B2, C3)

### 3.1 Иерархия

```
ClientMac ⊂ Vlan ⊂ Global
Session   ⊂ Vlan          (по vlan-path события)
Port      ⊂ Global
AccessPoint ⊂ Global
```

Child ⊂ Parent означает: каждый child-target однозначно отображается в
parent-target (напр. ClientMac → Vlan по vlan-id из DHCP event).

### 3.2 Roll-up алгоритм

Когда evidence-правило в child-scope `infer Cause(K){ target: t_parent }`:

```
σ_child = rule.scope           // ClientMac
σ_parent = scope-type(t_parent) // Vlan
требуется: σ_child ⊂ σ_parent  (04 §5 иерархия; иначе TypeError, 05 §4)

при ConfidenceMutation(K, t_parent, σ_child):
  parent_sg = ScopeId(σ_parent, parent_key(t_parent))
  // commutative roll-up: parent confidence = max over children OR sum?
  // ADR-003: MAX (не sum) — avoids double-counting одинаковых сбоев
  parent_conf = max over child-sg ∈ children(parent_sg) of Causes(K, t_parent).confidence
  SubGraph[parent_sg].Causes.upsert(K, t_parent, parent_conf)
  SubGraph[parent_sg].Edges.add(RollsUp{ child → parent })
  Effects += ConfidenceMutation(K, t_parent, σ_parent)   // trigger parent decision
```

**MAX, не SUM** (ADR-003): если 3 клиента в одном Vlan страдают от
AuthServerOutage, parent-Vlan Cause confidence = max child confidence, не
3×. Иначе единый сбой раздувался бы. Дедупликация provenance — per (rule, cause,
target, window) в **child** scope; roll-up — separate edge.

### 3.3 Decision в parent scope

Decision-правило `scope: Vlan anchor c: Cause(AuthServerOutage)` (Example 2)
срабатывает на `ConfidenceMutation` в Vlan partition (triggered roll-up'ом из
ClientMac). Это решает B2: Example 5 (ClientMac) → roll-up → Example 2 (Vlan).

## 4. `target` vs `scope` (B1, формальное)

| | `scope` | `target` |
|---|---|---|
| Что | partition key (где rule исполняется) | сущность гипотезы/проблемы |
| Где задаётся | `scope:` rule-header | `target:` в infer/emit |
| Меняется | fixed per rule | выражение over bindings |
| Scope-тип | один из 6 | ScopeId любого типа ⊇ scope (ancestor-or-equal) |
| Пример | `scope: ClientMac` | `target: dhcp.vlan` (Vlan) |

`target`-scope — **равен или ancestor** `rule.scope` (`rule.scope ⊑ target.scope`).
Same-scope: `scope: Session, target: rtx.target` (Session). Roll-up:
`scope: ClientMac, target: dhcp.vlan` (Vlan — ancestor, 09 §3). Не может быть
sibling или descendant (rule-in-parent/target-in-child) —
`TypeError::ScopeTargetMismatch` (05 §4).

## 5. Deterministic ordering (C12, B4)

Внутри partition:

```
firing order      = (anchor_event.time, rule_declaration_index)
correlate matches = earliest-time-first (03 §3.2)
emission order    = (anchor_event.time, rule_declaration_index, scope_id_hash)
```

Cross-partition merge (финальный SARIF):

```
global_effect_order = sort all Effects by (event_time, rule_decl_index, scope_id_hash)
```

`scope_id_hash` — детерминированный (одинаковый hash-функции на всех запусках).
Это даёт identical SARIF для identical `(PCAP, ProgramImage, catalog)` (12 §property).

## 6. Global singleton (B3)

`Global` scope = один partition `ScopeId(GLOBAL, ())`. Особенности:

- Все Global-правила routed в один shard ⇒ точка сериализации.
- Не ломает lock-free-заявление: Global-правила редкие (cross-cutting
  suppression, Example 8); non-Global partitions остаются lock-free cross.
- `Problem(DeviceUnreachable)` в Global partition; decision `suppress_downstream`
  correlate `upstream: Problem(DeviceUnreachable)` в том же Global partition.
- `upstream_of` traversal — cycle-bound (07 §6, ADR-010).

## 7. Scope-key нормализация

`ScopeId` = `hash(ScopeType, key_components)`. Key-компоненты — canonical:

```
Session    = bidir_tuple((ip, port), (ip, port))   // canonical, mirror N-FDL C4/C10
Port       = (switch_id, port_id)
ClientMac  = client_mac
Vlan       = vlan_id
AccessPoint= bssid
Global     = ()
```

`bidir_tuple` (как N-FDL ADR-008) — IP и port сортируются атомарно, не
независимо. Это критично для req/resp корреляции в Session scope.

## 8. Cross-subsystem interaction

- **Watermark (08)**: global `AtomicI64`; pending per-scope WaitQueue.
- **GC (07 §4)**: per-scope RingBuffer; eviction по global watermark.
- **Catalog (10)**: event-схема определяет key-компоненты для каждого scope.
- **Diagnostics (11)**: routing-failures (no matching scope) → audit, не panic.

## 9. Контракт подсистемы

1. Event routed во все scope, заявленные правилами для его типа (fan-out).
2. Anchor ≤ 1 раза на (rule, event, partition).
3. `rule.scope` ⊑ `target`-scope (target — равен или ancestor); cross-scope roll-up по иерархии §3.1.
4. Roll-up = MAX (не sum); provenance-дедуп в child scope.
5. Global = singleton; не ломает lock-free для non-Global.
6. ScopeId = canonical hash; Session uses `bidir_tuple`.
7. Внутри partition — deterministic order; cross-partition merge determinистичен.
