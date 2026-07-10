//! Catalog reference types per `docs/idea/spec/10-catalog-abi.md`.
//!
//! These are thin, strongly-typed identifier wrappers. Resolution against the
//! actual catalog (schemas, field types, scope validity) is owned by the
//! `airpulse_dsl-catalog` crate — the catalog is the single source of
//! event/cause/problem/action/topology names (`10-catalog-abi.md` §1).

macro_rules! symbolic_id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(Box<str>);

        impl $name {
            /// Wraps a symbolic catalog name. Existence in the catalog is
            /// checked by the verifier/catalog crate, not here.
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

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl From<&str> for $name {
            fn from(s: &str) -> Self {
                Self::new(s)
            }
        }
    };
}

symbolic_id! {
    /// Catalog event type identifier, e.g. `tcp.retransmission_burst`
    /// (`10-catalog-abi.md` §2; `04-type-system.md` §6.1).
    EventType
}

symbolic_id! {
    /// Catalog cause kind identifier, e.g. `PmtudBlackhole`
    /// (`10-catalog-abi.md` §3; `04-type-system.md` §3 `CauseNode.kind`).
    CauseKind
}

symbolic_id! {
    /// Catalog problem kind identifier, e.g. `XlIcmpTcpMss`
    /// (`10-catalog-abi.md` §4; `04-type-system.md` §3 `ProblemNode.kind`).
    ProblemKind
}

symbolic_id! {
    /// Stable symbolic SARIF id, e.g. `l3_pmtud_blackhole`
    /// (`10-catalog-abi.md` §4, C8/ADR-008 legacy-stable).
    SarifId
}

symbolic_id! {
    /// Ruleset capability requirement, e.g. `l3-deep`, `topology`
    /// (`04-type-system.md` §6.5; `10-catalog-abi.md` §8).
    Capability
}

/// Dotted metric path resolved against a catalog event/cause/problem schema,
/// e.g. `rtx.segment_size` → field `segment_size` (`04-type-system.md` §6.1,
/// `03-semantics.md` §5.2 qualified access).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MetricPath(Box<str>);

impl MetricPath {
    /// Wraps a dotted path. Path existence is checked by the verifier
    /// (`05-verification.md` §1), not here.
    #[must_use]
    pub fn new(path: impl Into<Box<str>>) -> Self {
        Self(path.into())
    }

    /// The full dotted path.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Iterator over `.`-separated segments.
    pub fn segments(&self) -> impl Iterator<Item = &str> {
        self.0.split('.')
    }
}

impl std::fmt::Display for MetricPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// The five action kinds — a closed set in v1
/// (`04-type-system.md` §6.4; `10-catalog-abi.md` §5):
/// `ActionKind ::= request_observation | run_check | suppress_symptom
///               | mark_ambiguous | request_topology`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ActionKind {
    /// Load an eBPF capture filter for the target scope(s) (live mode).
    RequestObservation,
    /// Enqueue an external check (e.g. `cable_loopback`, `lldp_poll`).
    RunCheck,
    /// Problem-level suppression: mark a Problem superseded (C7).
    SuppressSymptom,
    /// Create an AmbiguityNode (`03-semantics.md` §4, C5).
    MarkAmbiguous,
    /// Topology-Unknown fallback: enqueue LLDP/CDP poll (C10, ADR-010).
    RequestTopology,
}

impl ActionKind {
    /// The DSL surface keyword for this action (`10-catalog-abi.md` §5).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            ActionKind::RequestObservation => "request_observation",
            ActionKind::RunCheck => "run_check",
            ActionKind::SuppressSymptom => "suppress_symptom",
            ActionKind::MarkAmbiguous => "mark_ambiguous",
            ActionKind::RequestTopology => "request_topology",
        }
    }
}

impl std::fmt::Display for ActionKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symbolic_ids_are_distinct_types_with_value_semantics() {
        let e = EventType::new("tcp.retransmission_burst");
        assert_eq!(e, EventType::from("tcp.retransmission_burst"));
        assert_eq!(e.as_str(), "tcp.retransmission_burst");
        assert_ne!(
            CauseKind::new("Congestion"),
            CauseKind::new("PmtudBlackhole")
        );
        assert_eq!(
            SarifId::new("l3_pmtud_blackhole").to_string(),
            "l3_pmtud_blackhole"
        );
    }

    #[test]
    fn metric_path_segments() {
        let p = MetricPath::new("rtx.segment_size");
        assert_eq!(
            p.segments().collect::<Vec<_>>(),
            vec!["rtx", "segment_size"]
        );
        assert_eq!(p.as_str(), "rtx.segment_size");
    }

    #[test]
    fn action_kind_surface_names() {
        // 10 §5 keyword spellings.
        assert_eq!(
            ActionKind::RequestObservation.as_str(),
            "request_observation"
        );
        assert_eq!(ActionKind::RunCheck.as_str(), "run_check");
        assert_eq!(ActionKind::SuppressSymptom.as_str(), "suppress_symptom");
        assert_eq!(ActionKind::MarkAmbiguous.as_str(), "mark_ambiguous");
        assert_eq!(ActionKind::RequestTopology.as_str(), "request_topology");
    }
}
