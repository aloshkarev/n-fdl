//! Runtime state store for ADGL (AirPulse Diagnostic Graph Language).
//!
//! Crate-owner role per `docs/idea/spec/07-runtime.md` §1: `airpulse_dsl::store`
//! — "GraphStore, RingBuffer, WaitQueue, GC" (`07` §3–§4).
//!
//! This crate owns only *data structures, GC, and their invariants*:
//!
//! - [`EventNode`] — the owned, immutable event stored in rings (`04` §3,
//!   `07` §2),
//! - [`RingBuffer`] — per-scope, time-sorted, capacity-bounded event buffer
//!   with watermark GC (`07` §3–§4, ADR-011),
//! - [`SubGraph`] — per-partition Cause/Problem/Ambiguity/Edge state with
//!   provenance and emission dedup sets (`07` §3, `03` §3.3–3.4, §4),
//! - [`WaitQueue`] — bounded per-scope min-heap of suspended
//!   [`PendingMatch`](airpulse_dsl_ir::PendingMatch)es (`08` §3, ADR-011),
//! - [`GraphStore`] — the DashMap-sharded partition owner with the global
//!   monotone watermark (`07` §3, §10),
//! - [`Limits`] / [`StoreDiagnostic`] — ADR-011 DoS bounds and spill signals.
//!
//! Engine pipeline logic (ingest/route/anchor-match/correlate/resume,
//! `07` §5) lives in the evaluator crate; the store never interprets rules
//! and never prints or logs — spills surface as [`StoreDiagnostic`] values
//! for the caller to route (`11-diagnostics`).

#![deny(unsafe_code)]
#![warn(missing_docs)]

mod diag;
mod event;
mod limits;
mod ring;
mod store;
mod subgraph;
mod waitqueue;

pub use diag::StoreDiagnostic;
pub use event::{EventNode, EventProvenance};
pub use limits::Limits;
pub use ring::RingBuffer;
pub use store::GraphStore;
pub use subgraph::{
    AmbiguityNode, AmbiguityState, CauseNode, EdgeEndpoint, EvidenceEdge, EvidenceEdgeKind,
    ProblemNode, RuntimeProvKey, SubGraph, window_id,
};
pub use waitqueue::WaitQueue;
