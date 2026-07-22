//! Human (ariadne) and JSON rendering for lint diagnostics.

use crate::{FileDiagnostic, LintLevel};
use ariadne::{Label, Report, ReportKind, Source};
use std::io::{self, Write};

/// How to present collected diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RenderFormat {
    #[default]
    Human,
    Json,
}

/// Write diagnostics to `out` in the requested format.
///
/// Returns the number of deny-level diagnostics written (for exit status).
pub fn render(
    diagnostics: &[FileDiagnostic],
    format: RenderFormat,
    mut out: impl Write,
) -> io::Result<usize> {
    match format {
        RenderFormat::Human => render_human(diagnostics, &mut out),
        RenderFormat::Json => render_json(diagnostics, &mut out),
    }
}

fn report_kind(level: LintLevel) -> ReportKind<'static> {
    match level {
        LintLevel::Allow => ReportKind::Advice,
        LintLevel::Warn => ReportKind::Warning,
        LintLevel::Deny => ReportKind::Error,
    }
}

fn render_human(diagnostics: &[FileDiagnostic], out: &mut dyn Write) -> io::Result<usize> {
    let mut deny_count = 0usize;
    for fd in diagnostics {
        if fd.diagnostic.level == LintLevel::Allow {
            continue;
        }
        if fd.diagnostic.level == LintLevel::Deny {
            deny_count += 1;
        }

        let file = fd.path.display().to_string();
        let start = fd.diagnostic.span.start.min(fd.source.len());
        let end = fd.diagnostic.span.end.min(fd.source.len()).max(start);
        // Zero-width spans at EOF confuse ariadne; nudge to a 1-byte window when possible.
        let end = if end == start && start < fd.source.len() {
            start + 1
        } else {
            end
        };
        let span = start..end;

        let mut buf = Vec::new();
        Report::build(report_kind(fd.diagnostic.level), file.clone(), span.start)
            .with_code(fd.diagnostic.id.as_str())
            .with_message(&fd.diagnostic.message)
            .with_label(
                Label::new((file.clone(), span)).with_message(fd.diagnostic.message.clone()),
            )
            .finish()
            .write((file, Source::from(fd.source.as_str())), &mut buf)?;
        out.write_all(&buf)?;
    }
    Ok(deny_count)
}

fn render_json(diagnostics: &[FileDiagnostic], out: &mut dyn Write) -> io::Result<usize> {
    let mut deny_count = 0usize;
    write!(out, "[")?;
    let mut first = true;
    for fd in diagnostics {
        if fd.diagnostic.level == LintLevel::Allow {
            continue;
        }
        if fd.diagnostic.level == LintLevel::Deny {
            deny_count += 1;
        }
        if !first {
            write!(out, ",")?;
        }
        first = false;
        // Hand-rolled JSON to avoid a serde dependency for a flat record.
        write!(
            out,
            "{{\"id\":{},\"level\":{},\"message\":{},\"file\":{},\"span\":{{\"start\":{},\"end\":{}}}}}",
            json_str(fd.diagnostic.id.as_str()),
            json_str(&fd.diagnostic.level.to_string()),
            json_str(&fd.diagnostic.message),
            json_str(&fd.path.display().to_string()),
            fd.diagnostic.span.start,
            fd.diagnostic.span.end,
        )?;
    }
    writeln!(out, "]")?;
    Ok(deny_count)
}

fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LintDiagnostic, LintId};
    use ndsl_diag::Span;
    use std::path::PathBuf;

    #[test]
    fn json_render_escapes_and_counts_deny() {
        let diags = vec![
            FileDiagnostic {
                path: PathBuf::from("a.nfdl"),
                source: String::new(),
                diagnostic: LintDiagnostic::new(
                    LintId::new("NFDL0900"),
                    LintLevel::Warn,
                    "empty",
                    Span::unknown(),
                ),
            },
            FileDiagnostic {
                path: PathBuf::from("b.nfdl"),
                source: String::new(),
                diagnostic: LintDiagnostic::new(
                    LintId::new("NFDL0900"),
                    LintLevel::Deny,
                    "empty",
                    Span::unknown(),
                ),
            },
        ];
        let mut buf = Vec::new();
        let deny = render_json(&diags, &mut buf).unwrap();
        assert_eq!(deny, 1);
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("\"id\":\"NFDL0900\""));
        assert!(s.contains("\"level\":\"deny\""));
        assert!(s.starts_with('[') && s.trim_end().ends_with(']'));
    }

    #[test]
    fn human_render_handles_empty_source() {
        let diags = vec![FileDiagnostic {
            path: PathBuf::from("empty.nfdl"),
            source: String::new(),
            diagnostic: LintDiagnostic::new(
                LintId::new("NFDL0900"),
                LintLevel::Warn,
                "source file is empty",
                Span::unknown(),
            ),
        }];
        let mut buf = Vec::new();
        let deny = render_human(&diags, &mut buf).unwrap();
        assert_eq!(deny, 0);
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("NFDL0900") || s.contains("source file is empty"));
    }
}
