use airpulse_dsl_catalog::{
    ActionArgKind, EventOrBindingType, FieldType, capabilities, capability_for, catalog_ref,
    cause_count, check_kinds, event_count, events, exclusivity_defaults, observation_kinds,
    problem_count, resolve_action, resolve_cause, resolve_event, resolve_metric_path,
    resolve_problem, resolve_problem_by_sarif, resolve_topo_fn, wifi_w0_mappings,
};
use airpulse_dsl_types::{CauseKind, EventType, ProblemKind};

#[test]
fn resolves_all_identifiers_used_in_examples_and_fixtures() {
    // docs/idea/examples/*.adgl + evaluator fixtures (`tcp.retransmission_burst`, `icmp.ptb`).
    let events = [
        "tcp.retransmission_burst",
        "icmp.ptb",
        "wifi.deauth_burst",
        "wifi.rf_telemetry",
        "stp.topology_change",
        "dhcp.timeout",
        "radius.access_request",
        "dot1x.eapol_start",
        "port.crc_errors",
        "port.link_flap",
        "port.admin_state",
        "port.oper_state",
    ];
    for name in events {
        assert!(resolve_event(name).is_some(), "event {name} should resolve");
    }

    let causes = [
        "PmtudBlackhole",
        "Congestion",
        "TransientL2Disruption",
        "AuthServerOutage",
        "PhysicalCableDamage",
        "PhysicalLinkAbsent",
        "RfInterference",
        "UpstreamOutage",
    ];
    for name in causes {
        assert!(resolve_cause(name).is_some(), "cause {name} should resolve");
    }

    let problems = [
        "XlIcmpTcpMss",
        "SpanningTreeInstability",
        "ClientOnboardingFailure",
        "WlanRadiusOutage",
        "CableDisconnected",
        "DeviceUnreachable",
        "AmbiguousDiagnosis",
    ];
    for name in problems {
        assert!(
            resolve_problem(name).is_some(),
            "problem {name} should resolve"
        );
    }

    let actions = [
        "request_observation",
        "run_check",
        "suppress_symptom",
        "mark_ambiguous",
        "request_topology",
    ];
    for name in actions {
        assert!(
            resolve_action(name).is_some(),
            "action {name} should resolve"
        );
    }

    let topo_fns = [
        "same_session",
        "same_client",
        "same_port",
        "same_ap",
        "same_vlan",
        "upstream_of",
    ];
    for name in topo_fns {
        assert!(
            resolve_topo_fn(name).is_some(),
            "topology function {name} should resolve"
        );
    }
}

#[test]
fn resolves_metric_paths_and_rejects_unknown_fields() {
    let rtx = EventType::new("tcp.retransmission_burst");
    let (idx, ty) = resolve_metric_path(EventOrBindingType::Event(&rtx), "segment_size")
        .expect("segment_size should resolve");
    assert_eq!(idx.0, 0);
    assert_eq!(ty, FieldType::Int);

    let (idx, ty) = resolve_metric_path(EventOrBindingType::Event(&rtx), "rtx.segment_size")
        .expect("qualified path should resolve");
    assert_eq!(idx.0, 0);
    assert_eq!(ty, FieldType::Int);

    let cause = CauseKind::new("PmtudBlackhole");
    let (idx, ty) = resolve_metric_path(EventOrBindingType::Cause(&cause), "c.confidence")
        .expect("cause confidence should resolve");
    assert_eq!(idx.0, 2);
    assert_eq!(ty, FieldType::Confidence);

    let problem = ProblemKind::new("DeviceUnreachable");
    let (idx, ty) = resolve_metric_path(EventOrBindingType::Problem(&problem), "p.sarif_id")
        .expect("problem sarif_id should resolve");
    assert_eq!(idx.0, 4);
    assert_eq!(ty, FieldType::SarifId);

    assert!(resolve_metric_path(EventOrBindingType::Event(&rtx), "unknown_field").is_none());
}

#[test]
fn tcp_session_summary_resolves_all_metric_fields() {
    let schema = resolve_event("tcp.session_summary").expect("tcp.session_summary should exist");
    let fields = [
        "target",
        "loss_rate_pm",
        "burst_count",
        "loss_type",
        "rto_ratio_pm",
        "rtt_mean_ms",
        "rtt_p95_ms",
        "rtt_inflation_centi",
        "cwnd_rwin_ratio_centi",
        "rwin_median",
        "cwnd_median",
        "bdp_gap",
        "bufferbloat",
        "buffer_mean_ratio_centi",
        "throughput_mean_kbps",
        "zero_window_events",
        "acked_unseen_count",
        "checksum_bad_count",
        "completeness",
        "sack_scoreboard_retrans",
        "tail_loss_probe_events",
        "rst_count",
        "fast_retransmissions",
        "rto_count",
        "duration_ms",
        "total_bytes",
        "time",
        "lost_packets",
        "packet_count",
        "session_is_ipv6",
    ];
    assert_eq!(schema.fields.len(), fields.len());
    let event_type = EventType::new("tcp.session_summary");
    for field in fields {
        assert!(
            resolve_metric_path(EventOrBindingType::Event(&event_type), field).is_some(),
            "field {field} should resolve on tcp.session_summary"
        );
    }
}

#[test]
fn resolves_legacy_parity_causes() {
    let causes = [
        "PacketLossPath",
        "SpuriousRetransmission",
        "RadioChannelDegradation",
        "ReceiverWindowLimit",
        "SenderBufferLimit",
        "BufferbloatQueue",
        "MiddleboxInterference",
        "UplinkLatencyPath",
        "SlowStartRamp",
        "CaptureScopeArtifact",
        "LanL2Instability",
        "AddressingEdgeIssue",
        "TlsPathInterference",
        "ControlPlaneAnomaly",
        "CoexistencePressure",
    ];
    for name in causes {
        assert!(resolve_cause(name).is_some(), "cause {name} should resolve");
    }
}

#[test]
fn resolves_legacy_parity_sarif_ids() {
    let sarif_ids = [
        // TCP
        "tcp_zero_window",
        "tcp_capture_scope_middlebox",
        "tcp_checksum_link",
        "tcp_incomplete_handshake",
        "tcp_loss_sack",
        "tcp_spurious_reordering",
        "tcp_packet_loss",
        "tcp_radio_channel",
        "tcp_rwin_limited",
        "tcp_bufferbloat",
        "tcp_sender_buffer",
        "tcp_uplink_latency",
        "tcp_middlebox",
        "tcp_slow_ramping",
        // L3
        "l3_pmtud_blackhole",
        "l3_icmp_unreachable_path",
        "l3_icmp_control_plane",
        "l3_arp_duplicate_l2",
        "l3_arp_storm_segment",
        "l3_ndp_ipv6_visibility",
        "l3_stp_spanning_tree",
        "l3_dhcp_dns_edge",
        "l3_dns_resolution",
        "l3_dns_tls_consistency",
        "l3_tls_hs_slow",
        "l3_http_redirect_chain",
        "l3_quic_coexist_path",
        "l3_dot1x_wired",
        "l3_multicast_lan",
        "l3_zeroconf_noise",
        "l3_vlan_stack_visibility",
        "l3_ntp_chatter",
        "l3_lldp_context",
        // Protocol
        "radius_high_reject",
        "radius_retrans",
        "radius_slow_auth",
        "diameter_cer_no_cea",
        "diameter_watchdog_unanswered",
        "diameter_result_errors",
        "diameter_malformed",
        "diameter_session_term_imbalance",
        "gtp_create_session_failures",
        "gtp_echo_unanswered",
        "gtp_delete_session_imbalance",
        "gtp_modify_session_imbalance",
        "gtp_user_only",
        "dhcp_high_nak",
        "dhcp_missing_offer",
        "dhcp_slow_lease",
    ];
    for sarif_id in sarif_ids {
        assert!(
            resolve_problem_by_sarif(sarif_id).is_some(),
            "sarif_id {sarif_id} should resolve"
        );
    }
}

#[test]
fn resolves_protocol_per_rule_causes() {
    let causes = [
        "RadiusHighRejectSignal",
        "RadiusRetransSignal",
        "RadiusSlowAuthSignal",
        "DiameterCerNoCeaSignal",
        "DiameterWatchdogUnansweredSignal",
        "DiameterResultErrorsSignal",
        "DiameterMalformedSignal",
        "DiameterSessionTermImbalanceSignal",
        "GtpCreateSessionFailuresSignal",
        "GtpEchoUnansweredSignal",
        "GtpDeleteSessionImbalanceSignal",
        "GtpModifySessionImbalanceSignal",
        "GtpUserOnlySignal",
        "DhcpHighNakSignal",
        "DhcpMissingOfferSignal",
        "DhcpSlowLeaseSignal",
    ];
    for name in causes {
        assert!(resolve_cause(name).is_some(), "cause {name} should resolve");
    }
}

#[test]
fn catalog_entry_counts_match_legacy_parity_contract() {
    assert_eq!(
        event_count(),
        70,
        "expected 44 base + 26 bounded v1.3 Wi-Fi/cross-layer events"
    );
    assert_eq!(
        cause_count(),
        142,
        "expected 59 base + 83 v1.3 Wi-Fi per-rule causes"
    );
    assert_eq!(
        problem_count(),
        138,
        "expected 55 base + 83 v1.3 Wi-Fi problems"
    );
}

#[test]
fn exclusivity_defaults_match_catalog_pairs() {
    let pairs: Vec<(&str, &str)> = exclusivity_defaults()
        .iter()
        .map(|pair| (pair.left.as_str(), pair.right.as_str()))
        .collect();
    assert_eq!(
        pairs,
        vec![
            ("Congestion", "PmtudBlackhole"),
            ("Congestion", "TransientL2Disruption"),
            ("PmtudBlackhole", "TransientL2Disruption"),
            ("PhysicalCableDamage", "PhysicalLinkAbsent"),
            ("AuthServerOutage", "RfInterference"),
            ("PacketLossPath", "SpuriousRetransmission"),
            ("PacketLossPath", "RadioChannelDegradation"),
            ("BufferbloatQueue", "MiddleboxInterference"),
            ("PmtudPathIssue", "PacketLossPath"),
        ]
    );
}

#[test]
fn field_indices_are_deterministic() {
    let event = resolve_event("tcp.retransmission_burst").expect("event should exist");
    let idx1 = event
        .fields
        .iter()
        .find(|field| field.name == "segment_size")
        .expect("segment_size should exist")
        .idx;
    let idx2 = resolve_event("tcp.retransmission_burst")
        .expect("event should exist")
        .fields
        .iter()
        .find(|field| field.name == "segment_size")
        .expect("segment_size should exist")
        .idx;
    assert_eq!(idx1, idx2);

    // Every event declares contiguous field indices: idx == declaration position.
    for schema in events() {
        for (position, field) in schema.fields.iter().enumerate() {
            assert_eq!(
                field.idx.0 as usize, position,
                "event {} field {} should have idx == position {position}",
                schema.name, field.name
            );
        }
    }
}

#[test]
fn pii_markers_match_catalog_annotations() {
    let rtx = resolve_event("tcp.retransmission_burst").expect("event should exist");
    let dst_ip = rtx
        .fields
        .iter()
        .find(|field| field.name == "dst_ip")
        .expect("dst_ip should exist");
    let src_ip = rtx
        .fields
        .iter()
        .find(|field| field.name == "src_ip")
        .expect("src_ip should exist");
    assert!(dst_ip.pii);
    assert!(src_ip.pii);

    let deauth = resolve_event("wifi.deauth_burst").expect("event should exist");
    assert!(
        deauth
            .fields
            .iter()
            .find(|field| field.name == "bssid")
            .expect("bssid")
            .pii
    );
    assert!(
        deauth
            .fields
            .iter()
            .find(|field| field.name == "client_macs")
            .expect("client_macs")
            .pii
    );

    let dhcp = resolve_event("dhcp.timeout").expect("event should exist");
    assert!(
        dhcp.fields
            .iter()
            .find(|field| field.name == "client_mac")
            .expect("client_mac")
            .pii
    );
}

#[test]
fn exposes_catalog_metadata_and_kind_tables() {
    let cref = catalog_ref();
    assert_eq!(cref.id.as_ref(), "airpulse.catalog");
    assert_eq!(cref.version.as_ref(), "1.3");

    assert_eq!(
        observation_kinds(),
        &["icmp.visibility", "aaa.telemetry", "wifi.rf_metrics"]
    );
    assert_eq!(
        check_kinds(),
        &["cable_loopback", "lldp_poll", "stp_root_check"]
    );

    assert!(capability_for("topology").is_some());
    assert_eq!(capabilities().len(), 4);
}

#[test]
fn suppress_symptom_schema_matches_problem_ref_contract() {
    let schema = resolve_action("suppress_symptom").expect("suppress_symptom action should exist");
    assert_eq!(schema.arg_kind, ActionArgKind::ProblemRefBinding);
    assert!(
        schema.target_types.is_empty(),
        "suppress_symptom must not declare explicit target types"
    );
}

#[test]
fn l3_flag_events_resolve_with_count_field() {
    let names = [
        "icmp.unreach",
        "icmp.ttl_exceeded",
        "icmp.redirect",
        "icmp.tcp_mss",
        "icmp.tcp_blackhole",
        "arp.storm",
        "arp.dup_ip",
        "ndp.rs_only",
        "dhcp.ether_issue",
        "dhcp.dns_mismatch",
        "dhcpv6.ether_issue",
        "dns.onboard_anomaly",
        "dns.tls_name_mismatch",
        "tls.slow_handshake",
        "tls.alerts",
        "http.redirect_burst",
        "http.errors",
        "quic.tcp_coexist",
        "dot1x.wired",
        "stp.tcn",
        "stp.tcp_burst",
        "igmp.mld_heavy",
        "zeroconf.udp_burst",
        "vlan.qinq",
        "ntp.udp_heavy",
        "lldp.seen",
        "tunnel.icmp_mtu",
    ];
    assert_eq!(names.len(), 27, "the L3 flag family has 27 events");
    for name in names {
        let schema = resolve_event(name).expect("event should exist");
        // Exactly target/time/count with contiguous indices.
        assert_eq!(
            schema.fields.len(),
            3,
            "{name} should have exactly 3 fields"
        );
        for (position, expected) in ["target", "time", "count"].iter().enumerate() {
            let field = &schema.fields[position];
            assert_eq!(
                field.name, *expected,
                "{name} field {position} should be {expected}"
            );
            assert_eq!(
                field.idx.0 as usize, position,
                "{name} field {expected} should have contiguous idx {position}"
            );
        }
        // Session scope routing via `target`.
        assert_eq!(
            schema.routing_paths.len(),
            1,
            "{name} should have one routing path"
        );
        let route = &schema.routing_paths[0];
        assert_eq!(route.path, "target", "{name} should route via target");
        assert_eq!(
            route.scope,
            airpulse_dsl_types::ScopeType::Session,
            "{name} should route to Session scope"
        );
    }
}

#[test]
fn protocol_summary_events_resolve_global_scope() {
    for name in [
        "radius.summary",
        "diameter.summary",
        "gtp.summary",
        "dhcp.summary",
    ] {
        let schema = resolve_event(name).expect("event should exist");
        assert!(schema.routing_paths.iter().any(|r| r.path == "target"));
    }
}

#[test]
fn wifi_w0_catalog_bijection_and_legacy_stability() {
    let mappings = wifi_w0_mappings();
    assert_eq!(mappings.len(), 83);
    let sarif_ids: Vec<&str> = mappings.iter().map(|m| m.sarif_id).collect();
    let unique: std::collections::BTreeSet<&str> = sarif_ids.iter().copied().collect();
    assert_eq!(unique.len(), 83, "sarif ids must be unique");

    for row in mappings {
        let problem = resolve_problem(row.problem_kind).unwrap_or_else(|| {
            panic!(
                "problem {} for {} should resolve",
                row.problem_kind, row.sarif_id
            )
        });
        assert_eq!(problem.default_sarif_id.as_str(), row.sarif_id);
        assert!(
            problem.valid_scopes.contains(&row.scope),
            "{} scope {:?}",
            row.problem_kind,
            row.scope
        );

        let cause = resolve_cause(row.cause_kind).unwrap_or_else(|| {
            panic!(
                "cause {} for {} should resolve",
                row.cause_kind, row.sarif_id
            )
        });
        assert!(
            cause.valid_scopes.contains(&row.scope),
            "{} cause scope {:?}",
            row.cause_kind,
            row.scope
        );
    }

    // Legacy compatibility entries remain (not part of the 83).
    assert!(resolve_problem("WlanRadiusOutage").is_some());
    assert!(resolve_problem("AmbiguousDiagnosis").is_some());
    assert_eq!(catalog_ref().version.as_ref(), "1.3");

    // PII on MAC/BSSID fields for raw Wi-Fi events.
    let burst = resolve_event("wifi.deauth_burst").expect("wifi.deauth_burst");
    let bssid = burst
        .fields
        .iter()
        .find(|f| f.name == "bssid")
        .expect("bssid field");
    assert!(bssid.pii);
}

#[test]
fn wifi_event_vocabulary_stays_within_the_w1_budget() {
    let raw = [
        "wifi.mgmt.beacon",
        "wifi.mgmt.probe_req",
        "wifi.mgmt.probe_resp",
        "wifi.mgmt.auth",
        "wifi.mgmt.assoc_req",
        "wifi.mgmt.assoc_resp",
        "wifi.mgmt.deauth",
        "wifi.mgmt.action",
        "wifi.mgmt.cts",
        "wifi.ctrl.rts",
        "wifi.ctrl.bar",
        "wifi.data.frame",
        "wifi.eapol.frame",
    ];
    let outcomes = [
        "wifi.client.auth_outcome",
        "wifi.client.eap_outcome",
        "wifi.client.roam_outcome",
        "wifi.client.dhcp_outcome",
        "wifi.client.dhcpv6_outcome",
        "wifi.client.dns_outcome",
        "wifi.client.assoc_pending",
        "wifi.client.sae_outcome",
        "wifi.client.twt_outcome",
    ];
    let summaries = ["wifi.ap.summary", "wifi.client.summary"];
    let specials = [
        "wifi.deauth_burst",
        "wifi.rf_telemetry",
        "tcp.rtt_sample",
        "tcp.throughput_sample",
    ];
    let approved: std::collections::BTreeSet<&str> = raw
        .into_iter()
        .chain(outcomes)
        .chain(summaries)
        .chain(specials)
        .collect();
    let actual: std::collections::BTreeSet<&str> = events()
        .iter()
        .map(|event| event.name)
        .filter(|name| {
            name.starts_with("wifi.")
                || *name == "tcp.rtt_sample"
                || *name == "tcp.throughput_sample"
        })
        .collect();

    assert_eq!(raw.len(), 13);
    assert_eq!(outcomes.len(), 9);
    assert_eq!(summaries.len(), 2);
    assert_eq!(specials.len(), 4);
    assert_eq!(actual, approved, "unapproved Wi-Fi vocabulary entry");
}

#[test]
fn principal_wifi_summaries_cover_w0_fields_and_pii() {
    let ap_fields = [
        "gap_us",
        "expected_interval_us",
        "missed_count",
        "jitter_centi_ratio",
        "sample_count",
        "probe_age_ms",
        "beacon_age_ms",
        "beacon_rates_hash",
        "assoc_rates_hash",
        "consecutive_count",
        "max_gap_ms",
        "bar_count",
        "window_ms",
        "peer_rates_hash",
        "rates_hash",
        "ap_count",
        "has_rsn",
        "max_rate_kbps",
        "has_ht_cap",
        "streak",
        "channel_count",
        "rssi_dbm",
        "delta_db",
        "frame_count",
        "retry_permille",
        "weak_security",
        "client_count",
        "target_is_dfs",
        "ssid_count",
        "dtim_period",
        "util_centipercent",
        "security_fingerprint_count",
        "active_sta_count",
        "min_sta_frames",
        "ap_retry_permille",
        "client_retry_mean_permille",
        "peer_bssid_count",
        "high_co_channel_present",
        "is_6ghz",
        "has_he_cap",
        "eht_width_code",
        "has_eht_cap",
        "has_puncturing_bitmap",
        "has_visible_ssid",
        "has_supported_rates",
        "collision",
        "unencrypted_traffic_leak",
    ];
    let client_fields = [
        "streak",
        "ps_poll_count",
        "state_code",
        "count",
        "window_ms",
        "connected",
        "snr_db",
        "rate_kbps",
        "data_frame_count",
        "absence_ms",
        "has_ul",
        "has_dl",
        "ps_beacon_streak",
        "ft",
        "pmkid_count",
        "protected",
        "assoc_age_ms",
        "tid",
        "rssi_dbm",
        "neighbor_delta_db",
        "distinct_ap_count",
        "probe_count",
        "probe_5g_count",
        "probe_24g_count",
        "associated_5g",
        "failed_count",
        "ft_or_bss_tm",
        "seq_delta",
        "retry",
        "client_generation_code",
        "ap_generation_code",
        "mlo_present",
        "saw_6ghz",
        "saw_psc",
        "unique_servfail_count",
        "nxdomain_count",
        "server_rst_count",
        "ap_retry_permille",
        "auth_latency_ms",
    ];

    for (event_name, required) in [
        ("wifi.ap.summary", ap_fields.as_slice()),
        ("wifi.client.summary", client_fields.as_slice()),
    ] {
        let schema = resolve_event(event_name).expect("principal summary must resolve");
        let names: std::collections::BTreeSet<&str> =
            schema.fields.iter().map(|field| field.name).collect();
        for field in required {
            assert!(names.contains(field), "{event_name}.{field} missing");
        }
        for (position, field) in schema.fields.iter().enumerate() {
            assert_eq!(
                field.idx.0 as usize, position,
                "{event_name}.{}",
                field.name
            );
        }
    }

    let ap = resolve_event("wifi.ap.summary").expect("AP summary");
    assert!(
        ap.fields
            .iter()
            .any(|field| field.name == "bssid" && field.pii)
    );
    let client = resolve_event("wifi.client.summary").expect("client summary");
    assert!(
        client
            .fields
            .iter()
            .any(|field| field.name == "client_mac" && field.pii)
    );
}

#[test]
fn w1_cross_layer_paths_resolve_with_append_only_indices() {
    let session = resolve_event("tcp.session_summary").expect("session summary");
    for (field, expected_idx) in [
        ("loss_rate_pm", 1),
        ("rtt_mean_ms", 5),
        ("throughput_mean_kbps", 14),
        ("completeness", 18),
        ("rst_count", 21),
        ("rto_count", 23),
        ("duration_ms", 24),
        ("time", 26),
        ("lost_packets", 27),
        ("packet_count", 28),
        ("session_is_ipv6", 29),
    ] {
        let actual = session
            .fields
            .iter()
            .find(|candidate| candidate.name == field)
            .unwrap_or_else(|| panic!("tcp.session_summary.{field} missing"));
        assert_eq!(actual.idx.0, expected_idx, "tcp.session_summary.{field}");
    }

    let rtt = resolve_event("tcp.rtt_sample").expect("RTT sample");
    let rtt_fields: Vec<&str> = rtt.fields.iter().map(|field| field.name).collect();
    assert_eq!(rtt_fields, ["target", "time", "rtt_ms", "baseline_rtt_ms"]);

    let ap = resolve_event("wifi.ap.summary").expect("AP summary");
    assert_eq!(
        ap.fields.last().expect("AP fields").name,
        "unencrypted_traffic_leak"
    );
    let client = resolve_event("wifi.client.summary").expect("client summary");
    assert_eq!(
        client
            .fields
            .iter()
            .find(|field| field.name == "auth_latency_ms")
            .expect("W1 tail")
            .idx
            .0,
        41
    );
}

#[test]
fn w2_append_only_wifi_fields_are_distinct_and_stable() {
    let client = resolve_event("wifi.client.summary").expect("client summary");
    let original_last = client
        .fields
        .iter()
        .find(|field| field.name == "auth_latency_ms")
        .expect("W1 tail");
    assert_eq!(original_last.idx.0, 41, "W1 indices must not move");
    for (offset, name) in [
        "summary_kind_code",
        "probe_count_10s",
        "deauth_count_30s",
        "disconnect_count_60s",
        "distinct_ap_count_60s",
        "emlsr_transition_count_60s",
        "band_probe_count_60s",
        "band_probe_5g_count_60s",
        "band_probe_24g_count_60s",
        "psc_probe_count_120s",
        "dns_servfail_unique_count",
        "dns_nxdomain_count",
        "captive_rst_count_120s",
        "ft_failure_count_60s",
        "tid_has_ul",
        "tid_has_dl",
        "saw_eapol",
    ]
    .iter()
    .enumerate()
    {
        let field = client
            .fields
            .iter()
            .find(|field| field.name == *name)
            .unwrap_or_else(|| panic!("wifi.client.summary.{name} missing"));
        assert_eq!(field.idx.0, 42 + offset as u16, "{name}");
    }

    let raw = resolve_event("wifi.mgmt.deauth").expect("raw deauth");
    let bssid = raw
        .fields
        .iter()
        .find(|field| field.name == "bssid")
        .expect("raw deauth bssid");
    assert_eq!(bssid.idx.0, 7);
    assert!(bssid.pii);
    let ap_target = raw
        .fields
        .iter()
        .find(|field| field.name == "ap_target")
        .expect("raw deauth ap_target");
    assert_eq!(ap_target.idx.0, 8);
    assert_eq!(raw.routing_paths.len(), 2);
    assert!(
        raw.routing_paths
            .iter()
            .any(|route| route.path == "ap_target")
    );

    let action = resolve_event("wifi.mgmt.action").expect("raw action");
    let client_target = action
        .fields
        .iter()
        .find(|field| field.name == "client_target")
        .expect("raw action client_target");
    assert_eq!(client_target.idx.0, 7);
    assert_eq!(action.routing_paths.len(), 2);
    assert!(
        action
            .routing_paths
            .iter()
            .any(|route| route.path == "client_target")
    );

    let burst = resolve_event("wifi.deauth_burst").expect("burst");
    assert_eq!(
        burst
            .fields
            .iter()
            .find(|field| field.name == "window_ms")
            .expect("window_ms")
            .idx
            .0,
        5
    );
}

#[test]
fn w1_documented_raw_event_fields_resolve_exactly() {
    let documented = [
        (
            "wifi.mgmt.beacon",
            &[
                "target",
                "time",
                "channel",
                "rssi_dbm",
                "dtim_period",
                "has_rsn",
            ][..],
        ),
        ("wifi.mgmt.probe_req", &["target", "time", "freq_mhz"][..]),
        ("wifi.mgmt.probe_resp", &["target", "time", "rssi_dbm"][..]),
        (
            "wifi.mgmt.auth",
            &["target", "time", "bssid", "seq", "status", "algorithm"][..],
        ),
        (
            "wifi.mgmt.assoc_req",
            &["target", "time", "bssid", "ft_pmkid_hint"][..],
        ),
        ("wifi.mgmt.assoc_resp", &["target", "time", "status"][..]),
        (
            "wifi.mgmt.deauth",
            &[
                "target",
                "time",
                "initiator",
                "reason",
                "protected",
                "is_deauth_or_disassoc",
                "initiator_code",
                "bssid",
                "ap_target",
            ][..],
        ),
        (
            "wifi.mgmt.action",
            &[
                "target",
                "time",
                "category",
                "protected",
                "initiator_code",
                "associated",
                "csa_present",
                "client_target",
            ][..],
        ),
        ("wifi.mgmt.cts", &["target", "time", "nav_us"][..]),
        ("wifi.ctrl.rts", &["target", "time"][..]),
        ("wifi.ctrl.bar", &["target", "time"][..]),
        (
            "wifi.data.frame",
            &[
                "target",
                "time",
                "retry",
                "protected",
                "tid",
                "rate_kbps",
                "rssi_dbm",
            ][..],
        ),
        ("wifi.eapol.frame", &["target", "time", "frame_type"][..]),
    ];

    for (event_name, expected) in documented {
        let schema = resolve_event(event_name).unwrap_or_else(|| panic!("{event_name} missing"));
        let actual: Vec<&str> = schema.fields.iter().map(|field| field.name).collect();
        assert_eq!(actual, expected, "{event_name} documented field drift");
    }
}
