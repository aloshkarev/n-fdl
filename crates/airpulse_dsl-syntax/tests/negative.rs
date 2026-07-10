use airpulse_dsl_syntax::parse_ruleset;

#[test]
fn reports_missing_brace_with_span() {
    let src = r#"
ruleset "x" {
  version = "1.0"
  evidence r {
    scope: Session
    anchor a: event(tcp.retransmission_burst)
  }
"#;
    let err = parse_ruleset(src).expect_err("missing brace must fail");
    let rendered = err.render(src, "missing_brace.adgl");
    assert!(rendered.contains("ADGL0100"), "{rendered}");
    assert!(rendered.contains("missing_brace.adgl:"), "{rendered}");
}

#[test]
fn reports_bad_duration() {
    let src = r#"
ruleset "x" {
  version = "1.0"
  evidence r {
    scope: Session
    anchor a: event(tcp.retransmission_burst) { a.time > 1m }
  }
}
"#;
    let err = parse_ruleset(src).expect_err("bad duration must fail");
    let rendered = err.render(src, "bad_duration.adgl");
    assert!(rendered.contains("ADGL0110"), "{rendered}");
}

#[test]
fn rejects_emit_outside_decision_rule() {
    let src = r#"
ruleset "x" {
  version = "1.0"
  evidence r {
    scope: Session
    anchor a: event(tcp.retransmission_burst)
    emit Problem(X) { severity: High, evidence: [a] }
  }
}
"#;
    let err = parse_ruleset(src).expect_err("emit in evidence must fail");
    let rendered = err.render(src, "emit_outside_rule.adgl");
    assert!(rendered.contains("ADGL0450"), "{rendered}");
    assert!(
        rendered.contains("emit is not allowed in evidence rule body"),
        "{rendered}"
    );
}

#[test]
fn rejects_nesting_over_limit() {
    let nested = "(".repeat(65);
    let closed = ")".repeat(65);
    let src = format!(
        r#"
ruleset "x" {{
  version = "1.0"
  evidence r {{
    scope: Session
    anchor a: event(tcp.retransmission_burst) {{ {nested}1{closed} > 0 }}
  }}
}}
"#
    );
    let err = parse_ruleset(&src).expect_err("nesting > 64 must fail");
    let rendered = err.render(&src, "nesting_limit.adgl");
    assert!(rendered.contains("ADGL0103"), "{rendered}");
}

#[test]
fn rejects_reserved_keyword_in_present_ident() {
    let src = r#"
ruleset "x" {
  version = "1.0"
  evidence r {
    scope: Session
    anchor a: event(tcp.retransmission_burst)
    if present(and) {
      infer Cause(X) { target: a.target, weight: +40, evidence: [a] }
    }
  }
}
"#;
    let err = parse_ruleset(src).expect_err("reserved keyword ident must fail");
    let rendered = err.render(src, "reserved_present.adgl");
    assert!(rendered.contains("ADGL0100"), "{rendered}");
    assert!(rendered.contains("reserved keyword"), "{rendered}");
}

#[test]
fn rejects_reserved_keyword_in_ref_list() {
    let src = r#"
ruleset "x" {
  version = "1.0"
  evidence r {
    scope: Session
    anchor a: event(tcp.retransmission_burst)
    infer Cause(X) { target: a.target, weight: +40, evidence: [if] }
  }
}
"#;
    let err = parse_ruleset(src).expect_err("reserved keyword ref must fail");
    let rendered = err.render(src, "reserved_ref.adgl");
    assert!(rendered.contains("ADGL0100"), "{rendered}");
    assert!(rendered.contains("reserved keyword"), "{rendered}");
}

#[test]
fn rejects_unsigned_weight_literal() {
    let src = r#"
ruleset "x" {
  version = "1.0"
  evidence r {
    scope: Session
    anchor a: event(tcp.retransmission_burst)
    infer Cause(X) { target: a.target, weight: 40, evidence: [a] }
  }
}
"#;
    let err = parse_ruleset(src).expect_err("unsigned weight must fail");
    let rendered = err.render(src, "unsigned_weight.adgl");
    assert!(rendered.contains("ADGL0100"), "{rendered}");
    assert!(
        rendered.contains("weight requires explicit sign"),
        "{rendered}"
    );
}

#[test]
fn reports_unclosed_block_comment() {
    let src = r#"
ruleset "x" {
  version = "1.0"
  /* comment starts but never closes
  evidence r {
    scope: Session
    anchor a: event(tcp.retransmission_burst)
  }
}
"#;
    let err = parse_ruleset(src).expect_err("unclosed block comment must fail");
    let rendered = err.render(src, "unclosed_comment.adgl");
    assert!(rendered.contains("ADGL0106"), "{rendered}");
    assert!(rendered.contains("unclosed block comment"), "{rendered}");
}
