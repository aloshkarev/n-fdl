//! Parse-error suggestions: expected vs found + short recovery hints.

use nfdl_syntax::{ParseError, Parser, Severity};

#[test]
fn bitfield_width_includes_expected_found_and_tip() {
    let src = r#"
protocol Bad {
  meta { endian = big; mode = datagram; }
  message M {
    a: bitfield{0};
  }
}
"#;
    let err = Parser::new(src)
        .parse_protocol()
        .expect_err("out-of-range bitfield must fail");
    let msg = match &err {
        ParseError::Syntax(m) => m.as_str(),
        ParseError::WithLocation { msg, .. } => msg.as_str(),
    };
    assert!(
        msg.contains("expected:") && msg.contains("found:"),
        "expected vs found missing: {msg}"
    );
    assert!(
        msg.contains("1..=64") && msg.contains("tip:"),
        "range tip missing: {msg}"
    );
}

#[test]
fn missing_semicolon_after_field_suggests_terminator() {
    let src = r#"
protocol Bad {
  meta { endian = big; mode = datagram; }
  message M {
    a: u8
    b: u16;
    c: bitfield{0};
  }
}
"#;
    let (proto, diags) = Parser::new(src).parse_protocol_with_diagnostics();
    let rendered: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.message.as_str())
        .collect();
    assert!(
        rendered.iter().any(|m| {
            m.contains("expected `;` after field")
                && m.contains("expected:")
                && m.contains("found:")
        }),
        "semicolon suggestion missing: {rendered:?}"
    );
    let names: Vec<_> = proto.messages[0]
        .fields
        .iter()
        .map(|f| f.name.as_str())
        .collect();
    assert!(
        names.contains(&"a") && names.contains(&"b"),
        "missing-`;` recovery must keep following fields: {names:?}"
    );
    assert!(
        rendered
            .iter()
            .any(|m| m.contains("1..=64") || m.contains("bitfield")),
        "later bad field should still contribute a diagnostic: {rendered:?}"
    );
}

#[test]
fn bad_primary_includes_expected_found_tip() {
    let src = r#"
protocol Bad {
  meta { endian = big; mode = datagram; }
  message M {
    a: u8 if ;
  }
}
"#;
    let err = Parser::new(src)
        .parse_protocol()
        .expect_err("bad primary must fail");
    let msg = match &err {
        ParseError::Syntax(m) => m.as_str(),
        ParseError::WithLocation { msg, .. } => msg.as_str(),
    };
    assert!(
        msg.contains("expected expression") && msg.contains("found"),
        "expected vs found missing: {msg}"
    );
    assert!(
        msg.contains("tip:") || msg.contains("ident"),
        "tip missing: {msg}"
    );
}
