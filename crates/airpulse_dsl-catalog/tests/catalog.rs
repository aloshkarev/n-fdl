use airpulse_dsl_catalog::{
    ActionArgKind, EventOrBindingType, FieldType, capabilities, capability_for, catalog_ref, check_kinds,
    exclusivity_defaults, observation_kinds, resolve_action, resolve_cause, resolve_event, resolve_metric_path,
    resolve_problem, resolve_topo_fn,
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
        assert!(resolve_problem(name).is_some(), "problem {name} should resolve");
    }

    let actions = [
        "request_observation",
        "run_check",
        "suppress_symptom",
        "mark_ambiguous",
        "request_topology",
    ];
    for name in actions {
        assert!(resolve_action(name).is_some(), "action {name} should resolve");
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
        assert!(resolve_topo_fn(name).is_some(), "topology function {name} should resolve");
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
    assert!(deauth.fields.iter().find(|field| field.name == "bssid").expect("bssid").pii);
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
    assert_eq!(cref.version.as_ref(), "1.0");

    assert_eq!(observation_kinds(), &["icmp.visibility", "aaa.telemetry", "wifi.rf_metrics"]);
    assert_eq!(check_kinds(), &["cable_loopback", "lldp_poll", "stp_root_check"]);

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
