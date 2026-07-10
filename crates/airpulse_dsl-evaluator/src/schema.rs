//! Evaluator field-index schema.
//!
//! `06-ir-bytecode.md` §6 encodes metric paths as `field_idx (u16)`. The
//! catalog owns the canonical cause/problem field ordering, and evaluator
//! runtime loads must use those exact indices. This module keeps only the
//! evaluator-specific reserved event fields (`time`/`target`) and re-exports
//! cause/problem indices from `airpulse_dsl-catalog`.

pub use airpulse_dsl_catalog::{
    CAUSE_FIELD_CONFIDENCE, CAUSE_FIELD_TARGET, CAUSE_FIELD_TIME, PROBLEM_FIELD_TARGET,
    PROBLEM_FIELD_TIME,
};
use airpulse_dsl_ir::FieldIdx;

/// Reserved event field holding the event's *target* scope key, interned via
/// [`crate::ScopeInterner::intern`], for events whose target differs from
/// their partition scope (`03-semantics.md` §5.1 target vs scope). Events
/// without this field resolve `<binding>.target` to their partition scope.
pub const EVENT_FIELD_TARGET: FieldIdx = FieldIdx(u16::MAX);

/// Reserved event field resolving to `EventNode.time` (ms) — `rtx.time` in
/// `06-ir-bytecode.md` §4 loads (struct field, not a metric field).
pub const EVENT_FIELD_TIME: FieldIdx = FieldIdx(u16::MAX - 1);
