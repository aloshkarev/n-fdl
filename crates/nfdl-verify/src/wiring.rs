//! Verifier wiring (spec `05-verification.md`).
//!
//! Wires the standalone `IntervalAnalyzer` into a protocol-level pass that walks
//! the AST and flags `bytes[len]` slices whose length interval cannot be proven
//! to fit within the remaining input. Emits structured `Diagnostic`s.
//!
//! Z3 (`z3_backend`) remains a stub for v1.5/v2 per ADR-002 C5 (Z3 optional);
//! interval bounds analysis is the v1 pragmatic verifier.

#![forbid(unsafe_code)]

use crate::bounds::{Interval, IntervalAnalyzer};
use nfdl_diag::{DiagBuffer, Diagnostic};
use nfdl_syntax::ast::{Expr, Field, Message, NfdlType, Protocol, UnaryOp};

/// Verify a parsed protocol. Returns a `DiagBuffer` of warnings/notes (no fatal
/// errors — bounds analysis is advisory in v1; runtime `validate` is the hard
/// guard). Unknown variable intervals yield a conservative `unsafe_slice` note.
pub fn verify_protocol(proto: &Protocol) -> DiagBuffer {
    let mut buf = DiagBuffer::new();
    for msg in &proto.messages {
        verify_message(msg, &mut buf);
    }
    buf
}

fn verify_message(msg: &Message, buf: &mut DiagBuffer) {
    // Seed the analyzer with conservative per-field intervals. Concrete-size
    // scalars (u8/u16/u24/u32) read a fixed number of bytes; everything else
    // is left unknown (wide) so the slice-safety check stays conservative.
    let mut analyzer = IntervalAnalyzer::new();
    seed_field_intervals(&msg.fields, &mut analyzer);
    for lp in &msg.loops {
        seed_field_intervals(&lp.body, &mut analyzer);
    }

    for f in &msg.fields {
        check_field_slice(f, &analyzer, buf);
    }
    for lp in &msg.loops {
        for f in &lp.body {
            check_field_slice(f, &analyzer, buf);
        }
    }
}

fn seed_field_intervals(fields: &[Field], analyzer: &mut IntervalAnalyzer) {
    for f in fields {
        // Seed the *value* interval of scalar fields (not their byte size): a u8
        // field's value ranges over [0, 255], u16 over [0, 65535], etc. This makes
        // `bytes[length - 8]`-style subtraction checks meaningful (can go negative).
        let value = match f.ty {
            NfdlType::U8 => Some(Interval::new(0, (1i64 << 8) - 1)),
            NfdlType::U16 => Some(Interval::new(0, (1i64 << 16) - 1)),
            NfdlType::U24 => Some(Interval::new(0, (1i64 << 24) - 1)),
            NfdlType::U32 => Some(Interval::new(0, i64::MAX)), // full u32 overflows i64; cap
            _ => None,
        };
        if let Some(v) = value {
            analyzer.add_fact(&f.name, v);
        }
    }
}

fn check_field_slice(field: &Field, analyzer: &IntervalAnalyzer, buf: &mut DiagBuffer) {
    if let NfdlType::Bytes { len } = &field.ty {
        // Without a known __rem fact we cannot prove safety; emit a note so the
        // author is aware the slice relies on a runtime validate/__rem guard.
        let len_int = analyzer.analyze_expr(&expr_to_string(len));
        if len_int.lo < 0 {
            buf.push(Diagnostic::warning(
                "NFDV01",
                format!(
                    "`bytes[...]` length `{}` may be negative (interval [{},{}]); add a `validate` guard",
                    expr_to_string(len), len_int.lo, len_int.hi
                ),
                field.span,
            ));
        } else if len_int.hi == i64::MAX / 2 {
            buf.push(Diagnostic::note(
                "NFDV02",
                format!(
                    "`bytes[...]` length `{}` is not statically bounded; relies on runtime `validate`/`__rem`",
                    expr_to_string(len)
                ),
                field.span,
            ));
        }
    }
}

fn expr_to_string(expr: &Expr) -> String {
    match expr {
        Expr::Ident(s) => s.clone(),
        Expr::Int(v) => v.to_string(),
        Expr::Str(s) => format!("\"{s}\""),
        Expr::Binary { left, op, right } => {
            format!(
                "{} {} {}",
                expr_to_string(left),
                binop_str(op),
                expr_to_string(right)
            )
        }
        Expr::Unary { op, expr } => format!("{}{}", unaryop_str(op), expr_to_string(expr)),
        Expr::Ternary {
            cond,
            then_branch,
            else_branch,
        } => format!(
            "({} ? {} : {})",
            expr_to_string(cond),
            expr_to_string(then_branch),
            expr_to_string(else_branch)
        ),
        Expr::Coalesce { value, default } => {
            format!("({} ?? {})", expr_to_string(value), expr_to_string(default))
        }
        Expr::Call { name, args } => {
            let args = args
                .iter()
                .map(expr_to_string)
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}({})", name, args)
        }
        Expr::Tuple(elems) => {
            let elems = elems
                .iter()
                .map(expr_to_string)
                .collect::<Vec<_>>()
                .join(", ");
            format!("({})", elems)
        }
        Expr::Field(base, field) => format!("{}.{}", expr_to_string(base), field),
    }
}

fn binop_str(op: &nfdl_syntax::ast::BinOp) -> &'static str {
    use nfdl_syntax::ast::BinOp::*;
    match op {
        Add => "+",
        Sub => "-",
        Mul => "*",
        Div => "/",
        Mod => "%",
        Eq => "==",
        Ne => "!=",
        Lt => "<",
        Le => "<=",
        Gt => ">",
        Ge => ">=",
        And => "&&",
        Or => "||",
        BitAnd => "&",
        BitOr => "|",
        BitXor => "^",
        Shl => "<<",
        Shr => ">>",
    }
}

fn unaryop_str(op: &UnaryOp) -> &'static str {
    match op {
        UnaryOp::Not => "!",
        UnaryOp::BitNot => "~",
        UnaryOp::Neg => "-",
    }
}

// Re-export Severity so callers don't need a second dependency for the enum.
pub use nfdl_diag::Severity;

#[cfg(test)]
mod tests {
    use super::*;
    use nfdl_syntax::Parser;

    #[test]
    fn subtraction_length_minus_constant_can_go_negative() {
        // `bytes[length - 8]` where length is u16 (value [0,65535]); the
        // subtraction's low end is 0 - 8 = -8, so NFDV01 must fire (needs a
        // runtime `validate length >= 8` guard).
        let src = r#"
protocol P {
    meta { endian = big; mode = datagram; }
    message M {
        length: u16;
        data: bytes[length - 8];
    }
}
"#;
        let proto = Parser::new(src).parse_protocol().expect("parse");
        let buf = verify_protocol(&proto);
        assert!(
            buf.iter().any(|d| d.code == "NFDV01"),
            "expected NFDV01 for possibly-negative slice length, got: {}",
            buf.render(src, "x")
        );
        assert!(
            !buf.has_errors(),
            "bounds analysis is advisory (no hard errors)"
        );
    }

    #[test]
    fn nfdv01_bytes_diagnostic_carries_real_span() {
        // Field `data: bytes[length - 8];` must surface a non-unknown span so
        // diagnostics can point at the source rather than 1:1 placeholders.
        let src = "\
protocol P {\n\
    meta { endian = big; mode = datagram; }\n\
    message M {\n\
        length: u16;\n\
        data: bytes[length - 8];\n\
    }\n\
}\n";
        let field_start = src
            .find("data: bytes[length - 8];")
            .expect("field in source");
        let field_end = field_start + "data: bytes[length - 8];".len();

        let proto = Parser::new(src).parse_protocol().expect("parse");
        let buf = verify_protocol(&proto);
        let diag = buf
            .iter()
            .find(|d| d.code == "NFDV01")
            .expect("expected NFDV01");
        assert!(
            diag.span.start != 0 || diag.span.end != 0,
            "NFDV01 span must not be Span::unknown(), got {:?}",
            diag.span
        );
        assert_eq!(
            diag.span.start, field_start,
            "NFDV01 should start at the `data` field"
        );
        assert_eq!(
            diag.span.end, field_end,
            "NFDV01 should end at the field semicolon"
        );
    }

    #[test]
    fn constant_bytes_length_is_clean() {
        let src = r#"
protocol P {
    meta { endian = big; mode = datagram; }
    message M {
        data: bytes[4];
    }
}
"#;
        let proto = Parser::new(src).parse_protocol().expect("parse");
        let buf = verify_protocol(&proto);
        assert!(!buf.has_errors());
        let has_note = buf.iter().any(|d| d.code == "NFDV01" || d.code == "NFDV02");
        assert!(
            !has_note,
            "constant length should not trigger diagnostics, got: {:?}",
            buf.render(src, "x")
        );
    }

    #[test]
    fn bounded_variable_length_is_clean() {
        // `bytes[len]` where len is u8 (value [0,255]) is statically bounded and
        // non-negative, so no advisory diagnostic is needed.
        let src = r#"
protocol P {
    meta { endian = big; mode = datagram; }
    message M {
        len: u8;
        data: bytes[len];
    }
}
"#;
        let proto = Parser::new(src).parse_protocol().expect("parse");
        let buf = verify_protocol(&proto);
        assert!(!buf.has_errors());
        let has_note = buf.iter().any(|d| d.code == "NFDV01" || d.code == "NFDV02");
        assert!(
            !has_note,
            "bounded u8 length should not trigger diagnostics, got: {:?}",
            buf.render(src, "x")
        );
    }
}
