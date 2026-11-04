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
        if let Some(cond) = &field.conditional
            && expr_has_modulo_padding(cond)
        {
            has_padding = true;
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
    if !stripped.len().is_multiple_of(2) {
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
         nfdl-cli run <file> --hex <hex> [--hex <hex>...]  parse + run\n  \
         nfdl-cli validate <file>             parse + verify bounds diagnostics\n  \
         nfdl-cli dump <file>                 parse + dump bytecode program"
    );
}

fn limit_cap(msg: &str) -> &'static str {
    if msg.contains("instruction") {
        "max_instructions"
    } else if msg.contains("loop") {
        "max_loop_iterations"
    } else {
        "limit"
    }
}

fn partial_line(err: &nfdl_runtime::RuntimeError) -> Option<String> {
    match err {
        nfdl_runtime::RuntimeError::Partial { cap, kept, dropped } => Some(format!(
            "coverage=partial cap={cap} kept={kept} dropped={dropped}"
        )),
        nfdl_runtime::RuntimeError::LimitExceeded(msg) => Some(format!(
            "coverage=partial cap={} kept=0 dropped=1",
            limit_cap(msg)
        )),
        _ => None,
    }
}

fn cmd_parse(path: &str) -> i32 {
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

            match nfdl_runtime::parse_and_run(&src) {
                Ok((_, evs)) => {
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
                    println!("Limits enforced: true");

                    println!("\n[Production AST]");
                    println!("Protocol: {} (endian={})", proto.name, proto.endian);
                    for msg in &proto.messages {
                        println!("  msg {} ({} fields)", msg.name, msg.fields.len());
                        for f in msg.fields.iter().take(5) {
                            println!("    field {}: {:?}", f.name, f.ty);
                        }
                    }
                    println!("  bytecode events: {}", evs.len());
                    0
                }
                Err(e) => {
                    if let Some(line) = partial_line(&e) {
                        println!("{line}");
                        1
                    } else {
                        eprintln!("runtime error: {e}");
                        3
                    }
                }
            }
        }
        Err(e) => {
            print_parse_error(&e);
            2
        }
    }
}

/// Run a protocol file. Returns the process status and the stdout line or
/// report. Does not exit the process. Empty bytes are `coverage=absent`.
fn cmd_run(path: &str, hexes: &[String], options: nfdl_runtime::RunOptions) -> (i32, String) {
    let src = fs::read_to_string(path).expect("read file");
    let mut packets = Vec::with_capacity(hexes.len());
    for hex in hexes {
        match parse_hex(hex) {
            Ok(bytes) => packets.push(bytes),
            Err(e) => {
                eprintln!("invalid --hex: {e}");
                return (2, String::new());
            }
        }
    }
    if packets.is_empty() || packets.iter().all(|packet| packet.is_empty()) {
        return (1, "coverage=absent".to_string());
    }
    let views: Vec<&[u8]> = packets.iter().map(Vec::as_slice).collect();
    match nfdl_runtime::parse_and_run_with_options(&src, &views, &options) {
        Ok((_proto, ctx, final_state, evs)) => {
            let nbytes: usize = packets.iter().map(Vec::len).sum();
            let mut lines = vec![
                "coverage=complete".to_string(),
                format!(
                    "run: {nbytes} bytes, final_state={final_state}, events={}",
                    evs.len()
                ),
            ];
            let mut keys: Vec<&String> = ctx.keys().collect();
            keys.sort();
            for k in keys {
                lines.push(format!("  {k} = {}", ctx[k]));
            }
            for e in &evs {
                lines.push(format!("  event: {e:?}"));
            }
            (0, lines.join("\n"))
        }
        Err(e) => {
            if let Some(line) = partial_line(&e) {
                return (1, line);
            }
            eprintln!("runtime error: {e}");
            (3, String::new())
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
            std::process::exit(cmd_parse(&args[2]));
        }
        "run" => {
            // run <file> --hex <hex> [--hex <hex>...] [--max-instructions N]
            let path = args.get(2).cloned();
            let hexes = flag_values(&args[3..], "--hex");
            let max_instr =
                flag_value(&args[3..], "--max-instructions").and_then(|s| s.parse::<usize>().ok());
            let mut options = nfdl_runtime::RunOptions::default();
            if let Some(n) = max_instr {
                options.limits.max_instructions = n;
                options.limits.max_loop_iterations = (n / 10).max(1000);
            }
            match path {
                Some(p) if !hexes.is_empty() => {
                    let (code, text) = cmd_run(&p, &hexes, options);
                    if !text.is_empty() {
                        println!("{text}");
                    }
                    std::process::exit(code);
                }
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
            std::process::exit(cmd_parse(other));
        }
    }
}

/// Read the value of `--flag` from an args slice (returns the following arg).
fn flag_value(args: &[String], flag: &str) -> Option<String> {
    flag_values(args, flag).into_iter().next()
}

/// Read every value of a repeated `--flag`.
fn flag_values(args: &[String], flag: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == flag
            && let Some(value) = iter.next()
        {
            values.push(value.clone());
        }
    }
    values
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_nfdl(label: &str, body: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("nfdl-cli-{label}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("proto.nfdl");
        std::fs::write(&path, body).expect("write proto");
        (dir, path)
    }

    fn assert_only_line(status: i32, text: &str, line: &str) {
        assert_eq!(status, 1, "{text}");
        assert_eq!(text, line);
        assert!(!text.contains("coverage=complete"));
        assert!(!text.contains("N-FDL v1 SUCCESS"));
        assert!(!text.contains("run:"));
        assert!(!text.contains("event:"));
    }

    #[test]
    fn empty_hex_is_coverage_absent() {
        let (dir, path) = write_nfdl("empty-read", "protocol P {\n  message M { b: u8; }\n}\n");
        let (status, text) = cmd_run(
            path.to_str().expect("utf8"),
            &[String::new()],
            nfdl_runtime::RunOptions::default(),
        );
        assert_eq!(status, 1);
        assert_eq!(text, "coverage=absent");
        let _ = std::fs::remove_dir_all(dir);

        let (dir, path) = write_nfdl("empty-noread", "protocol P {\n  message M { }\n}\n");
        let (status, text) = cmd_run(
            path.to_str().expect("utf8"),
            &[String::new()],
            nfdl_runtime::RunOptions::default(),
        );
        assert_eq!(status, 1);
        assert_eq!(text, "coverage=absent");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn depth_past_16_is_partial_on_the_run_entry() {
        let (dir, path) = write_nfdl(
            "depth",
            "protocol Deep {\n  message Node { inner: Node; }\n}\n",
        );
        let (status, text) = cmd_run(
            path.to_str().expect("utf8"),
            &["00".to_string()],
            nfdl_runtime::RunOptions::default(),
        );
        assert_only_line(
            status,
            &text,
            "coverage=partial cap=max_layer_depth kept=16 dropped=1",
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn reassembly_drop_is_partial_on_the_run_entry() {
        let (dir, path) = write_nfdl("reassembly", "protocol P {\n  message M { b: u8; }\n}\n");
        let mut options = nfdl_runtime::RunOptions::default();
        options.max_reassembly = 1;
        let (status, text) = cmd_run(path.to_str().expect("utf8"), &["0102".to_string()], options);
        assert_only_line(
            status,
            &text,
            "coverage=partial cap=reassembly kept=0 dropped=2",
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn session_eviction_is_partial_on_the_run_entry() {
        let (dir, path) = write_nfdl(
            "sessions",
            "protocol Sess {\n  \
             message M { id: u8; }\n  \
             state_machine Flow {\n    \
             key = bidir(id, 0);\n    \
             state CLOSED {\n      \
             on M -> CLOSED;\n    \
             }\n  \
             }\n}\n",
        );
        let mut options = nfdl_runtime::RunOptions::default();
        options.max_sessions = 1;
        let (status, text) = cmd_run(
            path.to_str().expect("utf8"),
            &["01".to_string(), "02".to_string()],
            options,
        );
        assert_only_line(
            status,
            &text,
            "coverage=partial cap=max_sessions kept=1 dropped=1",
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn consumed_buffer_is_coverage_complete() {
        let (dir, path) = write_nfdl("complete", "protocol P {\n  message M { b: u8; }\n}\n");
        let (status, text) = cmd_run(
            path.to_str().expect("utf8"),
            &["01".to_string()],
            nfdl_runtime::RunOptions::default(),
        );
        assert_eq!(status, 0, "{text}");
        assert!(text.starts_with("coverage=complete\n"), "{text}");
        assert!(text.contains("run: 1 bytes"), "{text}");
        assert!(text.contains("b = 1"), "{text}");
        let _ = std::fs::remove_dir_all(dir);
    }
}
