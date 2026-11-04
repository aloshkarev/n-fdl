//! Shared structured diagnostics for the ndsl DSLs (N-FDL and ADGL);
//! see spec `11-error-diagnostics.md`.
//!
//! Minimal `Diagnostic` value type with severity, stable code, message, and
//! a byte-span (`start..end`) into the source. A `DiagBuffer` collects diagnostics
//! and supports rustc-style rendering (`<file>:<line>:<col>: <severity>: <code> <msg>`)
//! plus compact SARIF 2.1.0 export ([`to_sarif`]).

#![forbid(unsafe_code)]

mod sarif;

pub use sarif::{DEFAULT_TOOL_NAME, SarifOptions, to_sarif, to_sarif_with_options};

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

/// A single diagnostic. `code` is a stable diagnostic identifier (e.g. `NFD001`).
/// It is not a catalog cause id.
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

/// Maximum diagnostics kept before one overflow record.
pub const MAX_DIAGNOSTICS: usize = 256;
/// Maximum bytes of diagnostic code and message text kept before overflow.
pub const MAX_DIAGNOSTIC_BYTES: usize = 64 * 1024;
/// Overflow diagnostic. This code is not a cause id.
pub const DIAGNOSTIC_OVERFLOW_CODE: &str = "DIAG9001";

/// A simple in-memory diagnostic collector.
///
/// A full buffer keeps the diagnostics that fit, then one [`DIAGNOSTIC_OVERFLOW_CODE`]
/// record naming the cap, what was kept, and what was dropped. `code` on every
/// record is a diagnostic id, not a catalog cause.
#[derive(Debug, Clone)]
pub struct DiagBuffer {
    diags: Vec<Diagnostic>,
    bytes: usize,
    dropped: usize,
    overflowed: bool,
}

impl Default for DiagBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl DiagBuffer {
    pub fn new() -> Self {
        Self {
            diags: Vec::new(),
            bytes: 0,
            dropped: 0,
            overflowed: false,
        }
    }
    pub fn push(&mut self, d: Diagnostic) {
        if self.overflowed {
            self.dropped += 1;
            self.rewrite_overflow();
            return;
        }
        let add = diagnostic_bytes(&d);
        let over_count = self.diags.len() >= MAX_DIAGNOSTICS;
        let over_bytes = self.bytes.saturating_add(add) > MAX_DIAGNOSTIC_BYTES;
        if over_count || over_bytes {
            self.dropped += 1;
            self.overflowed = true;
            let cap = if over_count {
                "diagnostics"
            } else {
                "diagnostic-bytes"
            };
            self.push_overflow(cap);
            return;
        }
        self.bytes += add;
        self.diags.push(d);
    }
    pub fn extend(&mut self, other: impl IntoIterator<Item = Diagnostic>) {
        for d in other {
            self.push(d);
        }
    }

    fn push_overflow(&mut self, cap: &'static str) {
        let kept = self.diags.len();
        let overflow = Diagnostic::error(
            DIAGNOSTIC_OVERFLOW_CODE,
            overflow_message(cap, kept, self.dropped),
            Span::unknown(),
        );
        // The overflow record is the one extra diagnostic. Do not push it
        // through [`Self::push`] or the cap would recurse.
        self.bytes += diagnostic_bytes(&overflow);
        self.diags.push(overflow);
    }

    fn rewrite_overflow(&mut self) {
        let kept = self.diags.len().saturating_sub(1);
        let dropped = self.dropped;
        let Some(last) = self.diags.last_mut() else {
            return;
        };
        if last.code != DIAGNOSTIC_OVERFLOW_CODE {
            return;
        }
        let cap = if last.message.contains("cap=diagnostic-bytes") {
            "diagnostic-bytes"
        } else {
            "diagnostics"
        };
        let previous = diagnostic_bytes(last);
        last.message = overflow_message(cap, kept, dropped);
        let next = diagnostic_bytes(last);
        self.bytes = self.bytes.saturating_sub(previous).saturating_add(next);
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

    /// Render diagnostics rustc-style against `src`. Line/column are 1-based,
    /// so unknown spans (offset 0) render as `<file>:1:1`.
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

    /// Emit a minimal SARIF 2.1.0 JSON document for the buffered diagnostics.
    ///
    /// See [`to_sarif`].
    #[must_use]
    pub fn to_sarif(&self, src: &str, uri: &str) -> String {
        crate::to_sarif(&self.diags, src, uri)
    }

    /// Emit SARIF JSON with explicit tool options.
    #[must_use]
    pub fn to_sarif_with_options(&self, src: &str, uri: &str, options: SarifOptions<'_>) -> String {
        crate::to_sarif_with_options(&self.diags, src, uri, options)
    }
}

impl IntoIterator for DiagBuffer {
    type Item = Diagnostic;
    type IntoIter = std::vec::IntoIter<Diagnostic>;

    fn into_iter(self) -> Self::IntoIter {
        self.diags.into_iter()
    }
}

fn diagnostic_bytes(d: &Diagnostic) -> usize {
    d.code.len() + d.message.len()
}

fn overflow_message(cap: &str, kept: usize, dropped: usize) -> String {
    format!("coverage=partial cap={cap} kept={kept} dropped={dropped}")
}

pub(crate) fn line_col(src: &str, byte_off: usize) -> (usize, usize) {
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
    fn unknown_span_renders_as_one_one_and_span_clamps() {
        let clamped = Span::new(10, 3);
        assert_eq!(clamped.start, 10);
        assert_eq!(clamped.end, 10);

        let mut buf = DiagBuffer::new();
        buf.push(Diagnostic::note("NFD003", "fyi", Span::unknown()));
        let rendered = buf.render("line one\nline two\n", "x.nfdl");
        assert_eq!(rendered, "x.nfdl:1:1: note: NFD003 fyi\n");
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

    #[test]
    fn one_past_the_count_cap_is_one_overflow_diagnostic() {
        let mut buf = DiagBuffer::new();
        for i in 0..(MAX_DIAGNOSTICS + 3) {
            buf.push(Diagnostic::warning(
                "NFD0001",
                format!("n{i}"),
                Span::unknown(),
            ));
        }
        assert_eq!(buf.len(), MAX_DIAGNOSTICS + 1);
        let last = buf.iter().last().expect("overflow");
        assert_eq!(last.code, DIAGNOSTIC_OVERFLOW_CODE);
        assert!(last.code != "PhysicalLinkAbsent");
        assert!(last.message.contains("cap=diagnostics"));
        assert!(last.message.contains("kept=256"));
        assert!(last.message.contains("dropped=3"));
        assert!(last.message.contains("coverage=partial"));
    }

    #[test]
    fn one_past_the_byte_cap_names_the_cap() {
        let mut buf = DiagBuffer::new();
        buf.push(Diagnostic::error(
            "NFD0001",
            "x".repeat(MAX_DIAGNOSTIC_BYTES),
            Span::unknown(),
        ));
        assert_eq!(buf.len(), 1);
        let only = buf.iter().next().expect("overflow");
        assert_eq!(only.code, DIAGNOSTIC_OVERFLOW_CODE);
        assert!(only.message.contains("cap=diagnostic-bytes"));
        assert!(only.message.contains("kept=0"));
        assert!(only.message.contains("dropped=1"));
    }
}
