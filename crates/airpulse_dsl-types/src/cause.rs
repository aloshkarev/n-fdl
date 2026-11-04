//! A published cause. The kind, scope, target, and evidence come from the
//! catalog row. An agent does not fill these fields.

use crate::{CauseKind, ScopeId};

/// Whether a named evidence ref was observed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EvidenceState {
    /// The correlate fired.
    Present,
    /// A required correlate was looked for and was not there.
    Absent,
    /// The correlate was cut short by a cap or a partial window.
    Partial,
}

/// One evidence ref on a cause.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CauseEvidence {
    name: Box<str>,
    state: EvidenceState,
}

impl CauseEvidence {
    /// Builds a ref. The name is the catalog symbol, and it must not be empty.
    pub fn new(name: impl Into<Box<str>>, state: EvidenceState) -> Result<Self, CauseError> {
        let name = name.into();
        if name.is_empty() {
            return Err(CauseError::EmptyEvidenceName);
        }
        Ok(Self { name, state })
    }

    /// Catalog symbol for this ref.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Present, absent, or partial.
    #[must_use]
    pub const fn state(&self) -> EvidenceState {
        self.state
    }
}

/// A cause that can be published.
///
/// `scope` is the partition the rule fired on. `target` is the scope the
/// catalog row explains. Evidence is never empty: a missing correlate is
/// stored as [`EvidenceState::Absent`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Cause {
    kind: CauseKind,
    scope: ScopeId,
    target: ScopeId,
    evidence: Box<[CauseEvidence]>,
}

impl Cause {
    /// Rejects an empty kind and an empty evidence list.
    pub fn try_new(
        kind: CauseKind,
        scope: ScopeId,
        target: ScopeId,
        evidence: impl Into<Box<[CauseEvidence]>>,
    ) -> Result<Self, CauseError> {
        if kind.as_str().is_empty() {
            return Err(CauseError::EmptyKind);
        }
        let evidence = evidence.into();
        if evidence.is_empty() {
            return Err(CauseError::EmptyEvidence);
        }
        Ok(Self {
            kind,
            scope,
            target,
            evidence,
        })
    }

    /// Catalog kind.
    #[must_use]
    pub const fn kind(&self) -> &CauseKind {
        &self.kind
    }

    /// Partition the rule fired on.
    #[must_use]
    pub const fn scope(&self) -> ScopeId {
        self.scope
    }

    /// Scope the catalog row explains.
    #[must_use]
    pub const fn target(&self) -> ScopeId {
        self.target
    }

    /// Tagged evidence refs. Never empty.
    #[must_use]
    pub fn evidence(&self) -> &[CauseEvidence] {
        &self.evidence
    }
}

/// Why a cause value was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CauseError {
    /// `CauseKind` was an empty string.
    EmptyKind,
    /// The evidence list had no refs.
    EmptyEvidence,
    /// An evidence ref had an empty symbol name.
    EmptyEvidenceName,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ScopeId;

    fn evidence(name: &str, state: EvidenceState) -> CauseEvidence {
        CauseEvidence::new(name, state).expect("evidence name")
    }

    #[test]
    fn cause_keeps_kind_scope_target_and_tagged_evidence() {
        let scope = ScopeId::vlan(10);
        let target = ScopeId::session((0x7f00_0001, 443), (0x7f00_0002, 50_000));
        let cause = Cause::try_new(
            CauseKind::new("PacketLossPath"),
            scope,
            target,
            vec![
                evidence("rtx", EvidenceState::Present),
                evidence("icmp", EvidenceState::Absent),
            ],
        )
        .expect("cause");
        assert_eq!(cause.kind().as_str(), "PacketLossPath");
        assert_eq!(cause.scope(), scope);
        assert_eq!(cause.target(), target);
        assert_eq!(cause.evidence()[1].state(), EvidenceState::Absent);
    }

    #[test]
    fn empty_kind_and_empty_evidence_are_refused() {
        let scope = ScopeId::GLOBAL;
        assert_eq!(
            Cause::try_new(
                CauseKind::new(""),
                scope,
                scope,
                vec![evidence("rtx", EvidenceState::Present)]
            ),
            Err(CauseError::EmptyKind)
        );
        assert_eq!(
            Cause::try_new(
                CauseKind::new("PacketLossPath"),
                scope,
                scope,
                Vec::<CauseEvidence>::new()
            ),
            Err(CauseError::EmptyEvidence)
        );
        assert_eq!(
            CauseEvidence::new("", EvidenceState::Partial).unwrap_err(),
            CauseError::EmptyEvidenceName
        );
    }
}
