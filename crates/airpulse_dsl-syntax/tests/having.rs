//! Tests for `having: count >= N` correlate syntax.

use airpulse_dsl_syntax::parse_ruleset;

#[test]
fn parse_correlate_having_count_threshold() {
    let src = r#"
ruleset "syntax.having" {
  version = "1.0"
  requires = ["wifi-ota", "topology"]
  evidence deauth_flood {
    scope: AccessPoint
    anchor storm: event(wifi.deauth_burst)
    correlate hits: event(wifi.mgmt.deauth) {
      topo: same_ap(storm.target, hits.target)
      time: hits.time in [storm.time - 1s, storm.time]
      having: count >= 30
    }
    if present(hits) {
      infer Cause(RfInterference) { target: storm.target, weight: +75, evidence: [storm, hits] }
    }
  }
}
"#;
    let ast = parse_ruleset(src).expect("having clause should parse");
    let rule = match &ast.rules[0] {
        airpulse_dsl_syntax::ast::RuleDecl::Evidence(e) => e,
        _ => panic!("expected evidence rule"),
    };
    let corr = &rule.correlates[0];
    let min = corr.min_match.expect("having clause present");
    assert_eq!(min.count, 30);
}

#[test]
fn parse_preserves_large_having_literals_without_truncation() {
    for count in [33_i64, 257, 288, i64::MAX] {
        let src = format!(
            r#"
ruleset "syntax.having_large" {{
  version = "1.0"
  requires = ["wifi-ota", "topology"]
  evidence deauth_flood {{
    scope: AccessPoint
    anchor storm: event(wifi.deauth_burst)
    correlate hits: event(wifi.mgmt.deauth) {{
      topo: same_ap(storm.target, hits.target)
      time: hits.time in [storm.time - 1s, storm.time]
      having: count >= {count}
    }}
    infer Cause(RfInterference) {{ target: storm.target, weight: +1, evidence: [storm] }}
  }}
}}
"#
        );
        let ast = parse_ruleset(&src).expect("representable integer literal should parse");
        let rule = match &ast.rules[0] {
            airpulse_dsl_syntax::ast::RuleDecl::Evidence(e) => e,
            _ => panic!("expected evidence rule"),
        };
        assert_eq!(
            rule.correlates[0]
                .min_match
                .expect("having clause present")
                .count,
            count
        );
    }
}

#[test]
fn negative_having_literal_is_a_syntax_error() {
    let src = r#"
ruleset "syntax.having_negative" {
  version = "1.0"
  requires = ["wifi-ota", "topology"]
  evidence deauth_flood {
    scope: AccessPoint
    anchor storm: event(wifi.deauth_burst)
    correlate hits: event(wifi.mgmt.deauth) {
      topo: same_ap(storm.target, hits.target)
      time: hits.time in [storm.time - 1s, storm.time]
      having: count >= -1
    }
    infer Cause(RfInterference) { target: storm.target, weight: +1, evidence: [storm] }
  }
}
"#;
    let diags = parse_ruleset(src).expect_err("negative threshold must not parse");
    assert!(diags.iter().any(|d| d.code == "ADGL0100"), "{diags:?}");
}

#[test]
fn malformed_having_forms_report_syntax_diagnostics() {
    for correlate_body in [
        "topo: same_ap(storm.target, hits.target)\n      time: hits.time in [storm.time - 1s, storm.time]\n      having: count > 3",
        "topo: same_ap(storm.target, hits.target)\n      time: hits.time in [storm.time - 1s, storm.time]\n      having: matches >= 3",
        "topo: same_ap(storm.target, hits.target)\n      having: count >= 3\n      time: hits.time in [storm.time - 1s, storm.time]",
    ] {
        let src = format!(
            r#"
ruleset "syntax.having_malformed" {{
  version = "1.0"
  requires = ["wifi-ota", "topology"]
  evidence bad {{
    scope: AccessPoint
    anchor storm: event(wifi.deauth_burst)
    correlate hits: event(wifi.mgmt.deauth) {{
      {correlate_body}
    }}
    infer Cause(RfInterference) {{ target: storm.target, weight: +1, evidence: [storm] }}
  }}
}}
"#
        );
        let diags = parse_ruleset(&src).expect_err("malformed having must not parse");
        assert!(
            diags.iter().any(|diag| diag.code == "ADGL0100"),
            "malformed form should produce a syntax diagnostic: {diags:?}"
        );
    }
}

#[test]
fn parse_correlate_omitted_having_defaults_none() {
    let src = r#"
ruleset "syntax.no_having" {
  version = "1.0"
  requires = ["topology"]
  evidence ptb {
    scope: Session
    anchor rtx: event(tcp.retransmission_burst) { rtx.segment_size > 1400 }
    correlate ptb: event(icmp.ptb) {
      topo: same_session(rtx.target, ptb.target)
      time: ptb.time in [rtx.time - 500ms, rtx.time + 1s]
    }
    if present(ptb) {
      infer Cause(PmtudBlackhole) { target: rtx.target, weight: +85, evidence: [rtx, ptb] }
    }
  }
}
"#;
    let ast = parse_ruleset(src).expect("correlate without having should parse");
    let rule = match &ast.rules[0] {
        airpulse_dsl_syntax::ast::RuleDecl::Evidence(e) => e,
        _ => panic!("expected evidence rule"),
    };
    assert!(rule.correlates[0].min_match.is_none());
}
