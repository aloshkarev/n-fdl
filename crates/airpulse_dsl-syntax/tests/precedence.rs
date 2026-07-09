use airpulse_dsl_syntax::ast::{BinaryOp, ExprKind, UnaryOp};
use airpulse_dsl_syntax::parse_expression;

fn parse_ok(src: &str) -> airpulse_dsl_syntax::ast::Expr<'_> {
    parse_expression(src).unwrap_or_else(|buf| panic!("expr parse failed: {}", buf.render(src, "expr")))
}

#[test]
fn multiplicative_binds_tighter_than_additive() {
    let expr = parse_ok("1 + 2 * 3");
    match expr.kind {
        ExprKind::Binary { op: BinaryOp::Add, left: _, right } => match right.kind {
            ExprKind::Binary { op: BinaryOp::Mul, .. } => {}
            _ => panic!("expected mul on right side"),
        },
        _ => panic!("expected additive root"),
    }
}

#[test]
fn unary_binds_tighter_than_comparison() {
    let expr = parse_ok("not present(x) == false");
    match expr.kind {
        ExprKind::Binary { op: BinaryOp::Eq, left, .. } => match left.kind {
            ExprKind::Unary { op: UnaryOp::Not, .. } => {}
            _ => panic!("expected unary not on lhs"),
        },
        _ => panic!("expected equality root"),
    }
}

#[test]
fn and_binds_tighter_than_or() {
    let expr = parse_ok("a or b and c");
    match expr.kind {
        ExprKind::Binary { op: BinaryOp::Or, right, .. } => match right.kind {
            ExprKind::Binary { op: BinaryOp::And, .. } => {}
            _ => panic!("expected and in rhs"),
        },
        _ => panic!("expected or root"),
    }
}
