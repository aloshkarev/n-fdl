use airpulse_dsl_syntax::ast::{ActionName, RuleDecl, Stmt};
use airpulse_dsl_syntax::parse_ruleset;
use insta::assert_snapshot;

fn summarize(src_name: &str, src: &str) -> String {
    let parsed = parse_ruleset(src).unwrap_or_else(|buf| {
        panic!("failed to parse {src_name}: {}", buf.render(src, src_name));
    });
    let mut out = String::new();
    out.push_str(&format!("ruleset:{}\n", parsed.name.value));
    for rule in parsed.rules {
        match rule {
            RuleDecl::Evidence(e) => {
                out.push_str(&format!(
                    "evidence:{} scope={:?} anchor=event({}) correlates={}\n",
                    e.name.name,
                    e.scope,
                    e.anchor
                        .event_type
                        .segments
                        .iter()
                        .map(|s| s.name)
                        .collect::<Vec<_>>()
                        .join("."),
                    e.correlates.len()
                ));
                for stmt in e.body {
                    out.push_str(&format!("  {}\n", stmt_name(&stmt)));
                }
            }
            RuleDecl::Decision(d) => {
                let anchor_kind = match d.anchor {
                    airpulse_dsl_syntax::ast::DecisionAnchor::Cause(_) => "Cause",
                    airpulse_dsl_syntax::ast::DecisionAnchor::Problem(_) => "Problem",
                };
                out.push_str(&format!(
                    "decision:{} scope={:?} anchor={} correlates={}\n",
                    d.name.name,
                    d.scope,
                    anchor_kind,
                    d.correlates.len()
                ));
                for stmt in d.body {
                    out.push_str(&format!("  {}\n", stmt_name(&stmt)));
                }
            }
        }
    }
    out
}

fn stmt_name(stmt: &Stmt<'_>) -> String {
    match stmt {
        Stmt::Infer(i) => format!("infer:{}", i.cause.name),
        Stmt::Emit(e) => format!("emit:{}", e.problem.name),
        Stmt::Action(a) => {
            let name = match &a.action {
                ActionName::Known(k) => k.as_str().to_string(),
                ActionName::Custom(id) => id.name.to_string(),
            };
            format!("action:{name}")
        }
    }
}

#[test]
fn parse_all_examples_and_snapshot_shape() {
    let fixtures: [(&str, &str); 10] = [
        (
            "01-pmtud-blackhole.adgl",
            include_str!("../../../docs/idea/examples/01-pmtud-blackhole.adgl"),
        ),
        (
            "02-tcp-retrans-seed.adgl",
            include_str!("../../../docs/idea/examples/02-tcp-retrans-seed.adgl"),
        ),
        (
            "03-auth-outage-impact.adgl",
            include_str!("../../../docs/idea/examples/03-auth-outage-impact.adgl"),
        ),
        (
            "04-dhcp-missing-auth.adgl",
            include_str!("../../../docs/idea/examples/04-dhcp-missing-auth.adgl"),
        ),
        (
            "05-crc-link-flap.adgl",
            include_str!("../../../docs/idea/examples/05-crc-link-flap.adgl"),
        ),
        (
            "06-link-absent.adgl",
            include_str!("../../../docs/idea/examples/06-link-absent.adgl"),
        ),
        (
            "07-suppress-downstream.adgl",
            include_str!("../../../docs/idea/examples/07-suppress-downstream.adgl"),
        ),
        (
            "08-stp-tcp-burst.adgl",
            include_str!("../../../docs/idea/examples/08-stp-tcp-burst.adgl"),
        ),
        (
            "09-ap-deauth-missing-rf.adgl",
            include_str!("../../../docs/idea/examples/09-ap-deauth-missing-rf.adgl"),
        ),
        (
            "10-ambiguity-demo.adgl",
            include_str!("../../../docs/idea/examples/10-ambiguity-demo.adgl"),
        ),
    ];

    for (name, src) in fixtures {
        let summary = summarize(name, src);
        assert_snapshot!(name, summary);
    }
}
