//! Unified N-FDL + ADGL CLI (`parse` / `fmt` / `lint` / `check` / `verify` / `run`).
//!
//! - `check` — parse (+ ADGL `include` expand) and style lint only
//! - `verify` — ADGL semantic AOT verify (`airpulse_dsl-verify`); not the same as `check`

#![forbid(unsafe_code)]

use airpulse_dsl_syntax::{RuleDecl, load_ruleset};
use airpulse_dsl_verify::{render_diagnostics, verify_path};
use ndsl_clippy::{LintLevel, LintStore, RenderFormat, render};
use ndsl_fmt::{FormatError, format_adgl_source, format_nfdl_source};
use nfdl_syntax::{ParseError, Parser};
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lang {
    Nfdl,
    Adgl,
}

fn usage() {
    eprintln!(
        "Usage:\n  \
         ndsl-cli parse <file>\n  \
         ndsl-cli fmt [--check|--write] <paths...>\n  \
         ndsl-cli lint [--json] [--allow ID] [--deny ID] <paths...>\n  \
         ndsl-cli check <paths...>          # parse (+ ADGL include) + style lint\n  \
         ndsl-cli verify <adgl-paths...>    # ADGL semantic AOT verify (not lint)\n  \
         ndsl-cli run <nfdl> [hex]\n\n  \
         Note: `check` is not semantic verify — use `verify` for airpulse_dsl-verify."
    );
}

fn detect_lang(path: &Path) -> Option<Lang> {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .as_deref()
    {
        Some("nfdl") => Some(Lang::Nfdl),
        Some("adgl") => Some(Lang::Adgl),
        _ => None,
    }
}

fn print_nfdl_parse_error(err: &ParseError) {
    match err {
        ParseError::Syntax(msg) => eprintln!("parse error: {msg}"),
        ParseError::WithLocation { msg, pos } => {
            eprintln!("parse error at byte {pos}: {msg}");
        }
    }
}

fn print_format_error(path: &Path, src: &str, err: &FormatError) {
    match err {
        FormatError::Nfdl(e) => {
            eprint!("{}: ", path.display());
            print_nfdl_parse_error(e);
        }
        FormatError::Adgl(buf) => {
            let rendered = buf.render(src, &path.display().to_string());
            if rendered.is_empty() {
                eprintln!(
                    "{}: ADGL parse failed ({} diagnostic(s))",
                    path.display(),
                    buf.len()
                );
            } else {
                eprint!("{rendered}");
            }
        }
    }
}

fn cmd_parse(path: &Path) -> i32 {
    let lang = match detect_lang(path) {
        Some(l) => l,
        None => {
            eprintln!(
                "error: unsupported extension for {} (expected .nfdl or .adgl)",
                path.display()
            );
            return 2;
        }
    };

    match lang {
        Lang::Nfdl => {
            let src = match fs::read_to_string(path) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("error: cannot read {}: {e}", path.display());
                    return 2;
                }
            };
            match Parser::new(&src).parse_protocol() {
                Ok(proto) => {
                    println!(
                        "ok: nfdl {} (messages={}, state_machines={})",
                        proto.name,
                        proto.messages.len(),
                        proto.state_machines.len()
                    );
                    0
                }
                Err(e) => {
                    print_nfdl_parse_error(&e);
                    1
                }
            }
        }
        Lang::Adgl => {
            // Prefer path loader so leading `include` expands (parse_ruleset alone rejects it).
            let loaded = match load_ruleset(path) {
                Ok(l) => l,
                Err(buf) => {
                    let rendered = buf.render("", &path.display().to_string());
                    if rendered.is_empty() {
                        eprintln!(
                            "{}: ADGL load failed ({} diagnostic(s))",
                            path.display(),
                            buf.len()
                        );
                        for d in buf.iter() {
                            eprintln!("{}: {}: {}", d.code, d.severity, d.message);
                        }
                    } else {
                        eprint!("{rendered}");
                    }
                    return 1;
                }
            };
            match loaded.parse() {
                Ok(ruleset) => {
                    let evidence = ruleset
                        .rules
                        .iter()
                        .filter(|r| matches!(r, RuleDecl::Evidence(_)))
                        .count();
                    let decisions = ruleset.rules.len().saturating_sub(evidence);
                    println!(
                        "ok: adgl {} (rules={}, evidence={}, decisions={}, files={})",
                        ruleset.name.value,
                        ruleset.rules.len(),
                        evidence,
                        decisions,
                        loaded.files.len()
                    );
                    0
                }
                Err(buf) => {
                    let rendered = buf.render(&loaded.source, &path.display().to_string());
                    if rendered.is_empty() {
                        eprintln!(
                            "{}: ADGL parse failed ({} diagnostic(s))",
                            path.display(),
                            buf.len()
                        );
                    } else {
                        eprint!("{rendered}");
                    }
                    1
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FmtMode {
    Print,
    Check,
    Write,
}

fn cmd_fmt(mode: FmtMode, paths: &[PathBuf]) -> i32 {
    if paths.is_empty() {
        usage();
        return 1;
    }

    let mut exit = 0i32;
    for path in paths {
        let lang = match detect_lang(path) {
            Some(l) => l,
            None => {
                eprintln!(
                    "error: unsupported extension for {} (expected .nfdl or .adgl)",
                    path.display()
                );
                exit = 2;
                continue;
            }
        };

        let src = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("error: cannot read {}: {e}", path.display());
                exit = 2;
                continue;
            }
        };

        let formatted = match lang {
            Lang::Nfdl => format_nfdl_source(&src),
            Lang::Adgl => format_adgl_source(&src),
        };

        let formatted = match formatted {
            Ok(s) => s,
            Err(e) => {
                print_format_error(path, &src, &e);
                exit = 1;
                continue;
            }
        };

        let changed = formatted != src;
        match mode {
            FmtMode::Print => {
                if paths.len() > 1 {
                    println!("// === {} ===", path.display());
                }
                print!("{formatted}");
                if !formatted.ends_with('\n') {
                    println!();
                }
            }
            FmtMode::Check => {
                if changed {
                    eprintln!("would reformat: {}", path.display());
                    exit = 1;
                }
            }
            FmtMode::Write => {
                if changed {
                    if let Err(e) = fs::write(path, &formatted) {
                        eprintln!("error: cannot write {}: {e}", path.display());
                        exit = 2;
                    } else {
                        println!("reformatted: {}", path.display());
                    }
                }
            }
        }
    }
    exit
}

fn cmd_lint(paths: &[PathBuf], format: RenderFormat, allow: &[String], deny: &[String]) -> i32 {
    if paths.is_empty() {
        usage();
        return 1;
    }

    let mut store = LintStore::new();
    store.register_builtin();

    for id in allow {
        if let Err(e) = store.set_level(id, LintLevel::Allow) {
            eprintln!("error: {e}");
            return 2;
        }
    }
    for id in deny {
        if let Err(e) = store.set_level(id, LintLevel::Deny) {
            eprintln!("error: {e}");
            return 2;
        }
    }

    let diagnostics = match store.lint_paths(paths) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("error: {e}");
            return 2;
        }
    };

    let deny_count = match format {
        RenderFormat::Human => {
            // Human reports go to stderr (ariadne style); keep stdout free for piping.
            match render(&diagnostics, RenderFormat::Human, io::stderr()) {
                Ok(n) => n,
                Err(e) => {
                    eprintln!("error: render failed: {e}");
                    return 2;
                }
            }
        }
        RenderFormat::Json => match render(&diagnostics, RenderFormat::Json, io::stdout()) {
            Ok(n) => n,
            Err(e) => {
                eprintln!("error: render failed: {e}");
                return 2;
            }
        },
    };

    if deny_count > 0 || LintStore::has_deny(&diagnostics) {
        return 1;
    }
    if diagnostics.is_empty() && format == RenderFormat::Human {
        let _ = writeln!(io::stderr(), "lint: ok ({} path(s))", paths.len());
    }
    0
}

fn cmd_check(paths: &[PathBuf]) -> i32 {
    if paths.is_empty() {
        usage();
        return 1;
    }

    let mut exit = 0i32;
    for path in paths {
        let code = cmd_parse(path);
        if code != 0 {
            exit = code;
            continue;
        }
        let mut store = LintStore::new();
        store.register_builtin();
        match store.lint_paths(std::slice::from_ref(path)) {
            Ok(diags) if LintStore::has_deny(&diags) => {
                let _ = render(&diags, RenderFormat::Human, io::stderr());
                exit = 1;
            }
            Ok(diags) => {
                if !diags.is_empty() {
                    let _ = render(&diags, RenderFormat::Human, io::stderr());
                }
            }
            Err(e) => {
                eprintln!("error: {e}");
                exit = 2;
            }
        }
    }
    if exit == 0 {
        println!(
            "check: ok ({} path(s); parse+lint only — use `verify` for ADGL semantics)",
            paths.len()
        );
    }
    exit
}

/// ADGL semantic AOT verify (`airpulse_dsl-verify`), including `include` expansion.
fn cmd_verify(paths: &[PathBuf]) -> i32 {
    if paths.is_empty() {
        usage();
        return 1;
    }

    let mut exit = 0i32;
    for path in paths {
        if detect_lang(path) != Some(Lang::Adgl) {
            eprintln!("error: verify expects .adgl files, got {}", path.display());
            exit = 2;
            continue;
        }
        match verify_path(path) {
            Ok(verified) => {
                println!(
                    "ok: verify {} (rules={})",
                    path.display(),
                    verified.image.rules.len()
                );
            }
            Err(buf) => {
                // Prefer composed source when load succeeded far enough; fall back to disk.
                let src = load_ruleset(path)
                    .map(|l| l.source)
                    .unwrap_or_else(|_| fs::read_to_string(path).unwrap_or_default());
                let rendered = render_diagnostics(&src, &path.display().to_string(), &buf);
                if rendered.is_empty() {
                    eprintln!(
                        "{}: ADGL verify failed ({} diagnostic(s))",
                        path.display(),
                        buf.len()
                    );
                    for d in buf.iter() {
                        eprintln!("{}: {}: {}", d.code, d.severity, d.message);
                    }
                } else {
                    eprint!("{rendered}");
                }
                exit = 1;
            }
        }
    }
    exit
}

/// Parse a hex string (optional `0x`, whitespace, `_` separators) into bytes.
fn parse_hex(s: &str) -> Result<Vec<u8>, String> {
    let cleaned: String = s
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '_')
        .collect();
    let stripped = cleaned.strip_prefix("0x").unwrap_or(&cleaned);
    if stripped.is_empty() {
        return Ok(Vec::new());
    }
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

fn cmd_run(nfdl_path: &Path, hex: Option<&str>) -> i32 {
    if detect_lang(nfdl_path) != Some(Lang::Nfdl) {
        eprintln!(
            "error: run expects a .nfdl file, got {}",
            nfdl_path.display()
        );
        return 2;
    }

    let src = match fs::read_to_string(nfdl_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot read {}: {e}", nfdl_path.display());
            return 2;
        }
    };

    let data = match hex {
        Some(h) => match parse_hex(h) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("invalid hex: {e}");
                return 2;
            }
        },
        None => Vec::new(),
    };

    match nfdl_runtime::parse_and_run_with_data(&src, &data) {
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
                println!("  {k} = {}", ctx[k]);
            }
            for e in &evs {
                println!("  event: {e:?}");
            }
            0
        }
        Err(e) => {
            eprintln!("runtime error: {e}");
            3
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        usage();
        process::exit(1);
    }

    let code = match args[1].as_str() {
        "parse" => {
            if args.len() != 3 {
                usage();
                1
            } else {
                cmd_parse(Path::new(&args[2]))
            }
        }
        "fmt" => {
            let rest = &args[2..];
            let mut mode = FmtMode::Print;
            let mut paths = Vec::new();
            let mut i = 0;
            while i < rest.len() {
                match rest[i].as_str() {
                    "--check" => {
                        if mode != FmtMode::Print {
                            eprintln!("error: --check and --write are mutually exclusive");
                            process::exit(1);
                        }
                        mode = FmtMode::Check;
                    }
                    "--write" => {
                        if mode != FmtMode::Print {
                            eprintln!("error: --check and --write are mutually exclusive");
                            process::exit(1);
                        }
                        mode = FmtMode::Write;
                    }
                    "-h" | "--help" => {
                        usage();
                        process::exit(0);
                    }
                    flag if flag.starts_with('-') => {
                        eprintln!("error: unknown flag {flag}");
                        usage();
                        process::exit(1);
                    }
                    path => paths.push(PathBuf::from(path)),
                }
                i += 1;
            }
            cmd_fmt(mode, &paths)
        }
        "lint" => {
            let rest = &args[2..];
            let mut format = RenderFormat::Human;
            let mut allow = Vec::new();
            let mut deny = Vec::new();
            let mut paths = Vec::new();
            let mut i = 0;
            while i < rest.len() {
                match rest[i].as_str() {
                    "--json" => format = RenderFormat::Json,
                    "--allow" => {
                        i += 1;
                        let Some(id) = rest.get(i) else {
                            eprintln!("error: --allow requires a lint id");
                            usage();
                            process::exit(1);
                        };
                        allow.push(id.clone());
                    }
                    "--deny" => {
                        i += 1;
                        let Some(id) = rest.get(i) else {
                            eprintln!("error: --deny requires a lint id");
                            usage();
                            process::exit(1);
                        };
                        deny.push(id.clone());
                    }
                    "-h" | "--help" => {
                        usage();
                        process::exit(0);
                    }
                    flag if flag.starts_with('-') => {
                        eprintln!("error: unknown flag {flag}");
                        usage();
                        process::exit(1);
                    }
                    path => paths.push(PathBuf::from(path)),
                }
                i += 1;
            }
            cmd_lint(&paths, format, &allow, &deny)
        }
        "check" => {
            let paths: Vec<PathBuf> = args[2..].iter().map(PathBuf::from).collect();
            cmd_check(&paths)
        }
        "verify" => {
            let paths: Vec<PathBuf> = args[2..].iter().map(PathBuf::from).collect();
            cmd_verify(&paths)
        }
        "run" => {
            if args.len() < 3 || args.len() > 4 {
                usage();
                1
            } else {
                let hex = args.get(3).map(String::as_str);
                cmd_run(Path::new(&args[2]), hex)
            }
        }
        "-h" | "--help" | "help" => {
            usage();
            0
        }
        other => {
            eprintln!("error: unknown subcommand `{other}`");
            usage();
            1
        }
    };

    process::exit(code);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_lang_by_extension() {
        assert_eq!(detect_lang(Path::new("a.nfdl")), Some(Lang::Nfdl));
        assert_eq!(detect_lang(Path::new("b.ADGL")), Some(Lang::Adgl));
        assert_eq!(detect_lang(Path::new("c.txt")), None);
    }

    #[test]
    fn parse_hex_accepts_separators() {
        assert_eq!(parse_hex("0x01_02").unwrap(), vec![0x01, 0x02]);
        assert_eq!(parse_hex("ab cd").unwrap(), vec![0xab, 0xcd]);
        assert!(parse_hex("abc").is_err());
    }
}
