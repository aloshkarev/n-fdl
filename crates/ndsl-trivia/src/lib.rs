//! Trivia capture for N-FDL and ADGL formatters.
//!
//! Lexers attach comments, whitespace, and newlines as leading or trailing
//! trivia around AST nodes. Spans come from [`ndsl_diag::Span`].

#![forbid(unsafe_code)]

pub use ndsl_diag::Span;

/// Kind of non-semantic source text preserved for formatting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriviaKind {
    /// Ordinary `//` line comment (not a doc-comment).
    LineComment,
    /// Outer doc-comment line starting with `///` (but not `////`).
    DocComment,
    BlockComment,
    Whitespace,
    Newline,
}

/// Classify a line-comment lexeme (`//…` through end of line, no trailing newline).
///
/// `/// text` → [`TriviaKind::DocComment`]; `////…` and plain `//…` → [`TriviaKind::LineComment`].
pub fn classify_line_comment(text: &str) -> TriviaKind {
    if text.starts_with("///") && !text.starts_with("////") {
        TriviaKind::DocComment
    } else {
        TriviaKind::LineComment
    }
}

/// Join leading `///` doc-comments into a single doc string, or `None` if absent.
///
/// Strips the `///` prefix and one optional following space per line; multiple
/// consecutive doc lines are joined with `\n`. Non-doc trivia is ignored.
pub fn docs_from_leading(trivia: &[Trivia]) -> Option<String> {
    let mut lines = Vec::new();
    for t in trivia {
        if t.kind != TriviaKind::DocComment {
            continue;
        }
        lines.push(strip_doc_prefix(&t.text));
    }
    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n"))
    }
}

fn strip_doc_prefix(text: &str) -> String {
    let rest = text.strip_prefix("///").unwrap_or(text);
    rest.strip_prefix(' ').unwrap_or(rest).to_owned()
}

/// A single trivia fragment with its source span and exact text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trivia {
    pub kind: TriviaKind,
    pub span: Span,
    pub text: String,
}

/// Leading and trailing trivia attached to a syntactic element.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TriviaBag {
    pub leading: Vec<Trivia>,
    pub trailing: Vec<Trivia>,
}

impl TriviaBag {
    pub fn new() -> Self {
        Self::default()
    }

    /// Append `trivia` to the end of the leading sequence (preserves order).
    pub fn attach_leading(&mut self, trivia: Trivia) {
        self.leading.push(trivia);
    }

    /// Append `trivia` to the end of the trailing sequence (preserves order).
    pub fn attach_trailing(&mut self, trivia: Trivia) {
        self.trailing.push(trivia);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trivia(kind: TriviaKind, start: usize, end: usize, text: &str) -> Trivia {
        Trivia {
            kind,
            span: Span::new(start, end),
            text: text.to_owned(),
        }
    }

    #[test]
    fn empty_bag_has_no_trivia() {
        let bag = TriviaBag::new();
        assert!(bag.leading.is_empty());
        assert!(bag.trailing.is_empty());
    }

    #[test]
    fn attach_leading_preserves_order() {
        let mut bag = TriviaBag::new();
        let first = trivia(TriviaKind::LineComment, 0, 10, "// comment");
        let second = trivia(TriviaKind::Newline, 10, 11, "\n");
        bag.attach_leading(first.clone());
        bag.attach_leading(second.clone());
        assert_eq!(bag.leading, vec![first, second]);
        assert!(bag.trailing.is_empty());
    }

    #[test]
    fn attach_trailing_preserves_order() {
        let mut bag = TriviaBag::new();
        let first = trivia(TriviaKind::Whitespace, 5, 6, " ");
        let second = trivia(TriviaKind::BlockComment, 6, 13, "/* x */");
        bag.attach_trailing(first.clone());
        bag.attach_trailing(second.clone());
        assert_eq!(bag.trailing, vec![first, second]);
        assert!(bag.leading.is_empty());
    }

    #[test]
    fn leading_and_trailing_are_independent() {
        let mut bag = TriviaBag::new();
        let leading = trivia(TriviaKind::LineComment, 0, 2, "//");
        let trailing = trivia(TriviaKind::Newline, 8, 9, "\n");
        bag.attach_leading(leading.clone());
        bag.attach_trailing(trailing.clone());
        assert_eq!(bag.leading, vec![leading]);
        assert_eq!(bag.trailing, vec![trailing]);
    }

    #[test]
    fn span_reexport_matches_ndsl_diag() {
        let span = Span::new(3, 7);
        let item = trivia(TriviaKind::Whitespace, span.start, span.end, "    ");
        assert_eq!(item.span, span);
    }

    #[test]
    fn classify_line_comment_distinguishes_doc() {
        assert_eq!(classify_line_comment("// note"), TriviaKind::LineComment);
        assert_eq!(classify_line_comment("/// hello"), TriviaKind::DocComment);
        assert_eq!(classify_line_comment("//// not doc"), TriviaKind::LineComment);
        assert_eq!(classify_line_comment("///"), TriviaKind::DocComment);
    }

    #[test]
    fn docs_from_leading_joins_doc_lines() {
        assert_eq!(docs_from_leading(&[]), None);
        let leading = vec![
            trivia(TriviaKind::LineComment, 0, 7, "// skip"),
            trivia(TriviaKind::DocComment, 8, 17, "/// hello"),
            trivia(TriviaKind::Newline, 17, 18, "\n"),
            trivia(TriviaKind::DocComment, 18, 27, "/// world"),
        ];
        assert_eq!(
            docs_from_leading(&leading).as_deref(),
            Some("hello\nworld")
        );
    }
}
