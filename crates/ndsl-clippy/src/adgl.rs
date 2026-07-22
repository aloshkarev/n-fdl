//! ADGL style lint pack (`ADGLS0001`–`ADGLS0399`).
//!
//! Registered by [`register_adgl_pack`] from [`crate::builtin::register_builtins`].
//! Uses the canonical Rust parser (`airpulse_dsl_syntax::parse_ruleset`) — not tree-sitter.

use crate::{LintCheck, LintContext, LintDef, LintDiagnostic, LintId, LintLevel, LintStore};
use airpulse_dsl_syntax::ast::{
    ActionField, ActionStmt, AnchorBlock, CorrelateBlock, CorrelateSource, DecisionAnchor,
    DecisionRule, EmitField, EmitStmt, EvidenceRule, Expr, ExprKind, InferField, InferStmt,
    KindIdent, RuleDecl, Ruleset, Stmt,
};
use ndsl_diag::Span;
use std::collections::HashSet;

/// Correlate binding is never referenced (`present`/`absent`, evidence lists, exprs).
pub const ADGLS_UNUSED_CORRELATE: LintId = LintId::new("ADGLS0001");
/// Float literal in source (units ABI is i64 — use per-mille / centi / ms integers).
pub const ADGLS_FLOAT_LITERAL: LintId = LintId::new("ADGLS0100");
/// `having: count >= 1` is redundant with the omitted default (empty / no-op having).
pub const ADGLS_EMPTY_HAVING: LintId = LintId::new("ADGLS0200");
/// Absence-named signal without explicit `present`/`absent` correlate idiom.
pub const ADGLS_ABSENCE_IDIOM: LintId = LintId::new("ADGLS0300");

pub fn register_adgl_pack(store: &mut LintStore) {
    store.register(
        LintDef {
            id: ADGLS_UNUSED_CORRELATE,
            default_level: LintLevel::Warn,
            description: "correlate binding is never referenced",
        },
        check_unused_correlate as LintCheck,
    );
    store.register(
        LintDef {
            id: ADGLS_FLOAT_LITERAL,
            default_level: LintLevel::Warn,
            description: "float literal breaks units ABI hygiene (prefer i64 thresholds)",
        },
        check_float_literals as LintCheck,
    );
    store.register(
        LintDef {
            id: ADGLS_EMPTY_HAVING,
            default_level: LintLevel::Warn,
            description: "having: count >= 1 is redundant (omit having; default is 1)",
        },
        check_empty_having as LintCheck,
    );
    store.register(
        LintDef {
            id: ADGLS_ABSENCE_IDIOM,
            default_level: LintLevel::Warn,
            description: "absence-named signal without present/absent correlate idiom",
        },
        check_absence_idiom as LintCheck,
    );
}

fn ruleset<'a>(ctx: &LintContext<'a>) -> Option<&'a Ruleset<'a>> {
    ctx.adgl
}

fn check_unused_correlate(ctx: &LintContext<'_>) -> Vec<LintDiagnostic> {
    let Some(rs) = ruleset(ctx) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for rule in &rs.rules {
        match rule {
            RuleDecl::Evidence(ev) => check_unused_correlates_in_evidence(ev, &mut out),
            RuleDecl::Decision(dec) => check_unused_correlates_in_decision(dec, &mut out),
        }
    }
    out
}

fn check_unused_correlates_in_evidence(ev: &EvidenceRule<'_>, out: &mut Vec<LintDiagnostic>) {
    for (idx, c) in ev.correlates.iter().enumerate() {
        let used = external_uses_of_correlate_in_evidence(ev, idx);
        if !used {
            out.push(LintDiagnostic::new(
                ADGLS_UNUSED_CORRELATE,
                LintLevel::Warn,
                format!(
                    "correlate binding `{}` is never referenced (present/absent, evidence lists, or expressions)",
                    c.binding.name
                ),
                c.binding.span,
            ));
        }
    }
}

fn check_unused_correlates_in_decision(dec: &DecisionRule<'_>, out: &mut Vec<LintDiagnostic>) {
    for (idx, c) in dec.correlates.iter().enumerate() {
        let used = external_uses_of_correlate_in_decision(dec, idx);
        if !used {
            out.push(LintDiagnostic::new(
                ADGLS_UNUSED_CORRELATE,
                LintLevel::Warn,
                format!(
                    "correlate binding `{}` is never referenced (present/absent, evidence lists, or expressions)",
                    c.binding.name
                ),
                c.binding.span,
            ));
        }
    }
}

/// References inside a correlate's own `topo`/`time` do not count — those are
/// definition-site field paths. Uses are `present`/`absent`, evidence lists,
/// other correlates, if/else, and body statements.
fn external_uses_of_correlate_in_evidence(ev: &EvidenceRule<'_>, idx: usize) -> bool {
    let name = ev.correlates[idx].binding.name;
    let mut used = HashSet::new();
    collect_used_in_anchor(&ev.anchor, &mut used);
    for (j, other) in ev.correlates.iter().enumerate() {
        if j == idx {
            continue;
        }
        collect_used_in_correlate(other, &mut used);
    }
    if let Some(ie) = &ev.if_else {
        collect_used_in_expr(&ie.condition, &mut used);
        collect_used_in_stmts(&ie.then_body, &mut used);
        if let Some(else_body) = &ie.else_body {
            collect_used_in_stmts(else_body, &mut used);
        }
    }
    collect_used_in_stmts(&ev.body, &mut used);
    used.contains(name)
}

fn external_uses_of_correlate_in_decision(dec: &DecisionRule<'_>, idx: usize) -> bool {
    let name = dec.correlates[idx].binding.name;
    let mut used = HashSet::new();
    collect_used_in_decision_anchor(&dec.anchor, &mut used);
    for (j, other) in dec.correlates.iter().enumerate() {
        if j == idx {
            continue;
        }
        collect_used_in_correlate(other, &mut used);
    }
    if let Some(ie) = &dec.if_else {
        collect_used_in_expr(&ie.condition, &mut used);
        collect_used_in_stmts(&ie.then_body, &mut used);
        if let Some(else_body) = &ie.else_body {
            collect_used_in_stmts(else_body, &mut used);
        }
    }
    collect_used_in_stmts(&dec.body, &mut used);
    used.contains(name)
}

fn collect_used_in_anchor<'a>(anchor: &'a AnchorBlock<'a>, used: &mut HashSet<&'a str>) {
    if let Some(pred) = &anchor.predicate {
        collect_used_in_expr(pred, used);
    }
}

fn collect_used_in_decision_anchor<'a>(anchor: &'a DecisionAnchor<'a>, used: &mut HashSet<&'a str>) {
    match anchor {
        DecisionAnchor::Cause(c) => collect_used_in_expr(&c.predicate, used),
        DecisionAnchor::Problem(p) => {
            if let Some(pred) = &p.predicate {
                collect_used_in_expr(pred, used);
            }
        }
    }
}

fn collect_used_in_correlate<'a>(c: &'a CorrelateBlock<'a>, used: &mut HashSet<&'a str>) {
    for arg in &c.topo.args {
        collect_used_in_expr(arg, used);
    }
    collect_used_in_expr(&c.time.probe, used);
    collect_used_in_expr(&c.time.start, used);
    collect_used_in_expr(&c.time.end, used);
}

fn collect_used_in_stmts<'a>(stmts: &'a [Stmt<'a>], used: &mut HashSet<&'a str>) {
    for stmt in stmts {
        match stmt {
            Stmt::Infer(inf) => collect_used_in_infer(inf, used),
            Stmt::Emit(em) => collect_used_in_emit(em, used),
            Stmt::Action(act) => collect_used_in_action(act, used),
        }
    }
}

fn collect_used_in_infer<'a>(inf: &'a InferStmt<'a>, used: &mut HashSet<&'a str>) {
    for field in &inf.fields {
        match field {
            InferField::Target(expr, _) => collect_used_in_expr(expr, used),
            InferField::Weight { .. } => {}
            InferField::Evidence(idents, _) => {
                for id in idents {
                    used.insert(id.name);
                }
            }
        }
    }
}

fn collect_used_in_emit<'a>(em: &'a EmitStmt<'a>, used: &mut HashSet<&'a str>) {
    for field in &em.fields {
        match field {
            EmitField::Target(expr, _) => collect_used_in_expr(expr, used),
            EmitField::Severity(_, _) | EmitField::SarifId(_, _) => {}
            EmitField::Evidence(idents, _) => {
                for id in idents {
                    used.insert(id.name);
                }
            }
        }
    }
}

fn collect_used_in_action<'a>(act: &'a ActionStmt<'a>, used: &mut HashSet<&'a str>) {
    for field in &act.fields {
        match field {
            ActionField::Target(expr, _) => collect_used_in_expr(expr, used),
            ActionField::Reason(_, _) => {}
            ActionField::Evidence(idents, _) => {
                for id in idents {
                    used.insert(id.name);
                }
            }
        }
    }
}

fn collect_used_in_expr<'a>(expr: &'a Expr<'a>, used: &mut HashSet<&'a str>) {
    match &expr.kind {
        ExprKind::Ident(id) => {
            used.insert(id.name);
        }
        ExprKind::Present(id) | ExprKind::Absent(id) => {
            used.insert(id.name);
        }
        ExprKind::Int(_) | ExprKind::Duration(_) | ExprKind::String(_) | ExprKind::Bool(_) => {}
        ExprKind::Unary { expr, .. } => collect_used_in_expr(expr, used),
        ExprKind::Binary { left, right, .. } => {
            collect_used_in_expr(left, used);
            collect_used_in_expr(right, used);
        }
        ExprKind::Field { base, .. } => collect_used_in_expr(base, used),
        ExprKind::Call { callee, args } => {
            collect_used_in_expr(callee, used);
            for a in args {
                collect_used_in_expr(a, used);
            }
        }
        ExprKind::Index { base, index } => {
            collect_used_in_expr(base, used);
            collect_used_in_expr(index, used);
        }
    }
}

fn check_empty_having(ctx: &LintContext<'_>) -> Vec<LintDiagnostic> {
    let Some(rs) = ruleset(ctx) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for rule in &rs.rules {
        let correlates = match rule {
            RuleDecl::Evidence(ev) => &ev.correlates,
            RuleDecl::Decision(dec) => &dec.correlates,
        };
        for c in correlates {
            // Omitted having ≡ count >= 1. Explicit `having: count >= 1` adds nothing.
            // N=0 / N>32 are verify diagnostics (ADGL0504/0505) — do not duplicate.
            if let Some(mm) = &c.min_match {
                if mm.count == 1 {
                    out.push(LintDiagnostic::new(
                        ADGLS_EMPTY_HAVING,
                        LintLevel::Warn,
                        "having: count >= 1 is redundant (omit having; default minimum is 1)",
                        mm.span,
                    ));
                }
            }
        }
    }
    out
}

/// Suggest `present`/`absent` correlate idioms when absence-named signals appear
/// without any explicit correlate presence check (`docs/ABSENCE_SEMANTICS.md`).
///
/// Heuristic only — no IR/counterfactual sugar. Does not fire when the rule
/// already uses `present(...)` or `absent(...)`.
fn check_absence_idiom(ctx: &LintContext<'_>) -> Vec<LintDiagnostic> {
    let Some(rs) = ruleset(ctx) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for rule in &rs.rules {
        match rule {
            RuleDecl::Evidence(ev) => {
                if rule_uses_present_or_absent_evidence(ev) {
                    continue;
                }
                if let Some((name, span)) = first_absence_named_in_evidence(ev) {
                    out.push(absence_idiom_diagnostic(name, span));
                }
            }
            RuleDecl::Decision(dec) => {
                if rule_uses_present_or_absent_decision(dec) {
                    continue;
                }
                if let Some((name, span)) = first_absence_named_in_decision(dec) {
                    out.push(absence_idiom_diagnostic(name, span));
                }
            }
        }
    }
    out
}

fn absence_idiom_diagnostic(name: &str, span: Span) -> LintDiagnostic {
    LintDiagnostic::new(
        ADGLS_ABSENCE_IDIOM,
        LintLevel::Warn,
        format!(
            "absence-named signal `{name}` without `present`/`absent` correlate idiom; \
             prefer explicit `present(binding)` / `absent(binding)` (absence-as-code) \
             over silent all-positives-failed counters — see ABSENCE_SEMANTICS"
        ),
        span,
    )
}

/// Naming from `docs/ABSENCE_SEMANTICS.md` plus related unanswered/missing idioms.
fn is_absence_named(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.contains("unanswered")
        || n.contains("missing")
        || n.contains("absent")
        || n.contains("without")
        || n.contains("incomplete")
        || n.contains("no_response")
        || n.contains("noresponse")
        || n.contains("absence")
}

fn rule_uses_present_or_absent_evidence(ev: &EvidenceRule<'_>) -> bool {
    let mut found = false;
    scan_present_absent_in_anchor(&ev.anchor, &mut found);
    for c in &ev.correlates {
        scan_present_absent_in_correlate(c, &mut found);
    }
    if let Some(ie) = &ev.if_else {
        scan_present_absent_in_expr(&ie.condition, &mut found);
        scan_present_absent_in_stmts(&ie.then_body, &mut found);
        if let Some(else_body) = &ie.else_body {
            scan_present_absent_in_stmts(else_body, &mut found);
        }
    }
    scan_present_absent_in_stmts(&ev.body, &mut found);
    found
}

fn rule_uses_present_or_absent_decision(dec: &DecisionRule<'_>) -> bool {
    let mut found = false;
    scan_present_absent_in_decision_anchor(&dec.anchor, &mut found);
    for c in &dec.correlates {
        scan_present_absent_in_correlate(c, &mut found);
    }
    if let Some(ie) = &dec.if_else {
        scan_present_absent_in_expr(&ie.condition, &mut found);
        scan_present_absent_in_stmts(&ie.then_body, &mut found);
        if let Some(else_body) = &ie.else_body {
            scan_present_absent_in_stmts(else_body, &mut found);
        }
    }
    scan_present_absent_in_stmts(&dec.body, &mut found);
    found
}

fn scan_present_absent_in_anchor(anchor: &AnchorBlock<'_>, found: &mut bool) {
    if let Some(pred) = &anchor.predicate {
        scan_present_absent_in_expr(pred, found);
    }
}

fn scan_present_absent_in_decision_anchor(anchor: &DecisionAnchor<'_>, found: &mut bool) {
    match anchor {
        DecisionAnchor::Cause(c) => scan_present_absent_in_expr(&c.predicate, found),
        DecisionAnchor::Problem(p) => {
            if let Some(pred) = &p.predicate {
                scan_present_absent_in_expr(pred, found);
            }
        }
    }
}

fn scan_present_absent_in_correlate(c: &CorrelateBlock<'_>, found: &mut bool) {
    for arg in &c.topo.args {
        scan_present_absent_in_expr(arg, found);
    }
    scan_present_absent_in_expr(&c.time.probe, found);
    scan_present_absent_in_expr(&c.time.start, found);
    scan_present_absent_in_expr(&c.time.end, found);
}

fn scan_present_absent_in_stmts(stmts: &[Stmt<'_>], found: &mut bool) {
    for stmt in stmts {
        match stmt {
            Stmt::Infer(inf) => {
                for field in &inf.fields {
                    if let InferField::Target(expr, _) = field {
                        scan_present_absent_in_expr(expr, found);
                    }
                }
            }
            Stmt::Emit(em) => {
                for field in &em.fields {
                    if let EmitField::Target(expr, _) = field {
                        scan_present_absent_in_expr(expr, found);
                    }
                }
            }
            Stmt::Action(act) => {
                for field in &act.fields {
                    if let ActionField::Target(expr, _) = field {
                        scan_present_absent_in_expr(expr, found);
                    }
                }
            }
        }
    }
}

fn scan_present_absent_in_expr(expr: &Expr<'_>, found: &mut bool) {
    if *found {
        return;
    }
    match &expr.kind {
        ExprKind::Present(_) | ExprKind::Absent(_) => *found = true,
        ExprKind::Int(_)
        | ExprKind::Duration(_)
        | ExprKind::String(_)
        | ExprKind::Bool(_)
        | ExprKind::Ident(_) => {}
        ExprKind::Unary { expr, .. } => scan_present_absent_in_expr(expr, found),
        ExprKind::Binary { left, right, .. } => {
            scan_present_absent_in_expr(left, found);
            scan_present_absent_in_expr(right, found);
        }
        ExprKind::Field { base, .. } => scan_present_absent_in_expr(base, found),
        ExprKind::Call { callee, args } => {
            scan_present_absent_in_expr(callee, found);
            for a in args {
                scan_present_absent_in_expr(a, found);
            }
        }
        ExprKind::Index { base, index } => {
            scan_present_absent_in_expr(base, found);
            scan_present_absent_in_expr(index, found);
        }
    }
}

fn first_absence_named_in_evidence<'a>(ev: &'a EvidenceRule<'a>) -> Option<(&'a str, Span)> {
    if is_absence_named(ev.name.name) {
        return Some((ev.name.name, ev.name.span));
    }
    if let Some(hit) = first_absence_in_kind(&ev.anchor.event_type) {
        return Some(hit);
    }
    if let Some(pred) = &ev.anchor.predicate {
        if let Some(hit) = first_absence_in_expr(pred) {
            return Some(hit);
        }
    }
    for c in &ev.correlates {
        if let Some(hit) = first_absence_in_correlate(c) {
            return Some(hit);
        }
    }
    if let Some(ie) = &ev.if_else {
        if let Some(hit) = first_absence_in_expr(&ie.condition) {
            return Some(hit);
        }
        if let Some(hit) = first_absence_in_stmts(&ie.then_body) {
            return Some(hit);
        }
        if let Some(else_body) = &ie.else_body {
            if let Some(hit) = first_absence_in_stmts(else_body) {
                return Some(hit);
            }
        }
    }
    first_absence_in_stmts(&ev.body)
}

fn first_absence_named_in_decision<'a>(dec: &'a DecisionRule<'a>) -> Option<(&'a str, Span)> {
    if is_absence_named(dec.name.name) {
        return Some((dec.name.name, dec.name.span));
    }
    match &dec.anchor {
        DecisionAnchor::Cause(c) => {
            if is_absence_named(c.cause.name) {
                return Some((c.cause.name, c.cause.span));
            }
            if let Some(hit) = first_absence_in_expr(&c.predicate) {
                return Some(hit);
            }
        }
        DecisionAnchor::Problem(p) => {
            if is_absence_named(p.problem.name) {
                return Some((p.problem.name, p.problem.span));
            }
            if let Some(pred) = &p.predicate {
                if let Some(hit) = first_absence_in_expr(pred) {
                    return Some(hit);
                }
            }
        }
    }
    for c in &dec.correlates {
        if let Some(hit) = first_absence_in_correlate(c) {
            return Some(hit);
        }
    }
    if let Some(ie) = &dec.if_else {
        if let Some(hit) = first_absence_in_expr(&ie.condition) {
            return Some(hit);
        }
        if let Some(hit) = first_absence_in_stmts(&ie.then_body) {
            return Some(hit);
        }
        if let Some(else_body) = &ie.else_body {
            if let Some(hit) = first_absence_in_stmts(else_body) {
                return Some(hit);
            }
        }
    }
    first_absence_in_stmts(&dec.body)
}

fn first_absence_in_correlate<'a>(c: &'a CorrelateBlock<'a>) -> Option<(&'a str, Span)> {
    if is_absence_named(c.binding.name) {
        return Some((c.binding.name, c.binding.span));
    }
    match &c.source {
        CorrelateSource::Event(k) => {
            if let Some(hit) = first_absence_in_kind(k) {
                return Some(hit);
            }
        }
        CorrelateSource::Problem(id) | CorrelateSource::Cause(id) => {
            if is_absence_named(id.name) {
                return Some((id.name, id.span));
            }
        }
    }
    for arg in &c.topo.args {
        if let Some(hit) = first_absence_in_expr(arg) {
            return Some(hit);
        }
    }
    first_absence_in_expr(&c.time.probe)
        .or_else(|| first_absence_in_expr(&c.time.start))
        .or_else(|| first_absence_in_expr(&c.time.end))
}

fn first_absence_in_stmts<'a>(stmts: &'a [Stmt<'a>]) -> Option<(&'a str, Span)> {
    for stmt in stmts {
        match stmt {
            Stmt::Infer(inf) => {
                if is_absence_named(inf.cause.name) {
                    return Some((inf.cause.name, inf.cause.span));
                }
                for field in &inf.fields {
                    if let InferField::Target(expr, _) = field {
                        if let Some(hit) = first_absence_in_expr(expr) {
                            return Some(hit);
                        }
                    }
                }
            }
            Stmt::Emit(em) => {
                if is_absence_named(em.problem.name) {
                    return Some((em.problem.name, em.problem.span));
                }
                for field in &em.fields {
                    if let EmitField::Target(expr, _) = field {
                        if let Some(hit) = first_absence_in_expr(expr) {
                            return Some(hit);
                        }
                    }
                }
            }
            Stmt::Action(act) => {
                for field in &act.fields {
                    if let ActionField::Target(expr, _) = field {
                        if let Some(hit) = first_absence_in_expr(expr) {
                            return Some(hit);
                        }
                    }
                }
            }
        }
    }
    None
}

fn first_absence_in_kind<'a>(k: &'a KindIdent<'a>) -> Option<(&'a str, Span)> {
    for seg in &k.segments {
        if is_absence_named(seg.name) {
            return Some((seg.name, seg.span));
        }
    }
    None
}

fn first_absence_in_expr<'a>(expr: &'a Expr<'a>) -> Option<(&'a str, Span)> {
    match &expr.kind {
        ExprKind::Ident(id) => {
            if is_absence_named(id.name) {
                Some((id.name, id.span))
            } else {
                None
            }
        }
        ExprKind::Present(_)
        | ExprKind::Absent(_)
        | ExprKind::Int(_)
        | ExprKind::Duration(_)
        | ExprKind::String(_)
        | ExprKind::Bool(_) => None,
        ExprKind::Unary { expr, .. } => first_absence_in_expr(expr),
        ExprKind::Binary { left, right, .. } => {
            first_absence_in_expr(left).or_else(|| first_absence_in_expr(right))
        }
        ExprKind::Field { base, field } => {
            if is_absence_named(field.name) {
                Some((field.name, field.span))
            } else {
                first_absence_in_expr(base)
            }
        }
        ExprKind::Call { callee, args } => {
            if let Some(hit) = first_absence_in_expr(callee) {
                return Some(hit);
            }
            for a in args {
                if let Some(hit) = first_absence_in_expr(a) {
                    return Some(hit);
                }
            }
            None
        }
        ExprKind::Index { base, index } => {
            first_absence_in_expr(base).or_else(|| first_absence_in_expr(index))
        }
    }
}

/// Scan ADGL source for float literals outside strings/comments.
///
/// Floats are not part of the units ABI (`docs/DIAGNOSIS_UNITS.md`); thresholds
/// must be i64 (per-mille, centi, ms). The parser rejects floats, so this lint
/// runs on the raw source (including unparsable files) for `.adgl` inputs.
fn check_float_literals(ctx: &LintContext<'_>) -> Vec<LintDiagnostic> {
    if ctx.lang != crate::LintLang::Adgl {
        return Vec::new();
    }
    scan_float_literals(ctx.source)
}

fn scan_float_literals(source: &str) -> Vec<LintDiagnostic> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        // Line comment
        if bytes[i] == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
            i += 2;
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        // Block comment
        if bytes[i] == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            i = (i + 2).min(bytes.len());
            continue;
        }
        // String literal
        if bytes[i] == b'"' {
            i += 1;
            while i < bytes.len() {
                if bytes[i] == b'\\' {
                    i = (i + 2).min(bytes.len());
                    continue;
                }
                if bytes[i] == b'"' {
                    i += 1;
                    break;
                }
                i += 1;
            }
            continue;
        }

        if bytes[i].is_ascii_digit() {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'_') {
                i += 1;
            }
            if i < bytes.len() && bytes[i] == b'.' {
                let after_dot = i + 1;
                if after_dot < bytes.len() && bytes[after_dot].is_ascii_digit() {
                    // Confirm this is not an ident-continue after the fractional part
                    // (field access would be digit? No — field is Ident.digit only as
                    // `name.field`. Numeric `1.0` / `0.04` are floats.)
                    let before_ok = start == 0 || !is_ident_byte(bytes[start - 1]);
                    let mut j = after_dot;
                    while j < bytes.len() && (bytes[j].is_ascii_digit() || bytes[j] == b'_') {
                        j += 1;
                    }
                    // Exponent form `1.0e5` still a float.
                    if j < bytes.len() && (bytes[j] == b'e' || bytes[j] == b'E') {
                        let mut k = j + 1;
                        if k < bytes.len() && (bytes[k] == b'+' || bytes[k] == b'-') {
                            k += 1;
                        }
                        while k < bytes.len() && bytes[k].is_ascii_digit() {
                            k += 1;
                        }
                        j = k;
                    }
                    let after_ok = j >= bytes.len() || !is_ident_byte(bytes[j]);
                    if before_ok && after_ok {
                        out.push(LintDiagnostic::new(
                            ADGLS_FLOAT_LITERAL,
                            LintLevel::Warn,
                            "float literal is not units-ABI-safe; use i64 thresholds (per-mille, centi, ms)",
                            Span::new(start, j),
                        ));
                        i = j;
                        continue;
                    }
                }
            }
            continue;
        }
        i += 1;
    }
    out
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LintLang, LintStore};
    use std::path::Path;

    fn lint(src: &str) -> Vec<crate::FileDiagnostic> {
        let mut store = LintStore::new();
        store.register_builtin();
        store.lint_source(Path::new("t.adgl"), src, LintLang::Adgl)
    }

    fn minimal_ruleset(body: &str) -> String {
        format!(
            r#"
ruleset "test.rs" {{
    version = "1.0"
    requires = ["l3-deep"]

    {body}
}}
"#
        )
    }

    #[test]
    fn unused_correlate_when_never_referenced() {
        let src = minimal_ruleset(
            r#"
    evidence e {
        scope: Session
        anchor a: event(tcp.retransmission_burst) {
            a.segment_size > 1400
        }
        correlate orphan: event(icmp.ptb) {
            topo: same_session(a.target, orphan.target)
            time: orphan.time in [a.time - 500ms, a.time + 1s]
        }
        infer Cause(PmtudBlackhole) { target: a.target, weight: +35, evidence: [a] }
    }
"#,
        );
        let diags = lint(&src);
        assert!(
            diags.iter().any(|d| d.diagnostic.id == ADGLS_UNUSED_CORRELATE
                && d.diagnostic.message.contains("orphan")),
            "expected unused correlate, got: {:?}",
            diags
                .iter()
                .map(|d| (&d.diagnostic.id, &d.diagnostic.message))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn correlate_used_only_in_present_is_not_unused() {
        let src = minimal_ruleset(
            r#"
    evidence e {
        scope: Session
        anchor a: event(tcp.retransmission_burst) {
            a.segment_size > 1400
        }
        correlate ptb: event(icmp.ptb) {
            topo: same_session(a.target, ptb.target)
            time: ptb.time in [a.time - 500ms, a.time + 1s]
        }
        if present(ptb) {
            infer Cause(PmtudBlackhole) { target: a.target, weight: +85, evidence: [a] }
        } else {
            infer Cause(PmtudBlackhole) { target: a.target, weight: +35, evidence: [a] }
        }
    }
"#,
        );
        let diags = lint(&src);
        assert!(
            !diags
                .iter()
                .any(|d| d.diagnostic.id == ADGLS_UNUSED_CORRELATE),
            "present(ptb) must count as use, got: {:?}",
            diags
                .iter()
                .filter(|d| d.diagnostic.id == ADGLS_UNUSED_CORRELATE)
                .map(|d| &d.diagnostic.message)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn correlate_used_only_in_evidence_list_is_not_unused() {
        let src = minimal_ruleset(
            r#"
    evidence e {
        scope: Session
        anchor a: event(tcp.retransmission_burst) {
            a.segment_size > 1400
        }
        correlate ptb: event(icmp.ptb) {
            topo: same_session(a.target, ptb.target)
            time: ptb.time in [a.time - 500ms, a.time + 1s]
        }
        infer Cause(PmtudBlackhole) { target: a.target, weight: +85, evidence: [a, ptb] }
    }
"#,
        );
        let diags = lint(&src);
        assert!(
            !diags
                .iter()
                .any(|d| d.diagnostic.id == ADGLS_UNUSED_CORRELATE),
            "evidence: [..., ptb] must count as use, got: {:?}",
            diags
                .iter()
                .filter(|d| d.diagnostic.id == ADGLS_UNUSED_CORRELATE)
                .map(|d| &d.diagnostic.message)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn float_literal_in_predicate_warns() {
        // Intentionally unparsable once float is present — lint still scans source.
        let src = r#"
ruleset "t" {
    version = "1.0"
    evidence e {
        scope: Session
        anchor a: event(tcp.retransmission_burst) {
            a.loss_rate > 0.04
        }
        infer Cause(X) { target: a.target, weight: +10, evidence: [a] }
    }
}
"#;
        let diags = lint(src);
        assert!(
            diags
                .iter()
                .any(|d| d.diagnostic.id == ADGLS_FLOAT_LITERAL
                    && d.diagnostic.message.contains("units-ABI")),
            "expected float literal lint, got: {:?}",
            diags
                .iter()
                .map(|d| (&d.diagnostic.id, &d.diagnostic.message))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn float_in_string_or_comment_is_ignored() {
        let src = minimal_ruleset(
            r#"
    // legacy threshold was 0.04
    evidence e {
        scope: Session
        anchor a: event(tcp.retransmission_burst) {
            a.segment_size > 1400
        }
        infer Cause(PmtudBlackhole) { target: a.target, weight: +35, evidence: [a] }
        action suppress_symptom { reason: "not 0.04", evidence: [a] }
    }
"#,
        );
        let diags = lint(&src);
        assert!(
            !diags
                .iter()
                .any(|d| d.diagnostic.id == ADGLS_FLOAT_LITERAL),
            "floats in comments/strings must not warn, got: {:?}",
            diags
                .iter()
                .filter(|d| d.diagnostic.id == ADGLS_FLOAT_LITERAL)
                .map(|d| &d.diagnostic.message)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn empty_having_count_one_warns() {
        let src = minimal_ruleset(
            r#"
    evidence e {
        scope: Session
        anchor a: event(tcp.retransmission_burst) {
            a.segment_size > 1400
        }
        correlate ptb: event(icmp.ptb) {
            topo: same_session(a.target, ptb.target)
            time: ptb.time in [a.time - 500ms, a.time + 1s]
            having: count >= 1
        }
        if present(ptb) {
            infer Cause(PmtudBlackhole) { target: a.target, weight: +85, evidence: [a, ptb] }
        } else {
            infer Cause(PmtudBlackhole) { target: a.target, weight: +35, evidence: [a] }
        }
    }
"#,
        );
        let diags = lint(&src);
        assert!(
            diags
                .iter()
                .any(|d| d.diagnostic.id == ADGLS_EMPTY_HAVING),
            "expected empty having lint, got: {:?}",
            diags
                .iter()
                .map(|d| (&d.diagnostic.id, &d.diagnostic.message))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn meaningful_having_does_not_warn() {
        let src = minimal_ruleset(
            r#"
    evidence e {
        scope: Session
        anchor a: event(tcp.retransmission_burst) {
            a.segment_size > 1400
        }
        correlate hits: event(wifi.mgmt.deauth) {
            topo: same_session(a.target, hits.target)
            time: hits.time in [a.time - 1s, a.time + 1s]
            having: count >= 30
        }
        infer Cause(PmtudBlackhole) { target: a.target, weight: +50, evidence: [a, hits] }
    }
"#,
        );
        let diags = lint(&src);
        assert!(
            !diags
                .iter()
                .any(|d| d.diagnostic.id == ADGLS_EMPTY_HAVING),
            "having: count >= 30 must not warn, got: {:?}",
            diags
                .iter()
                .filter(|d| d.diagnostic.id == ADGLS_EMPTY_HAVING)
                .map(|d| &d.diagnostic.message)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn absence_named_without_present_absent_warns() {
        let src = minimal_ruleset(
            r#"
    evidence dhcp_missing_offer {
        scope: Global
        anchor h: event(dhcp.summary) {
            h.discover_without_offer >= 1
        }
        infer Cause(DhcpMissingOfferSignal) { target: h.target, weight: +65, evidence: [h] }
    }
"#,
        );
        let diags = lint(&src);
        assert!(
            diags.iter().any(|d| {
                d.diagnostic.id == ADGLS_ABSENCE_IDIOM
                    && d.diagnostic.message.contains("present(binding)")
                    && d.diagnostic.message.contains("absent(binding)")
            }),
            "expected ADGLS0300 with present/absent suggestion, got: {:?}",
            diags
                .iter()
                .map(|d| (&d.diagnostic.id, &d.diagnostic.message))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn absence_named_with_present_absent_does_not_warn() {
        let src = minimal_ruleset(
            r#"
    evidence dhcp_missing_offer {
        scope: Session
        anchor h: event(dhcp.discover) {
            h.xid > 0
        }
        correlate offer: event(dhcp.offer) {
            topo: same_session(h.target, offer.target)
            time: offer.time in [h.time, h.time + 5s]
        }
        if absent(offer) {
            infer Cause(DhcpMissingOfferSignal) { target: h.target, weight: +65, evidence: [h] }
        }
    }
"#,
        );
        let diags = lint(&src);
        assert!(
            !diags
                .iter()
                .any(|d| d.diagnostic.id == ADGLS_ABSENCE_IDIOM),
            "present/absent idiom must suppress ADGLS0300, got: {:?}",
            diags
                .iter()
                .filter(|d| d.diagnostic.id == ADGLS_ABSENCE_IDIOM)
                .map(|d| &d.diagnostic.message)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn healthy_pmtud_snippet_has_no_adgl_pack_lints() {
        let src = include_str!("../../../docs/idea/examples/01-pmtud-blackhole.adgl");
        let diags = lint(src);
        let pack: Vec<_> = diags
            .iter()
            .filter(|d| {
                matches!(
                    d.diagnostic.id.as_str(),
                    "ADGLS0001" | "ADGLS0100" | "ADGLS0200" | "ADGLS0300"
                )
            })
            .collect();
        assert!(
            pack.is_empty(),
            "unexpected ADGL pack lints on healthy example: {:?}",
            pack.iter()
                .map(|d| (&d.diagnostic.id, &d.diagnostic.message))
                .collect::<Vec<_>>()
        );
    }
}
