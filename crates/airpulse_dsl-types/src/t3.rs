//! Three-valued logic (`T3`) per `docs/idea/spec/04-type-system.md` §2 and
//! ADR-010 (`docs/idea/adr/ADR-010-topology-unknown.md`).

/// Three-valued logic value: `Bool | Unknown`.
///
/// Spec: `04-type-system.md` §2 (`T3` scalar domain, C10) and ADR-010.
/// Topology functions (`07-runtime.md` §6) return `T3`; `Unknown` MUST NOT
/// collapse to `False` — it drives the `request_topology` fallback
/// (`03-semantics.md` §3.7) instead of a false-negative branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum T3 {
    /// Definitely true.
    True,
    /// Definitely false.
    False,
    /// Topology (or other three-valued source) could not decide.
    Unknown,
}

impl T3 {
    /// Kleene conjunction (`03-semantics.md` §3.7):
    /// `True and Unknown = Unknown`; `False and Unknown = False`.
    ///
    /// Note: evaluator-level short-circuiting (do not *evaluate* RHS when LHS
    /// is `False`/`Unknown`) is an evaluator concern; this is the pure truth
    /// table over already-computed values.
    #[must_use]
    pub const fn and(self, rhs: T3) -> T3 {
        match (self, rhs) {
            (T3::False, _) | (_, T3::False) => T3::False,
            (T3::True, T3::True) => T3::True,
            _ => T3::Unknown,
        }
    }

    /// Kleene disjunction (`03-semantics.md` §3.7):
    /// `Unknown or True = True`; `Unknown or Unknown = Unknown`.
    #[must_use]
    pub const fn or(self, rhs: T3) -> T3 {
        match (self, rhs) {
            (T3::True, _) | (_, T3::True) => T3::True,
            (T3::False, T3::False) => T3::False,
            _ => T3::Unknown,
        }
    }

    /// Kleene negation: `not Unknown = Unknown`.
    #[must_use]
    pub const fn not(self) -> T3 {
        match self {
            T3::True => T3::False,
            T3::False => T3::True,
            T3::Unknown => T3::Unknown,
        }
    }

    /// `true` only for [`T3::True`]. `Unknown` is *not* true (ADR-010: the
    /// `then`-branch requires `True`).
    #[must_use]
    pub const fn is_true(self) -> bool {
        matches!(self, T3::True)
    }

    /// `true` only for [`T3::False`]. `Unknown` is *not* false (ADR-010: the
    /// `else`-branch requires `False`, never `Unknown`).
    #[must_use]
    pub const fn is_false(self) -> bool {
        matches!(self, T3::False)
    }

    /// `true` only for [`T3::Unknown`] — the `request_topology` branch
    /// (`03-semantics.md` §3.7).
    #[must_use]
    pub const fn is_unknown(self) -> bool {
        matches!(self, T3::Unknown)
    }
}

/// Lifts `Bool` into `T3` (metric comparisons are `Bool` lifted to `T3`,
/// `03-semantics.md` §3.7).
impl From<bool> for T3 {
    fn from(b: bool) -> Self {
        if b { T3::True } else { T3::False }
    }
}

#[cfg(test)]
mod tests {
    use super::T3::{False, True, Unknown};
    use super::*;

    const ALL: [T3; 3] = [True, False, Unknown];

    #[test]
    fn kleene_and_truth_table() {
        assert_eq!(True.and(True), True);
        assert_eq!(True.and(False), False);
        assert_eq!(False.and(True), False);
        assert_eq!(False.and(False), False);
        // Spec 03 §3.7 literal cases:
        assert_eq!(True.and(Unknown), Unknown);
        assert_eq!(Unknown.and(True), Unknown);
        assert_eq!(False.and(Unknown), False);
        assert_eq!(Unknown.and(False), False);
        assert_eq!(Unknown.and(Unknown), Unknown);
    }

    #[test]
    fn kleene_or_truth_table() {
        assert_eq!(True.or(True), True);
        assert_eq!(True.or(False), True);
        assert_eq!(False.or(True), True);
        assert_eq!(False.or(False), False);
        // Spec 03 §3.7 literal cases:
        assert_eq!(Unknown.or(True), True);
        assert_eq!(True.or(Unknown), True);
        assert_eq!(Unknown.or(False), Unknown);
        assert_eq!(False.or(Unknown), Unknown);
        assert_eq!(Unknown.or(Unknown), Unknown);
    }

    #[test]
    fn kleene_not() {
        assert_eq!(True.not(), False);
        assert_eq!(False.not(), True);
        assert_eq!(Unknown.not(), Unknown);
    }

    #[test]
    fn unknown_is_neither_true_nor_false() {
        // ADR-010: Unknown must not collapse to false (or true).
        assert!(!Unknown.is_true());
        assert!(!Unknown.is_false());
        assert!(Unknown.is_unknown());
        assert!(True.is_true() && !True.is_unknown());
        assert!(False.is_false() && !False.is_unknown());
    }

    #[test]
    fn unknown_propagates_unless_dominated() {
        // Unknown is absorbed only by the dominating element (False for and,
        // True for or); otherwise it propagates.
        for x in ALL {
            assert_eq!(x.and(Unknown).is_unknown(), x != False);
            assert_eq!(x.or(Unknown).is_unknown(), x != True);
        }
    }

    #[test]
    fn bool_lifting() {
        assert_eq!(T3::from(true), True);
        assert_eq!(T3::from(false), False);
    }

    #[test]
    fn and_or_commutative() {
        for a in ALL {
            for b in ALL {
                assert_eq!(a.and(b), b.and(a));
                assert_eq!(a.or(b), b.or(a));
            }
        }
    }
}
