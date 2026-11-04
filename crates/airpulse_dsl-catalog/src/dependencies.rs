//! Closed dependency-edge catalog for `airpulse.dependencies`.
//!
//! Each problem sarif is its own event type. A correlate binds the earliest
//! event of one type, so the ruleset names both sides instead of filtering
//! a shared event after the bind.

use airpulse_dsl_types::ScopeType;

use crate::helpers::{cause, event, idx, int_field, session_target};
use crate::{CauseSchema, EventSchema, FieldSchema, FieldType, ScopeRoute};

/// One closed upstream-explains-downstream pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DependencyEdgeSpec {
    /// Upstream problem sarif id.
    pub upstream: &'static str,
    /// Downstream problem sarif id.
    pub downstream: &'static str,
    /// Infer weight. A single infer sets cause confidence to this value.
    pub weight: u8,
    /// Unique cause kind for this pair.
    pub cause: &'static str,
    /// Emitted dependency problem sarif id.
    pub sarif_id: &'static str,
}

/// The 62 explanation pairs. This list is a DAG.
pub const DEPENDENCY_EDGES: &[DependencyEdgeSpec] = &[
    DependencyEdgeSpec {
        upstream: "l3_pmtud_blackhole",
        downstream: "tcp_rwin_limited",
        weight: 86,
        cause: "DepL3PmtudBlackholeExplainsTcpRwinLimited",
        sarif_id: "dep_l3_pmtud_blackhole_explains_tcp_rwin_limited",
    },
    DependencyEdgeSpec {
        upstream: "l3_pmtud_blackhole",
        downstream: "tcp_middlebox",
        weight: 86,
        cause: "DepL3PmtudBlackholeExplainsTcpMiddlebox",
        sarif_id: "dep_l3_pmtud_blackhole_explains_tcp_middlebox",
    },
    DependencyEdgeSpec {
        upstream: "l3_icmp_unreachable_path",
        downstream: "tcp_packet_loss",
        weight: 90,
        cause: "DepL3IcmpUnreachablePathExplainsTcpPacketLoss",
        sarif_id: "dep_l3_icmp_unreachable_path_explains_tcp_packet_loss",
    },
    DependencyEdgeSpec {
        upstream: "l3_icmp_unreachable_path",
        downstream: "tcp_incomplete_handshake",
        weight: 90,
        cause: "DepL3IcmpUnreachablePathExplainsTcpIncompleteHandshake",
        sarif_id: "dep_l3_icmp_unreachable_path_explains_tcp_incomplete_handshake",
    },
    DependencyEdgeSpec {
        upstream: "l3_stp_spanning_tree",
        downstream: "tcp_packet_loss",
        weight: 88,
        cause: "DepL3StpSpanningTreeExplainsTcpPacketLoss",
        sarif_id: "dep_l3_stp_spanning_tree_explains_tcp_packet_loss",
    },
    DependencyEdgeSpec {
        upstream: "l3_dns_servfail_session",
        downstream: "tcp_incomplete_handshake",
        weight: 84,
        cause: "DepL3DnsServfailSessionExplainsTcpIncompleteHandshake",
        sarif_id: "dep_l3_dns_servfail_session_explains_tcp_incomplete_handshake",
    },
    DependencyEdgeSpec {
        upstream: "l3_dhcp_dns_edge",
        downstream: "tcp_incomplete_handshake",
        weight: 82,
        cause: "DepL3DhcpDnsEdgeExplainsTcpIncompleteHandshake",
        sarif_id: "dep_l3_dhcp_dns_edge_explains_tcp_incomplete_handshake",
    },
    DependencyEdgeSpec {
        upstream: "wifi_xlayer_roaming_stall",
        downstream: "tcp_uplink_latency",
        weight: 76,
        cause: "DepWifiXlayerRoamingStallExplainsTcpUplinkLatency",
        sarif_id: "dep_wifi_xlayer_roaming_stall_explains_tcp_uplink_latency",
    },
    DependencyEdgeSpec {
        upstream: "wifi_xlayer_retries_loss",
        downstream: "tcp_packet_loss",
        weight: 78,
        cause: "DepWifiXlayerRetriesLossExplainsTcpPacketLoss",
        sarif_id: "dep_wifi_xlayer_retries_loss_explains_tcp_packet_loss",
    },
    DependencyEdgeSpec {
        upstream: "wifi_xlayer_retries_loss",
        downstream: "tcp_loss_sack",
        weight: 78,
        cause: "DepWifiXlayerRetriesLossExplainsTcpLossSack",
        sarif_id: "dep_wifi_xlayer_retries_loss_explains_tcp_loss_sack",
    },
    DependencyEdgeSpec {
        upstream: "wifi_xlayer_deauth_disconnect",
        downstream: "tcp_incomplete_handshake",
        weight: 80,
        cause: "DepWifiXlayerDeauthDisconnectExplainsTcpIncompleteHandshake",
        sarif_id: "dep_wifi_xlayer_deauth_disconnect_explains_tcp_incomplete_handshake",
    },
    DependencyEdgeSpec {
        upstream: "wifi_xlayer_deauth_disconnect",
        downstream: "tcp_packet_loss",
        weight: 80,
        cause: "DepWifiXlayerDeauthDisconnectExplainsTcpPacketLoss",
        sarif_id: "dep_wifi_xlayer_deauth_disconnect_explains_tcp_packet_loss",
    },
    DependencyEdgeSpec {
        upstream: "wifi_xlayer_sleep_rto",
        downstream: "tcp_packet_loss",
        weight: 78,
        cause: "DepWifiXlayerSleepRtoExplainsTcpPacketLoss",
        sarif_id: "dep_wifi_xlayer_sleep_rto_explains_tcp_packet_loss",
    },
    DependencyEdgeSpec {
        upstream: "cellular_xlayer_handover_rtt",
        downstream: "tcp_uplink_latency",
        weight: 68,
        cause: "DepCellularXlayerHandoverRttExplainsTcpUplinkLatency",
        sarif_id: "dep_cellular_xlayer_handover_rtt_explains_tcp_uplink_latency",
    },
    DependencyEdgeSpec {
        upstream: "cellular_xlayer_low_cqi_loss",
        downstream: "tcp_packet_loss",
        weight: 72,
        cause: "DepCellularXlayerLowCqiLossExplainsTcpPacketLoss",
        sarif_id: "dep_cellular_xlayer_low_cqi_loss_explains_tcp_packet_loss",
    },
    DependencyEdgeSpec {
        upstream: "cellular_xlayer_low_cqi_loss",
        downstream: "tcp_radio_channel",
        weight: 72,
        cause: "DepCellularXlayerLowCqiLossExplainsTcpRadioChannel",
        sarif_id: "dep_cellular_xlayer_low_cqi_loss_explains_tcp_radio_channel",
    },
    DependencyEdgeSpec {
        upstream: "cellular_xlayer_rsrp_path",
        downstream: "tcp_radio_channel",
        weight: 70,
        cause: "DepCellularXlayerRsrpPathExplainsTcpRadioChannel",
        sarif_id: "dep_cellular_xlayer_rsrp_path_explains_tcp_radio_channel",
    },
    DependencyEdgeSpec {
        upstream: "wmg_gtpc_csresp_cause_not_accepted",
        downstream: "tcp_packet_loss",
        weight: 92,
        cause: "DepWmgGtpcCsrespCauseNotAcceptedExplainsTcpPacketLoss",
        sarif_id: "dep_wmg_gtpc_csresp_cause_not_accepted_explains_tcp_packet_loss",
    },
    DependencyEdgeSpec {
        upstream: "wmg_gtpc_csresp_cause_not_accepted",
        downstream: "tcp_incomplete_handshake",
        weight: 92,
        cause: "DepWmgGtpcCsrespCauseNotAcceptedExplainsTcpIncompleteHandshake",
        sarif_id: "dep_wmg_gtpc_csresp_cause_not_accepted_explains_tcp_incomplete_handshake",
    },
    DependencyEdgeSpec {
        upstream: "wmg_gtpc_csresp_cause_not_accepted",
        downstream: "tcp_uplink_latency",
        weight: 92,
        cause: "DepWmgGtpcCsrespCauseNotAcceptedExplainsTcpUplinkLatency",
        sarif_id: "dep_wmg_gtpc_csresp_cause_not_accepted_explains_tcp_uplink_latency",
    },
    DependencyEdgeSpec {
        upstream: "wmg_sta_der_unanswered",
        downstream: "tcp_packet_loss",
        weight: 92,
        cause: "DepWmgStaDerUnansweredExplainsTcpPacketLoss",
        sarif_id: "dep_wmg_sta_der_unanswered_explains_tcp_packet_loss",
    },
    DependencyEdgeSpec {
        upstream: "wmg_sta_der_unanswered",
        downstream: "tcp_incomplete_handshake",
        weight: 92,
        cause: "DepWmgStaDerUnansweredExplainsTcpIncompleteHandshake",
        sarif_id: "dep_wmg_sta_der_unanswered_explains_tcp_incomplete_handshake",
    },
    DependencyEdgeSpec {
        upstream: "wmg_sta_der_unanswered",
        downstream: "tcp_uplink_latency",
        weight: 92,
        cause: "DepWmgStaDerUnansweredExplainsTcpUplinkLatency",
        sarif_id: "dep_wmg_sta_der_unanswered_explains_tcp_uplink_latency",
    },
    DependencyEdgeSpec {
        upstream: "wmg_sta_dea_result_error",
        downstream: "tcp_packet_loss",
        weight: 92,
        cause: "DepWmgStaDeaResultErrorExplainsTcpPacketLoss",
        sarif_id: "dep_wmg_sta_dea_result_error_explains_tcp_packet_loss",
    },
    DependencyEdgeSpec {
        upstream: "wmg_sta_dea_result_error",
        downstream: "tcp_incomplete_handshake",
        weight: 92,
        cause: "DepWmgStaDeaResultErrorExplainsTcpIncompleteHandshake",
        sarif_id: "dep_wmg_sta_dea_result_error_explains_tcp_incomplete_handshake",
    },
    DependencyEdgeSpec {
        upstream: "wmg_sta_dea_result_error",
        downstream: "tcp_uplink_latency",
        weight: 92,
        cause: "DepWmgStaDeaResultErrorExplainsTcpUplinkLatency",
        sarif_id: "dep_wmg_sta_dea_result_error_explains_tcp_uplink_latency",
    },
    DependencyEdgeSpec {
        upstream: "wmg_eap_aka_sync_failure",
        downstream: "tcp_packet_loss",
        weight: 92,
        cause: "DepWmgEapAkaSyncFailureExplainsTcpPacketLoss",
        sarif_id: "dep_wmg_eap_aka_sync_failure_explains_tcp_packet_loss",
    },
    DependencyEdgeSpec {
        upstream: "wmg_eap_aka_sync_failure",
        downstream: "tcp_incomplete_handshake",
        weight: 92,
        cause: "DepWmgEapAkaSyncFailureExplainsTcpIncompleteHandshake",
        sarif_id: "dep_wmg_eap_aka_sync_failure_explains_tcp_incomplete_handshake",
    },
    DependencyEdgeSpec {
        upstream: "wmg_eap_aka_sync_failure",
        downstream: "tcp_uplink_latency",
        weight: 92,
        cause: "DepWmgEapAkaSyncFailureExplainsTcpUplinkLatency",
        sarif_id: "dep_wmg_eap_aka_sync_failure_explains_tcp_uplink_latency",
    },
    DependencyEdgeSpec {
        upstream: "wmg_radius_accept_without_gtpc",
        downstream: "tcp_packet_loss",
        weight: 92,
        cause: "DepWmgRadiusAcceptWithoutGtpcExplainsTcpPacketLoss",
        sarif_id: "dep_wmg_radius_accept_without_gtpc_explains_tcp_packet_loss",
    },
    DependencyEdgeSpec {
        upstream: "wmg_radius_accept_without_gtpc",
        downstream: "tcp_incomplete_handshake",
        weight: 92,
        cause: "DepWmgRadiusAcceptWithoutGtpcExplainsTcpIncompleteHandshake",
        sarif_id: "dep_wmg_radius_accept_without_gtpc_explains_tcp_incomplete_handshake",
    },
    DependencyEdgeSpec {
        upstream: "wmg_radius_accept_without_gtpc",
        downstream: "tcp_uplink_latency",
        weight: 92,
        cause: "DepWmgRadiusAcceptWithoutGtpcExplainsTcpUplinkLatency",
        sarif_id: "dep_wmg_radius_accept_without_gtpc_explains_tcp_uplink_latency",
    },
    DependencyEdgeSpec {
        upstream: "wmg_gtpc_ok_without_gtpu",
        downstream: "tcp_packet_loss",
        weight: 92,
        cause: "DepWmgGtpcOkWithoutGtpuExplainsTcpPacketLoss",
        sarif_id: "dep_wmg_gtpc_ok_without_gtpu_explains_tcp_packet_loss",
    },
    DependencyEdgeSpec {
        upstream: "wmg_gtpc_ok_without_gtpu",
        downstream: "tcp_incomplete_handshake",
        weight: 92,
        cause: "DepWmgGtpcOkWithoutGtpuExplainsTcpIncompleteHandshake",
        sarif_id: "dep_wmg_gtpc_ok_without_gtpu_explains_tcp_incomplete_handshake",
    },
    DependencyEdgeSpec {
        upstream: "wmg_gtpc_ok_without_gtpu",
        downstream: "tcp_uplink_latency",
        weight: 92,
        cause: "DepWmgGtpcOkWithoutGtpuExplainsTcpUplinkLatency",
        sarif_id: "dep_wmg_gtpc_ok_without_gtpu_explains_tcp_uplink_latency",
    },
    DependencyEdgeSpec {
        upstream: "wmg_gtpu_error_indication",
        downstream: "tcp_packet_loss",
        weight: 92,
        cause: "DepWmgGtpuErrorIndicationExplainsTcpPacketLoss",
        sarif_id: "dep_wmg_gtpu_error_indication_explains_tcp_packet_loss",
    },
    DependencyEdgeSpec {
        upstream: "wmg_gtpu_error_indication",
        downstream: "tcp_incomplete_handshake",
        weight: 92,
        cause: "DepWmgGtpuErrorIndicationExplainsTcpIncompleteHandshake",
        sarif_id: "dep_wmg_gtpu_error_indication_explains_tcp_incomplete_handshake",
    },
    DependencyEdgeSpec {
        upstream: "wmg_gtpu_error_indication",
        downstream: "tcp_uplink_latency",
        weight: 92,
        cause: "DepWmgGtpuErrorIndicationExplainsTcpUplinkLatency",
        sarif_id: "dep_wmg_gtpu_error_indication_explains_tcp_uplink_latency",
    },
    DependencyEdgeSpec {
        upstream: "wmg_inner_rtt_inflation_clean_outer",
        downstream: "tcp_packet_loss",
        weight: 92,
        cause: "DepWmgInnerRttInflationCleanOuterExplainsTcpPacketLoss",
        sarif_id: "dep_wmg_inner_rtt_inflation_clean_outer_explains_tcp_packet_loss",
    },
    DependencyEdgeSpec {
        upstream: "wmg_inner_rtt_inflation_clean_outer",
        downstream: "tcp_incomplete_handshake",
        weight: 92,
        cause: "DepWmgInnerRttInflationCleanOuterExplainsTcpIncompleteHandshake",
        sarif_id: "dep_wmg_inner_rtt_inflation_clean_outer_explains_tcp_incomplete_handshake",
    },
    DependencyEdgeSpec {
        upstream: "wmg_inner_rtt_inflation_clean_outer",
        downstream: "tcp_uplink_latency",
        weight: 92,
        cause: "DepWmgInnerRttInflationCleanOuterExplainsTcpUplinkLatency",
        sarif_id: "dep_wmg_inner_rtt_inflation_clean_outer_explains_tcp_uplink_latency",
    },
    DependencyEdgeSpec {
        upstream: "wmg_gtpu_first_byte_slow",
        downstream: "tcp_packet_loss",
        weight: 92,
        cause: "DepWmgGtpuFirstByteSlowExplainsTcpPacketLoss",
        sarif_id: "dep_wmg_gtpu_first_byte_slow_explains_tcp_packet_loss",
    },
    DependencyEdgeSpec {
        upstream: "wmg_gtpu_first_byte_slow",
        downstream: "tcp_incomplete_handshake",
        weight: 92,
        cause: "DepWmgGtpuFirstByteSlowExplainsTcpIncompleteHandshake",
        sarif_id: "dep_wmg_gtpu_first_byte_slow_explains_tcp_incomplete_handshake",
    },
    DependencyEdgeSpec {
        upstream: "wmg_gtpu_first_byte_slow",
        downstream: "tcp_uplink_latency",
        weight: 92,
        cause: "DepWmgGtpuFirstByteSlowExplainsTcpUplinkLatency",
        sarif_id: "dep_wmg_gtpu_first_byte_slow_explains_tcp_uplink_latency",
    },
    DependencyEdgeSpec {
        upstream: "wmg_dhcp_missing_after_attach",
        downstream: "tcp_packet_loss",
        weight: 92,
        cause: "DepWmgDhcpMissingAfterAttachExplainsTcpPacketLoss",
        sarif_id: "dep_wmg_dhcp_missing_after_attach_explains_tcp_packet_loss",
    },
    DependencyEdgeSpec {
        upstream: "wmg_dhcp_missing_after_attach",
        downstream: "tcp_incomplete_handshake",
        weight: 92,
        cause: "DepWmgDhcpMissingAfterAttachExplainsTcpIncompleteHandshake",
        sarif_id: "dep_wmg_dhcp_missing_after_attach_explains_tcp_incomplete_handshake",
    },
    DependencyEdgeSpec {
        upstream: "wmg_dhcp_missing_after_attach",
        downstream: "tcp_uplink_latency",
        weight: 92,
        cause: "DepWmgDhcpMissingAfterAttachExplainsTcpUplinkLatency",
        sarif_id: "dep_wmg_dhcp_missing_after_attach_explains_tcp_uplink_latency",
    },
    DependencyEdgeSpec {
        upstream: "wmg_roam_without_rule_refresh",
        downstream: "tcp_packet_loss",
        weight: 92,
        cause: "DepWmgRoamWithoutRuleRefreshExplainsTcpPacketLoss",
        sarif_id: "dep_wmg_roam_without_rule_refresh_explains_tcp_packet_loss",
    },
    DependencyEdgeSpec {
        upstream: "wmg_roam_without_rule_refresh",
        downstream: "tcp_incomplete_handshake",
        weight: 92,
        cause: "DepWmgRoamWithoutRuleRefreshExplainsTcpIncompleteHandshake",
        sarif_id: "dep_wmg_roam_without_rule_refresh_explains_tcp_incomplete_handshake",
    },
    DependencyEdgeSpec {
        upstream: "wmg_roam_without_rule_refresh",
        downstream: "tcp_uplink_latency",
        weight: 92,
        cause: "DepWmgRoamWithoutRuleRefreshExplainsTcpUplinkLatency",
        sarif_id: "dep_wmg_roam_without_rule_refresh_explains_tcp_uplink_latency",
    },
    DependencyEdgeSpec {
        upstream: "wmg_radius_request_unanswered",
        downstream: "tcp_packet_loss",
        weight: 92,
        cause: "DepWmgRadiusRequestUnansweredExplainsTcpPacketLoss",
        sarif_id: "dep_wmg_radius_request_unanswered_explains_tcp_packet_loss",
    },
    DependencyEdgeSpec {
        upstream: "wmg_radius_request_unanswered",
        downstream: "tcp_incomplete_handshake",
        weight: 92,
        cause: "DepWmgRadiusRequestUnansweredExplainsTcpIncompleteHandshake",
        sarif_id: "dep_wmg_radius_request_unanswered_explains_tcp_incomplete_handshake",
    },
    DependencyEdgeSpec {
        upstream: "wmg_radius_request_unanswered",
        downstream: "tcp_uplink_latency",
        weight: 92,
        cause: "DepWmgRadiusRequestUnansweredExplainsTcpUplinkLatency",
        sarif_id: "dep_wmg_radius_request_unanswered_explains_tcp_uplink_latency",
    },
    DependencyEdgeSpec {
        upstream: "wmg_gtpc_csr_unanswered",
        downstream: "tcp_packet_loss",
        weight: 92,
        cause: "DepWmgGtpcCsrUnansweredExplainsTcpPacketLoss",
        sarif_id: "dep_wmg_gtpc_csr_unanswered_explains_tcp_packet_loss",
    },
    DependencyEdgeSpec {
        upstream: "wmg_gtpc_csr_unanswered",
        downstream: "tcp_incomplete_handshake",
        weight: 92,
        cause: "DepWmgGtpcCsrUnansweredExplainsTcpIncompleteHandshake",
        sarif_id: "dep_wmg_gtpc_csr_unanswered_explains_tcp_incomplete_handshake",
    },
    DependencyEdgeSpec {
        upstream: "wmg_gtpc_csr_unanswered",
        downstream: "tcp_uplink_latency",
        weight: 92,
        cause: "DepWmgGtpcCsrUnansweredExplainsTcpUplinkLatency",
        sarif_id: "dep_wmg_gtpc_csr_unanswered_explains_tcp_uplink_latency",
    },
    DependencyEdgeSpec {
        upstream: "wmg_gy_ccr_unanswered",
        downstream: "tcp_packet_loss",
        weight: 92,
        cause: "DepWmgGyCcrUnansweredExplainsTcpPacketLoss",
        sarif_id: "dep_wmg_gy_ccr_unanswered_explains_tcp_packet_loss",
    },
    DependencyEdgeSpec {
        upstream: "wmg_gy_ccr_unanswered",
        downstream: "tcp_incomplete_handshake",
        weight: 92,
        cause: "DepWmgGyCcrUnansweredExplainsTcpIncompleteHandshake",
        sarif_id: "dep_wmg_gy_ccr_unanswered_explains_tcp_incomplete_handshake",
    },
    DependencyEdgeSpec {
        upstream: "wmg_gy_ccr_unanswered",
        downstream: "tcp_uplink_latency",
        weight: 92,
        cause: "DepWmgGyCcrUnansweredExplainsTcpUplinkLatency",
        sarif_id: "dep_wmg_gy_ccr_unanswered_explains_tcp_uplink_latency",
    },
    DependencyEdgeSpec {
        upstream: "wmg_gy_cca_result_error",
        downstream: "tcp_packet_loss",
        weight: 92,
        cause: "DepWmgGyCcaResultErrorExplainsTcpPacketLoss",
        sarif_id: "dep_wmg_gy_cca_result_error_explains_tcp_packet_loss",
    },
    DependencyEdgeSpec {
        upstream: "wmg_gy_cca_result_error",
        downstream: "tcp_incomplete_handshake",
        weight: 92,
        cause: "DepWmgGyCcaResultErrorExplainsTcpIncompleteHandshake",
        sarif_id: "dep_wmg_gy_cca_result_error_explains_tcp_incomplete_handshake",
    },
    DependencyEdgeSpec {
        upstream: "wmg_gy_cca_result_error",
        downstream: "tcp_uplink_latency",
        weight: 92,
        cause: "DepWmgGyCcaResultErrorExplainsTcpUplinkLatency",
        sarif_id: "dep_wmg_gy_cca_result_error_explains_tcp_uplink_latency",
    },
];

/// Problem sarif id and the event type projected for it.
pub const DEPENDENCY_PROBLEM_EVENTS: &[(&str, &str)] = &[
    (
        "cellular_xlayer_handover_rtt",
        "diag.problem.cellular_xlayer_handover_rtt",
    ),
    (
        "cellular_xlayer_low_cqi_loss",
        "diag.problem.cellular_xlayer_low_cqi_loss",
    ),
    (
        "cellular_xlayer_rsrp_path",
        "diag.problem.cellular_xlayer_rsrp_path",
    ),
    ("l3_dhcp_dns_edge", "diag.problem.l3_dhcp_dns_edge"),
    (
        "l3_dns_servfail_session",
        "diag.problem.l3_dns_servfail_session",
    ),
    (
        "l3_icmp_unreachable_path",
        "diag.problem.l3_icmp_unreachable_path",
    ),
    ("l3_pmtud_blackhole", "diag.problem.l3_pmtud_blackhole"),
    ("l3_stp_spanning_tree", "diag.problem.l3_stp_spanning_tree"),
    (
        "tcp_incomplete_handshake",
        "diag.problem.tcp_incomplete_handshake",
    ),
    ("tcp_loss_sack", "diag.problem.tcp_loss_sack"),
    ("tcp_middlebox", "diag.problem.tcp_middlebox"),
    ("tcp_packet_loss", "diag.problem.tcp_packet_loss"),
    ("tcp_radio_channel", "diag.problem.tcp_radio_channel"),
    ("tcp_rwin_limited", "diag.problem.tcp_rwin_limited"),
    ("tcp_uplink_latency", "diag.problem.tcp_uplink_latency"),
    (
        "wifi_xlayer_deauth_disconnect",
        "diag.problem.wifi_xlayer_deauth_disconnect",
    ),
    (
        "wifi_xlayer_retries_loss",
        "diag.problem.wifi_xlayer_retries_loss",
    ),
    (
        "wifi_xlayer_roaming_stall",
        "diag.problem.wifi_xlayer_roaming_stall",
    ),
    (
        "wifi_xlayer_sleep_rto",
        "diag.problem.wifi_xlayer_sleep_rto",
    ),
    (
        "wmg_dhcp_missing_after_attach",
        "diag.problem.wmg_dhcp_missing_after_attach",
    ),
    (
        "wmg_eap_aka_sync_failure",
        "diag.problem.wmg_eap_aka_sync_failure",
    ),
    (
        "wmg_gtpc_csr_unanswered",
        "diag.problem.wmg_gtpc_csr_unanswered",
    ),
    (
        "wmg_gtpc_csresp_cause_not_accepted",
        "diag.problem.wmg_gtpc_csresp_cause_not_accepted",
    ),
    (
        "wmg_gtpc_ok_without_gtpu",
        "diag.problem.wmg_gtpc_ok_without_gtpu",
    ),
    (
        "wmg_gtpu_error_indication",
        "diag.problem.wmg_gtpu_error_indication",
    ),
    (
        "wmg_gtpu_first_byte_slow",
        "diag.problem.wmg_gtpu_first_byte_slow",
    ),
    (
        "wmg_gy_cca_result_error",
        "diag.problem.wmg_gy_cca_result_error",
    ),
    (
        "wmg_gy_ccr_unanswered",
        "diag.problem.wmg_gy_ccr_unanswered",
    ),
    (
        "wmg_inner_rtt_inflation_clean_outer",
        "diag.problem.wmg_inner_rtt_inflation_clean_outer",
    ),
    (
        "wmg_radius_accept_without_gtpc",
        "diag.problem.wmg_radius_accept_without_gtpc",
    ),
    (
        "wmg_radius_request_unanswered",
        "diag.problem.wmg_radius_request_unanswered",
    ),
    (
        "wmg_roam_without_rule_refresh",
        "diag.problem.wmg_roam_without_rule_refresh",
    ),
    (
        "wmg_sta_dea_result_error",
        "diag.problem.wmg_sta_dea_result_error",
    ),
    (
        "wmg_sta_der_unanswered",
        "diag.problem.wmg_sta_der_unanswered",
    ),
];

/// Cause schemas for [`DEPENDENCY_EDGES`].
pub(crate) fn dependency_cause_schemas() -> Vec<CauseSchema> {
    DEPENDENCY_EDGES
        .iter()
        .map(|edge| cause(edge.cause, &[ScopeType::Session], None))
        .collect()
}

/// One event schema per problem sarif the dependency ruleset can join.
pub(crate) fn dependency_event_schemas() -> Vec<EventSchema> {
    DEPENDENCY_PROBLEM_EVENTS
        .iter()
        .map(|(_sarif, name)| {
            event(
                name,
                &[
                    session_target(),
                    int_field("time", 1),
                    FieldSchema {
                        name: "sarif_id",
                        field_type: FieldType::String,
                        pii: false,
                        idx: idx(2),
                    },
                ],
                &[ScopeRoute {
                    scope: ScopeType::Session,
                    path: "target",
                }],
            )
        })
        .collect()
}
