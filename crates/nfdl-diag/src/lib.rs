//! Backward-compatibility facade: re-exports the shared [`ndsl_diag`] crate
//! (`Severity`, `Span`, `Diagnostic`, `DiagBuffer`) so existing `nfdl_diag::*`
//! paths keep working unchanged.

#![forbid(unsafe_code)]

pub use ndsl_diag::*;

#[cfg(test)]
mod tests {
    use super::{DiagBuffer, Diagnostic, Severity, Span, to_sarif};

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

    #[test]
    fn reexported_to_sarif_includes_rule_id() {
        let d = Diagnostic::error("NFD001", "boom", Span::unknown());
        let json = to_sarif(std::slice::from_ref(&d), "", "x.nfdl");
        assert!(json.contains("\"ruleId\":\"NFD001\""));
        assert!(json.contains("\"message\":{\"text\":\"boom\"}"));
    }
}
