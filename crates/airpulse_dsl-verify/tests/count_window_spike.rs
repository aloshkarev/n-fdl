use airpulse_dsl_verify::{render_diagnostics, verify_source};

#[test]
fn count_in_window_expressibility_spike() {
    let pre_aggregated = r#"
ruleset "spike.pre_aggregated" {
  version = "1.0"
  requires = ["wifi-ota"]
  evidence deauth_flood {
    scope: AccessPoint
    anchor storm: event(wifi.deauth_burst) { storm.count >= 30 }
    infer Cause(RfInterference) { target: storm.target, weight: +75, evidence: [storm] }
  }
}
"#;
    verify_source(pre_aggregated).expect("pre-aggregated count field must verify");

    let aggregate_call = r#"
ruleset "spike.aggregate_call" {
  version = "1.0"
  requires = ["wifi-ota", "topology"]
  evidence deauth_flood {
    scope: AccessPoint
    anchor storm: event(wifi.deauth_burst)
    correlate matches: event(wifi.deauth_burst) {
      topo: same_ap(storm.target, matches.target)
      time: matches.time in [storm.time - 1s, storm.time]
    }
    if count(matches) >= 30 {
      infer Cause(RfInterference) { target: storm.target, weight: +75, evidence: [storm, matches] }
    }
  }
}
"#;
    let diags = verify_source(aggregate_call).expect_err("aggregate calls are not supported");
    let rendered = render_diagnostics(aggregate_call, "count_in_window.adgl", &diags);
    println!("{rendered}");
    assert!(rendered.contains("ADGL0501"), "{rendered}");
    assert!(
        rendered.contains("calls are not allowed in pure expression positions"),
        "{rendered}"
    );

    let having_supported = r#"
ruleset "spike.having_supported" {
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
    verify_source(having_supported).expect("having: count >= N must verify");
}
