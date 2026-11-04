//! Foundational shared types for ADGL (AirPulse Diagnostic Graph Language).
//!
//! Crate-owner role per `docs/idea/spec/07-runtime.md` §1: `airpulse_dsl::types`
//! — "node/edge/scope/confidence types" from `docs/idea/spec/04-type-system.md`.
//!
//! This crate contains only value types shared across the later ADGL crates
//! (`-ir`, `-store`, `-evaluator`, `-catalog`, `-syntax`, `-verify`); no store,
//! evaluator, or catalog-resolution logic lives here.

#![deny(unsafe_code)]
#![warn(missing_docs)]

mod bounded_list;
mod catalog;
mod cause;
mod confidence;
mod ids;
mod scope;
mod severity;
mod t3;
mod time;
mod value_encoding;

pub use bounded_list::{
    BoundedList, ListCoverage, ListKind, MAX_CATALOG_LIST_VALUES, bound_int_list,
    bound_node_id_list, bound_scope_id_list,
};
pub use catalog::{ActionKind, Capability, CauseKind, EventType, MetricPath, ProblemKind, SarifId};
pub use cause::{Cause, CauseError, CauseEvidence, EvidenceState};
pub use confidence::{Confidence, Weight};
pub use ids::{EventId, NodeId, RuleId};
pub use scope::{ScopeId, ScopeType};
pub use severity::Severity;
pub use t3::T3;
pub use time::{DurationMs, EventTime};
pub use value_encoding::{stable_hash_u64, stable_string_i64};
