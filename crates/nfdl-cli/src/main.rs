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
        Expr::Tuple(elems) => elems.iter().any(expr_has_bidir),
        Expr::Field(base, _) => expr_has_bidir(base),
        Expr::Ident(_) | Expr::Int(_) | Expr::Str(_) => false,
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
        Expr::Tuple(elems) => elems.iter().any(expr_has_modulo_padding),
        Expr::Field(base, _) => expr_has_modulo_padding(base),
        Expr::Ident(_) | Expr::Int(_) | Expr::Str(_) => false,
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

/// Parse a hex string (with optional `0x`, whitespace, and `_` separators) into bytes.
fn parse_hex(s: &str) -> Result<Vec<u8>, String> {
    let cleaned: String = s
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '_')
        .collect();
    let stripped = cleaned.strip_prefix("0x").unwrap_or(&cleaned);
    if stripped.len() % 2 != 0 {
        return Err(format!("hex string has odd length ({})", stripped.len()));
    }
    (0..stripped.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&stripped[i..i + 2], 16).map_err(|e| format!("bad hex byte: {e}"))
        })
        .collect()
}

fn usage() {
    eprintln!(
        "Usage:\n  \
         nfdl-cli <file>                      parse + summary (default)\n  \
         nfdl-cli parse <file>                parse + summary\n  \
         nfdl-cli run <file> --hex <hex>      parse + run with hex bytes\n  \
         nfdl-cli validate <file>             parse + verify bounds diagnostics\n  \
         nfdl-cli dump <file>                 parse + dump bytecode program"
    );
}

fn cmd_parse(path: &str) -> Result<Protocol, ()> {
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
            println!("Binds: {}", proto.binds.len());
            for b in &proto.binds {
                println!(
                    "  bind {} payload={} source={} when={:?}",
                    b.layer, b.field, b.source, b.when
                );
            }
            println!("mode=stream: {}", mode_stream);
            println!("bytes[EOF]: {}", has_eof);
            println!("bidir: {}", has_bidir);
            println!("u24: {}", has_u24);
            println!("match: {}", has_match);
            println!("padding: {}", has_padding);
            println!("Max depth: {}", max_depth_seen);
            println!("Total fields: {}", total_fields);
            println!("Limits enforced: {}", true);

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
            Ok(proto)
        }
        Err(e) => {
            print_parse_error(&e);
            Err(())
        }
    }
}

fn cmd_run(path: &str, hex: &str, max_instr: Option<usize>) {
    let src = fs::read_to_string(path).expect("read file");
    let data = match parse_hex(hex) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("invalid --hex: {e}");
            std::process::exit(2);
        }
    };
    let limits = match max_instr {
        Some(n) => nfdl_runtime::Limits {
            max_instructions: n,
            max_loop_iterations: (n / 10).max(1000),
        },
        None => nfdl_runtime::Limits::default(),
    };
    match nfdl_runtime::parse_and_run_with_data_and_limits(&src, &data, limits) {
        Ok((_proto, ctx, final_state, evs)) => {
            println!(
                "run: {} bytes, final_state={}, events={}",
                data.len(),
                final_state,
                evs.len()
            );
            let mut keys: Vec<&String> = ctx.keys().collect();
            keys.sort();
            for k in keys {
                println!("  {} = {}", k, ctx[k]);
            }
            for e in &evs {
                println!("  event: {:?}", e);
            }
        }
        Err(e) => {
            eprintln!("runtime error: {e}");
            std::process::exit(3);
        }
    }
}

fn cmd_validate(path: &str) {
    let src = fs::read_to_string(path).expect("read file");
    let proto = match Parser::new(&src).parse_protocol() {
        Ok(p) => p,
        Err(e) => {
            print_parse_error(&e);
            std::process::exit(2);
        }
    };
    let buf = nfdl_verify::verify_protocol(&proto);
    if buf.is_empty() {
        println!("validate: OK (no diagnostics)");
    } else {
        print!("{}", buf.render(&src, path));
        if buf.has_errors() {
            std::process::exit(4);
        }
    }
}

fn cmd_dump(path: &str) {
    let src = fs::read_to_string(path).expect("read file");
    let proto = match Parser::new(&src).parse_protocol() {
        Ok(p) => p,
        Err(e) => {
            print_parse_error(&e);
            std::process::exit(2);
        }
    };
    // Compile only the root message (matching runner semantics).
    let root = nfdl_runtime::runner::root_message_name(&proto);
    let (program, field_map) = nfdl_runtime::integration::protocol_to_bytecode_with_map(&proto);
    println!(
        "dump: root={}, slots={}, instructions={}",
        root,
        program.slot_count,
        program.instructions.len()
    );
    for (i, ins) in program.instructions.iter().enumerate() {
        println!("  {:>4}: {:?}", i, ins);
    }
    println!("field_map:");
    let mut entries: Vec<(&String, &u16)> = field_map.iter().collect();
    entries.sort_by_key(|(_, s)| *s);
    for (name, &slot) in entries {
        println!("  slot {:>3} = {}", slot, name);
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        usage();
        std::process::exit(1);
    }

    // Subcommand dispatch. A bare file path (no recognized subcommand) keeps the
    // original `parse + summary` behavior so existing `nfdl-cli <file>` runs work.
    match args[1].as_str() {
        "parse" => {
            if args.len() < 3 {
                usage();
                std::process::exit(1);
            }
            let _ = cmd_parse(&args[2]);
        }
        "run" => {
            // run <file> --hex <hex> [--max-instructions N]
            let path = args.get(2).cloned();
            let hex = flag_value(&args[3..], "--hex");
            let max_instr =
                flag_value(&args[3..], "--max-instructions").and_then(|s| s.parse::<usize>().ok());
            match (path, hex) {
                (Some(p), Some(h)) => cmd_run(&p, &h, max_instr),
                _ => {
                    usage();
                    std::process::exit(1);
                }
            }
        }
        "validate" => {
            if args.len() < 3 {
                usage();
                std::process::exit(1);
            }
            cmd_validate(&args[2]);
        }
        "dump" => {
            if args.len() < 3 {
                usage();
                std::process::exit(1);
            }
            cmd_dump(&args[2]);
        }
        // Default: treat arg[1] as a file path (legacy behavior).
        other => {
            if other.starts_with('-') {
                usage();
                std::process::exit(1);
            }
            let _ = cmd_parse(other);
        }
    }
}

/// Read the value of `--flag` from an args slice (returns the following arg).
fn flag_value(args: &[String], flag: &str) -> Option<String> {
    let mut iter = args.iter();
    while let Some(a) = iter.next() {
        if a == flag {
            return iter.next().cloned();
        }
    }
    None
}
