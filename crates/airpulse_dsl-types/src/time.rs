//! Event-time and duration scalar domains per
//! `docs/idea/spec/04-type-system.md` §2 and
//! `docs/idea/spec/08-stream-watermarking.md` (event-time watermarking).

/// Event-time in milliseconds (`i64`), the domain of `EventNode.time`,
/// `Cause.time`, `Problem.time` and the global watermark.
///
/// Spec: `04-type-system.md` §3 (`time: Int`), `07-runtime.md` §3
/// (`watermark: AtomicI64`), `08-stream-watermarking.md` (event-time, ms).
/// Arithmetic is saturating — no panic on data-driven paths (`07` §9).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EventTime(i64);

impl EventTime {
    /// Wraps a millisecond event-time.
    #[must_use]
    pub const fn from_millis(ms: i64) -> Self {
        Self(ms)
    }

    /// Milliseconds since the stream epoch.
    #[must_use]
    pub const fn millis(self) -> i64 {
        self.0
    }

    /// Window arithmetic: `self + d` (e.g. `anchor.time + forward_bound`,
    /// `08` §2 upper-bound computation). Saturating.
    #[must_use]
    pub const fn add(self, d: DurationMs) -> EventTime {
        EventTime(self.0.saturating_add(d.millis()))
    }

    /// Window arithmetic: `self - d` (e.g. `anchor.time - back`,
    /// `03-semantics.md` §3.2 backward window). Saturating.
    #[must_use]
    pub const fn sub(self, d: DurationMs) -> EventTime {
        EventTime(self.0.saturating_sub(d.millis()))
    }
}

/// Non-negative duration in milliseconds.
///
/// Spec: `04-type-system.md` §2 — `Duration | i64_ms | ≥ 0` (correlate `time:`
/// windows, dedup windows).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DurationMs(i64);

impl DurationMs {
    /// Constructs a duration; `None` for negative values (domain is `≥ 0`).
    #[must_use]
    pub const fn from_millis(ms: i64) -> Option<DurationMs> {
        if ms >= 0 { Some(DurationMs(ms)) } else { None }
    }

    /// Milliseconds.
    #[must_use]
    pub const fn millis(self) -> i64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(ms: i64) -> DurationMs {
        DurationMs::from_millis(ms).expect("valid duration in test")
    }

    #[test]
    fn duration_is_non_negative() {
        assert!(DurationMs::from_millis(0).is_some());
        assert!(DurationMs::from_millis(1000).is_some());
        assert!(DurationMs::from_millis(-1).is_none());
    }

    #[test]
    fn window_arithmetic() {
        let t = EventTime::from_millis(10_000);
        assert_eq!(t.add(d(1_000)).millis(), 11_000); // forward bound
        assert_eq!(t.sub(d(500)).millis(), 9_500); // backward bound
    }

    #[test]
    fn window_arithmetic_saturates_instead_of_panicking() {
        let max = EventTime::from_millis(i64::MAX);
        assert_eq!(max.add(d(1)).millis(), i64::MAX);
        let min = EventTime::from_millis(i64::MIN);
        assert_eq!(min.sub(d(1)).millis(), i64::MIN);
    }
}
