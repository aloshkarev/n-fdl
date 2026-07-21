//! Rule/decl-level error recovery: collect multiple diagnostics in one parse.
//!
//! Sync points: `}` and keywords (`evidence`, `decision`, `mutually_exclusive`,
//! `version`, `requires`). ADGL has no statement-terminating `;`.

use airpulse_dsl_syntax::parse_ruleset;

#[test]
fn two_emit_in_evidence_errors_yield_at_least_two_diagnostics() {
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
    let err = parse_ruleset(src).expect_err("two illegal emits must fail");
    assert!(
        err.len() >= 2,
        "expected ≥2 diagnostics, got {}: {}",
        err.len(),
        err.render(src, "two_emits.adgl")
    );
    let rendered = err.render(src, "two_emits.adgl");
    assert!(
        rendered.matches("ADGL0450").count() >= 2,
        "expected ≥2 ADGL0450 codes: {rendered}"
    );
}

#[test]
fn recovery_continues_across_header_decls() {
    let src = r#"
ruleset "Decls" {
  version = "1.0"
  requires =
  mutually_exclusive
  evidence ok {
    scope: Session
    anchor a: event(tcp.retransmission_burst)
    infer Cause(X) { target: a.target, weight: +40, evidence: [a] }
  }
}
"#;
    let err = parse_ruleset(src).expect_err("two bad decls must fail");
    assert!(
        err.len() >= 2,
        "expected ≥2 diagnostics across decls, got {}: {}",
        err.len(),
        err.render(src, "two_decls.adgl")
    );
}

#[test]
fn recovery_across_rules_still_surfaces_later_errors() {
    let src = r#"
ruleset "Multi" {
  version = "1.0"
  evidence broken {
    scope: Session
    anchor a: event(tcp.retransmission_burst)
    infer Cause(X) { target: a.target, weight: 40, evidence: [a] }
  }
  decision later {
    scope: Session
    anchor c: Cause(X) { c.confidence >= 80 }
    emit Problem(P) { severity: High, evidence: [c] }
    infer Cause(Y) { target: c.target, weight: +10, evidence: [c] }
  }
}
"#;
    let err = parse_ruleset(src).expect_err("errors in two rules must fail");
    assert!(
        err.len() >= 2,
        "expected ≥2 diagnostics across rules, got {}: {}",
        err.len(),
        err.render(src, "multi_rule.adgl")
    );
    let rendered = err.render(src, "multi_rule.adgl");
    assert!(
        rendered.contains("weight requires explicit sign") || rendered.contains("ADGL0100"),
        "expected unsigned-weight error: {rendered}"
    );
    assert!(
        rendered.contains("infer is not allowed") || rendered.contains("ADGL0450"),
        "expected infer-in-decision error: {rendered}"
    );
}

#[test]
fn snapshot_recovery_diagnostics() {
    let src = r#"
ruleset "Snap" {
  version = "1.0"
  evidence e1 {
    scope: Session
    anchor a: event(tcp.retransmission_burst)
    emit Problem(X) { severity: High, evidence: [a] }
  }
  evidence e2 {
    scope: Session
    anchor b: event(tcp.retransmission_burst)
    emit Problem(Y) { severity: High, evidence: [b] }
  }
}
"#;
    let err = parse_ruleset(src).expect_err("snapshot source must fail");
    let summary: Vec<(&str, &str)> = err
        .iter()
        .map(|d| (d.code, d.message.as_str()))
        .collect();
    insta::assert_debug_snapshot!(summary);
}
