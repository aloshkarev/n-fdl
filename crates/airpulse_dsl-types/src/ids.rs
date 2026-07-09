//! Core runtime ID types per `docs/idea/spec/07-runtime.md` §2 and
//! `docs/idea/spec/06-ir-bytecode.md` §2.1/§2.2.

/// Identifier of an `EventNode` stored in a partition RingBuffer.
///
/// Spec: `07-runtime.md` §2 — `PendingMatch` holds `anchor ref = EventId +
/// RingBuffer-lookup` (never a borrowed reference); `06-ir-bytecode.md` §2.2.
/// Opaque, engine-assigned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EventId(u64);

impl EventId {
    /// Wraps an engine-assigned event id.
    #[must_use]
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    /// Raw id value.
    #[must_use]
    pub const fn raw(self) -> u64 {
        self.0
    }
}

/// Identifier of a graph node (Event/Cause/Problem/Ambiguity/Action node).
///
/// Spec: `04-type-system.md` §3 — every `NodeKind` variant carries
/// `id: NodeId`; Cause/Problem `evidence: List<NodeId>`; `07-runtime.md` §2
/// "Cause/Problem/Ambiguity owned by Sub, stable NodeId". Opaque,
/// engine-assigned, stable for the node's lifetime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(u64);

impl NodeId {
    /// Wraps an engine-assigned node id.
    #[must_use]
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    /// Raw id value.
    #[must_use]
    pub const fn raw(self) -> u64 {
        self.0
    }
}

/// Stable symbolic rule identifier.
///
/// Spec: `06-ir-bytecode.md` §2.1 — `RuleInstance.id: RuleId // stable
/// symbolic, C12`; part of the provenance dedup key (`03-semantics.md` §3.3).
/// Symbolic (not an index) so it stays stable across ruleset edits and
/// hot-reloads (ADR-012).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RuleId(Box<str>);

impl RuleId {
    /// Wraps a stable symbolic rule name.
    #[must_use]
    pub fn new(name: impl Into<Box<str>>) -> Self {
        Self(name.into())
    }

    /// The symbolic name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for RuleId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_opaque_value_types() {
        assert_eq!(EventId::new(7), EventId::new(7));
        assert_ne!(EventId::new(7), EventId::new(8));
        assert_eq!(NodeId::new(1).raw(), 1);
        assert_eq!(RuleId::new("pmtud_evidence").as_str(), "pmtud_evidence");
        assert_ne!(RuleId::new("a"), RuleId::new("b"));
    }
}
