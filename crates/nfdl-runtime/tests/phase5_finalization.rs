//! Phase 5 finalization regression tests: locks in two correctness fixes
//! found while running the example suite end-to-end.
//!
//! 1. EFSM initial state: a fresh flow must start at the machine's declared
//!    `initial` state (e.g. TCP `CLOSED`), not the generic `IDLE`, otherwise
//!    the first transition never matches.
//! 2. `MessageRef` lexical scoping + phantom-slot filtering: a `match` whose
//!    case arm recursively inlines a message (diameter grouped AVP) must not
//!    leak its `let` bindings to the sibling `default` arm, and the output
//!    context must not contain slots for recursion levels the data never
//!    reached.

use nfdl_runtime::{Limits, parse_and_run_with_data_and_limits};

fn hex_to_bytes(hex: &str) -> Vec<u8> {
    let s: String = hex.chars().filter(|c| !c.is_whitespace()).collect();
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

/// TCP SYN segment: 20-byte header (data_offset=5), SYN flag (0x02), 5-byte
/// payload. The `Connection` machine starts in `CLOSED`; a SYN with is_ack==0
/// must transition to `SYN_SENT` and emit `TCP_SYN_SEEN`.
#[test]
fn fsm_initial_state_is_machine_initial_not_idle() {
    let src = include_str!("../../../docs/examples/tcp.nfdl");
    let pkt = hex_to_bytes(
        "04d2 0050 00000000 00000000 5002 2000 0000 0000 48454c4c4f",
    );
    let (_proto, ctx, final_state, evs) =
        parse_and_run_with_data_and_limits(src, &pkt, Limits::default()).expect("tcp run");

    assert_eq!(*ctx.get("is_syn").unwrap_or(&0), 1, "is_syn parsed");
    assert_eq!(*ctx.get("is_ack").unwrap_or(&0), 0, "is_ack parsed");
    assert_eq!(final_state, "SYN_SENT", "fresh flow transitions from CLOSED");
    assert!(
        evs.iter().any(|e| matches!(
            e,
            nfdl_runtime::Event::Emit { name } if name == "TCP_SYN_SEEN"
        )),
        "SYN_SEEN event emitted: {evs:?}"
    );
}

/// 32-byte Diameter message: 20-byte header + one non-grouped AVP (code=1,
/// length=12, 4-byte payload 0xAABBCCDD). Before the scoping fix, the
/// `default => data: bytes[payload_len]` arm resolved `payload_len` to a
/// phantom nested slot (0), read 0 bytes, the AVP loop over-iterated, and the
/// next iteration's `validate length >= 8` failed.
#[test]
fn diameter_avp_with_payload_consumes_full_avp() {
    let src = include_str!("../../../docs/examples/diameter.nfdl");
    let pkt = hex_to_bytes(
        "0100002000000000000000000000000000000000000000010000000caabbccdd",
    );
    let (_proto, ctx, _state, _evs) =
        parse_and_run_with_data_and_limits(src, &pkt, Limits::default()).expect("diameter run");

    assert_eq!(*ctx.get("__current_offset").unwrap_or(&0), 32, "full consumption");
    assert_eq!(*ctx.get("avps.a.code").unwrap_or(&0), 1, "avp code");
    assert_eq!(*ctx.get("avps.a.length").unwrap_or(&0), 12, "avp length");
    assert_eq!(
        *ctx.get("avps.a.data").unwrap_or(&0),
        0xAABBCCDD,
        "avp payload (default arm read correct payload_len)"
    );
}

/// The recursive `284 => grouped_avps: AVP[]` arm unrolls 8 levels deep at
/// compile time, registering slots for every level. With no grouped AVP in the
/// data, those nested slots are never written and must NOT appear in the
/// output context.
#[test]
fn diameter_context_excludes_phantom_nested_avps() {
    let src = include_str!("../../../docs/examples/diameter.nfdl");
    let pkt = hex_to_bytes(
        "0100002000000000000000000000000000000000000000010000000caabbccdd",
    );
    let (_proto, ctx, _state, _evs) =
        parse_and_run_with_data_and_limits(src, &pkt, Limits::default()).expect("diameter run");

    assert!(
        !ctx.contains_key("avps.a.grouped.inner.code"),
        "phantom nested AVP field leaked into context: {:?}",
        ctx.keys().collect::<Vec<_>>()
    );
    assert!(
        !ctx.contains_key("avps.a.grouped.inner.payload_len"),
        "phantom nested let leaked into context"
    );
}

/// Minimal scoping unit test: a message with a `let`, then a `match` whose
/// first arm inlines a message (introducing a deeper `let` of the same name)
/// and whose `default` arm references the outer `let`. The default arm must
/// see the OUTER value, proving `var_slots` is scoped per `MessageRef` inlining.
#[test]
fn messageref_let_does_not_leak_to_sibling_match_arm() {
    let src = r#"
        protocol Scope {
            meta { endian = big; mode = datagram; }
            message Leaf {
                let depth = 7;
                tag: u8;
                body: bytes[depth] if tag == 99;
            }
            message Root {
                let depth = 2;
                kind: u8;
                match kind {
                    case 99 => {
                        inner: Leaf;
                    }
                    default => {
                        rest: bytes[depth];
                    }
                }
            }
        }
    "#;
    // kind = 5 (not 99) -> default arm reads bytes[depth]; depth must be 2 (Root),
    // not 7 (Leaf, which was inlined while emitting the `case 99` arm first).
    let pkt = hex_to_bytes("05 aabb ffff");
    let (_proto, ctx, _state, _evs) =
        parse_and_run_with_data_and_limits(src, &pkt, Limits::default()).expect("scope run");

    // rest = bytes[2] = 0xAABB; the third byte (0xFF) is left unconsumed.
    assert_eq!(
        *ctx.get("rest").unwrap_or(&0),
        0xAABB,
        "default arm used outer depth=2, not leaked Leaf depth=7"
    );
    assert_eq!(*ctx.get("__current_offset").unwrap_or(&0), 3, "consumed kind + 2 bytes");
}
