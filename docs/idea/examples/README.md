# ADGL Examples

Каноничные `.adgl` правила. 
Это single source of truth для примеров (как [../../examples/README.md](../../examples/README.md)
для N-FDL). Тесты ссылаются по относительному пути
`include_str!("../../../../docs/idea/examples/<file>.adgl")`.

## Список и маппинг

| Файл | Prospect rule | Scope | Покрывает (spec / ADR / catalog) | Legacy SARIF ID |
|---|---|---|---|---|
| `01-pmtud-blackhole.adgl` | Rule 3 | Session | 03 §3.1–3.4, 08 §5, C4 | `l3_pmtud_blackhole` |
| `02-tcp-retrans-seed.adgl` | Rule 1 | Session | 03 §3.3, C5 (mutually_exclusive) | — (seed only) |
| `03-auth-outage-impact.adgl` | Rule 2 | Vlan | 03 §3.4–3.5, C3 roll-up (09 §3) | `l3_dot1x_wired`, `ap_wlan_radius_outage` |
| `04-dhcp-missing-auth.adgl` | Rule 5 | ClientMac→Vlan | 09 §3 cross-scope, C10 Unknown | — (seed + action) |
| `05-crc-link-flap.adgl` | Rule 6 | Port | 03 §3.2, C5 | `ap_port_cable_disconnected` (complete: decision in file) |
| `06-link-absent.adgl` | Rule 7 | Port | 03 §3.7 short-circuit | — |
| `07-suppress-downstream.adgl` | Rule 8 | Global | 09 §6, C6 Problem-anchor, C10/C11 cycle | requires companion stub (`tests/golden/_stubs/`) |
| `08-stp-tcp-burst.adgl` | Rule 9 | Session | 08 §3.1 (no future window) | requires companion stub (`tests/golden/_stubs/`) |
| `09-ap-deauth-missing-rf.adgl` | Rule 10 | AccessPoint | C10 Unknown, 08 §5 | — (seed + action) |
| `10-ambiguity-demo.adgl` | new | Session | 03 §4, C5, ADR-005 | `ap_ambiguous` |


## Запуск (после реализации)

```bash
cargo test -p airpulse_dsl-syntax --test parse_examples
cargo test -p airpulse_dsl-verify
cargo test -p airpulse_dsl-evaluator --test golden_pipeline
```

## Каталог-зависимости

Все event/cause/problem/action/topology refs разрешаются через
[../spec/10-catalog-abi.md](../spec/10-catalog-abi.md).
