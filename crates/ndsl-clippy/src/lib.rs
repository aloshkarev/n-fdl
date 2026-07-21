//! Clippy-style lint driver for N-FDL and ADGL.
//!
//! Lint identifiers and levels are defined in `docs/tooling/lints.md`.
//! Built-in lint packs register via [`LintStore::register_builtin`] (Wave 3).

#![forbid(unsafe_code)]

use std::fmt;
use std::str::FromStr;

pub use ndsl_diag::Span;

/// Stable lint identifier (e.g. `NFDL0001`, `ADGLS0042`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LintId(pub &'static str);

impl LintId {
    pub const fn new(code: &'static str) -> Self {
        Self(code)
    }

    pub fn as_str(self) -> &'static str {
        self.0
    }

    /// Returns `true` when `code` matches the reserved N-FDL or ADGL style ranges.
    pub fn is_valid(code: &str) -> bool {
        is_nfdl_style_lint(code) || is_adgl_style_lint(code)
    }
}

impl fmt::Display for LintId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

/// Per-lint enforcement level (rustc/clippy-style).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LintLevel {
    Allow,
    #[default]
    Warn,
    Deny,
}

impl LintLevel {
    /// Parse a level name (`allow`, `warn`, `deny`), case-insensitive.
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "allow" => Some(Self::Allow),
            "warn" | "warning" => Some(Self::Warn),
            "deny" | "forbid" => Some(Self::Deny),
            _ => None,
        }
    }
}

impl FromStr for LintLevel {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s).ok_or(())
    }
}

impl fmt::Display for LintLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Allow => "allow",
            Self::Warn => "warn",
            Self::Deny => "deny",
        })
    }
}

/// A single lint finding with source span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LintDiagnostic {
    pub id: LintId,
    pub level: LintLevel,
    pub message: String,
    pub span: Span,
}

impl LintDiagnostic {
    pub fn new(id: LintId, level: LintLevel, message: impl Into<String>, span: Span) -> Self {
        Self {
            id,
            level,
            message: message.into(),
            span,
        }
    }
}

/// Registry of lint definitions and effective levels.
#[derive(Debug, Default)]
pub struct LintStore {
    _private: (),
}

impl LintStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register built-in N-FDL and ADGL lint packs.
    ///
    /// Wave 0 stub: no-op until lint packs land in Wave 3.
    pub fn register_builtin(&mut self) {}
}

fn is_nfdl_style_lint(code: &str) -> bool {
    let Some(digits) = code.strip_prefix("NFDL") else {
        return false;
    };
    digits.len() == 4 && digits.chars().all(|c| c.is_ascii_digit()) && digits <= "0999"
}

fn is_adgl_style_lint(code: &str) -> bool {
    let Some(digits) = code.strip_prefix("ADGLS") else {
        return false;
    };
    digits.len() == 4 && digits.chars().all(|c| c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lint_level_parse_accepts_canonical_names() {
        assert_eq!(LintLevel::parse("allow"), Some(LintLevel::Allow));
        assert_eq!(LintLevel::parse("warn"), Some(LintLevel::Warn));
        assert_eq!(LintLevel::parse("deny"), Some(LintLevel::Deny));
    }

    #[test]
    fn lint_level_parse_is_case_insensitive() {
        assert_eq!(LintLevel::parse("ALLOW"), Some(LintLevel::Allow));
        assert_eq!(LintLevel::parse("Warn"), Some(LintLevel::Warn));
        assert_eq!(LintLevel::parse("DENY"), Some(LintLevel::Deny));
    }

    #[test]
    fn lint_level_parse_accepts_aliases() {
        assert_eq!(LintLevel::parse("warning"), Some(LintLevel::Warn));
        assert_eq!(LintLevel::parse("forbid"), Some(LintLevel::Deny));
    }

    #[test]
    fn lint_level_parse_trims_whitespace() {
        assert_eq!(LintLevel::parse("  warn  "), Some(LintLevel::Warn));
    }

    #[test]
    fn lint_level_parse_rejects_unknown() {
        assert_eq!(LintLevel::parse(""), None);
        assert_eq!(LintLevel::parse("off"), None);
        assert_eq!(LintLevel::parse("error"), None);
    }

    #[test]
    fn lint_level_from_str_matches_parse() {
        assert_eq!("deny".parse(), Ok(LintLevel::Deny));
        assert!("bogus".parse::<LintLevel>().is_err());
    }

    #[test]
    fn lint_level_display_round_trips() {
        for level in [LintLevel::Allow, LintLevel::Warn, LintLevel::Deny] {
            assert_eq!(LintLevel::parse(&level.to_string()), Some(level));
        }
    }

    #[test]
    fn lint_id_validates_reserved_ranges() {
        assert!(LintId::is_valid("NFDL0001"));
        assert!(LintId::is_valid("NFDL0999"));
        assert!(LintId::is_valid("ADGLS0001"));
        assert!(LintId::is_valid("ADGLS9999"));

        assert!(!LintId::is_valid("NFDL1000"));
        assert!(!LintId::is_valid("NFDL001"));
        assert!(!LintId::is_valid("ADGL0001"));
        assert!(!LintId::is_valid("ADGLS001"));
        assert!(!LintId::is_valid("NFD001"));
    }

    #[test]
    fn lint_store_register_builtin_is_noop_stub() {
        let mut store = LintStore::new();
        store.register_builtin();
    }
}
