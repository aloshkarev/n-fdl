//! Phase 3 targeted tests: bitfield (bit-cursor, cross-byte), bytes[EOF]/bytes[..]
//! (ReadRest terminal), and `match` tagged-union dispatch.
use nfdl_runtime::{Event, parse_and_run_stream, parse_and_run_with_data};

fn ctx_of(src: &str, data: &[u8]) -> std::collections::HashMap<String, u64> {
    let (_proto, ctx, _final, _evs) = parse_and_run_with_data(src, data).expect("run");
    ctx
}

#[test]
fn bitfield_packs_within_a_byte() {
    let src = r#"
protocol P {
    meta { endian = big; mode = datagram; }
    message M {
        a: bitfield{4};
        b: bitfield{4};
        c: u8;
    }
}
"#;
    // 0xAB -> a=0xA, b=0xB; then c=0x99
    let ctx = ctx_of(src, &[0xAB, 0x99]);
    assert_eq!(*ctx.get("a").unwrap_or(&0), 0xA, "high nibble");
    assert_eq!(*ctx.get("b").unwrap_or(&0), 0xB, "low nibble");
    assert_eq!(*ctx.get("c").unwrap_or(&0), 0x99, "byte after bitfields");
}

#[test]
fn bitfield_crosses_byte_boundary() {
    let src = r#"
protocol P {
    meta { endian = big; mode = datagram; }
    message M {
        a: bitfield{4};
        b: bitfield{12};
    }
}
"#;
    // 0xAB 0xCD -> a=0xA (top 4 of 0xAB), b=0xBCD (low 4 of 0xAB + 0xCD)
    let ctx = ctx_of(src, &[0xAB, 0xCD]);
    assert_eq!(*ctx.get("a").unwrap_or(&0), 0xA, "a across boundary");
    assert_eq!(
        *ctx.get("b").unwrap_or(&0),
        0xBCD,
        "b = 12 bits spanning two bytes"
    );
}

#[test]
fn bytes_eof_consumes_rest() {
    let src = r#"
protocol P {
    meta { endian = big; mode = stream; eof = on_fin; }
    message M {
        a: u8;
        payload: bytes[EOF];
    }
}
"#;
    // a=5, payload consumes the remaining 2 bytes
    let ctx = ctx_of(src, &[0x05, 0xAA, 0xBB]);
    assert_eq!(*ctx.get("a").unwrap_or(&0), 5);
    assert_eq!(
        *ctx.get("payload").unwrap_or(&0),
        2,
        "bytes[EOF] = remaining byte count"
    );
    assert_eq!(
        *ctx.get("__current_offset").unwrap_or(&0),
        3,
        "offset at end of input"
    );
}

#[test]
fn bytes_dotdot_consumes_rest() {
    let src = r#"
protocol P {
    meta { endian = big; mode = datagram; }
    message M {
        a: u8;
        rest: bytes[..];
    }
}
"#;
    let ctx = ctx_of(src, &[0x07, 0x01, 0x02, 0x03]);
    assert_eq!(*ctx.get("a").unwrap_or(&0), 7);
    assert_eq!(
        *ctx.get("rest").unwrap_or(&0),
        3,
        "bytes[..] = remaining byte count"
    );
}

#[test]
fn match_dispatches_case_arm() {
    let src = r#"
protocol P {
    meta { endian = big; mode = datagram; }
    message M {
        code: u8;
        match code {
            case 1 => { x: u8; }
            default => { y: u16; }
        }
    }
}
"#;
    // code = 1 -> case arm reads x
    let ctx1 = ctx_of(src, &[1, 0x42]);
    assert_eq!(*ctx1.get("code").unwrap_or(&0), 1);
    assert_eq!(*ctx1.get("x").unwrap_or(&0), 0x42, "case-1 arm field");
    // code = 2 -> default arm reads y (2 bytes BE)
    let ctx2 = ctx_of(src, &[2, 0x12, 0x34]);
    assert_eq!(*ctx2.get("code").unwrap_or(&0), 2);
    assert_eq!(*ctx2.get("y").unwrap_or(&0), 0x1234, "default arm field");
}

#[test]
fn match_case_arm_with_loop() {
    // Mirrors diameter's grouped AVP: a case arm containing a `let` + `loop`.
    let src = r#"
protocol P {
    meta { endian = big; mode = datagram; }
    message M {
        code: u8;
        match code {
            case 99 => {
                let g_start = __current_offset;
                loop items
                    while (__current_offset - g_start) < 2
                {
                    b: u8;
                }
            }
            default => { z: u8; }
        }
    }
}
"#;
    // code = 99, then loop reads two u8s (2 bytes)
    let ctx = ctx_of(src, &[99, 0xAA, 0xBB]);
    assert_eq!(*ctx.get("code").unwrap_or(&0), 99);
    // Loop-body fields are registered under `<loopname>.<field>`; the slot is
    // reused across iterations so it holds the last read value (0xBB).
    assert_eq!(
        *ctx.get("items.b").unwrap_or(&0),
        0xBB,
        "loop body field inside match arm"
    );
}

#[test]
fn bind_dispatches_in_protocol_layer() {
    let src = r#"
protocol P {
    meta { endian = big; mode = datagram; }
    message Outer {
        tag: u8;
        payload: bytes[..];
    }
    message Inner {
        x: u8;
        y: u8;
    }
    bind Inner payload to Outer when tag == 1;
}
"#;
    // tag = 1 -> bind fires; Inner parsed from payload tail [0xAA, 0xBB]
    let (_proto, ctx, _final, evs) = parse_and_run_with_data(src, &[1, 0xAA, 0xBB]).expect("run");
    assert_eq!(*ctx.get("tag").unwrap_or(&0), 1);
    assert_eq!(
        *ctx.get("Inner.x").unwrap_or(&0),
        0xAA,
        "bound layer field x"
    );
    assert_eq!(
        *ctx.get("Inner.y").unwrap_or(&0),
        0xBB,
        "bound layer field y"
    );
    assert!(
        evs.iter()
            .any(|e| matches!(e, Event::Message { msg_type, .. } if msg_type == "Inner")),
        "expected a Message event for the dispatched layer, got: {:?}",
        evs
    );

    // tag = 2 -> bind condition false; no Inner dispatch
    let (_proto2, ctx2, _f2, evs2) = parse_and_run_with_data(src, &[2, 0xAA, 0xBB]).expect("run2");
    assert!(
        !ctx2.contains_key("Inner.x"),
        "Inner should not be dispatched when when-condition is false"
    );
    assert!(
        !evs2
            .iter()
            .any(|e| matches!(e, Event::Message { msg_type, .. } if msg_type == "Inner")),
        "no Inner event expected when bind condition is false"
    );
}

#[test]
fn stream_reassembly_handles_ooo_segments() {
    // A simple stream-mode protocol: two u16 fields then a byte.
    let src = r#"
protocol P {
    meta { endian = big; mode = stream; }
    message Pkt {
        a: u16;
        b: u16;
        c: u8;
    }
}
"#;
    // Send segments OUT OF ORDER; Reassembler must reconstruct the byte stream
    // [00 01 02 03 04] before parsing. base_seq = 1000.
    let segs: Vec<(u32, Vec<u8>)> = vec![
        (1004, vec![0x04]),
        (1000, vec![0x00, 0x01]),
        (1002, vec![0x02, 0x03]),
    ];
    let (_proto, ctx, _final, _evs) = parse_and_run_stream(src, 1000, &segs).expect("stream run");
    assert_eq!(*ctx.get("a").unwrap_or(&0), 0x0001);
    assert_eq!(*ctx.get("b").unwrap_or(&0), 0x0203);
    assert_eq!(*ctx.get("c").unwrap_or(&0), 0x04);
}
