//! Wi-Fi W0 cause/problem pairs (83 isolated SARIF-ID bijection).
//! [`WIFI_MAPPINGS`] is the sole row-level source of truth.

use crate::helpers::{cause, problem};
use crate::{CauseSchema, ProblemSchema};
use airpulse_dsl_types::ScopeType;

/// One W0 Wi-Fi mapping row (catalog contract tests).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WifiMapping {
    /// Problem kind name.
    pub problem_kind: &'static str,
    /// Paired cause kind (`<ProblemKind>Signal`).
    pub cause_kind: &'static str,
    /// Default SARIF id (W0 authoritative).
    pub sarif_id: &'static str,
    /// Valid target scope.
    pub scope: ScopeType,
}

/// Canonical W0 table (83 rows).
pub const WIFI_MAPPINGS: &[WifiMapping] = &[
    WifiMapping {
        problem_kind: "WifiApBeaconLoss",
        cause_kind: "WifiApBeaconLossSignal",
        sarif_id: "wifi_ap_beacon_loss",
        scope: ScopeType::AccessPoint,
    },
    WifiMapping {
        problem_kind: "WifiApMalformedBeacon",
        cause_kind: "WifiApMalformedBeaconSignal",
        sarif_id: "wifi_ap_malformed_beacon",
        scope: ScopeType::AccessPoint,
    },
    WifiMapping {
        problem_kind: "WifiApBeaconJitter",
        cause_kind: "WifiApBeaconJitterSignal",
        sarif_id: "wifi_ap_beacon_jitter",
        scope: ScopeType::AccessPoint,
    },
    WifiMapping {
        problem_kind: "WifiApRadioHang",
        cause_kind: "WifiApRadioHangSignal",
        sarif_id: "wifi_ap_radio_hang",
        scope: ScopeType::AccessPoint,
    },
    WifiMapping {
        problem_kind: "WifiApAssocRateMismatch",
        cause_kind: "WifiApAssocRateMismatchSignal",
        sarif_id: "wifi_ap_assoc_rate_mismatch",
        scope: ScopeType::AccessPoint,
    },
    WifiMapping {
        problem_kind: "WifiApRtsFlood",
        cause_kind: "WifiApRtsFloodSignal",
        sarif_id: "wifi_ap_rts_flood",
        scope: ScopeType::AccessPoint,
    },
    WifiMapping {
        problem_kind: "WifiApUnprotectedAction",
        cause_kind: "WifiApUnprotectedActionSignal",
        sarif_id: "wifi_ap_unprotected_action",
        scope: ScopeType::AccessPoint,
    },
    WifiMapping {
        problem_kind: "WifiApCsa",
        cause_kind: "WifiApCsaSignal",
        sarif_id: "wifi_ap_csa",
        scope: ScopeType::AccessPoint,
    },
    WifiMapping {
        problem_kind: "WifiApRateMismatch",
        cause_kind: "WifiApRateMismatchSignal",
        sarif_id: "wifi_ap_rate_mismatch",
        scope: ScopeType::AccessPoint,
    },
    WifiMapping {
        problem_kind: "WifiApHighCoChannel",
        cause_kind: "WifiApHighCoChannelSignal",
        sarif_id: "wifi_ap_high_co_channel",
        scope: ScopeType::AccessPoint,
    },
    WifiMapping {
        problem_kind: "WifiApUnencryptedLeak",
        cause_kind: "WifiApUnencryptedLeakSignal",
        sarif_id: "wifi_ap_unencrypted_leak",
        scope: ScopeType::AccessPoint,
    },
    WifiMapping {
        problem_kind: "WifiApMissingHtCap",
        cause_kind: "WifiApMissingHtCapSignal",
        sarif_id: "wifi_ap_missing_ht_cap",
        scope: ScopeType::AccessPoint,
    },
    WifiMapping {
        problem_kind: "WifiApTimStuck",
        cause_kind: "WifiApTimStuckSignal",
        sarif_id: "wifi_ap_tim_stuck",
        scope: ScopeType::AccessPoint,
    },
    WifiMapping {
        problem_kind: "WifiApDuplicateBssid",
        cause_kind: "WifiApDuplicateBssidSignal",
        sarif_id: "wifi_ap_duplicate_bssid",
        scope: ScopeType::AccessPoint,
    },
    WifiMapping {
        problem_kind: "WifiApLowBeaconRssi",
        cause_kind: "WifiApLowBeaconRssiSignal",
        sarif_id: "wifi_ap_low_beacon_rssi",
        scope: ScopeType::AccessPoint,
    },
    WifiMapping {
        problem_kind: "WifiApProbePowerMismatch",
        cause_kind: "WifiApProbePowerMismatchSignal",
        sarif_id: "wifi_ap_probe_power_mismatch",
        scope: ScopeType::AccessPoint,
    },
    WifiMapping {
        problem_kind: "WifiApExcessiveRetries",
        cause_kind: "WifiApExcessiveRetriesSignal",
        sarif_id: "wifi_ap_excessive_retries",
        scope: ScopeType::AccessPoint,
    },
    WifiMapping {
        problem_kind: "WifiApBarFlood",
        cause_kind: "WifiApBarFloodSignal",
        sarif_id: "wifi_ap_bar_flood",
        scope: ScopeType::AccessPoint,
    },
    WifiMapping {
        problem_kind: "WifiApLargeNavCts",
        cause_kind: "WifiApLargeNavCtsSignal",
        sarif_id: "wifi_ap_large_nav_cts",
        scope: ScopeType::AccessPoint,
    },
    WifiMapping {
        problem_kind: "WifiApPmfDeauth",
        cause_kind: "WifiApPmfDeauthSignal",
        sarif_id: "wifi_ap_pmf_deauth",
        scope: ScopeType::AccessPoint,
    },
    WifiMapping {
        problem_kind: "WifiApWeakSecurity",
        cause_kind: "WifiApWeakSecuritySignal",
        sarif_id: "wifi_ap_weak_security",
        scope: ScopeType::AccessPoint,
    },
    WifiMapping {
        problem_kind: "WifiApDfsEvent",
        cause_kind: "WifiApDfsEventSignal",
        sarif_id: "wifi_ap_dfs_event",
        scope: ScopeType::AccessPoint,
    },
    WifiMapping {
        problem_kind: "WifiApOverlappingBss",
        cause_kind: "WifiApOverlappingBssSignal",
        sarif_id: "wifi_ap_overlapping_bss",
        scope: ScopeType::AccessPoint,
    },
    WifiMapping {
        problem_kind: "WifiApSuboptimalDtim",
        cause_kind: "WifiApSuboptimalDtimSignal",
        sarif_id: "wifi_ap_suboptimal_dtim",
        scope: ScopeType::AccessPoint,
    },
    WifiMapping {
        problem_kind: "WifiApQbssOverload",
        cause_kind: "WifiApQbssOverloadSignal",
        sarif_id: "wifi_ap_qbss_overload",
        scope: ScopeType::AccessPoint,
    },
    WifiMapping {
        problem_kind: "WifiApRogueCandidate",
        cause_kind: "WifiApRogueCandidateSignal",
        sarif_id: "wifi_ap_rogue_candidate",
        scope: ScopeType::AccessPoint,
    },
    WifiMapping {
        problem_kind: "WifiApCollisionStress",
        cause_kind: "WifiApCollisionStressSignal",
        sarif_id: "wifi_ap_collision_stress",
        scope: ScopeType::AccessPoint,
    },
    WifiMapping {
        problem_kind: "WifiApNonWifiInterference",
        cause_kind: "WifiApNonWifiInterferenceSignal",
        sarif_id: "wifi_ap_non_wifi_interference",
        scope: ScopeType::AccessPoint,
    },
    WifiMapping {
        problem_kind: "WifiApDeauthFlood",
        cause_kind: "WifiApDeauthFloodSignal",
        sarif_id: "wifi_ap_deauth_flood",
        scope: ScopeType::AccessPoint,
    },
    WifiMapping {
        problem_kind: "WifiApMissingHeCap",
        cause_kind: "WifiApMissingHeCapSignal",
        sarif_id: "wifi_ap_missing_he_cap",
        scope: ScopeType::AccessPoint,
    },
    WifiMapping {
        problem_kind: "WifiApWidth320WithoutEht",
        cause_kind: "WifiApWidth320WithoutEhtSignal",
        sarif_id: "wifi_ap_width320_without_eht",
        scope: ScopeType::AccessPoint,
    },
    WifiMapping {
        problem_kind: "WifiApPuncturingMismatch",
        cause_kind: "WifiApPuncturingMismatchSignal",
        sarif_id: "wifi_ap_puncturing_mismatch",
        scope: ScopeType::AccessPoint,
    },
    WifiMapping {
        problem_kind: "WifiClientHighRetries",
        cause_kind: "WifiClientHighRetriesSignal",
        sarif_id: "wifi_client_high_retries",
        scope: ScopeType::ClientMac,
    },
    WifiMapping {
        problem_kind: "WifiClientSleepPattern",
        cause_kind: "WifiClientSleepPatternSignal",
        sarif_id: "wifi_client_sleep_pattern",
        scope: ScopeType::ClientMac,
    },
    WifiMapping {
        problem_kind: "WifiClientExcessiveProbing",
        cause_kind: "WifiClientExcessiveProbingSignal",
        sarif_id: "wifi_client_excessive_probing",
        scope: ScopeType::ClientMac,
    },
    WifiMapping {
        problem_kind: "WifiClientFailedPskAuth",
        cause_kind: "WifiClientFailedPskAuthSignal",
        sarif_id: "wifi_client_failed_psk_auth",
        scope: ScopeType::ClientMac,
    },
    WifiMapping {
        problem_kind: "WifiClientEapAuthFailure",
        cause_kind: "WifiClientEapAuthFailureSignal",
        sarif_id: "wifi_client_eap_auth_failure",
        scope: ScopeType::ClientMac,
    },
    WifiMapping {
        problem_kind: "WifiClientEapMethodMismatch",
        cause_kind: "WifiClientEapMethodMismatchSignal",
        sarif_id: "wifi_client_eap_method_mismatch",
        scope: ScopeType::ClientMac,
    },
    WifiMapping {
        problem_kind: "WifiClientRoamingFailure",
        cause_kind: "WifiClientRoamingFailureSignal",
        sarif_id: "wifi_client_roaming_failure",
        scope: ScopeType::ClientMac,
    },
    WifiMapping {
        problem_kind: "WifiClientSlowRoaming",
        cause_kind: "WifiClientSlowRoamingSignal",
        sarif_id: "wifi_client_slow_roaming",
        scope: ScopeType::ClientMac,
    },
    WifiMapping {
        problem_kind: "WifiClientUnprotectedAction",
        cause_kind: "WifiClientUnprotectedActionSignal",
        sarif_id: "wifi_client_unprotected_action",
        scope: ScopeType::ClientMac,
    },
    WifiMapping {
        problem_kind: "WifiClientRateSnrMismatch",
        cause_kind: "WifiClientRateSnrMismatchSignal",
        sarif_id: "wifi_client_rate_snr_mismatch",
        scope: ScopeType::ClientMac,
    },
    WifiMapping {
        problem_kind: "WifiClientOneWayTraffic",
        cause_kind: "WifiClientOneWayTrafficSignal",
        sarif_id: "wifi_client_one_way_traffic",
        scope: ScopeType::ClientMac,
    },
    WifiMapping {
        problem_kind: "WifiClientDeauthLoop",
        cause_kind: "WifiClientDeauthLoopSignal",
        sarif_id: "wifi_client_deauth_loop",
        scope: ScopeType::ClientMac,
    },
    WifiMapping {
        problem_kind: "WifiClientPowerSaveStuck",
        cause_kind: "WifiClientPowerSaveStuckSignal",
        sarif_id: "wifi_client_power_save_stuck",
        scope: ScopeType::ClientMac,
    },
    WifiMapping {
        problem_kind: "WifiClientInvalidPmkid",
        cause_kind: "WifiClientInvalidPmkidSignal",
        sarif_id: "wifi_client_invalid_pmkid",
        scope: ScopeType::ClientMac,
    },
    WifiMapping {
        problem_kind: "WifiClientNullPmkidFt",
        cause_kind: "WifiClientNullPmkidFtSignal",
        sarif_id: "wifi_client_null_pmkid_ft",
        scope: ScopeType::ClientMac,
    },
    WifiMapping {
        problem_kind: "WifiClientSlowAuth",
        cause_kind: "WifiClientSlowAuthSignal",
        sarif_id: "wifi_client_slow_auth",
        scope: ScopeType::ClientMac,
    },
    WifiMapping {
        problem_kind: "WifiClientEncryptedBeforeM4",
        cause_kind: "WifiClientEncryptedBeforeM4Signal",
        sarif_id: "wifi_client_encrypted_before_m4",
        scope: ScopeType::ClientMac,
    },
    WifiMapping {
        problem_kind: "WifiClientPerTidAsymmetry",
        cause_kind: "WifiClientPerTidAsymmetrySignal",
        sarif_id: "wifi_client_per_tid_asymmetry",
        scope: ScopeType::ClientMac,
    },
    WifiMapping {
        problem_kind: "WifiClientSaeDeauthNoEap",
        cause_kind: "WifiClientSaeDeauthNoEapSignal",
        sarif_id: "wifi_client_sae_deauth_no_eap",
        scope: ScopeType::ClientMac,
    },
    WifiMapping {
        problem_kind: "WifiClientSentDeauth",
        cause_kind: "WifiClientSentDeauthSignal",
        sarif_id: "wifi_client_sent_deauth",
        scope: ScopeType::ClientMac,
    },
    WifiMapping {
        problem_kind: "WifiClientSticky",
        cause_kind: "WifiClientStickySignal",
        sarif_id: "wifi_client_sticky",
        scope: ScopeType::ClientMac,
    },
    WifiMapping {
        problem_kind: "WifiClientAssocTimeout",
        cause_kind: "WifiClientAssocTimeoutSignal",
        sarif_id: "wifi_client_assoc_timeout",
        scope: ScopeType::ClientMac,
    },
    WifiMapping {
        problem_kind: "WifiClientFrequentDisconnects",
        cause_kind: "WifiClientFrequentDisconnectsSignal",
        sarif_id: "wifi_client_frequent_disconnects",
        scope: ScopeType::ClientMac,
    },
    WifiMapping {
        problem_kind: "WifiClientBandSteeringResistance",
        cause_kind: "WifiClientBandSteeringResistanceSignal",
        sarif_id: "wifi_client_band_steering_resistance",
        scope: ScopeType::ClientMac,
    },
    WifiMapping {
        problem_kind: "WifiClientUplinkAsymmetry",
        cause_kind: "WifiClientUplinkAsymmetrySignal",
        sarif_id: "wifi_client_uplink_asymmetry",
        scope: ScopeType::ClientMac,
    },
    WifiMapping {
        problem_kind: "WifiClientFtKvCompat",
        cause_kind: "WifiClientFtKvCompatSignal",
        sarif_id: "wifi_client_ft_kv_compat",
        scope: ScopeType::ClientMac,
    },
    WifiMapping {
        problem_kind: "WifiClientMacSpoofSuspected",
        cause_kind: "WifiClientMacSpoofSuspectedSignal",
        sarif_id: "wifi_client_mac_spoof_suspected",
        scope: ScopeType::ClientMac,
    },
    WifiMapping {
        problem_kind: "WifiClientGenerationMismatch",
        cause_kind: "WifiClientGenerationMismatchSignal",
        sarif_id: "wifi_client_generation_mismatch",
        scope: ScopeType::ClientMac,
    },
    WifiMapping {
        problem_kind: "WifiClientEmlsrChurn",
        cause_kind: "WifiClientEmlsrChurnSignal",
        sarif_id: "wifi_client_emlsr_churn",
        scope: ScopeType::ClientMac,
    },
    WifiMapping {
        problem_kind: "WifiClientNoPscScan",
        cause_kind: "WifiClientNoPscScanSignal",
        sarif_id: "wifi_client_no_psc_scan",
        scope: ScopeType::ClientMac,
    },
    WifiMapping {
        problem_kind: "WifiClientEapRadiusTimeout",
        cause_kind: "WifiClientEapRadiusTimeoutSignal",
        sarif_id: "wifi_client_eap_radius_timeout",
        scope: ScopeType::ClientMac,
    },
    WifiMapping {
        problem_kind: "WifiClientEapAccessReject",
        cause_kind: "WifiClientEapAccessRejectSignal",
        sarif_id: "wifi_client_eap_access_reject",
        scope: ScopeType::ClientMac,
    },
    WifiMapping {
        problem_kind: "WifiClientEapTlsCert",
        cause_kind: "WifiClientEapTlsCertSignal",
        sarif_id: "wifi_client_eap_tls_cert",
        scope: ScopeType::ClientMac,
    },
    WifiMapping {
        problem_kind: "WifiClientDhcpTimeout",
        cause_kind: "WifiClientDhcpTimeoutSignal",
        sarif_id: "wifi_client_dhcp_timeout",
        scope: ScopeType::ClientMac,
    },
    WifiMapping {
        problem_kind: "WifiClientDhcpNak",
        cause_kind: "WifiClientDhcpNakSignal",
        sarif_id: "wifi_client_dhcp_nak",
        scope: ScopeType::ClientMac,
    },
    WifiMapping {
        problem_kind: "WifiClientDhcpDnsMismatch",
        cause_kind: "WifiClientDhcpDnsMismatchSignal",
        sarif_id: "wifi_client_dhcp_dns_mismatch",
        scope: ScopeType::ClientMac,
    },
    WifiMapping {
        problem_kind: "WifiClientDhcpv6Timeout",
        cause_kind: "WifiClientDhcpv6TimeoutSignal",
        sarif_id: "wifi_client_dhcpv6_timeout",
        scope: ScopeType::ClientMac,
    },
    WifiMapping {
        problem_kind: "WifiClientDnsTimeout",
        cause_kind: "WifiClientDnsTimeoutSignal",
        sarif_id: "wifi_client_dns_timeout",
        scope: ScopeType::ClientMac,
    },
    WifiMapping {
        problem_kind: "WifiClientDnsSlow",
        cause_kind: "WifiClientDnsSlowSignal",
        sarif_id: "wifi_client_dns_slow",
        scope: ScopeType::ClientMac,
    },
    WifiMapping {
        problem_kind: "WifiClientDnsServfail",
        cause_kind: "WifiClientDnsServfailSignal",
        sarif_id: "wifi_client_dns_servfail",
        scope: ScopeType::ClientMac,
    },
    WifiMapping {
        problem_kind: "WifiClientCaptivePortal",
        cause_kind: "WifiClientCaptivePortalSignal",
        sarif_id: "wifi_client_captive_portal",
        scope: ScopeType::ClientMac,
    },
    WifiMapping {
        problem_kind: "WifiXlayerRoamingStall",
        cause_kind: "WifiXlayerRoamingStallSignal",
        sarif_id: "wifi_xlayer_roaming_stall",
        scope: ScopeType::Session,
    },
    WifiMapping {
        problem_kind: "WifiXlayerRetriesLoss",
        cause_kind: "WifiXlayerRetriesLossSignal",
        sarif_id: "wifi_xlayer_retries_loss",
        scope: ScopeType::Session,
    },
    WifiMapping {
        problem_kind: "WifiXlayerRateThroughputDrop",
        cause_kind: "WifiXlayerRateThroughputDropSignal",
        sarif_id: "wifi_xlayer_rate_throughput_drop",
        scope: ScopeType::Session,
    },
    WifiMapping {
        problem_kind: "WifiXlayerSleepRto",
        cause_kind: "WifiXlayerSleepRtoSignal",
        sarif_id: "wifi_xlayer_sleep_rto",
        scope: ScopeType::Session,
    },
    WifiMapping {
        problem_kind: "WifiXlayerDeauthDisconnect",
        cause_kind: "WifiXlayerDeauthDisconnectSignal",
        sarif_id: "wifi_xlayer_deauth_disconnect",
        scope: ScopeType::Session,
    },
    WifiMapping {
        problem_kind: "WifiXlayerCsaGap",
        cause_kind: "WifiXlayerCsaGapSignal",
        sarif_id: "wifi_xlayer_csa_gap",
        scope: ScopeType::Session,
    },
    WifiMapping {
        problem_kind: "WifiXlayerL3HintTcp",
        cause_kind: "WifiXlayerL3HintTcpSignal",
        sarif_id: "wifi_xlayer_l3_hint_tcp",
        scope: ScopeType::Session,
    },
    WifiMapping {
        problem_kind: "WifiXlayerBssColorLoss",
        cause_kind: "WifiXlayerBssColorLossSignal",
        sarif_id: "wifi_xlayer_bss_color_loss",
        scope: ScopeType::Session,
    },
    WifiMapping {
        problem_kind: "WifiXlayerTwtRto",
        cause_kind: "WifiXlayerTwtRtoSignal",
        sarif_id: "wifi_xlayer_twt_rto",
        scope: ScopeType::Session,
    },
    WifiMapping {
        problem_kind: "WifiXlayerCsaThroughputDrop",
        cause_kind: "WifiXlayerCsaThroughputDropSignal",
        sarif_id: "wifi_xlayer_csa_throughput_drop",
        scope: ScopeType::Session,
    },
];

pub(crate) fn wifi_problems() -> Box<[ProblemSchema]> {
    WIFI_MAPPINGS
        .iter()
        .map(|row| {
            problem(
                row.problem_kind,
                std::slice::from_ref(&row.scope),
                row.sarif_id,
                None,
            )
        })
        .collect::<Vec<_>>()
        .into_boxed_slice()
}

pub(crate) fn wifi_causes() -> Box<[CauseSchema]> {
    WIFI_MAPPINGS
        .iter()
        .map(|row| cause(row.cause_kind, std::slice::from_ref(&row.scope), None))
        .collect::<Vec<_>>()
        .into_boxed_slice()
}
