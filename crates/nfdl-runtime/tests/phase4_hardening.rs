//! Phase 4 hardening tests: golden harness, conservation-of-bytes property,
//! configurable `Limits` enforcement, and bounds-verifier diagnostics.

use nfdl_runtime::{Limits, parse_and_run_with_data_and_limits};

/// A canonical 28-byte Ethernet/IPv4 ARP request (who-has 10.0.0.2 tell 10.0.0.1).
/// htype=1, ptype=0x0800, hlen=6, plen=4, op=1, sha=00:11:22:33:44:55,
/// spa=10.0.0.1, tha=00:00:00:00:00:00, tpa=10.0.0.2.
const ARP_HEX: &str = "0001 0800 06 04 0001 001122334455 0a000001 000000000000 0a000002";

fn arp_bytes() -> Vec<u8> {
    let s: String = ARP_HEX.chars().filter(|c| !c.is_whitespace()).collect();
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

fn arp_src() -> &'static str {
    include_str!("../../../docs/examples/arp.nfdl")
}

#[test]
fn golden_arp_hex_matches_expected_fields() {
    let (_proto, ctx, _state, _evs) =
        parse_and_run_with_data_and_limits(arp_src(), &arp_bytes(), Limits::default())
            .expect("arp run");
    // Field-by-field golden values.
    assert_eq!(*ctx.get("hw_type").unwrap_or(&0), 1, "htype");
    assert_eq!(*ctx.get("proto_type").unwrap_or(&0), 0x0800, "ptype");
    assert_eq!(*ctx.get("hw_len").unwrap_or(&0), 6, "hlen");
    assert_eq!(*ctx.get("proto_len").unwrap_or(&0), 4, "plen");
    assert_eq!(*ctx.get("opcode").unwrap_or(&0), 1, "op");
    assert_eq!(*ctx.get("sender_ip").unwrap_or(&0), 0x0a000001, "spa");
    assert_eq!(*ctx.get("target_ip").unwrap_or(&0), 0x0a000002, "tpa");
    // 6-byte MACs are read into u64 slots big-endian.
    assert_eq!(*ctx.get("sender_mac").unwrap_or(&0), 0x001122334455, "sha");
    assert_eq!(*ctx.get("target_mac").unwrap_or(&0), 0, "tha (all zero)");
}

#[test]
fn conservation_of_bytes_offset_equals_input_len() {
    // For a fixed-layout protocol (no terminal bytes[EOF]), a well-formed packet
    // must be fully consumed: __current_offset == input.len().
    let (_proto, ctx, _state, _evs) =
        parse_and_run_with_data_and_limits(arp_src(), &arp_bytes(), Limits::default())
            .expect("arp run");
    let off = *ctx.get("__current_offset").unwrap_or(&0) as usize;
    assert_eq!(off, arp_bytes().len(), "all input bytes accounted for");
}

#[test]
fn limits_enforced_tiny_max_instructions_aborts() {
    // With a max_instructions budget smaller than the program needs, the VM must
    // abort with LimitExceeded rather than running forever or truncating silently.
    let res = parse_and_run_with_data_and_limits(
        arp_src(),
        &arp_bytes(),
        Limits {
            max_instructions: 5,
            max_loop_iterations: 1000,
        },
    );
    match res {
        Err(nfdl_runtime::RuntimeError::LimitExceeded(msg)) => {
            assert!(
                msg.contains("instruction"),
                "expected instruction-limit message, got: {msg}"
            );
        }
        other => panic!("expected LimitExceeded, got: {:?}", other.map(|_| "Ok")),
    }
}
