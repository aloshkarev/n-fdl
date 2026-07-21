//! Stream NeedMoreBytes / continuation API (Task 25).

use nfdl_runtime::{parse_stream_start, resume, StreamParseStep};

fn tiny_stream_proto() -> &'static str {
    r#"
protocol Tiny {
    meta {
        endian = big;
        mode   = stream;
    }
    message Hdr {
        a: u8;
        b: u8;
        c: u16;
    }
}
"#
}

#[test]
fn stream_start_yields_continuation_not_bare_error() {
    let step = parse_stream_start(tiny_stream_proto(), &[0x11]).expect("start");
    match step {
        StreamParseStep::NeedMoreBytes {
            needed,
            continuation,
        } => {
            assert!(needed >= 1);
            assert_eq!(continuation.consumed(), 1);
            // Compat: bare NeedMoreBytes still available via adapter.
            let err = StreamParseStep::NeedMoreBytes {
                needed,
                continuation,
            }
            .into_complete()
            .unwrap_err();
            assert!(matches!(
                err,
                nfdl_runtime::RuntimeError::NeedMoreBytes
            ));
        }
        StreamParseStep::Done { .. } => panic!("expected NeedMoreBytes"),
    }
}

#[test]
fn resume_equivalence_split_vs_whole() {
    let src = tiny_stream_proto();
    let whole = [0x11u8, 0x22, 0x33, 0x44];

    let baseline = match parse_stream_start(src, &whole).expect("whole") {
        StreamParseStep::Done { ctx, .. } => ctx,
        StreamParseStep::NeedMoreBytes { .. } => panic!("whole buffer should complete"),
    };

    let cont = match parse_stream_start(src, &whole[..1]).expect("split start") {
        StreamParseStep::NeedMoreBytes { continuation, .. } => *continuation,
        StreamParseStep::Done { .. } => panic!("expected NeedMoreBytes"),
    };
    let split = match resume(cont, &whole[1..]).expect("resume") {
        StreamParseStep::Done { ctx, .. } => ctx,
        StreamParseStep::NeedMoreBytes { .. } => panic!("resume should complete"),
    };

    assert_eq!(split.get("a"), baseline.get("a"));
    assert_eq!(split.get("b"), baseline.get("b"));
    assert_eq!(split.get("c"), baseline.get("c"));
    assert_eq!(split.get("a"), Some(&0x11));
    assert_eq!(split.get("b"), Some(&0x22));
    assert_eq!(split.get("c"), Some(&0x3344));
}

#[test]
fn bare_need_more_bytes_still_displayable() {
    let e = nfdl_runtime::RuntimeError::NeedMoreBytes;
    assert_eq!(e.to_string(), "need more bytes");
}
