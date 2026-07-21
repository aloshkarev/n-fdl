//! Opinionated formatter for N-FDL and ADGL.
//!
//! Wave 0 stub: parse via the canonical syntax crates and return source unchanged
//! on success. Pretty-printing lands in Wave 2 (`Task 13` / `Task 14`).

#![forbid(unsafe_code)]

use airpulse_dsl_syntax::parse_ruleset;
use ndsl_diag::DiagBuffer;
use nfdl_syntax::{ParseError, Parser};

/// Formatter configuration (pretty-printer will honor `indent` in Wave 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormatOptions {
    /// Spaces per indentation level.
    pub indent: usize,
}

impl Default for FormatOptions {
    fn default() -> Self {
        Self { indent: 4 }
    }
}

/// Format failure from the underlying syntax crate.
#[derive(Debug)]
pub enum FormatError {
    Nfdl(ParseError),
    Adgl(DiagBuffer),
}

impl std::fmt::Display for FormatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Nfdl(err) => write!(f, "{err:?}"),
            Self::Adgl(buf) => write!(f, "ADGL parse failed ({} diagnostic(s))", buf.len()),
        }
    }
}

impl std::error::Error for FormatError {}

/// Parse N-FDL source; on success return `src` unchanged (identity stub).
pub fn format_nfdl_source(src: &str) -> Result<String, FormatError> {
    Parser::new(src)
        .parse_protocol()
        .map_err(FormatError::Nfdl)?;
    Ok(src.to_owned())
}

/// Parse ADGL source; on success return `src` unchanged (identity stub).
pub fn format_adgl_source(src: &str) -> Result<String, FormatError> {
    parse_ruleset(src).map_err(FormatError::Adgl)?;
    Ok(src.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL_NFDL: &str = r#"protocol P {
    meta { endian = big; mode = datagram; }
    message M {
        data: bytes[4];
    }
}
"#;

    const MINIMAL_ADGL: &str = r#"ruleset "x" {
  version = "1.0"
  evidence r {
    scope: Session
    anchor a: event(tcp.retransmission_burst)
  }
}
"#;

    #[test]
    fn format_options_default_indent_is_four() {
        assert_eq!(FormatOptions::default().indent, 4);
    }

    #[test]
    fn format_nfdl_source_valid_is_identity() {
        let out = format_nfdl_source(MINIMAL_NFDL).expect("valid N-FDL must parse");
        assert_eq!(out, MINIMAL_NFDL);
    }

    #[test]
    fn format_nfdl_source_invalid_returns_err() {
        let err = format_nfdl_source("protocol P { message M { x: u8 if __rem; } }")
            .expect_err("__rem in conditional field must fail");
        assert!(matches!(err, FormatError::Nfdl(_)));
    }

    #[test]
    fn format_adgl_source_valid_is_identity() {
        let out = format_adgl_source(MINIMAL_ADGL).expect("valid ADGL must parse");
        assert_eq!(out, MINIMAL_ADGL);
    }

    #[test]
    fn format_adgl_source_invalid_returns_err() {
        let err = format_adgl_source("ruleset \"x\" {").expect_err("broken ADGL must fail");
        assert!(matches!(err, FormatError::Adgl(_)));
    }
}
