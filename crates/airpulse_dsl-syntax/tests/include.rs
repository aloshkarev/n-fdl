//! ADGL multi-file `include` loader tests.

use airpulse_dsl_syntax::{load_ruleset, parse_ruleset, RuleDecl};
use std::path::PathBuf;

fn fixture(parts: &[&str]) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("tests");
    p.push("fixtures");
    for part in parts {
        p.push(part);
    }
    p
}

#[test]
fn load_ruleset_merges_included_rules() {
    let path = fixture(&["include", "ok", "main.adgl"]);
    let loaded = load_ruleset(&path).expect("load with include");
    let ast = loaded.parse().expect("parse composed source");

    assert_eq!(ast.name.value, "main");
    let names: Vec<&str> = ast
        .rules
        .iter()
        .map(|r| match r {
            RuleDecl::Evidence(e) => e.name.name,
            RuleDecl::Decision(d) => d.name.name,
        })
        .collect();
    assert_eq!(names, ["shared_ev", "main_ev"]);

    // Single-file API still rejects raw include syntax.
    let raw = std::fs::read_to_string(&path).unwrap();
    assert!(
        parse_ruleset(&raw).is_err(),
        "parse_ruleset must not accept include directives"
    );
}

#[test]
fn load_ruleset_detects_include_cycle() {
    let path = fixture(&["include", "cycle", "a.adgl"]);
    let err = load_ruleset(&path).expect_err("cycle must fail");
    let msg = err
        .iter()
        .map(|d| format!("{} {}", d.code, d.message))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        msg.contains("ADGL0200") && msg.contains("cycle"),
        "expected cycle diagnostic, got:\n{msg}"
    );
    assert!(
        msg.contains("a.adgl") && msg.contains("b.adgl"),
        "cycle path should name both files, got:\n{msg}"
    );
}
