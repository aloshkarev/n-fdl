//! Loader-level ADGL `include` composition (Wave 7 first cut).
//!
//! # Surface (ADGL only)
//!
//! Leading file-level directives, before the `ruleset` keyword:
//!
//! ```text
//! include "relative/path.adgl"
//!
//! ruleset "entry" { ... }
//! ```
//!
//! - Paths are resolved relative to the **including** file's directory.
//! - Included files are full ADGL rulesets (they may themselves `include`).
//! - Composition keeps the **entry** ruleset name / version / header decls and
//!   **prepends** rules from includes (depth-first) ahead of the entry's own rules.
//! - Header decls (`requires`, `mutually_exclusive`) from included files are
//!   ignored in this first cut.
//!
//! This is intentionally **not** wired through tree-sitter; the canonical Rust
//! parser remains the verify/runtime path. Grammar + tree-sitter dual updates
//! are a follow-up.
//!
//! [`parse_ruleset`](crate::parse_ruleset) is unchanged and does **not** accept
//! `include` lines — use [`load_ruleset`] / [`LoadedRuleset::parse`].

use crate::ast::Ruleset;
use crate::parser::parse_ruleset;
use ndsl_diag::{DiagBuffer, Diagnostic, Span};
use std::fs;
use std::path::{Path, PathBuf};

/// Owned ADGL source after `include` expansion.
#[derive(Debug, Clone)]
pub struct LoadedRuleset {
    /// Canonical path of the entry file.
    pub entry: PathBuf,
    /// Files visited during expansion (canonical), entry first.
    pub files: Vec<PathBuf>,
    /// Single composed ADGL source (no `include` lines).
    pub source: String,
}

impl LoadedRuleset {
    /// Parse the composed source with [`parse_ruleset`].
    pub fn parse(&self) -> Result<Ruleset<'_>, DiagBuffer> {
        parse_ruleset(&self.source)
    }
}

/// Load an ADGL file from disk, resolving leading `include "…"` directives.
///
/// See module docs for merge semantics and cycle detection.
pub fn load_ruleset(path: impl AsRef<Path>) -> Result<LoadedRuleset, DiagBuffer> {
    let path = path.as_ref();
    let mut stack = Vec::new();
    let mut files = Vec::new();
    let expanded = expand_file(path, &mut stack, &mut files)?;
    let source = compose_entry(&expanded.body, &expanded.imported_rules_text)?;
    Ok(LoadedRuleset {
        entry: files.first().cloned().unwrap_or_else(|| path.to_path_buf()),
        files,
        source,
    })
}

struct ExpandedFile {
    /// File body with leading `include` lines removed (still a full ruleset).
    body: String,
    /// Text of rules collected from includes (depth-first), ready to splice.
    imported_rules_text: String,
}

fn expand_file(
    path: &Path,
    stack: &mut Vec<PathBuf>,
    files: &mut Vec<PathBuf>,
) -> Result<ExpandedFile, DiagBuffer> {
    expand_file_from_include(path, &Span::unknown(), stack, files)
}

fn expand_file_from_include(
    path: &Path,
    include_span: &Span,
    stack: &mut Vec<PathBuf>,
    files: &mut Vec<PathBuf>,
) -> Result<ExpandedFile, DiagBuffer> {
    let canon = canonicalize_existing(path, *include_span)?;
    if let Some(cycle_from) = stack.iter().position(|p| p == &canon) {
        return Err(cycle_diagnostic(stack, cycle_from, &canon));
    }

    stack.push(canon.clone());
    if !files.contains(&canon) {
        files.push(canon.clone());
    }

    let raw = fs::read_to_string(&canon).map_err(|e| {
        io_diagnostic(
            format!("failed to read ADGL file `{}`: {e}", path.display()),
            *include_span,
        )
    })?;

    let (includes, body) = strip_leading_includes(&raw, &canon)?;

    let mut imported_rules_text = String::new();
    let parent = canon.parent().unwrap_or_else(|| Path::new("."));
    for inc in &includes {
        let child_path = parent.join(&inc.path);
        let child = expand_file_from_include(&child_path, &inc.span, stack, files)?;
        imported_rules_text.push_str(&child.imported_rules_text);
        imported_rules_text.push_str(&rules_text_from_body(&child.body)?);
        if !imported_rules_text.is_empty() && !imported_rules_text.ends_with('\n') {
            imported_rules_text.push('\n');
        }
    }

    stack.pop();
    Ok(ExpandedFile {
        body,
        imported_rules_text,
    })
}

fn rules_text_from_body(body: &str) -> Result<String, DiagBuffer> {
    let ast = parse_ruleset(body)?;
    let mut out = String::new();
    for rule in &ast.rules {
        let span = match rule {
            crate::ast::RuleDecl::Evidence(e) => e.span,
            crate::ast::RuleDecl::Decision(d) => d.span,
        };
        if span.end > body.len() || span.start > span.end {
            let mut buf = DiagBuffer::new();
            buf.push(Diagnostic::error(
                "ADGL0203",
                "internal error: rule span out of bounds while expanding include",
                Span::unknown(),
            ));
            return Err(buf);
        }
        out.push_str(body[span.start..span.end].trim());
        out.push('\n');
    }
    Ok(out)
}

fn compose_entry(body: &str, imported_rules: &str) -> Result<String, DiagBuffer> {
    if imported_rules.is_empty() {
        return Ok(body.to_owned());
    }
    let ast = parse_ruleset(body)?;
    // Prepend imported rules after the header, before the entry's own rules.
    let insert_at = match ast.rules.first() {
        Some(crate::ast::RuleDecl::Evidence(e)) => e.span.start,
        Some(crate::ast::RuleDecl::Decision(d)) => d.span.start,
        None => {
            let close = ast.span.end.checked_sub(1).ok_or_else(|| {
                let mut buf = DiagBuffer::new();
                buf.push(Diagnostic::error(
                    "ADGL0203",
                    "internal error: empty ruleset span while composing includes",
                    Span::unknown(),
                ));
                buf
            })?;
            if body.as_bytes().get(close) != Some(&b'}') {
                let mut buf = DiagBuffer::new();
                buf.push(Diagnostic::error(
                    "ADGL0203",
                    "internal error: ruleset span does not end at '}' while composing includes",
                    Span::new(close, close.saturating_add(1).min(body.len())),
                ));
                return Err(buf);
            }
            close
        }
    };

    let mut out = String::with_capacity(body.len() + imported_rules.len() + 2);
    out.push_str(&body[..insert_at]);
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(imported_rules);
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(&body[insert_at..]);
    Ok(out)
}

struct IncludeDir {
    path: String,
    span: Span,
}

fn strip_leading_includes(src: &str, file: &Path) -> Result<(Vec<IncludeDir>, String), DiagBuffer> {
    let bytes = src.as_bytes();
    let mut i = 0usize;
    let mut includes = Vec::new();

    loop {
        i = skip_ws_and_comments(bytes, i).map_err(|span| {
            let mut buf = DiagBuffer::new();
            buf.push(Diagnostic::error(
                "ADGL0106",
                format!(
                    "unclosed block comment while scanning includes in {}",
                    file.display()
                ),
                span,
            ));
            buf
        })?;

        if i >= bytes.len() {
            break;
        }

        if !starts_with_word(bytes, i, b"include") {
            break;
        }
        let start = i;
        i += b"include".len();
        if i < bytes.len() && is_ident_cont(bytes[i]) {
            // Identifier that merely starts with "include" — not a directive.
            break;
        }

        i = skip_ws_only(bytes, i);
        let (path, path_span, next) = parse_string_lit(bytes, i).map_err(|span| {
            malformed_include(file, "expected string literal path after `include`", span)
        })?;
        i = next;

        // Optional semicolon is not part of the surface; require EOL / comment.
        i = skip_ws_only(bytes, i);
        if i < bytes.len() && bytes[i] != b'\n' && bytes[i] != b'/' {
            return Err(malformed_include(
                file,
                "unexpected tokens after `include \"…\"` (directive must end at line break)",
                Span::new(i, (i + 1).min(bytes.len())),
            ));
        }

        includes.push(IncludeDir {
            path,
            span: Span::new(start, path_span.end),
        });
        let _ = path_span; // retained for future span-accurate IO errors
    }

    Ok((includes, src[i..].to_owned()))
}

fn parse_string_lit(bytes: &[u8], mut i: usize) -> Result<(String, Span, usize), Span> {
    if i >= bytes.len() || bytes[i] != b'"' {
        return Err(Span::new(i, i.min(bytes.len())));
    }
    let start = i;
    i += 1;
    let mut out = String::new();
    while i < bytes.len() {
        match bytes[i] {
            b'"' => {
                i += 1;
                return Ok((out, Span::new(start, i), i));
            }
            b'\\' => {
                i += 1;
                if i >= bytes.len() {
                    return Err(Span::new(start, bytes.len()));
                }
                match bytes[i] {
                    b'"' => out.push('"'),
                    b'\\' => out.push('\\'),
                    b'n' => out.push('\n'),
                    b't' => out.push('\t'),
                    other => out.push(other as char),
                }
                i += 1;
            }
            b'\n' | b'\r' => return Err(Span::new(start, i)),
            c => {
                out.push(c as char);
                i += 1;
            }
        }
    }
    Err(Span::new(start, bytes.len()))
}

fn skip_ws_and_comments(bytes: &[u8], mut i: usize) -> Result<usize, Span> {
    loop {
        while i < bytes.len() && matches!(bytes[i], b' ' | b'\t' | b'\r' | b'\n') {
            i += 1;
        }
        if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'/' {
            i += 2;
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'*' {
            let start = i;
            i += 2;
            loop {
                if i + 1 >= bytes.len() {
                    return Err(Span::new(start, bytes.len()));
                }
                if bytes[i] == b'*' && bytes[i + 1] == b'/' {
                    i += 2;
                    break;
                }
                i += 1;
            }
            continue;
        }
        return Ok(i);
    }
}

fn skip_ws_only(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() && matches!(bytes[i], b' ' | b'\t' | b'\r') {
        i += 1;
    }
    i
}

fn starts_with_word(bytes: &[u8], i: usize, word: &[u8]) -> bool {
    bytes.get(i..).is_some_and(|s| s.starts_with(word))
}

fn is_ident_cont(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn canonicalize_existing(path: &Path, span: Span) -> Result<PathBuf, DiagBuffer> {
    fs::canonicalize(path).map_err(|e| {
        io_diagnostic(
            format!("cannot open include path `{}`: {e}", path.display()),
            span,
        )
    })
}

fn cycle_diagnostic(stack: &[PathBuf], from: usize, again: &Path) -> DiagBuffer {
    let mut chain: Vec<String> = stack[from..].iter().map(|p| display_name(p)).collect();
    chain.push(display_name(again));
    let mut buf = DiagBuffer::new();
    buf.push(Diagnostic::error(
        "ADGL0200",
        format!("include cycle detected: {}", chain.join(" -> ")),
        Span::unknown(),
    ));
    buf
}

fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

fn io_diagnostic(message: impl Into<String>, span: Span) -> DiagBuffer {
    let mut buf = DiagBuffer::new();
    buf.push(Diagnostic::error("ADGL0201", message, span));
    buf
}

fn malformed_include(file: &Path, message: impl Into<String>, span: Span) -> DiagBuffer {
    let mut buf = DiagBuffer::new();
    buf.push(Diagnostic::error(
        "ADGL0202",
        format!("{} ({})", message.into(), file.display()),
        span,
    ));
    buf
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn strip_leading_includes_allows_comments() {
        let src = r#"
// banner
include "a.adgl"
/* block */
include "b.adgl"

ruleset "x" { version = "1.0" }
"#;
        let (incs, body) = strip_leading_includes(src, Path::new("t.adgl")).unwrap();
        assert_eq!(
            incs.iter().map(|i| i.path.as_str()).collect::<Vec<_>>(),
            ["a.adgl", "b.adgl"]
        );
        assert!(body.trim_start().starts_with("ruleset"));
    }
}
