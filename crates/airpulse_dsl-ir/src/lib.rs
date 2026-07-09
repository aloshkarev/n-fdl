//! Verified intermediate representation for ADGL (AirPulse Diagnostic Graph
//! Language).
//!
//! Crate-owner role per `docs/idea/spec/07-runtime.md` §1: `airpulse_dsl::ir`
//! — "ProgramImage, RuleInstance, Intent, opcodes" from
//! `docs/idea/spec/06-ir-bytecode.md`.
//!
//! This crate defines only the *shapes* the evaluator executes:
//!
//! - [`ProgramImage`] — the verified, loadable ruleset artifact (`06` §2),
//! - [`RuleInstance`] — one evidence/decision rule with anchor, correlates,
//!   branch table, body intents and verified annotations (`06` §2.1, `05`
//!   §11–12),
//! - [`Predicate`] / [`PredOp`] — the compact slot-register opcode sequence
//!   for anchor/branch predicates (`06` §4),
//! - [`Intent`] — effect directives dispatched over the graph store (`06`
//!   §2.3, `03` §3.3–3.6).
//!
//! Execution (opcode interpretation, correlate scanning, intent dispatch)
//! lives in the evaluator crate (`07` §1 crate-tree); no evaluation logic is
//! implemented here. Phase 1 runs without a parser, so every type here is
//! publicly constructible from plain Rust (hand-coded `ProgramImage`s in
//! evaluator tests).

#![deny(unsafe_code)]
#![warn(missing_docs)]

mod image;
mod intent;
mod predicate;
mod rule;
mod symbol;

pub use image::{AnchorKey, CatalogRef, ExclusivityGroup, PendingMatch, ProgramImage};
pub use intent::{Intent, ProvKey};
pub use predicate::{BindingIdx, FieldIdx, MAX_SLOTS, PredOp, Predicate, SlotIdx, TopoFuncIdx};
pub use rule::{
    AnchorSource, AnchorSpec, BranchTable, CorrelateSource, CorrelateSpec, RuleInstance, RuleKind,
    TopoCall, VerifiedAnnotations, WindowProof,
};
pub use symbol::Symbol;
