//! Built-in lint packs registered by [`crate::LintStore::register_builtin`].
//!
//! Real N-FDL / ADGL style packs land in later tasks. This module ships one
//! trivial engine-smoke lint so `ndsl lint` exercises the driver.

use crate::{LintCheck, LintContext, LintDef, LintDiagnostic, LintId, LintLevel, LintStore};
use ndsl_diag::Span;

/// Engine-smoke demo: warn on empty source files.
///
/// Outside the Wave-0 reserved naming/unused/validate blocks (`NFDL0001`–`NFDL0299`).
pub const NFDL_EMPTY_FILE: LintId = LintId::new("NFDL0900");

pub fn register_builtins(store: &mut LintStore) {
    store.register(
        LintDef {
            id: NFDL_EMPTY_FILE,
            default_level: LintLevel::Warn,
            description: "source file is empty or whitespace-only",
        },
        check_empty_file as LintCheck,
    );
}

fn check_empty_file(ctx: &LintContext<'_>) -> Vec<LintDiagnostic> {
    if ctx.source.trim().is_empty() {
        vec![LintDiagnostic::new(
            NFDL_EMPTY_FILE,
            // Level is rewritten by the store from effective overrides.
            LintLevel::Warn,
            "source file is empty",
            Span::unknown(),
        )]
    } else {
        Vec::new()
    }
}
