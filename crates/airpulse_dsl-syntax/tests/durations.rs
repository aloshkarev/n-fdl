use airpulse_dsl_syntax::ast::ExprKind;
use airpulse_dsl_syntax::parse_expression;

#[test]
fn parses_duration_forms() {
    let ms = parse_expression("500ms").expect("500ms");
    let s = parse_expression("1s").expect("1s");
    let min = parse_expression("2min").expect("2min");

    assert!(matches!(ms.kind, ExprKind::Duration(d) if d.millis == 500));
    assert!(matches!(s.kind, ExprKind::Duration(d) if d.millis == 1_000));
    assert!(matches!(min.kind, ExprKind::Duration(d) if d.millis == 120_000));
}

#[test]
fn rejects_bad_duration_unit() {
    let err = parse_expression("7m").expect_err("7m must fail");
    let rendered = err.render("7m", "duration.adgl");
    assert!(rendered.contains("ADGL0110"), "{rendered}");
}
