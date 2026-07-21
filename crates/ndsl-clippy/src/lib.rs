//! Clippy-style lint driver for N-FDL and ADGL.
//!
//! Lint identifiers and levels are defined in `docs/tooling/lints.md`.
//! Built-in lint packs register via [`LintStore::register_builtin`].

#![forbid(unsafe_code)]

mod builtin;
mod render;

use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::str::FromStr;

pub use builtin::NFDL_EMPTY_FILE;
pub use ndsl_diag::Span;
pub use render::{render, RenderFormat};

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

/// Lint attached to the file it was found in (plus source for rendering).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileDiagnostic {
    pub path: PathBuf,
    pub source: String,
    pub diagnostic: LintDiagnostic,
}

/// Source language of a linted file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LintLang {
    Nfdl,
    Adgl,
}

/// Context passed to each registered lint check.
#[derive(Debug)]
pub struct LintContext<'a> {
    pub path: &'a Path,
    pub source: &'a str,
    pub lang: LintLang,
}

/// Static metadata for a registered lint.
#[derive(Debug, Clone, Copy)]
pub struct LintDef {
    pub id: LintId,
    pub default_level: LintLevel,
    pub description: &'static str,
}

/// Function pointer invoked per file for a registered lint.
pub type LintCheck = fn(&LintContext<'_>) -> Vec<LintDiagnostic>;

struct LintEntry {
    def: LintDef,
    check: LintCheck,
}

/// Errors while discovering or reading lint targets.
#[derive(Debug)]
pub enum WalkError {
    Io(PathBuf, io::Error),
    Unsupported(PathBuf),
    UnknownLint(String),
}

impl fmt::Display for WalkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(path, err) => write!(f, "cannot read {}: {err}", path.display()),
            Self::Unsupported(path) => write!(
                f,
                "unsupported extension for {} (expected .nfdl or .adgl)",
                path.display()
            ),
            Self::UnknownLint(id) => write!(f, "unknown lint id `{id}`"),
        }
    }
}

impl std::error::Error for WalkError {}

/// Registry of lint definitions and effective levels.
#[derive(Default)]
pub struct LintStore {
    entries: HashMap<&'static str, LintEntry>,
    overrides: HashMap<String, LintLevel>,
}

impl fmt::Debug for LintStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LintStore")
            .field("lints", &self.entries.keys().collect::<Vec<_>>())
            .field("overrides", &self.overrides)
            .finish()
    }
}

impl LintStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a lint definition and its check function.
    ///
    /// Panics if `def.id` is not a valid style lint id or is already registered.
    pub fn register(&mut self, def: LintDef, check: LintCheck) {
        assert!(
            LintId::is_valid(def.id.as_str()),
            "invalid lint id {}",
            def.id
        );
        let key = def.id.as_str();
        assert!(
            !self.entries.contains_key(key),
            "duplicate lint registration for {key}"
        );
        self.entries.insert(key, LintEntry { def, check });
    }

    /// Register built-in N-FDL and ADGL lint packs (demo engine-smoke for now).
    pub fn register_builtin(&mut self) {
        builtin::register_builtins(self);
    }

    /// Override the effective level for a registered lint (`--allow` / `--deny`).
    pub fn set_level(&mut self, id: &str, level: LintLevel) -> Result<(), WalkError> {
        if !self.entries.contains_key(id) {
            return Err(WalkError::UnknownLint(id.to_string()));
        }
        self.overrides.insert(id.to_string(), level);
        Ok(())
    }

    /// Effective level after CLI / future attribute overrides.
    pub fn effective_level(&self, id: LintId) -> LintLevel {
        if let Some(level) = self.overrides.get(id.as_str()) {
            return *level;
        }
        self.entries
            .get(id.as_str())
            .map(|e| e.def.default_level)
            .unwrap_or(LintLevel::Warn)
    }

    /// Number of registered lints.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Walk `paths` (files or directories), run all registered lints, collect findings.
    ///
    /// Allowed findings are omitted. Directory paths are walked recursively for
    /// `.nfdl` / `.adgl` files.
    pub fn lint_paths(&self, paths: &[PathBuf]) -> Result<Vec<FileDiagnostic>, WalkError> {
        let mut files = Vec::new();
        for path in paths {
            collect_targets(path, &mut files)?;
        }
        files.sort();
        files.dedup();

        let mut out = Vec::new();
        for path in files {
            out.extend(self.lint_file(&path)?);
        }
        Ok(out)
    }

    /// Lint a single `.nfdl` / `.adgl` file.
    pub fn lint_file(&self, path: &Path) -> Result<Vec<FileDiagnostic>, WalkError> {
        let lang = detect_lang(path).ok_or_else(|| WalkError::Unsupported(path.to_path_buf()))?;
        let source = fs::read_to_string(path).map_err(|e| WalkError::Io(path.to_path_buf(), e))?;
        Ok(self.lint_source(path, &source, lang))
    }

    /// Run registered checks against an in-memory source (used by tests and CLI).
    pub fn lint_source(&self, path: &Path, source: &str, lang: LintLang) -> Vec<FileDiagnostic> {
        let ctx = LintContext {
            path,
            source,
            lang,
        };
        let mut out = Vec::new();
        // Stable order by lint id for deterministic output.
        let mut keys: Vec<&'static str> = self.entries.keys().copied().collect();
        keys.sort_unstable();
        for key in keys {
            let entry = &self.entries[key];
            let level = self.effective_level(entry.def.id);
            if level == LintLevel::Allow {
                continue;
            }
            for mut diag in (entry.check)(&ctx) {
                diag.level = level;
                if diag.level == LintLevel::Allow {
                    continue;
                }
                out.push(FileDiagnostic {
                    path: path.to_path_buf(),
                    source: source.to_string(),
                    diagnostic: diag,
                });
            }
        }
        out
    }

    /// True when any collected diagnostic is at deny level.
    pub fn has_deny(diagnostics: &[FileDiagnostic]) -> bool {
        diagnostics
            .iter()
            .any(|d| d.diagnostic.level == LintLevel::Deny)
    }
}

fn detect_lang(path: &Path) -> Option<LintLang> {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .as_deref()
    {
        Some("nfdl") => Some(LintLang::Nfdl),
        Some("adgl") => Some(LintLang::Adgl),
        _ => None,
    }
}

fn collect_targets(path: &Path, out: &mut Vec<PathBuf>) -> Result<(), WalkError> {
    collect_targets_inner(path, out, /*require_lang=*/ true)
}

fn collect_targets_inner(
    path: &Path,
    out: &mut Vec<PathBuf>,
    require_lang: bool,
) -> Result<(), WalkError> {
    let meta = fs::metadata(path).map_err(|e| WalkError::Io(path.to_path_buf(), e))?;
    if meta.is_dir() {
        let entries = fs::read_dir(path).map_err(|e| WalkError::Io(path.to_path_buf(), e))?;
        for entry in entries {
            let entry = entry.map_err(|e| WalkError::Io(path.to_path_buf(), e))?;
            // Nested files with other extensions are skipped; only top-level
            // explicit paths must be .nfdl / .adgl.
            collect_targets_inner(&entry.path(), out, /*require_lang=*/ false)?;
        }
        return Ok(());
    }
    if detect_lang(path).is_none() {
        if require_lang {
            return Err(WalkError::Unsupported(path.to_path_buf()));
        }
        return Ok(());
    }
    out.push(path.to_path_buf());
    Ok(())
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
    use std::io::Write;

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
    fn lint_store_register_builtin_registers_demo() {
        let mut store = LintStore::new();
        store.register_builtin();
        assert_eq!(store.len(), 1);
        assert_eq!(
            store.effective_level(NFDL_EMPTY_FILE),
            LintLevel::Warn
        );
    }

    #[test]
    fn lint_store_set_level_allow_suppresses_finding() {
        let mut store = LintStore::new();
        store.register_builtin();
        store
            .set_level(NFDL_EMPTY_FILE.as_str(), LintLevel::Allow)
            .unwrap();

        let diags = store.lint_source(Path::new("empty.nfdl"), "   \n", LintLang::Nfdl);
        assert!(diags.is_empty());
    }

    #[test]
    fn lint_store_set_level_deny_promotes_finding() {
        let mut store = LintStore::new();
        store.register_builtin();
        store
            .set_level(NFDL_EMPTY_FILE.as_str(), LintLevel::Deny)
            .unwrap();

        let diags = store.lint_source(Path::new("empty.nfdl"), "", LintLang::Nfdl);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].diagnostic.level, LintLevel::Deny);
        assert!(LintStore::has_deny(&diags));
    }

    #[test]
    fn lint_store_walk_skips_clean_nonempty_file() {
        let dir = std::env::temp_dir().join(format!(
            "ndsl-clippy-walk-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("ok.nfdl");
        {
            let mut f = fs::File::create(&path).unwrap();
            writeln!(f, "protocol P {{}}").unwrap();
        }

        let mut store = LintStore::new();
        store.register_builtin();
        let diags = store.lint_paths(&[path.clone()]).unwrap();
        assert!(diags.is_empty());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn lint_store_walk_directory_finds_empty_adgl() {
        let dir = std::env::temp_dir().join(format!(
            "ndsl-clippy-dir-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let empty = dir.join("empty.adgl");
        fs::write(&empty, "").unwrap();
        fs::write(dir.join("ok.nfdl"), "protocol P {}\n").unwrap();

        let mut store = LintStore::new();
        store.register_builtin();
        let diags = store.lint_paths(&[dir.clone()]).unwrap();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].path, empty);
        assert_eq!(diags[0].diagnostic.id, NFDL_EMPTY_FILE);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn lint_store_rejects_unknown_level_override() {
        let mut store = LintStore::new();
        store.register_builtin();
        let err = store.set_level("NFDL0001", LintLevel::Deny).unwrap_err();
        assert!(matches!(err, WalkError::UnknownLint(_)));
    }

    #[test]
    fn lint_store_rejects_unsupported_extension() {
        let path = std::env::temp_dir().join(format!(
            "ndsl-clippy-notes-{}.txt",
            std::process::id()
        ));
        fs::write(&path, "hello").unwrap();

        let mut store = LintStore::new();
        store.register_builtin();
        let err = store.lint_paths(&[path.clone()]).unwrap_err();
        assert!(matches!(err, WalkError::Unsupported(_)));

        let _ = fs::remove_file(&path);
    }
}
