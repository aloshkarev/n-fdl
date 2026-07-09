//! `Severity` scalar domain per `docs/idea/spec/04-type-system.md` §2.

/// Problem severity, `04-type-system.md` §2:
/// `Critical/High/Medium/Low/Recommended/Optional` — used by `emit`
/// (`03-semantics.md` §3.4, `ProblemNode.severity` in `04` §3).
///
/// Ordered most-severe-first, matching declaration order in the spec table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    /// Highest severity.
    Critical,
    /// High severity.
    High,
    /// Medium severity (default for `AmbiguousDiagnosis`, `10` §4).
    Medium,
    /// Low severity.
    Low,
    /// Recommended remediation.
    Recommended,
    /// Optional remediation.
    Optional,
}

impl Severity {
    /// The DSL surface spelling of this severity.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Severity::Critical => "Critical",
            Severity::High => "High",
            Severity::Medium => "Medium",
            Severity::Low => "Low",
            Severity::Recommended => "Recommended",
            Severity::Optional => "Optional",
        }
    }
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordering_is_most_severe_first() {
        assert!(Severity::Critical < Severity::High);
        assert!(Severity::High < Severity::Medium);
        assert!(Severity::Low < Severity::Recommended);
        assert!(Severity::Recommended < Severity::Optional);
    }
}
