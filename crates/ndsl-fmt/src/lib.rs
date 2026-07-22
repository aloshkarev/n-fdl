//! Opinionated formatter for N-FDL and ADGL.
//!
//! Both tracks are AST pretty-printers (N-FDL: Task 13; ADGL: Task 14).

#![forbid(unsafe_code)]

mod adgl;

use ndsl_diag::DiagBuffer;
use ndsl_trivia::TriviaKind;
use nfdl_syntax::ast::{
    Action, BinOp, Bind, Expr, Field, Let, Loop, Match, MatchArm, Message, NextStmt, NfdlType,
    Protocol, State, StateMachine, Transition, UnaryOp, Validate,
};
use nfdl_syntax::{Lexer, ParseError, Parser, Token};

pub use adgl::{format_adgl_source, format_adgl_source_with};

/// Formatter configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormatOptions {
    /// Spaces per indentation level.
    pub indent: usize,
}

impl Default for FormatOptions {
    fn default() -> Self {
        Self { indent: 4 }
    }
}

/// Format failure from the underlying syntax crate.
#[derive(Debug)]
pub enum FormatError {
    Nfdl(ParseError),
    Adgl(DiagBuffer),
}

impl std::fmt::Display for FormatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Nfdl(err) => write!(f, "{err:?}"),
            Self::Adgl(buf) => write!(f, "ADGL parse failed ({} diagnostic(s))", buf.len()),
        }
    }
}

impl std::error::Error for FormatError {}

/// Parse N-FDL source and pretty-print with default [`FormatOptions`].
pub fn format_nfdl_source(src: &str) -> Result<String, FormatError> {
    format_nfdl_source_with(src, &FormatOptions::default())
}

/// Parse N-FDL source and pretty-print with `opts`.
pub fn format_nfdl_source_with(src: &str, opts: &FormatOptions) -> Result<String, FormatError> {
    let comments = collect_comments(src);
    let spans = protocol_spans(src);
    if spans.is_empty() {
        // Surface a parse error for invalid / empty input (matches prior stub).
        Parser::new(src)
            .parse_protocol()
            .map_err(FormatError::Nfdl)?;
        let mut out = String::new();
        emit_comments(&mut out, &comments);
        return Ok(out);
    }

    let mut protocols = Vec::with_capacity(spans.len());
    for span in &spans {
        let chunk = &src[span.clone()];
        let proto = Parser::new(chunk)
            .parse_protocol()
            .map_err(FormatError::Nfdl)?;
        protocols.push(proto);
    }

    let mut out = String::new();
    emit_comments(&mut out, &comments);
    for (i, proto) in protocols.iter().enumerate() {
        if i > 0 || !comments.is_empty() {
            if !out.is_empty() && !out.ends_with('\n') {
                out.push('\n');
            }
            if i > 0 {
                out.push('\n');
            }
        }
        emit_protocol(&mut out, proto, opts);
    }
    if !out.ends_with('\n') {
        out.push('\n');
    }
    Ok(out)
}

fn collect_comments(src: &str) -> Vec<String> {
    let mut lexer = Lexer::new(src);
    let mut comments = Vec::new();
    loop {
        let tok = lexer.next_token();
        for t in lexer.trivia_before_next_token() {
            if matches!(
                t.kind,
                TriviaKind::LineComment | TriviaKind::DocComment | TriviaKind::BlockComment
            ) {
                comments.push(t.text);
            }
        }
        if tok == Token::Eof {
            break;
        }
    }
    comments
}

fn emit_comments(out: &mut String, comments: &[String]) {
    for c in comments {
        out.push_str(c);
        out.push('\n');
    }
}

/// Byte ranges of each top-level `protocol { ... }` (brace-depth aware).
fn protocol_spans(src: &str) -> Vec<std::ops::Range<usize>> {
    let mut lexer = Lexer::new(src);
    let mut spans = Vec::new();
    let mut start: Option<usize> = None;
    let mut depth: i32 = 0;
    let mut in_protocol = false;

    loop {
        let tok = lexer.next_token();
        let span = lexer.last_span();
        match tok {
            Token::Protocol if !in_protocol => {
                start = Some(span.start);
                in_protocol = true;
                depth = 0;
            }
            Token::LBrace if in_protocol => {
                depth += 1;
            }
            Token::RBrace if in_protocol => {
                depth -= 1;
                if depth == 0 {
                    if let Some(s) = start.take() {
                        spans.push(s..span.end);
                    }
                    in_protocol = false;
                }
            }
            Token::Eof => break,
            _ => {}
        }
    }
    spans
}

fn emit_protocol(out: &mut String, proto: &Protocol, opts: &FormatOptions) {
    out.push_str("protocol ");
    out.push_str(&proto.name);
    out.push_str(" {\n");

    emit_meta(out, proto, opts, 1);

    for msg in &proto.messages {
        out.push('\n');
        emit_message(out, msg, opts, 1);
    }

    for bind in &proto.binds {
        out.push('\n');
        emit_bind(out, bind, opts, 1);
    }

    for sm in &proto.state_machines {
        out.push('\n');
        emit_state_machine(out, sm, opts, 1);
    }

    out.push('}');
    out.push('\n');
}

fn indent_str(opts: &FormatOptions, level: usize) -> String {
    " ".repeat(opts.indent.saturating_mul(level))
}

fn emit_meta(out: &mut String, proto: &Protocol, opts: &FormatOptions, level: usize) {
    let pad = indent_str(opts, level);
    let inner = indent_str(opts, level + 1);
    out.push_str(&pad);
    out.push_str("meta {\n");
    out.push_str(&inner);
    out.push_str("endian = ");
    out.push_str(&proto.endian);
    out.push_str(";\n");
    out.push_str(&inner);
    out.push_str("mode = ");
    out.push_str(&proto.mode);
    out.push_str(";\n");
    if !proto.eof.is_empty() {
        out.push_str(&inner);
        out.push_str("eof = ");
        out.push_str(&proto.eof);
        out.push_str(";\n");
    }
    out.push_str(&pad);
    out.push_str("}\n");
}

fn emit_message(out: &mut String, msg: &Message, opts: &FormatOptions, level: usize) {
    let pad = indent_str(opts, level);
    out.push_str(&pad);
    out.push_str("message ");
    out.push_str(&msg.name);
    out.push_str(" {\n");
    emit_body(
        out,
        &msg.fields,
        &msg.lets,
        &msg.loops,
        &msg.validates,
        &msg.matches,
        opts,
        level + 1,
    );
    out.push_str(&pad);
    out.push_str("}\n");
}

#[allow(clippy::too_many_arguments)]
fn emit_body(
    out: &mut String,
    fields: &[Field],
    lets: &[Let],
    loops: &[Loop],
    validates: &[Validate],
    matches: &[Match],
    opts: &FormatOptions,
    level: usize,
) {
    #[derive(Clone, Copy)]
    enum Item<'a> {
        Field(&'a Field),
        Let(&'a Let),
        Loop(&'a Loop),
        Validate(&'a Validate),
        Match(&'a Match),
    }

    let mut items: Vec<(u32, Item<'_>)> = Vec::new();
    for f in fields {
        items.push((f.order, Item::Field(f)));
    }
    for l in lets {
        items.push((l.order, Item::Let(l)));
    }
    for lp in loops {
        items.push((lp.order, Item::Loop(lp)));
    }
    for v in validates {
        items.push((v.order, Item::Validate(v)));
    }
    for m in matches {
        items.push((m.order, Item::Match(m)));
    }
    items.sort_by_key(|(o, _)| *o);

    for (_, item) in items {
        match item {
            Item::Field(f) => emit_field(out, f, opts, level),
            Item::Let(l) => emit_let(out, l, opts, level),
            Item::Loop(lp) => emit_loop(out, lp, opts, level),
            Item::Validate(v) => emit_validate(out, v, opts, level),
            Item::Match(m) => emit_match(out, m, opts, level),
        }
    }
}

fn emit_field(out: &mut String, field: &Field, opts: &FormatOptions, level: usize) {
    let pad = indent_str(opts, level);
    out.push_str(&pad);
    out.push_str(&field.name);
    out.push_str(": ");
    emit_type(out, &field.ty);
    if let Some(v) = &field.validate {
        out.push_str(" validate ");
        emit_expr(out, &v.expr, 0);
        out.push_str(" -> ");
        emit_string_lit(out, &v.message);
    }
    if let Some(cond) = &field.conditional {
        out.push_str(" if ");
        emit_expr(out, cond, 0);
    }
    out.push_str(";\n");
}

fn emit_let(out: &mut String, let_: &Let, opts: &FormatOptions, level: usize) {
    let pad = indent_str(opts, level);
    out.push_str(&pad);
    out.push_str("let ");
    out.push_str(&let_.name);
    out.push_str(" = ");
    emit_expr(out, &let_.value, 0);
    out.push_str(";\n");
}

fn emit_validate(out: &mut String, v: &Validate, opts: &FormatOptions, level: usize) {
    let pad = indent_str(opts, level);
    out.push_str(&pad);
    out.push_str("validate ");
    emit_expr(out, &v.expr, 0);
    out.push_str(" -> ");
    emit_string_lit(out, &v.message);
    out.push_str(";\n");
}

fn emit_loop(out: &mut String, lp: &Loop, opts: &FormatOptions, level: usize) {
    let pad = indent_str(opts, level);
    let inner = indent_str(opts, level + 1);
    out.push_str(&pad);
    out.push_str("loop ");
    out.push_str(&lp.name);
    out.push('\n');
    for c in &lp.carries {
        out.push_str(&inner);
        out.push_str("carry ");
        out.push_str(&c.name);
        out.push_str(": ");
        emit_type(out, &c.ty);
        out.push_str(" = ");
        emit_expr(out, &c.init, 0);
        out.push('\n');
    }
    out.push_str(&inner);
    out.push_str("while ");
    emit_expr(out, &lp.condition, 0);
    out.push('\n');
    out.push_str(&pad);
    out.push_str("{\n");
    for f in &lp.body {
        emit_field(out, f, opts, level + 1);
    }
    for n in &lp.nexts {
        emit_next(out, n, opts, level + 1);
    }
    out.push_str(&pad);
    out.push_str("}\n");
}

fn emit_next(out: &mut String, n: &NextStmt, opts: &FormatOptions, level: usize) {
    let pad = indent_str(opts, level);
    out.push_str(&pad);
    out.push_str("next ");
    out.push_str(&n.name);
    out.push_str(" = ");
    emit_expr(out, &n.value, 0);
    out.push_str(";\n");
}

fn emit_match(out: &mut String, m: &Match, opts: &FormatOptions, level: usize) {
    let pad = indent_str(opts, level);
    out.push_str(&pad);
    out.push_str("match ");
    emit_expr(out, &m.tag, 0);
    out.push_str(" {\n");
    for (i, arm) in m.arms.iter().enumerate() {
        emit_match_arm(out, arm, opts, level + 1);
        if i + 1 < m.arms.len() {
            // no trailing comma required by parser; keep clean
        }
    }
    out.push_str(&pad);
    out.push_str("}\n");
}

fn emit_match_arm(out: &mut String, arm: &MatchArm, opts: &FormatOptions, level: usize) {
    let pad = indent_str(opts, level);
    out.push_str(&pad);
    match arm.case {
        Some(v) => {
            out.push_str("case ");
            out.push_str(&v.to_string());
            out.push_str(" => {\n");
        }
        None => out.push_str("default => {\n"),
    }
    emit_body(
        out,
        &arm.fields,
        &arm.lets,
        &arm.loops,
        &arm.validates,
        &arm.matches,
        opts,
        level + 1,
    );
    out.push_str(&pad);
    out.push_str("}\n");
}

fn emit_bind(out: &mut String, bind: &Bind, opts: &FormatOptions, level: usize) {
    let pad = indent_str(opts, level);
    out.push_str(&pad);
    out.push_str("bind ");
    out.push_str(&bind.layer);
    out.push(' ');
    out.push_str(&bind.field);
    out.push_str(" to ");
    out.push_str(&bind.source);
    out.push_str(" when ");
    emit_expr(out, &bind.when, 0);
    out.push_str(";\n");
}

fn emit_state_machine(out: &mut String, sm: &StateMachine, opts: &FormatOptions, level: usize) {
    let pad = indent_str(opts, level);
    let inner = indent_str(opts, level + 1);
    out.push_str(&pad);
    out.push_str("state_machine ");
    out.push_str(&sm.name);
    out.push_str(" {\n");
    if let Some(key) = &sm.key {
        out.push_str(&inner);
        out.push_str("key = ");
        emit_expr(out, key, 0);
        out.push_str(";\n");
        out.push('\n');
    }
    for state in &sm.states {
        emit_state(out, state, opts, level + 1);
    }
    out.push_str(&pad);
    out.push_str("}\n");
}

fn emit_state(out: &mut String, state: &State, opts: &FormatOptions, level: usize) {
    let pad = indent_str(opts, level);
    out.push_str(&pad);
    out.push_str("state ");
    out.push_str(&state.name);
    out.push_str(" {\n");
    for t in &state.transitions {
        emit_transition(out, t, opts, level + 1);
    }
    out.push_str(&pad);
    out.push_str("}\n");
}

fn emit_transition(out: &mut String, t: &Transition, opts: &FormatOptions, level: usize) {
    let pad = indent_str(opts, level);
    let inner = indent_str(opts, level + 1);
    out.push_str(&pad);
    out.push_str("on ");
    out.push_str(&t.msg_type);
    if let Some(g) = &t.guard {
        out.push_str(" guard (");
        emit_expr(out, g, 0);
        out.push(')');
    }
    out.push_str(" -> ");
    out.push_str(&t.to_state);
    if t.actions.is_empty() {
        out.push_str(";\n");
        return;
    }
    out.push_str(" {\n");
    for a in &t.actions {
        out.push_str(&inner);
        match a {
            Action::Emit { event } => {
                out.push_str("emit ");
                out.push_str(event);
                out.push_str(";\n");
            }
            Action::Set { var, value } => {
                out.push_str("set ");
                out.push_str(var);
                out.push_str(" = ");
                emit_expr(out, value, 0);
                out.push_str(";\n");
            }
        }
    }
    out.push_str(&pad);
    out.push_str("};\n");
}

fn emit_type(out: &mut String, ty: &NfdlType) {
    match ty {
        NfdlType::U8 => out.push_str("u8"),
        NfdlType::U16 => out.push_str("u16"),
        NfdlType::U24 => out.push_str("u24"),
        NfdlType::U32 => out.push_str("u32"),
        NfdlType::Bytes { len } => {
            out.push_str("bytes[");
            emit_expr(out, len, 0);
            out.push(']');
        }
        NfdlType::BytesRest => out.push_str("bytes[..]"),
        NfdlType::BytesEof => out.push_str("bytes[EOF]"),
        NfdlType::BytesStream => out.push_str("bytes[stream]"),
        NfdlType::Bitfield { bits } => {
            out.push_str("bitfield{");
            out.push_str(&bits.to_string());
            out.push('}');
        }
        NfdlType::MessageRef(name) => out.push_str(name),
    }
}

fn emit_string_lit(out: &mut String, s: &str) {
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Precedence matching `nfdl_syntax` parse ladder (higher = tighter).
fn expr_prec(expr: &Expr) -> u8 {
    match expr {
        Expr::Ident(_) | Expr::Int(_) | Expr::Str(_) | Expr::Call { .. } | Expr::Tuple(_) => 13,
        Expr::Field(_, _) => 13,
        Expr::Unary { .. } => 12,
        Expr::Binary { op, .. } => binop_prec(op),
        Expr::Coalesce { .. } => 2,
        Expr::Ternary { .. } => 1,
    }
}

fn binop_prec(op: &BinOp) -> u8 {
    match op {
        BinOp::Or => 3,
        BinOp::And => 4,
        BinOp::BitOr => 5,
        BinOp::BitXor => 6,
        BinOp::BitAnd => 7,
        BinOp::Eq | BinOp::Ne => 8,
        BinOp::Gt | BinOp::Lt | BinOp::Ge | BinOp::Le => 9,
        BinOp::Shl | BinOp::Shr => 10,
        BinOp::Add | BinOp::Sub => 11,
        BinOp::Mul | BinOp::Div | BinOp::Mod => 12,
    }
}

fn emit_expr(out: &mut String, expr: &Expr, parent_prec: u8) {
    let prec = expr_prec(expr);
    let wrap = prec < parent_prec;
    if wrap {
        out.push('(');
    }
    match expr {
        Expr::Ident(s) => out.push_str(s),
        Expr::Int(v) => out.push_str(&v.to_string()),
        Expr::Str(s) => emit_string_lit(out, s),
        Expr::Unary { op, expr } => {
            out.push_str(unary_str(op));
            // Unary and Mul/Div/Mod share prec 12; demand >12 so `!(a*b)` keeps parens
            // (same fix as ADGL `emit_expr` for Unary).
            emit_expr(out, expr, 13);
        }
        Expr::Binary { op, left, right } => {
            let p = binop_prec(op);
            emit_expr(out, left, p);
            out.push(' ');
            out.push_str(binop_str(op));
            out.push(' ');
            // Left-assoc: right side needs paren on equal precedence.
            emit_expr(out, right, p + 1);
        }
        Expr::Ternary {
            cond,
            then_branch,
            else_branch,
        } => {
            emit_expr(out, cond, 2);
            out.push_str(" ? ");
            emit_expr(out, then_branch, 1);
            out.push_str(" : ");
            emit_expr(out, else_branch, 1);
        }
        Expr::Coalesce { value, default } => {
            emit_expr(out, value, 3);
            out.push_str(" ?? ");
            emit_expr(out, default, 2);
        }
        Expr::Call { name, args } => {
            out.push_str(name);
            out.push('(');
            for (i, a) in args.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                emit_expr(out, a, 0);
            }
            out.push(')');
        }
        Expr::Tuple(elems) => {
            out.push('(');
            for (i, e) in elems.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                emit_expr(out, e, 0);
            }
            out.push(')');
        }
        Expr::Field(base, field) => {
            emit_expr(out, base, 13);
            out.push('.');
            out.push_str(field);
        }
    }
    if wrap {
        out.push(')');
    }
}

fn binop_str(op: &BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Mod => "%",
        BinOp::Eq => "==",
        BinOp::Ne => "!=",
        BinOp::Lt => "<",
        BinOp::Le => "<=",
        BinOp::Gt => ">",
        BinOp::Ge => ">=",
        BinOp::And => "&&",
        BinOp::Or => "||",
        BinOp::BitAnd => "&",
        BinOp::BitOr => "|",
        BinOp::BitXor => "^",
        BinOp::Shl => "<<",
        BinOp::Shr => ">>",
    }
}

fn unary_str(op: &UnaryOp) -> &'static str {
    match op {
        UnaryOp::Not => "!",
        UnaryOp::BitNot => "~",
        UnaryOp::Neg => "-",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    const MINIMAL_NFDL: &str = r#"protocol P {
    meta {
        endian = big;
        mode = datagram;
    }

    message M {
        data: bytes[4];
    }
}
"#;

    const MINIMAL_ADGL: &str = r#"ruleset "x" {
    version = "1.0"

    evidence r {
        scope: Session
        anchor a: event(tcp.retransmission_burst)
    }
}
"#;

    #[test]
    fn format_options_default_indent_is_four() {
        assert_eq!(FormatOptions::default().indent, 4);
    }

    #[test]
    fn format_nfdl_source_pretty_prints_minimal() {
        let messy = "protocol P{meta{endian=big;mode=datagram;}message M{data:bytes[4];}}";
        let out = format_nfdl_source(messy).expect("valid N-FDL must parse");
        assert_eq!(out, MINIMAL_NFDL);
    }

    #[test]
    fn format_nfdl_source_already_canonical_is_stable() {
        let out = format_nfdl_source(MINIMAL_NFDL).expect("valid N-FDL must parse");
        assert_eq!(out, MINIMAL_NFDL);
    }

    #[test]
    fn format_nfdl_source_invalid_returns_err() {
        let err = format_nfdl_source("protocol P { message M { x: u8 if __rem; } }")
            .expect_err("__rem in conditional field must fail");
        assert!(matches!(err, FormatError::Nfdl(_)));
    }

    #[test]
    fn format_nfdl_honors_indent_option() {
        let src = "protocol P{meta{endian=big;mode=datagram;}message M{data:u8;}}";
        let out = format_nfdl_source_with(src, &FormatOptions { indent: 2 }).unwrap();
        assert!(out.contains("\n  meta {\n"));
        assert!(out.contains("\n    endian = big;\n"));
        assert!(out.contains("\n  message M {\n"));
        assert!(out.contains("\n    data: u8;\n"));
    }

    #[test]
    fn format_adgl_source_pretty_prints_minimal() {
        let messy = r#"ruleset "x"{version="1.0"evidence r{scope:Session anchor a:event(tcp.retransmission_burst)}}"#;
        let out = format_adgl_source(messy).expect("valid ADGL must parse");
        assert_eq!(out, MINIMAL_ADGL);
    }

    #[test]
    fn format_adgl_source_already_canonical_is_stable() {
        let out = format_adgl_source(MINIMAL_ADGL).expect("valid ADGL must parse");
        assert_eq!(out, MINIMAL_ADGL);
    }

    #[test]
    fn format_adgl_source_invalid_returns_err() {
        let err = format_adgl_source("ruleset \"x\" {").expect_err("broken ADGL must fail");
        assert!(matches!(err, FormatError::Adgl(_)));
    }

    #[test]
    fn format_adgl_honors_indent_option() {
        let src = r#"ruleset "x"{version="1.0"evidence r{scope:Session anchor a:event(tcp.retransmission_burst)}}"#;
        let out = format_adgl_source_with(src, &FormatOptions { indent: 2 }).unwrap();
        assert!(out.contains("\n  version = \"1.0\"\n"));
        assert!(out.contains("\n  evidence r {\n"));
        assert!(out.contains("\n    scope: Session\n"));
    }

    #[test]
    fn format_nfdl_keeps_parens_for_unary_over_mul() {
        // Unary and Mul share prec 12; without raising unary-child parent prec,
        // `!(a * b)` prints as `!a * b` and re-parses as `(!a) * b`.
        let src = r#"
protocol P {
    meta {
        endian = big;
        mode = datagram;
    }

    message M {
        a: u8;
        b: u8;
        validate !(a * b);
    }
}
"#;
        let out = format_nfdl_source(src).expect("valid N-FDL must parse");
        assert!(
            out.contains("!(a * b)"),
            "unary-over-mul must keep parens; got:\n{out}"
        );
        assert!(
            !out.contains("!a * b"),
            "must not drop parens into `!a * b`; got:\n{out}"
        );
        let twice = format_nfdl_source(&out).expect("re-format");
        assert_eq!(out, twice);
    }

    #[test]
    fn format_adgl_keeps_parens_for_unary_over_mul() {
        // Unary and Mul share prec 12; without raising unary-child parent prec,
        // `!(a.x * a.y)` prints as `!a.x * a.y` and re-parses as `(!a.x) * a.y`.
        let src = r#"
ruleset "x" {
    version = "1.0"
    evidence r {
        scope: Session
        anchor a: event(tcp.retransmission_burst) {
            !(a.x * a.y)
        }
    }
}
"#;
        let out = format_adgl_source(src).expect("valid ADGL must parse");
        assert!(
            out.contains("!(a.x * a.y)"),
            "unary-over-mul must keep parens; got:\n{out}"
        );
        assert!(
            !out.contains("!a.x * a.y"),
            "must not drop parens into `!a.x * a.y`; got:\n{out}"
        );
        let twice = format_adgl_source(&out).expect("re-format");
        assert_eq!(out, twice);
    }

    #[test]
    fn format_nfdl_examples_are_idempotent() {
        let examples_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/examples");
        let mut saw = 0usize;
        for entry in std::fs::read_dir(&examples_dir).expect("docs/examples") {
            let entry = entry.expect("dir entry");
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("nfdl") {
                continue;
            }
            saw += 1;
            let src = std::fs::read_to_string(&path).expect("read example");
            let once = format_nfdl_source(&src)
                .unwrap_or_else(|e| panic!("format {} failed: {e}", path.display()));
            let twice = format_nfdl_source(&once)
                .unwrap_or_else(|e| panic!("re-format {} failed: {e}", path.display()));
            assert_eq!(once, twice, "idempotence failed for {}", path.display());
        }
        assert!(saw >= 5, "expected several .nfdl examples, saw {saw}");
    }

    #[test]
    fn format_adgl_idea_examples_are_idempotent() {
        let examples_dir =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/idea/examples");
        let mut saw = 0usize;
        for entry in std::fs::read_dir(&examples_dir).expect("docs/idea/examples") {
            let entry = entry.expect("dir entry");
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("adgl") {
                continue;
            }
            saw += 1;
            let src = std::fs::read_to_string(&path).expect("read example");
            let once = format_adgl_source(&src)
                .unwrap_or_else(|e| panic!("format {} failed: {e}", path.display()));
            let twice = format_adgl_source(&once)
                .unwrap_or_else(|e| panic!("re-format {} failed: {e}", path.display()));
            assert_eq!(once, twice, "idempotence failed for {}", path.display());
        }
        assert!(saw >= 10, "expected all idea .adgl examples, saw {saw}");
    }

    #[test]
    fn format_adgl_parent_diagnostics_idempotent_when_reachable() {
        // Optional: parent AirPulse data/diagnostics/*.adgl (four levels up from crate).
        let diagnostics_dir =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../data/diagnostics");
        let Ok(entries) = std::fs::read_dir(&diagnostics_dir) else {
            return;
        };
        let mut saw = 0usize;
        for entry in entries {
            let entry = entry.expect("dir entry");
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("adgl") {
                continue;
            }
            let src = match std::fs::read_to_string(&path) {
                Ok(s) => s,
                Err(_) => continue,
            };
            let Ok(once) = format_adgl_source(&src) else {
                // Parent pack may include files outside current parser surface.
                continue;
            };
            saw += 1;
            let twice = format_adgl_source(&once)
                .unwrap_or_else(|e| panic!("re-format {} failed: {e}", path.display()));
            assert_eq!(once, twice, "idempotence failed for {}", path.display());
        }
        // Soft check: if the directory exists we expect at least one parseable file.
        if diagnostics_dir.is_dir() {
            assert!(
                saw >= 1,
                "expected at least one parseable parent .adgl under {}",
                diagnostics_dir.display()
            );
        }
    }

    #[test]
    fn format_preserves_multi_protocol_file() {
        let src = r#"
protocol A {
    meta { endian = big; mode = datagram; }
    message M { x: u8; }
}
protocol B {
    meta { endian = little; mode = stream; }
    message N { y: u16; }
}
"#;
        let out = format_nfdl_source(src).unwrap();
        assert!(out.contains("protocol A {"));
        assert!(out.contains("protocol B {"));
        assert!(out.contains("endian = little"));
        assert!(out.contains("message N {"));
        let twice = format_nfdl_source(&out).unwrap();
        assert_eq!(out, twice);
    }

    #[test]
    fn format_floats_comments_to_file_head() {
        let src = r#"
// head
protocol P {
    // inside
    meta { endian = big; mode = datagram; }
    message M { x: u8; /* trail */ }
}
"#;
        let out = format_nfdl_source(src).unwrap();
        assert!(out.starts_with("// head\n"));
        assert!(out.contains("// inside\n"));
        assert!(out.contains("/* trail */\n"));
        // Comments appear before the protocol body content.
        let proto_pos = out.find("protocol P").unwrap();
        assert!(out.find("// head").unwrap() < proto_pos);
        assert!(out.find("// inside").unwrap() < proto_pos);
        let twice = format_nfdl_source(&out).unwrap();
        assert_eq!(out, twice);
    }

    #[test]
    fn format_adgl_floats_comments_to_file_head() {
        let src = r#"
// head
ruleset "x" {
    /* mid */
    version = "1.0"
    evidence r {
        scope: Session
        anchor a: event(tcp.retransmission_burst) // trail
    }
}
"#;
        let out = format_adgl_source(src).unwrap();
        assert!(out.starts_with("// head\n"));
        assert!(out.contains("/* mid */\n"));
        assert!(out.contains("// trail\n"));
        let ruleset_pos = out.find("ruleset ").unwrap();
        assert!(out.find("// head").unwrap() < ruleset_pos);
        assert!(out.find("/* mid */").unwrap() < ruleset_pos);
        let twice = format_adgl_source(&out).unwrap();
        assert_eq!(out, twice);
    }
}
