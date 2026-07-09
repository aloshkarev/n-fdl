//! Offline execution engine for ADGL (AirPulse Diagnostic Graph Language).
//!
//! Crate-owner role per `docs/idea/spec/07-runtime.md` §1:
//! `airpulse_dsl::evaluator` — "Engine: ingest/route/advance/correlate/exec"
//! (`07` §5).
//!
//! Contents (Phase 1 / M0–M1):
//!
//! - [`eval_predicate`] — predicate interpreter over stack-allocated slot
//!   registers (`06` §4; checked arithmetic, Kleene `T3`);
//! - [`Engine`] — the `07` §5 pipeline: `ingest` → anchor match → suspend /
//!   run → `advance_watermark` → resume → correlate → intents, plus
//!   [`Engine::finish`] end-of-stream flush (`08` §3.4);
//! - [`TopologyProvider`] (`07` §6) + [`StaticTopology`] test oracle with
//!   cycle-bounded `upstream_of`;
//! - [`ActionSink`] (`07` §7) + [`OfflineAuditSink`] (`ADGL3001`
//!   ActionNoOpInReplay);
//! - [`fixtures`] — hand-coded `ProgramImage`s for examples 01 and 07;
//! - [`Snapshot`] — deterministic result extraction (ADR-012) for golden
//!   assertions and later SARIF emission (T-08).
//!
//! # Spec notes / Phase 1 interpretations
//!
//! - **Provenance target**: the IR `ProvKey` carries a static
//!   `target_expr_hash`; the spec dedup key is over the *evaluated* target
//!   (`03` §3.3). The engine deterministically mixes the evaluated target's
//!   scope hash into the key (see `engine::mix_target`).
//! - **Problem cooldown**: the spec references a cooldown (F3) but assigns
//!   it no home in IR or ADR-011 `Limits`; the engine uses one engine-wide
//!   value defaulting to `dedup_window`
//!   ([`Engine::with_problem_cooldown`]).
//! - **Target paths**: without the catalog crate, `<binding>.<field>`
//!   target expressions resolve to the binding's target scope; the raw path
//!   is preserved on [`ActionIntent`] for audit fidelity.
//! - **Self-candidates**: a correlate whose source matches the anchor's own
//!   node/event excludes the anchor itself from candidates (Example 8:
//!   `downstream` never binds as its own `upstream`).

#![deny(unsafe_code)]
#![warn(missing_docs)]

mod binding;
mod diag;
mod engine;
mod error;
mod extract;
pub mod fixtures;
mod interner;
mod predicate;
pub mod sarif;
pub mod schema;
mod sink;
mod topology;

pub use binding::{Binding, Bound, CauseSnapshot, ProblemSnapshot};
pub use diag::EngineDiagnostic;
pub use engine::Engine;
pub use error::CorrelateError;
pub use extract::{CauseView, ProblemView, Snapshot};
pub use interner::{ScopeInterner, scope_key_i64};
pub use predicate::{PredCtx, eval_predicate};
pub use sarif::to_sarif;
pub use sink::{ActionIntent, ActionSink, AuditEntry, OfflineAuditSink, RunMode};
pub use topology::{StaticTopology, TopoFunc, TopologyDiagnostic, TopologyProvider};
