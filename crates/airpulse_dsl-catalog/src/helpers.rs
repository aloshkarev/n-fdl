//! Shared catalog schema builders.

use airpulse_dsl_ir::FieldIdx;
use airpulse_dsl_types::{CauseKind, ScopeType, Severity};

use crate::{EventSchema, FieldSchema, FieldType, ScopeRoute};

/// Stable field index constructor.
pub(crate) fn idx(n: u16) -> FieldIdx {
    FieldIdx(n)
}

/// Int field with sequential index derived from position in `fields` slice.
pub(crate) fn int_field(name: &'static str, index: u16) -> FieldSchema {
    FieldSchema {
        name,
        field_type: FieldType::Int,
        pii: false,
        idx: idx(index),
    }
}

/// Session-scoped `target` field at index 0.
pub(crate) fn session_target() -> FieldSchema {
    FieldSchema {
        name: "target",
        field_type: FieldType::ScopeId(ScopeType::Session),
        pii: false,
        idx: idx(0),
    }
}

/// Global-scoped `target` field at index 0.
pub(crate) fn global_target() -> FieldSchema {
    FieldSchema {
        name: "target",
        field_type: FieldType::ScopeId(ScopeType::Global),
        pii: false,
        idx: idx(0),
    }
}

/// Timestamped L3 session flag event (`target`, `time`, `count`).
pub(crate) fn l3_session_flag_event(name: &'static str) -> EventSchema {
    event(
        name,
        &[
            session_target(),
            int_field("time", 1),
            int_field("count", 2),
        ],
        &[ScopeRoute {
            scope: ScopeType::Session,
            path: "target",
        }],
    )
}

/// Build an event schema.
pub(crate) fn event(
    name: &'static str,
    fields: &[FieldSchema],
    routes: &[ScopeRoute],
) -> EventSchema {
    EventSchema {
        name,
        event_type: airpulse_dsl_types::EventType::new(name),
        fields: fields.to_vec().into_boxed_slice(),
        routing_paths: routes.to_vec().into_boxed_slice(),
    }
}

/// Build a cause schema.
pub(crate) fn cause(
    name: &'static str,
    scopes: &[ScopeType],
    default_severity: Option<Severity>,
) -> crate::CauseSchema {
    crate::CauseSchema {
        name,
        kind: CauseKind::new(name),
        valid_scopes: scopes.to_vec().into_boxed_slice(),
        default_severity,
    }
}

/// Build a problem schema.
pub(crate) fn problem(
    name: &'static str,
    scopes: &[ScopeType],
    sarif_id: &'static str,
    severity: Option<Severity>,
) -> crate::ProblemSchema {
    crate::ProblemSchema {
        name,
        kind: airpulse_dsl_types::ProblemKind::new(name),
        valid_scopes: scopes.to_vec().into_boxed_slice(),
        default_sarif_id: airpulse_dsl_types::SarifId::new(sarif_id),
        severity,
    }
}
