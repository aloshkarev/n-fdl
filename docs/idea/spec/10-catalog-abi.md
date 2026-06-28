# ADGL Universal Catalog & Trait ABI v1

Определяет универсальный каталог сетевой диагностики (events, causes, problems,
actions, topology-функции, exclusivity, capabilities) и Rust-trait ABI
(`TopologyProvider`, `ActionSink`). Аналог N-FDL
[../../spec/10-plugin-abi.md](../../spec/10-plugin-abi.md), но вместо FFI-плагинов
— встроенный catalog + два trait-расширения. Crate-владелец `airpulse_dsl::catalog`.
Решает E1..E5, G2, C9, C10 (ADRs 005/009/010).

Каталог — compile-time контракт (верификатор, [05 §1](05-verification.md));
runtime-реестр — `TopologyProvider`/`ActionSink` traits.

## 1. Принципы

1. Каталог — единственный источник имён event/cause/problem/action/topology.
2. Все metric-paths (`rtx.segment_size`) типизируются по catalog-схеме (04 §6.1).
3. PII-поля помечены `[pii]` (C9); strict-redaction в evidence JSON.
4. Расширение каталога — v1.5 (новые events/cause); v1 — фиксированный набор ниже.

## 2. Events (12)

Каждый event: `{ fields..., .target: ScopeId(scope), .time: Int, .vlan/.path: ... }`.
`.target` — canonical scope-key для своего scope (04 §6.1).

```
tcp.retransmission_burst  { segment_size: Int, target: ScopeId(Session),
                            time, vlan: ScopeId(Vlan), path: List<ScopeId>,
                            dst_ip: Int [pii], src_ip: Int [pii] }
icmp.ptb                  { target: ScopeId(Session), time, quoted_mtu: Int,
                            path: List<ScopeId>, dst_ip: Int [pii] }       // PTB = Packet Too Big
wifi.deauth_burst         { target: ScopeId(AccessPoint), time, count: Int,
                            bssid [pii], client_macs: List<Int> [pii] }
wifi.rf_telemetry         { target: ScopeId(AccessPoint), time, rssi: Int,
                            noise: Int, channel: Int }
stp.topology_change       { target: ScopeId(Vlan), time, vlan: ScopeId(Vlan) }
dhcp.timeout              { target: ScopeId(ClientMac), time, vlan: ScopeId(Vlan),
                            client_mac [pii] }
radius.access_request     { target: ScopeId(ClientMac), time, vlan: ScopeId(Vlan) }
dot1x.eapol_start          { target: ScopeId(ClientMac), time, vlan: ScopeId(Vlan) }
port.crc_errors           { target: ScopeId(Port), time, count: Int }
port.link_flap            { target: ScopeId(Port), time, count: Int }
port.admin_state          { target: ScopeId(Port), time, state: String }   // "UP"|"DOWN"
port.oper_state           { target: ScopeId(Port), time, state: String }
```

`.target` для `icmp.ptb` = session-id (derived из quoted TCP tuple в PTB) —
это bridge к `same_session(rtx.target, ptb.target)` (E2).

## 3. Causes (8)

```
PmtudBlackhole        { target: ScopeId(Session|Vlan), confidence, evidence }
Congestion            { target: ScopeId(Session),       confidence, evidence }
TransientL2Disruption { target: ScopeId(Session|Vlan),  confidence, evidence }
PhysicalCableDamage   { target: ScopeId(Port),          confidence, evidence }
AuthServerOutage      { target: ScopeId(Vlan),          confidence, evidence }   // Vlan (roll-up из ClientMac)
RfInterference        { target: ScopeId(AccessPoint),   confidence, evidence }
UpstreamOutage        { target: ScopeId(Global),        confidence, evidence }
PhysicalLinkAbsent    { target: ScopeId(Port),          confidence, evidence }   // из Example 7
```

`target` scope-тип в скобках — допустимые (05 §4 `CauseScopeInvalid`). Каждый
cause имеет `target: ScopeId`, `time: Int` (first-infer event-time, 03 §3.3 — для
correlate time-window 03 §3.2), `confidence: Confidence` (0..100) и
`evidence: List<NodeId>`.

## 4. Problems (6 + Ambiguous)

```
XlIcmpTcpMss           { target, severity, evidence, sarif_id: "l3_pmtud_blackhole" }
CableDisconnected      { target: Port, severity, evidence, sarif_id: "ap_port_cable_disconnected" }
SpanningTreeInstability{ target: Vlan, severity, evidence, sarif_id: "l3_stp_spanning_tree" }
ClientOnboardingFailure{ target: Vlan, severity, evidence, sarif_id: "l3_dot1x_wired" }
WlanRadiusOutage       { target: Vlan, severity, evidence, sarif_id: "ap_wlan_radius_outage" }
DeviceUnreachable      { target: Global, severity, evidence, sarif_id: "ap_device_unreachable" }
AmbiguousDiagnosis     { target, severity: Medium, evidence, sarif_id: "ap_ambiguous",
                         partialFingerprints: {scope, target, causes} }   // C5/C8
```

Каждый Problem также имеет `time: Int` (emission watermark, 03 §3.4) — для
correlate time-window (03 §3.2; Example 7 `upstream.time`/`downstream.time`).
`sarif_id` — стабильный symbolic ID (C8, ADR-008); для legacy-covered диагнозов
= legacy `recommendation_id` (`l3_pmtud_blackhole`, `l3_stp_spanning_tree`,
`l3_dot1x_wired`; verified vs `airpulse://rules`); `ap_*` — новые стабильные IDs
(legacy wired-TOML не имеет wifi/AP/L2/global-эквивалентов). Parallel-run parity
на verdict level (many-to-one legacy rule ids → sarif_id, plans/migration §2).
`AmbiguousDiagnosis` — surfacing AmbiguityNode (03 §4).

## 5. Actions (5)

```
request_observation(kind: KindIdent)  { target: ScopeId | List<ScopeId>, evidence }   // List<ScopeId> = path-targeting (eBPF filter вдоль пути, Example 3: rtx.path); kind ∈ observation_kinds
run_check(kind: KindIdent)            { target: ScopeId, evidence }
suppress_symptom(problem: Ref)    { reason, evidence }   // Problem-level suppression (C7)
mark_ambiguous                    { target: ScopeId, causes }     // → AmbiguityNode (03 §4)
request_topology                  { target: ScopeId }             // topology Unknown fallback (C10)
```

`observation_kinds` (для `request_observation`): `icmp.visibility`,
`aaa.telemetry`, `wifi.rf_metrics`. `check_kinds`: `cable_loopback`,
`lldp_poll`, `stp_root_check`. Расширение — v1.5.

## 6. Topology-функции (6, C10)

```
same_session(a: ScopeId, b: ScopeId) : T3     // Session scope
same_client (a: ScopeId, b: ScopeId) : T3     // ClientMac
same_port   (a: ScopeId, b: ScopeId) : T3     // Port
same_ap     (a: ScopeId, b: ScopeId) : T3     // AccessPoint
same_vlan   (a: ScopeId, b: ScopeId) : T3     // Vlan
upstream_of (up: ScopeId, down: ScopeId) : T3 // Global; cycle-bound, max_topology_hops
```

Все возвращают `T3 = Bool | Unknown`. `Unknown` → `request_topology` (03 §3.7).
`upstream_of` — BFS с `visited`-set, `max_topology_hops` (ADR-011); цикл →
`False` (не upstream) + diagnostic (07 §6).

## 7. Exclusivity relations (C5, E4)

`mutually_exclusive` объявляется в ruleset ([02 §1](02-grammar.ebnf)) и/или в
catalog-defaults:

```
catalog.exclusivity_defaults = {
  (Congestion, PmtudBlackhole),          // оба объясняют retrans, но разные fix
  (Congestion, TransientL2Disruption),
  (PmtudBlackhole, TransientL2Disruption),
  (PhysicalCableDamage, PhysicalLinkAbsent),  // оба Port, но разные root
  (AuthServerOutage, RfInterference),    // разные plane (AAA vs RF)
}
```

Ruleset может переопределять/дополнять. AmbiguityNode синтезируется для пар в
этом множестве (03 §4).

## 8. Capabilities (`requires`, E5)

```
l3-deep    : L3 cross-protocol hints (ICMP/ARP/DHCP/DNS/STP/VLAN)
topology   : LLDP/CDP adjacency (TopologyProvider non-Unknown)
wifi-ota   : 802.11 radiotap analysis
radio-nemo : cellular radio logs (NEMO)
```

Load-time: `ruleset.requires ⊆ catalog.capabilities` (05 §6). Runtime
availability (topology Unknown) — отдельно (03 §3.7).

## 9. TopologyProvider trait ABI

```rust
pub trait TopologyProvider: Send + Sync {
    fn same_session(&self, a: ScopeId, b: ScopeId) -> T3;
    fn same_client (&self, a: ScopeId, b: ScopeId) -> T3;
    fn same_port   (&self, a: ScopeId, b: ScopeId) -> T3;
    fn same_ap     (&self, a: ScopeId, b: ScopeId) -> T3;
    fn same_vlan   (&self, a: ScopeId, b: ScopeId) -> T3;
    fn upstream_of (&self, up: ScopeId, down: ScopeId) -> T3;  // max_hops baked in impl
}
```

Реализация — указывает на AirPulse adjacency hashes (`wifi_analysis`,
`l3_cross_diagnostics`), plans/migration §3. `Send + Sync` для cross-partition
lock-free (07 §10). Нет FFI в v1.

## 10. ActionSink trait ABI

```rust
pub trait ActionSink: Send {
    fn emit(&mut self, intent: ActionIntent, mode: RunMode, wm: i64);
}
pub enum RunMode { Offline { audit: AuditLog }, Live { ebpf: EbpfController, topo: TopologyController } }
```

- `Offline`: `request_observation` → audit `ADGL3001 ActionNoOpInReplay`;
  `suppress_symptom` → graph mutation (mark superseded); `request_topology` → audit.
- `Live`: `request_observation` → `ebpf.load_filter(scope)`; `request_topology` →
  `topo.poll(scope)`; `run_check` → external enqueue; `mark_ambiguous` → graph.

Trait `Send` (не `Sync`) — sink mutable per emission; Effects упорядочены (C12).

## 11. Privacy redaction (C9, ADR-009)

```rust
pub fn redact_evidence(evidence: &[NodeId], store: &GraphStore, strict: bool) -> JsonEvidence
```

В strict режиме PII-поля (`[pii]` в catalog) в evidence JSON заменяются на
`"<redacted>"`. Внутри графа (ScopeId, индексация) PII сохраняются.
`Intent.pii` mask (06 §2.3) указывает, какие FieldPath redact — без повторного
catalog-анализа в runtime.

## 12. Контракт

1. Все event/cause/problem/action/topology refs разрешимы через catalog (05 §1).
2. PII-поля помечены; strict-redaction по `Intent.pii` mask.
3. Topology → `T3`; `Unknown` → `request_topology` (C10).
4. `upstream_of` cycle-bound; цикл → `False` + diagnostic.
5. Exclusivity — catalog-defaults + ruleset overrides; Ambiguity только для пар в множестве.
6. `sarif_id` стабильный symbolic; legacy-covered = legacy `recommendation_id`, `ap_*` новые; parallel-run verdict-level parity.
7. `TopologyProvider`/`ActionSink` — `Send`(+`Sync` для topo) Rust traits; нет FFI v1.
