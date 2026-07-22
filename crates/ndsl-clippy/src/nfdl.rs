//! First-wave N-FDL style lint pack (`NFDL0001`–`NFDL0299`).
//!
//! Registered by [`register_nfdl_pack`] from [`crate::builtin::register_builtins`].

use crate::{LintCheck, LintContext, LintDef, LintDiagnostic, LintId, LintLevel, LintStore};
use ndsl_diag::Span;
use nfdl_syntax::ast::{
    Action, BinOp, Expr, Field, Let, Loop, Match, MatchArm, Message, NfdlType, Protocol,
    StateMachine, Validate,
};
use std::collections::HashSet;

/// Protocol / message names should be CamelCase (PascalCase).
pub const NFDL_NAMING_TYPE: LintId = LintId::new("NFDL0001");
/// Field names should be snake_case.
pub const NFDL_NAMING_FIELD: LintId = LintId::new("NFDL0002");
/// Message declared but never referenced by bind / MessageRef / transition.
pub const NFDL_UNUSED_MESSAGE: LintId = LintId::new("NFDL0100");
/// `let` binding declared but never referenced in any expression.
/// Wire-layout message fields are never flagged — declaration is their use.
pub const NFDL_UNUSED_LET: LintId = LintId::new("NFDL0101");
/// Validate expression is a constant / tautology stub.
pub const NFDL_REDUNDANT_VALIDATE: LintId = LintId::new("NFDL0200");

pub fn register_nfdl_pack(store: &mut LintStore) {
    store.register(
        LintDef {
            id: NFDL_NAMING_TYPE,
            default_level: LintLevel::Warn,
            description: "protocol and message names should be CamelCase",
        },
        check_naming_types as LintCheck,
    );
    store.register(
        LintDef {
            id: NFDL_NAMING_FIELD,
            default_level: LintLevel::Warn,
            description: "field names should be snake_case",
        },
        check_naming_fields as LintCheck,
    );
    store.register(
        LintDef {
            id: NFDL_UNUSED_MESSAGE,
            default_level: LintLevel::Warn,
            description: "message is never referenced",
        },
        check_unused_messages as LintCheck,
    );
    store.register(
        LintDef {
            id: NFDL_UNUSED_LET,
            default_level: LintLevel::Warn,
            description: "let binding is never referenced in an expression",
        },
        check_unused_lets as LintCheck,
    );
    store.register(
        LintDef {
            id: NFDL_REDUNDANT_VALIDATE,
            default_level: LintLevel::Warn,
            description: "validate expression is constant or tautological",
        },
        check_redundant_validate as LintCheck,
    );
}

fn protocol<'a>(ctx: &LintContext<'a>) -> Option<&'a Protocol> {
    ctx.nfdl
}

fn check_naming_types(ctx: &LintContext<'_>) -> Vec<LintDiagnostic> {
    let Some(proto) = protocol(ctx) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    if !proto.name.is_empty() && !is_camel_case(&proto.name) {
        out.push(LintDiagnostic::new(
            NFDL_NAMING_TYPE,
            LintLevel::Warn,
            format!("protocol name `{}` should be CamelCase", proto.name),
            find_ident_span(ctx.source, &proto.name),
        ));
    }
    for msg in &proto.messages {
        if msg.name.is_empty() {
            continue;
        }
        if !is_camel_case(&msg.name) {
            out.push(LintDiagnostic::new(
                NFDL_NAMING_TYPE,
                LintLevel::Warn,
                format!("message name `{}` should be CamelCase", msg.name),
                find_ident_span(ctx.source, &msg.name),
            ));
        }
    }
    out
}

fn check_naming_fields(ctx: &LintContext<'_>) -> Vec<LintDiagnostic> {
    let Some(proto) = protocol(ctx) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for_each_field(proto, &mut |field| {
        if !is_snake_case(&field.name) {
            out.push(LintDiagnostic::new(
                NFDL_NAMING_FIELD,
                LintLevel::Warn,
                format!("field name `{}` should be snake_case", field.name),
                field.span,
            ));
        }
    });
    out
}

fn check_unused_messages(ctx: &LintContext<'_>) -> Vec<LintDiagnostic> {
    let Some(proto) = protocol(ctx) else {
        return Vec::new();
    };
    if proto.messages.is_empty() {
        return Vec::new();
    }

    let referenced = referenced_messages(proto);
    let has_dispatch = !proto.binds.is_empty()
        || !proto.state_machines.is_empty()
        || protocol_has_message_ref(proto);

    let mut out = Vec::new();
    for (idx, msg) in proto.messages.iter().enumerate() {
        if referenced.contains(msg.name.as_str()) {
            continue;
        }
        // Single entry-point message with no dispatch graph: treat as used.
        if !has_dispatch && proto.messages.len() == 1 {
            continue;
        }
        // Without binds/refs/SMs, the first message is the parse entry point.
        if !has_dispatch && idx == 0 {
            continue;
        }
        out.push(LintDiagnostic::new(
            NFDL_UNUSED_MESSAGE,
            LintLevel::Warn,
            format!("message `{}` is never referenced", msg.name),
            find_ident_span(ctx.source, &msg.name),
        ));
    }
    out
}

fn check_unused_lets(ctx: &LintContext<'_>) -> Vec<LintDiagnostic> {
    let Some(proto) = protocol(ctx) else {
        return Vec::new();
    };
    let used = referenced_idents(proto);
    let mut out = Vec::new();
    for_each_let(proto, &mut |lt| {
        if !used.contains(lt.name.as_str()) {
            out.push(LintDiagnostic::new(
                NFDL_UNUSED_LET,
                LintLevel::Warn,
                format!(
                    "let binding `{}` is never referenced in an expression",
                    lt.name
                ),
                find_ident_span(ctx.source, &lt.name),
            ));
        }
    });
    out
}

fn check_redundant_validate(ctx: &LintContext<'_>) -> Vec<LintDiagnostic> {
    let Some(proto) = protocol(ctx) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for_each_validate(proto, &mut |v| {
        if v.message.trim().is_empty() || is_redundant_validate_expr(&v.expr) {
            out.push(LintDiagnostic::new(
                NFDL_REDUNDANT_VALIDATE,
                LintLevel::Warn,
                "validate expression is constant, tautological, or has an empty message",
                find_validate_span(ctx.source, v),
            ));
        }
    });
    out
}

fn is_camel_case(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_uppercase() => {}
        _ => return false,
    }
    name.chars().all(|c| c.is_ascii_alphanumeric())
}

fn is_snake_case(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return false,
    }
    if name.ends_with('_') || name.contains("__") {
        return false;
    }
    name.chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn is_redundant_validate_expr(expr: &Expr) -> bool {
    match expr {
        // `true`/`false` lower to Int(1)/Int(0); any bare integer is constant.
        Expr::Int(_) => true,
        Expr::Binary {
            op: BinOp::Eq | BinOp::Ne | BinOp::Ge | BinOp::Le | BinOp::Gt | BinOp::Lt,
            left,
            right,
        } if left == right => true,
        Expr::Binary {
            op: BinOp::Or,
            left,
            right,
        } => is_redundant_validate_expr(left) || is_redundant_validate_expr(right),
        Expr::Binary {
            op: BinOp::And,
            left,
            right,
        } => is_redundant_validate_expr(left) && is_redundant_validate_expr(right),
        _ => false,
    }
}

fn referenced_messages(proto: &Protocol) -> HashSet<&str> {
    let mut set = HashSet::new();
    for bind in &proto.binds {
        set.insert(bind.layer.as_str());
        set.insert(bind.source.as_str());
    }
    for msg in &proto.messages {
        collect_message_refs_in_message(msg, &mut set);
    }
    for sm in &proto.state_machines {
        collect_message_refs_in_sm(sm, &mut set);
    }
    set
}

fn protocol_has_message_ref(proto: &Protocol) -> bool {
    let mut set = HashSet::new();
    for msg in &proto.messages {
        collect_message_refs_in_message(msg, &mut set);
    }
    !set.is_empty()
}

fn collect_message_refs_in_message<'a>(msg: &'a Message, set: &mut HashSet<&'a str>) {
    for field in &msg.fields {
        if let NfdlType::MessageRef(name) = &field.ty {
            set.insert(name.as_str());
        }
    }
    for lp in &msg.loops {
        collect_message_refs_in_loop(lp, set);
    }
    for m in &msg.matches {
        collect_message_refs_in_match(m, set);
    }
}

fn collect_message_refs_in_loop<'a>(lp: &'a Loop, set: &mut HashSet<&'a str>) {
    for field in &lp.body {
        if let NfdlType::MessageRef(name) = &field.ty {
            set.insert(name.as_str());
        }
    }
}

fn collect_message_refs_in_match<'a>(m: &'a Match, set: &mut HashSet<&'a str>) {
    for arm in &m.arms {
        collect_message_refs_in_arm(arm, set);
    }
}

fn collect_message_refs_in_arm<'a>(arm: &'a MatchArm, set: &mut HashSet<&'a str>) {
    for field in &arm.fields {
        if let NfdlType::MessageRef(name) = &field.ty {
            set.insert(name.as_str());
        }
    }
    for lp in &arm.loops {
        collect_message_refs_in_loop(lp, set);
    }
    for nested in &arm.matches {
        collect_message_refs_in_match(nested, set);
    }
}

fn collect_message_refs_in_sm<'a>(sm: &'a StateMachine, set: &mut HashSet<&'a str>) {
    for state in &sm.states {
        for tr in &state.transitions {
            set.insert(tr.msg_type.as_str());
        }
    }
}

fn referenced_idents(proto: &Protocol) -> HashSet<&str> {
    let mut set = HashSet::new();
    for bind in &proto.binds {
        collect_idents_in_expr(&bind.when, &mut set);
    }
    for msg in &proto.messages {
        collect_idents_in_message(msg, &mut set);
    }
    for sm in &proto.state_machines {
        for state in &sm.states {
            for tr in &state.transitions {
                if let Some(g) = &tr.guard {
                    collect_idents_in_expr(g, &mut set);
                }
                for action in &tr.actions {
                    match action {
                        Action::Set { var, value } => {
                            set.insert(var.as_str());
                            collect_idents_in_expr(value, &mut set);
                        }
                        Action::Emit { .. } => {}
                    }
                }
            }
        }
        if let Some(key) = &sm.key {
            collect_idents_in_expr(key, &mut set);
        }
    }
    set
}

fn collect_idents_in_message<'a>(msg: &'a Message, set: &mut HashSet<&'a str>) {
    for field in &msg.fields {
        collect_idents_in_field(field, set);
    }
    for lt in &msg.lets {
        collect_idents_in_expr(&lt.value, set);
    }
    for lp in &msg.loops {
        collect_idents_in_expr(&lp.condition, set);
        for c in &lp.carries {
            collect_idents_in_expr(&c.init, set);
        }
        for n in &lp.nexts {
            collect_idents_in_expr(&n.value, set);
        }
        for field in &lp.body {
            collect_idents_in_field(field, set);
        }
    }
    for v in &msg.validates {
        collect_idents_in_expr(&v.expr, set);
    }
    for m in &msg.matches {
        collect_idents_in_match(m, set);
    }
}

fn collect_idents_in_field<'a>(field: &'a Field, set: &mut HashSet<&'a str>) {
    collect_idents_in_type(&field.ty, set);
    if let Some(v) = &field.validate {
        collect_idents_in_expr(&v.expr, set);
    }
    if let Some(c) = &field.conditional {
        collect_idents_in_expr(c, set);
    }
}

fn collect_idents_in_type<'a>(ty: &'a NfdlType, set: &mut HashSet<&'a str>) {
    if let NfdlType::Bytes { len } = ty {
        collect_idents_in_expr(len, set);
    }
}

fn collect_idents_in_match<'a>(m: &'a Match, set: &mut HashSet<&'a str>) {
    collect_idents_in_expr(&m.tag, set);
    for arm in &m.arms {
        for field in &arm.fields {
            collect_idents_in_field(field, set);
        }
        for lt in &arm.lets {
            collect_idents_in_expr(&lt.value, set);
        }
        for lp in &arm.loops {
            collect_idents_in_expr(&lp.condition, set);
            for c in &lp.carries {
                collect_idents_in_expr(&c.init, set);
            }
            for n in &lp.nexts {
                collect_idents_in_expr(&n.value, set);
            }
            for field in &lp.body {
                collect_idents_in_field(field, set);
            }
        }
        for v in &arm.validates {
            collect_idents_in_expr(&v.expr, set);
        }
        for nested in &arm.matches {
            collect_idents_in_match(nested, set);
        }
    }
}

fn collect_idents_in_expr<'a>(expr: &'a Expr, set: &mut HashSet<&'a str>) {
    match expr {
        Expr::Ident(name) => {
            set.insert(name.as_str());
        }
        Expr::Int(_) | Expr::Str(_) => {}
        Expr::Binary { left, right, .. } => {
            collect_idents_in_expr(left, set);
            collect_idents_in_expr(right, set);
        }
        Expr::Unary { expr, .. } => collect_idents_in_expr(expr, set),
        Expr::Ternary {
            cond,
            then_branch,
            else_branch,
        } => {
            collect_idents_in_expr(cond, set);
            collect_idents_in_expr(then_branch, set);
            collect_idents_in_expr(else_branch, set);
        }
        Expr::Coalesce { value, default } => {
            collect_idents_in_expr(value, set);
            collect_idents_in_expr(default, set);
        }
        Expr::Call { args, .. } => {
            for a in args {
                collect_idents_in_expr(a, set);
            }
        }
        Expr::Tuple(items) => {
            for a in items {
                collect_idents_in_expr(a, set);
            }
        }
        Expr::Field(base, name) => {
            collect_idents_in_expr(base, set);
            set.insert(name.as_str());
        }
    }
}

fn for_each_let(proto: &Protocol, f: &mut dyn FnMut(&Let)) {
    for msg in &proto.messages {
        for_each_let_in_message(msg, f);
    }
}

fn for_each_let_in_message(msg: &Message, f: &mut dyn FnMut(&Let)) {
    for lt in &msg.lets {
        f(lt);
    }
    for m in &msg.matches {
        for_each_let_in_match(m, f);
    }
}

fn for_each_let_in_match(m: &Match, f: &mut dyn FnMut(&Let)) {
    for arm in &m.arms {
        for lt in &arm.lets {
            f(lt);
        }
        for nested in &arm.matches {
            for_each_let_in_match(nested, f);
        }
    }
}

fn for_each_field(proto: &Protocol, f: &mut dyn FnMut(&Field)) {
    for msg in &proto.messages {
        for_each_field_in_message(msg, f);
    }
}

fn for_each_field_in_message(msg: &Message, f: &mut dyn FnMut(&Field)) {
    for field in &msg.fields {
        f(field);
    }
    for lp in &msg.loops {
        for field in &lp.body {
            f(field);
        }
    }
    for m in &msg.matches {
        for_each_field_in_match(m, f);
    }
}

fn for_each_field_in_match(m: &Match, f: &mut dyn FnMut(&Field)) {
    for arm in &m.arms {
        for field in &arm.fields {
            f(field);
        }
        for lp in &arm.loops {
            for field in &lp.body {
                f(field);
            }
        }
        for nested in &arm.matches {
            for_each_field_in_match(nested, f);
        }
    }
}

fn for_each_validate(proto: &Protocol, f: &mut dyn FnMut(&Validate)) {
    for msg in &proto.messages {
        for_each_validate_in_message(msg, f);
    }
}

fn for_each_validate_in_message(msg: &Message, f: &mut dyn FnMut(&Validate)) {
    for v in &msg.validates {
        f(v);
    }
    for field in &msg.fields {
        if let Some(v) = &field.validate {
            f(v);
        }
    }
    for lp in &msg.loops {
        for field in &lp.body {
            if let Some(v) = &field.validate {
                f(v);
            }
        }
    }
    for m in &msg.matches {
        for_each_validate_in_match(m, f);
    }
}

fn for_each_validate_in_match(m: &Match, f: &mut dyn FnMut(&Validate)) {
    for arm in &m.arms {
        for v in &arm.validates {
            f(v);
        }
        for field in &arm.fields {
            if let Some(v) = &field.validate {
                f(v);
            }
        }
        for lp in &arm.loops {
            for field in &lp.body {
                if let Some(v) = &field.validate {
                    f(v);
                }
            }
        }
        for nested in &arm.matches {
            for_each_validate_in_match(nested, f);
        }
    }
}

fn find_ident_span(source: &str, name: &str) -> Span {
    if name.is_empty() {
        return Span::unknown();
    }
    let mut start = 0usize;
    while let Some(rel) = source[start..].find(name) {
        let abs = start + rel;
        let end = abs + name.len();
        let before_ok = abs == 0
            || !source.as_bytes()[abs - 1].is_ascii_alphanumeric()
                && source.as_bytes()[abs - 1] != b'_';
        let after_ok = end >= source.len()
            || !source.as_bytes()[end].is_ascii_alphanumeric() && source.as_bytes()[end] != b'_';
        if before_ok && after_ok {
            return Span::new(abs, end);
        }
        start = abs + 1;
    }
    Span::unknown()
}

fn find_validate_span(source: &str, v: &Validate) -> Span {
    // Prefer pointing at the validate keyword near the message text when possible.
    if let Some(rel) = source.find("validate") {
        return Span::new(rel, rel + "validate".len());
    }
    let _ = v;
    Span::unknown()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LintLang, LintStore};
    use std::path::Path;

    fn lint(src: &str) -> Vec<crate::FileDiagnostic> {
        let mut store = LintStore::new();
        store.register_builtin();
        store.lint_source(Path::new("t.nfdl"), src, LintLang::Nfdl)
    }

    fn ids(diags: &[crate::FileDiagnostic]) -> Vec<&str> {
        diags.iter().map(|d| d.diagnostic.id.as_str()).collect()
    }

    #[test]
    fn naming_flags_non_camel_protocol_and_message() {
        let src = r#"
protocol bad_proto {
    meta { endian = big; mode = datagram; }
    message bad_msg {
        ok_field: u8;
    }
}
"#;
        let diags = lint(src);
        assert!(
            diags.iter().any(|d| d.diagnostic.id == NFDL_NAMING_TYPE
                && d.diagnostic.message.contains("bad_proto")),
            "expected protocol naming lint, got: {:?}",
            diags
        );
        assert!(
            diags
                .iter()
                .any(|d| d.diagnostic.id == NFDL_NAMING_TYPE
                    && d.diagnostic.message.contains("bad_msg")),
            "expected message naming lint, got: {:?}",
            diags
        );
    }

    #[test]
    fn naming_flags_non_snake_field() {
        let src = r#"
protocol Good {
    meta { endian = big; mode = datagram; }
    message Pkt {
        BadField: u8;
    }
}
"#;
        let diags = lint(src);
        assert!(
            diags.iter().any(|d| d.diagnostic.id == NFDL_NAMING_FIELD
                && d.diagnostic.message.contains("BadField")),
            "expected field naming lint, got: {:?}",
            diags
        );
    }

    #[test]
    fn unused_message_when_not_bound() {
        let src = r#"
protocol Good {
    meta { endian = big; mode = datagram; }
    message Used {
        x: u8;
    }
    message Orphan {
        y: u8;
    }
    bind Eth payload to Used when ethertype == 1;
}
"#;
        let diags = lint(src);
        assert!(
            diags.iter().any(|d| d.diagnostic.id == NFDL_UNUSED_MESSAGE
                && d.diagnostic.message.contains("Orphan")),
            "expected unused message lint, got: {:?}",
            diags
        );
        assert!(
            !diags.iter().any(|d| d.diagnostic.id == NFDL_UNUSED_MESSAGE
                && d.diagnostic.message.contains("`Used`")),
            "Used should not be unused: {:?}",
            diags
        );
    }

    #[test]
    fn unused_let_when_never_referenced() {
        let src = r#"
protocol Good {
    meta { endian = big; mode = datagram; }
    message Pkt {
        used: u8;
        let alive = used + 1;
        let dead = used + 2;
        validate alive == 1 -> "ok";
    }
}
"#;
        let diags = lint(src);
        assert!(
            diags.iter().any(
                |d| d.diagnostic.id == NFDL_UNUSED_LET && d.diagnostic.message.contains("dead")
            ),
            "expected unused let lint, got: {:?}",
            diags
        );
        assert!(
            !diags
                .iter()
                .any(|d| d.diagnostic.id == NFDL_UNUSED_LET
                    && d.diagnostic.message.contains("`alive`")),
            "alive let should not warn: {:?}",
            diags
        );
        assert!(
            !diags
                .iter()
                .any(|d| d.diagnostic.id == NFDL_UNUSED_LET
                    && d.diagnostic.message.contains("`used`")),
            "wire field must not be flagged as unused let: {:?}",
            diags
        );
    }

    #[test]
    fn wire_layout_fields_never_trigger_nfdl0101() {
        // ARP-like mixed validates + pure payload fields (regression for false positives).
        let src = r#"
protocol Arp {
    meta { endian = big; mode = datagram; }
    message ArpPacket {
        hw_type: u16;
        proto_type: u16;
        validate proto_type == 0x0800 -> "Only IPv4";
        hw_len: u8;
        validate hw_len > 0 -> "hw";
        proto_len: u8;
        validate proto_len > 0 -> "proto";
        opcode: u16;
        sender_mac: bytes[hw_len];
        sender_ip: bytes[proto_len];
        target_mac: bytes[hw_len];
        target_ip: bytes[proto_len];
    }
    bind Ethernet payload to ArpPacket when ethertype == 0x0806;
}
"#;
        let diags = lint(src);
        assert!(
            !diags.iter().any(|d| d.diagnostic.id == NFDL_UNUSED_LET),
            "wire layout fields must not emit NFDL0101, got: {:?}",
            diags
                .iter()
                .filter(|d| d.diagnostic.id == NFDL_UNUSED_LET)
                .map(|d| &d.diagnostic.message)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn redundant_validate_true_and_tautology() {
        let src = r#"
protocol Good {
    meta { endian = big; mode = datagram; }
    message Pkt {
        x: u8;
        validate true -> "always";
        validate x == x -> "tautology";
    }
}
"#;
        let diags = lint(src);
        let redundant: Vec<_> = diags
            .iter()
            .filter(|d| d.diagnostic.id == NFDL_REDUNDANT_VALIDATE)
            .collect();
        assert!(
            redundant.len() >= 2,
            "expected >=2 redundant validates, got: {:?}",
            ids(&diags)
        );
    }

    #[test]
    fn clean_snippet_has_no_pack_lints() {
        let src = r#"
protocol Arp {
    meta { endian = big; mode = datagram; }
    message ArpPacket {
        hw_type: u16;
        proto_type: u16;
        validate proto_type == 0x0800 -> "Only IPv4";
        hw_len: u8;
        validate hw_len > 0 -> "hw";
        proto_len: u8;
        validate proto_len > 0 -> "proto";
        sender_mac: bytes[hw_len];
        sender_ip: bytes[proto_len];
    }
    bind Ethernet payload to ArpPacket when ethertype == 0x0806;
}
"#;
        let diags = lint(src);
        let pack: Vec<_> = diags
            .iter()
            .filter(|d| {
                matches!(
                    d.diagnostic.id.as_str(),
                    "NFDL0001" | "NFDL0002" | "NFDL0100" | "NFDL0101" | "NFDL0200"
                )
            })
            .collect();
        assert!(
            pack.is_empty(),
            "unexpected pack lints: {:?}",
            pack.iter()
                .map(|d| (&d.diagnostic.id, &d.diagnostic.message))
                .collect::<Vec<_>>()
        );
    }
}
