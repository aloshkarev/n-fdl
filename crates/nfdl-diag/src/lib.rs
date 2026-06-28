//! Structured diagnostics for N-FDL (spec `11-error-diagnostics.md`).
//!
//! M0: a minimal `Diagnostic` value type with severity, stable code, message, and
//! a byte-span (`start..end`) into the source. A `DiagBuffer` collects diagnostics
//! and supports rustc-style rendering (`<file>:<line>:<col>: <severity>: <code> <msg>`).
//! SARIF 2.1.0 export is deferred to v2 (see `docs/spec/13-roadmap.md`).

#![forbid(unsafe_code)]

use std::fmt;

/// Diagnostic severity (mirrors spec `11-error-diagnostics.md`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Note,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Note => "note",
        })
    }
}

/// A half-open byte span `[start, end)` into the source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Self {
            start,
            end: end.max(start),
        }
    }
    pub fn unknown() -> Self {
        Self { start: 0, end: 0 }
    }
}

/// A single diagnostic. `code` is a stable identifier (e.g. `NFD001`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: &'static str,
    pub message: String,
    pub span: Span,
}

impl Diagnostic {
    pub fn error(code: &'static str, message: impl Into<String>, span: Span) -> Self {
        Self {
            severity: Severity::Error,
            code,
            message: message.into(),
            span,
        }
    }
    pub fn warning(code: &'static str, message: impl Into<String>, span: Span) -> Self {
        Self {
            severity: Severity::Warning,
            code,
            message: message.into(),
            span,
        }
    }
    pub fn note(code: &'static str, message: impl Into<String>, span: Span) -> Self {
        Self {
            severity: Severity::Note,
            code,
            message: message.into(),
            span,
        }
    }
}

/// A simple in-memory diagnostic collector.
#[derive(Debug, Default, Clone)]
pub struct DiagBuffer {
    diags: Vec<Diagnostic>,
}

impl DiagBuffer {
    pub fn new() -> Self {
        Self { diags: Vec::new() }
    }
    pub fn push(&mut self, d: Diagnostic) {
        self.diags.push(d);
    }
    pub fn extend(&mut self, other: impl IntoIterator<Item = Diagnostic>) {
        self.diags.extend(other);
    }
    pub fn iter(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diags.iter()
    }
    pub fn len(&self) -> usize {
        self.diags.len()
    }
    pub fn is_empty(&self) -> bool {
        self.diags.is_empty()
    }
    pub fn has_errors(&self) -> bool {
        self.diags.iter().any(|d| d.severity == Severity::Error)
    }
    pub fn into_inner(self) -> Vec<Diagnostic> {
        self.diags
    }

    /// Render diagnostics rustc-style against `src`. Unknown spans render as
    /// `<file>:0:0`. Line/column are 1-based.
    pub fn render(&self, src: &str, file: &str) -> String {
        let mut out = String::new();
        for d in &self.diags {
            let (line, col) = line_col(src, d.span.start);
            out.push_str(&format!(
                "{}:{}:{}: {}: {} {}\n",
                file, line, col, d.severity, d.code, d.message
            ));
        }
        out
    }
}

fn line_col(src: &str, byte_off: usize) -> (usize, usize) {
    let off = byte_off.min(src.len());
    let mut line = 1usize;
    let mut col = 1usize;
    for (i, b) in src.bytes().enumerate() {
        if i >= off {
            break;
        }
        if b == b'\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_uses_one_based_line_col() {
        let src = "protocol P {\n  bad: u8;\n}\n";
        // span at the start of "bad" on line 2
        let start = src.find("bad").unwrap();
        let mut buf = DiagBuffer::new();
        buf.push(Diagnostic::error(
            "NFD001",
            "undefined field",
            Span::new(start, start + 3),
        ));
        let rendered = buf.render(src, "proto.nfdl");
        assert!(
            rendered.contains("proto.nfdl:2:3: error: NFD001 undefined field"),
            "got: {rendered}"
        );
    }

    #[test]
    fn has_errors_reflects_severity() {
        let mut buf = DiagBuffer::new();
        assert!(!buf.has_errors());
        buf.push(Diagnostic::warning("NFD002", "x", Span::unknown()));
        assert!(!buf.has_errors());
        buf.push(Diagnostic::error("NFD001", "y", Span::unknown()));
        assert!(buf.has_errors());
        assert_eq!(buf.len(), 2);
    }
}
