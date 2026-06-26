use std::str::Chars;

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
    Payload,
    To,
    When,
    U8,
    U16,
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
    chars: Chars<'a>,
    pos: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(input: &'a str) -> Self {
        Self {
            chars: input.chars(),
            pos: 0,
        }
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.chars.next();
        if c.is_some() {
            self.pos += 1;
        }
        c
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
                    while self.peek() != Some('\n') && self.peek().is_some() {
                        self.bump();
                    }
                }
                Some('/') if self.peek_nth(1) == Some('*') => {
                    self.bump();
                    self.bump();
                    while !(self.peek() == Some('*') && self.peek_nth(1) == Some('/'))
                        && self.peek().is_some()
                    {
                        self.bump();
                    }
                    self.bump();
                    self.bump();
                }
                _ => break,
            }
        }
    }

    fn peek_nth(&self, n: usize) -> Option<char> {
        self.chars.clone().nth(n)
    }

    pub fn next_token(&mut self) -> Token {
        self.skip_whitespace_and_comments();
        match self.bump() {
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
                    "payload" => Token::Payload,
                    "to" => Token::To,
                    "when" => Token::When,
                    "u8" => Token::U8,
                    "u16" => Token::U16,
                    _ => Token::Ident(ident),
                }
            }
            Some(c) if c.is_ascii_digit() => {
                let mut num = String::new();
                num.push(c);
                while let Some(c) = self.peek() {
                    if c.is_ascii_digit() || c == '_' {
                        num.push(self.bump().unwrap());
                    } else {
                        break;
                    }
                }
                if let Ok(v) = num.replace('_', "").parse::<i64>() {
                    Token::Int(v)
                } else {
                    Token::Error("bad int".into())
                }
            }
            Some('"') => {
                let mut s = String::new();
                while let Some(c) = self.bump() {
                    if c == '"' {
                        break;
                    }
                    s.push(c);
                }
                Token::String(s)
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
            Some('!') => Token::Bang,
            Some('~') => Token::Tilde,
            Some('*') => Token::Star,
            Some('/') => Token::Slash,
            Some('(') => Token::LParen,
            Some(',') => Token::Comma,
            Some(')') => Token::RParen,

            Some(c) => Token::Error(format!("unexpected {}", c)),
            None => Token::Eof,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lex_basic() {
        let mut l = Lexer::new("protocol ARP { message X { a: u8; } }");
        assert!(matches!(l.next_token(), Token::Protocol));
    }
}
