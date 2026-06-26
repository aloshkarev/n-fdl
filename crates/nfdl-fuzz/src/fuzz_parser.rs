//! Simple fuzz target for parser (M6)
//! Run with: cargo run --bin fuzz_parser

use nfdl_syntax::{Lexer, Parser};
use std::env;
use std::fs;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: fuzz_parser <file.nfdl>");
        std::process::exit(1);
    }

    let src = fs::read_to_string(&args[1]).expect("read");

    // Fuzz 1: Lexer should never panic
    let mut lex = Lexer::new(&src);
    let mut count = 0;
    loop {
        let t = lex.next_token();
        count += 1;
        if t == nfdl_syntax::Token::Eof {
            break;
        }
        if count > 10000 {
            break;
        } // safety
    }

    // Fuzz 2: Parser should never panic
    let mut p = Parser::new(&src);
    let _ = p.parse_protocol();

    println!("Fuzz passed: {} tokens processed, no panic", count);
}
