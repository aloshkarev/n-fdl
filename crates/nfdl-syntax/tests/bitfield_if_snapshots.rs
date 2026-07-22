//! Insta snapshots for EBNF `BitfieldType` and conditional `FieldStmt`.
//!
//! Spec: `docs/spec/02-grammar.ebnf`
//! - `BitfieldType = "bitfield" "{" INT "}"`  (* 1..=64 bits *)
//! - `FieldStmt = IDENT ":" Type [ "if" Expr ] ";"`

use nfdl_syntax::{Field, NfdlType, ParseError, Parser};

fn field_cond_triples<'a>(
    fields: impl IntoIterator<Item = &'a Field>,
) -> Vec<(String, NfdlType, Option<String>)> {
    fields
        .into_iter()
        .map(|f| {
            let cond = f.conditional.as_ref().map(|e| format!("{e:?}"));
            (f.name.clone(), f.ty.clone(), cond)
        })
        .collect()
}

fn field_view(src: &str) -> Vec<(String, NfdlType, Option<String>)> {
    let proto = Parser::new(src).parse_protocol().expect("parse");
    field_cond_triples(&proto.messages[0].fields)
}

fn loop_body_field_view(src: &str) -> Vec<(String, NfdlType, Option<String>)> {
    let proto = Parser::new(src).parse_protocol().expect("parse");
    let lp = &proto.messages[0].loops[0];
    field_cond_triples(&lp.body)
}

fn match_arm_loop_body_field_view(src: &str) -> Vec<(String, NfdlType, Option<String>)> {
    let proto = Parser::new(src).parse_protocol().expect("parse");
    let lp = &proto.messages[0].matches[0].arms[0].loops[0];
    field_cond_triples(&lp.body)
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

#[test]
fn snapshot_loop_body_conditional_field() {
    let src = r#"
protocol LoopCond {
  meta { endian = big; mode = datagram; }
  message Packet {
    flags: u8;
    loop items while flags > 0 {
      opt: u16 if flags > 0;
    }
  }
}
"#;
    insta::assert_debug_snapshot!(loop_body_field_view(src));
}

#[test]
fn snapshot_match_arm_loop_body_conditional_field() {
    let src = r#"
protocol ArmLoopCond {
  meta { endian = big; mode = datagram; }
  message Packet {
    kind: u8;
    match kind {
      case 1 => {
        loop items while kind > 0 {
          payload: u8 if kind == 1;
        }
      }
    }
  }
}
"#;
    insta::assert_debug_snapshot!(match_arm_loop_body_field_view(src));
}

#[test]
fn conditional_if_requires_expr() {
    let src = r#"
protocol Bad {
  meta { endian = big; mode = datagram; }
  message M { x: u8 if ; }
}
"#;
    let err = Parser::new(src)
        .parse_protocol()
        .expect_err("if without expr");
    insta::assert_debug_snapshot!(err);
}
