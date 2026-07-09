//! Runtime correlate/predicate errors per `docs/idea/spec/03-semantics.md`
//! §1 (`Err` domain: `CorrelateError` is a runtime error class) and
//! `docs/idea/spec/06-ir-bytecode.md` §8 item 6: opcodes use checked
//! arithmetic; overflow surfaces as `CorrelateError::ArithOverflow`, never a
//! panic (`07-runtime.md` §9).

/// A predicate/correlate evaluation error — always a *value*, never a panic
/// (`07-runtime.md` §9: no `unwrap`/`expect` on data-driven paths).
///
/// The engine records these as [`crate::EngineDiagnostic::PredicateError`]
/// and skips the offending rule instance; it never aborts the stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CorrelateError {
    /// Checked `i64` arithmetic overflowed (`06` §8 item 6).
    ArithOverflow,
    /// `DIV`/`MOD` by zero (`06` §4 "division by zero is a CorrelateError").
    DivisionByZero,
    /// An opcode received a slot value of the wrong kind (e.g. arithmetic on
    /// a `T3`, `LOAD_EVENT_FIELD` on a Cause binding). A verified
    /// `ProgramImage` never produces this (`06` §8 item 1); it guards
    /// hand-coded Phase 1 images.
    TypeMismatch {
        /// The opcode group that rejected its operand.
        op: &'static str,
    },
    /// An opcode referenced a binding index that does not exist for the
    /// rule (`06` §2.1 binding order: anchor = 0, correlates follow).
    UnknownBinding {
        /// The out-of-range binding index.
        binding: u8,
    },
    /// `TOPO_CALL` referenced a function index outside the closed catalog
    /// set (`07-runtime.md` §6 lists exactly six functions).
    UnknownTopoFunction {
        /// The unknown function index.
        func: u8,
    },
}

impl std::fmt::Display for CorrelateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CorrelateError::ArithOverflow => f.write_str("checked arithmetic overflow"),
            CorrelateError::DivisionByZero => f.write_str("division by zero"),
            CorrelateError::TypeMismatch { op } => write!(f, "operand type mismatch in {op}"),
            CorrelateError::UnknownBinding { binding } => {
                write!(f, "unknown binding index {binding}")
            }
            CorrelateError::UnknownTopoFunction { func } => {
                write!(f, "unknown topology function index {func}")
            }
        }
    }
}

impl std::error::Error for CorrelateError {}
