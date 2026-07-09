//! Static ADGL catalog data and name-resolution API.
//!
//! This crate is the catalog authority referenced by
//! `docs/idea/spec/10-catalog-abi.md` and consumed by verifier/lowering phases
//! (`docs/idea/spec/05-verification.md` §1/§1.1).

#![deny(unsafe_code)]
#![warn(missing_docs)]

use std::sync::LazyLock;

use airpulse_dsl_ir::{CatalogRef, FieldIdx, TopoFuncIdx};
use airpulse_dsl_types::{ActionKind, Capability, CauseKind, EventType, ProblemKind, SarifId, ScopeType, Severity};

/// Stable catalog id used by `ProgramImage.catalog_ref.id`
/// (`06-ir-bytecode.md` §2 catalog_ref).
pub const CATALOG_ID: &str = "airpulse.catalog";

/// Stable catalog version used by `ProgramImage.catalog_ref.version`
/// (`06-ir-bytecode.md` §2 catalog_ref).
pub const CATALOG_VERSION: &str = "1.0";

/// Field type carried by the catalog (`04-type-system.md` §1/§6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FieldType {
    /// `Int` scalar.
    Int,
    /// `String` scalar.
    String,
    /// `Severity` enum.
    Severity,
    /// `Confidence` scalar (0..100).
    Confidence,
    /// Stable symbolic SARIF id.
    SarifId,
    /// `ScopeId(scope_type)`.
    ScopeId(ScopeType),
    /// `List<ScopeId>`.
    ScopeIdList,
    /// `List<Int>`.
    IntList,
    /// `List<NodeId>`.
    NodeIdList,
}

/// One named field inside an event/cause/problem schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FieldSchema {
    /// DSL field name.
    pub name: &'static str,
    /// Field type.
    pub field_type: FieldType,
    /// Privacy marker from `10-catalog-abi.md` `[pii]`.
    pub pii: bool,
    /// Stable field index used by IR opcodes (`06-ir-bytecode.md` §6).
    pub idx: FieldIdx,
}

/// Route from an event payload to a scope key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScopeRoute {
    /// Scope type this path can route to.
    pub scope: ScopeType,
    /// Field path (e.g. `target`, `vlan`, `path`).
    pub path: &'static str,
}

/// Event catalog schema (`10-catalog-abi.md` §2).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EventSchema {
    /// Surface event name, e.g. `tcp.retransmission_burst`.
    pub name: &'static str,
    /// Typed event id wrapper.
    pub event_type: EventType,
    /// Declared fields in deterministic order.
    pub fields: Box<[FieldSchema]>,
    /// Scope routing paths available from this event.
    pub routing_paths: Box<[ScopeRoute]>,
}

/// Cause catalog schema (`10-catalog-abi.md` §3).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CauseSchema {
    /// Surface cause kind, e.g. `PmtudBlackhole`.
    pub name: &'static str,
    /// Typed cause id wrapper.
    pub kind: CauseKind,
    /// Valid target scopes from the catalog.
    pub valid_scopes: Box<[ScopeType]>,
    /// Optional default severity if catalog-spec'd.
    pub default_severity: Option<Severity>,
}

/// Problem catalog schema (`10-catalog-abi.md` §4).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProblemSchema {
    /// Surface problem kind.
    pub name: &'static str,
    /// Typed problem id wrapper.
    pub kind: ProblemKind,
    /// Valid target scopes.
    pub valid_scopes: Box<[ScopeType]>,
    /// Default SARIF id.
    pub default_sarif_id: SarifId,
    /// Optional default severity.
    pub severity: Option<Severity>,
}

/// Action target argument type (`10-catalog-abi.md` §5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ActionTargetType {
    /// `ScopeId`.
    ScopeId,
    /// `List<ScopeId>`.
    ScopeIdList,
}

/// Action argument contract (`05-verification.md` §1.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ActionArgKind {
    /// Catalog observation kind (`request_observation`).
    ObservationKind,
    /// Catalog check kind (`run_check`).
    CheckKind,
    /// In-scope `ProblemRef` binding (`suppress_symptom`).
    ProblemRefBinding,
    /// No argument.
    None,
}

/// Action catalog schema (`10-catalog-abi.md` §5).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ActionSchema {
    /// Action kind.
    pub kind: ActionKind,
    /// DSL keyword spelling.
    pub name: &'static str,
    /// Argument contract for verifier.
    pub arg_kind: ActionArgKind,
    /// Allowed target value types.
    pub target_types: Box<[ActionTargetType]>,
    /// Allowed kind symbols for kind-bearing actions.
    pub allowed_kinds: Box<[&'static str]>,
}

/// Topology function argument type (`10-catalog-abi.md` §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TopoArgType {
    /// `ScopeId`.
    ScopeId,
}

/// Topology function schema (`10-catalog-abi.md` §6).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TopoFnSchema {
    /// Function name in DSL (`same_session`, ...).
    pub name: &'static str,
    /// Stable IR function index.
    pub func_idx: TopoFuncIdx,
    /// Arity.
    pub arity: usize,
    /// Argument types.
    pub arg_types: Box<[TopoArgType]>,
}

/// One default exclusivity pair from `catalog.exclusivity_defaults`
/// (`10-catalog-abi.md` §7).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExclusivityPair {
    /// Left cause kind.
    pub left: CauseKind,
    /// Right cause kind.
    pub right: CauseKind,
}

/// Metric path root type for schema resolution (`05-verification.md` §1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventOrBindingType<'a> {
    /// Event binding schema.
    Event(&'a EventType),
    /// Cause binding schema.
    Cause(&'a CauseKind),
    /// Problem binding schema.
    Problem(&'a ProblemKind),
}

fn idx(n: u16) -> FieldIdx {
    FieldIdx(n)
}

fn event(name: &'static str, fields: &[FieldSchema], routes: &[ScopeRoute]) -> EventSchema {
    EventSchema {
        name,
        event_type: EventType::new(name),
        fields: fields.to_vec().into_boxed_slice(),
        routing_paths: routes.to_vec().into_boxed_slice(),
    }
}

fn cause(name: &'static str, scopes: &[ScopeType], default_severity: Option<Severity>) -> CauseSchema {
    CauseSchema {
        name,
        kind: CauseKind::new(name),
        valid_scopes: scopes.to_vec().into_boxed_slice(),
        default_severity,
    }
}

fn problem(
    name: &'static str,
    scopes: &[ScopeType],
    sarif_id: &'static str,
    severity: Option<Severity>,
) -> ProblemSchema {
    ProblemSchema {
        name,
        kind: ProblemKind::new(name),
        valid_scopes: scopes.to_vec().into_boxed_slice(),
        default_sarif_id: SarifId::new(sarif_id),
        severity,
    }
}

static EVENTS: LazyLock<Box<[EventSchema]>> = LazyLock::new(|| {
    Box::new([
        // 10-catalog-abi.md §2 `tcp.retransmission_burst`.
        event(
            "tcp.retransmission_burst",
            &[
                FieldSchema { name: "segment_size", field_type: FieldType::Int, pii: false, idx: idx(0) },
                FieldSchema {
                    name: "target",
                    field_type: FieldType::ScopeId(ScopeType::Session),
                    pii: false,
                    idx: idx(1),
                },
                // Inference: `time` is untyped in §2 event listings, but 04 §2/§6
                // models event-time as `Int` milliseconds.
                FieldSchema { name: "time", field_type: FieldType::Int, pii: false, idx: idx(2) },
                FieldSchema {
                    name: "vlan",
                    field_type: FieldType::ScopeId(ScopeType::Vlan),
                    pii: false,
                    idx: idx(3),
                },
                FieldSchema { name: "path", field_type: FieldType::ScopeIdList, pii: false, idx: idx(4) },
                FieldSchema { name: "dst_ip", field_type: FieldType::Int, pii: true, idx: idx(5) },
                FieldSchema { name: "src_ip", field_type: FieldType::Int, pii: true, idx: idx(6) },
            ],
            &[
                ScopeRoute { scope: ScopeType::Session, path: "target" },
                ScopeRoute { scope: ScopeType::Vlan, path: "vlan" },
                ScopeRoute { scope: ScopeType::Vlan, path: "path" },
            ],
        ),
        // 10-catalog-abi.md §2 `icmp.ptb`.
        event(
            "icmp.ptb",
            &[
                FieldSchema {
                    name: "target",
                    field_type: FieldType::ScopeId(ScopeType::Session),
                    pii: false,
                    idx: idx(0),
                },
                // Inference: `time` is untyped in §2 event listings; modelled as
                // `Int` event-time in 04 §2/§6.
                FieldSchema { name: "time", field_type: FieldType::Int, pii: false, idx: idx(1) },
                FieldSchema { name: "quoted_mtu", field_type: FieldType::Int, pii: false, idx: idx(2) },
                FieldSchema { name: "path", field_type: FieldType::ScopeIdList, pii: false, idx: idx(3) },
                FieldSchema { name: "dst_ip", field_type: FieldType::Int, pii: true, idx: idx(4) },
            ],
            &[ScopeRoute { scope: ScopeType::Session, path: "target" }],
        ),
        // 10-catalog-abi.md §2 `wifi.deauth_burst`.
        event(
            "wifi.deauth_burst",
            &[
                FieldSchema {
                    name: "target",
                    field_type: FieldType::ScopeId(ScopeType::AccessPoint),
                    pii: false,
                    idx: idx(0),
                },
                // Inference: `time` is untyped in §2 event listings; modelled as
                // `Int` event-time in 04 §2/§6.
                FieldSchema { name: "time", field_type: FieldType::Int, pii: false, idx: idx(1) },
                FieldSchema { name: "count", field_type: FieldType::Int, pii: false, idx: idx(2) },
                // Inference: `bssid` is untyped in §2. 04 §2 has no MAC scalar, and
                // examples treat AP ids as numeric identifiers, so store as `Int`.
                FieldSchema { name: "bssid", field_type: FieldType::Int, pii: true, idx: idx(3) },
                // Inference: `client_macs` is declared `List<Int>` in §2.
                FieldSchema { name: "client_macs", field_type: FieldType::IntList, pii: true, idx: idx(4) },
            ],
            &[ScopeRoute { scope: ScopeType::AccessPoint, path: "target" }],
        ),
        // 10-catalog-abi.md §2 `wifi.rf_telemetry`.
        event(
            "wifi.rf_telemetry",
            &[
                FieldSchema {
                    name: "target",
                    field_type: FieldType::ScopeId(ScopeType::AccessPoint),
                    pii: false,
                    idx: idx(0),
                },
                // Inference: `time` is untyped in §2 event listings; modelled as
                // `Int` event-time in 04 §2/§6.
                FieldSchema { name: "time", field_type: FieldType::Int, pii: false, idx: idx(1) },
                FieldSchema { name: "rssi", field_type: FieldType::Int, pii: false, idx: idx(2) },
                FieldSchema { name: "noise", field_type: FieldType::Int, pii: false, idx: idx(3) },
                FieldSchema { name: "channel", field_type: FieldType::Int, pii: false, idx: idx(4) },
            ],
            &[ScopeRoute { scope: ScopeType::AccessPoint, path: "target" }],
        ),
        // 10-catalog-abi.md §2 `stp.topology_change`.
        event(
            "stp.topology_change",
            &[
                FieldSchema {
                    name: "target",
                    field_type: FieldType::ScopeId(ScopeType::Vlan),
                    pii: false,
                    idx: idx(0),
                },
                // Inference: `time` is untyped in §2 event listings; modelled as
                // `Int` event-time in 04 §2/§6.
                FieldSchema { name: "time", field_type: FieldType::Int, pii: false, idx: idx(1) },
                FieldSchema {
                    name: "vlan",
                    field_type: FieldType::ScopeId(ScopeType::Vlan),
                    pii: false,
                    idx: idx(2),
                },
            ],
            &[ScopeRoute { scope: ScopeType::Vlan, path: "target" }],
        ),
        // 10-catalog-abi.md §2 `dhcp.timeout`.
        event(
            "dhcp.timeout",
            &[
                FieldSchema {
                    name: "target",
                    field_type: FieldType::ScopeId(ScopeType::ClientMac),
                    pii: false,
                    idx: idx(0),
                },
                // Inference: `time` is untyped in §2 event listings; modelled as
                // `Int` event-time in 04 §2/§6.
                FieldSchema { name: "time", field_type: FieldType::Int, pii: false, idx: idx(1) },
                FieldSchema {
                    name: "vlan",
                    field_type: FieldType::ScopeId(ScopeType::Vlan),
                    pii: false,
                    idx: idx(2),
                },
                // Inference: `client_mac` is untyped in §2. 04 §2 lacks a dedicated
                // MAC scalar, so represent identifiers as `Int`.
                FieldSchema { name: "client_mac", field_type: FieldType::Int, pii: true, idx: idx(3) },
            ],
            &[
                ScopeRoute { scope: ScopeType::ClientMac, path: "target" },
                ScopeRoute { scope: ScopeType::Vlan, path: "vlan" },
            ],
        ),
        // 10-catalog-abi.md §2 `radius.access_request`.
        event(
            "radius.access_request",
            &[
                FieldSchema {
                    name: "target",
                    field_type: FieldType::ScopeId(ScopeType::ClientMac),
                    pii: false,
                    idx: idx(0),
                },
                // Inference: `time` is untyped in §2 event listings; modelled as
                // `Int` event-time in 04 §2/§6.
                FieldSchema { name: "time", field_type: FieldType::Int, pii: false, idx: idx(1) },
                FieldSchema {
                    name: "vlan",
                    field_type: FieldType::ScopeId(ScopeType::Vlan),
                    pii: false,
                    idx: idx(2),
                },
            ],
            &[
                ScopeRoute { scope: ScopeType::ClientMac, path: "target" },
                ScopeRoute { scope: ScopeType::Vlan, path: "vlan" },
            ],
        ),
        // 10-catalog-abi.md §2 `dot1x.eapol_start`.
        event(
            "dot1x.eapol_start",
            &[
                FieldSchema {
                    name: "target",
                    field_type: FieldType::ScopeId(ScopeType::ClientMac),
                    pii: false,
                    idx: idx(0),
                },
                // Inference: `time` is untyped in §2 event listings; modelled as
                // `Int` event-time in 04 §2/§6.
                FieldSchema { name: "time", field_type: FieldType::Int, pii: false, idx: idx(1) },
                FieldSchema {
                    name: "vlan",
                    field_type: FieldType::ScopeId(ScopeType::Vlan),
                    pii: false,
                    idx: idx(2),
                },
            ],
            &[
                ScopeRoute { scope: ScopeType::ClientMac, path: "target" },
                ScopeRoute { scope: ScopeType::Vlan, path: "vlan" },
            ],
        ),
        // 10-catalog-abi.md §2 `port.crc_errors`.
        event(
            "port.crc_errors",
            &[
                FieldSchema {
                    name: "target",
                    field_type: FieldType::ScopeId(ScopeType::Port),
                    pii: false,
                    idx: idx(0),
                },
                // Inference: `time` is untyped in §2 event listings; modelled as
                // `Int` event-time in 04 §2/§6.
                FieldSchema { name: "time", field_type: FieldType::Int, pii: false, idx: idx(1) },
                FieldSchema { name: "count", field_type: FieldType::Int, pii: false, idx: idx(2) },
            ],
            &[ScopeRoute { scope: ScopeType::Port, path: "target" }],
        ),
        // 10-catalog-abi.md §2 `port.link_flap`.
        event(
            "port.link_flap",
            &[
                FieldSchema {
                    name: "target",
                    field_type: FieldType::ScopeId(ScopeType::Port),
                    pii: false,
                    idx: idx(0),
                },
                // Inference: `time` is untyped in §2 event listings; modelled as
                // `Int` event-time in 04 §2/§6.
                FieldSchema { name: "time", field_type: FieldType::Int, pii: false, idx: idx(1) },
                FieldSchema { name: "count", field_type: FieldType::Int, pii: false, idx: idx(2) },
            ],
            &[ScopeRoute { scope: ScopeType::Port, path: "target" }],
        ),
        // 10-catalog-abi.md §2 `port.admin_state`.
        event(
            "port.admin_state",
            &[
                FieldSchema {
                    name: "target",
                    field_type: FieldType::ScopeId(ScopeType::Port),
                    pii: false,
                    idx: idx(0),
                },
                // Inference: `time` is untyped in §2 event listings; modelled as
                // `Int` event-time in 04 §2/§6.
                FieldSchema { name: "time", field_type: FieldType::Int, pii: false, idx: idx(1) },
                FieldSchema { name: "state", field_type: FieldType::String, pii: false, idx: idx(2) },
            ],
            &[ScopeRoute { scope: ScopeType::Port, path: "target" }],
        ),
        // 10-catalog-abi.md §2 `port.oper_state`.
        event(
            "port.oper_state",
            &[
                FieldSchema {
                    name: "target",
                    field_type: FieldType::ScopeId(ScopeType::Port),
                    pii: false,
                    idx: idx(0),
                },
                // Inference: `time` is untyped in §2 event listings; modelled as
                // `Int` event-time in 04 §2/§6.
                FieldSchema { name: "time", field_type: FieldType::Int, pii: false, idx: idx(1) },
                FieldSchema { name: "state", field_type: FieldType::String, pii: false, idx: idx(2) },
            ],
            &[ScopeRoute { scope: ScopeType::Port, path: "target" }],
        ),
    ])
});

static CAUSES: LazyLock<Box<[CauseSchema]>> = LazyLock::new(|| {
    Box::new([
        // 10-catalog-abi.md §3 `PmtudBlackhole`.
        cause("PmtudBlackhole", &[ScopeType::Session, ScopeType::Vlan], None),
        // 10-catalog-abi.md §3 `Congestion`.
        cause("Congestion", &[ScopeType::Session], None),
        // 10-catalog-abi.md §3 `TransientL2Disruption`.
        cause("TransientL2Disruption", &[ScopeType::Session, ScopeType::Vlan], None),
        // 10-catalog-abi.md §3 `PhysicalCableDamage`.
        cause("PhysicalCableDamage", &[ScopeType::Port], None),
        // 10-catalog-abi.md §3 `AuthServerOutage`.
        cause("AuthServerOutage", &[ScopeType::Vlan], None),
        // 10-catalog-abi.md §3 `RfInterference`.
        cause("RfInterference", &[ScopeType::AccessPoint], None),
        // 10-catalog-abi.md §3 `UpstreamOutage`.
        cause("UpstreamOutage", &[ScopeType::Global], None),
        // 10-catalog-abi.md §3 `PhysicalLinkAbsent`.
        cause("PhysicalLinkAbsent", &[ScopeType::Port], None),
    ])
});

static PROBLEMS: LazyLock<Box<[ProblemSchema]>> = LazyLock::new(|| {
    Box::new([
        // 10-catalog-abi.md §4 `XlIcmpTcpMss`.
        problem("XlIcmpTcpMss", &[ScopeType::Session], "l3_pmtud_blackhole", None),
        // 10-catalog-abi.md §4 `CableDisconnected`.
        problem(
            "CableDisconnected",
            &[ScopeType::Port],
            "ap_port_cable_disconnected",
            None,
        ),
        // 10-catalog-abi.md §4 `SpanningTreeInstability`.
        problem(
            "SpanningTreeInstability",
            &[ScopeType::Vlan],
            "l3_stp_spanning_tree",
            None,
        ),
        // 10-catalog-abi.md §4 `ClientOnboardingFailure`.
        problem(
            "ClientOnboardingFailure",
            &[ScopeType::Vlan],
            "l3_dot1x_wired",
            None,
        ),
        // 10-catalog-abi.md §4 `WlanRadiusOutage`.
        problem("WlanRadiusOutage", &[ScopeType::Vlan], "ap_wlan_radius_outage", None),
        // 10-catalog-abi.md §4 `DeviceUnreachable`.
        problem("DeviceUnreachable", &[ScopeType::Global], "ap_device_unreachable", None),
        // 10-catalog-abi.md §4 `AmbiguousDiagnosis`.
        problem(
            "AmbiguousDiagnosis",
            &[
                ScopeType::Session,
                ScopeType::Port,
                ScopeType::ClientMac,
                ScopeType::Vlan,
                ScopeType::AccessPoint,
                ScopeType::Global,
            ],
            "ap_ambiguous",
            Some(Severity::Medium),
        ),
    ])
});

static ACTIONS: LazyLock<Box<[ActionSchema]>> = LazyLock::new(|| {
    Box::new([
        // 10-catalog-abi.md §5 `request_observation`.
        ActionSchema {
            kind: ActionKind::RequestObservation,
            name: "request_observation",
            arg_kind: ActionArgKind::ObservationKind,
            target_types: vec![ActionTargetType::ScopeId, ActionTargetType::ScopeIdList]
                .into_boxed_slice(),
            allowed_kinds: OBSERVATION_KINDS.to_vec().into_boxed_slice(),
        },
        // 10-catalog-abi.md §5 `run_check`.
        ActionSchema {
            kind: ActionKind::RunCheck,
            name: "run_check",
            arg_kind: ActionArgKind::CheckKind,
            target_types: vec![ActionTargetType::ScopeId].into_boxed_slice(),
            allowed_kinds: CHECK_KINDS.to_vec().into_boxed_slice(),
        },
        // 10-catalog-abi.md §5 `suppress_symptom`.
        ActionSchema {
            kind: ActionKind::SuppressSymptom,
            name: "suppress_symptom",
            arg_kind: ActionArgKind::ProblemRefBinding,
            target_types: Box::new([]),
            allowed_kinds: Box::new([]),
        },
        // 10-catalog-abi.md §5 `mark_ambiguous`.
        ActionSchema {
            kind: ActionKind::MarkAmbiguous,
            name: "mark_ambiguous",
            arg_kind: ActionArgKind::None,
            target_types: vec![ActionTargetType::ScopeId].into_boxed_slice(),
            allowed_kinds: Box::new([]),
        },
        // 10-catalog-abi.md §5 `request_topology`.
        ActionSchema {
            kind: ActionKind::RequestTopology,
            name: "request_topology",
            arg_kind: ActionArgKind::None,
            target_types: vec![ActionTargetType::ScopeId].into_boxed_slice(),
            allowed_kinds: Box::new([]),
        },
    ])
});

static TOPO_FUNCTIONS: LazyLock<Box<[TopoFnSchema]>> = LazyLock::new(|| {
    Box::new([
        // 10-catalog-abi.md §6 `same_session`.
        TopoFnSchema {
            name: "same_session",
            func_idx: TopoFuncIdx(0),
            arity: 2,
            arg_types: vec![TopoArgType::ScopeId, TopoArgType::ScopeId].into_boxed_slice(),
        },
        // 10-catalog-abi.md §6 `same_client`.
        TopoFnSchema {
            name: "same_client",
            func_idx: TopoFuncIdx(1),
            arity: 2,
            arg_types: vec![TopoArgType::ScopeId, TopoArgType::ScopeId].into_boxed_slice(),
        },
        // 10-catalog-abi.md §6 `same_port`.
        TopoFnSchema {
            name: "same_port",
            func_idx: TopoFuncIdx(2),
            arity: 2,
            arg_types: vec![TopoArgType::ScopeId, TopoArgType::ScopeId].into_boxed_slice(),
        },
        // 10-catalog-abi.md §6 `same_ap`.
        TopoFnSchema {
            name: "same_ap",
            func_idx: TopoFuncIdx(3),
            arity: 2,
            arg_types: vec![TopoArgType::ScopeId, TopoArgType::ScopeId].into_boxed_slice(),
        },
        // 10-catalog-abi.md §6 `same_vlan`.
        TopoFnSchema {
            name: "same_vlan",
            func_idx: TopoFuncIdx(4),
            arity: 2,
            arg_types: vec![TopoArgType::ScopeId, TopoArgType::ScopeId].into_boxed_slice(),
        },
        // 10-catalog-abi.md §6 `upstream_of`.
        TopoFnSchema {
            name: "upstream_of",
            func_idx: TopoFuncIdx(5),
            arity: 2,
            arg_types: vec![TopoArgType::ScopeId, TopoArgType::ScopeId].into_boxed_slice(),
        },
    ])
});

static CAPABILITIES: LazyLock<Box<[Capability]>> = LazyLock::new(|| {
    Box::new([
        // 10-catalog-abi.md §8
        Capability::new("l3-deep"),
        // 10-catalog-abi.md §8
        Capability::new("topology"),
        // 10-catalog-abi.md §8
        Capability::new("wifi-ota"),
        // 10-catalog-abi.md §8
        Capability::new("radio-nemo"),
    ])
});

// 10-catalog-abi.md §5 observation kinds.
static OBSERVATION_KINDS: [&str; 3] = ["icmp.visibility", "aaa.telemetry", "wifi.rf_metrics"];

// 10-catalog-abi.md §5 check kinds.
static CHECK_KINDS: [&str; 3] = ["cable_loopback", "lldp_poll", "stp_root_check"];

static EXCLUSIVITY_DEFAULTS: LazyLock<Box<[ExclusivityPair]>> = LazyLock::new(|| {
    Box::new([
        // 10-catalog-abi.md §7 pair 1.
        ExclusivityPair { left: CauseKind::new("Congestion"), right: CauseKind::new("PmtudBlackhole") },
        // 10-catalog-abi.md §7 pair 2.
        ExclusivityPair {
            left: CauseKind::new("Congestion"),
            right: CauseKind::new("TransientL2Disruption"),
        },
        // 10-catalog-abi.md §7 pair 3.
        ExclusivityPair {
            left: CauseKind::new("PmtudBlackhole"),
            right: CauseKind::new("TransientL2Disruption"),
        },
        // 10-catalog-abi.md §7 pair 4.
        ExclusivityPair {
            left: CauseKind::new("PhysicalCableDamage"),
            right: CauseKind::new("PhysicalLinkAbsent"),
        },
        // 10-catalog-abi.md §7 pair 5.
        ExclusivityPair { left: CauseKind::new("AuthServerOutage"), right: CauseKind::new("RfInterference") },
    ])
});

/// `c.target` field index in the catalog Cause schema (`04` §6.2).
pub const CAUSE_FIELD_TARGET: FieldIdx = FieldIdx(0);
/// `c.time` field index in the catalog Cause schema (`04` §6.2).
pub const CAUSE_FIELD_TIME: FieldIdx = FieldIdx(1);
/// `c.confidence` field index in the catalog Cause schema (`04` §6.2).
pub const CAUSE_FIELD_CONFIDENCE: FieldIdx = FieldIdx(2);

/// `p.target` field index in the catalog Problem schema (`04` §6.2).
pub const PROBLEM_FIELD_TARGET: FieldIdx = FieldIdx(0);
/// `p.time` field index in the catalog Problem schema (`04` §6.2).
pub const PROBLEM_FIELD_TIME: FieldIdx = FieldIdx(1);

/// Returns the static `CatalogRef` for ADGL `ProgramImage`.
#[must_use]
pub fn catalog_ref() -> CatalogRef {
    CatalogRef { id: CATALOG_ID.into(), version: CATALOG_VERSION.into() }
}

/// Resolve an event schema by catalog name (`10-catalog-abi.md` §2).
#[must_use]
pub fn resolve_event(name: &str) -> Option<&'static EventSchema> {
    EVENTS.iter().find(|schema| schema.name == name)
}

/// Resolve a cause schema by catalog name (`10-catalog-abi.md` §3).
#[must_use]
pub fn resolve_cause(name: &str) -> Option<&'static CauseSchema> {
    CAUSES.iter().find(|schema| schema.name == name)
}

/// Resolve a problem schema by catalog name (`10-catalog-abi.md` §4).
#[must_use]
pub fn resolve_problem(name: &str) -> Option<&'static ProblemSchema> {
    PROBLEMS.iter().find(|schema| schema.name == name)
}

/// Resolve an action schema by DSL keyword (`10-catalog-abi.md` §5).
#[must_use]
pub fn resolve_action(name: &str) -> Option<&'static ActionSchema> {
    ACTIONS.iter().find(|schema| schema.name == name)
}

/// Resolve a topology function schema by name (`10-catalog-abi.md` §6).
#[must_use]
pub fn resolve_topo_fn(name: &str) -> Option<&'static TopoFnSchema> {
    TOPO_FUNCTIONS.iter().find(|schema| schema.name == name)
}

/// Returns all catalog capabilities (`10-catalog-abi.md` §8).
#[must_use]
pub fn capabilities() -> &'static [Capability] {
    CAPABILITIES.as_ref()
}

/// Resolve one capability symbol.
#[must_use]
pub fn capability_for(name: &str) -> Option<&'static Capability> {
    CAPABILITIES.iter().find(|cap| cap.as_str() == name)
}

/// Returns all observation kinds (`10-catalog-abi.md` §5).
#[must_use]
pub fn observation_kinds() -> &'static [&'static str] {
    &OBSERVATION_KINDS
}

/// Returns all check kinds (`10-catalog-abi.md` §5).
#[must_use]
pub fn check_kinds() -> &'static [&'static str] {
    &CHECK_KINDS
}

/// Returns catalog default exclusivity pairs (`10-catalog-abi.md` §7).
#[must_use]
pub fn exclusivity_defaults() -> &'static [ExclusivityPair] {
    EXCLUSIVITY_DEFAULTS.as_ref()
}

/// Resolve a metric-path leaf against event/cause/problem schema (`05` §1).
///
/// `path` may be either a bare field (`segment_size`) or a binding-qualified
/// form (`rtx.segment_size`); only the final segment is resolved.
#[must_use]
pub fn resolve_metric_path(
    event_or_binding_type: EventOrBindingType<'_>,
    path: &str,
) -> Option<(FieldIdx, FieldType)> {
    let field_name = path.rsplit('.').next()?;
    let fields: &[FieldSchema] = match event_or_binding_type {
        EventOrBindingType::Event(event) => resolve_event(event.as_str())?.fields.as_ref(),
        EventOrBindingType::Cause(cause) => {
            if resolve_cause(cause.as_str()).is_none() {
                return None;
            }
            &CAUSE_FIELDS
        }
        EventOrBindingType::Problem(problem) => {
            if resolve_problem(problem.as_str()).is_none() {
                return None;
            }
            &PROBLEM_FIELDS
        }
    };
    fields
        .iter()
        .find(|field| field.name == field_name)
        .map(|field| (field.idx, field.field_type))
}

// 04-type-system.md §6.2 Cause schema.
static CAUSE_FIELDS: [FieldSchema; 4] = [
    FieldSchema {
        name: "target",
        field_type: FieldType::ScopeId(ScopeType::Global),
        pii: false,
        idx: CAUSE_FIELD_TARGET,
    },
    // Inference: cause `time` is modelled as `Int` in 04 §6.2.
    FieldSchema { name: "time", field_type: FieldType::Int, pii: false, idx: CAUSE_FIELD_TIME },
    FieldSchema {
        name: "confidence",
        field_type: FieldType::Confidence,
        pii: false,
        idx: CAUSE_FIELD_CONFIDENCE,
    },
    FieldSchema { name: "evidence", field_type: FieldType::NodeIdList, pii: false, idx: FieldIdx(3) },
];

// 04-type-system.md §6.2 Problem schema.
static PROBLEM_FIELDS: [FieldSchema; 5] = [
    FieldSchema {
        name: "target",
        field_type: FieldType::ScopeId(ScopeType::Global),
        pii: false,
        idx: PROBLEM_FIELD_TARGET,
    },
    // Inference: problem `time` is modelled as `Int` in 04 §6.2.
    FieldSchema { name: "time", field_type: FieldType::Int, pii: false, idx: PROBLEM_FIELD_TIME },
    FieldSchema {
        name: "severity",
        field_type: FieldType::Severity,
        pii: false,
        idx: FieldIdx(2),
    },
    FieldSchema { name: "evidence", field_type: FieldType::NodeIdList, pii: false, idx: FieldIdx(3) },
    FieldSchema { name: "sarif_id", field_type: FieldType::SarifId, pii: false, idx: FieldIdx(4) },
];
