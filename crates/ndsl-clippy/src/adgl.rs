//! First-wave ADGL style lint pack (`ADGLS0001`–`ADGLS0299`).
//!
//! Registered by [`register_adgl_pack`] from [`crate::builtin::register_builtins`].
//! Uses the canonical Rust parser (`airpulse_dsl_syntax::parse_ruleset`) — not tree-sitter.

use crate::{LintCheck, LintContext, LintDef, LintDiagnostic, LintId, LintLevel, LintStore};
use airpulse_dsl_syntax::ast::{
    ActionField, ActionStmt, AnchorBlock, CorrelateBlock, DecisionAnchor, DecisionRule, EmitField,
    EmitStmt, EvidenceRule, Expr, ExprKind, InferField, InferStmt, RuleDecl, Ruleset, Stmt,
};
use ndsl_diag::Span;
use std::collections::HashSet;

/// Correlate binding is never referenced (`present`/`absent`, evidence lists, exprs).
pub const ADGLS_UNUSED_CORRELATE: LintId = LintId::new("ADGLS0001");
/// Float literal in source (units ABI is i64 — use per-mille / centi / ms integers).
pub const ADGLS_FLOAT_LITERAL: LintId = LintId::new("ADGLS0100");
/// `having: count >= 1` is redundant with the omitted default (empty / no-op having).
pub const ADGLS_EMPTY_HAVING: LintId = LintId::new("ADGLS0200");

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
    fn healthy_pmtud_snippet_has_no_adgl_pack_lints() {
        let src = include_str!("../../../docs/idea/examples/01-pmtud-blackhole.adgl");
        let diags = lint(src);
        let pack: Vec<_> = diags
            .iter()
            .filter(|d| {
                matches!(
                    d.diagnostic.id.as_str(),
                    "ADGLS0001" | "ADGLS0100" | "ADGLS0200"
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
