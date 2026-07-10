//! Verifier tests for correlate `having: count >= N`.

use airpulse_dsl_verify::verify_source;

#[test]
fn verify_lowers_having_min_match() {
    let src = r#"
ruleset "verify.having" {
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
    let verified = verify_source(src).expect("supported having should verify");
    assert_eq!(verified.image.rules[0].correlates[0].min_match, 30);
}

#[test]
fn verify_rejects_count_zero_with_adgl0504() {
    let src = r#"
ruleset "verify.zero" {
  version = "1.0"
  requires = ["wifi-ota", "topology"]
  evidence bad {
    scope: AccessPoint
    anchor storm: event(wifi.deauth_burst)
    correlate hits: event(wifi.mgmt.deauth) {
      topo: same_ap(storm.target, hits.target)
      time: hits.time in [storm.time - 1s, storm.time]
      having: count >= 0
    }
    infer Cause(RfInterference) { target: storm.target, weight: +1, evidence: [storm] }
  }
}
"#;
    let diags = verify_source(src).expect_err("N=0 must fail");
    assert!(diags.iter().any(|d| d.code == "ADGL0504"), "{diags:?}");
}

#[test]
fn verify_rejects_count_over_32_with_adgl0505() {
    let src = r#"
ruleset "verify.over_cap" {
  version = "1.0"
  requires = ["wifi-ota", "topology"]
  evidence bad {
    scope: AccessPoint
    anchor storm: event(wifi.deauth_burst)
    correlate hits: event(wifi.mgmt.deauth) {
      topo: same_ap(storm.target, hits.target)
      time: hits.time in [storm.time - 1s, storm.time]
      having: count >= 33
    }
    infer Cause(RfInterference) { target: storm.target, weight: +1, evidence: [storm] }
  }
}
"#;
    let diags = verify_source(src).expect_err("N>32 must fail");
    assert!(diags.iter().any(|d| d.code == "ADGL0505"), "{diags:?}");
}

#[test]
fn verify_rejects_every_large_representable_count_without_wrapping() {
    for count in [33_i64, 257, 288, i64::MAX] {
        let src = format!(
            r#"
ruleset "verify.large_count" {{
  version = "1.0"
  requires = ["wifi-ota", "topology"]
  evidence bad {{
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
        let diags = verify_source(&src).expect_err("N > 32 must fail");
        assert!(
            diags.iter().any(|d| d.code == "ADGL0505"),
            "count {count} wrapped or produced the wrong diagnostic: {diags:?}"
        );
    }
}
