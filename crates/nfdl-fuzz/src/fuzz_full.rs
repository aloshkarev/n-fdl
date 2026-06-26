//! Full Fuzzing Target (parser + VM + reassembly)

use nfdl_syntax::{Lexer, Parser};
use nfdl_runtime::{BytecodeVm, Reassembler, protocol_to_bytecode};

pub fn fuzz_all(data: &[u8]) {
    if data.len() < 8 { return; }
    
    // Fuzz 1: Parser
    let src = std::str::from_utf8(data).unwrap_or("");
    if src.len() > 10000 { return; }
    
    let mut lex = Lexer::new(src);
    let mut tokens = 0;
    loop {
        if lex.next_token() == nfdl_syntax::Token::Eof { break; }
        tokens += 1;
        if tokens > 5000 { break; }
    }
    
    let mut p = Parser::new(src);
    if let Ok(proto) = p.parse_protocol() {
        // Fuzz 2: Bytecode VM
        let program = protocol_to_bytecode(&proto);
        let mut vm = BytecodeVm::new(program.slot_count);
        let _ = vm.run(&program);
        
        // Fuzz 3: Reassembly
        let mut r = Reassembler::new(1000);
        if data.len() > 16 {
            r.accept_segment(1000, data[0..8].to_vec());
            r.accept_segment(1008, data[8..16].to_vec());
        }
    }
}
