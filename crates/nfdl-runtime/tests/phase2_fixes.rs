//! Phase 2 targeted tests: u24 (3 bytes), endianness, ternary bytecode,
//! `!=` operator, validate enforcement, conditional-field skip, EmitField.
use nfdl_runtime::{
    BytecodeVm, Event, RuntimeError, parse_and_run_with_data, protocol_to_bytecode_with_map,
};
use nfdl_syntax::Parser;

fn run(
    src: &str,
    data: &[u8],
) -> Result<(std::collections::HashMap<String, u64>, Vec<Event>), RuntimeError> {
    let (_proto, ctx, _final, evs) = parse_and_run_with_data(src, data)?;
    Ok((ctx, evs))
}

#[test]
fn u24_reads_three_bytes_big_endian() {
    let src = r#"
protocol P {
    meta { endian = big; mode = datagram; }
    message M {
        a: u24;
        b: u8;
    }
}
"#;
    // a = 0x010203 (3 bytes BE), b = 0x04
    let (ctx, _) = run(src, &[0x01, 0x02, 0x03, 0x04]).expect("run");
    assert_eq!(*ctx.get("a").unwrap_or(&0), 0x010203, "u24 value");
    assert_eq!(*ctx.get("b").unwrap_or(&0), 0x04, "byte after u24");
    // __current_offset must have advanced by 4 (3 for u24 + 1 for u8)
    assert_eq!(*ctx.get("__current_offset").unwrap_or(&0), 4);
}

#[test]
fn little_endian_u16() {
    let src = r#"
protocol P {
    meta { endian = little; mode = datagram; }
    message M { a: u16; b: u8; }
}
"#;
    // a = 0x1234 little-endian -> bytes [0x34, 0x12]
    let (ctx, _) = run(src, &[0x34, 0x12, 0x99]).expect("run");
    assert_eq!(*ctx.get("a").unwrap_or(&0), 0x1234, "LE u16 value");
    assert_eq!(*ctx.get("b").unwrap_or(&0), 0x99, "byte after LE u16");
}

#[test]
fn ternary_bytecode_branches() {
    let src = r#"
protocol P {
    meta { endian = big; mode = datagram; }
    message M {
        c: u8;
        let x = c > 0 ? 100 : 200;
    }
}
"#;
    let (ctx_hi, _) = run(src, &[1]).expect("run c=1");
    assert_eq!(*ctx_hi.get("x").unwrap_or(&0), 100, "ternary then-branch");
    let (ctx_lo, _) = run(src, &[0]).expect("run c=0");
    assert_eq!(*ctx_lo.get("x").unwrap_or(&0), 200, "ternary else-branch");
}

#[test]
fn ne_operator_in_validate() {
    let src = r#"
protocol P {
    meta { endian = big; mode = datagram; }
    message M {
        a: u8;
        validate a != 0 -> "must be non-zero";
    }
}
"#;
    // a = 5 -> predicate (5 != 0) is true -> ok
    assert!(run(src, &[5]).is_ok(), "non-zero should pass validate");
    // a = 0 -> predicate false -> Constraint
    let err = run(src, &[0]).expect_err("zero should fail validate");
    assert!(
        matches!(err, RuntimeError::Constraint(ref m) if m.contains("non-zero")),
        "got: {:?}",
        err
    );
}

#[test]
fn conditional_field_skipped_when_cond_false() {
    let src = r#"
protocol P {
    meta { endian = big; mode = datagram; }
    message M {
        len: u8;
        val: u16 if len > 0;
        tail: u8;
    }
}
"#;
    // len = 0 -> val skipped, tail reads the next byte
    let (ctx_off, _) = run(src, &[0, 99]).expect("run len=0");
    assert_eq!(*ctx_off.get("len").unwrap_or(&0), 0);
    assert_eq!(
        *ctx_off.get("val").unwrap_or(&0),
        0,
        "val slot stays 0 when skipped"
    );
    assert_eq!(
        *ctx_off.get("tail").unwrap_or(&0),
        99,
        "tail reads byte right after len"
    );

    // len = 1 -> val read (2 bytes BE), tail after
    let (ctx_on, _) = run(src, &[1, 0xAA, 0xBB, 99]).expect("run len=1");
    assert_eq!(*ctx_on.get("len").unwrap_or(&0), 1);
    assert_eq!(
        *ctx_on.get("val").unwrap_or(&0),
        0xAABB,
        "val read when cond true"
    );
    assert_eq!(
        *ctx_on.get("tail").unwrap_or(&0),
        99,
        "tail reads after val"
    );
}

#[test]
fn emit_field_records_values() {
    let src = r#"
protocol P {
    meta { endian = big; mode = datagram; }
    message M {
        a: u8;
        b: u16;
    }
}
"#;
    let mut p = Parser::new(src);
    let proto = p.parse_protocol().expect("parse");
    let (program, _map) = protocol_to_bytecode_with_map(&proto);
    let mut vm = BytecodeVm::new(program.slot_count);
    vm.load_input(&[0x07, 0x12, 0x34]);
    vm.run(&program).expect("run");
    let emitted = vm.emitted();
    let find = |name: &str| -> u64 {
        *emitted
            .iter()
            .rev()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v)
            .unwrap_or(&0)
    };
    assert_eq!(find("a"), 0x07, "emitted a");
    assert_eq!(find("b"), 0x1234, "emitted b");
}

#[test]
fn message_dispatch_event_emitted() {
    let src = r#"
protocol P {
    meta { endian = big; mode = datagram; }
    message M { a: u8; }
}
"#;
    let (_ctx, evs) = run(src, &[42]).expect("run");
    assert!(
        evs.iter()
            .any(|e| matches!(e, Event::Message { msg_type, .. } if msg_type == "M")),
        "expected a Message dispatch event, got: {:?}",
        evs
    );
}
