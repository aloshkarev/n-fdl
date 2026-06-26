//! v1 property-based tests

use nfdl_syntax::{Lexer, Parser, Token};
use proptest::prelude::*;

fn parse_file(src: &str) -> Result<usize, String> {
    let mut lex = Lexer::new(src);
    let mut tokens = 0;
    loop {
        if lex.next_token() == Token::Eof {
            break;
        }
        tokens += 1;
    }
    let mut p = Parser::new(src);
    p.parse_protocol().map_err(|e| format!("{:?}", e))?;
    Ok(tokens)
}

proptest! {
    #[test]
    fn all_protocols_no_panic_conservation(_ in 0..50u32) {
        let files = [
            include_str!("../../../docs/examples/arp.nfdl"),
            include_str!("../../../docs/examples/udp_dns.nfdl"),
            include_str!("../../../docs/examples/tcp.nfdl"),
            include_str!("../../../docs/examples/radius.nfdl"),
            include_str!("../../../docs/examples/diameter.nfdl"),
            include_str!("../../../docs/examples/gtpu.nfdl"),
        ];
        for src in files {
            let tokens = parse_file(src).expect("parse failed");
            prop_assert!(tokens > 50);
        }
    }

    #[test]
    fn deterministic_parse(_ in 0..20u32) {
        let src = include_str!("../../../docs/examples/arp.nfdl");
        let t1 = parse_file(src).unwrap();
        let t2 = parse_file(src).unwrap();
        prop_assert_eq!(t1, t2);
    }
}
