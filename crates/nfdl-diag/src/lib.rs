//! Backward-compatibility facade: re-exports the shared [`ndsl_diag`] crate
//! (`Severity`, `Span`, `Diagnostic`, `DiagBuffer`) so existing `nfdl_diag::*`
//! paths keep working unchanged.

#![forbid(unsafe_code)]

pub use ndsl_diag::*;

#[cfg(test)]
mod tests {
    use super::{DiagBuffer, Diagnostic, Severity, Span};

    #[test]
    fn reexported_api_is_usable() {
        let mut buf = DiagBuffer::new();
        assert!(!buf.has_errors());
        let d = Diagnostic::error("NFD001", "boom", Span::new(1, 4));
        assert_eq!(d.severity, Severity::Error);
        buf.push(d);
        assert!(buf.has_errors());
        assert_eq!(buf.len(), 1);
    }
}
