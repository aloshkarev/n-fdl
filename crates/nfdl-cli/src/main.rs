use nfdl_syntax::{
    Lexer, ParseError, Parser, Token,
    ast::{BinOp, Expr, Message, NfdlType, Protocol},
};
use std::env;
use std::fs;

fn expr_has_bidir(expr: &Expr) -> bool {
    match expr {
        Expr::Call { name, args } => {
            name == "bidir" || name == "bidir_tuple" || args.iter().any(expr_has_bidir)
        }
        Expr::Binary { left, right, .. } => expr_has_bidir(left) || expr_has_bidir(right),
        Expr::Unary { expr, .. } => expr_has_bidir(expr),
        Expr::Ternary {
            cond,
            then_branch,
            else_branch,
        } => expr_has_bidir(cond) || expr_has_bidir(then_branch) || expr_has_bidir(else_branch),
        Expr::Coalesce { value, default } => expr_has_bidir(value) || expr_has_bidir(default),
        Expr::Ident(_) | Expr::Int(_) => false,
    }
}

fn type_has_eof(ty: &NfdlType) -> bool {
    match ty {
        NfdlType::BytesEof | NfdlType::BytesStream => true,
        NfdlType::Bytes { len } => expr_has_modulo_padding(len),
        _ => false,
    }
}

fn expr_has_modulo_padding(expr: &Expr) -> bool {
    match expr {
        Expr::Binary { op: BinOp::Mod, .. } => true,
        Expr::Binary { left, right, .. } => {
            expr_has_modulo_padding(left) || expr_has_modulo_padding(right)
        }
        Expr::Unary { expr, .. } => expr_has_modulo_padding(expr),
        Expr::Ternary {
            cond,
            then_branch,
            else_branch,
        } => {
            expr_has_modulo_padding(cond)
                || expr_has_modulo_padding(then_branch)
                || expr_has_modulo_padding(else_branch)
        }
        Expr::Coalesce { value, default } => {
            expr_has_modulo_padding(value) || expr_has_modulo_padding(default)
        }
        Expr::Call { args, .. } => args.iter().any(expr_has_modulo_padding),
        Expr::Ident(_) | Expr::Int(_) => false,
    }
}

fn message_features(msg: &Message) -> (bool, bool, bool, bool, usize) {
    let mut has_eof = false;
    let mut has_u24 = false;
    let mut has_padding = false;
    let mut total_fields = msg.fields.len();
    let mut max_depth = 0usize;

    for field in &msg.fields {
        if type_has_eof(&field.ty) {
            has_eof = true;
        }
        if matches!(field.ty, NfdlType::U24) {
            has_u24 = true;
        }
        if field.name == "padding" {
            has_padding = true;
        }
        if let NfdlType::MessageRef(_) = field.ty {
            max_depth = max_depth.max(1);
        }
        if let Some(cond) = &field.conditional {
            if expr_has_modulo_padding(cond) {
                has_padding = true;
            }
        }
    }

    for let_bind in &msg.lets {
        if expr_has_modulo_padding(&let_bind.value) {
            has_padding = true;
        }
    }

    for loop_stmt in &msg.loops {
        total_fields += loop_stmt.body.len();
        for field in &loop_stmt.body {
            if type_has_eof(&field.ty) {
                has_eof = true;
            }
            if matches!(field.ty, NfdlType::U24) {
                has_u24 = true;
            }
            if field.name == "padding" {
                has_padding = true;
            }
            if let NfdlType::MessageRef(_) = field.ty {
                max_depth = max_depth.max(1);
            }
        }
    }

    (has_eof, has_u24, has_padding, max_depth > 0, total_fields)
}

fn protocol_summary(
    proto: &Protocol,
    src: &str,
) -> (bool, bool, bool, bool, bool, bool, usize, usize) {
    let mode_stream = proto.mode == "stream";
    let has_match = src.contains("match ");
    let mut has_eof = false;
    let mut has_u24 = false;
    let mut has_padding = false;
    let mut max_depth_seen = 0usize;
    let mut total_fields = 0usize;

    let has_bidir = proto
        .state_machines
        .iter()
        .filter_map(|sm| sm.key.as_ref())
        .any(expr_has_bidir);

    for msg in &proto.messages {
        let (eof, u24, pad, depth, fields) = message_features(msg);
        has_eof |= eof;
        has_u24 |= u24;
        has_padding |= pad;
        if depth {
            max_depth_seen = max_depth_seen.max(1);
        }
        total_fields += fields;
    }

    (
        mode_stream,
        has_eof,
        has_bidir,
        has_u24,
        has_match,
        has_padding,
        max_depth_seen,
        total_fields,
    )
}

fn print_parse_error(err: &ParseError) {
    match err {
        ParseError::Syntax(msg) => eprintln!("Syntax error: {}", msg),
        ParseError::WithLocation { msg, pos } => eprintln!("Syntax error at {}: {}", pos, msg),
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: nfdl-cli <file.nfdl>");
        std::process::exit(1);
    }
    let path = &args[1];
    let src = fs::read_to_string(path).expect("read file");

    let mut lex = Lexer::new(&src);
    let mut token_count = 0;
    loop {
        let t = lex.next_token();
        if t == Token::Eof {
            break;
        }
        token_count += 1;
    }

    let mut parser = Parser::new(&src);
    match parser.parse_protocol() {
        Ok(proto) => {
            let (
                mode_stream,
                has_eof,
                has_bidir,
                has_u24,
                has_match,
                has_padding,
                max_depth_seen,
                total_fields,
            ) = protocol_summary(&proto, &src);

            println!("N-FDL v1 SUCCESS");
            println!("File: {}", path);
            println!("Tokens: {}", token_count);
            println!("Messages: {}", proto.messages.len());
            println!("State machines: {}", proto.state_machines.len());
            println!("mode=stream: {}", mode_stream);
            println!("bytes[EOF]: {}", has_eof);
            println!("bidir: {}", has_bidir);
            println!("u24: {}", has_u24);
            println!("match: {}", has_match);
            println!("padding: {}", has_padding);
            println!("Max depth: {}", max_depth_seen);
            println!("Total fields: {}", total_fields);
            println!("Limits enforced: {}", false);

            println!("\n[Production AST]");
            println!("Protocol: {} (endian={})", proto.name, proto.endian);
            for msg in &proto.messages {
                println!("  msg {} ({} fields)", msg.name, msg.fields.len());
                for f in msg.fields.iter().take(5) {
                    println!("    field {}: {:?}", f.name, f.ty);
                }
            }

            match nfdl_runtime::parse_and_run(&src) {
                Ok((_, evs)) => println!("  bytecode events: {}", evs.len()),
                Err(e) => eprintln!("  runtime error: {}", e),
            }
        }
        Err(e) => {
            print_parse_error(&e);
            std::process::exit(2);
        }
    }
}
