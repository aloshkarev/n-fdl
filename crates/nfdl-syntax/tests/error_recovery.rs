//! Statement-level error recovery: collect multiple diagnostics in one parse.
//!
//! Sync points: `;`, `}`, and keywords (`message`, `bind`, `meta`, `state_machine`, …).

use nfdl_syntax::{Parser, Severity};

#[test]
fn two_bitfield_errors_yield_at_least_two_diagnostics() {
    let src = r#"
protocol Bad {
  meta { endian = big; mode = datagram; }
  message M {
    a: bitfield{0};
    ok: u8;
    c: bitfield{65};
  }
}
"#;
    let (proto, diags) = Parser::new(src).parse_protocol_with_diagnostics();
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(
        errors.len() >= 2,
        "expected ≥2 diagnostics, got {}: {:?}",
        errors.len(),
        errors
            .iter()
            .map(|d| d.message.as_str())
            .collect::<Vec<_>>()
    );
    assert!(
        errors.iter().any(|d| d.message.contains("1..=64")),
        "expected bitfield width messages: {:?}",
        errors
            .iter()
            .map(|d| d.message.as_str())
            .collect::<Vec<_>>()
    );
    // Recovery should still surface the valid middle field.
    assert_eq!(proto.messages.len(), 1);
    assert!(
        proto.messages[0].fields.iter().any(|f| f.name == "ok"),
        "expected recovered field `ok`, got {:?}",
        proto.messages[0]
            .fields
            .iter()
            .map(|f| f.name.as_str())
            .collect::<Vec<_>>()
    );
}

#[test]
fn recovery_continues_across_messages() {
    let src = r#"
protocol Multi {
  meta { endian = big; mode = datagram; }
  message First { x: bitfield{0}; }
  message Second { y: bitfield{65}; }
  message Third { z: u8; }
}
"#;
    let (proto, diags) = Parser::new(src).parse_protocol_with_diagnostics();
    assert!(
        diags.len() >= 2,
        "expected ≥2 diagnostics across messages, got {}",
        diags.len()
    );
    assert!(
        proto.messages.iter().any(|m| m.name == "Third"),
        "expected to keep parsing after errors; messages={:?}",
        proto.messages.iter().map(|m| m.name.as_str()).collect::<Vec<_>>()
    );
    let third = proto.messages.iter().find(|m| m.name == "Third").unwrap();
    assert!(third.fields.iter().any(|f| f.name == "z"));
}

#[test]
fn fail_fast_api_still_returns_first_error() {
    let src = r#"
protocol Bad {
  meta { endian = big; mode = datagram; }
  message M {
    a: bitfield{0};
    c: bitfield{65};
  }
}
"#;
    let err = Parser::new(src)
        .parse_protocol()
        .expect_err("fail-fast should still error");
    let msg = match &err {
        nfdl_syntax::ParseError::Syntax(m) => m.as_str(),
        nfdl_syntax::ParseError::WithLocation { msg, .. } => msg.as_str(),
    };
    assert!(
        msg.contains("1..=64") || msg.contains("bitfield"),
        "unexpected first error: {msg}"
    );
}

#[test]
fn snapshot_recovery_diagnostics() {
    let src = r#"
protocol Snap {
  meta { endian = big; mode = datagram; }
  message M {
    bad0: bitfield{0};
    good: u16;
    bad65: bitfield{65};
  }
}
"#;
    let (_proto, diags) = Parser::new(src).parse_protocol_with_diagnostics();
    let summary: Vec<(Severity, &str)> = diags
        .iter()
        .map(|d| (d.severity, d.message.as_str()))
        .collect();
    insta::assert_debug_snapshot!(summary);
}
