//! `ProgramImage` — the verified, loadable ruleset artifact per
//! `docs/idea/spec/06-ir-bytecode.md` §2, plus the WaitQueue entry
//! (`06` §2.2).

use std::cmp::Ordering;
use std::collections::HashMap;

use airpulse_dsl_types::{
    Capability, CauseKind, EventId, EventTime, EventType, ProblemKind, RuleId, ScopeId, ScopeType,
};

use crate::rule::{AnchorSource, RuleInstance, RuleKind};

/// One `mutually_exclusive(K1, K2, ...)` declaration
/// (`06-ir-bytecode.md` §2 `ProgramImage.exclusivity`; well-formedness per
/// `05-verification.md` §7 — group size ≤ 8, one pair per group).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExclusivityGroup {
    /// The mutually exclusive cause kinds.
    pub causes: Box<[CauseKind]>,
}

/// Reference to the catalog version this image was verified against
/// (`06-ir-bytecode.md` §2 `catalog_ref`; hot-reload compares
/// `(ruleset_id, version, catalog_ref)`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CatalogRef {
    /// Catalog identifier.
    pub id: Box<str>,
    /// Catalog version string.
    pub version: Box<str>,
}

/// Borrowed anchor lookup key for [`ProgramImage::rules_for`], mirroring
/// [`AnchorSource`] without cloning the symbolic id on the hot path
/// (`06-ir-bytecode.md` §6 zero-copy).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnchorKey<'a> {
    /// Look up evidence rules anchored on this event type.
    Event(&'a EventType),
    /// Look up decision rules anchored on this cause kind
    /// (`ConfidenceMutation` re-eval, `03-semantics.md` §3.5).
    Cause(&'a CauseKind),
    /// Look up decision rules anchored on this problem kind
    /// (`ProblemEmission` re-eval, Example 8).
    Problem(&'a ProblemKind),
}

/// Anchor-type lookup index: rule positions (declaration order preserved —
/// ordered firing, C12) keyed per anchor source family.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct RuleIndex {
    by_event: HashMap<EventType, Vec<u32>>,
    by_cause: HashMap<CauseKind, Vec<u32>>,
    by_problem: HashMap<ProblemKind, Vec<u32>>,
}

/// The Verified IR artifact (`06-ir-bytecode.md` §2 `ProgramImage`): a
/// versioned, self-contained ruleset the engine loads and executes.
///
/// Serialization (serde, versioned, ADR-012 hot-reload/cache) is deferred —
/// Phase 1 hand-codes images in Rust, so no serde dependency yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgramImage {
    /// `b"ADGL"` (`06` §2; vs N-FDL `b"NFDL"`).
    pub magic: [u8; 4],
    /// Semver-packed version: `major << 16 | minor << 8 | patch`.
    pub version: u32,
    /// Ruleset id, e.g. `"airpulse.tcp_diagnostics"`.
    pub ruleset_id: Box<str>,
    /// Required capabilities (`requires = [...]`; checked at load per
    /// `05-verification.md` §6).
    pub requires: Box<[Capability]>,
    /// `mutually_exclusive` groups (C5).
    pub exclusivity: Box<[ExclusivityGroup]>,
    /// Rules in ruleset declaration order (C12 ordered firing).
    pub rules: Box<[RuleInstance]>,
    /// Catalog version this image was verified against.
    pub catalog_ref: CatalogRef,
    /// Anchor lookup index, built at construction — not part of the
    /// serialized image (derivable from `rules`).
    index: RuleIndex,
}

impl ProgramImage {
    /// The ADGL image magic (`06-ir-bytecode.md` §2).
    pub const MAGIC: [u8; 4] = *b"ADGL";

    /// Packs a semver triple as `major << 16 | minor << 8 | patch`
    /// (`06-ir-bytecode.md` §2 `version` field comment).
    #[must_use]
    pub const fn pack_version(major: u8, minor: u8, patch: u8) -> u32 {
        ((major as u32) << 16) | ((minor as u32) << 8) | (patch as u32)
    }

    /// Builds an image from its parts, deriving the anchor lookup index.
    /// `rules` order is the ruleset declaration order (C12).
    #[must_use]
    pub fn new(
        version: u32,
        ruleset_id: impl Into<Box<str>>,
        requires: Box<[Capability]>,
        exclusivity: Box<[ExclusivityGroup]>,
        rules: Box<[RuleInstance]>,
        catalog_ref: CatalogRef,
    ) -> ProgramImage {
        let mut index = RuleIndex::default();
        for (pos, rule) in rules.iter().enumerate() {
            // Positions fit u32: rule count is verifier-bounded (05 §9).
            let pos = u32::try_from(pos).unwrap_or(u32::MAX);
            match &rule.anchor.source {
                AnchorSource::Event(t) => index.by_event.entry(t.clone()).or_default().push(pos),
                AnchorSource::Cause(k) => index.by_cause.entry(k.clone()).or_default().push(pos),
                AnchorSource::Problem(p) => {
                    index.by_problem.entry(p.clone()).or_default().push(pos);
                }
            }
        }
        ProgramImage {
            magic: Self::MAGIC,
            version,
            ruleset_id: ruleset_id.into(),
            requires,
            exclusivity,
            rules,
            catalog_ref,
            index,
        }
    }

    /// Rules anchored on `key` in partitions of scope type `scope`, filtered
    /// to rule class `kind`, in declaration order — the engine dispatch
    /// lookup `img.rules_for(evt.type, sg, Evidence)` of `07-runtime.md` §5
    /// and the decision re-eval lookups of `03-semantics.md` §3.5.
    pub fn rules_for(
        &self,
        key: AnchorKey<'_>,
        scope: ScopeType,
        kind: RuleKind,
    ) -> impl Iterator<Item = &RuleInstance> {
        let positions = match key {
            AnchorKey::Event(t) => self.index.by_event.get(t),
            AnchorKey::Cause(k) => self.index.by_cause.get(k),
            AnchorKey::Problem(p) => self.index.by_problem.get(p),
        };
        positions
            .map(|v| v.as_slice())
            .unwrap_or(&[])
            .iter()
            .filter_map(|&pos| self.rules.get(pos as usize))
            .filter(move |r| r.kind == kind && r.scope == scope)
    }
}

/// A suspended anchor match waiting in the WaitQueue until
/// `watermark > upper_bound` (`06-ir-bytecode.md` §2.2 `PendingMatch`;
/// lifecycle in `08-stream-watermarking.md` §2–3).
///
/// Holds an [`EventId`] + RingBuffer lookup, never a borrowed event
/// reference (`07-runtime.md` §2) — the MAX_LOOKBACK invariant
/// (`05-verification.md` §3.1) guarantees the anchor event survives the wait.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PendingMatch {
    /// Suspended rule.
    pub rule: RuleId,
    /// Anchor event reference (RingBuffer lookup).
    pub anchor_event: EventId,
    /// `anchor.time + max_forward` — resume strictly when the watermark
    /// exceeds this (`08` §3.2).
    pub upper_bound: EventTime,
    /// Partition the match belongs to.
    pub scope: ScopeId,
}

/// Ordered by `upper_bound` first (WaitQueue is a min-heap on the nearest
/// deadline, `06` §2.2 — wrap in `std::cmp::Reverse` for `BinaryHeap`), with
/// deterministic tie-breaks on scope hash and rule id (C12).
impl Ord for PendingMatch {
    fn cmp(&self, other: &Self) -> Ordering {
        self.upper_bound
            .cmp(&other.upper_bound)
            .then_with(|| self.scope.hash_key().cmp(&other.scope.hash_key()))
            .then_with(|| self.rule.cmp(&other.rule))
            .then_with(|| self.anchor_event.cmp(&other.anchor_event))
    }
}

impl PartialOrd for PendingMatch {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
