//! ADGL parser (`docs/idea/spec/02-grammar.ebnf`) implemented with winnow-powered lexing.

use std::borrow::Cow;

use airpulse_dsl_types::{ActionKind, ScopeType, Severity};
use ndsl_diag::{DiagBuffer, Diagnostic, Span};
use ndsl_trivia::{Trivia, TriviaKind};
use winnow::Parser;
use winnow::error::InputError;
use winnow::token::take_while;

use crate::ast::{
    ActionField, ActionName, ActionStmt, AnchorBlock, BinaryOp, CauseAnchor, CorrelateBlock,
    CorrelateSource, DecisionAnchor, DecisionRule, Decl, DurationLit, EmitField, EmitStmt,
    EvidenceRule, Expr, ExprKind, Ident, IfElseBlock, InferField, InferStmt, IntLit, KindIdent,
    MutuallyExclusiveDecl, ProblemAnchor, RequiresDecl, RuleDecl, Ruleset, RulesetHeader, Stmt,
    StringLit, TimeWindow, TopoPredicate, UnaryOp,
};

// ============================================================================
// Token model and parse error envelope
// ============================================================================

#[derive(Debug, Clone, PartialEq)]
enum TokenKind<'a> {
    Ident(&'a str),
    String(String),
    Int(i64),
    Duration(i64),
    LBrace,
    RBrace,
    LParen,
    RParen,
    LBracket,
    RBracket,
    Colon,
    Comma,
    Dot,
    Eq,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Bang,
    Lt,
    Le,
    Gt,
    Ge,
    EqEq,
    Ne,
    AndAnd,
    OrOr,
    Kw(&'static str),
    Eof,
}

#[derive(Debug, Clone, PartialEq)]
struct Token<'a> {
    kind: TokenKind<'a>,
    span: Span,
}

#[derive(Debug, Clone)]
struct ParseErr {
    code: &'static str,
    message: Cow<'static, str>,
    expected: Cow<'static, str>,
    span: Span,
}

impl ParseErr {
    fn new(
        code: &'static str,
        message: impl Into<Cow<'static, str>>,
        expected: impl Into<Cow<'static, str>>,
        span: Span,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            expected: expected.into(),
            span,
        }
    }
}

const KEYWORDS: &[&str] = &[
    "ruleset",
    "version",
    "requires",
    "mutually_exclusive",
    "evidence",
    "decision",
    "scope",
    "anchor",
    "correlate",
    "having",
    "infer",
    "emit",
    "action",
    "if",
    "else",
    "present",
    "absent",
    "and",
    "or",
    "not",
    "Cause",
    "Problem",
    "event",
    "in",
    "true",
    "false",
];

// ============================================================================
// Public parse entry points
// ============================================================================

/// Parse an ADGL ruleset from source.
///
/// On rule/decl-level syntax errors the parser records a diagnostic, resyncs to
/// the next ruleset member (`}` or keywords `evidence` / `decision` /
/// `mutually_exclusive` / `version` / `requires`), and continues so later errors
/// accumulate in the same [`DiagBuffer`]. A successful AST is returned only when
/// no diagnostics were recorded; otherwise `Err(buf)` carries every diagnostic.
pub fn parse_ruleset<'a>(src: &'a str) -> Result<Ruleset<'a>, DiagBuffer> {
    match tokenize(src) {
        Ok(tokens) => {
            let mut p = ParserState::new(tokens);
            p.recover = true;
            match p.parse_ruleset() {
                Ok(ast) if p.diags.is_empty() => Ok(ast),
                Ok(_) => Err(std::mem::take(&mut p.diags)),
                Err(err) => {
                    p.record_error(err);
                    Err(std::mem::take(&mut p.diags))
                }
            }
        }
        Err(err) => Err(diag_from_err(err)),
    }
}

/// Fail-fast variant: stops at the first syntax error (no rule/decl recovery).
#[cfg(test)]
fn parse_ruleset_fail_fast<'a>(src: &'a str) -> Result<Ruleset<'a>, DiagBuffer> {
    match tokenize(src) {
        Ok(tokens) => match ParserState::new(tokens).parse_ruleset() {
            Ok(ast) => Ok(ast),
            Err(err) => Err(diag_from_err(err)),
        },
        Err(err) => Err(diag_from_err(err)),
    }
}

/// Parse an expression for precedence-focused tests.
pub fn parse_expression<'a>(src: &'a str) -> Result<Expr<'a>, DiagBuffer> {
    match tokenize(src) {
        Ok(tokens) => {
            let mut p = ParserState::new(tokens);
            match p.parse_expr() {
                Ok(expr) => {
                    if !matches!(p.peek_kind(), TokenKind::Eof) {
                        Err(diag_from_err(ParseErr::new(
                            "ADGL0100",
                            "unexpected trailing tokens",
                            "end of input",
                            p.peek().span,
                        )))
                    } else {
                        Ok(expr)
                    }
                }
                Err(err) => Err(diag_from_err(err)),
            }
        }
        Err(err) => Err(diag_from_err(err)),
    }
}

fn diag_from_err(err: ParseErr) -> DiagBuffer {
    let mut buf = DiagBuffer::new();
    buf.push(diag_of(err));
    buf
}

fn diag_of(err: ParseErr) -> Diagnostic {
    Diagnostic::error(
        err.code,
        format!("{} (expected: {})", err.message, err.expected),
        err.span,
    )
}

// ============================================================================
// Lexer / tokenizer
// ============================================================================

fn tokenize<'a>(src: &'a str) -> Result<Vec<Token<'a>>, ParseErr> {
    if src.len() > 4 * 1024 * 1024 {
        return Err(ParseErr::new(
            "ADGL0102",
            "source exceeds 4 MiB limit",
            "input <= 4 MiB",
            Span::new(0, src.len()),
        ));
    }

    let mut lx = Lexer::new(src);
    let mut out = Vec::new();
    let mut depth: usize = 0;
    while !lx.is_eof() {
        lx.skip_ws_and_comments()?;
        if lx.is_eof() {
            break;
        }
        let tok = lx.next_token()?;
        match tok.kind {
            TokenKind::LBrace | TokenKind::LParen | TokenKind::LBracket => {
                depth += 1;
                if depth > 64 {
                    return Err(ParseErr::new(
                        "ADGL0103",
                        "nesting depth exceeds 64",
                        "nesting depth <= 64",
                        tok.span,
                    ));
                }
            }
            TokenKind::RBrace | TokenKind::RParen | TokenKind::RBracket => {
                depth = depth.saturating_sub(1);
            }
            _ => {}
        }
        out.push(tok);
    }
    out.push(Token {
        kind: TokenKind::Eof,
        span: Span::new(src.len(), src.len()),
    });
    Ok(out)
}

struct Lexer<'a> {
    src: &'a str,
    pos: usize,
    /// Trivia collected while skipping ahead to the most recent token.
    pending_trivia: Vec<Trivia>,
}

impl<'a> Lexer<'a> {
    fn new(src: &'a str) -> Self {
        Self {
            src,
            pos: 0,
            pending_trivia: Vec::new(),
        }
    }

    /// Take trivia collected immediately before the most recent token cycle
    /// (`skip_ws_and_comments` + `next_token`).
    ///
    /// Formatters and AST attach will drain this; unit tests cover the API today.
    #[allow(dead_code)]
    fn trivia_before_next_token(&mut self) -> Vec<Trivia> {
        std::mem::take(&mut self.pending_trivia)
    }

    fn is_eof(&self) -> bool {
        self.pos >= self.src.len()
    }

    fn rest(&self) -> &'a str {
        &self.src[self.pos..]
    }

    fn bump_char(&mut self) -> Option<char> {
        let rest = self.rest();
        let ch = rest.chars().next()?;
        self.pos += ch.len_utf8();
        Some(ch)
    }

    fn peek_char(&self) -> Option<char> {
        self.rest().chars().next()
    }

    fn skip_ws_and_comments(&mut self) -> Result<(), ParseErr> {
        self.pending_trivia.clear();
        loop {
            let mut consumed = false;
            while matches!(self.peek_char(), Some(' ' | '\t' | '\r' | '\n')) {
                let _ = self.bump_char();
                consumed = true;
            }
            if self.rest().starts_with("//") {
                consumed = true;
                let start = self.pos;
                while let Some(ch) = self.peek_char() {
                    if ch == '\n' {
                        break;
                    }
                    let _ = self.bump_char();
                }
                let end = self.pos;
                self.pending_trivia.push(Trivia {
                    kind: TriviaKind::LineComment,
                    span: Span::new(start, end),
                    text: self.src[start..end].to_owned(),
                });
            } else if self.rest().starts_with("/*") {
                consumed = true;
                let start = self.pos;
                let _ = self.bump_char();
                let _ = self.bump_char();
                while !self.is_eof() && !self.rest().starts_with("*/") {
                    let _ = self.bump_char();
                }
                if self.is_eof() {
                    return Err(ParseErr::new(
                        "ADGL0106",
                        "unclosed block comment",
                        "closing */",
                        Span::new(start, start + 2),
                    ));
                }
                let _ = self.bump_char();
                let _ = self.bump_char();
                let end = self.pos;
                self.pending_trivia.push(Trivia {
                    kind: TriviaKind::BlockComment,
                    span: Span::new(start, end),
                    text: self.src[start..end].to_owned(),
                });
            }
            if !consumed {
                break;
            }
        }
        Ok(())
    }

    fn next_token(&mut self) -> Result<Token<'a>, ParseErr> {
        let start = self.pos;
        let rest = self.rest();

        if rest.starts_with("&&") {
            self.pos += 2;
            return Ok(Token {
                kind: TokenKind::AndAnd,
                span: Span::new(start, self.pos),
            });
        }
        if rest.starts_with("||") {
            self.pos += 2;
            return Ok(Token {
                kind: TokenKind::OrOr,
                span: Span::new(start, self.pos),
            });
        }
        if rest.starts_with("==") {
            self.pos += 2;
            return Ok(Token {
                kind: TokenKind::EqEq,
                span: Span::new(start, self.pos),
            });
        }
        if rest.starts_with("!=") {
            self.pos += 2;
            return Ok(Token {
                kind: TokenKind::Ne,
                span: Span::new(start, self.pos),
            });
        }
        if rest.starts_with("<=") {
            self.pos += 2;
            return Ok(Token {
                kind: TokenKind::Le,
                span: Span::new(start, self.pos),
            });
        }
        if rest.starts_with(">=") {
            self.pos += 2;
            return Ok(Token {
                kind: TokenKind::Ge,
                span: Span::new(start, self.pos),
            });
        }

        match self.peek_char() {
            Some('{') => {
                let _ = self.bump_char();
                Ok(Token {
                    kind: TokenKind::LBrace,
                    span: Span::new(start, self.pos),
                })
            }
            Some('}') => {
                let _ = self.bump_char();
                Ok(Token {
                    kind: TokenKind::RBrace,
                    span: Span::new(start, self.pos),
                })
            }
            Some('(') => {
                let _ = self.bump_char();
                Ok(Token {
                    kind: TokenKind::LParen,
                    span: Span::new(start, self.pos),
                })
            }
            Some(')') => {
                let _ = self.bump_char();
                Ok(Token {
                    kind: TokenKind::RParen,
                    span: Span::new(start, self.pos),
                })
            }
            Some('[') => {
                let _ = self.bump_char();
                Ok(Token {
                    kind: TokenKind::LBracket,
                    span: Span::new(start, self.pos),
                })
            }
            Some(']') => {
                let _ = self.bump_char();
                Ok(Token {
                    kind: TokenKind::RBracket,
                    span: Span::new(start, self.pos),
                })
            }
            Some(':') => {
                let _ = self.bump_char();
                Ok(Token {
                    kind: TokenKind::Colon,
                    span: Span::new(start, self.pos),
                })
            }
            Some(',') => {
                let _ = self.bump_char();
                Ok(Token {
                    kind: TokenKind::Comma,
                    span: Span::new(start, self.pos),
                })
            }
            Some('.') => {
                let _ = self.bump_char();
                Ok(Token {
                    kind: TokenKind::Dot,
                    span: Span::new(start, self.pos),
                })
            }
            Some('=') => {
                let _ = self.bump_char();
                Ok(Token {
                    kind: TokenKind::Eq,
                    span: Span::new(start, self.pos),
                })
            }
            Some('+') => {
                let _ = self.bump_char();
                Ok(Token {
                    kind: TokenKind::Plus,
                    span: Span::new(start, self.pos),
                })
            }
            Some('-') => {
                let _ = self.bump_char();
                Ok(Token {
                    kind: TokenKind::Minus,
                    span: Span::new(start, self.pos),
                })
            }
            Some('*') => {
                let _ = self.bump_char();
                Ok(Token {
                    kind: TokenKind::Star,
                    span: Span::new(start, self.pos),
                })
            }
            Some('/') => {
                let _ = self.bump_char();
                Ok(Token {
                    kind: TokenKind::Slash,
                    span: Span::new(start, self.pos),
                })
            }
            Some('%') => {
                let _ = self.bump_char();
                Ok(Token {
                    kind: TokenKind::Percent,
                    span: Span::new(start, self.pos),
                })
            }
            Some('!') => {
                let _ = self.bump_char();
                Ok(Token {
                    kind: TokenKind::Bang,
                    span: Span::new(start, self.pos),
                })
            }
            Some('<') => {
                let _ = self.bump_char();
                Ok(Token {
                    kind: TokenKind::Lt,
                    span: Span::new(start, self.pos),
                })
            }
            Some('>') => {
                let _ = self.bump_char();
                Ok(Token {
                    kind: TokenKind::Gt,
                    span: Span::new(start, self.pos),
                })
            }
            Some('"') => self.lex_string(start),
            Some(ch) if ch.is_ascii_digit() => self.lex_number_or_duration(start),
            Some(ch) if is_ident_start(ch) => self.lex_ident_or_kw(start),
            Some(_) => {
                let _ = self.bump_char();
                Err(ParseErr::new(
                    "ADGL0100",
                    "unexpected character",
                    "token",
                    Span::new(start, self.pos),
                ))
            }
            None => Ok(Token {
                kind: TokenKind::Eof,
                span: Span::new(self.pos, self.pos),
            }),
        }
    }

    fn lex_ident_or_kw(&mut self, start: usize) -> Result<Token<'a>, ParseErr> {
        let mut input = self.rest();
        let parsed = take_run(&mut input, is_ident_continue).map_err(|_| {
            ParseErr::new(
                "ADGL0100",
                "malformed identifier",
                "identifier",
                Span::new(start, start + 1),
            )
        })?;
        if parsed.len() > 255 {
            return Err(ParseErr::new(
                "ADGL0101",
                "identifier exceeds 255 bytes",
                "identifier length <= 255",
                Span::new(start, start + parsed.len()),
            ));
        }
        if parsed.starts_with("__")
            && !matches!(
                parsed,
                "__watermark" | "__scope" | "__confidence" | "__ruleset_version"
            )
        {
            return Err(ParseErr::new(
                "ADGL0104",
                "identifier prefix '__' is reserved",
                "builtin identifier",
                Span::new(start, start + parsed.len()),
            ));
        }
        self.pos += parsed.len();
        let kind = if KEYWORDS.contains(&parsed) {
            TokenKind::Kw(keyword_static(parsed))
        } else {
            TokenKind::Ident(parsed)
        };
        Ok(Token {
            kind,
            span: Span::new(start, self.pos),
        })
    }

    fn lex_number_or_duration(&mut self, start: usize) -> Result<Token<'a>, ParseErr> {
        let rest = self.rest();
        let mut end = 0usize;
        let mut radix = 10u32;
        if rest.starts_with("0x") || rest.starts_with("0X") {
            radix = 16;
            end = 2;
            while let Some(ch) = rest[end..].chars().next() {
                if ch.is_ascii_hexdigit() || ch == '_' {
                    end += ch.len_utf8();
                } else {
                    break;
                }
            }
        } else if rest.starts_with("0b") || rest.starts_with("0B") {
            radix = 2;
            end = 2;
            while let Some(ch) = rest[end..].chars().next() {
                if matches!(ch, '0' | '1' | '_') {
                    end += ch.len_utf8();
                } else {
                    break;
                }
            }
        } else {
            while let Some(ch) = rest[end..].chars().next() {
                if ch.is_ascii_digit() || ch == '_' {
                    end += ch.len_utf8();
                } else {
                    break;
                }
            }
        }
        if end == 0 {
            return Err(ParseErr::new(
                "ADGL0100",
                "malformed number",
                "integer literal",
                Span::new(start, start + 1),
            ));
        }
        let digits = rest[..end].replace('_', "");
        let parsed = if radix == 10 {
            digits.parse::<i64>()
        } else {
            i64::from_str_radix(
                digits
                    .trim_start_matches("0x")
                    .trim_start_matches("0X")
                    .trim_start_matches("0b")
                    .trim_start_matches("0B"),
                radix,
            )
        };
        let value = parsed.map_err(|_| {
            ParseErr::new(
                "ADGL0100",
                "integer literal out of range",
                "i64 integer literal",
                Span::new(start, start + end),
            )
        })?;

        let unit_rest = &rest[end..];
        if unit_rest.starts_with("ms") {
            self.pos += end + 2;
            return Ok(Token {
                kind: TokenKind::Duration(value),
                span: Span::new(start, self.pos),
            });
        }
        if unit_rest.starts_with('s') && !starts_ident_continue(unit_rest, 1) {
            self.pos += end + 1;
            return Ok(Token {
                kind: TokenKind::Duration(value.saturating_mul(1_000)),
                span: Span::new(start, self.pos),
            });
        }
        if unit_rest.starts_with("min") {
            self.pos += end + 3;
            return Ok(Token {
                kind: TokenKind::Duration(value.saturating_mul(60_000)),
                span: Span::new(start, self.pos),
            });
        }
        if unit_rest.starts_with('m') || unit_rest.starts_with("mi") {
            return Err(ParseErr::new(
                "ADGL0110",
                "malformed duration unit",
                "duration unit: ms, s, min",
                Span::new(
                    start,
                    start + end + unit_rest.chars().take(3).map(char::len_utf8).sum::<usize>(),
                ),
            ));
        }

        self.pos += end;
        Ok(Token {
            kind: TokenKind::Int(value),
            span: Span::new(start, self.pos),
        })
    }

    fn lex_string(&mut self, start: usize) -> Result<Token<'a>, ParseErr> {
        let _ = self.bump_char(); // opening "
        let mut out = String::new();
        while let Some(ch) = self.bump_char() {
            if ch == '"' {
                return Ok(Token {
                    kind: TokenKind::String(out),
                    span: Span::new(start, self.pos),
                });
            }
            if ch == '\\' {
                let esc = match self.bump_char() {
                    Some('n') => '\n',
                    Some('t') => '\t',
                    Some('r') => '\r',
                    Some('0') => '\0',
                    Some('"') => '"',
                    Some('\\') => '\\',
                    Some('x') => {
                        let hi = self.bump_char().ok_or_else(|| {
                            ParseErr::new(
                                "ADGL0100",
                                "incomplete hex escape",
                                "two hex digits",
                                Span::new(start, self.pos),
                            )
                        })?;
                        let lo = self.bump_char().ok_or_else(|| {
                            ParseErr::new(
                                "ADGL0100",
                                "incomplete hex escape",
                                "two hex digits",
                                Span::new(start, self.pos),
                            )
                        })?;
                        let hex = [hi, lo].iter().collect::<String>();
                        let b = u8::from_str_radix(&hex, 16).map_err(|_| {
                            ParseErr::new(
                                "ADGL0100",
                                "invalid hex escape",
                                "\\xHH",
                                Span::new(start, self.pos),
                            )
                        })?;
                        b as char
                    }
                    Some(other) => {
                        return Err(ParseErr::new(
                            "ADGL0100",
                            format!("unknown escape '\\{}'", other),
                            "valid string escape",
                            Span::new(start, self.pos),
                        ));
                    }
                    None => {
                        return Err(ParseErr::new(
                            "ADGL0100",
                            "unterminated string",
                            "closing quote",
                            Span::new(start, self.pos),
                        ));
                    }
                };
                out.push(esc);
            } else {
                out.push(ch);
            }
        }
        Err(ParseErr::new(
            "ADGL0100",
            "unterminated string",
            "closing quote",
            Span::new(start, self.pos),
        ))
    }
}

fn starts_ident_continue(s: &str, byte_idx: usize) -> bool {
    s.get(byte_idx..)
        .and_then(|rest| rest.chars().next())
        .is_some_and(is_ident_continue)
}

fn is_ident_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_'
}

fn is_ident_continue(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

/// Maximal run via winnow `take_while` — keeps lexer helpers readable.
fn take_run<'a>(input: &mut &'a str, pred: impl Fn(char) -> bool) -> Result<&'a str, ()> {
    take_while::<_, _, InputError<&str>>(1.., pred)
        .parse_next(input)
        .map_err(|_| ())
}

fn keyword_static(s: &str) -> &'static str {
    match s {
        "ruleset" => "ruleset",
        "version" => "version",
        "requires" => "requires",
        "mutually_exclusive" => "mutually_exclusive",
        "evidence" => "evidence",
        "decision" => "decision",
        "scope" => "scope",
        "anchor" => "anchor",
        "correlate" => "correlate",
        "having" => "having",
        "infer" => "infer",
        "emit" => "emit",
        "action" => "action",
        "if" => "if",
        "else" => "else",
        "present" => "present",
        "absent" => "absent",
        "and" => "and",
        "or" => "or",
        "not" => "not",
        "Cause" => "Cause",
        "Problem" => "Problem",
        "event" => "event",
        "in" => "in",
        "true" => "true",
        "false" => "false",
        _ => "unknown",
    }
}

// ============================================================================
// Token stream parser (statement and declaration productions)
// ============================================================================

struct ParserState<'a> {
    tokens: Vec<Token<'a>>,
    idx: usize,
    /// Nesting depth of `{` / `}` only (ruleset / rule / stmt blocks).
    brace_depth: usize,
    /// When true, rule/decl errors are recorded and parsing continues after resync.
    recover: bool,
    diags: DiagBuffer,
}

impl<'a> ParserState<'a> {
    fn new(tokens: Vec<Token<'a>>) -> Self {
        Self {
            tokens,
            idx: 0,
            brace_depth: 0,
            recover: false,
            diags: DiagBuffer::new(),
        }
    }

    fn peek(&self) -> &Token<'a> {
        let idx = self.idx.min(self.tokens.len().saturating_sub(1));
        &self.tokens[idx]
    }

    fn peek_kind(&self) -> &TokenKind<'a> {
        &self.peek().kind
    }

    fn next(&mut self) -> &Token<'a> {
        if self.idx + 1 < self.tokens.len() {
            match self.peek_kind() {
                TokenKind::LBrace => self.brace_depth += 1,
                TokenKind::RBrace => self.brace_depth = self.brace_depth.saturating_sub(1),
                _ => {}
            }
            self.idx += 1;
        }
        self.peek()
    }

    fn record_error(&mut self, err: ParseErr) {
        self.diags.push(diag_of(err));
    }

    fn is_sync_keyword(kw: &str) -> bool {
        matches!(
            kw,
            "evidence" | "decision" | "mutually_exclusive" | "version" | "requires"
        )
    }

    /// Sync to the next ruleset member: leave cursor on a sync keyword or the
    /// ruleset-closing `}` (brace depth 1), consuming the failed rule's `}` when
    /// present. Keywords inside nested blocks are ignored via brace depth.
    fn resync_decl_or_rule(&mut self) {
        let start_idx = self.idx;
        // Ruleset body sits at depth 1 after `ruleset "…" {`.
        let target_depth = 1;
        loop {
            match self.peek_kind() {
                TokenKind::Eof => break,
                TokenKind::Kw(k)
                    if self.brace_depth == target_depth && Self::is_sync_keyword(k) =>
                {
                    break;
                }
                TokenKind::RBrace if self.brace_depth == target_depth => {
                    // Ruleset closer — leave for the caller.
                    break;
                }
                TokenKind::RBrace if self.brace_depth == target_depth + 1 => {
                    // Close the failed rule/decl block, then stop.
                    let _ = self.next();
                    break;
                }
                _ => {
                    let _ = self.next();
                }
            }
        }
        if self.idx == start_idx {
            match self.peek_kind() {
                TokenKind::Eof => {}
                TokenKind::Kw(k) if Self::is_sync_keyword(k) => {}
                TokenKind::RBrace if self.brace_depth <= target_depth => {}
                _ => {
                    let _ = self.next();
                }
            }
        }
    }

    /// Record + resync on recovery; otherwise propagate.
    /// `Ok(None)` means the caller should `continue` the surrounding loop.
    fn recover_item<T>(&mut self, result: Result<T, ParseErr>) -> Result<Option<T>, ParseErr> {
        match result {
            Ok(v) => Ok(Some(v)),
            Err(e) => {
                if self.recover {
                    self.record_error(e);
                    self.resync_decl_or_rule();
                    Ok(None)
                } else {
                    Err(e)
                }
            }
        }
    }

    fn consume_kw(&mut self, kw: &'static str) -> Result<Span, ParseErr> {
        match self.peek_kind() {
            TokenKind::Kw(got) if *got == kw => {
                let sp = self.peek().span;
                let _ = self.next();
                Ok(sp)
            }
            _ => Err(ParseErr::new(
                "ADGL0100",
                format!("expected keyword '{kw}'"),
                kw,
                self.peek().span,
            )),
        }
    }

    fn consume_word_exact(&mut self, word: &'static str) -> Result<Span, ParseErr> {
        match self.peek_kind() {
            TokenKind::Ident(got) if *got == word => {
                let sp = self.peek().span;
                let _ = self.next();
                Ok(sp)
            }
            TokenKind::Kw(got) if *got == word => {
                let sp = self.peek().span;
                let _ = self.next();
                Ok(sp)
            }
            _ => Err(ParseErr::new(
                "ADGL0100",
                format!("expected '{word}'"),
                word,
                self.peek().span,
            )),
        }
    }

    fn consume_kind<F>(&mut self, expected: &'static str, pred: F) -> Result<Token<'a>, ParseErr>
    where
        F: Fn(&TokenKind<'a>) -> bool,
    {
        if pred(self.peek_kind()) {
            let tok = self.peek().clone();
            let _ = self.next();
            Ok(tok)
        } else {
            Err(ParseErr::new(
                "ADGL0100",
                "unexpected token",
                expected,
                self.peek().span,
            ))
        }
    }

    fn parse_ruleset(&mut self) -> Result<Ruleset<'a>, ParseErr> {
        // EBNF: Ruleset ::= "ruleset" StringLit "{" RulesetHeader { Rule } "}" ;
        let start = self.consume_kw("ruleset")?.start;
        let name = self.parse_string_lit()?;
        let _ = self.consume_punct(TokenKind::LBrace, "{")?;
        let header = self.parse_ruleset_header()?;
        let mut rules = Vec::new();
        loop {
            match self.peek_kind() {
                TokenKind::Kw("evidence") => {
                    let parsed = self.parse_evidence_rule();
                    let Some(rule) = self.recover_item(parsed)? else {
                        continue;
                    };
                    rules.push(RuleDecl::Evidence(rule));
                }
                TokenKind::Kw("decision") => {
                    let parsed = self.parse_decision_rule();
                    let Some(rule) = self.recover_item(parsed)? else {
                        continue;
                    };
                    rules.push(RuleDecl::Decision(rule));
                }
                _ => break,
            }
        }
        let end = self.consume_punct(TokenKind::RBrace, "}")?.end;
        if !matches!(self.peek_kind(), TokenKind::Eof) {
            return Err(ParseErr::new(
                "ADGL0100",
                "unexpected tokens after ruleset",
                "end of input",
                self.peek().span,
            ));
        }
        Ok(Ruleset {
            name,
            header,
            rules,
            span: Span::new(start, end),
        })
    }

    fn parse_ruleset_header(&mut self) -> Result<RulesetHeader<'a>, ParseErr> {
        // EBNF: RulesetHeader ::= Version { Decl } ;
        let start = self.peek().span.start;
        let _ = self.consume_kw("version")?;
        let _ = self.consume_punct(TokenKind::Eq, "=")?;
        let version = self.parse_string_lit()?;
        let mut decls = Vec::new();
        loop {
            match self.peek_kind() {
                TokenKind::Kw("requires") => {
                    let parsed = self.parse_requires_decl();
                    let Some(decl) = self.recover_item(parsed)? else {
                        continue;
                    };
                    decls.push(Decl::Requires(decl));
                }
                TokenKind::Kw("mutually_exclusive") => {
                    let parsed = self.parse_mutually_exclusive_decl();
                    let Some(decl) = self.recover_item(parsed)? else {
                        continue;
                    };
                    decls.push(Decl::MutuallyExclusive(decl));
                }
                _ => break,
            }
        }
        let end = decls
            .last()
            .map(|d| match d {
                Decl::Requires(r) => r.span.end,
                Decl::MutuallyExclusive(m) => m.span.end,
            })
            .unwrap_or(version.span.end);
        Ok(RulesetHeader {
            version,
            decls,
            span: Span::new(start, end),
        })
    }

    fn parse_requires_decl(&mut self) -> Result<RequiresDecl, ParseErr> {
        // EBNF: Decl ::= "requires" "=" "[" StringLit { "," StringLit } "]"
        let start = self.consume_kw("requires")?.start;
        let _ = self.consume_punct(TokenKind::Eq, "=")?;
        let _ = self.consume_punct(TokenKind::LBracket, "[")?;
        let mut caps = Vec::new();
        if !matches!(self.peek_kind(), TokenKind::RBracket) {
            caps.push(self.parse_string_lit()?);
            while matches!(self.peek_kind(), TokenKind::Comma) {
                let _ = self.next();
                caps.push(self.parse_string_lit()?);
            }
        }
        if caps.len() > 32 {
            return Err(ParseErr::new(
                "ADGL0105",
                "too many requires entries",
                "at most 32 entries",
                self.peek().span,
            ));
        }
        let end = self.consume_punct(TokenKind::RBracket, "]")?.end;
        Ok(RequiresDecl {
            capabilities: caps,
            span: Span::new(start, end),
        })
    }

    fn parse_mutually_exclusive_decl(&mut self) -> Result<MutuallyExclusiveDecl<'a>, ParseErr> {
        // EBNF: Decl ::= "mutually_exclusive" "(" IdentList ")"
        let start = self.consume_kw("mutually_exclusive")?.start;
        let _ = self.consume_punct(TokenKind::LParen, "(")?;
        let mut idents = Vec::new();
        idents.push(self.parse_ident_any()?);
        while matches!(self.peek_kind(), TokenKind::Comma) {
            let _ = self.next();
            idents.push(self.parse_ident_any()?);
        }
        let end = self.consume_punct(TokenKind::RParen, ")")?.end;
        Ok(MutuallyExclusiveDecl {
            idents,
            span: Span::new(start, end),
        })
    }

    fn parse_evidence_rule(&mut self) -> Result<EvidenceRule<'a>, ParseErr> {
        // EBNF: EvidenceRule ::= ...
        let start = self.consume_kw("evidence")?.start;
        let name = self.parse_ident_non_kw()?;
        let _ = self.consume_punct(TokenKind::LBrace, "{")?;
        let _ = self.consume_kw("scope")?;
        let _ = self.consume_punct(TokenKind::Colon, ":")?;
        let scope = self.parse_scope_type()?;
        let anchor = self.parse_anchor_block()?;
        let mut correlates = Vec::new();
        while matches!(self.peek_kind(), TokenKind::Kw("correlate")) {
            correlates.push(self.parse_correlate_block()?);
        }
        let if_else = if matches!(self.peek_kind(), TokenKind::Kw("if")) {
            Some(self.parse_if_else_block()?)
        } else {
            None
        };
        let mut body = Vec::new();
        while !matches!(self.peek_kind(), TokenKind::RBrace | TokenKind::Eof) {
            body.push(self.parse_stmt_in_rule(true)?);
        }
        let end = self.consume_punct(TokenKind::RBrace, "}")?.end;
        Ok(EvidenceRule {
            name,
            scope,
            anchor,
            correlates,
            if_else,
            body,
            span: Span::new(start, end),
        })
    }

    fn parse_decision_rule(&mut self) -> Result<DecisionRule<'a>, ParseErr> {
        // EBNF: DecisionRule ::= ...
        let start = self.consume_kw("decision")?.start;
        let name = self.parse_ident_non_kw()?;
        let _ = self.consume_punct(TokenKind::LBrace, "{")?;
        let _ = self.consume_kw("scope")?;
        let _ = self.consume_punct(TokenKind::Colon, ":")?;
        let scope = self.parse_scope_type()?;
        let anchor = self.parse_decision_anchor()?;
        let mut correlates = Vec::new();
        while matches!(self.peek_kind(), TokenKind::Kw("correlate")) {
            correlates.push(self.parse_correlate_block()?);
        }
        let if_else = if matches!(self.peek_kind(), TokenKind::Kw("if")) {
            Some(self.parse_if_else_block()?)
        } else {
            None
        };
        let mut body = Vec::new();
        while !matches!(self.peek_kind(), TokenKind::RBrace | TokenKind::Eof) {
            body.push(self.parse_stmt_in_rule(false)?);
        }
        let end = self.consume_punct(TokenKind::RBrace, "}")?.end;
        Ok(DecisionRule {
            name,
            scope,
            anchor,
            correlates,
            if_else,
            body,
            span: Span::new(start, end),
        })
    }

    fn parse_anchor_block(&mut self) -> Result<AnchorBlock<'a>, ParseErr> {
        // EBNF: AnchorBlock ::= "anchor" Ident ":" "event" "(" EventType ")" [ "{" [ Predicate ] "}" ] ;
        let start = self.consume_kw("anchor")?.start;
        let binding = self.parse_ident_non_kw()?;
        let _ = self.consume_punct(TokenKind::Colon, ":")?;
        let _ = self.consume_kw("event")?;
        let _ = self.consume_punct(TokenKind::LParen, "(")?;
        let event_type = self.parse_kind_ident()?;
        let _ = self.consume_punct(TokenKind::RParen, ")")?;
        let predicate = if matches!(self.peek_kind(), TokenKind::LBrace) {
            let _ = self.next();
            let pred = if matches!(self.peek_kind(), TokenKind::RBrace) {
                None
            } else {
                Some(self.parse_expr()?)
            };
            let _ = self.consume_punct(TokenKind::RBrace, "}")?;
            pred
        } else {
            None
        };
        let end = predicate
            .as_ref()
            .map(|e| e.span.end)
            .unwrap_or(event_type.span.end);
        Ok(AnchorBlock {
            binding,
            event_type,
            predicate,
            span: Span::new(start, end),
        })
    }

    fn parse_decision_anchor(&mut self) -> Result<DecisionAnchor<'a>, ParseErr> {
        // EBNF: DecisionAnchor ::= "anchor" Ident ":" ( CauseAnchor | ProblemAnchor ) ;
        let _anchor_kw = self.consume_kw("anchor")?;
        let binding = self.parse_ident_non_kw()?;
        let _ = self.consume_punct(TokenKind::Colon, ":")?;
        match self.peek_kind() {
            TokenKind::Kw("Cause") => {
                let start = self.consume_kw("Cause")?.start;
                let _ = self.consume_punct(TokenKind::LParen, "(")?;
                let cause = self.parse_ident_non_kw()?;
                let _ = self.consume_punct(TokenKind::RParen, ")")?;
                let _ = self.consume_punct(TokenKind::LBrace, "{")?;
                let predicate = self.parse_expr()?;
                let end = self.consume_punct(TokenKind::RBrace, "}")?.end;
                Ok(DecisionAnchor::Cause(CauseAnchor {
                    binding,
                    cause,
                    predicate,
                    span: Span::new(start, end),
                }))
            }
            TokenKind::Kw("Problem") => {
                let start = self.consume_kw("Problem")?.start;
                let _ = self.consume_punct(TokenKind::LParen, "(")?;
                let problem = self.parse_ident_non_kw()?;
                let _ = self.consume_punct(TokenKind::RParen, ")")?;
                let predicate = if matches!(self.peek_kind(), TokenKind::LBrace) {
                    let _ = self.next();
                    let p = if matches!(self.peek_kind(), TokenKind::RBrace) {
                        None
                    } else {
                        Some(self.parse_expr()?)
                    };
                    let _ = self.consume_punct(TokenKind::RBrace, "}")?;
                    p
                } else {
                    None
                };
                let end = predicate
                    .as_ref()
                    .map(|p| p.span.end)
                    .unwrap_or(problem.span.end);
                Ok(DecisionAnchor::Problem(ProblemAnchor {
                    binding,
                    problem,
                    predicate,
                    span: Span::new(start, end),
                }))
            }
            _ => Err(ParseErr::new(
                "ADGL0100",
                "expected Cause(...) or Problem(...)",
                "Cause/Problem anchor",
                self.peek().span,
            )),
        }
    }

    fn parse_correlate_block(&mut self) -> Result<CorrelateBlock<'a>, ParseErr> {
        // EBNF: CorrelateBlock ::= ...
        let start = self.consume_kw("correlate")?.start;
        let binding = self.parse_ident_non_kw()?;
        let _ = self.consume_punct(TokenKind::Colon, ":")?;
        let source = self.parse_correlate_source()?;
        let _ = self.consume_punct(TokenKind::LBrace, "{")?;
        let _ = self.consume_word_exact("topo")?;
        let _ = self.consume_punct(TokenKind::Colon, ":")?;
        let topo = self.parse_topo_predicate()?;
        let _ = self.consume_word_exact("time")?;
        let _ = self.consume_punct(TokenKind::Colon, ":")?;
        let time = self.parse_time_window()?;
        let min_match = self.parse_optional_min_match()?;
        let end = self.consume_punct(TokenKind::RBrace, "}")?.end;
        Ok(CorrelateBlock {
            binding,
            source,
            topo,
            time,
            min_match,
            span: Span::new(start, end),
        })
    }

    fn parse_optional_min_match(&mut self) -> Result<Option<crate::ast::MinMatchClause>, ParseErr> {
        if !matches!(self.peek_kind(), TokenKind::Kw("having")) {
            return Ok(None);
        }
        let _ = self.consume_kw("having")?;
        let _ = self.consume_punct(TokenKind::Colon, ":")?;
        let _ = self.consume_word_exact("count")?;
        let _ = self.consume_punct(TokenKind::Ge, ">=")?;
        let tok = self.consume_kind("integer", |k| matches!(k, TokenKind::Int(_)))?;
        let lit = match tok.kind {
            TokenKind::Int(v) => v,
            _ => 0,
        };
        if lit < 0 {
            return Err(ParseErr::new(
                "ADGL0100",
                "min match count must be a non-negative integer literal",
                "non-negative integer literal",
                tok.span,
            ));
        }
        Ok(Some(crate::ast::MinMatchClause {
            count: lit,
            span: tok.span,
        }))
    }

    fn parse_correlate_source(&mut self) -> Result<CorrelateSource<'a>, ParseErr> {
        // EBNF: CorrelateSource ::= "event"(...) | "Problem"(...) | "Cause"(...)
        match self.peek_kind() {
            TokenKind::Kw("event") => {
                let _ = self.consume_kw("event")?;
                let _ = self.consume_punct(TokenKind::LParen, "(")?;
                let k = self.parse_kind_ident()?;
                let _ = self.consume_punct(TokenKind::RParen, ")")?;
                Ok(CorrelateSource::Event(k))
            }
            TokenKind::Kw("Problem") => {
                let _ = self.consume_kw("Problem")?;
                let _ = self.consume_punct(TokenKind::LParen, "(")?;
                let k = self.parse_ident_non_kw()?;
                let _ = self.consume_punct(TokenKind::RParen, ")")?;
                Ok(CorrelateSource::Problem(k))
            }
            TokenKind::Kw("Cause") => {
                let _ = self.consume_kw("Cause")?;
                let _ = self.consume_punct(TokenKind::LParen, "(")?;
                let k = self.parse_ident_non_kw()?;
                let _ = self.consume_punct(TokenKind::RParen, ")")?;
                Ok(CorrelateSource::Cause(k))
            }
            _ => Err(ParseErr::new(
                "ADGL0100",
                "invalid correlate source",
                "event(...) | Problem(...) | Cause(...)",
                self.peek().span,
            )),
        }
    }

    fn parse_topo_predicate(&mut self) -> Result<TopoPredicate<'a>, ParseErr> {
        // EBNF: TopoPredicate ::= Ident "(" ExprList ")" ;
        let name = self.parse_ident_non_kw()?;
        let start = name.span.start;
        let args = self.parse_paren_expr_list()?;
        let end = args.last().map(|e| e.span.end).unwrap_or(name.span.end);
        Ok(TopoPredicate {
            name,
            args,
            span: Span::new(start, end),
        })
    }

    fn parse_time_window(&mut self) -> Result<TimeWindow<'a>, ParseErr> {
        // EBNF: TimeWindow ::= Expr "in" "[" Expr "," Expr "]" ;
        // Keep `in` as the TimeWindow separator here (not Expr binary `in`).
        let probe = self.parse_additive()?;
        let start = probe.span.start;
        let _ = self.consume_kw("in")?;
        let _ = self.consume_punct(TokenKind::LBracket, "[")?;
        let lo = self.parse_expr()?;
        let _ = self.consume_punct(TokenKind::Comma, ",")?;
        let hi = self.parse_expr()?;
        let end = self.consume_punct(TokenKind::RBracket, "]")?.end;
        Ok(TimeWindow {
            probe,
            start: lo,
            end: hi,
            span: Span::new(start, end),
        })
    }

    fn parse_if_else_block(&mut self) -> Result<IfElseBlock<'a>, ParseErr> {
        // EBNF: IfElseBlock ::= "if" Expr "{" { InferStmt | EmitStmt | ActionStmt } "}" [ ... ]
        let start = self.consume_kw("if")?.start;
        let condition = self.parse_expr()?;
        let _ = self.consume_punct(TokenKind::LBrace, "{")?;
        let mut then_body = Vec::new();
        while !matches!(self.peek_kind(), TokenKind::RBrace | TokenKind::Eof) {
            then_body.push(self.parse_stmt_any()?);
        }
        let _ = self.consume_punct(TokenKind::RBrace, "}")?;
        let else_body = if matches!(self.peek_kind(), TokenKind::Kw("else")) {
            let _ = self.consume_kw("else")?;
            let _ = self.consume_punct(TokenKind::LBrace, "{")?;
            let mut body = Vec::new();
            while !matches!(self.peek_kind(), TokenKind::RBrace | TokenKind::Eof) {
                body.push(self.parse_stmt_any()?);
            }
            let _ = self.consume_punct(TokenKind::RBrace, "}")?;
            Some(body)
        } else {
            None
        };
        let end = else_body
            .as_ref()
            .and_then(|body| body.last())
            .map(stmt_end)
            .unwrap_or(condition.span.end);
        Ok(IfElseBlock {
            condition,
            then_body,
            else_body,
            span: Span::new(start, end),
        })
    }

    fn parse_stmt_any(&mut self) -> Result<Stmt<'a>, ParseErr> {
        match self.peek_kind() {
            TokenKind::Kw("infer") => Ok(Stmt::Infer(self.parse_infer_stmt()?)),
            TokenKind::Kw("emit") => Ok(Stmt::Emit(self.parse_emit_stmt()?)),
            TokenKind::Kw("action") => Ok(Stmt::Action(self.parse_action_stmt()?)),
            _ => Err(ParseErr::new(
                "ADGL0100",
                "unexpected statement",
                "infer | emit | action",
                self.peek().span,
            )),
        }
    }

    fn parse_stmt_in_rule(&mut self, evidence: bool) -> Result<Stmt<'a>, ParseErr> {
        let stmt = self.parse_stmt_any()?;
        if evidence {
            if matches!(stmt, Stmt::Emit(_)) {
                return Err(ParseErr::new(
                    "ADGL0450",
                    "emit is not allowed in evidence rule body",
                    "infer | action",
                    stmt_span(&stmt),
                ));
            }
        } else if matches!(stmt, Stmt::Infer(_)) {
            return Err(ParseErr::new(
                "ADGL0450",
                "infer is not allowed in decision rule body",
                "emit | action",
                stmt_span(&stmt),
            ));
        }
        Ok(stmt)
    }

    fn parse_infer_stmt(&mut self) -> Result<InferStmt<'a>, ParseErr> {
        // EBNF: InferStmt ::= "infer" "Cause" "(" Ident ")" "{" InferField { "," InferField } "}" ;
        let start = self.consume_kw("infer")?.start;
        let _ = self.consume_kw("Cause")?;
        let _ = self.consume_punct(TokenKind::LParen, "(")?;
        let cause = self.parse_ident_non_kw()?;
        let _ = self.consume_punct(TokenKind::RParen, ")")?;
        let _ = self.consume_punct(TokenKind::LBrace, "{")?;
        let mut fields = Vec::new();
        if !matches!(self.peek_kind(), TokenKind::RBrace) {
            fields.push(self.parse_infer_field()?);
            while matches!(self.peek_kind(), TokenKind::Comma) {
                let _ = self.next();
                fields.push(self.parse_infer_field()?);
            }
        }
        let end = self.consume_punct(TokenKind::RBrace, "}")?.end;
        Ok(InferStmt {
            cause,
            fields,
            span: Span::new(start, end),
        })
    }

    fn parse_infer_field(&mut self) -> Result<InferField<'a>, ParseErr> {
        let (key_name, key_span) = self.parse_word_any()?;
        let _ = self.consume_punct(TokenKind::Colon, ":")?;
        match key_name {
            "target" => {
                let expr = self.parse_expr()?;
                let end = expr.span.end;
                Ok(InferField::Target(expr, Span::new(key_span.start, end)))
            }
            "weight" => {
                let sign = if matches!(self.peek_kind(), TokenKind::Plus) {
                    let _ = self.next();
                    1i64
                } else if matches!(self.peek_kind(), TokenKind::Minus) {
                    let _ = self.next();
                    -1i64
                } else {
                    return Err(ParseErr::new(
                        "ADGL0100",
                        "weight requires explicit sign",
                        "'+' or '-' before integer literal",
                        self.peek().span,
                    ));
                };
                let tok = self.consume_kind("integer", |k| matches!(k, TokenKind::Int(_)))?;
                let val = match tok.kind {
                    TokenKind::Int(v) => sign.saturating_mul(v),
                    _ => 0,
                };
                Ok(InferField::Weight {
                    value: val,
                    span: Span::new(key_span.start, tok.span.end),
                })
            }
            "evidence" => {
                let refs = self.parse_ref_list()?;
                let end = refs.last().map(|r| r.span.end).unwrap_or(key_span.end);
                Ok(InferField::Evidence(refs, Span::new(key_span.start, end)))
            }
            _ => Err(ParseErr::new(
                "ADGL0100",
                "unknown infer field",
                "target | weight | evidence",
                key_span,
            )),
        }
    }

    fn parse_emit_stmt(&mut self) -> Result<EmitStmt<'a>, ParseErr> {
        // EBNF: EmitStmt ::= "emit" "Problem" "(" Ident ")" "{" EmitField { "," EmitField } "}" ;
        let start = self.consume_kw("emit")?.start;
        let _ = self.consume_kw("Problem")?;
        let _ = self.consume_punct(TokenKind::LParen, "(")?;
        let problem = self.parse_ident_non_kw()?;
        let _ = self.consume_punct(TokenKind::RParen, ")")?;
        let _ = self.consume_punct(TokenKind::LBrace, "{")?;
        let mut fields = Vec::new();
        if !matches!(self.peek_kind(), TokenKind::RBrace) {
            fields.push(self.parse_emit_field()?);
            while matches!(self.peek_kind(), TokenKind::Comma) {
                let _ = self.next();
                fields.push(self.parse_emit_field()?);
            }
        }
        let end = self.consume_punct(TokenKind::RBrace, "}")?.end;
        Ok(EmitStmt {
            problem,
            fields,
            span: Span::new(start, end),
        })
    }

    fn parse_emit_field(&mut self) -> Result<EmitField<'a>, ParseErr> {
        let (key_name, key_span) = self.parse_word_any()?;
        let _ = self.consume_punct(TokenKind::Colon, ":")?;
        match key_name {
            "target" => {
                let expr = self.parse_expr()?;
                let end = expr.span.end;
                Ok(EmitField::Target(expr, Span::new(key_span.start, end)))
            }
            "severity" => {
                let sev = self.parse_severity()?;
                Ok(EmitField::Severity(
                    sev,
                    Span::new(key_span.start, self.prev_end()),
                ))
            }
            "evidence" => {
                let refs = self.parse_ref_list()?;
                let end = refs.last().map(|r| r.span.end).unwrap_or(key_span.end);
                Ok(EmitField::Evidence(refs, Span::new(key_span.start, end)))
            }
            "sarif_id" => {
                let s = self.parse_string_lit()?;
                let end = s.span.end;
                Ok(EmitField::SarifId(s, Span::new(key_span.start, end)))
            }
            _ => Err(ParseErr::new(
                "ADGL0100",
                "unknown emit field",
                "target | severity | evidence | sarif_id",
                key_span,
            )),
        }
    }

    fn parse_action_stmt(&mut self) -> Result<ActionStmt<'a>, ParseErr> {
        // EBNF: ActionStmt ::= "action" Ident [ "(" KindIdent ")" ] "{" ... "}" ;
        let start = self.consume_kw("action")?.start;
        let action_ident = self.parse_ident_any()?;
        let action = match action_ident.name {
            "request_observation" => ActionName::Known(ActionKind::RequestObservation),
            "run_check" => ActionName::Known(ActionKind::RunCheck),
            "suppress_symptom" => ActionName::Known(ActionKind::SuppressSymptom),
            "mark_ambiguous" => ActionName::Known(ActionKind::MarkAmbiguous),
            "request_topology" => ActionName::Known(ActionKind::RequestTopology),
            _ => ActionName::Custom(action_ident),
        };
        let arg = if matches!(self.peek_kind(), TokenKind::LParen) {
            Some(self.parse_kind_ident_in_parens()?)
        } else {
            None
        };
        let _ = self.consume_punct(TokenKind::LBrace, "{")?;
        let mut fields = Vec::new();
        if !matches!(self.peek_kind(), TokenKind::RBrace) {
            fields.push(self.parse_action_field()?);
            while matches!(self.peek_kind(), TokenKind::Comma) {
                let _ = self.next();
                fields.push(self.parse_action_field()?);
            }
        }
        let end = self.consume_punct(TokenKind::RBrace, "}")?.end;
        Ok(ActionStmt {
            action,
            arg,
            fields,
            span: Span::new(start, end),
        })
    }

    fn parse_action_field(&mut self) -> Result<ActionField<'a>, ParseErr> {
        let (key_name, key_span) = self.parse_word_any()?;
        let _ = self.consume_punct(TokenKind::Colon, ":")?;
        match key_name {
            "target" => {
                let expr = self.parse_expr()?;
                let end = expr.span.end;
                Ok(ActionField::Target(expr, Span::new(key_span.start, end)))
            }
            "reason" => {
                let s = self.parse_string_lit()?;
                let end = s.span.end;
                Ok(ActionField::Reason(s, Span::new(key_span.start, end)))
            }
            "evidence" => {
                let refs = self.parse_ref_list()?;
                let end = refs.last().map(|r| r.span.end).unwrap_or(key_span.end);
                Ok(ActionField::Evidence(refs, Span::new(key_span.start, end)))
            }
            _ => Err(ParseErr::new(
                "ADGL0100",
                "unknown action field",
                "target | reason | evidence",
                key_span,
            )),
        }
    }

    fn parse_ref_list(&mut self) -> Result<Vec<Ident<'a>>, ParseErr> {
        let _ = self.consume_punct(TokenKind::LBracket, "[")?;
        let mut refs = Vec::new();
        if !matches!(self.peek_kind(), TokenKind::RBracket) {
            refs.push(self.parse_ident_any()?);
            while matches!(self.peek_kind(), TokenKind::Comma) {
                let _ = self.next();
                refs.push(self.parse_ident_any()?);
            }
        }
        let _ = self.consume_punct(TokenKind::RBracket, "]")?;
        Ok(refs)
    }

    fn parse_scope_type(&mut self) -> Result<ScopeType, ParseErr> {
        let ident = self.parse_ident_any()?;
        match ident.name {
            "Session" => Ok(ScopeType::Session),
            "Port" => Ok(ScopeType::Port),
            "ClientMac" => Ok(ScopeType::ClientMac),
            "Vlan" => Ok(ScopeType::Vlan),
            "AccessPoint" => Ok(ScopeType::AccessPoint),
            "Global" => Ok(ScopeType::Global),
            _ => Err(ParseErr::new(
                "ADGL0100",
                "invalid scope type",
                "Session | Port | ClientMac | Vlan | AccessPoint | Global",
                ident.span,
            )),
        }
    }

    fn parse_severity(&mut self) -> Result<Severity, ParseErr> {
        let ident = self.parse_ident_any()?;
        match ident.name {
            "Critical" => Ok(Severity::Critical),
            "High" => Ok(Severity::High),
            "Medium" => Ok(Severity::Medium),
            "Low" => Ok(Severity::Low),
            "Recommended" => Ok(Severity::Recommended),
            "Optional" => Ok(Severity::Optional),
            _ => Err(ParseErr::new(
                "ADGL0100",
                "invalid severity",
                "Critical | High | Medium | Low | Recommended | Optional",
                ident.span,
            )),
        }
    }

    fn parse_kind_ident_in_parens(&mut self) -> Result<KindIdent<'a>, ParseErr> {
        let _ = self.consume_punct(TokenKind::LParen, "(")?;
        let k = self.parse_kind_ident()?;
        let _ = self.consume_punct(TokenKind::RParen, ")")?;
        Ok(k)
    }

    fn parse_kind_ident(&mut self) -> Result<KindIdent<'a>, ParseErr> {
        // EBNF: KindIdent ::= Ident { "." Ident } ;
        // Catalog event names may use reserved words as path segments (e.g. wifi.mgmt.action).
        let (name, start_span) = self.parse_word_any()?;
        let start = start_span.start;
        let mut segments = vec![Ident {
            name,
            span: start_span,
        }];
        while matches!(self.peek_kind(), TokenKind::Dot) {
            let _ = self.next();
            let (name, span) = self.parse_word_any()?;
            segments.push(Ident { name, span });
        }
        let end = segments.last().map(|s| s.span.end).unwrap_or(start);
        Ok(KindIdent {
            segments,
            span: Span::new(start, end),
        })
    }

    fn parse_string_lit(&mut self) -> Result<StringLit, ParseErr> {
        let tok = self.consume_kind("string literal", |k| matches!(k, TokenKind::String(_)))?;
        match tok.kind {
            TokenKind::String(value) => Ok(StringLit {
                value,
                span: tok.span,
            }),
            _ => Err(ParseErr::new(
                "ADGL0100",
                "expected string literal",
                "string literal",
                tok.span,
            )),
        }
    }

    fn parse_ident_any(&mut self) -> Result<Ident<'a>, ParseErr> {
        let tok = self.consume_kind("identifier", |k| {
            matches!(k, TokenKind::Ident(_) | TokenKind::Kw(_))
        })?;
        let name = match tok.kind {
            TokenKind::Ident(s) => s,
            TokenKind::Kw(s) if matches!(s, "event" | "Cause" | "Problem") => s,
            TokenKind::Kw(_) => {
                return Err(ParseErr::new(
                    "ADGL0100",
                    "reserved keyword is not allowed here",
                    "identifier (non-keyword)",
                    tok.span,
                ));
            }
            _ => unreachable!("consume_kind filtered non-ident token"),
        };
        Ok(Ident {
            name,
            span: tok.span,
        })
    }

    fn parse_word_any(&mut self) -> Result<(&'a str, Span), ParseErr> {
        let tok = self.consume_kind("identifier or keyword", |k| {
            matches!(k, TokenKind::Ident(_) | TokenKind::Kw(_))
        })?;
        match tok.kind {
            TokenKind::Ident(name) => Ok((name, tok.span)),
            TokenKind::Kw(name) => Ok((name, tok.span)),
            _ => unreachable!("consume_kind filtered non-word token"),
        }
    }

    fn parse_ident_non_kw(&mut self) -> Result<Ident<'a>, ParseErr> {
        let tok = self.consume_kind("identifier", |k| matches!(k, TokenKind::Ident(_)))?;
        if let TokenKind::Ident(name) = tok.kind {
            Ok(Ident {
                name,
                span: tok.span,
            })
        } else {
            Err(ParseErr::new(
                "ADGL0100",
                "expected identifier",
                "identifier",
                tok.span,
            ))
        }
    }

    fn consume_punct(
        &mut self,
        kind: TokenKind<'a>,
        expected: &'static str,
    ) -> Result<Span, ParseErr> {
        if std::mem::discriminant(self.peek_kind()) == std::mem::discriminant(&kind) {
            let sp = self.peek().span;
            let _ = self.next();
            Ok(sp)
        } else {
            Err(ParseErr::new(
                "ADGL0100",
                "unexpected token",
                expected,
                self.peek().span,
            ))
        }
    }

    fn prev_end(&self) -> usize {
        if self.idx == 0 {
            0
        } else {
            self.tokens[self.idx - 1].span.end
        }
    }

    // ============================================================================
    // Expression ladder (EBNF precedence chain)
    // ============================================================================

    // Expr ::= LogicOr
    fn parse_expr(&mut self) -> Result<Expr<'a>, ParseErr> {
        self.parse_logic_or()
    }

    // LogicOr ::= LogicAnd { ( "||" | "or" ) LogicAnd } ;
    fn parse_logic_or(&mut self) -> Result<Expr<'a>, ParseErr> {
        let mut left = self.parse_logic_and()?;
        loop {
            let is_or = matches!(self.peek_kind(), TokenKind::OrOr)
                || matches!(self.peek_kind(), TokenKind::Kw("or"));
            if !is_or {
                break;
            }
            let _ = self.next();
            let right = self.parse_logic_and()?;
            let span = Span::new(left.span.start, right.span.end);
            left = Expr {
                kind: ExprKind::Binary {
                    op: BinaryOp::Or,
                    left: Box::new(left),
                    right: Box::new(right),
                },
                span,
            };
        }
        Ok(left)
    }

    // LogicAnd ::= Equality { ( "&&" | "and" ) Equality } ;
    fn parse_logic_and(&mut self) -> Result<Expr<'a>, ParseErr> {
        let mut left = self.parse_equality()?;
        loop {
            let is_and = matches!(self.peek_kind(), TokenKind::AndAnd)
                || matches!(self.peek_kind(), TokenKind::Kw("and"));
            if !is_and {
                break;
            }
            let _ = self.next();
            let right = self.parse_equality()?;
            let span = Span::new(left.span.start, right.span.end);
            left = Expr {
                kind: ExprKind::Binary {
                    op: BinaryOp::And,
                    left: Box::new(left),
                    right: Box::new(right),
                },
                span,
            };
        }
        Ok(left)
    }

    // Equality ::= Additive { ( "==" | "!=" | "<" | "<=" | ">" | ">=" | "in" ) Additive } ;
    fn parse_equality(&mut self) -> Result<Expr<'a>, ParseErr> {
        let mut left = self.parse_additive()?;
        loop {
            let op = match self.peek_kind() {
                TokenKind::EqEq => Some(BinaryOp::Eq),
                TokenKind::Ne => Some(BinaryOp::Ne),
                TokenKind::Lt => Some(BinaryOp::Lt),
                TokenKind::Le => Some(BinaryOp::Le),
                TokenKind::Gt => Some(BinaryOp::Gt),
                TokenKind::Ge => Some(BinaryOp::Ge),
                TokenKind::Kw("in") => Some(BinaryOp::In),
                _ => None,
            };
            if let Some(op) = op {
                let _ = self.next();
                let right = self.parse_additive()?;
                let span = Span::new(left.span.start, right.span.end);
                left = Expr {
                    kind: ExprKind::Binary {
                        op,
                        left: Box::new(left),
                        right: Box::new(right),
                    },
                    span,
                };
            } else {
                break;
            }
        }
        Ok(left)
    }

    // Additive ::= Multiplicative { ( "+" | "-" ) Multiplicative } ;
    fn parse_additive(&mut self) -> Result<Expr<'a>, ParseErr> {
        let mut left = self.parse_multiplicative()?;
        loop {
            let op = match self.peek_kind() {
                TokenKind::Plus => Some(BinaryOp::Add),
                TokenKind::Minus => Some(BinaryOp::Sub),
                _ => None,
            };
            if let Some(op) = op {
                let _ = self.next();
                let right = self.parse_multiplicative()?;
                let span = Span::new(left.span.start, right.span.end);
                left = Expr {
                    kind: ExprKind::Binary {
                        op,
                        left: Box::new(left),
                        right: Box::new(right),
                    },
                    span,
                };
            } else {
                break;
            }
        }
        Ok(left)
    }

    // Multiplicative ::= Unary { ( "*" | "/" | "%" ) Unary } ;
    fn parse_multiplicative(&mut self) -> Result<Expr<'a>, ParseErr> {
        let mut left = self.parse_unary()?;
        loop {
            let op = match self.peek_kind() {
                TokenKind::Star => Some(BinaryOp::Mul),
                TokenKind::Slash => Some(BinaryOp::Div),
                TokenKind::Percent => Some(BinaryOp::Rem),
                _ => None,
            };
            if let Some(op) = op {
                let _ = self.next();
                let right = self.parse_unary()?;
                let span = Span::new(left.span.start, right.span.end);
                left = Expr {
                    kind: ExprKind::Binary {
                        op,
                        left: Box::new(left),
                        right: Box::new(right),
                    },
                    span,
                };
            } else {
                break;
            }
        }
        Ok(left)
    }

    // Unary ::= ( "!" | "not" ) Unary | Postfix ;
    fn parse_unary(&mut self) -> Result<Expr<'a>, ParseErr> {
        if matches!(self.peek_kind(), TokenKind::Bang | TokenKind::Kw("not")) {
            let start = self.peek().span.start;
            let _ = self.next();
            let inner = self.parse_unary()?;
            let end = inner.span.end;
            return Ok(Expr {
                kind: ExprKind::Unary {
                    op: UnaryOp::Not,
                    expr: Box::new(inner),
                },
                span: Span::new(start, end),
            });
        }
        self.parse_postfix()
    }

    // Postfix ::= Primary { "." Ident | "(" ExprList ")" | "[" Expr "]" } ;
    fn parse_postfix(&mut self) -> Result<Expr<'a>, ParseErr> {
        let mut expr = self.parse_primary()?;
        loop {
            if matches!(self.peek_kind(), TokenKind::Dot) {
                let _ = self.next();
                let field = self.parse_ident_any()?;
                let span = Span::new(expr.span.start, field.span.end);
                expr = Expr {
                    kind: ExprKind::Field {
                        base: Box::new(expr),
                        field,
                    },
                    span,
                };
            } else if matches!(self.peek_kind(), TokenKind::LParen) {
                let args = self.parse_paren_expr_list()?;
                let end = self.prev_end();
                let span = Span::new(expr.span.start, end);
                expr = Expr {
                    kind: ExprKind::Call {
                        callee: Box::new(expr),
                        args,
                    },
                    span,
                };
            } else if matches!(self.peek_kind(), TokenKind::LBracket) {
                let expr_start = expr.span.start;
                let _ = self.next();
                let idx_expr = self.parse_expr()?;
                let end = self.consume_punct(TokenKind::RBracket, "]")?.end;
                expr = Expr {
                    kind: ExprKind::Index {
                        base: Box::new(expr),
                        index: Box::new(idx_expr),
                    },
                    span: Span::new(expr_start, end),
                };
            } else {
                break;
            }
        }
        Ok(expr)
    }

    fn parse_paren_expr_list(&mut self) -> Result<Vec<Expr<'a>>, ParseErr> {
        let _ = self.consume_punct(TokenKind::LParen, "(")?;
        let mut args = Vec::new();
        if !matches!(self.peek_kind(), TokenKind::RParen) {
            args.push(self.parse_expr()?);
            while matches!(self.peek_kind(), TokenKind::Comma) {
                let _ = self.next();
                args.push(self.parse_expr()?);
            }
        }
        let _ = self.consume_punct(TokenKind::RParen, ")")?;
        Ok(args)
    }

    // Primary ::= IntLit | DurationLit | StringLit | Ident | true | false | present(Ident) | absent(Ident) | "(" Expr ")" ;
    fn parse_primary(&mut self) -> Result<Expr<'a>, ParseErr> {
        let tok = self.peek().clone();
        match tok.kind {
            TokenKind::Int(v) => {
                let _ = self.next();
                Ok(Expr {
                    kind: ExprKind::Int(IntLit {
                        value: v,
                        span: tok.span,
                    }),
                    span: tok.span,
                })
            }
            TokenKind::Duration(v) => {
                let _ = self.next();
                Ok(Expr {
                    kind: ExprKind::Duration(DurationLit {
                        millis: v,
                        span: tok.span,
                    }),
                    span: tok.span,
                })
            }
            TokenKind::String(ref s) => {
                let _ = self.next();
                Ok(Expr {
                    kind: ExprKind::String(StringLit {
                        value: s.clone(),
                        span: tok.span,
                    }),
                    span: tok.span,
                })
            }
            TokenKind::Kw("true") => {
                let _ = self.next();
                Ok(Expr {
                    kind: ExprKind::Bool(true),
                    span: tok.span,
                })
            }
            TokenKind::Kw("false") => {
                let _ = self.next();
                Ok(Expr {
                    kind: ExprKind::Bool(false),
                    span: tok.span,
                })
            }
            TokenKind::Kw("present") => {
                let start = tok.span.start;
                let _ = self.next();
                let _ = self.consume_punct(TokenKind::LParen, "(")?;
                let id = self.parse_ident_any()?;
                let end = self.consume_punct(TokenKind::RParen, ")")?.end;
                Ok(Expr {
                    kind: ExprKind::Present(id),
                    span: Span::new(start, end),
                })
            }
            TokenKind::Kw("absent") => {
                let start = tok.span.start;
                let _ = self.next();
                let _ = self.consume_punct(TokenKind::LParen, "(")?;
                let id = self.parse_ident_any()?;
                let end = self.consume_punct(TokenKind::RParen, ")")?.end;
                Ok(Expr {
                    kind: ExprKind::Absent(id),
                    span: Span::new(start, end),
                })
            }
            TokenKind::Ident(name) => {
                let _ = self.next();
                Ok(Expr {
                    kind: ExprKind::Ident(Ident {
                        name,
                        span: tok.span,
                    }),
                    span: tok.span,
                })
            }
            TokenKind::Kw(kw) if matches!(kw, "event" | "Cause" | "Problem") => {
                let _ = self.next();
                Ok(Expr {
                    kind: ExprKind::Ident(Ident {
                        name: kw,
                        span: tok.span,
                    }),
                    span: tok.span,
                })
            }
            TokenKind::LParen => {
                let _ = self.next();
                let expr = self.parse_expr()?;
                let _ = self.consume_punct(TokenKind::RParen, ")")?;
                Ok(expr)
            }
            _ => Err(ParseErr::new(
                "ADGL0100",
                "expected expression primary",
                "literal | identifier | (expr)",
                tok.span,
            )),
        }
    }
}

// ============================================================================
// Misc helpers
// ============================================================================

fn stmt_span(stmt: &Stmt<'_>) -> Span {
    match stmt {
        Stmt::Infer(s) => s.span,
        Stmt::Emit(s) => s.span,
        Stmt::Action(s) => s.span,
    }
}

fn stmt_end(stmt: &Stmt<'_>) -> usize {
    stmt_span(stmt).end
}

/// Convert a byte offset to `(line, column)`; both are 1-based.
pub fn line_col(src: &str, byte_off: usize) -> (usize, usize) {
    let off = byte_off.min(src.len());
    let mut line = 1usize;
    let mut col = 1usize;
    for (idx, ch) in src.char_indices() {
        if idx >= off {
            break;
        }
        if ch == '\n' {
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
    use ndsl_trivia::{Span as TriviaSpan, TriviaKind};

    #[test]
    fn line_col_handles_utf8_boundaries() {
        let src = "a\nЖ\nz";
        assert_eq!(line_col(src, 0), (1, 1));
        assert_eq!(line_col(src, 2), (2, 1));
        assert_eq!(line_col(src, src.len()), (3, 2));
    }

    #[test]
    fn lex_duration_variants() {
        let tokens = tokenize("500ms 1s 2min").unwrap_or_default();
        assert!(matches!(
            tokens.first().map(|t| &t.kind),
            Some(TokenKind::Duration(500))
        ));
        assert!(matches!(
            tokens.get(1).map(|t| &t.kind),
            Some(TokenKind::Duration(1000))
        ));
        assert!(matches!(
            tokens.get(2).map(|t| &t.kind),
            Some(TokenKind::Duration(120000))
        ));
    }

    #[test]
    fn line_comment_preserved_as_trivia() {
        let src = "// leading note\nruleset";
        let mut lexer = Lexer::new(src);
        lexer.skip_ws_and_comments().unwrap();
        let tok = lexer.next_token().unwrap();
        assert!(matches!(tok.kind, TokenKind::Kw("ruleset")));

        let trivia = lexer.trivia_before_next_token();
        assert_eq!(trivia.len(), 1);
        assert_eq!(trivia[0].kind, TriviaKind::LineComment);
        assert_eq!(trivia[0].text, "// leading note");
        assert_eq!(trivia[0].span, TriviaSpan::new(0, "// leading note".len()));
    }

    #[test]
    fn block_comment_preserved_as_trivia() {
        let src = "/* block */ruleset";
        let mut lexer = Lexer::new(src);
        lexer.skip_ws_and_comments().unwrap();
        let tok = lexer.next_token().unwrap();
        assert!(matches!(tok.kind, TokenKind::Kw("ruleset")));

        let trivia = lexer.trivia_before_next_token();
        assert_eq!(trivia.len(), 1);
        assert_eq!(trivia[0].kind, TriviaKind::BlockComment);
        assert_eq!(trivia[0].text, "/* block */");
        assert_eq!(trivia[0].span, TriviaSpan::new(0, "/* block */".len()));
    }

    #[test]
    fn unclosed_block_comment_still_errors() {
        let src = "/* unclosed\nruleset";
        let mut lexer = Lexer::new(src);
        let err = lexer.skip_ws_and_comments().unwrap_err();
        assert_eq!(err.code, "ADGL0106");
    }

    #[test]
    fn fail_fast_returns_only_first_rule_error() {
        let src = r#"
ruleset "Bad" {
  version = "1.0"
  evidence first {
    scope: Session
    anchor a: event(tcp.retransmission_burst)
    emit Problem(X) { severity: High, evidence: [a] }
  }
  evidence second {
    scope: Session
    anchor b: event(tcp.retransmission_burst)
    emit Problem(Y) { severity: High, evidence: [b] }
  }
}
"#;
        let err = parse_ruleset_fail_fast(src).expect_err("must fail");
        assert_eq!(err.len(), 1, "fail-fast must not recover: {}", err.render(src, "ff.adgl"));
        assert!(err.render(src, "ff.adgl").contains("ADGL0450"));
    }
}
