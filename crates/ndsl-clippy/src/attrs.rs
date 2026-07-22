//! Wave-0 file-scoped lint suppress / elevate directives.
//!
//! Full grammar attributes (`#[allow(...)]` as real AST nodes) are deferred.
//! Until then, `ndsl-clippy` recognizes the same surface as **line-comment
//! directives** so examples stay valid without grammar churn.
//!
//! See `docs/tooling/lints.md` § "Suppress / deny attributes (Wave 0)".

use std::collections::HashMap;

use crate::LintLevel;

/// Parse file-scoped lint level overrides from `//` line comments.
///
/// Recognized forms (after `//` and optional whitespace):
/// - `#[allow(ID)]`, `#[deny(ID)]`, `#[warn(ID)]` (outer)
/// - `#![allow(ID)]` (inner; same file-wide effect in Wave 0)
/// - `ndsl:allow(ID)`, `ndsl:deny(ID)`, `ndsl:warn(ID)`
///
/// Comma-separated IDs are accepted. `forbid` is an alias for `deny`.
/// Later directives for the same ID win. Unknown / malformed lines are ignored.
pub fn parse_file_attrs(source: &str) -> HashMap<String, LintLevel> {
    let mut out = HashMap::new();
    for line in source.lines() {
        let Some(body) = strip_line_comment(line) else {
            continue;
        };
        if let Some((level, ids)) = parse_directive(body) {
            for id in ids {
                out.insert(id, level);
            }
        }
    }
    out
}

fn strip_line_comment(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    let rest = trimmed.strip_prefix("//")?;
    Some(rest.trim())
}

fn parse_directive(body: &str) -> Option<(LintLevel, Vec<String>)> {
    if let Some(rest) = body.strip_prefix("ndsl:") {
        let (level, ids, after) = parse_level_and_ids(rest)?;
        return if after.trim().is_empty() {
            Some((level, ids))
        } else {
            None
        };
    }

    let rest = body.strip_prefix('#')?;
    let rest = rest.strip_prefix('!').unwrap_or(rest);
    let rest = rest.strip_prefix('[')?.trim_start();
    let (level, ids, after) = parse_level_and_ids(rest)?;
    let after = after.trim_start();
    if after.is_empty() {
        return Some((level, ids));
    }
    let after_bracket = after.strip_prefix(']')?;
    if after_bracket.trim().is_empty() {
        Some((level, ids))
    } else {
        None
    }
}

/// Parse `allow(ID, ID2)` → `(level, ids, remainder_after_close_paren)`.
fn parse_level_and_ids(s: &str) -> Option<(LintLevel, Vec<String>, &str)> {
    let s = s.trim_start();
    let (level_str, rest) = split_ident(s)?;
    let level = LintLevel::parse(level_str)?;
    let rest = rest.trim_start().strip_prefix('(')?;
    let close = rest.find(')')?;
    let ids = parse_id_list(&rest[..close])?;
    if ids.is_empty() {
        return None;
    }
    Some((level, ids, &rest[close + 1..]))
}

fn split_ident(s: &str) -> Option<(&str, &str)> {
    let end = s
        .char_indices()
        .find(|(_, c)| !c.is_ascii_alphabetic())
        .map(|(i, _)| i)
        .unwrap_or(s.len());
    if end == 0 {
        return None;
    }
    Some((&s[..end], &s[end..]))
}

fn parse_id_list(raw: &str) -> Option<Vec<String>> {
    let mut ids = Vec::new();
    for part in raw.split(',') {
        let id = part.trim();
        if id.is_empty() {
            continue;
        }
        if !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return None;
        }
        ids.push(id.to_ascii_uppercase());
    }
    Some(ids)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hash_allow() {
        let m = parse_file_attrs("// #[allow(NFDL0001)]\nprotocol P {}\n");
        assert_eq!(m.get("NFDL0001"), Some(&LintLevel::Allow));
    }

    #[test]
    fn parses_hash_deny_and_ndsl_form() {
        let m = parse_file_attrs("// #[deny(ADGLS0100)]\n// ndsl:allow(NFDL0001)\n");
        assert_eq!(m.get("ADGLS0100"), Some(&LintLevel::Deny));
        assert_eq!(m.get("NFDL0001"), Some(&LintLevel::Allow));
    }

    #[test]
    fn parses_inner_attr_and_comma_list() {
        let m = parse_file_attrs("// #![warn(NFDL0001, nfdl0002)]\n");
        assert_eq!(m.get("NFDL0001"), Some(&LintLevel::Warn));
        assert_eq!(m.get("NFDL0002"), Some(&LintLevel::Warn));
    }

    #[test]
    fn later_directive_wins() {
        let m = parse_file_attrs("// #[allow(NFDL0001)]\n// #[deny(NFDL0001)]\n");
        assert_eq!(m.get("NFDL0001"), Some(&LintLevel::Deny));
    }

    #[test]
    fn ignores_non_directive_comments() {
        let m = parse_file_attrs("// just a note\nprotocol P {}\n");
        assert!(m.is_empty());
    }

    #[test]
    fn parses_forbid_as_deny() {
        let m = parse_file_attrs("// ndsl:forbid(NFDL0900)\n");
        assert_eq!(m.get("NFDL0900"), Some(&LintLevel::Deny));
    }
}
