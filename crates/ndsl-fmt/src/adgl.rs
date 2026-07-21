//! ADGL (`Ruleset`) AST pretty-printer.

use airpulse_dsl_syntax::ast::{
    ActionField, ActionName, ActionStmt, AnchorBlock, BinaryOp, CauseAnchor, CorrelateBlock,
    CorrelateSource, DecisionAnchor, DecisionRule, Decl, DurationLit, EmitField, EmitStmt,
    EvidenceRule, Expr, ExprKind, Ident, IfElseBlock, InferField, InferStmt, KindIdent,
    MutuallyExclusiveDecl, ProblemAnchor, RequiresDecl, RuleDecl, Ruleset, Stmt, StringLit,
    TimeWindow, TopoPredicate, UnaryOp,
};
use airpulse_dsl_syntax::parse_ruleset;
use airpulse_dsl_types::ScopeType;
use ndsl_diag::DiagBuffer;

use crate::{FormatError, FormatOptions};

/// Parse ADGL source and pretty-print with default [`FormatOptions`].
pub fn format_adgl_source(src: &str) -> Result<String, FormatError> {
    format_adgl_source_with(src, &FormatOptions::default())
}

/// Parse ADGL source and pretty-print with `opts`.
pub fn format_adgl_source_with(src: &str, opts: &FormatOptions) -> Result<String, FormatError> {
    let ruleset = parse_ruleset(src).map_err(FormatError::Adgl)?;
    let comments = collect_adgl_comments(src).map_err(FormatError::Adgl)?;

    let mut out = String::new();
    emit_comments(&mut out, &comments);
    emit_ruleset(&mut out, &ruleset, opts);
    if !out.ends_with('\n') {
        out.push('\n');
    }
    Ok(out)
}

fn emit_comments(out: &mut String, comments: &[String]) {
    for c in comments {
        out.push_str(c);
        out.push('\n');
    }
}

/// Collect line/block comments (best-effort float-to-head, same strategy as N-FDL).
///
/// Assumes `src` already parsed successfully, so comments are well-formed.
fn collect_adgl_comments(src: &str) -> Result<Vec<String>, DiagBuffer> {
    let mut comments = Vec::new();
    let bytes = src.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        let rest = &src[i..];
        if rest.starts_with("//") {
            let start = i;
            i += 2;
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            comments.push(src[start..i].to_owned());
            continue;
        }
        if rest.starts_with("/*") {
            let start = i;
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            if i + 1 >= bytes.len() {
                // Should be unreachable after a successful parse.
                break;
            }
            i += 2;
            comments.push(src[start..i].to_owned());
            continue;
        }
        if bytes[i] == b'"' {
            i += 1;
            while i < bytes.len() {
                match bytes[i] {
                    b'\\' if i + 1 < bytes.len() => i += 2,
                    b'"' => {
                        i += 1;
                        break;
                    }
                    _ => i += 1,
                }
            }
            continue;
        }
        i += 1;
    }
    Ok(comments)
}

fn indent_str(opts: &FormatOptions, level: usize) -> String {
    " ".repeat(opts.indent.saturating_mul(level))
}

fn emit_ruleset(out: &mut String, ruleset: &Ruleset<'_>, opts: &FormatOptions) {
    out.push_str("ruleset ");
    emit_string_lit(out, &ruleset.name);
    out.push_str(" {\n");

    let pad1 = indent_str(opts, 1);
    out.push_str(&pad1);
    out.push_str("version = ");
    emit_string_lit(out, &ruleset.header.version);
    out.push('\n');

    for decl in &ruleset.header.decls {
        match decl {
            Decl::Requires(r) => emit_requires(out, r, opts, 1),
            Decl::MutuallyExclusive(m) => {
                out.push('\n');
                emit_mutually_exclusive(out, m, opts, 1);
            }
        }
    }

    for rule in &ruleset.rules {
        out.push('\n');
        match rule {
            RuleDecl::Evidence(e) => emit_evidence(out, e, opts, 1),
            RuleDecl::Decision(d) => emit_decision(out, d, opts, 1),
        }
    }

    out.push('}');
    out.push('\n');
}

fn emit_requires(out: &mut String, r: &RequiresDecl, opts: &FormatOptions, level: usize) {
    let pad = indent_str(opts, level);
    out.push_str(&pad);
    out.push_str("requires = [");
    for (i, cap) in r.capabilities.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        emit_string_lit(out, cap);
    }
    out.push(']');
    out.push('\n');
}

fn emit_mutually_exclusive(
    out: &mut String,
    m: &MutuallyExclusiveDecl<'_>,
    opts: &FormatOptions,
    level: usize,
) {
    let pad = indent_str(opts, level);
    out.push_str(&pad);
    out.push_str("mutually_exclusive(");
    for (i, id) in m.idents.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(id.name);
    }
    out.push(')');
    out.push('\n');
}

fn emit_evidence(out: &mut String, rule: &EvidenceRule<'_>, opts: &FormatOptions, level: usize) {
    let pad = indent_str(opts, level);
    out.push_str(&pad);
    out.push_str("evidence ");
    out.push_str(rule.name.name);
    out.push_str(" {\n");
    emit_scope(out, rule.scope, opts, level + 1);
    emit_anchor_block(out, &rule.anchor, opts, level + 1);
    for c in &rule.correlates {
        emit_correlate(out, c, opts, level + 1);
    }
    if let Some(ife) = &rule.if_else {
        emit_if_else(out, ife, opts, level + 1);
    }
    for stmt in &rule.body {
        emit_stmt(out, stmt, opts, level + 1);
    }
    out.push_str(&pad);
    out.push_str("}\n");
}

fn emit_decision(out: &mut String, rule: &DecisionRule<'_>, opts: &FormatOptions, level: usize) {
    let pad = indent_str(opts, level);
    out.push_str(&pad);
    out.push_str("decision ");
    out.push_str(rule.name.name);
    out.push_str(" {\n");
    emit_scope(out, rule.scope, opts, level + 1);
    emit_decision_anchor(out, &rule.anchor, opts, level + 1);
    for c in &rule.correlates {
        emit_correlate(out, c, opts, level + 1);
    }
    if let Some(ife) = &rule.if_else {
        emit_if_else(out, ife, opts, level + 1);
    }
    for stmt in &rule.body {
        emit_stmt(out, stmt, opts, level + 1);
    }
    out.push_str(&pad);
    out.push_str("}\n");
}

fn emit_scope(out: &mut String, scope: ScopeType, opts: &FormatOptions, level: usize) {
    let pad = indent_str(opts, level);
    out.push_str(&pad);
    out.push_str("scope: ");
    out.push_str(scope_str(scope));
    out.push('\n');
}

fn scope_str(scope: ScopeType) -> &'static str {
    match scope {
        ScopeType::Session => "Session",
        ScopeType::Port => "Port",
        ScopeType::ClientMac => "ClientMac",
        ScopeType::Vlan => "Vlan",
        ScopeType::AccessPoint => "AccessPoint",
        ScopeType::Global => "Global",
    }
}

fn emit_anchor_block(out: &mut String, a: &AnchorBlock<'_>, opts: &FormatOptions, level: usize) {
    let pad = indent_str(opts, level);
    out.push_str(&pad);
    out.push_str("anchor ");
    out.push_str(a.binding.name);
    out.push_str(": event(");
    emit_kind_ident(out, &a.event_type);
    out.push(')');
    if let Some(pred) = &a.predicate {
        out.push_str(" {\n");
        let inner = indent_str(opts, level + 1);
        out.push_str(&inner);
        emit_expr(out, pred, 0);
        out.push('\n');
        out.push_str(&pad);
        out.push('}');
    }
    out.push('\n');
}

fn emit_decision_anchor(
    out: &mut String,
    a: &DecisionAnchor<'_>,
    opts: &FormatOptions,
    level: usize,
) {
    let pad = indent_str(opts, level);
    out.push_str(&pad);
    out.push_str("anchor ");
    match a {
        DecisionAnchor::Cause(c) => emit_cause_anchor(out, c, opts, level),
        DecisionAnchor::Problem(p) => emit_problem_anchor(out, p, opts, level),
    }
}

fn emit_cause_anchor(out: &mut String, c: &CauseAnchor<'_>, opts: &FormatOptions, level: usize) {
    out.push_str(c.binding.name);
    out.push_str(": Cause(");
    out.push_str(c.cause.name);
    out.push_str(") {\n");
    let inner = indent_str(opts, level + 1);
    out.push_str(&inner);
    emit_expr(out, &c.predicate, 0);
    out.push('\n');
    let pad = indent_str(opts, level);
    out.push_str(&pad);
    out.push_str("}\n");
}

fn emit_problem_anchor(
    out: &mut String,
    p: &ProblemAnchor<'_>,
    opts: &FormatOptions,
    level: usize,
) {
    out.push_str(p.binding.name);
    out.push_str(": Problem(");
    out.push_str(p.problem.name);
    out.push(')');
    if let Some(pred) = &p.predicate {
        out.push_str(" {\n");
        let inner = indent_str(opts, level + 1);
        out.push_str(&inner);
        emit_expr(out, pred, 0);
        out.push('\n');
        let pad = indent_str(opts, level);
        out.push_str(&pad);
        out.push('}');
    }
    out.push('\n');
}

fn emit_correlate(out: &mut String, c: &CorrelateBlock<'_>, opts: &FormatOptions, level: usize) {
    let pad = indent_str(opts, level);
    let inner = indent_str(opts, level + 1);
    out.push_str(&pad);
    out.push_str("correlate ");
    out.push_str(c.binding.name);
    out.push_str(": ");
    emit_correlate_source(out, &c.source);
    out.push_str(" {\n");
    out.push_str(&inner);
    out.push_str("topo: ");
    emit_topo(out, &c.topo);
    out.push('\n');
    out.push_str(&inner);
    out.push_str("time: ");
    emit_time_window(out, &c.time);
    out.push('\n');
    if let Some(mm) = &c.min_match {
        out.push_str(&inner);
        out.push_str("having: count >= ");
        out.push_str(&mm.count.to_string());
        out.push('\n');
    }
    out.push_str(&pad);
    out.push_str("}\n");
}

fn emit_correlate_source(out: &mut String, src: &CorrelateSource<'_>) {
    match src {
        CorrelateSource::Event(k) => {
            out.push_str("event(");
            emit_kind_ident(out, k);
            out.push(')');
        }
        CorrelateSource::Problem(id) => {
            out.push_str("Problem(");
            out.push_str(id.name);
            out.push(')');
        }
        CorrelateSource::Cause(id) => {
            out.push_str("Cause(");
            out.push_str(id.name);
            out.push(')');
        }
    }
}

fn emit_topo(out: &mut String, t: &TopoPredicate<'_>) {
    out.push_str(t.name.name);
    out.push('(');
    for (i, a) in t.args.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        emit_expr(out, a, 0);
    }
    out.push(')');
}

fn emit_time_window(out: &mut String, tw: &TimeWindow<'_>) {
    emit_expr(out, &tw.probe, 0);
    out.push_str(" in [");
    emit_expr(out, &tw.start, 0);
    out.push_str(", ");
    emit_expr(out, &tw.end, 0);
    out.push(']');
}

fn emit_if_else(out: &mut String, block: &IfElseBlock<'_>, opts: &FormatOptions, level: usize) {
    let pad = indent_str(opts, level);
    out.push_str(&pad);
    out.push_str("if ");
    emit_expr(out, &block.condition, 0);
    out.push_str(" {\n");
    for stmt in &block.then_body {
        emit_stmt(out, stmt, opts, level + 1);
    }
    out.push_str(&pad);
    out.push('}');
    if let Some(else_body) = &block.else_body {
        out.push_str(" else {\n");
        for stmt in else_body {
            emit_stmt(out, stmt, opts, level + 1);
        }
        out.push_str(&pad);
        out.push('}');
    }
    out.push('\n');
}

fn emit_stmt(out: &mut String, stmt: &Stmt<'_>, opts: &FormatOptions, level: usize) {
    match stmt {
        Stmt::Infer(i) => emit_infer(out, i, opts, level),
        Stmt::Emit(e) => emit_emit(out, e, opts, level),
        Stmt::Action(a) => emit_action(out, a, opts, level),
    }
}

fn emit_infer(out: &mut String, stmt: &InferStmt<'_>, opts: &FormatOptions, level: usize) {
    let pad = indent_str(opts, level);
    out.push_str(&pad);
    out.push_str("infer Cause(");
    out.push_str(stmt.cause.name);
    out.push_str(") { ");
    for (i, f) in stmt.fields.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        emit_infer_field(out, f);
    }
    out.push_str(" }\n");
}

fn emit_infer_field(out: &mut String, f: &InferField<'_>) {
    match f {
        InferField::Target(expr, _) => {
            out.push_str("target: ");
            emit_expr(out, expr, 0);
        }
        InferField::Weight { value, .. } => {
            out.push_str("weight: ");
            if *value >= 0 {
                out.push('+');
            }
            out.push_str(&value.to_string());
        }
        InferField::Evidence(refs, _) => {
            out.push_str("evidence: ");
            emit_ref_list(out, refs);
        }
    }
}

fn emit_emit(out: &mut String, stmt: &EmitStmt<'_>, opts: &FormatOptions, level: usize) {
    let pad = indent_str(opts, level);
    out.push_str(&pad);
    out.push_str("emit Problem(");
    out.push_str(stmt.problem.name);
    out.push_str(") { ");
    for (i, f) in stmt.fields.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        emit_emit_field(out, f);
    }
    out.push_str(" }\n");
}

fn emit_emit_field(out: &mut String, f: &EmitField<'_>) {
    match f {
        EmitField::Target(expr, _) => {
            out.push_str("target: ");
            emit_expr(out, expr, 0);
        }
        EmitField::Severity(sev, _) => {
            out.push_str("severity: ");
            out.push_str(sev.as_str());
        }
        EmitField::Evidence(refs, _) => {
            out.push_str("evidence: ");
            emit_ref_list(out, refs);
        }
        EmitField::SarifId(s, _) => {
            out.push_str("sarif_id: ");
            emit_string_lit(out, s);
        }
    }
}

fn emit_action(out: &mut String, stmt: &ActionStmt<'_>, opts: &FormatOptions, level: usize) {
    let pad = indent_str(opts, level);
    out.push_str(&pad);
    out.push_str("action ");
    match &stmt.action {
        ActionName::Known(k) => out.push_str(k.as_str()),
        ActionName::Custom(id) => out.push_str(id.name),
    }
    if let Some(arg) = &stmt.arg {
        out.push('(');
        emit_kind_ident(out, arg);
        out.push(')');
    }
    out.push_str(" { ");
    for (i, f) in stmt.fields.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        emit_action_field(out, f);
    }
    out.push_str(" }\n");
}

fn emit_action_field(out: &mut String, f: &ActionField<'_>) {
    match f {
        ActionField::Target(expr, _) => {
            out.push_str("target: ");
            emit_expr(out, expr, 0);
        }
        ActionField::Reason(s, _) => {
            out.push_str("reason: ");
            emit_string_lit(out, s);
        }
        ActionField::Evidence(refs, _) => {
            out.push_str("evidence: ");
            emit_ref_list(out, refs);
        }
    }
}

fn emit_ref_list(out: &mut String, refs: &[Ident<'_>]) {
    out.push('[');
    for (i, r) in refs.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(r.name);
    }
    out.push(']');
}

fn emit_kind_ident(out: &mut String, k: &KindIdent<'_>) {
    for (i, seg) in k.segments.iter().enumerate() {
        if i > 0 {
            out.push('.');
        }
        out.push_str(seg.name);
    }
}

fn emit_string_lit(out: &mut String, s: &StringLit) {
    out.push('"');
    for ch in s.value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c => out.push(c),
        }
    }
    out.push('"');
}

fn emit_duration(out: &mut String, d: &DurationLit) {
    let ms = d.millis;
    if ms != 0 && ms % 60_000 == 0 {
        out.push_str(&(ms / 60_000).to_string());
        out.push_str("min");
    } else if ms != 0 && ms % 1_000 == 0 {
        out.push_str(&(ms / 1_000).to_string());
        out.push('s');
    } else {
        out.push_str(&ms.to_string());
        out.push_str("ms");
    }
}

/// Precedence matching `airpulse_dsl_syntax` parse ladder (higher = tighter).
fn expr_prec(expr: &Expr<'_>) -> u8 {
    match &expr.kind {
        ExprKind::Int(_)
        | ExprKind::Duration(_)
        | ExprKind::String(_)
        | ExprKind::Ident(_)
        | ExprKind::Bool(_)
        | ExprKind::Present(_)
        | ExprKind::Absent(_) => 13,
        ExprKind::Field { .. } | ExprKind::Call { .. } | ExprKind::Index { .. } => 13,
        ExprKind::Unary { .. } => 12,
        ExprKind::Binary { op, .. } => binop_prec(op),
    }
}

fn binop_prec(op: &BinaryOp) -> u8 {
    match op {
        BinaryOp::Or => 3,
        BinaryOp::And => 4,
        BinaryOp::Eq
        | BinaryOp::Ne
        | BinaryOp::Lt
        | BinaryOp::Le
        | BinaryOp::Gt
        | BinaryOp::Ge
        | BinaryOp::In => 8,
        BinaryOp::Add | BinaryOp::Sub => 11,
        BinaryOp::Mul | BinaryOp::Div | BinaryOp::Rem => 12,
    }
}

fn emit_expr(out: &mut String, expr: &Expr<'_>, parent_prec: u8) {
    let prec = expr_prec(expr);
    let wrap = prec < parent_prec;
    if wrap {
        out.push('(');
    }
    match &expr.kind {
        ExprKind::Int(i) => out.push_str(&i.value.to_string()),
        ExprKind::Duration(d) => emit_duration(out, d),
        ExprKind::String(s) => emit_string_lit(out, s),
        ExprKind::Ident(id) => out.push_str(id.name),
        ExprKind::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        ExprKind::Present(id) => {
            out.push_str("present(");
            out.push_str(id.name);
            out.push(')');
        }
        ExprKind::Absent(id) => {
            out.push_str("absent(");
            out.push_str(id.name);
            out.push(')');
        }
        ExprKind::Unary { op, expr } => {
            out.push_str(unary_str(op));
            emit_expr(out, expr, 12);
        }
        ExprKind::Binary { op, left, right } => {
            let p = binop_prec(op);
            emit_expr(out, left, p);
            out.push(' ');
            out.push_str(binop_str(op));
            out.push(' ');
            emit_expr(out, right, p + 1);
        }
        ExprKind::Field { base, field } => {
            emit_expr(out, base, 13);
            out.push('.');
            out.push_str(field.name);
        }
        ExprKind::Call { callee, args } => {
            emit_expr(out, callee, 13);
            out.push('(');
            for (i, a) in args.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                emit_expr(out, a, 0);
            }
            out.push(')');
        }
        ExprKind::Index { base, index } => {
            emit_expr(out, base, 13);
            out.push('[');
            emit_expr(out, index, 0);
            out.push(']');
        }
    }
    if wrap {
        out.push(')');
    }
}

fn binop_str(op: &BinaryOp) -> &'static str {
    match op {
        BinaryOp::Or => "or",
        BinaryOp::And => "and",
        BinaryOp::Eq => "==",
        BinaryOp::Ne => "!=",
        BinaryOp::Lt => "<",
        BinaryOp::Le => "<=",
        BinaryOp::Gt => ">",
        BinaryOp::Ge => ">=",
        BinaryOp::In => "in",
        BinaryOp::Add => "+",
        BinaryOp::Sub => "-",
        BinaryOp::Mul => "*",
        BinaryOp::Div => "/",
        BinaryOp::Rem => "%",
    }
}

fn unary_str(op: &UnaryOp) -> &'static str {
    match op {
        UnaryOp::Not => "!",
    }
}
