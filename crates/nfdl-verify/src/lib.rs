#![forbid(unsafe_code)]

pub mod bounds;
pub mod wiring;
pub mod z3_backend;

pub use bounds::IntervalAnalyzer;
pub use nfdl_diag::Severity;
pub use wiring::verify_protocol;
pub use z3_backend::{can_prove_non_negative, prove_bounds};
