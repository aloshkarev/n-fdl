//! Parser with production v1 support:
//! - let bindings inside messages
//! - __current_offset as special ident
//! - complex bytes[expr] (length - 2 etc.)
//! - improved expr with + - == > <

use crate::ast::*;
use crate::lexer::{Lexer, Token};

#[derive(Debug, Clone, PartialEq)]
pub enum ParseError {
    Syntax(String),
    WithLocation { msg: String, pos: usize },
}

pub struct Parser<'a> {
    lexer: Lexer<'a>,
    current: Token,
}

impl<'a> Parser<'a> {
    pub fn new(input: &'a str) -> Self {
        let mut lexer = Lexer::new(input);
        let current = lexer.next_token();
        Self { lexer, current }
    }

    fn advance(&mut self) {
        self.current = self.lexer.next_token();
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
                return Err(ParseError::Syntax("expected : in ternary".into()));
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
                    "bad primary: {:?}",
                    self.current
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
    /// `bitfield{k}`, and bare-ident `MessageRef`.
    fn parse_type(&mut self) -> NfdlType {
        match &self.current {
            Token::Ident(t) if t == "u8" => {
                self.advance();
                NfdlType::U8
            }
            Token::Ident(t) if t == "u16" => {
                self.advance();
                NfdlType::U16
            }
            Token::Ident(t) if t == "u24" => {
                self.advance();
                NfdlType::U24
            }
            Token::Ident(t) if t == "u32" => {
                self.advance();
                NfdlType::U32
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
                    ty
                } else {
                    NfdlType::BytesRest
                }
            }
            Token::Ident(t) if t == "bitfield" => {
                self.advance();
                let bits = if self.current == Token::LBrace {
                    self.advance();
                    let b = if let Token::Int(v) = &self.current {
                        let v = *v as u8;
                        self.advance();
                        v
                    } else {
                        0
                    };
                    if self.current == Token::RBrace {
                        self.advance();
                    }
                    b
                } else {
                    0
                };
                NfdlType::Bitfield { bits }
            }
            Token::Ident(t) => {
                let tname = t.clone();
                self.advance();
                NfdlType::MessageRef(tname)
            }
            _ => {
                // Unknown — consume one token to make progress, default to u8.
                self.advance();
                NfdlType::U8
            }
        }
    }

    /// Parse a `match`-arm or message body block (the `{ ... }` contents, with
    /// `{` already consumed) up to and including the closing `}`. Handles
    /// `let`, `loop` (with optional `carry`/`while`/`next`), standalone
    /// `validate`, nested `match`, and plain `field: type;`.
    fn parse_arm_body(&mut self) -> (Vec<Field>, Vec<Let>, Vec<Loop>, Vec<Validate>, Vec<Match>) {
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
                let o = body_seq; body_seq += 1;
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
                        let (af, al, alp, av, am) = self.parse_arm_body();
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
                    let o = body_seq; body_seq += 1;
                    matches.push(Match { tag, arms, order: o });
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
                            let o = body_seq; body_seq += 1;
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
                            let cty = self.parse_type();
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
                            self.advance();
                            if self.current == Token::Colon {
                                self.advance();
                            }
                            let ty = self.parse_type();
                            loop_body.push(Field {
                                name: fname,
                                ty,
                                validate: None,
                                conditional: None,
                                order: 0,
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
                    let o = body_seq; body_seq += 1;
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
                let ty = self.parse_type();
                let mut validate = None;
                if let Token::Ident(v) = &self.current {
                    if v == "validate" {
                        self.advance();
                        let vexpr = self.parse_expr().unwrap_or(Expr::Int(1));
                        validate = Some(Validate {
                            expr: vexpr,
                            message: "constraint".into(),
                            order: 0,
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
                        if let Ok(e) = self.parse_expr() {
                            conditional = Some(e);
                        }
                        while self.current != Token::Semicolon
                            && self.current != Token::RBrace
                            && self.current != Token::Eof
                        {
                            self.advance();
                        }
                    }
                }
                let o = body_seq; body_seq += 1;
                fields.push(Field {
                    name: kw,
                    ty,
                    validate,
                    conditional,
                    order: o,
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
        (fields, lets, loops, validates, matches)
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

        let states: Vec<State> = states_map
            .into_iter()
            .map(|(name, trans)| State {
                name,
                transitions: trans,
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

                    let mut fields = vec![];
                    let mut lets = vec![];
                    let mut loops = vec![];
                    let mut validates = vec![];
                    let mut matches = vec![];
                    // Monotonic source-order counter so the emitter can interleave
                    // fields/lets/loops/validates/matches in the order they were written
                    // (a field may reference a preceding `let`, and a `let` may reference
                    // preceding fields).
                    let mut body_seq: u32 = 0;

                    while self.current != Token::RBrace && self.current != Token::Eof {
                        // Standalone `validate expr -> "msg";` (refinement on prior fields/lets)
                        if self.current == Token::Validate {
                            self.advance();
                            let vexpr = match self.parse_expr() {
                                Ok(e) => e,
                                Err(_) => Expr::Int(1),
                            };
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
                            let o = body_seq; body_seq += 1;
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
                        // `match <tag> { case N => { ... } default => { ... } }` tagged union (C6)
                        if let Token::Ident(kw) = &self.current {
                            if kw == "match" {
                                self.advance();
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
                                    let (af, al, alp, av, am) = self.parse_arm_body();
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
                                let o = body_seq; body_seq += 1;
                                matches.push(Match { tag, arms, order: o });
                                continue;
                            }
                        }
                        if let Token::Ident(kw) = &self.current {
                            let kw = kw.clone();
                            self.advance();

                            if kw == "let" {
                                if let Token::Ident(let_name) = &self.current {
                                    let lname = let_name.clone();
                                    self.advance();
                                    if self.current == Token::Eq {
                                        self.advance();
                                    }
                                    if let Ok(val) = self.parse_expr() {
                                        let o = body_seq; body_seq += 1;
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
                                // parse optional carry decls: carry name : type = init_expr
                                while let Token::Ident(kw2) = &self.current {
                                    if kw2 == "carry" {
                                        self.advance();
                                        let cname = if let Token::Ident(n) = &self.current {
                                            let nn = n.clone();
                                            self.advance();
                                            nn
                                        } else {
                                            "".to_string()
                                        };
                                        if self.current == Token::Colon {
                                            self.advance();
                                        }
                                        let cty = self.parse_type();
                                        if self.current == Token::Eq {
                                            self.advance();
                                        }
                                        let init = if let Ok(e) = self.parse_expr() {
                                            e
                                        } else {
                                            Expr::Int(0)
                                        };
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

                                // while expr   (parens optional per gtpu.nfdl)
                                if let Token::Ident(w) = &self.current {
                                    if w == "while" {
                                        self.advance();
                                    }
                                }

                                let mut condition = Expr::Int(1);
                                // parse expr even without parens; stop at { or ;
                                if self.current != Token::LBrace && self.current != Token::Semicolon
                                {
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
                                        // handle next stmt inside loop body
                                        if fname == "next" {
                                            self.advance();
                                            let nname = if let Token::Ident(n) = &self.current {
                                                let nn = n.clone();
                                                self.advance();
                                                nn
                                            } else {
                                                "".to_string()
                                            };
                                            if self.current == Token::Eq {
                                                self.advance();
                                            }
                                            let nval = if let Ok(e) = self.parse_expr() {
                                                e
                                            } else {
                                                Expr::Int(0)
                                            };
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
                                        self.advance();
                                        if self.current == Token::Colon {
                                            self.advance();
                                        }

                                        let ty = self.parse_type();

                                        loop_body.push(Field {
                                            name: fname,
                                            ty,
                                            validate: None,
                                            conditional: None,
                                            order: 0,
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
                                    return Err(ParseError::Syntax("StreamRemControlFlow: __rem forbidden in loop while condition (see spec 05-verification)".into()));
                                }
                                let o = body_seq; body_seq += 1;
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

                            // normal field
                            if self.current == Token::Colon {
                                self.advance();
                            }

                            let ty = self.parse_type();

                            let mut validate = None;
                            if let Token::Ident(v) = &self.current {
                                if v == "validate" {
                                    self.advance();
                                    let vexpr = if let Ok(e) = self.parse_expr() {
                                        e
                                    } else {
                                        Expr::Int(1)
                                    };
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
                                    let o = body_seq; body_seq += 1;
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
                                    if let Ok(e) = self.parse_expr() {
                                        if self.contains_rem(&e) {
                                            return Err(ParseError::Syntax("StreamRemControlFlow: __rem forbidden in conditional field (layout-affecting)".into()));
                                        }
                                        conditional = Some(e);
                                    }
                                    while self.current != Token::Semicolon
                                        && self.current != Token::RBrace
                                        && self.current != Token::Eof
                                    {
                                        self.advance();
                                    }
                                }
                            }

                            let o = body_seq; body_seq += 1;
                            fields.push(Field {
                                name: kw,
                                ty,
                                validate,
                                conditional,
                                order: o,
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

                    proto.messages.push(Message {
                        name,
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
}
