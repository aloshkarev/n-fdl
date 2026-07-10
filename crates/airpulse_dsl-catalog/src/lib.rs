//! Static ADGL catalog data and name-resolution API.
//!
//! This crate is the catalog authority referenced by
//! `docs/idea/spec/10-catalog-abi.md` and consumed by verifier/lowering phases
//! (`docs/idea/spec/05-verification.md` §1/§1.1).

#![deny(unsafe_code)]
#![warn(missing_docs)]

mod causes;
mod events;
mod helpers;
mod problems;
mod wifi;

use std::sync::LazyLock;

use airpulse_dsl_ir::{CatalogRef, FieldIdx, TopoFuncIdx};
use airpulse_dsl_types::{
    ActionKind, Capability, CauseKind, EventType, ProblemKind, SarifId, ScopeType, Severity,
};

/// Stable catalog id used by `ProgramImage.catalog_ref.id`
/// (`06-ir-bytecode.md` §2 catalog_ref).
pub const CATALOG_ID: &str = "airpulse.catalog";

/// Stable catalog version used by `ProgramImage.catalog_ref.version`
/// (`06-ir-bytecode.md` §2 catalog_ref).
pub const CATALOG_VERSION: &str = "1.3";

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

static EVENTS: LazyLock<Box<[EventSchema]>> = LazyLock::new(events::all_events);

static CAUSES: LazyLock<Box<[CauseSchema]>> = LazyLock::new(causes::all_causes);

static PROBLEMS: LazyLock<Box<[ProblemSchema]>> = LazyLock::new(problems::all_problems);

static ACTIONS: LazyLock<Box<[ActionSchema]>> = LazyLock::new(|| {
    Box::new([
        ActionSchema {
            kind: ActionKind::RequestObservation,
            name: "request_observation",
            arg_kind: ActionArgKind::ObservationKind,
            target_types: vec![ActionTargetType::ScopeId, ActionTargetType::ScopeIdList]
                .into_boxed_slice(),
            allowed_kinds: OBSERVATION_KINDS.to_vec().into_boxed_slice(),
        },
        ActionSchema {
            kind: ActionKind::RunCheck,
            name: "run_check",
            arg_kind: ActionArgKind::CheckKind,
            target_types: vec![ActionTargetType::ScopeId].into_boxed_slice(),
            allowed_kinds: CHECK_KINDS.to_vec().into_boxed_slice(),
        },
        ActionSchema {
            kind: ActionKind::SuppressSymptom,
            name: "suppress_symptom",
            arg_kind: ActionArgKind::ProblemRefBinding,
            target_types: Box::new([]),
            allowed_kinds: Box::new([]),
        },
        ActionSchema {
            kind: ActionKind::MarkAmbiguous,
            name: "mark_ambiguous",
            arg_kind: ActionArgKind::None,
            target_types: vec![ActionTargetType::ScopeId].into_boxed_slice(),
            allowed_kinds: Box::new([]),
        },
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
        TopoFnSchema {
            name: "same_session",
            func_idx: TopoFuncIdx(0),
            arity: 2,
            arg_types: vec![TopoArgType::ScopeId, TopoArgType::ScopeId].into_boxed_slice(),
        },
        TopoFnSchema {
            name: "same_client",
            func_idx: TopoFuncIdx(1),
            arity: 2,
            arg_types: vec![TopoArgType::ScopeId, TopoArgType::ScopeId].into_boxed_slice(),
        },
        TopoFnSchema {
            name: "same_port",
            func_idx: TopoFuncIdx(2),
            arity: 2,
            arg_types: vec![TopoArgType::ScopeId, TopoArgType::ScopeId].into_boxed_slice(),
        },
        TopoFnSchema {
            name: "same_ap",
            func_idx: TopoFuncIdx(3),
            arity: 2,
            arg_types: vec![TopoArgType::ScopeId, TopoArgType::ScopeId].into_boxed_slice(),
        },
        TopoFnSchema {
            name: "same_vlan",
            func_idx: TopoFuncIdx(4),
            arity: 2,
            arg_types: vec![TopoArgType::ScopeId, TopoArgType::ScopeId].into_boxed_slice(),
        },
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
        Capability::new("l3-deep"),
        Capability::new("topology"),
        Capability::new("wifi-ota"),
        Capability::new("radio-nemo"),
    ])
});

// 10-catalog-abi.md §5 observation kinds.
static OBSERVATION_KINDS: [&str; 3] = ["icmp.visibility", "aaa.telemetry", "wifi.rf_metrics"];

// 10-catalog-abi.md §5 check kinds.
static CHECK_KINDS: [&str; 3] = ["cable_loopback", "lldp_poll", "stp_root_check"];

static EXCLUSIVITY_DEFAULTS: LazyLock<Box<[ExclusivityPair]>> = LazyLock::new(|| {
    Box::new([
        ExclusivityPair {
            left: CauseKind::new("Congestion"),
            right: CauseKind::new("PmtudBlackhole"),
        },
        ExclusivityPair {
            left: CauseKind::new("Congestion"),
            right: CauseKind::new("TransientL2Disruption"),
        },
        ExclusivityPair {
            left: CauseKind::new("PmtudBlackhole"),
            right: CauseKind::new("TransientL2Disruption"),
        },
        ExclusivityPair {
            left: CauseKind::new("PhysicalCableDamage"),
            right: CauseKind::new("PhysicalLinkAbsent"),
        },
        ExclusivityPair {
            left: CauseKind::new("AuthServerOutage"),
            right: CauseKind::new("RfInterference"),
        },
        ExclusivityPair {
            left: CauseKind::new("PacketLossPath"),
            right: CauseKind::new("SpuriousRetransmission"),
        },
        ExclusivityPair {
            left: CauseKind::new("PacketLossPath"),
            right: CauseKind::new("RadioChannelDegradation"),
        },
        ExclusivityPair {
            left: CauseKind::new("BufferbloatQueue"),
            right: CauseKind::new("MiddleboxInterference"),
        },
        ExclusivityPair {
            left: CauseKind::new("PmtudPathIssue"),
            right: CauseKind::new("PacketLossPath"),
        },
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

/// Returns the canonical W0 Wi-Fi mapping table (83 rows).
#[must_use]
pub fn wifi_w0_mappings() -> &'static [wifi::WifiMapping] {
    wifi::WIFI_MAPPINGS
}

/// Returns the static `CatalogRef` for ADGL `ProgramImage`.
#[must_use]
pub fn catalog_ref() -> CatalogRef {
    CatalogRef {
        id: CATALOG_ID.into(),
        version: CATALOG_VERSION.into(),
    }
}

/// Returns all catalog event schemas in declaration order.
#[must_use]
pub fn events() -> &'static [EventSchema] {
    EVENTS.as_ref()
}

/// Returns the number of catalog events.
#[must_use]
pub fn event_count() -> usize {
    EVENTS.len()
}

/// Returns the number of catalog causes.
#[must_use]
pub fn cause_count() -> usize {
    CAUSES.len()
}

/// Returns the number of catalog problems.
#[must_use]
pub fn problem_count() -> usize {
    PROBLEMS.len()
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

/// Resolve a problem schema by default SARIF id (`10-catalog-abi.md` §4).
#[must_use]
pub fn resolve_problem_by_sarif(sarif_id: &str) -> Option<&'static ProblemSchema> {
    PROBLEMS
        .iter()
        .find(|schema| schema.default_sarif_id.as_str() == sarif_id)
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
            resolve_cause(cause.as_str())?;
            &CAUSE_FIELDS
        }
        EventOrBindingType::Problem(problem) => {
            resolve_problem(problem.as_str())?;
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
    FieldSchema {
        name: "time",
        field_type: FieldType::Int,
        pii: false,
        idx: CAUSE_FIELD_TIME,
    },
    FieldSchema {
        name: "confidence",
        field_type: FieldType::Confidence,
        pii: false,
        idx: CAUSE_FIELD_CONFIDENCE,
    },
    FieldSchema {
        name: "evidence",
        field_type: FieldType::NodeIdList,
        pii: false,
        idx: FieldIdx(3),
    },
];

// 04-type-system.md §6.2 Problem schema.
static PROBLEM_FIELDS: [FieldSchema; 5] = [
    FieldSchema {
        name: "target",
        field_type: FieldType::ScopeId(ScopeType::Global),
        pii: false,
        idx: PROBLEM_FIELD_TARGET,
    },
    FieldSchema {
        name: "time",
        field_type: FieldType::Int,
        pii: false,
        idx: PROBLEM_FIELD_TIME,
    },
    FieldSchema {
        name: "severity",
        field_type: FieldType::Severity,
        pii: false,
        idx: FieldIdx(2),
    },
    FieldSchema {
        name: "evidence",
        field_type: FieldType::NodeIdList,
        pii: false,
        idx: FieldIdx(3),
    },
    FieldSchema {
        name: "sarif_id",
        field_type: FieldType::SarifId,
        pii: false,
        idx: FieldIdx(4),
    },
];
