//! Golden hex harness (M0 ARP) — `docs/spec/12-testing.md` §1.2.
//!
//! Loads `docs/examples/arp.nfdl`, runs `tests/golden/arp/input.hex`, and
//! compares the runner JSON projection to `expected.json`.

use nfdl_runtime::{Event, parse_and_run_with_data};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

fn golden_arp_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/arp")
}

/// Parse `input.hex`: whitespace-separated hex bytes; `#` starts a line comment.
fn parse_hex_file(src: &str) -> Vec<u8> {
    let mut out = Vec::new();
    for line in src.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        for tok in line.split_whitespace() {
            let b = u8::from_str_radix(tok, 16)
                .unwrap_or_else(|e| panic!("invalid hex byte `{tok}`: {e}"));
            out.push(b);
        }
    }
    out
}

fn event_to_json(ev: &Event) -> Value {
    match ev {
        Event::Message { msg_type, size } => json!({
            "type": "Message",
            "msg_type": msg_type,
            "size": size,
        }),
        Event::FsmTransition { from, to, machine } => json!({
            "type": "FsmTransition",
            "from": from,
            "to": to,
            "machine": machine,
        }),
        Event::Emit { name } => json!({
            "type": "Emit",
            "name": name,
        }),
        Event::SessionExpired { key_hash } => json!({
            "type": "SessionExpired",
            "key_hash": key_hash,
        }),
        Event::Diagnostic { code, message } => json!({
            "type": "Diagnostic",
            "code": code,
            "message": message,
        }),
        Event::Anomaly { kind } => json!({
            "type": "Anomaly",
            "kind": kind,
        }),
    }
}

fn run_to_json(src: &str, data: &[u8]) -> Value {
    match parse_and_run_with_data(src, data) {
        Ok((proto, ctx, final_state, events)) => {
            let fields: BTreeMap<&String, u64> = ctx.iter().map(|(k, v)| (k, *v)).collect();
            let events: Vec<Value> = events.iter().map(event_to_json).collect();
            let consumed = ctx.get("__current_offset").copied().unwrap_or(0);
            json!({
                "ok": true,
                "protocol": proto.name,
                "final_state": final_state,
                "consumed": consumed,
                "fields": fields,
                "events": events,
            })
        }
        Err(e) => json!({
            "ok": false,
            "error": format!("{e:?}"),
        }),
    }
}

#[test]
fn arp_hex_matches_expected_json() {
    let dir = golden_arp_dir();
    let hex_src = fs::read_to_string(dir.join("input.hex")).expect("read input.hex");
    let expected_src =
        fs::read_to_string(dir.join("expected.json")).expect("read expected.json");
    let expected: Value = serde_json::from_str(&expected_src).expect("parse expected.json");

    let pkt = parse_hex_file(&hex_src);
    assert_eq!(pkt.len(), 28, "ARP request must be 28 bytes");

    let nfdl = include_str!("../../../docs/examples/arp.nfdl");
    let actual = run_to_json(nfdl, &pkt);

    assert_eq!(
        actual, expected,
        "ARP golden mismatch.\nactual:\n{}\nexpected:\n{}",
        serde_json::to_string_pretty(&actual).unwrap(),
        serde_json::to_string_pretty(&expected).unwrap()
    );
}

#[test]
fn parse_hex_file_strips_comments() {
    let bytes = parse_hex_file("AA # comment\nBB CC\n");
    assert_eq!(bytes, vec![0xAA, 0xBB, 0xCC]);
}
