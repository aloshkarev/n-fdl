//! Catalog event schemas.

use airpulse_dsl_types::ScopeType;

use crate::EventSchema;
use crate::FieldSchema;
use crate::FieldType;
use crate::ScopeRoute;
use crate::helpers::{event, global_target, idx, int_field, l3_session_flag_event, session_target};
use crate::wifi;

/// Returns all catalog event schemas.
pub(crate) fn all_events() -> Box<[EventSchema]> {
    [
        // --- Legacy seed events (10-catalog-abi.md §2) ---
        event(
            "tcp.retransmission_burst",
            &[
                FieldSchema {
                    name: "segment_size",
                    field_type: FieldType::Int,
                    pii: false,
                    idx: idx(0),
                },
                FieldSchema {
                    name: "target",
                    field_type: FieldType::ScopeId(ScopeType::Session),
                    pii: false,
                    idx: idx(1),
                },
                FieldSchema {
                    name: "time",
                    field_type: FieldType::Int,
                    pii: false,
                    idx: idx(2),
                },
                FieldSchema {
                    name: "vlan",
                    field_type: FieldType::ScopeId(ScopeType::Vlan),
                    pii: false,
                    idx: idx(3),
                },
                FieldSchema {
                    name: "path",
                    field_type: FieldType::ScopeIdList,
                    pii: false,
                    idx: idx(4),
                },
                FieldSchema {
                    name: "dst_ip",
                    field_type: FieldType::Int,
                    pii: true,
                    idx: idx(5),
                },
                FieldSchema {
                    name: "src_ip",
                    field_type: FieldType::Int,
                    pii: true,
                    idx: idx(6),
                },
            ],
            &[
                ScopeRoute {
                    scope: ScopeType::Session,
                    path: "target",
                },
                ScopeRoute {
                    scope: ScopeType::Vlan,
                    path: "vlan",
                },
                ScopeRoute {
                    scope: ScopeType::Vlan,
                    path: "path",
                },
            ],
        ),
        event(
            "icmp.ptb",
            &[
                session_target(),
                int_field("time", 1),
                int_field("quoted_mtu", 2),
                FieldSchema {
                    name: "path",
                    field_type: FieldType::ScopeIdList,
                    pii: false,
                    idx: idx(3),
                },
                FieldSchema {
                    name: "dst_ip",
                    field_type: FieldType::Int,
                    pii: true,
                    idx: idx(4),
                },
            ],
            &[ScopeRoute {
                scope: ScopeType::Session,
                path: "target",
            }],
        ),
        event(
            "wifi.deauth_burst",
            &[
                FieldSchema {
                    name: "target",
                    field_type: FieldType::ScopeId(ScopeType::AccessPoint),
                    pii: false,
                    idx: idx(0),
                },
                int_field("time", 1),
                int_field("count", 2),
                FieldSchema {
                    name: "bssid",
                    field_type: FieldType::Int,
                    pii: true,
                    idx: idx(3),
                },
                FieldSchema {
                    name: "client_macs",
                    field_type: FieldType::IntList,
                    pii: true,
                    idx: idx(4),
                },
                int_field("window_ms", 5),
            ],
            &[ScopeRoute {
                scope: ScopeType::AccessPoint,
                path: "target",
            }],
        ),
        event(
            "wifi.rf_telemetry",
            &[
                FieldSchema {
                    name: "target",
                    field_type: FieldType::ScopeId(ScopeType::AccessPoint),
                    pii: false,
                    idx: idx(0),
                },
                int_field("time", 1),
                int_field("rssi", 2),
                int_field("noise", 3),
                int_field("channel", 4),
            ],
            &[ScopeRoute {
                scope: ScopeType::AccessPoint,
                path: "target",
            }],
        ),
        event(
            "stp.topology_change",
            &[
                FieldSchema {
                    name: "target",
                    field_type: FieldType::ScopeId(ScopeType::Vlan),
                    pii: false,
                    idx: idx(0),
                },
                int_field("time", 1),
                FieldSchema {
                    name: "vlan",
                    field_type: FieldType::ScopeId(ScopeType::Vlan),
                    pii: false,
                    idx: idx(2),
                },
            ],
            &[ScopeRoute {
                scope: ScopeType::Vlan,
                path: "target",
            }],
        ),
        event(
            "dhcp.timeout",
            &[
                FieldSchema {
                    name: "target",
                    field_type: FieldType::ScopeId(ScopeType::ClientMac),
                    pii: false,
                    idx: idx(0),
                },
                int_field("time", 1),
                FieldSchema {
                    name: "vlan",
                    field_type: FieldType::ScopeId(ScopeType::Vlan),
                    pii: false,
                    idx: idx(2),
                },
                FieldSchema {
                    name: "client_mac",
                    field_type: FieldType::Int,
                    pii: true,
                    idx: idx(3),
                },
            ],
            &[
                ScopeRoute {
                    scope: ScopeType::ClientMac,
                    path: "target",
                },
                ScopeRoute {
                    scope: ScopeType::Vlan,
                    path: "vlan",
                },
            ],
        ),
        event(
            "radius.access_request",
            &[
                FieldSchema {
                    name: "target",
                    field_type: FieldType::ScopeId(ScopeType::ClientMac),
                    pii: false,
                    idx: idx(0),
                },
                int_field("time", 1),
                FieldSchema {
                    name: "vlan",
                    field_type: FieldType::ScopeId(ScopeType::Vlan),
                    pii: false,
                    idx: idx(2),
                },
            ],
            &[
                ScopeRoute {
                    scope: ScopeType::ClientMac,
                    path: "target",
                },
                ScopeRoute {
                    scope: ScopeType::Vlan,
                    path: "vlan",
                },
            ],
        ),
        event(
            "dot1x.eapol_start",
            &[
                FieldSchema {
                    name: "target",
                    field_type: FieldType::ScopeId(ScopeType::ClientMac),
                    pii: false,
                    idx: idx(0),
                },
                int_field("time", 1),
                FieldSchema {
                    name: "vlan",
                    field_type: FieldType::ScopeId(ScopeType::Vlan),
                    pii: false,
                    idx: idx(2),
                },
            ],
            &[
                ScopeRoute {
                    scope: ScopeType::ClientMac,
                    path: "target",
                },
                ScopeRoute {
                    scope: ScopeType::Vlan,
                    path: "vlan",
                },
            ],
        ),
        event(
            "port.crc_errors",
            &[
                FieldSchema {
                    name: "target",
                    field_type: FieldType::ScopeId(ScopeType::Port),
                    pii: false,
                    idx: idx(0),
                },
                int_field("time", 1),
                int_field("count", 2),
            ],
            &[ScopeRoute {
                scope: ScopeType::Port,
                path: "target",
            }],
        ),
        event(
            "port.link_flap",
            &[
                FieldSchema {
                    name: "target",
                    field_type: FieldType::ScopeId(ScopeType::Port),
                    pii: false,
                    idx: idx(0),
                },
                int_field("time", 1),
                int_field("count", 2),
            ],
            &[ScopeRoute {
                scope: ScopeType::Port,
                path: "target",
            }],
        ),
        event(
            "port.admin_state",
            &[
                FieldSchema {
                    name: "target",
                    field_type: FieldType::ScopeId(ScopeType::Port),
                    pii: false,
                    idx: idx(0),
                },
                int_field("time", 1),
                FieldSchema {
                    name: "state",
                    field_type: FieldType::String,
                    pii: false,
                    idx: idx(2),
                },
            ],
            &[ScopeRoute {
                scope: ScopeType::Port,
                path: "target",
            }],
        ),
        event(
            "port.oper_state",
            &[
                FieldSchema {
                    name: "target",
                    field_type: FieldType::ScopeId(ScopeType::Port),
                    pii: false,
                    idx: idx(0),
                },
                int_field("time", 1),
                FieldSchema {
                    name: "state",
                    field_type: FieldType::String,
                    pii: false,
                    idx: idx(2),
                },
            ],
            &[ScopeRoute {
                scope: ScopeType::Port,
                path: "target",
            }],
        ),
        // --- TCP session summary (legacy parity anchor) ---
        event(
            "tcp.session_summary",
            &[
                session_target(),
                int_field("loss_rate_pm", 1),
                int_field("burst_count", 2),
                int_field("loss_type", 3),
                int_field("rto_ratio_pm", 4),
                int_field("rtt_mean_ms", 5),
                int_field("rtt_p95_ms", 6),
                int_field("rtt_inflation_centi", 7),
                int_field("cwnd_rwin_ratio_centi", 8),
                int_field("rwin_median", 9),
                int_field("cwnd_median", 10),
                int_field("bdp_gap", 11),
                int_field("bufferbloat", 12),
                int_field("buffer_mean_ratio_centi", 13),
                int_field("throughput_mean_kbps", 14),
                int_field("zero_window_events", 15),
                int_field("acked_unseen_count", 16),
                int_field("checksum_bad_count", 17),
                int_field("completeness", 18),
                int_field("sack_scoreboard_retrans", 19),
                int_field("tail_loss_probe_events", 20),
                int_field("rst_count", 21),
                int_field("fast_retransmissions", 22),
                int_field("rto_count", 23),
                int_field("duration_ms", 24),
                int_field("total_bytes", 25),
                // v1.2 (additive): session end, ms — enables calculable
                // correlate windows anchored on the summary.
                int_field("time", 26),
                // v1.3 (additive, projector deferred to W2): exact legacy
                // Wi-Fi/L3 correlation gates.
                int_field("lost_packets", 27),
                int_field("packet_count", 28),
                int_field("session_is_ipv6", 29),
            ],
            &[ScopeRoute {
                scope: ScopeType::Session,
                path: "target",
            }],
        ),
        // --- L3 cross-layer flag events (timestamped, Session scope) ---
        l3_session_flag_event("icmp.unreach"),
        l3_session_flag_event("icmp.ttl_exceeded"),
        l3_session_flag_event("icmp.redirect"),
        l3_session_flag_event("icmp.tcp_mss"),
        l3_session_flag_event("icmp.tcp_blackhole"),
        l3_session_flag_event("arp.storm"),
        l3_session_flag_event("arp.dup_ip"),
        l3_session_flag_event("ndp.rs_only"),
        l3_session_flag_event("dhcp.ether_issue"),
        l3_session_flag_event("dhcp.dns_mismatch"),
        l3_session_flag_event("dhcpv6.ether_issue"),
        l3_session_flag_event("dns.onboard_anomaly"),
        l3_session_flag_event("dns.tls_name_mismatch"),
        l3_session_flag_event("tls.slow_handshake"),
        l3_session_flag_event("tls.alerts"),
        l3_session_flag_event("http.redirect_burst"),
        l3_session_flag_event("http.errors"),
        l3_session_flag_event("quic.tcp_coexist"),
        l3_session_flag_event("dot1x.wired"),
        l3_session_flag_event("stp.tcn"),
        l3_session_flag_event("stp.tcp_burst"),
        l3_session_flag_event("igmp.mld_heavy"),
        l3_session_flag_event("zeroconf.udp_burst"),
        l3_session_flag_event("vlan.qinq"),
        l3_session_flag_event("ntp.udp_heavy"),
        l3_session_flag_event("lldp.seen"),
        l3_session_flag_event("tunnel.icmp_mtu"),
        // --- Protocol summary events (Global scope) ---
        event(
            "radius.summary",
            &[
                global_target(),
                int_field("reject_rate_pm", 1),
                int_field("access_request", 2),
                int_field("retransmissions", 3),
                int_field("latency_median_ms", 4),
            ],
            &[ScopeRoute {
                scope: ScopeType::Global,
                path: "target",
            }],
        ),
        event(
            "diameter.summary",
            &[
                global_target(),
                int_field("dwr_dwa_gap", 1),
                int_field("result_errors_total", 2),
                int_field("cer", 3),
                int_field("cea", 4),
                int_field("malformed", 5),
                int_field("session_term_gap", 6),
            ],
            &[ScopeRoute {
                scope: ScopeType::Global,
                path: "target",
            }],
        ),
        event(
            "gtp.summary",
            &[
                global_target(),
                int_field("echo_request", 1),
                int_field("echo_response", 2),
                int_field("create_session_failure_rate_pm", 3),
                int_field("create_session_req", 4),
                int_field("user_only", 5),
                int_field("delete_session_req", 6),
                int_field("delete_session_resp", 7),
                int_field("modify_session_req", 8),
                int_field("modify_session_resp", 9),
            ],
            &[ScopeRoute {
                scope: ScopeType::Global,
                path: "target",
            }],
        ),
        event(
            "dhcp.summary",
            &[
                global_target(),
                int_field("nak_rate_pm", 1),
                int_field("nak", 2),
                int_field("discover_without_offer", 3),
                int_field("lease_latency_median_ms", 4),
            ],
            &[ScopeRoute {
                scope: ScopeType::Global,
                path: "target",
            }],
        ),
    ]
    .into_iter()
    .chain(wifi::wifi_events())
    .collect()
}
