//! Catalog entries for `airpulse.wmg_e2e`.
//!
//! Field indexes match the projector in `adgl_adapter`: `target` is 0, `time` is 1,
//! and the remaining integers follow the order that ruleset reads.

use airpulse_dsl_types::{ScopeType, Severity};

use crate::helpers::{cause, event, int_field, problem, session_target};
use crate::{CauseSchema, EventSchema, ProblemSchema, ScopeRoute};

/// Session event with `target`, `time`, then the named integer fields starting at index 2.
fn session_event(name: &'static str, extras: &[&'static str]) -> EventSchema {
    let mut fields = vec![session_target(), int_field("time", 1)];
    for (offset, field) in extras.iter().enumerate() {
        fields.push(int_field(field, (offset + 2) as u16));
    }
    event(
        name,
        &fields,
        &[ScopeRoute {
            scope: ScopeType::Session,
            path: "target",
        }],
    )
}

/// WMG attach-chain events projected per UE session.
pub(crate) fn wmg_events() -> Vec<EventSchema> {
    vec![
        session_event(
            "gtp.session_summary",
            &[
                "create_session_req",
                "create_session_resp",
                "cause_accepted",
                "gtpu_first_byte_delay_ms",
            ],
        ),
        session_event(
            "sta.der_dea_summary",
            &["unanswered_der", "result_error", "dea"],
        ),
        session_event("radius.eap_exchange", &["aka_sync_failure"]),
        session_event(
            "radius.ue_summary",
            &[
                "access_request",
                "access_accept",
                "access_reject",
                "unanswered_request",
            ],
        ),
        session_event("wmg.chain_profile", &["nswo"]),
        session_event("gtpu.inner_summary", &["inner_packets"]),
        session_event("gtpu.summary", &["error_indication"]),
        session_event(
            "tcp.tunnel_rtt",
            &["inner_rtt_inflation_centi", "outer_rtt_inflation_centi"],
        ),
        session_event("dhcp.transaction", &["ack"]),
        session_event("wmg.reassoc", &["count"]),
        session_event("gy.cc_summary", &["unanswered_ccr", "result_error"]),
    ]
}

/// Cause kinds inferred by `airpulse.wmg_e2e`.
pub(crate) fn wmg_causes() -> Vec<CauseSchema> {
    [
        "GtpcS2aRejectSignal",
        "StaDerUnansweredSignal",
        "StaDeaResultErrorSignal",
        "EapAkaSyncFailureSignal",
        "AttachChainBreakTwifToGtpcSignal",
        "UserPlaneNotInstalledSignal",
        "GtpuInvalidTeidSignal",
        "TunnelRttInflationSignal",
        "UserPlaneSlowInstallSignal",
        "DhcpAllocationMissingSignal",
        "RoamWithoutBearerUpdateSignal",
        "RadiusRequestUnansweredSignal",
        "GtpcCsrUnansweredSignal",
        "GyCcrUnansweredSignal",
        "GyCcaResultErrorSignal",
    ]
    .into_iter()
    .map(|name| cause(name, &[ScopeType::Session], None))
    .collect()
}

/// Problems emitted by `airpulse.wmg_e2e`.
pub(crate) fn wmg_problems() -> Vec<ProblemSchema> {
    let rows = [
        (
            "GtpcS2aReject",
            "wmg_gtpc_csresp_cause_not_accepted",
            Severity::High,
        ),
        ("StaDerUnanswered", "wmg_sta_der_unanswered", Severity::High),
        (
            "StaDeaResultError",
            "wmg_sta_dea_result_error",
            Severity::High,
        ),
        (
            "EapAkaSyncFailure",
            "wmg_eap_aka_sync_failure",
            Severity::High,
        ),
        (
            "AttachChainBreakTwifToGtpc",
            "wmg_radius_accept_without_gtpc",
            Severity::High,
        ),
        (
            "UserPlaneNotInstalled",
            "wmg_gtpc_ok_without_gtpu",
            Severity::High,
        ),
        (
            "GtpuInvalidTeid",
            "wmg_gtpu_error_indication",
            Severity::High,
        ),
        (
            "TunnelRttInflation",
            "wmg_inner_rtt_inflation_clean_outer",
            Severity::High,
        ),
        (
            "UserPlaneSlowInstall",
            "wmg_gtpu_first_byte_slow",
            Severity::Medium,
        ),
        (
            "DhcpAllocationMissing",
            "wmg_dhcp_missing_after_attach",
            Severity::Medium,
        ),
        (
            "RoamWithoutBearerUpdate",
            "wmg_roam_without_rule_refresh",
            Severity::Medium,
        ),
        (
            "RadiusRequestUnanswered",
            "wmg_radius_request_unanswered",
            Severity::High,
        ),
        (
            "GtpcCsrUnanswered",
            "wmg_gtpc_csr_unanswered",
            Severity::High,
        ),
        ("GyCcrUnanswered", "wmg_gy_ccr_unanswered", Severity::High),
        (
            "GyCcaResultError",
            "wmg_gy_cca_result_error",
            Severity::High,
        ),
    ];
    rows.into_iter()
        .map(|(name, sarif, severity)| problem(name, &[ScopeType::Session], sarif, Some(severity)))
        .collect()
}
