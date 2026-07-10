//! Catalog cause schemas.

use airpulse_dsl_types::ScopeType;

use crate::CauseSchema;
use crate::helpers::cause;
use crate::wifi;

/// Returns all catalog cause schemas.
pub(crate) fn all_causes() -> Box<[CauseSchema]> {
    [
        // --- Legacy seed causes (10-catalog-abi.md §3) ---
        cause(
            "PmtudBlackhole",
            &[ScopeType::Session, ScopeType::Vlan],
            None,
        ),
        cause("Congestion", &[ScopeType::Session], None),
        cause(
            "TransientL2Disruption",
            &[ScopeType::Session, ScopeType::Vlan],
            None,
        ),
        cause("PhysicalCableDamage", &[ScopeType::Port], None),
        cause("AuthServerOutage", &[ScopeType::Vlan], None),
        cause("RfInterference", &[ScopeType::AccessPoint], None),
        cause("UpstreamOutage", &[ScopeType::Global], None),
        cause("PhysicalLinkAbsent", &[ScopeType::Port], None),
        // --- Legacy-parity causes (docs/migration/legacy-to-adgl-ids.md) ---
        cause("PacketLossPath", &[ScopeType::Session], None),
        cause("SpuriousRetransmission", &[ScopeType::Session], None),
        cause("RadioChannelDegradation", &[ScopeType::Session], None),
        cause("ReceiverWindowLimit", &[ScopeType::Session], None),
        cause("SenderBufferLimit", &[ScopeType::Session], None),
        cause("BufferbloatQueue", &[ScopeType::Session], None),
        cause(
            "MiddleboxInterference",
            &[ScopeType::Session, ScopeType::Vlan],
            None,
        ),
        cause("UplinkLatencyPath", &[ScopeType::Session], None),
        cause("SlowStartRamp", &[ScopeType::Session], None),
        cause("CaptureScopeArtifact", &[ScopeType::Session], None),
        cause(
            "LanL2Instability",
            &[ScopeType::Session, ScopeType::Vlan],
            None,
        ),
        cause(
            "AddressingEdgeIssue",
            &[ScopeType::Session, ScopeType::Vlan, ScopeType::ClientMac],
            None,
        ),
        cause("TlsPathInterference", &[ScopeType::Session], None),
        cause(
            "ControlPlaneAnomaly",
            &[ScopeType::Global, ScopeType::Vlan],
            None,
        ),
        cause(
            "CoexistencePressure",
            &[ScopeType::Session, ScopeType::Vlan],
            None,
        ),
        // --- v1.2 per-sarif-family causes (additive; one cause per L3 sarif
        // family so conjunctive evidence never sums across families) ---
        cause("PmtudPathIssue", &[ScopeType::Session], None),
        cause("IcmpUnreachablePath", &[ScopeType::Session], None),
        cause("IcmpControlPlaneNoise", &[ScopeType::Session], None),
        cause("ArpDuplicateAddress", &[ScopeType::Session], None),
        cause("ArpStormPressure", &[ScopeType::Session], None),
        cause("NdpVisibilityGap", &[ScopeType::Session], None),
        cause("StpReconvergence", &[ScopeType::Session], None),
        cause("DhcpDnsEdgeIssue", &[ScopeType::Session], None),
        cause("DnsResolutionPath", &[ScopeType::Session], None),
        cause("DnsTlsInconsistency", &[ScopeType::Session], None),
        cause("TlsHandshakeSlowness", &[ScopeType::Session], None),
        cause("HttpPolicyChain", &[ScopeType::Session], None),
        cause("QuicCoexistence", &[ScopeType::Session], None),
        cause("Dot1xOnboarding", &[ScopeType::Session], None),
        cause("MulticastLanLoad", &[ScopeType::Session], None),
        cause("ZeroconfNoise", &[ScopeType::Session], None),
        cause("VlanStackingVisibility", &[ScopeType::Session], None),
        cause("NtpChatter", &[ScopeType::Session], None),
        cause("LldpContextOnly", &[ScopeType::Session], None),
        // v1.2: splits CaptureScopeArtifact so tcp_incomplete_handshake and
        // tcp_capture_scope_middlebox families no longer share one cause.
        cause("IncompleteHandshakeCapture", &[ScopeType::Session], None),
        // --- v1.2 per-rule protocol causes (additive; one cause per protocol
        // rule so conjunctive evidence never sums across protocols) ---
        cause("RadiusHighRejectSignal", &[ScopeType::Global], None),
        cause("RadiusRetransSignal", &[ScopeType::Global], None),
        cause("RadiusSlowAuthSignal", &[ScopeType::Global], None),
        cause("DiameterCerNoCeaSignal", &[ScopeType::Global], None),
        cause(
            "DiameterWatchdogUnansweredSignal",
            &[ScopeType::Global],
            None,
        ),
        cause("DiameterResultErrorsSignal", &[ScopeType::Global], None),
        cause("DiameterMalformedSignal", &[ScopeType::Global], None),
        cause(
            "DiameterSessionTermImbalanceSignal",
            &[ScopeType::Global],
            None,
        ),
        cause("GtpCreateSessionFailuresSignal", &[ScopeType::Global], None),
        cause("GtpEchoUnansweredSignal", &[ScopeType::Global], None),
        cause(
            "GtpDeleteSessionImbalanceSignal",
            &[ScopeType::Global],
            None,
        ),
        cause(
            "GtpModifySessionImbalanceSignal",
            &[ScopeType::Global],
            None,
        ),
        cause("GtpUserOnlySignal", &[ScopeType::Global], None),
        cause("DhcpHighNakSignal", &[ScopeType::Global], None),
        cause("DhcpMissingOfferSignal", &[ScopeType::Global], None),
        cause("DhcpSlowLeaseSignal", &[ScopeType::Global], None),
    ]
    .into_iter()
    .chain(wifi::wifi_causes())
    .collect()
}
