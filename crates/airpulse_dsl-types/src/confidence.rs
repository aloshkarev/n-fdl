//! `Confidence` / `Weight` newtype domains per
//! `docs/idea/spec/04-type-system.md` §2 and ADR-002
//! (`docs/idea/adr/ADR-002-confidence-scale.md`).

/// Cause confidence on the engine-internal `0..100` scale (ADR-002).
///
/// Spec: `04-type-system.md` §2 — `Confidence: u8`, domain `0..100`, a distinct
/// newtype (never confused with `Int` by the typer, §2 note / contract §9.2).
/// Mutation is commutative and clamped: `C_new = clamp(0, 100, C_old + W)`
/// (`03-semantics.md` §3.3, ADR-002).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Confidence(u8);

impl Confidence {
    /// Minimum confidence (0).
    pub const MIN: Confidence = Confidence(0);
    /// Maximum confidence (100).
    pub const MAX: Confidence = Confidence(100);

    /// Constructs a confidence, returning `None` when `value > 100`.
    /// No panic on the data-driven path (`07-runtime.md` §9).
    #[must_use]
    pub const fn new(value: u8) -> Option<Confidence> {
        if value <= 100 { Some(Confidence(value)) } else { None }
    }

    /// Constructs a confidence, clamping values above 100 down to 100.
    #[must_use]
    pub const fn new_clamped(value: u8) -> Confidence {
        if value <= 100 { Confidence(value) } else { Confidence::MAX }
    }

    /// Raw `0..100` value.
    #[must_use]
    pub const fn value(self) -> u8 {
        self.0
    }

    /// Commutative confidence mutation: `clamp(0, 100, self + weight)`
    /// (ADR-002; `03-semantics.md` §3.3). Negative weight decrements with
    /// floor 0 (Contradicts, C7); positive saturates at 100.
    #[must_use]
    pub const fn apply(self, weight: Weight) -> Confidence {
        // i16 cannot overflow: 0..=100 plus -100..=100 stays within i16.
        let sum = self.0 as i16 + weight.value() as i16;
        let clamped = if sum < 0 {
            0
        } else if sum > 100 {
            100
        } else {
            sum
        };
        Confidence(clamped as u8)
    }

    /// `Candidate` threshold pseudo-value: `confidence ∈ [10, 39]`
    /// (`04-type-system.md` §2.1 — a predicate, not a type).
    #[must_use]
    pub const fn is_candidate(self) -> bool {
        self.0 >= 10 && self.0 <= 39
    }

    /// `Probable` threshold pseudo-value: `confidence ∈ [40, 79]`
    /// (`04-type-system.md` §2.1).
    #[must_use]
    pub const fn is_probable(self) -> bool {
        self.0 >= 40 && self.0 <= 79
    }

    /// `Confirmed` threshold pseudo-value: `confidence ∈ [80, 100]`
    /// (`04-type-system.md` §2.1).
    #[must_use]
    pub const fn is_confirmed(self) -> bool {
        self.0 >= 80
    }

    /// Legacy AirPulse verdict scale mapping: `confidence / 100.0` → `0..1`
    /// (ADR-002 — output-boundary only, never on the hot path).
    #[must_use]
    pub fn to_legacy(self) -> f64 {
        f64::from(self.0) / 100.0
    }
}

/// Rule weight on the `-100..+100` scale.
///
/// Spec: `04-type-system.md` §2 — `Weight: i8`, distinct newtype (contract
/// §9.2). Negative weight means a `Contradicts` evidence edge (§4, C7).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Weight(i8);

impl Weight {
    /// Minimum weight (-100).
    pub const MIN: Weight = Weight(-100);
    /// Maximum weight (+100).
    pub const MAX: Weight = Weight(100);

    /// Constructs a weight, returning `None` outside `-100..=100`
    /// (T-Weight typing rule, `04-type-system.md` §7).
    #[must_use]
    pub const fn new(value: i8) -> Option<Weight> {
        if value >= -100 && value <= 100 { Some(Weight(value)) } else { None }
    }

    /// Raw `-100..=100` value.
    #[must_use]
    pub const fn value(self) -> i8 {
        self.0
    }

    /// Whether this weight produces a `Supports` edge (`weight >= 0`) as
    /// opposed to `Contradicts` (`04-type-system.md` §4; `03-semantics.md` §3.3
    /// uses `W >= 0 ? Supports : Contradicts`).
    #[must_use]
    pub const fn is_supporting(self) -> bool {
        self.0 >= 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(v: u8) -> Confidence {
        Confidence::new(v).expect("valid confidence in test")
    }
    fn w(v: i8) -> Weight {
        Weight::new(v).expect("valid weight in test")
    }

    #[test]
    fn confidence_domain_bounds() {
        assert_eq!(Confidence::new(0), Some(Confidence::MIN));
        assert_eq!(Confidence::new(100), Some(Confidence::MAX));
        assert_eq!(Confidence::new(101), None);
        assert_eq!(Confidence::new(255), None);
        assert_eq!(Confidence::new_clamped(255), Confidence::MAX);
    }

    #[test]
    fn weight_domain_bounds() {
        assert_eq!(Weight::new(-100), Some(Weight::MIN));
        assert_eq!(Weight::new(100), Some(Weight::MAX));
        assert_eq!(Weight::new(-101), None);
        assert_eq!(Weight::new(101), None);
        assert_eq!(Weight::new(i8::MIN), None);
    }

    #[test]
    fn apply_saturates_at_upper_bound() {
        assert_eq!(c(90).apply(w(85)), Confidence::MAX);
        assert_eq!(c(100).apply(w(100)), Confidence::MAX);
        assert_eq!(c(50).apply(w(50)), c(100));
    }

    #[test]
    fn apply_floors_at_zero() {
        // Negative weight decrements with floor 0 (03 §3.3, C7).
        assert_eq!(c(30).apply(w(-85)), Confidence::MIN);
        assert_eq!(c(0).apply(w(-100)), Confidence::MIN);
        assert_eq!(c(50).apply(w(-50)), c(0));
    }

    #[test]
    fn apply_is_commutative_within_bounds() {
        // ADR-002: accumulation is commutative (clamping makes order matter
        // only at the bounds; interior additions commute exactly).
        assert_eq!(c(10).apply(w(20)).apply(w(30)), c(10).apply(w(30)).apply(w(20)));
    }

    #[test]
    fn threshold_predicates() {
        // 04 §2.1: Candidate [10,39], Probable [40,79], Confirmed [80,100].
        assert!(!c(9).is_candidate());
        assert!(c(10).is_candidate() && c(39).is_candidate());
        assert!(c(40).is_probable() && c(79).is_probable());
        assert!(!c(40).is_candidate());
        assert!(c(80).is_confirmed() && c(100).is_confirmed());
        assert!(!c(79).is_confirmed());
        // 0..9 falls below every named threshold.
        assert!(!c(5).is_candidate() && !c(5).is_probable() && !c(5).is_confirmed());
    }

    #[test]
    fn legacy_mapping() {
        // ADR-002: legacy_confidence = confidence / 100.0.
        assert_eq!(c(80).to_legacy(), 0.8);
        assert_eq!(Confidence::MIN.to_legacy(), 0.0);
        assert_eq!(Confidence::MAX.to_legacy(), 1.0);
    }

    #[test]
    fn supports_vs_contradicts() {
        assert!(w(0).is_supporting());
        assert!(w(85).is_supporting());
        assert!(!w(-1).is_supporting());
    }
}
