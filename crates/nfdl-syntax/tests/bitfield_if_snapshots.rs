//! Insta snapshots for EBNF `BitfieldType` and conditional `FieldStmt`.
//!
//! Spec: `docs/spec/02-grammar.ebnf`
//! - `BitfieldType = "bitfield" "{" INT "}"`  (* 1..=64 bits *)
//! - `FieldStmt = IDENT ":" Type [ "if" Expr ] ";"`

use nfdl_syntax::{NfdlType, ParseError, Parser};

fn field_view(src: &str) -> Vec<(String, NfdlType, Option<String>)> {
    let proto = Parser::new(src).parse_protocol().expect("parse");
    let msg = &proto.messages[0];
    msg.fields
        .iter()
        .map(|f| {
            let cond = f.conditional.as_ref().map(|e| format!("{e:?}"));
            (f.name.clone(), f.ty.clone(), cond)
        })
        .collect()
}

#[test]
fn snapshot_bitfield_message() {
    let src = r#"
protocol Flags {
  meta { endian = big; mode = datagram; }
  message Header {
    version: bitfield{4};
    ihl: bitfield{4};
    tos: u8;
  }
}
"#;
    insta::assert_debug_snapshot!(field_view(src));
}

#[test]
fn snapshot_conditional_field() {
    let src = r#"
protocol Opt {
  meta { endian = big; mode = datagram; }
  message Packet {
    flags: u8;
    options: bytes[flags] if flags > 0;
  }
}
"#;
    insta::assert_debug_snapshot!(field_view(src));
}

#[test]
fn snapshot_bitfield_and_if_together() {
    let src = r#"
protocol Mixed {
  meta { endian = big; mode = datagram; }
  message Frame {
    present: bitfield{1};
    payload: u16 if present == 1;
  }
}
"#;
    insta::assert_debug_snapshot!(field_view(src));
}

#[test]
fn bitfield_width_out_of_range_errors() {
    let src = r#"
protocol Bad {
  meta { endian = big; mode = datagram; }
  message M { x: bitfield{0}; }
}
"#;
    let err = Parser::new(src).parse_protocol().expect_err("bitfield{0}");
    insta::assert_debug_snapshot!(err);
}

#[test]
fn bitfield_width_above_64_errors() {
    let src = r#"
protocol Bad {
  meta { endian = big; mode = datagram; }
  message M { x: bitfield{65}; }
}
"#;
    let err = Parser::new(src).parse_protocol().expect_err("bitfield{65}");
    match err {
        ParseError::Syntax(msg) => assert!(msg.contains("1..=64"), "{msg}"),
        other => panic!("unexpected: {other:?}"),
    }
}

#[test]
fn conditional_field_rejects_rem() {
    let src = r#"
protocol Bad {
  meta { endian = big; mode = datagram; }
  message M { x: u8 if __rem; }
}
"#;
    let err = Parser::new(src).parse_protocol().expect_err("__rem if");
    insta::assert_debug_snapshot!(err);
}
