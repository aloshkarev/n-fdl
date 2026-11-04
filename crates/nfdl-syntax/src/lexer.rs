use std::str::Chars;

use ndsl_trivia::{Span, Trivia, TriviaKind, classify_line_comment};

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Protocol,
    Message,
    Meta,
    Endian,
    Mode,
    Big,
    Datagram,
    Validate,
    Bind,
    To,
    When,
    Ident(String),
    Int(i64),
    String(String),
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    LParen,
    RParen,
    Colon,
    Semicolon,
    Eq,
    Ne,
    Dot,
    Minus,
    Plus,
    Star,
    Slash,
    Gt,
    Lt,
    Ge,
    Le,
    And,
    Or,
    Arrow,
    Question,
    Coalesce,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
    Mod,
    Bang,
    Tilde,
    Comma,
    Eof,
    Error(String),
}

pub struct Lexer<'a> {
    input: &'a str,
    chars: Chars<'a>,
    /// Byte offset into `input`.
    pos: usize,
    /// Trivia collected while skipping ahead to the most recent token.
    pending_trivia: Vec<Trivia>,
    /// Byte span of the most recently returned token.
    last_token_span: Span,
    /// Set after the one error emitted for a source over [`MAX_SOURCE_BYTES`].
    source_faulted: bool,
}

/// A source larger than this is one error, then `Eof`. It is not a cause.
const MAX_SOURCE_BYTES: usize = 4 * 1024 * 1024;
/// A string longer than this, or an unclosed string, is `Token::Error`.
const MAX_STRING_BYTES: usize = 255;

impl<'a> Lexer<'a> {
    pub fn new(input: &'a str) -> Self {
        Self {
            input,
            chars: input.chars(),
            pos: 0,
            pending_trivia: Vec::new(),
            last_token_span: Span::unknown(),
            source_faulted: false,
        }
    }

    /// Take trivia collected immediately before the most recent [`Self::next_token`].
    pub fn trivia_before_next_token(&mut self) -> Vec<Trivia> {
        std::mem::take(&mut self.pending_trivia)
    }

    /// Byte span of the token most recently produced by [`Self::next_token`].
    pub fn last_span(&self) -> Span {
        self.last_token_span
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.chars.next()?;
        self.pos += c.len_utf8();
        Some(c)
    }

    fn peek(&self) -> Option<char> {
        self.chars.clone().next()
    }

    fn skip_whitespace_and_comments(&mut self) {
        loop {
            match self.peek() {
                Some(' ') | Some('\t') | Some('\n') | Some('\r') => {
                    self.bump();
                }
                Some('/') if self.peek_nth(1) == Some('/') => {
                    let start = self.pos;
                    while self.peek() != Some('\n') && self.peek().is_some() {
                        self.bump();
                    }
                    let end = self.pos;
                    let text = self.input[start..end].to_owned();
                    self.pending_trivia.push(Trivia {
                        kind: classify_line_comment(&text),
                        span: Span::new(start, end),
                        text,
                    });
                }
                Some('/') if self.peek_nth(1) == Some('*') => {
                    let start = self.pos;
                    self.bump();
                    self.bump();
                    while !(self.peek() == Some('*') && self.peek_nth(1) == Some('/'))
                        && self.peek().is_some()
                    {
                        self.bump();
                    }
                    self.bump();
                    self.bump();
                    let end = self.pos;
                    self.pending_trivia.push(Trivia {
                        kind: TriviaKind::BlockComment,
                        span: Span::new(start, end),
                        text: self.input[start..end].to_owned(),
                    });
                }
                _ => break,
            }
        }
    }

    fn peek_nth(&self, n: usize) -> Option<char> {
        self.chars.clone().nth(n)
    }

    pub fn next_token(&mut self) -> Token {
        if self.input.len() > MAX_SOURCE_BYTES {
            if !self.source_faulted {
                self.source_faulted = true;
                self.last_token_span = Span::unknown();
                return Token::Error("source exceeds 4MiB".into());
            }
            return Token::Eof;
        }
        self.pending_trivia.clear();
        self.skip_whitespace_and_comments();
        let start = self.pos;
        let tok = match self.bump() {
            Some('{') => Token::LBrace,
            Some('}') => Token::RBrace,
            Some('[') => Token::LBracket,
            Some(']') => Token::RBracket,
            Some(':') => Token::Colon,
            Some(';') => Token::Semicolon,
            Some('=') => Token::Eq,
            Some('.') => Token::Dot,
            Some('-') => {
                if self.peek() == Some('>') {
                    self.bump(); // consume >
                    Token::Arrow
                } else {
                    Token::Minus
                }
            }
            Some(c) if c.is_ascii_alphabetic() || c == '_' => {
                let mut ident = String::new();
                ident.push(c);
                while let Some(c) = self.peek() {
                    if c.is_ascii_alphanumeric() || c == '_' {
                        ident.push(self.bump().unwrap());
                    } else {
                        break;
                    }
                }
                match ident.as_str() {
                    "protocol" => Token::Protocol,
                    "message" => Token::Message,
                    "meta" => Token::Meta,
                    "endian" => Token::Endian,
                    "mode" => Token::Mode,
                    "big" => Token::Big,
                    "datagram" => Token::Datagram,
                    "validate" => Token::Validate,
                    "bind" => Token::Bind,
                    // `payload` stays an `Ident`: it is a common field name and
                    // the bind syntax reads it positionally (`bind L payload to M`).
                    "to" => Token::To,
                    "when" => Token::When,
                    // `u8`/`u16`/`u24`/`u32` stay as `Ident` so the type parser
                    // matches them uniformly via `Token::Ident(t) if t == "uN"`.
                    _ => Token::Ident(ident),
                }
            }
            Some(c) if c.is_ascii_digit() => {
                let mut num = String::new();
                num.push(c);
                let radix = if c == '0' {
                    match self.peek() {
                        Some('x') | Some('X') => {
                            self.bump();
                            16
                        }
                        Some('o') | Some('O') => {
                            self.bump();
                            8
                        }
                        Some('b') | Some('B') => {
                            self.bump();
                            2
                        }
                        _ => 10,
                    }
                } else {
                    10
                };
                if radix != 10 {
                    // num currently holds the leading "0" prefix; drop it before digits
                    num.clear();
                }
                while let Some(ch) = self.peek() {
                    if ch.is_digit(radix) || ch == '_' {
                        num.push(self.bump().unwrap());
                    } else {
                        break;
                    }
                }
                let digits: String = num.chars().filter(|c| *c != '_').collect();
                match i64::from_str_radix(&digits, radix) {
                    Ok(v) => Token::Int(v),
                    Err(_) => Token::Error("bad int".into()),
                }
            }
            Some('"') => {
                let mut s = String::new();
                let mut closed = false;
                let mut truncated = false;
                while let Some(c) = self.bump() {
                    if c == '"' {
                        closed = true;
                        break;
                    }
                    if truncated || s.len() + c.len_utf8() > MAX_STRING_BYTES {
                        truncated = true;
                        continue;
                    }
                    s.push(c);
                }
                if closed && !truncated {
                    Token::String(s)
                } else {
                    Token::Error("truncated string".into())
                }
            }
            Some('>') => {
                if self.peek() == Some('=') {
                    self.bump();
                    Token::Ge
                } else if self.peek() == Some('>') {
                    self.bump();
                    Token::Shr
                } else {
                    Token::Gt
                }
            }
            Some('<') => {
                if self.peek() == Some('=') {
                    self.bump();
                    Token::Le
                } else if self.peek() == Some('<') {
                    self.bump();
                    Token::Shl
                } else {
                    Token::Lt
                }
            }
            Some('?') => {
                if self.peek() == Some('?') {
                    self.bump();
                    Token::Coalesce
                } else {
                    Token::Question
                }
            }
            Some('&') => {
                if self.peek() == Some('&') {
                    self.bump();
                    Token::And
                } else {
                    Token::BitAnd
                }
            }
            Some('|') => {
                if self.peek() == Some('|') {
                    self.bump();
                    Token::Or
                } else {
                    Token::BitOr
                }
            }
            Some('^') => Token::BitXor,
            Some('%') => Token::Mod,
            Some('!') => {
                if self.peek() == Some('=') {
                    self.bump();
                    Token::Ne
                } else {
                    Token::Bang
                }
            }
            Some('~') => Token::Tilde,
            Some('*') => Token::Star,
            Some('/') => Token::Slash,
            Some('(') => Token::LParen,
            Some(',') => Token::Comma,
            Some(')') => Token::RParen,

            Some(c) => Token::Error(format!("unexpected {}", c)),
            None => Token::Eof,
        };
        self.last_token_span = Span::new(start, self.pos);
        tok
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ndsl_trivia::{Span, TriviaKind};

    #[test]
    fn lex_basic() {
        let mut l = Lexer::new("protocol ARP { message X { a: u8; } }");
        assert!(matches!(l.next_token(), Token::Protocol));
        assert_eq!(l.last_span(), Span::new(0, "protocol".len()));
    }

    #[test]
    fn line_comment_preserved_as_trivia() {
        let src = "// leading note\nprotocol";
        let mut lexer = Lexer::new(src);
        assert!(matches!(lexer.next_token(), Token::Protocol));

        let trivia = lexer.trivia_before_next_token();
        assert_eq!(trivia.len(), 1);
        assert_eq!(trivia[0].kind, TriviaKind::LineComment);
        assert_eq!(trivia[0].text, "// leading note");
        assert_eq!(trivia[0].span, Span::new(0, "// leading note".len()));
    }

    #[test]
    fn block_comment_preserved_as_trivia() {
        let src = "/* block */protocol";
        let mut lexer = Lexer::new(src);
        assert!(matches!(lexer.next_token(), Token::Protocol));

        let trivia = lexer.trivia_before_next_token();
        assert_eq!(trivia.len(), 1);
        assert_eq!(trivia[0].kind, TriviaKind::BlockComment);
        assert_eq!(trivia[0].text, "/* block */");
        assert_eq!(trivia[0].span, Span::new(0, "/* block */".len()));
    }

    #[test]
    fn doc_comment_preserved_as_trivia() {
        let src = "/// hello\nprotocol";
        let mut lexer = Lexer::new(src);
        assert!(matches!(lexer.next_token(), Token::Protocol));

        let trivia = lexer.trivia_before_next_token();
        assert_eq!(trivia.len(), 1);
        assert_eq!(trivia[0].kind, TriviaKind::DocComment);
        assert_eq!(trivia[0].text, "/// hello");
        assert_eq!(trivia[0].span, Span::new(0, "/// hello".len()));
    }

    #[test]
    fn unclosed_or_long_string_is_truncated_and_consumed() {
        let mut open = Lexer::new("\"abc");
        assert_eq!(open.next_token(), Token::Error("truncated string".into()));
        assert_eq!(open.next_token(), Token::Eof);

        let long_src = format!("\"{}\" protocol", "a".repeat(256));
        let mut long = Lexer::new(&long_src);
        assert_eq!(long.next_token(), Token::Error("truncated string".into()));
        assert!(matches!(long.next_token(), Token::Protocol));
    }

    #[test]
    fn source_over_4mib_is_one_error_then_eof() {
        let src = "a".repeat(MAX_SOURCE_BYTES + 1);
        let mut lexer = Lexer::new(&src);
        assert_eq!(
            lexer.next_token(),
            Token::Error("source exceeds 4MiB".into())
        );
        assert_eq!(lexer.next_token(), Token::Eof);
        assert_eq!(lexer.next_token(), Token::Eof);
    }
}
