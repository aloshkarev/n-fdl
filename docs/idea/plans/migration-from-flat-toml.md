# Migration Plan — flat-TOML AirPulse → ADGL

Из проспекта V1 §8, формализовано под spec/ADR. Цель: заменить плоский TOML
движок AirPulse (`airpulse://rules`) на ADGL graph engine без потери stable IDs
и без регрессии verdict-вывода.

## 1. Adapter layer

Обернуть существующие AirPulse outcomes в `EventNode`-схемы, совместимые с
catalog ([../spec/10-catalog-abi.md](../spec/10-catalog-abi.md) §2):

- `Arc<TcpSession>` → `tcp.retransmission_burst` / `tcp.*` events
  (segment_size, target=Session ScopeId via `bidir_tuple`, time, vlan, path).
- `L3Hints` (`xl_icmp_ptb`, `xl_stp_tcn`, ...) → `icmp.ptb`, `stp.topology_change`,
  ... events. `xl_*` bool-флаги → typed events с полями.
- `wifi_analysis` → `wifi.deauth_burst`, `wifi.rf_telemetry` events.
- `port.*` / `dot1x.*` / `radius.*` / `dhcp.*` — из соответствующих подсистем.

Adapter — чистая проекция (no logic); вся диагностика переносится в ADGL rules.

## 2. Parallel run

- V1 graph engine запускается **silently alongside** legacy flat-TOML.
- Оба получают один PCAP; сравнивается SARIF/JSON output.
- **Stable ID parity**: SARIF `ruleId` (= ADGL `sarif_id`) сравнивается с legacy
  на **verdict/recommendation level** via adapter mapping `legacy_rule_id →
  sarif_id` (many-to-one: несколько legacy PMTUD rules — `l3_icmp_ptb_with_loss`,
  `l3_icmp_tcp_mss_loss`, `l3_icmp_tcp_mss_rst`, `l3_icmp_tcp_blackhole_loss` —
  → один `XlIcmpTcpMss`); raw `fired_rule_id` 1:1 equality невозможна.
  `sarif_id` catalog-defaults (10 §4) = legacy `recommendation_id` для
  legacy-covered диагнозов (`l3_pmtud_blackhole`, `l3_stp_spanning_tree`,
  `l3_dot1x_wired`); `ap_*` — новые стабильные IDs (legacy wired-TOML не имеет
  wifi/AP/L2/global-эквивалентов).
- `confidence`: legacy 0..1 = ADGL 0..100 / 100 (ADR-002).
- Gate: ≥95% agreement на AirPulse corpus ([../spec/12-testing.md](../spec/12-testing.md) §1.4);
  расхождения triage-ятся (документ).

## 3. Topology trait mapping

- `TopologyProvider` trait ([../spec/07-runtime.md](../spec/07-runtime.md) §6)
  → указывает на существующие AirPulse adjacency hashes:
  - `wifi_analysis` (BSSID/AP adjacency, roaming) → `same_ap`, `same_vlan`.
  - `l3_cross_diagnostics` (L3 topology, upstream/downstream) → `upstream_of`,
    `same_session`.
- `Unknown` return (C10) — когда LLDP/CDP отсутствует; legacy не имел этого
  понятия (treated as false) — differential может показать улучшения (fewer
  false-negative) — документировать.

## 4. Hardcode deprecation

- Retire `l3_tcp_correlation.rs` (и аналогичные hard-coded correlation) — их
  логика переносится в ADGL rules (Rules 3, 8, 9).
- Удаление — после parallel-run parity ≥95% на corpus, покрывающем эти правила.
- Deprecated modules помечаются `#[deprecated]` один release, затем удаляются.

## 5. Rollback / risk

- Parallel run позволяет rollback: если ADGL output regresses, legacy остаётся
  source-of-truth до фикса.
- Конфиг-флаг `engine: "legacy" | "adgl" | "both"` (both = parallel + compare).
- Migration complete когда `both` parity ≥95% на N corpus runs, затем default
  switches to `adgl`, legacy archived.

## 6. Acceptance (link 13-roadmap M6)

- [ ] Adapter покрывает все 12 catalog events.
- [ ] Parallel-run на AirPulse corpus ≥95% stable-ID parity.
- [ ] `l3_tcp_correlation.rs` deprecated + tests pass without it.
- [ ] `confidence` 0..1 ↔ 0..100 mapping verified (ADR-002).
- [ ] Topology `Unknown` handling documented vs legacy `false`.
