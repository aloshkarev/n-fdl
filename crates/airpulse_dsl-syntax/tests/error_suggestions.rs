//! Parse-error suggestions: expected vs found + short recovery hints.

use airpulse_dsl_syntax::parse_ruleset;

#[test]
fn misspelled_evidence_suggests_keyword() {
    let src = r#"
ruleset "Typo" {
  version = "1.0"
  evidance seed {
    scope: Session
    anchor a: event(tcp.retransmission_burst)
    infer Cause(X) { target: a.target, weight: +40, evidence: [a] }
  }
}
"#;
    let err = parse_ruleset(src).expect_err("misspelled evidence must fail");
    let rendered = err.render(src, "typo.adgl");
    assert!(
        rendered.contains("expected:") && rendered.contains("found:"),
        "expected vs found missing: {rendered}"
    );
    assert!(
        rendered.contains("did you mean `evidence`?"),
        "suggestion missing: {rendered}"
    );
}

#[test]
fn missing_colon_after_field_suggests_punct() {
    let src = r#"
ruleset "Colon" {
  version = "1.0"
  evidence seed {
    scope Session
    anchor a: event(tcp.retransmission_burst)
    infer Cause(X) { target: a.target, weight: +40, evidence: [a] }
  }
}
"#;
    let err = parse_ruleset(src).expect_err("missing colon must fail");
    let rendered = err.render(src, "colon.adgl");
    assert!(
        rendered.contains("expected:") && rendered.contains("found:"),
        "expected vs found missing: {rendered}"
    );
    assert!(
        rendered.contains("expected `:` after field name") || rendered.contains("expected: :"),
        "colon suggestion missing: {rendered}"
    );
}

#[test]
fn emit_in_evidence_includes_found_and_help() {
    let src = r#"
ruleset "Emit" {
  version = "1.0"
  evidence seed {
    scope: Session
    anchor a: event(tcp.retransmission_burst)
    emit Problem(X) { severity: High, evidence: [a] }
  }
}
"#;
    let err = parse_ruleset(src).expect_err("emit-in-evidence must fail");
    let rendered = err.render(src, "emit.adgl");
    assert!(
        rendered.contains("expected: infer | action") && rendered.contains("found: emit"),
        "expected vs found missing: {rendered}"
    );
    assert!(
        rendered.contains("move `emit` into a decision rule"),
        "help missing: {rendered}"
    );
}
