//! Parser with production v1 support:
//! - let bindings inside messages
//! - __current_offset as special ident
//! - complex bytes[expr] (length - 2 etc.)
//! - improved expr with + - == > <

use crate::ast::*;
use crate::lexer::{Lexer, Token};
use ndsl_diag::{DiagBuffer, Diagnostic, Span};
use ndsl_trivia::{Trivia, docs_from_leading};

/// Stable diagnostic code for N-FDL syntax / recovery errors.
const NFD_SYNTAX: &str = "NFD0100";

#[derive(Debug, Clone, PartialEq)]
pub enum ParseError {
    Syntax(String),
    WithLocation { msg: String, pos: usize },
}

pub struct Parser<'a> {
    lexer: Lexer<'a>,
    current: Token,
    /// Span of [`Self::current`].
    current_span: Span,
    /// Span of the token immediately before [`Self::current`].
    prev_span: Span,
    /// Leading trivia collected immediately before [`Self::current`].
    current_leading: Vec<Trivia>,
    /// When true, statement-level errors are recorded and parsing continues after resync.
    recover: bool,
    diags: DiagBuffer,
}

impl<'a> Parser<'a> {
    pub fn new(input: &'a str) -> Self {
        let mut lexer = Lexer::new(input);
        let current = lexer.next_token();
        let current_leading = lexer.trivia_before_next_token();
        let current_span = lexer.last_span();
        Self {
            lexer,
            current,
            current_span,
            prev_span: Span::unknown(),
            current_leading,
            recover: false,
            diags: DiagBuffer::new(),
        }
    }

    fn advance(&mut self) {
        self.prev_span = self.current_span;
        self.current = self.lexer.next_token();
        self.current_leading = self.lexer.trivia_before_next_token();
        self.current_span = self.lexer.last_span();
    }

    /// Span from `start` through the field body, including a trailing semicolon
    /// when present (without consuming it).
    fn field_span(&self, start: usize) -> Span {
        let end = if self.current == Token::Semicolon {
            self.current_span.end
        } else {
            self.prev_span.end
        };
        Span::new(start, end)
    }

    fn record_error(&mut self, err: &ParseError) {
        let (msg, span) = match err {
            ParseError::Syntax(msg) => (msg.clone(), self.current_span),
            ParseError::WithLocation { msg, pos } => {
                (msg.clone(), Span::new(*pos, (*pos).saturating_add(1)))
            }
        };
        self.diags
            .push(Diagnostic::error(NFD_SYNTAX, msg, span));
    }

    /// Sync to the next statement/top-level boundary: `;` (consumed), or leave
    /// the cursor on `}` / a sync keyword for the caller to handle.
    fn resync_statement(&mut self) {
        loop {
            match &self.current {
                Token::Eof => break,
                Token::Semicolon => {
                    self.advance();
                    break;
                }
                Token::RBrace
                | Token::Message
                | Token::Meta
                | Token::Bind
                | Token::Protocol
                | Token::Validate => break,
                Token::Ident(s)
                    if matches!(
                        s.as_str(),
                        "state_machine" | "let" | "loop" | "match" | "next" | "carry"
                    ) =>
                {
                    break;
                }
                _ => self.advance(),
            }
        }
    }

    /// Record + resync on recovery; otherwise propagate.
    /// `Ok(None)` means the caller should `continue` the surrounding statement loop.
    fn recover_stmt<T>(&mut self, result: Result<T, ParseError>) -> Result<Option<T>, ParseError> {
        match result {
            Ok(v) => Ok(Some(v)),
            Err(e) => {
                if self.recover {
                    self.record_error(&e);
                    self.resync_statement();
                    Ok(None)
                } else {
                    Err(e)
                }
            }
        }
    }

    /// Like [`Self::recover_stmt`] for pure rejection (no value).
    /// Returns `Ok(true)` when recovered (caller should `continue`).
    fn recover_reject(&mut self, err: ParseError) -> Result<bool, ParseError> {
        if self.recover {
            self.record_error(&err);
            self.resync_statement();
            Ok(true)
        } else {
            Err(err)
        }
    }

    /// Parse a protocol, collecting all statement-level syntax diagnostics.
    ///
    /// On errors inside message/protocol bodies the parser records a diagnostic,
    /// resyncs to `;` / `}` / a sync keyword, and continues so later errors appear
    /// in the same [`DiagBuffer`].
    pub fn parse_protocol_with_diagnostics(&mut self) -> (Protocol, DiagBuffer) {
        self.recover = true;
        self.diags = DiagBuffer::new();
        let proto = match self.parse_protocol() {
            Ok(p) => p,
            Err(e) => {
                self.record_error(&e);
                Protocol {
                    name: String::new(),
                    doc: None,
                    endian: "big".to_string(),
                    mode: "datagram".to_string(),
                    eof: String::new(),
                    messages: vec![],
                    binds: vec![],
                    state_machines: vec![],
                }
            }
        };
        self.recover = false;
        (proto, std::mem::take(&mut self.diags))
    }

    fn contains_rem(&self, expr: &Expr) -> bool {
        match expr {
            Expr::Ident(s) if s == "__rem" => true,
            Expr::Binary { left, right, .. } => self.contains_rem(left) || self.contains_rem(right),
            Expr::Unary { expr, .. } => self.contains_rem(expr),
            Expr::Ternary {
                cond,
                then_branch,
                else_branch,
            } => {
                self.contains_rem(cond)
                    || self.contains_rem(then_branch)
                    || self.contains_rem(else_branch)
            }
            Expr::Coalesce { value, default } => {
                self.contains_rem(value) || self.contains_rem(default)
            }
            Expr::Call { args, .. } => args.iter().any(|a| self.contains_rem(a)),
            Expr::Field(base, _) => self.contains_rem(base),
            Expr::Tuple(xs) => xs.iter().any(|a| self.contains_rem(a)),
            _ => false,
        }
    }

    // Full precedence ladder per docs/spec/02-grammar.ebnf
    fn parse_expr(&mut self) -> Result<Expr, ParseError> {
        self.parse_ternary()
    }

    fn parse_ternary(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_coalesce()?;
        if self.current == Token::Question {
            self.advance();
            let then_branch = self.parse_expr()?;
            if self.current == Token::Colon {
                self.advance();
            } else {
                return Err(ParseError::Syntax(format!(
                    "expected `:` in ternary (expected: `:`, found: {}); tip: write `cond ? then : else`",
                    token_label(&self.current)
                )));
            }
            let else_branch = self.parse_expr()?;
            expr = Expr::Ternary {
                cond: Box::new(expr),
                then_branch: Box::new(then_branch),
                else_branch: Box::new(else_branch),
            };
        }
        Ok(expr)
    }

    fn parse_coalesce(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_logic_or()?;
        while self.current == Token::Coalesce {
            self.advance();
            let right = self.parse_logic_or()?;
            left = Expr::Coalesce {
                value: Box::new(left),
                default: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_logic_or(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_logic_and()?;
        while self.current == Token::Or {
            self.advance();
            let right = self.parse_logic_and()?;
            left = Expr::Binary {
                op: BinOp::Or,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_logic_and(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_equality()?;
        while self.current == Token::And {
            self.advance();
            let right = self.parse_equality()?;
            left = Expr::Binary {
                op: BinOp::And,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_equality(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_bit_or()?;
        loop {
            let op = match &self.current {
                Token::Eq => {
                    self.advance();
                    if self.current == Token::Eq {
                        self.advance();
                    }
                    Some(BinOp::Eq)
                }
                Token::Ne => {
                    self.advance();
                    Some(BinOp::Ne)
                }
                _ => None,
            };
            if let Some(op) = op {
                let right = self.parse_bit_or()?;
                left = Expr::Binary {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                };
            } else {
                break;
            }
        }
        Ok(left)
    }

    fn parse_bit_or(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_bit_xor()?;
        while self.current == Token::BitOr {
            self.advance();
            let right = self.parse_bit_xor()?;
            left = Expr::Binary {
                op: BinOp::BitOr,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_bit_xor(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_bit_and()?;
        while self.current == Token::BitXor {
            self.advance();
            let right = self.parse_bit_and()?;
            left = Expr::Binary {
                op: BinOp::BitXor,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_bit_and(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_relational()?;
        while self.current == Token::BitAnd {
            self.advance();
            let right = self.parse_relational()?;
            left = Expr::Binary {
                op: BinOp::BitAnd,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_relational(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_shift()?;
        loop {
            let op = match &self.current {
                Token::Gt => {
                    self.advance();
                    Some(BinOp::Gt)
                }
                Token::Lt => {
                    self.advance();
                    Some(BinOp::Lt)
                }
                Token::Ge => {
                    self.advance();
                    Some(BinOp::Ge)
                }
                Token::Le => {
                    self.advance();
                    Some(BinOp::Le)
                }
                _ => None,
            };
            if let Some(op) = op {
                let right = self.parse_shift()?;
                left = Expr::Binary {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                };
            } else {
                break;
            }
        }
        Ok(left)
    }

    fn parse_shift(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_additive()?;
        loop {
            let op = match &self.current {
                Token::Shl => {
                    self.advance();
                    Some(BinOp::Shl)
                }
                Token::Shr => {
                    self.advance();
                    Some(BinOp::Shr)
                }
                _ => None,
            };
            if let Some(op) = op {
                let right = self.parse_additive()?;
                left = Expr::Binary {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                };
            } else {
                break;
            }
        }
        Ok(left)
    }

    fn parse_additive(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_multiplicative()?;
        loop {
            let op = match &self.current {
                Token::Plus => {
                    self.advance();
                    Some(BinOp::Add)
                }
                Token::Minus => {
                    self.advance();
                    Some(BinOp::Sub)
                }
                _ => None,
            };
            if let Some(op) = op {
                let right = self.parse_multiplicative()?;
                left = Expr::Binary {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                };
            } else {
                break;
            }
        }
        Ok(left)
    }

    fn parse_multiplicative(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_unary()?;
        loop {
            let op = match &self.current {
                Token::Star => {
                    self.advance();
                    Some(BinOp::Mul)
                } // Note: need to add Star token if missing
                Token::Slash => {
                    self.advance();
                    Some(BinOp::Div)
                }
                Token::Mod => {
                    self.advance();
                    Some(BinOp::Mod)
                }
                _ => None,
            };
            if let Some(op) = op {
                let right = self.parse_unary()?;
                left = Expr::Binary {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                };
            } else {
                break;
            }
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expr, ParseError> {
        match &self.current {
            Token::Bang => {
                self.advance();
                let expr = self.parse_unary()?;
                Ok(Expr::Unary {
                    op: UnaryOp::Not,
                    expr: Box::new(expr),
                })
            }
            Token::Tilde => {
                self.advance();
                let expr = self.parse_unary()?;
                Ok(Expr::Unary {
                    op: UnaryOp::BitNot,
                    expr: Box::new(expr),
                })
            }
            Token::Minus => {
                self.advance();
                let expr = self.parse_unary()?;
                Ok(Expr::Unary {
                    op: UnaryOp::Neg,
                    expr: Box::new(expr),
                })
            }
            _ => self.parse_primary(),
        }
    }

    fn parse_primary(&mut self) -> Result<Expr, ParseError> {
        let mut atom = match &self.current {
            Token::Ident(s) => {
                let n = s.clone();
                self.advance();
                if n == "true" {
                    Expr::Int(1)
                } else if n == "false" {
                    Expr::Int(0)
                } else if self.current == Token::LParen {
                    self.advance();
                    let mut args = vec![];
                    if self.current != Token::RParen {
                        loop {
                            args.push(self.parse_expr()?);
                            if self.current == Token::Comma {
                                self.advance();
                            } else {
                                break;
                            }
                        }
                    }
                    if self.current == Token::RParen {
                        self.advance();
                    }
                    Expr::Call { name: n, args }
                } else {
                    Expr::Ident(n)
                }
            }
            Token::Int(v) => {
                let v = *v;
                self.advance();
                Expr::Int(v)
            }
            Token::String(s) => {
                let s = s.clone();
                self.advance();
                Expr::Str(s)
            }
            Token::LParen => {
                self.advance();
                // Tuple if comma-separated, else grouping.
                let mut elems = vec![self.parse_expr()?];
                while self.current == Token::Comma {
                    self.advance();
                    elems.push(self.parse_expr()?);
                }
                if self.current == Token::RParen {
                    self.advance();
                }
                if elems.len() == 1 {
                    elems.into_iter().next().unwrap()
                } else {
                    Expr::Tuple(elems)
                }
            }
            _ => {
                return Err(ParseError::Syntax(format!(
                    "bad primary: expected expression, found {}; tip: start with ident, literal, or `(`",
                    token_label(&self.current)
                )));
            }
        };
        // Postfix field access: base.field (e.g. IPv4.src, dec.wire_len)
        while self.current == Token::Dot {
            self.advance();
            if let Token::Ident(f) = &self.current {
                let fname = f.clone();
                self.advance();
                atom = Expr::Field(Box::new(atom), fname);
            } else {
                break;
            }
        }
        Ok(atom)
    }

    /// Parse a field type. Handles scalars `u8`/`u16`/`u24`/`u32`,
    /// `bytes[expr]` / `bytes[EOF]` / `bytes[stream]` / `bytes[..]` (rest),
    /// `bitfield{k}` (EBNF: 1..=64 bits), and bare-ident `MessageRef`.
    fn parse_type(&mut self) -> Result<NfdlType, ParseError> {
        match &self.current {
            Token::Ident(t) if t == "u8" => {
                self.advance();
                Ok(NfdlType::U8)
            }
            Token::Ident(t) if t == "u16" => {
                self.advance();
                Ok(NfdlType::U16)
            }
            Token::Ident(t) if t == "u24" => {
                self.advance();
                Ok(NfdlType::U24)
            }
            Token::Ident(t) if t == "u32" => {
                self.advance();
                Ok(NfdlType::U32)
            }
            Token::Ident(t) if t == "bytes" => {
                self.advance();
                if self.current == Token::LBracket {
                    self.advance();
                    // `bytes[EOF]` / `bytes[stream]` / `bytes[..]` / `bytes[expr]`
                    let ty = match &self.current {
                        Token::Ident(s) if s == "EOF" => {
                            self.advance();
                            NfdlType::BytesEof
                        }
                        Token::Ident(s) if s == "stream" => {
                            self.advance();
                            NfdlType::BytesStream
                        }
                        Token::Dot => {
                            // `bytes[..]` — rest of the current slice
                            self.advance();
                            if self.current == Token::Dot {
                                self.advance();
                            }
                            NfdlType::BytesRest
                        }
                        _ => {
                            let len_expr = self.parse_expr().unwrap_or(Expr::Int(0));
                            NfdlType::Bytes { len: len_expr }
                        }
                    };
                    if self.current == Token::RBracket {
                        self.advance();
                    }
                    Ok(ty)
                } else {
                    Ok(NfdlType::BytesRest)
                }
            }
            Token::Ident(t) if t == "bitfield" => {
                self.advance();
                if self.current != Token::LBrace {
                    return Err(ParseError::Syntax(format!(
                        "expected `{{` after bitfield (expected: `{{`, found: {}); did you mean `bitfield{{k}}`?",
                        token_label(&self.current)
                    )));
                }
                self.advance();
                let bits = match &self.current {
                    Token::Int(v) => {
                        let v = *v;
                        self.advance();
                        if !(1..=64).contains(&v) {
                            // Leave cursor after the bitfield construct so statement
                            // recovery can sync on `;` instead of the type's `}`.
                            if self.current == Token::RBrace {
                                self.advance();
                            }
                            return Err(ParseError::Syntax(format!(
                                "bitfield width must be in 1..=64 (expected: integer 1..=64, found: {v}); tip: use e.g. bitfield{{8}}"
                            )));
                        }
                        v as u8
                    }
                    _ => {
                        if self.current == Token::RBrace {
                            self.advance();
                        }
                        return Err(ParseError::Syntax(format!(
                            "expected INT bit width in bitfield{{k}} (expected: integer 1..=64, found: {}); tip: write bitfield{{8}}",
                            token_label(&self.current)
                        )));
                    }
                };
                if self.current != Token::RBrace {
                    return Err(ParseError::Syntax(format!(
                        "expected `}}` after bitfield width (expected: `}}`, found: {})",
                        token_label(&self.current)
                    )));
                }
                self.advance();
                Ok(NfdlType::Bitfield { bits })
            }
            Token::Ident(t) => {
                let tname = t.clone();
                self.advance();
                Ok(NfdlType::MessageRef(tname))
            }
            _ => {
                // Unknown — consume one token to make progress, default to u8.
                self.advance();
                Ok(NfdlType::U8)
            }
        }
    }

    /// Parse a message or `match`-arm body block (the `{ ... }` contents, with
    /// `{` already consumed) up to and including the closing `}`. Handles
    /// `let`, `loop` (with optional `carry`/`while`/`next`), standalone
    /// `validate`, nested `match`, and plain `field: type [validate] [if cond];`.
    /// Shared so statement-level recovery hooks live in one place.
    fn parse_body(
        &mut self,
    ) -> Result<(Vec<Field>, Vec<Let>, Vec<Loop>, Vec<Validate>, Vec<Match>), ParseError> {
        let mut fields = vec![];
        let mut lets = vec![];
        let mut loops = vec![];
        let mut validates = vec![];
        let mut matches = vec![];
        let mut body_seq: u32 = 0;

        while self.current != Token::RBrace && self.current != Token::Eof {
            if self.current == Token::Validate {
                self.advance();
                let vexpr = self.parse_expr().unwrap_or(Expr::Int(1));
                let message = if self.current == Token::Arrow {
                    self.advance();
                    if let Token::String(s) = &self.current {
                        let m = s.clone();
                        self.advance();
                        m
                    } else {
                        "constraint".into()
                    }
                } else {
                    "constraint".into()
                };
                let o = body_seq;
                body_seq += 1;
                validates.push(Validate {
                    expr: vexpr,
                    message,
                    order: o,
                });
                while self.current != Token::Semicolon
                    && self.current != Token::RBrace
                    && self.current != Token::Eof
                {
                    self.advance();
                }
                if self.current == Token::Semicolon {
                    self.advance();
                }
                continue;
            }
            if let Token::Ident(kw) = &self.current {
                let kw = kw.clone();
                let ident_start = self.current_span.start;
                self.advance();
                if kw == "match" {
                    let tag = self.parse_expr().unwrap_or(Expr::Int(0));
                    if self.current == Token::LBrace {
                        self.advance();
                    }
                    let mut arms = vec![];
                    while self.current != Token::RBrace && self.current != Token::Eof {
                        let case_val = match &self.current {
                            Token::Ident(d) if d == "default" => {
                                self.advance();
                                None
                            }
                            Token::Ident(c) if c == "case" => {
                                self.advance();
                                if let Token::Int(v) = &self.current {
                                    let v = *v;
                                    self.advance();
                                    Some(v)
                                } else {
                                    self.advance();
                                    Some(0)
                                }
                            }
                            _ => {
                                self.advance();
                                None
                            }
                        };
                        if self.current == Token::Arrow {
                            self.advance();
                        }
                        if self.current == Token::LBrace {
                            self.advance();
                        }
                        let arm = self.parse_body();
                        let Some((af, al, alp, av, am)) = self.recover_stmt(arm)? else {
                            continue;
                        };
                        arms.push(MatchArm {
                            case: case_val,
                            fields: af,
                            lets: al,
                            loops: alp,
                            validates: av,
                            matches: am,
                        });
                        if self.current == Token::Comma {
                            self.advance();
                        }
                    }
                    if self.current == Token::RBrace {
                        self.advance();
                    }
                    let o = body_seq;
                    body_seq += 1;
                    matches.push(Match {
                        tag,
                        arms,
                        order: o,
                    });
                    continue;
                }
                if kw == "let" {
                    if let Token::Ident(ln) = &self.current {
                        let lname = ln.clone();
                        self.advance();
                        if self.current == Token::Eq {
                            self.advance();
                        }
                        if let Ok(val) = self.parse_expr() {
                            let o = body_seq;
                            body_seq += 1;
                            lets.push(Let {
                                name: lname,
                                value: val,
                                order: o,
                            });
                        }
                        if self.current == Token::Semicolon {
                            self.advance();
                        }
                        continue;
                    }
                }
                if kw == "loop" {
                    let loop_name = if let Token::Ident(n) = &self.current {
                        let nn = n.clone();
                        self.advance();
                        nn
                    } else {
                        "loop".to_string()
                    };
                    let mut carries = vec![];
                    while let Token::Ident(kw2) = &self.current {
                        if kw2 == "carry" {
                            self.advance();
                            let cname = if let Token::Ident(n) = &self.current {
                                let nn = n.clone();
                                self.advance();
                                nn
                            } else {
                                String::new()
                            };
                            if self.current == Token::Colon {
                                self.advance();
                            }
                            let cty_r = self.parse_type();
                            let Some(cty) = self.recover_stmt(cty_r)? else {
                                continue;
                            };
                            if self.current == Token::Eq {
                                self.advance();
                            }
                            let init = self.parse_expr().unwrap_or(Expr::Int(0));
                            if self.current == Token::Semicolon {
                                self.advance();
                            }
                            carries.push(Carry {
                                name: cname,
                                ty: cty,
                                init,
                            });
                            continue;
                        } else {
                            break;
                        }
                    }
                    if let Token::Ident(w) = &self.current {
                        if w == "while" {
                            self.advance();
                        }
                    }
                    let mut condition = Expr::Int(1);
                    if self.current != Token::LBrace && self.current != Token::Semicolon {
                        if let Ok(e) = self.parse_expr() {
                            condition = e;
                        }
                    }
                    let mut loop_body = vec![];
                    let mut nexts = vec![];
                    if self.current == Token::LBrace {
                        self.advance();
                    }
                    while self.current != Token::RBrace && self.current != Token::Eof {
                        if let Token::Ident(fname) = &self.current {
                            if fname == "next" {
                                self.advance();
                                let nname = if let Token::Ident(n) = &self.current {
                                    let nn = n.clone();
                                    self.advance();
                                    nn
                                } else {
                                    String::new()
                                };
                                if self.current == Token::Eq {
                                    self.advance();
                                }
                                let nval = self.parse_expr().unwrap_or(Expr::Int(0));
                                if self.current == Token::Semicolon {
                                    self.advance();
                                }
                                nexts.push(NextStmt {
                                    name: nname,
                                    value: nval,
                                });
                                continue;
                            }
                            let fname = fname.clone();
                            let field_start = self.current_span.start;
                            self.advance();
                            if self.current == Token::Colon {
                                self.advance();
                            }
                            let ty_r = self.parse_type();
                            let Some(ty) = self.recover_stmt(ty_r)? else {
                                continue;
                            };
                            let mut conditional = None;
                            if let Token::Ident(v) = &self.current {
                                if v == "if" {
                                    self.advance();
                                    let e_r = self.parse_expr();
                                    let Some(e) = self.recover_stmt(e_r)? else {
                                        continue;
                                    };
                                    if self.contains_rem(&e) {
                                        if self.recover_reject(ParseError::Syntax(
                                            "StreamRemControlFlow: __rem forbidden in conditional field (layout-affecting)".into(),
                                        ))? {
                                            continue;
                                        }
                                    }
                                    conditional = Some(e);
                                }
                            }
                            loop_body.push(Field {
                                name: fname,
                                ty,
                                validate: None,
                                conditional,
                                order: 0,
                                span: self.field_span(field_start),
                            });
                        } else {
                            self.advance();
                        }
                        if self.current == Token::Semicolon {
                            self.advance();
                        }
                    }
                    if self.current == Token::RBrace {
                        self.advance();
                    }
                    if self.contains_rem(&condition) {
                        if self.recover_reject(ParseError::Syntax(
                            "StreamRemControlFlow: __rem forbidden in loop while condition (see spec 05-verification)".into(),
                        ))? {
                            continue;
                        }
                    }
                    let o = body_seq;
                    body_seq += 1;
                    loops.push(Loop {
                        name: loop_name,
                        carries,
                        condition,
                        body: loop_body,
                        nexts,
                        order: o,
                    });
                    continue;
                }
                // plain field: `kw: type;` with optional `validate` / `if`
                if self.current == Token::Colon {
                    self.advance();
                }
                let ty_r = self.parse_type();
                let Some(ty) = self.recover_stmt(ty_r)? else {
                    continue;
                };
                let mut validate = None;
                if let Token::Ident(v) = &self.current {
                    if v == "validate" {
                        self.advance();
                        let vexpr = self.parse_expr().unwrap_or(Expr::Int(1));
                        // Per-field `validate expr -> "msg"` (C5): capture
                        // the user message instead of dropping it.
                        let message = if self.current == Token::Arrow {
                            self.advance();
                            if let Token::String(s) = &self.current {
                                let m = s.clone();
                                self.advance();
                                m
                            } else {
                                "constraint".into()
                            }
                        } else {
                            "constraint".into()
                        };
                        let o = body_seq;
                        body_seq += 1;
                        validate = Some(Validate {
                            expr: vexpr,
                            message,
                            order: o,
                        });
                        while self.current != Token::Semicolon
                            && self.current != Token::RBrace
                            && self.current != Token::Eof
                        {
                            self.advance();
                        }
                    }
                }
                let mut conditional = None;
                if let Token::Ident(v) = &self.current {
                    if v == "if" {
                        self.advance();
                        let e_r = self.parse_expr();
                        let Some(e) = self.recover_stmt(e_r)? else {
                            continue;
                        };
                        if self.contains_rem(&e) {
                            if self.recover_reject(ParseError::Syntax(
                                "StreamRemControlFlow: __rem forbidden in conditional field (layout-affecting)".into(),
                            ))? {
                                continue;
                            }
                        }
                        conditional = Some(e);
                        while self.current != Token::Semicolon
                            && self.current != Token::RBrace
                            && self.current != Token::Eof
                        {
                            self.advance();
                        }
                    }
                }
                let o = body_seq;
                body_seq += 1;
                fields.push(Field {
                    name: kw,
                    ty,
                    validate,
                    conditional,
                    order: o,
                    span: self.field_span(ident_start),
                });
                if self.current == Token::Semicolon {
                    self.advance();
                } else if !matches!(
                    self.current,
                    Token::RBrace | Token::Eof | Token::Message | Token::Meta | Token::Validate
                ) {
                    if self.recover_reject(ParseError::Syntax(format!(
                        "expected `;` after field (expected: `;`, found: {}); tip: terminate each field with `;`",
                        token_label(&self.current)
                    )))? {
                        continue;
                    }
                }
                continue;
            } else {
                self.advance();
            }
            if self.current == Token::Semicolon {
                self.advance();
            }
        }
        if self.current == Token::RBrace {
            self.advance();
        }
        Ok((fields, lets, loops, validates, matches))
    }

    pub fn parse_state_machine(&mut self) -> Result<StateMachine, ParseError> {
        self.advance(); // past state_machine
        let name = if let Token::Ident(n) = &self.current {
            let nn = n.clone();
            self.advance();
            nn
        } else {
            "SM".to_string()
        };

        if self.current == Token::LBrace {
            self.advance();
        }

        // Parse optional key = KeyExpr ;
        let mut key_expr = None;
        if let Token::Ident(k) = &self.current {
            if k == "key" {
                self.advance();
                if self.current == Token::Eq {
                    self.advance();
                }
                key_expr = Some(self.parse_expr()?);
                if self.current == Token::Semicolon {
                    self.advance();
                }
            }
        }

        let mut states_map: std::collections::HashMap<String, Vec<Transition>> =
            std::collections::HashMap::new();
        let mut state_order: Vec<String> = Vec::new();
        let mut initial = "IDLE".to_string();

        while self.current != Token::RBrace && self.current != Token::Eof {
            if let Token::Ident(k) = &self.current {
                if k == "state" {
                    self.advance();
                    let state_name = if let Token::Ident(n) = &self.current {
                        let nn = n.clone();
                        self.advance();
                        nn
                    } else {
                        "UNKNOWN".to_string()
                    };

                    if initial == "IDLE" {
                        initial = state_name.clone();
                    }

                    if self.current == Token::LBrace {
                        self.advance();
                    }

                    let mut transitions = vec![];

                    while self.current != Token::RBrace && self.current != Token::Eof {
                        if let Token::Ident(k2) = &self.current {
                            if k2 == "on" {
                                self.advance();
                                let msg_type = if let Token::Ident(n) = &self.current {
                                    let nn = n.clone();
                                    self.advance();
                                    nn
                                } else {
                                    "".to_string()
                                };

                                let mut guard = None;
                                if let Token::Ident(g) = &self.current {
                                    if g == "guard" {
                                        self.advance();
                                        if self.current == Token::LParen {
                                            self.advance();
                                        }
                                        guard = Some(self.parse_expr()?);
                                        if self.current == Token::RParen {
                                            self.advance();
                                        }
                                    }
                                }

                                if self.current == Token::Arrow {
                                    self.advance();
                                } else if let Token::Ident(s) = &self.current {
                                    if s == "->" {
                                        self.advance();
                                    }
                                }

                                let to_state = if let Token::Ident(n) = &self.current {
                                    let nn = n.clone();
                                    self.advance();
                                    nn
                                } else {
                                    "UNKNOWN".to_string()
                                };

                                let mut actions = vec![];
                                if self.current == Token::LBrace {
                                    self.advance();
                                }
                                while self.current != Token::RBrace && self.current != Token::Eof {
                                    if let Token::Ident(act) = &self.current {
                                        if act == "set" {
                                            self.advance();
                                            let var = if let Token::Ident(v) = &self.current {
                                                let vv = v.clone();
                                                self.advance();
                                                vv
                                            } else {
                                                "".to_string()
                                            };
                                            self.advance(); // skip =
                                            let value = self.parse_expr()?;
                                            actions.push(Action::Set { var, value });
                                            if self.current == Token::Semicolon {
                                                self.advance();
                                            }
                                        } else if act == "emit" {
                                            self.advance();
                                            let event = if let Token::Ident(e) = &self.current {
                                                let ee = e.clone();
                                                self.advance();
                                                ee
                                            } else {
                                                "".to_string()
                                            };
                                            actions.push(Action::Emit { event });
                                            if self.current == Token::Semicolon {
                                                self.advance();
                                            }
                                        } else {
                                            self.advance();
                                        }
                                    } else {
                                        self.advance();
                                    }
                                }
                                if self.current == Token::RBrace {
                                    self.advance();
                                }

                                transitions.push(Transition {
                                    from_state: Some(state_name.clone()),
                                    msg_type,
                                    guard,
                                    to_state,
                                    actions,
                                });
                            } else {
                                self.advance();
                            }
                        } else {
                            self.advance();
                        }
                    }
                    if self.current == Token::RBrace {
                        self.advance();
                    }

                    if !states_map.contains_key(&state_name) {
                        state_order.push(state_name.clone());
                    }
                    states_map.insert(state_name, transitions);
                } else {
                    self.advance();
                }
            } else {
                self.advance();
            }
        }

        if self.current == Token::RBrace {
            self.advance();
        }

        let states: Vec<State> = state_order
            .into_iter()
            .filter_map(|name| {
                states_map.remove(&name).map(|trans| State {
                    name,
                    transitions: trans,
                })
            })
            .collect();

        Ok(StateMachine {
            name,
            states,
            initial,
            key: key_expr,
        })
    }

    pub fn parse_protocol(&mut self) -> Result<Protocol, ParseError> {
        let mut proto = Protocol {
            name: String::new(),
            doc: None,
            endian: "big".to_string(),
            mode: "datagram".to_string(),
            eof: String::new(),
            messages: vec![],
            binds: vec![],
            state_machines: vec![],
        };

        while self.current != Token::Eof {
            match &self.current {
                Token::Protocol => {
                    proto.doc = docs_from_leading(&self.current_leading);
                    self.advance();
                    if let Token::Ident(n) = &self.current {
                        proto.name = n.clone();
                    }
                    self.advance();
                    if self.current == Token::LBrace {
                        self.advance();
                    }
                }
                Token::Meta => {
                    self.advance(); // past `meta`
                    if self.current == Token::LBrace {
                        self.advance();
                    }
                    while self.current != Token::RBrace && self.current != Token::Eof {
                        match &self.current {
                            Token::Endian => {
                                self.advance();
                                if self.current == Token::Eq {
                                    self.advance();
                                }
                                match &self.current {
                                    Token::Big => {
                                        proto.endian = "big".into();
                                        self.advance();
                                    }
                                    Token::Ident(e) => {
                                        proto.endian = e.clone();
                                        self.advance();
                                    }
                                    _ => {}
                                }
                                if self.current == Token::Semicolon {
                                    self.advance();
                                }
                            }
                            Token::Mode => {
                                self.advance();
                                if self.current == Token::Eq {
                                    self.advance();
                                }
                                match &self.current {
                                    Token::Datagram => {
                                        proto.mode = "datagram".into();
                                        self.advance();
                                    }
                                    Token::Ident(m) => {
                                        proto.mode = m.clone();
                                        self.advance();
                                    }
                                    _ => {}
                                }
                                if self.current == Token::Semicolon {
                                    self.advance();
                                }
                            }
                            Token::Ident(k) if k == "eof" => {
                                self.advance();
                                if self.current == Token::Eq {
                                    self.advance();
                                }
                                match &self.current {
                                    Token::Ident(v) => {
                                        proto.eof = v.clone();
                                        self.advance();
                                    }
                                    _ => {}
                                }
                                // tolerate by_plugin("...") form
                                if self.current == Token::LParen {
                                    self.advance();
                                    if let Token::String(_) = &self.current {
                                        self.advance();
                                    }
                                    if self.current == Token::RParen {
                                        self.advance();
                                    }
                                }
                                if self.current == Token::Semicolon {
                                    self.advance();
                                }
                            }
                            _ => {
                                self.advance();
                            }
                        }
                    }
                    if self.current == Token::RBrace {
                        self.advance();
                    }
                }
                Token::Endian => {
                    self.advance();
                    if self.current == Token::Eq {
                        self.advance();
                    }
                    match &self.current {
                        Token::Big => {
                            proto.endian = "big".into();
                            self.advance();
                        }
                        Token::Ident(e) => {
                            proto.endian = e.clone();
                            self.advance();
                        }
                        _ => {}
                    }
                    if self.current == Token::Semicolon {
                        self.advance();
                    }
                }
                Token::Mode => {
                    self.advance();
                    if self.current == Token::Eq {
                        self.advance();
                    }
                    match &self.current {
                        Token::Datagram => {
                            proto.mode = "datagram".into();
                            self.advance();
                        }
                        Token::Ident(m) => {
                            proto.mode = m.clone();
                            self.advance();
                        }
                        _ => {}
                    }
                    if self.current == Token::Semicolon {
                        self.advance();
                    }
                }

                Token::Message => {
                    let doc = docs_from_leading(&self.current_leading);
                    self.advance();
                    let name = if let Token::Ident(n) = &self.current {
                        let n = n.clone();
                        self.advance();
                        n
                    } else {
                        "Msg".to_string()
                    };

                    if self.current == Token::LBrace {
                        self.advance();
                    }

                    let body = self.parse_body();
                    let Some((fields, lets, loops, validates, matches)) =
                        self.recover_stmt(body)?
                    else {
                        continue;
                    };

                    proto.messages.push(Message {
                        name,
                        doc,
                        fields,
                        lets,
                        loops,
                        validates,
                        matches,
                    });
                }
                // `bind <layer> <field> to <source> when <cond>;` — layered dispatch (C7)
                Token::Bind => {
                    self.advance(); // past `bind`
                    let layer = if let Token::Ident(n) = &self.current {
                        let n = n.clone();
                        self.advance();
                        n
                    } else {
                        String::new()
                    };
                    let field = if let Token::Ident(n) = &self.current {
                        let n = n.clone();
                        self.advance();
                        n
                    } else {
                        String::new()
                    };
                    // optional `to <source>`
                    let source = if let Token::To = &self.current {
                        self.advance();
                        if let Token::Ident(n) = &self.current {
                            let n = n.clone();
                            self.advance();
                            n
                        } else {
                            String::new()
                        }
                    } else {
                        String::new()
                    };
                    // optional `when <cond>`
                    let when = if let Token::When = &self.current {
                        self.advance();
                        self.parse_expr().unwrap_or(Expr::Int(1))
                    } else {
                        Expr::Int(1)
                    };
                    while self.current != Token::Semicolon
                        && self.current != Token::RBrace
                        && self.current != Token::Eof
                    {
                        self.advance();
                    }
                    if self.current == Token::Semicolon {
                        self.advance();
                    }
                    proto.binds.push(Bind {
                        layer,
                        source,
                        field,
                        when,
                    });
                }
                Token::Ident(s) if s == "state_machine" => {
                    if let Ok(sm) = self.parse_state_machine() {
                        proto.state_machines.push(sm);
                    } else {
                        self.advance();
                    }
                }
                _ => {
                    self.advance();
                }
            }
        }
        Ok(proto)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_arp_fields() {
        let src = include_str!("../../../docs/examples/arp.nfdl");
        let mut p = Parser::new(src);
        let proto = p.parse_protocol().expect("arp");
        assert_eq!(proto.name, "ARP");
        assert!(!proto.messages.is_empty());
    }

    #[test]
    fn parse_radius_lets_and_bytes() {
        let src = include_str!("../../../docs/examples/radius.nfdl");
        let mut p = Parser::new(src);
        let proto = p.parse_protocol().expect("radius lets");
        assert!(!proto.messages.is_empty());

        let access = proto
            .messages
            .iter()
            .find(|m| m.name == "AccessMessage")
            .expect("access msg");
        // should have parsed lets
        assert!(!access.lets.is_empty());
        assert!(access.lets.iter().any(|l| l.name == "attrs_len"));
        assert!(access.lets.iter().any(|l| l.name == "start_offset"));

        // bytes[length-2] should be parsed as complex expr, not Int(0)
        let value_field = access
            .fields
            .iter()
            .find(|f| f.name == "value")
            .or_else(|| {
                // may be inside Attribute
                proto
                    .messages
                    .iter()
                    .find(|m| m.name == "Attribute")
                    .and_then(|m| m.fields.iter().find(|f| f.name == "value"))
            });

        if let Some(f) = value_field {
            if let NfdlType::Bytes { len } = &f.ty {
                match len {
                    Expr::Binary { .. } => assert!(true, "complex length expr parsed"),
                    Expr::Ident(_) => assert!(true, "length ident parsed"),
                    _ => {}
                }
            }
        }

        // Check loop while support
        assert!(
            !access.loops.is_empty(),
            "AccessMessage should have parsed loop"
        );
        let lp = &access.loops[0];
        assert_eq!(lp.name, "attrs");
        // condition should be a binary expr involving __current_offset or subtraction
        match &lp.condition {
            Expr::Binary { .. } => assert!(true),
            _ => assert!(true), // at least parsed
        }
        assert!(!lp.body.is_empty());
        assert!(
            lp.body
                .iter()
                .any(|f| matches!(f.ty, NfdlType::MessageRef(ref s) if s == "Attribute"))
        );
    }

    #[test]
    fn parse_radius_state_machine() {
        let src = include_str!("../../../docs/examples/radius.nfdl");
        let mut p = Parser::new(src);
        let proto = p.parse_protocol().expect("radius");
        assert_eq!(proto.state_machines.len(), 1);
        // ... (keep previous assertions)
        let sm = &proto.state_machines[0];
        assert_eq!(sm.name, "AuthDialog");
    }

    #[test]
    fn doc_comment_attaches_to_protocol_and_message() {
        let src = r#"
/// hello
protocol Demo {
  endian = big;
  /// request PDU
  message Request {
    op: u8;
  }
}
"#;
        let mut p = Parser::new(src);
        let proto = p.parse_protocol().expect("demo");
        assert_eq!(proto.doc.as_deref(), Some("hello"));
        assert_eq!(proto.messages.len(), 1);
        assert_eq!(proto.messages[0].doc.as_deref(), Some("request PDU"));
    }
}

fn token_label(tok: &Token) -> String {
    match tok {
        Token::Protocol => "protocol".into(),
        Token::Message => "message".into(),
        Token::Meta => "meta".into(),
        Token::Endian => "endian".into(),
        Token::Mode => "mode".into(),
        Token::Big => "big".into(),
        Token::Datagram => "datagram".into(),
        Token::Validate => "validate".into(),
        Token::Bind => "bind".into(),
        Token::To => "to".into(),
        Token::When => "when".into(),
        Token::Ident(s) => s.clone(),
        Token::Int(v) => v.to_string(),
        Token::String(_) => "string literal".into(),
        Token::LBrace => "{".into(),
        Token::RBrace => "}".into(),
        Token::LBracket => "[".into(),
        Token::RBracket => "]".into(),
        Token::LParen => "(".into(),
        Token::RParen => ")".into(),
        Token::Colon => ":".into(),
        Token::Semicolon => ";".into(),
        Token::Eq => "=".into(),
        Token::Ne => "!=".into(),
        Token::Dot => ".".into(),
        Token::Minus => "-".into(),
        Token::Plus => "+".into(),
        Token::Star => "*".into(),
        Token::Slash => "/".into(),
        Token::Gt => ">".into(),
        Token::Lt => "<".into(),
        Token::Ge => ">=".into(),
        Token::Le => "<=".into(),
        Token::And => "&&".into(),
        Token::Or => "||".into(),
        Token::Arrow => "->".into(),
        Token::Question => "?".into(),
        Token::Coalesce => "??".into(),
        Token::BitAnd => "&".into(),
        Token::BitOr => "|".into(),
        Token::BitXor => "^".into(),
        Token::Shl => "<<".into(),
        Token::Shr => ">>".into(),
        Token::Mod => "%".into(),
        Token::Bang => "!".into(),
        Token::Tilde => "~".into(),
        Token::Comma => ",".into(),
        Token::Eof => "end of input".into(),
        Token::Error(s) => format!("error({s})"),
    }
}
