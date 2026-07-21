//! Minimal SARIF 2.1.0 emission for N-FDL / ADGL structured diagnostics.
//!
//! Compact skeleton mirroring ADGL evaluator `to_sarif`:
//! `version`, `$schema`, one `run` with `tool.driver.name`, and
//! `results[]` with `ruleId` / `level` / `message` / `locations`.

use std::fmt::Write as _;

use crate::{Diagnostic, Severity, Span, line_col};

const SARIF_SCHEMA_URL: &str = "https://json.schemastore.org/sarif-2.1.0.json";

/// Default tool driver name for N-FDL / shared ndsl diagnostics.
pub const DEFAULT_TOOL_NAME: &str = "nfdl";

/// Options for SARIF emission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SarifOptions<'a> {
    /// `runs[0].tool.driver.name` (defaults to [`DEFAULT_TOOL_NAME`]).
    pub tool_name: &'a str,
}

impl Default for SarifOptions<'_> {
    fn default() -> Self {
        Self {
            tool_name: DEFAULT_TOOL_NAME,
        }
    }
}

impl SarifOptions<'static> {
    /// Convenience constructor with an explicit tool driver name.
    #[must_use]
    pub const fn with_tool(tool_name: &'static str) -> Self {
        Self { tool_name }
    }
}

/// Converts diagnostics into a minimal deterministic SARIF 2.1.0 JSON document.
///
/// `src` is the source text used to map byte spans to 1-based line/column.
/// `uri` becomes `artifactLocation.uri` for each result location.
#[must_use]
pub fn to_sarif<'a, I>(diags: I, src: &str, uri: &str) -> String
where
    I: IntoIterator<Item = &'a Diagnostic>,
{
    to_sarif_with_options(diags, src, uri, SarifOptions::default())
}

/// Converts diagnostics into SARIF JSON with explicit tool options.
#[must_use]
pub fn to_sarif_with_options<'a, I>(
    diags: I,
    src: &str,
    uri: &str,
    options: SarifOptions<'_>,
) -> String
where
    I: IntoIterator<Item = &'a Diagnostic>,
{
    let mut out = String::new();
    out.push('{');
    out.push_str("\"version\":\"2.1.0\",");
    out.push_str("\"$schema\":");
    push_json_string(&mut out, SARIF_SCHEMA_URL);
    out.push(',');
    out.push_str("\"runs\":[{\"tool\":{\"driver\":{\"name\":");
    push_json_string(&mut out, options.tool_name);
    out.push_str("}},\"results\":[");

    let mut first = true;
    for d in diags {
        if !first {
            out.push(',');
        }
        first = false;
        write_result(&mut out, d, src, uri);
    }

    out.push_str("]}]}");
    out
}

fn write_result(out: &mut String, d: &Diagnostic, src: &str, uri: &str) {
    out.push('{');
    out.push_str("\"ruleId\":");
    push_json_string(out, d.code);
    out.push(',');
    out.push_str("\"level\":");
    push_json_string(out, sarif_level(d.severity));
    out.push(',');
    out.push_str("\"message\":{\"text\":");
    push_json_string(out, &d.message);
    out.push_str("},");
    out.push_str("\"locations\":[");
    write_location(out, src, uri, d.span);
    out.push_str("]}");
}

fn write_location(out: &mut String, src: &str, uri: &str, span: Span) {
    let (start_line, start_col) = line_col(src, span.start);
    let (end_line, end_col) = line_col(src, span.end);
    out.push('{');
    out.push_str("\"physicalLocation\":{");
    out.push_str("\"artifactLocation\":{\"uri\":");
    push_json_string(out, uri);
    out.push_str("},\"region\":{");
    let _ = write!(
        out,
        "\"startLine\":{start_line},\"startColumn\":{start_col},\"endLine\":{end_line},\"endColumn\":{end_col}"
    );
    out.push_str("}}}");
}

fn sarif_level(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Note => "note",
    }
}

fn push_json_string(out: &mut String, s: &str) {
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0C}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c <= '\u{1F}' => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::{SarifOptions, to_sarif, to_sarif_with_options};
    use crate::{DiagBuffer, Diagnostic, Span};

    #[test]
    fn diagnostic_round_trips_into_sarif_with_rule_and_message() {
        let src = "protocol P {\n  bad: u8;\n}\n";
        let start = src.find("bad").unwrap();
        let d = Diagnostic::error("NFD001", "undefined field", Span::new(start, start + 3));

        let json = to_sarif(std::slice::from_ref(&d), src, "proto.nfdl");

        assert!(
            json.contains("\"version\":\"2.1.0\""),
            "missing SARIF version: {json}"
        );
        assert!(
            json.contains("json.schemastore.org/sarif-2.1.0.json"),
            "missing schema URL: {json}"
        );
        assert!(
            json.contains("\"name\":\"nfdl\""),
            "missing tool driver: {json}"
        );
        assert!(
            json.contains("\"ruleId\":\"NFD001\""),
            "missing ruleId: {json}"
        );
        assert!(json.contains("\"level\":\"error\""), "missing level: {json}");
        assert!(
            json.contains("\"message\":{\"text\":\"undefined field\"}"),
            "missing message: {json}"
        );
        assert!(
            json.contains("\"uri\":\"proto.nfdl\""),
            "missing location uri: {json}"
        );
        assert!(
            json.contains("\"startLine\":2") && json.contains("\"startColumn\":3"),
            "missing start region: {json}"
        );
        assert!(
            json.contains("\"endLine\":2") && json.contains("\"endColumn\":6"),
            "missing end region: {json}"
        );
    }

    #[test]
    fn diag_buffer_to_sarif_emits_all_results() {
        let src = "a\nb\n";
        let mut buf = DiagBuffer::new();
        buf.push(Diagnostic::warning("NFDL0001", "naming", Span::new(0, 1)));
        buf.push(Diagnostic::note("NFD003", "hint \"x\"", Span::unknown()));

        let json = buf.to_sarif(src, "t.nfdl");
        assert!(json.contains("\"ruleId\":\"NFDL0001\""));
        assert!(json.contains("\"ruleId\":\"NFD003\""));
        assert!(json.contains("\"level\":\"warning\""));
        assert!(json.contains("\"level\":\"note\""));
        assert!(json.contains("hint \\\"x\\\""));
    }

    #[test]
    fn custom_tool_name_is_honored() {
        let d = Diagnostic::error("NFD001", "x", Span::unknown());
        let json = to_sarif_with_options(
            std::slice::from_ref(&d),
            "",
            "x.nfdl",
            SarifOptions::with_tool("ndsl"),
        );
        assert!(json.contains("\"name\":\"ndsl\""));
    }
}
