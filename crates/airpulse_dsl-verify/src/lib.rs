#![deny(unsafe_code)]
#![warn(missing_docs)]

//! ADGL AOT verifier and lowering to `airpulse_dsl_ir::ProgramImage`.

use std::collections::{BTreeSet, HashMap, HashSet};

use airpulse_dsl_catalog::{
    ActionArgKind, ActionTargetType, EventOrBindingType, FieldType, capability_for, catalog_ref, check_kinds,
    observation_kinds, resolve_action, resolve_cause, resolve_event, resolve_metric_path, resolve_problem,
    resolve_topo_fn,
};
use airpulse_dsl_ir::{
    AnchorSource, AnchorSpec, BranchTable, CorrelateSource, CorrelateSpec, ExclusivityGroup, Intent, PredOp,
    Predicate, ProgramImage, ProvKey, RuleInstance, RuleKind, Symbol, TopoCall, VerifiedAnnotations, WindowProof,
};
use airpulse_dsl_syntax::ast::{
    ActionField, ActionName, ActionStmt, BinaryOp, CorrelateBlock, CorrelateSource as AstCorrelateSource,
    DecisionAnchor, Decl, EmitField, EmitStmt, Expr, ExprKind, InferField, InferStmt, KindIdent,
    RuleDecl, Ruleset, Stmt, UnaryOp,
};
use airpulse_dsl_syntax::parse_ruleset;
use airpulse_dsl_types::{
    ActionKind, Capability, CauseKind, DurationMs, EventType, MetricPath, ProblemKind, RuleId, ScopeType, Severity,
    Weight, stable_hash_u64, stable_string_i64,
};
use ariadne::{Color, Label, Report, ReportKind, Source};
use ndsl_diag::{DiagBuffer, Diagnostic, Span};

const DEFAULT_MAX_LOOKBACK_MS: i64 = 60_000;
const DEFAULT_DEDUP_WINDOW_MS: i64 = 1_000;
const MAX_NESTING: usize = 64;
const MAX_CORRELATES_PER_RULE: usize = 8;
const MAX_INTENTS_PER_RULE: usize = 16;
const MAX_REQUIRES: usize = 32;
const MAX_EXCLUSIVITY_GROUP: usize = 8;

/// Verified ADGL artifact.
#[derive(Debug, Clone)]
pub struct VerifiedProgram {
    /// Lowered verified image.
    pub image: ProgramImage,
    /// Conservative type-system judgement calls applied by this implementation.
    pub conservative_judgements: Vec<&'static str>,
}

/// Verifier configuration values that come from runtime limits/config.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VerifyConfig {
    /// Maximum lookback horizon used by temporal proof checks.
    pub max_lookback_ms: i64,
    /// Dedup window used by runtime provenance bucketing.
    pub dedup_window_ms: i64,
}

impl Default for VerifyConfig {
    fn default() -> Self {
        Self { max_lookback_ms: DEFAULT_MAX_LOOKBACK_MS, dedup_window_ms: DEFAULT_DEDUP_WINDOW_MS }
    }
}

/// Verifies already parsed ruleset and lowers it to IR.
pub fn verify(ruleset: &Ruleset<'_>) -> Result<VerifiedProgram, DiagBuffer> {
    verify_with_config(ruleset, VerifyConfig::default())
}

/// Verifies already parsed ruleset and lowers it to IR with explicit limits config.
pub fn verify_with_config(ruleset: &Ruleset<'_>, config: VerifyConfig) -> Result<VerifiedProgram, DiagBuffer> {
    let mut diags = DiagBuffer::new();
    let mut state = VerifyState::new();

    phase01_catalog_resolution(ruleset, &mut diags);
    phase02_name_and_typecheck(ruleset, &mut state, &mut diags);
    phase03_effect_in_pure(ruleset, &mut diags);
    phase04_capability(ruleset, &mut diags);
    phase05_scope_target(ruleset, &mut diags);
    phase06_temporal(ruleset, config, &mut diags);
    phase07_topology_signatures(ruleset, &mut diags);
    phase08_acyclicity(ruleset, &mut diags);
    phase09_exclusivity(ruleset, &mut diags);
    phase10_bipartite(ruleset, &mut diags);
    phase11_dos_limits(ruleset, config, &mut diags);
    phase12_privacy_annotations(ruleset, &mut state);
    if diags.has_errors() {
        return Err(diags);
    }

    let image = lower_to_image(ruleset, &mut state, &mut diags);
    if diags.has_errors() {
        return Err(diags);
    }

    Ok(VerifiedProgram {
        image,
        conservative_judgements: vec![
            "Treats non-comparison arithmetic predicates as integer-only and rejects unresolved cases.",
            "Treats call expressions in pure predicate/window positions as ADGL0501 regardless of callee name.",
            "Infers cause/problem target scope from catalog valid scopes only when unambiguous.",
            "Requires explicit metric-path style target expressions for lowering.",
        ],
    })
}

/// Parses, verifies and lowers ADGL source.
pub fn verify_source(src: &str) -> Result<VerifiedProgram, DiagBuffer> {
    let ast = parse_ruleset(src)?;
    verify_with_config(&ast, VerifyConfig::default())
}

/// Parses, verifies and lowers ADGL source with explicit limits config.
pub fn verify_source_with_config(src: &str, config: VerifyConfig) -> Result<VerifiedProgram, DiagBuffer> {
    let ast = parse_ruleset(src)?;
    verify_with_config(&ast, config)
}

/// Renders diagnostics with ariadne for a source file.
#[must_use]
pub fn render_diagnostics(src: &str, file: &str, diags: &DiagBuffer) -> String {
    let mut out = String::new();
    for d in diags.iter() {
        let mut rendered = Vec::<u8>::new();
        let kind = match d.severity {
            ndsl_diag::Severity::Error => ReportKind::Error,
            ndsl_diag::Severity::Warning => ReportKind::Warning,
            ndsl_diag::Severity::Note => ReportKind::Advice,
        };
        let report = Report::build(kind, file, d.span.start)
            .with_code(d.code)
            .with_message(d.message.clone())
            .with_label(
                Label::new((file, d.span.start..d.span.end.max(d.span.start + 1)))
                    .with_color(Color::Red)
                    .with_message(d.message.clone()),
            )
            .finish();
        if report.write((file, Source::from(src)), &mut rendered).is_ok() {
            out.push_str(&String::from_utf8_lossy(&rendered));
        } else {
            out.push_str(&format!("{file}:{}: {} {}\n", d.span.start, d.code, d.message));
        }
    }
    out
}

#[derive(Debug, Default)]
struct VerifyState {
    pii_by_binding_path: HashSet<String>,
}

impl VerifyState {
    fn new() -> Self {
        Self::default()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum BindingTy {
    Event(EventType),
    Cause(CauseKind),
    Problem(ProblemKind),
}

#[derive(Debug, Clone)]
struct RuleEnv {
    rule_scope: ScopeType,
    bindings: HashMap<String, BindingTy>,
    binding_idx: HashMap<String, u8>,
}

impl RuleEnv {
    fn for_rule(rule: &RuleDecl<'_>) -> Self {
        let mut bindings = HashMap::new();
        let mut binding_idx = HashMap::new();
        match rule {
            RuleDecl::Evidence(e) => {
                let event_ty = EventType::new(kind_ident_name(&e.anchor.event_type));
                bindings.insert(e.anchor.binding.name.to_string(), BindingTy::Event(event_ty));
                binding_idx.insert(e.anchor.binding.name.to_string(), 0);
                for (i, c) in e.correlates.iter().enumerate() {
                    if let Some(b) = correlate_binding_ty(c) {
                        bindings.insert(c.binding.name.to_string(), b);
                        binding_idx.insert(c.binding.name.to_string(), (i + 1) as u8);
                    }
                }
                Self { rule_scope: e.scope, bindings, binding_idx }
            }
            RuleDecl::Decision(d) => {
                match &d.anchor {
                    DecisionAnchor::Cause(c) => {
                        bindings.insert(c.binding.name.to_string(), BindingTy::Cause(CauseKind::new(c.cause.name)));
                        binding_idx.insert(c.binding.name.to_string(), 0);
                    }
                    DecisionAnchor::Problem(p) => {
                        bindings.insert(
                            p.binding.name.to_string(),
                            BindingTy::Problem(ProblemKind::new(p.problem.name)),
                        );
                        binding_idx.insert(p.binding.name.to_string(), 0);
                    }
                }
                for (i, c) in d.correlates.iter().enumerate() {
                    if let Some(b) = correlate_binding_ty(c) {
                        bindings.insert(c.binding.name.to_string(), b);
                        binding_idx.insert(c.binding.name.to_string(), (i + 1) as u8);
                    }
                }
                Self { rule_scope: d.scope, bindings, binding_idx }
            }
        }
    }
}

// ===== Phase 1: Catalog Resolution =====
fn phase01_catalog_resolution(ruleset: &Ruleset<'_>, diags: &mut DiagBuffer) {
    for rule in &ruleset.rules {
        match rule {
            RuleDecl::Evidence(e) => {
                if resolve_event(&kind_ident_name(&e.anchor.event_type)).is_none() {
                    err(diags, "ADGL0201", "unknown event type", e.anchor.event_type.span);
                }
                for c in &e.correlates {
                    match &c.source {
                        AstCorrelateSource::Event(k) => {
                            if resolve_event(&kind_ident_name(k)).is_none() {
                                err(diags, "ADGL0201", "unknown event type", k.span);
                            }
                        }
                        AstCorrelateSource::Cause(id) => {
                            if resolve_cause(id.name).is_none() {
                                err(diags, "ADGL0202", "unknown cause kind", id.span);
                            }
                        }
                        AstCorrelateSource::Problem(id) => {
                            if resolve_problem(id.name).is_none() {
                                err(diags, "ADGL0203", "unknown problem kind", id.span);
                            }
                        }
                    }
                }
                validate_stmt_catalog(rule, &e.body, diags);
                if let Some(if_else) = &e.if_else {
                    validate_stmt_catalog(rule, &if_else.then_body, diags);
                    if let Some(else_body) = &if_else.else_body {
                        validate_stmt_catalog(rule, else_body, diags);
                    }
                }
            }
            RuleDecl::Decision(d) => {
                match &d.anchor {
                    DecisionAnchor::Cause(c) => {
                        if resolve_cause(c.cause.name).is_none() {
                            err(diags, "ADGL0202", "unknown cause kind", c.cause.span);
                        }
                    }
                    DecisionAnchor::Problem(p) => {
                        if resolve_problem(p.problem.name).is_none() {
                            err(diags, "ADGL0203", "unknown problem kind", p.problem.span);
                        }
                    }
                }
                for c in &d.correlates {
                    match &c.source {
                        AstCorrelateSource::Event(k) => {
                            if resolve_event(&kind_ident_name(k)).is_none() {
                                err(diags, "ADGL0201", "unknown event type", k.span);
                            }
                        }
                        AstCorrelateSource::Cause(id) => {
                            if resolve_cause(id.name).is_none() {
                                err(diags, "ADGL0202", "unknown cause kind", id.span);
                            }
                        }
                        AstCorrelateSource::Problem(id) => {
                            if resolve_problem(id.name).is_none() {
                                err(diags, "ADGL0203", "unknown problem kind", id.span);
                            }
                        }
                    }
                }
                validate_stmt_catalog(rule, &d.body, diags);
                if let Some(if_else) = &d.if_else {
                    validate_stmt_catalog(rule, &if_else.then_body, diags);
                    if let Some(else_body) = &if_else.else_body {
                        validate_stmt_catalog(rule, else_body, diags);
                    }
                }
            }
        }
    }
}

fn validate_stmt_catalog(rule: &RuleDecl<'_>, stmts: &[Stmt<'_>], diags: &mut DiagBuffer) {
    let env = RuleEnv::for_rule(rule);
    for stmt in stmts {
        match stmt {
            Stmt::Infer(i) => {
                if resolve_cause(i.cause.name).is_none() {
                    err(diags, "ADGL0202", "unknown cause kind", i.cause.span);
                }
                for f in &i.fields {
                    if let InferField::Evidence(ids, span) = f {
                        for id in ids {
                            if !env.bindings.contains_key(id.name) {
                                err(diags, "ADGL0209", "unknown evidence binding", *span);
                            }
                        }
                    }
                }
            }
            Stmt::Emit(e) => {
                if resolve_problem(e.problem.name).is_none() {
                    err(diags, "ADGL0203", "unknown problem kind", e.problem.span);
                }
                for f in &e.fields {
                    if let EmitField::Evidence(ids, span) = f {
                        for id in ids {
                            if !env.bindings.contains_key(id.name) {
                                err(diags, "ADGL0209", "unknown evidence binding", *span);
                            }
                        }
                    }
                }
            }
            Stmt::Action(a) => {
                let name = action_name(a);
                let Some(action_schema) = resolve_action(name) else {
                    err(diags, "ADGL0206", "unknown action kind", a.span);
                    continue;
                };
                match action_schema.arg_kind {
                    ActionArgKind::ObservationKind => {
                        let Some(arg) = &a.arg else {
                            err(diags, "ADGL0207", "missing observation kind argument", a.span);
                            continue;
                        };
                        let arg_name = kind_ident_name(arg);
                        if !observation_kinds().iter().any(|k| *k == arg_name) {
                            err(diags, "ADGL0207", "unknown observation kind", arg.span);
                        }
                    }
                    ActionArgKind::CheckKind => {
                        let Some(arg) = &a.arg else {
                            err(diags, "ADGL0208", "missing check kind argument", a.span);
                            continue;
                        };
                        let arg_name = kind_ident_name(arg);
                        if !check_kinds().iter().any(|k| *k == arg_name) {
                            err(diags, "ADGL0208", "unknown check kind", arg.span);
                        }
                    }
                    ActionArgKind::ProblemRefBinding => {
                        let Some(arg) = &a.arg else {
                            err(diags, "ADGL0209", "missing suppress_symptom binding argument", a.span);
                            continue;
                        };
                        if arg.segments.len() != 1 {
                            err(diags, "ADGL0209", "suppress_symptom argument must be a ProblemRef binding", arg.span);
                            continue;
                        }
                        let id = arg.segments[0].name.to_string();
                        match env.bindings.get(&id) {
                            Some(BindingTy::Problem(_)) => {}
                            _ => {
                                err(
                                    diags,
                                    "ADGL0209",
                                    "suppress_symptom argument must be a ProblemRef binding",
                                    arg.span,
                                );
                            }
                        }
                    }
                    ActionArgKind::None => {
                        if a.arg.is_some() {
                            err(diags, "ADGL0206", "action does not accept argument", a.span);
                        }
                    }
                }
            }
        }
    }
}

// ===== Phase 2: Name Resolution + Type Check =====
fn phase02_name_and_typecheck(ruleset: &Ruleset<'_>, state: &mut VerifyState, diags: &mut DiagBuffer) {
    for rule in &ruleset.rules {
        let env = RuleEnv::for_rule(rule);
        let mut exprs = Vec::new();
        match rule {
            RuleDecl::Evidence(e) => {
                if let Some(pred) = &e.anchor.predicate {
                    exprs.push(pred);
                }
                for c in &e.correlates {
                    exprs.push(&c.time.probe);
                    exprs.push(&c.time.start);
                    exprs.push(&c.time.end);
                    for arg in &c.topo.args {
                        exprs.push(arg);
                    }
                }
                collect_stmt_exprs(&e.body, &mut exprs);
                if let Some(if_else) = &e.if_else {
                    exprs.push(&if_else.condition);
                    collect_stmt_exprs(&if_else.then_body, &mut exprs);
                    if let Some(else_body) = &if_else.else_body {
                        collect_stmt_exprs(else_body, &mut exprs);
                    }
                }
            }
            RuleDecl::Decision(d) => {
                match &d.anchor {
                    DecisionAnchor::Cause(c) => exprs.push(&c.predicate),
                    DecisionAnchor::Problem(p) => {
                        if let Some(pred) = &p.predicate {
                            exprs.push(pred);
                        }
                    }
                }
                for c in &d.correlates {
                    exprs.push(&c.time.probe);
                    exprs.push(&c.time.start);
                    exprs.push(&c.time.end);
                    for arg in &c.topo.args {
                        exprs.push(arg);
                    }
                }
                collect_stmt_exprs(&d.body, &mut exprs);
                if let Some(if_else) = &d.if_else {
                    exprs.push(&if_else.condition);
                    collect_stmt_exprs(&if_else.then_body, &mut exprs);
                    if let Some(else_body) = &if_else.else_body {
                        collect_stmt_exprs(else_body, &mut exprs);
                    }
                }
            }
        }
        for expr in exprs {
            infer_expr_type(expr, &env, state, diags);
        }
        let mut stmts = Vec::new();
        gather_rule_statements(rule, &mut stmts);
        for stmt in stmts {
            match stmt {
                Stmt::Infer(infer) => {
                    let has_target = infer.fields.iter().any(|field| matches!(field, InferField::Target(_, _)));
                    if !has_target {
                        // 04-type-system.md §7 (T-Infer) requires an explicit target expression.
                        err(diags, "ADGL0210", "infer requires an explicit target field", infer.span);
                    }
                    for field in &infer.fields {
                        if let Some((weight, span)) = field.weight_value()
                            && (weight < i64::from(i8::MIN) || weight > i64::from(i8::MAX) || !(-100..=100).contains(&weight))
                        {
                            err(diags, "ADGL0205", "weight must be within [-100, 100]", span);
                        }
                    }
                }
                Stmt::Action(action) => validate_action_target_types(action, &env, diags),
                Stmt::Emit(_) => {}
            }
        }
    }
}

// ===== Phase 3: EffectInPurePosition =====
fn phase03_effect_in_pure(ruleset: &Ruleset<'_>, diags: &mut DiagBuffer) {
    for rule in &ruleset.rules {
        match rule {
            RuleDecl::Evidence(e) => {
                if let Some(pred) = &e.anchor.predicate {
                    check_no_calls(pred, diags);
                }
                for c in &e.correlates {
                    check_no_calls(&c.time.start, diags);
                    check_no_calls(&c.time.end, diags);
                }
                if let Some(if_else) = &e.if_else {
                    check_no_calls(&if_else.condition, diags);
                }
            }
            RuleDecl::Decision(d) => {
                match &d.anchor {
                    DecisionAnchor::Cause(c) => check_no_calls(&c.predicate, diags),
                    DecisionAnchor::Problem(p) => {
                        if let Some(pred) = &p.predicate {
                            check_no_calls(pred, diags);
                        }
                    }
                }
                for c in &d.correlates {
                    check_no_calls(&c.time.start, diags);
                    check_no_calls(&c.time.end, diags);
                }
                if let Some(if_else) = &d.if_else {
                    check_no_calls(&if_else.condition, diags);
                }
            }
        }
    }
}

// ===== Phase 4: Capability Check =====
fn phase04_capability(ruleset: &Ruleset<'_>, diags: &mut DiagBuffer) {
    for decl in &ruleset.header.decls {
        if let Decl::Requires(requires) = decl {
            for cap in &requires.capabilities {
                if capability_for(&cap.value).is_none() {
                    err(diags, "ADGL0430", "unknown capability", cap.span);
                }
            }
        }
    }
}

// ===== Phase 5: Scope/Target Compatibility =====
fn phase05_scope_target(ruleset: &Ruleset<'_>, diags: &mut DiagBuffer) {
    for rule in &ruleset.rules {
        let env = RuleEnv::for_rule(rule);
        let rule_scope = env.rule_scope;
        let mut stmts = Vec::new();
        gather_rule_statements(rule, &mut stmts);
        for stmt in stmts {
            match stmt {
                Stmt::Infer(infer) => {
                    let target_scope = infer_target_scope_from_infer(infer, &env).unwrap_or(rule_scope);
                    if !rule_scope.is_subsumed_by(target_scope) {
                        err(diags, "ADGL0210", "target scope not compatible with rule scope", infer.span);
                    }
                    if let Some(schema) = resolve_cause(infer.cause.name) {
                        if !schema.valid_scopes.iter().any(|s| *s == target_scope) {
                            err(diags, "ADGL0211", "cause is invalid for target scope", infer.span);
                        }
                    }
                }
                Stmt::Emit(emit) => {
                    let target_scope = infer_target_scope_from_emit(emit, &env).unwrap_or(rule_scope);
                    if !rule_scope.is_subsumed_by(target_scope) {
                        err(diags, "ADGL0210", "target scope not compatible with rule scope", emit.span);
                    }
                    if let Some(schema) = resolve_problem(emit.problem.name) {
                        if !schema.valid_scopes.iter().any(|s| *s == target_scope) {
                            err(diags, "ADGL0212", "problem is invalid for target scope", emit.span);
                        }
                    }
                }
                Stmt::Action(_) => {}
            }
        }
    }
}

// ===== Phase 6: Temporal Bounds =====
fn phase06_temporal(ruleset: &Ruleset<'_>, config: VerifyConfig, diags: &mut DiagBuffer) {
    // 05-verification.md §3.1:
    //   slack >= max_forward_window and each bound <= MAX_LOOKBACK - slack.
    // We compute slack from the ruleset itself as max calculable forward window.
    let slack_ms = ruleset_max_forward_window_ms(ruleset);
    let allowed_window_ms = config.max_lookback_ms.saturating_sub(slack_ms);
    for rule in &ruleset.rules {
        let anchor_binding = match rule {
            RuleDecl::Evidence(e) => e.anchor.binding.name,
            RuleDecl::Decision(d) => match &d.anchor {
                DecisionAnchor::Cause(c) => c.binding.name,
                DecisionAnchor::Problem(p) => p.binding.name,
            },
        };
        let correlates = match rule {
            RuleDecl::Evidence(e) => &e.correlates,
            RuleDecl::Decision(d) => &d.correlates,
        };
        for c in correlates {
            if !is_binding_time_probe(&c.time.probe, c.binding.name) {
                err(diags, "ADGL0413", "malformed window probe, expected <binding>.time", c.time.span);
            }
            match calculable_window(anchor_binding, &c.time.start, &c.time.end) {
                Some((back, forward)) => {
                    if back > allowed_window_ms || forward > allowed_window_ms {
                        err(diags, "ADGL0412", "window exceeds MAX_LOOKBACK", c.time.span);
                    }
                }
                None => err(diags, "ADGL0411", "non calculable window expression", c.time.span),
            }
        }
    }
}

// ===== Phase 7: Topology Signatures =====
fn phase07_topology_signatures(ruleset: &Ruleset<'_>, diags: &mut DiagBuffer) {
    for rule in &ruleset.rules {
        let env = RuleEnv::for_rule(rule);
        let correlates = match rule {
            RuleDecl::Evidence(e) => &e.correlates,
            RuleDecl::Decision(d) => &d.correlates,
        };
        for c in correlates {
            let Some(topo_fn) = resolve_topo_fn(c.topo.name.name) else {
                err(diags, "ADGL0420", "unknown topology function", c.topo.name.span);
                continue;
            };
            if topo_fn.arity != c.topo.args.len() {
                err(diags, "ADGL0421", "topology function arity mismatch", c.topo.span);
            }
            let mut arg_scopes = Vec::new();
            for arg in &c.topo.args {
                match infer_target_scope_expr(arg, &env) {
                    Some(scope) => arg_scopes.push(scope),
                    None => err(diags, "ADGL0421", "topology args must be scope-id paths", arg.span),
                }
            }
            if c.topo.name.name == "upstream_of" && arg_scopes.len() == 2 && arg_scopes[0] != arg_scopes[1] {
                err(diags, "ADGL0421", "upstream_of requires same scope type for both args", c.topo.span);
            }
        }
    }
}

// ===== Phase 8: Rule-DAG Acyclicity =====
fn phase08_acyclicity(ruleset: &Ruleset<'_>, diags: &mut DiagBuffer) {
    let mut cause_anchor_rules: HashMap<String, Vec<usize>> = HashMap::new();
    let mut problem_anchor_rules: HashMap<String, Vec<usize>> = HashMap::new();
    for (idx, rule) in ruleset.rules.iter().enumerate() {
        if let RuleDecl::Decision(d) = rule {
            match &d.anchor {
                DecisionAnchor::Cause(c) => cause_anchor_rules.entry(c.cause.name.to_string()).or_default().push(idx),
                DecisionAnchor::Problem(p) => {
                    problem_anchor_rules.entry(p.problem.name.to_string()).or_default().push(idx)
                }
            }
        }
    }
    let mut edges: Vec<Vec<usize>> = vec![Vec::new(); ruleset.rules.len()];
    for (idx, rule) in ruleset.rules.iter().enumerate() {
        let mut stmts = Vec::new();
        gather_rule_statements(rule, &mut stmts);
        for stmt in stmts {
            match stmt {
                Stmt::Infer(i) => {
                    if let Some(targets) = cause_anchor_rules.get(i.cause.name) {
                        for t in targets {
                            edges[idx].push(*t);
                        }
                    }
                }
                Stmt::Emit(e) => {
                    if let Some(targets) = problem_anchor_rules.get(e.problem.name) {
                        for t in targets {
                            edges[idx].push(*t);
                        }
                    }
                }
                Stmt::Action(_) => {}
            }
        }
    }
    if has_cycle(&edges) {
        err(diags, "ADGL0410", "cyclic rule dependency detected", ruleset.span);
    }
}

// ===== Phase 9: Exclusivity =====
fn phase09_exclusivity(ruleset: &Ruleset<'_>, diags: &mut DiagBuffer) {
    let mut seen_pairs = HashSet::<(String, String)>::new();
    for decl in &ruleset.header.decls {
        if let Decl::MutuallyExclusive(group) = decl {
            if group.idents.len() > MAX_EXCLUSIVITY_GROUP {
                err(
                    diags,
                    "ADGL0441",
                    "mutually_exclusive group is too large",
                    group.span,
                );
            }
            let mut scopes_by_cause = HashMap::new();
            for id in &group.idents {
                let Some(schema) = resolve_cause(id.name) else {
                    err(diags, "ADGL0202", "unknown cause in exclusivity group", id.span);
                    continue;
                };
                scopes_by_cause.insert(id.name.to_string(), schema.valid_scopes.to_vec());
            }
            for i in 0..group.idents.len() {
                for j in (i + 1)..group.idents.len() {
                    let a = group.idents[i].name.to_string();
                    let b = group.idents[j].name.to_string();
                    let pair = if a <= b { (a.clone(), b.clone()) } else { (b.clone(), a.clone()) };
                    if !seen_pairs.insert(pair) {
                        err(diags, "ADGL0440", "overlapping exclusivity pair", group.span);
                    }
                    if let (Some(sa), Some(sb)) = (scopes_by_cause.get(&a), scopes_by_cause.get(&b)) {
                        let overlap = sa.iter().any(|s| sb.iter().any(|x| x == s));
                        if !overlap {
                            warn(
                                diags,
                                "ADGL0502",
                                "redundant exclusivity pair has no common scope",
                                group.span,
                            );
                        }
                    }
                }
            }
        }
    }
}

// ===== Phase 10: Bipartite Isolation =====
fn phase10_bipartite(ruleset: &Ruleset<'_>, diags: &mut DiagBuffer) {
    for rule in &ruleset.rules {
        match rule {
            RuleDecl::Evidence(e) => {
                let mut stmts = Vec::new();
                stmts.extend(e.body.iter());
                if let Some(if_else) = &e.if_else {
                    stmts.extend(if_else.then_body.iter());
                    if let Some(else_body) = &if_else.else_body {
                        stmts.extend(else_body.iter());
                    }
                }
                for stmt in stmts {
                    if matches!(stmt, Stmt::Emit(_)) {
                        err(diags, "ADGL0450", "evidence rule cannot emit Problem", e.span);
                    }
                }
            }
            RuleDecl::Decision(d) => {
                let mut stmts = Vec::new();
                stmts.extend(d.body.iter());
                if let Some(if_else) = &d.if_else {
                    stmts.extend(if_else.then_body.iter());
                    if let Some(else_body) = &if_else.else_body {
                        stmts.extend(else_body.iter());
                    }
                }
                for stmt in stmts {
                    if matches!(stmt, Stmt::Infer(_)) {
                        err(diags, "ADGL0450", "decision rule cannot infer Cause", d.span);
                    }
                }
            }
        }
    }
}

// ===== Phase 11: DoS Limits =====
fn phase11_dos_limits(ruleset: &Ruleset<'_>, config: VerifyConfig, diags: &mut DiagBuffer) {
    // ADGL0503 is config-derived (ADR-011 dedup_window), not source-derived:
    // we validate it when explicit verify config is provided.
    if config.dedup_window_ms < 1 {
        err(diags, "ADGL0503", "dedup window must be at least 1ms", ruleset.span);
    }
    let mut requires_count = 0usize;
    for decl in &ruleset.header.decls {
        if let Decl::Requires(requires) = decl {
            requires_count += requires.capabilities.len();
        }
    }
    if requires_count > MAX_REQUIRES {
        err(diags, "ADGL0105", "too many requires entries", ruleset.span);
    }
    for rule in &ruleset.rules {
        let (correlates, if_else, body, span) = match rule {
            RuleDecl::Evidence(e) => (&e.correlates, &e.if_else, &e.body, e.span),
            RuleDecl::Decision(d) => (&d.correlates, &d.if_else, &d.body, d.span),
        };
        if correlates.len() > MAX_CORRELATES_PER_RULE {
            err(diags, "ADGL0204", "too many correlate blocks", span);
        }
        let mut stmt_count = body.len();
        if let Some(if_else) = if_else {
            stmt_count += if_else.then_body.len();
            if let Some(else_body) = &if_else.else_body {
                stmt_count += else_body.len();
            }
        }
        if stmt_count > MAX_INTENTS_PER_RULE {
            err(diags, "ADGL0205", "too many infer/emit/action intents", span);
        }
        if max_expr_depth_rule(rule) > MAX_NESTING {
            err(diags, "ADGL0103", "nesting exceeds supported maximum", span);
        }
    }
}

// ===== Phase 12: Privacy Annotations =====
fn phase12_privacy_annotations(ruleset: &Ruleset<'_>, state: &mut VerifyState) {
    state.pii_by_binding_path.clear();
    for rule in &ruleset.rules {
        let env = RuleEnv::for_rule(rule);
        for (binding, ty) in env.bindings {
            if let BindingTy::Event(event) = ty {
                if let Some(schema) = resolve_event(event.as_str()) {
                    for field in schema.fields.iter().filter(|f| f.pii) {
                        state.pii_by_binding_path.insert(format!("{binding}.{}", field.name));
                    }
                }
            }
        }
    }
}

// ===== Lowering: Typed AST -> ProgramImage =====
fn lower_to_image(ruleset: &Ruleset<'_>, state: &mut VerifyState, diags: &mut DiagBuffer) -> ProgramImage {
    let mut rules = Vec::new();
    for rule in &ruleset.rules {
        let env = RuleEnv::for_rule(rule);
        if let Some(rule_instance) = lower_rule(rule, &env, state, diags) {
            rules.push(rule_instance);
        }
    }
    let mut requires = Vec::<Capability>::new();
    let mut exclusivity = Vec::<ExclusivityGroup>::new();
    for decl in &ruleset.header.decls {
        match decl {
            Decl::Requires(r) => {
                requires.extend(r.capabilities.iter().map(|s| Capability::new(s.value.clone())));
            }
            Decl::MutuallyExclusive(group) => {
                exclusivity.push(ExclusivityGroup {
                    causes: group
                        .idents
                        .iter()
                        .map(|id| CauseKind::new(id.name.to_string()))
                        .collect::<Vec<_>>()
                        .into_boxed_slice(),
                });
            }
        }
    }

    ProgramImage::new(
        parse_version(&ruleset.header.version.value),
        ruleset.name.value.clone(),
        requires.into_boxed_slice(),
        exclusivity.into_boxed_slice(),
        rules.into_boxed_slice(),
        catalog_ref(),
    )
}

fn lower_rule(rule: &RuleDecl<'_>, env: &RuleEnv, state: &VerifyState, diags: &mut DiagBuffer) -> Option<RuleInstance> {
    let (id, kind, scope, anchor_binding_name, anchor_spec, correlates_ast, if_else, body, _span) = match rule {
        RuleDecl::Evidence(e) => {
            let anchor_source = AnchorSource::Event(EventType::new(kind_ident_name(&e.anchor.event_type)));
            let pred = e
                .anchor
                .predicate
                .as_ref()
                .map(|p| compile_predicate(p, env, diags))
                .unwrap_or_else(Predicate::always_true);
            (
                RuleId::new(e.name.name.to_string()),
                RuleKind::Evidence,
                e.scope,
                e.anchor.binding.name.to_string(),
                AnchorSpec { binding: Symbol::new(e.anchor.binding.name.to_string()), source: anchor_source, predicate: pred },
                &e.correlates,
                &e.if_else,
                &e.body,
                e.span,
            )
        }
        RuleDecl::Decision(d) => {
            let (binding, source, pred) = match &d.anchor {
                DecisionAnchor::Cause(c) => (
                    c.binding.name.to_string(),
                    AnchorSource::Cause(CauseKind::new(c.cause.name.to_string())),
                    compile_predicate(&c.predicate, env, diags),
                ),
                DecisionAnchor::Problem(p) => (
                    p.binding.name.to_string(),
                    AnchorSource::Problem(ProblemKind::new(p.problem.name.to_string())),
                    p.predicate
                        .as_ref()
                        .map(|p| compile_predicate(p, env, diags))
                        .unwrap_or_else(Predicate::always_true),
                ),
            };
            (
                RuleId::new(d.name.name.to_string()),
                RuleKind::Decision,
                d.scope,
                binding.clone(),
                AnchorSpec { binding: Symbol::new(binding), source, predicate: pred },
                &d.correlates,
                &d.if_else,
                &d.body,
                d.span,
            )
        }
    };

    let correlates = correlates_ast
        .iter()
        .map(|c| lower_correlate(c, env, &anchor_binding_name, diags))
        .collect::<Option<Vec<_>>>()?;
    let max_backward = correlates
        .iter()
        .filter_map(|c| match c.window {
            WindowProof::Calculable { back, .. } => Some(back),
            WindowProof::RuntimeCheck => None,
        })
        .max()
        .unwrap_or_default();
    let max_forward = correlates
        .iter()
        .filter_map(|c| match c.window {
            WindowProof::Calculable { forward, .. } => Some(forward),
            WindowProof::RuntimeCheck => None,
        })
        .max()
        .unwrap_or_default();

    let mut unconditional = Vec::new();
    for stmt in body {
        if let Some(intent) = lower_stmt(stmt, env, &id, scope, state, diags) {
            unconditional.extend(intent);
        }
    }

    let branches = if let Some(if_else) = if_else {
        let cond = compile_predicate(&if_else.condition, env, diags);
        let mut then_body = Vec::new();
        for stmt in &if_else.then_body {
            if let Some(intent) = lower_stmt(stmt, env, &id, scope, state, diags) {
                then_body.extend(intent);
            }
        }
        let mut else_intents = Vec::new();
        if let Some(else_body) = &if_else.else_body {
            for stmt in else_body {
                if let Some(intent) = lower_stmt(stmt, env, &id, scope, state, diags) {
                    else_intents.extend(intent);
                }
            }
        }
        let unknown_body = if contains_present_absent(&if_else.condition) {
            // 06-ir-bytecode.md §3.1 requires unknown_body synthesis when branch
            // conditions may evaluate to Unknown due to topology-dependent correlate state.
            // We conservatively over-approximate with any present/absent primary.
            vec![Intent::EmitAction {
                kind: ActionKind::RequestTopology,
                arg: None,
                target: None,
                reason: None,
                evidence: Box::new([]),
            }]
        } else {
            Vec::new()
        };
        Some(BranchTable {
            cond,
            then_body: then_body.into_boxed_slice(),
            else_body: if else_intents.is_empty() { None } else { Some(else_intents.into_boxed_slice()) },
            unknown_body: unknown_body.into_boxed_slice(),
        })
    } else {
        None
    };

    Some(RuleInstance {
        id,
        kind,
        scope,
        anchor: anchor_spec,
        correlates: correlates.into_boxed_slice(),
        branches,
        body: unconditional.into_boxed_slice(),
        annotations: VerifiedAnnotations { max_backward, max_forward, target_scope: Some(scope) },
    })
}

fn lower_correlate(
    c: &CorrelateBlock<'_>,
    _env: &RuleEnv,
    anchor_binding: &str,
    diags: &mut DiagBuffer,
) -> Option<CorrelateSpec> {
    let source = match &c.source {
        AstCorrelateSource::Event(e) => CorrelateSource::Event(EventType::new(kind_ident_name(e))),
        AstCorrelateSource::Cause(k) => CorrelateSource::Cause(CauseKind::new(k.name.to_string())),
        AstCorrelateSource::Problem(k) => CorrelateSource::Problem(ProblemKind::new(k.name.to_string())),
    };
    let topo_fn = resolve_topo_fn(c.topo.name.name)?;
    let mut args = Vec::new();
    for arg in &c.topo.args {
        if let Some(path) = metric_path_from_expr(arg) {
            args.push(path);
        } else {
            err(diags, "ADGL0421", "topology arg must be a metric path", arg.span);
            return None;
        }
    }
    let window = if let Some((back, forward)) = calculable_window(anchor_binding, &c.time.start, &c.time.end) {
        let back = DurationMs::from_millis(back).unwrap_or_default();
        let forward = DurationMs::from_millis(forward).unwrap_or_default();
        WindowProof::Calculable { back, forward }
    } else {
        WindowProof::RuntimeCheck
    };
    Some(CorrelateSpec {
        binding: Symbol::new(c.binding.name.to_string()),
        source,
        topo: TopoCall {
            func: c.topo.name.name.into(),
            func_idx: topo_fn.func_idx,
            args: args.into_boxed_slice(),
        },
        window,
    })
}

fn lower_stmt(
    stmt: &Stmt<'_>,
    env: &RuleEnv,
    rule_id: &RuleId,
    rule_scope: ScopeType,
    state: &VerifyState,
    diags: &mut DiagBuffer,
) -> Option<Vec<Intent>> {
    match stmt {
        Stmt::Infer(i) => lower_infer(i, env, rule_id, state, diags).map(|it| vec![it]),
        Stmt::Emit(e) => lower_emit(e, env, state, rule_scope, diags).map(|it| vec![it]),
        Stmt::Action(a) => lower_action(a, env, diags).map(|v| if v.is_empty() { Vec::new() } else { v }),
    }
}

fn lower_infer(
    infer: &InferStmt<'_>,
    _env: &RuleEnv,
    rule_id: &RuleId,
    state: &VerifyState,
    diags: &mut DiagBuffer,
) -> Option<Intent> {
    let Some(target) = infer
        .fields
        .iter()
        .find_map(|f| match f {
            InferField::Target(expr, _) => metric_path_from_expr(expr),
            _ => None,
        }) else {
        err(diags, "ADGL0210", "infer requires an explicit target field", infer.span);
        return None;
    };
    let weight_val = infer
        .fields
        .iter()
        .find_map(|f| f.weight_value().map(|(w, _)| w))
        .unwrap_or(0);
    let Ok(weight_i8) = i8::try_from(weight_val) else {
        err(diags, "ADGL0205", "weight out of i8 range", infer.span);
        return None;
    };
    let Some(weight) = Weight::new(weight_i8) else {
        err(diags, "ADGL0205", "weight out of allowed range [-100,100]", infer.span);
        return None;
    };
    let evidence = infer
        .fields
        .iter()
        .find_map(|f| match f {
            InferField::Evidence(ids, _) => Some(
                ids.iter()
                    .map(|id| Symbol::new(id.name.to_string()))
                    .collect::<Vec<_>>()
                    .into_boxed_slice(),
            ),
            _ => None,
        })
        .unwrap_or_else(|| Box::new([]));
    let mut pii = BTreeSet::new();
    for sym in evidence.iter() {
        let prefix = format!("{}.", sym.as_str());
        for p in &state.pii_by_binding_path {
            if p.starts_with(&prefix) {
                pii.insert(MetricPath::new(p.clone()));
            }
        }
    }
    Some(Intent::InferCause {
        cause: CauseKind::new(infer.cause.name.to_string()),
        target: target.clone(),
        weight,
        evidence,
        provenance_key: ProvKey {
            rule: rule_id.clone(),
            cause: CauseKind::new(infer.cause.name.to_string()),
            target_expr_hash: stable_hash_u64(target.as_str().as_bytes()),
        },
        evidence_pii: pii.into_iter().collect::<Vec<_>>().into_boxed_slice(),
    })
}

fn lower_emit(
    emit: &EmitStmt<'_>,
    _env: &RuleEnv,
    state: &VerifyState,
    _rule_scope: ScopeType,
    _diags: &mut DiagBuffer,
) -> Option<Intent> {
    let target = emit.fields.iter().find_map(|f| match f {
        EmitField::Target(expr, _) => metric_path_from_expr(expr),
        _ => None,
    });
    let severity = emit
        .fields
        .iter()
        .find_map(|f| match f {
            EmitField::Severity(sev, _) => Some(*sev),
            _ => None,
        })
        .or_else(|| resolve_problem(emit.problem.name).and_then(|p| p.severity))
        .unwrap_or(Severity::Medium);
    let evidence = emit
        .fields
        .iter()
        .find_map(|f| match f {
            EmitField::Evidence(ids, _) => Some(
                ids.iter()
                    .map(|id| Symbol::new(id.name.to_string()))
                    .collect::<Vec<_>>()
                    .into_boxed_slice(),
            ),
            _ => None,
        })
        .unwrap_or_else(|| Box::new([]));
    let sarif_id = emit
        .fields
        .iter()
        .find_map(|f| match f {
            EmitField::SarifId(s, _) => Some(airpulse_dsl_types::SarifId::new(s.value.clone())),
            _ => None,
        })
        .or_else(|| resolve_problem(emit.problem.name).map(|p| p.default_sarif_id.clone()))
        .unwrap_or_else(|| airpulse_dsl_types::SarifId::new("unknown"));

    let mut pii = BTreeSet::new();
    for sym in evidence.iter() {
        let prefix = format!("{}.", sym.as_str());
        for p in &state.pii_by_binding_path {
            if p.starts_with(&prefix) {
                pii.insert(MetricPath::new(p.clone()));
            }
        }
    }
    Some(Intent::EmitProblem {
        problem: ProblemKind::new(emit.problem.name.to_string()),
        target,
        severity,
        evidence,
        sarif_id,
        pii: pii.into_iter().collect::<Vec<_>>().into_boxed_slice(),
    })
}

fn lower_action(action: &ActionStmt<'_>, env: &RuleEnv, diags: &mut DiagBuffer) -> Option<Vec<Intent>> {
    let name = action_name(action);
    let known = resolve_action(name)?;
    let reason = action.fields.iter().find_map(|f| match f {
        ActionField::Reason(s, _) => Some(s.value.clone().into_boxed_str()),
        _ => None,
    });
    let evidence = action
        .fields
        .iter()
        .find_map(|f| match f {
            ActionField::Evidence(ids, _) => Some(
                ids.iter()
                    .map(|id| Symbol::new(id.name.to_string()))
                    .collect::<Vec<_>>()
                    .into_boxed_slice(),
            ),
            _ => None,
        })
        .unwrap_or_else(|| Box::new([]));
    if known.kind == ActionKind::SuppressSymptom {
        let Some(arg) = &action.arg else {
            err(diags, "ADGL0209", "suppress_symptom requires binding arg", action.span);
            return None;
        };
        if arg.segments.len() != 1 {
            err(diags, "ADGL0209", "suppress_symptom arg must be one binding", arg.span);
            return None;
        }
        let binding = arg.segments[0].name;
        let Some(BindingTy::Problem(problem)) = env.bindings.get(binding) else {
            err(diags, "ADGL0209", "suppress_symptom arg must be ProblemRef binding", arg.span);
            return None;
        };
        let target = MetricPath::new(format!("{binding}.target"));
        return Some(vec![
            Intent::SupersedeProblem {
                problem: problem.clone(),
                target: target.clone(),
            },
            Intent::EmitAction {
                kind: ActionKind::SuppressSymptom,
                arg: Some(Symbol::new(binding.to_string())),
                target: Some(target),
                reason,
                evidence,
            },
        ]);
    }
    let arg = action.arg.as_ref().map(|k| Symbol::new(kind_ident_name(k)));
    let target = action.fields.iter().find_map(|f| match f {
        ActionField::Target(expr, _) => metric_path_from_expr(expr),
        _ => None,
    });
    Some(vec![Intent::EmitAction {
        kind: known.kind,
        arg,
        target,
        reason,
        evidence,
    }])
}

fn compile_predicate(expr: &Expr<'_>, env: &RuleEnv, diags: &mut DiagBuffer) -> Predicate {
    let mut ops = Vec::new();
    let mut slot = SlotAllocator::default();
    let result = compile_expr(expr, env, diags, &mut ops, &mut slot).unwrap_or(0);
    let result_slot = airpulse_dsl_ir::SlotIdx::new(result)
        .or_else(|| airpulse_dsl_ir::SlotIdx::new(0))
        .unwrap_or(Predicate::always_true().result);
    Predicate { ops: ops.into_boxed_slice(), result: result_slot }
}

#[derive(Default)]
struct SlotAllocator {
    next: u8,
}

impl SlotAllocator {
    fn alloc(&mut self, diags: &mut DiagBuffer, span: Span) -> u8 {
        let idx = self.next;
        self.next = self.next.saturating_add(1);
        if airpulse_dsl_ir::SlotIdx::new(idx).is_none() {
            err(diags, "ADGL0205", "predicate exceeded max slots", span);
            0
        } else {
            idx
        }
    }
}

fn compile_expr(
    expr: &Expr<'_>,
    env: &RuleEnv,
    diags: &mut DiagBuffer,
    ops: &mut Vec<PredOp>,
    slots: &mut SlotAllocator,
) -> Option<u8> {
    match &expr.kind {
        ExprKind::Int(i) => {
            let dst = slots.alloc(diags, expr.span);
            ops.push(PredOp::LoadConst {
                imm: i.value,
                dst: airpulse_dsl_ir::SlotIdx::new(dst)?,
            });
            Some(dst)
        }
        ExprKind::Duration(d) => {
            let dst = slots.alloc(diags, expr.span);
            let dur = DurationMs::from_millis(d.millis).unwrap_or_default();
            ops.push(PredOp::LoadDuration {
                dur,
                dst: airpulse_dsl_ir::SlotIdx::new(dst)?,
            });
            Some(dst)
        }
        ExprKind::String(s) => {
            let dst = slots.alloc(diags, expr.span);
            ops.push(PredOp::LoadConst {
                imm: stable_string_i64(&s.value),
                dst: airpulse_dsl_ir::SlotIdx::new(dst)?,
            });
            Some(dst)
        }
        ExprKind::Bool(b) => {
            let dst = slots.alloc(diags, expr.span);
            ops.push(PredOp::LoadConst {
                imm: if *b { 1 } else { 0 },
                dst: airpulse_dsl_ir::SlotIdx::new(dst)?,
            });
            Some(dst)
        }
        ExprKind::Ident(id) => {
            if id.name == "scope" {
                let dst = slots.alloc(diags, expr.span);
                ops.push(PredOp::LoadScopeKey {
                    dst: airpulse_dsl_ir::SlotIdx::new(dst)?,
                });
                return Some(dst);
            }
            err(diags, "ADGL0501", "bare identifier is not a predicate value", id.span);
            None
        }
        ExprKind::Present(id) => {
            let idx = env.binding_idx.get(id.name).copied().unwrap_or(0);
            let dst = slots.alloc(diags, expr.span);
            ops.push(PredOp::Present {
                binding: airpulse_dsl_ir::BindingIdx(idx),
                dst: airpulse_dsl_ir::SlotIdx::new(dst)?,
            });
            Some(dst)
        }
        ExprKind::Absent(id) => {
            let idx = env.binding_idx.get(id.name).copied().unwrap_or(0);
            let dst = slots.alloc(diags, expr.span);
            ops.push(PredOp::Absent {
                binding: airpulse_dsl_ir::BindingIdx(idx),
                dst: airpulse_dsl_ir::SlotIdx::new(dst)?,
            });
            Some(dst)
        }
        ExprKind::Field { base, field } => {
            let ExprKind::Ident(binding) = &base.kind else {
                err(diags, "ADGL0501", "field access base must be a binding", expr.span);
                return None;
            };
            let Some(binding_ty) = env.bindings.get(binding.name) else {
                err(diags, "ADGL0209", "unknown binding", binding.span);
                return None;
            };
            let (field_idx, op_kind) = match binding_ty {
                BindingTy::Event(event) => resolve_metric_path(
                    EventOrBindingType::Event(event),
                    &format!("{}.{}", binding.name, field.name),
                )
                .map(|(idx, _)| (idx, 0)),
                BindingTy::Cause(cause) => resolve_metric_path(
                    EventOrBindingType::Cause(cause),
                    &format!("{}.{}", binding.name, field.name),
                )
                .map(|(idx, _)| (idx, 1)),
                BindingTy::Problem(problem) => resolve_metric_path(
                    EventOrBindingType::Problem(problem),
                    &format!("{}.{}", binding.name, field.name),
                )
                .map(|(idx, _)| (idx, 2)),
            }?;
            let dst = slots.alloc(diags, expr.span);
            let binding_idx = airpulse_dsl_ir::BindingIdx(*env.binding_idx.get(binding.name).unwrap_or(&0));
            let dst_slot = airpulse_dsl_ir::SlotIdx::new(dst)?;
            match op_kind {
                0 => ops.push(PredOp::LoadEventField {
                    binding: binding_idx,
                    field: field_idx,
                    dst: dst_slot,
                }),
                1 => ops.push(PredOp::LoadCauseField {
                    binding: binding_idx,
                    field: field_idx,
                    dst: dst_slot,
                }),
                _ => ops.push(PredOp::LoadProblemField {
                    binding: binding_idx,
                    field: field_idx,
                    dst: dst_slot,
                }),
            }
            Some(dst)
        }
        ExprKind::Unary { op, expr: rhs } => {
            let src = compile_expr(rhs, env, diags, ops, slots)?;
            let dst = slots.alloc(diags, expr.span);
            match op {
                UnaryOp::Not => ops.push(PredOp::Not {
                    src: airpulse_dsl_ir::SlotIdx::new(src)?,
                    dst: airpulse_dsl_ir::SlotIdx::new(dst)?,
                }),
            }
            Some(dst)
        }
        ExprKind::Binary { op, left, right } => {
            let lhs = compile_expr(left, env, diags, ops, slots)?;
            let rhs = compile_expr(right, env, diags, ops, slots)?;
            let dst = slots.alloc(diags, expr.span);
            let lhs = airpulse_dsl_ir::SlotIdx::new(lhs)?;
            let rhs = airpulse_dsl_ir::SlotIdx::new(rhs)?;
            let dst_slot = airpulse_dsl_ir::SlotIdx::new(dst)?;
            match op {
                BinaryOp::Or => ops.push(PredOp::Or { lhs, rhs, dst: dst_slot }),
                BinaryOp::And => ops.push(PredOp::And { lhs, rhs, dst: dst_slot }),
                BinaryOp::Eq => ops.push(PredOp::CmpEq { lhs, rhs, dst: dst_slot }),
                BinaryOp::Ne => ops.push(PredOp::CmpNe { lhs, rhs, dst: dst_slot }),
                BinaryOp::Lt => ops.push(PredOp::CmpLt { lhs, rhs, dst: dst_slot }),
                BinaryOp::Le => ops.push(PredOp::CmpLe { lhs, rhs, dst: dst_slot }),
                BinaryOp::Gt => ops.push(PredOp::CmpGt { lhs, rhs, dst: dst_slot }),
                BinaryOp::Ge => ops.push(PredOp::CmpGe { lhs, rhs, dst: dst_slot }),
                BinaryOp::In => {
                    ops.push(PredOp::CmpGe { lhs, rhs, dst: dst_slot });
                }
                BinaryOp::Add => ops.push(PredOp::Add { lhs, rhs, dst: dst_slot }),
                BinaryOp::Sub => ops.push(PredOp::Sub { lhs, rhs, dst: dst_slot }),
                BinaryOp::Mul => ops.push(PredOp::Mul { lhs, rhs, dst: dst_slot }),
                BinaryOp::Div => ops.push(PredOp::Div { lhs, rhs, dst: dst_slot }),
                BinaryOp::Rem => ops.push(PredOp::Mod { lhs, rhs, dst: dst_slot }),
            }
            Some(dst)
        }
        ExprKind::Call { .. } => {
            err(diags, "ADGL0501", "calls are not allowed in pure positions", expr.span);
            None
        }
        ExprKind::Index { .. } => {
            err(diags, "ADGL0501", "indexing is not supported in predicates", expr.span);
            None
        }
    }
}

fn infer_expr_type(expr: &Expr<'_>, env: &RuleEnv, _state: &VerifyState, diags: &mut DiagBuffer) -> Option<FieldType> {
    match &expr.kind {
        ExprKind::Int(_) => Some(FieldType::Int),
        ExprKind::Duration(_) => Some(FieldType::Int),
        ExprKind::String(_) => Some(FieldType::String),
        ExprKind::Bool(_) => Some(FieldType::Int),
        ExprKind::Ident(id) => {
            if env.bindings.contains_key(id.name) {
                Some(FieldType::NodeIdList)
            } else {
                err(diags, "ADGL0209", "unknown identifier", id.span);
                None
            }
        }
        ExprKind::Present(id) | ExprKind::Absent(id) => {
            if env.bindings.contains_key(id.name) {
                Some(FieldType::Int)
            } else {
                err(diags, "ADGL0209", "unknown correlate binding", id.span);
                None
            }
        }
        ExprKind::Unary { expr, .. } => infer_expr_type(expr, env, _state, diags),
        ExprKind::Binary { op, left, right } => {
            let lt = infer_expr_type(left, env, _state, diags);
            let rt = infer_expr_type(right, env, _state, diags);
            if let (Some(lt), Some(rt)) = (lt, rt) {
                match op {
                    BinaryOp::Eq | BinaryOp::Ne => {
                        if !types_compatible_for_equality(lt, rt) {
                            err(diags, "ADGL0210", "comparison operand type mismatch", expr.span);
                        }
                    }
                    BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge => {
                        if !types_compatible_for_ordering(lt, rt) {
                            err(diags, "ADGL0210", "ordered comparison requires compatible scalar types", expr.span);
                        }
                    }
                    BinaryOp::In => {
                        if rt != FieldType::ScopeIdList && rt != FieldType::IntList {
                            err(diags, "ADGL0210", "`in` RHS must be a list", right.span);
                        }
                    }
                    BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Rem => {
                        if !is_numeric_type(lt) || !is_numeric_type(rt) {
                            err(diags, "ADGL0210", "arithmetic operands must be numeric", expr.span);
                        }
                    }
                    BinaryOp::And | BinaryOp::Or => {}
                }
            }
            Some(FieldType::Int)
        }
        ExprKind::Field { base, field } => {
            let ExprKind::Ident(id) = &base.kind else {
                err(diags, "ADGL0209", "invalid field access base", base.span);
                return None;
            };
            let Some(binding_ty) = env.bindings.get(id.name) else {
                err(diags, "ADGL0209", "unknown binding", id.span);
                return None;
            };
            let resolved = match binding_ty {
                BindingTy::Event(event) => {
                    resolve_metric_path(EventOrBindingType::Event(event), &format!("{}.{}", id.name, field.name))
                }
                BindingTy::Cause(cause) => {
                    resolve_metric_path(EventOrBindingType::Cause(cause), &format!("{}.{}", id.name, field.name))
                }
                BindingTy::Problem(problem) => {
                    resolve_metric_path(EventOrBindingType::Problem(problem), &format!("{}.{}", id.name, field.name))
                }
            };
            if let Some((_idx, ty)) = resolved {
                Some(ty)
            } else {
                err(diags, "ADGL0209", "unknown metric path field", field.span);
                None
            }
        }
        ExprKind::Call { .. } => {
            err(diags, "ADGL0501", "effectful/topology calls are disallowed in pure expressions", expr.span);
            None
        }
        ExprKind::Index { base, index } => {
            infer_expr_type(base, env, _state, diags);
            infer_expr_type(index, env, _state, diags);
            Some(FieldType::Int)
        }
    }
}

fn validate_action_target_types(action: &ActionStmt<'_>, env: &RuleEnv, diags: &mut DiagBuffer) {
    let Some(schema) = resolve_action(action_name(action)) else {
        return;
    };
    let target_expr = action.fields.iter().find_map(|f| match f {
        ActionField::Target(expr, _) => Some(expr),
        _ => None,
    });
    match target_expr {
        Some(expr) => {
            let Some(target_ty) = infer_expr_type(expr, env, &VerifyState::new(), diags) else {
                return;
            };
            let target_allowed = schema.target_types.iter().any(|allowed| match allowed {
                ActionTargetType::ScopeId => matches!(target_ty, FieldType::ScopeId(_)),
                ActionTargetType::ScopeIdList => target_ty == FieldType::ScopeIdList,
            });
            if !target_allowed {
                err(diags, "ADGL0210", "action target type is incompatible with action contract", expr.span);
            }
        }
        None => {
            if !schema.target_types.is_empty() && schema.kind != ActionKind::SuppressSymptom {
                err(diags, "ADGL0210", "action requires a target field", action.span);
            }
        }
    }
}

fn is_numeric_type(ty: FieldType) -> bool {
    matches!(ty, FieldType::Int | FieldType::Confidence)
}

fn types_compatible_for_ordering(left: FieldType, right: FieldType) -> bool {
    (is_numeric_type(left) && is_numeric_type(right))
        || matches!((left, right), (FieldType::String, FieldType::String) | (FieldType::Severity, FieldType::Severity))
}

fn types_compatible_for_equality(left: FieldType, right: FieldType) -> bool {
    if types_compatible_for_ordering(left, right) {
        return true;
    }
    matches!(
        (left, right),
        (FieldType::ScopeId(_), FieldType::ScopeId(_))
            | (FieldType::ScopeIdList, FieldType::ScopeIdList)
            | (FieldType::NodeIdList, FieldType::NodeIdList)
            | (FieldType::SarifId, FieldType::SarifId)
    )
}

fn collect_stmt_exprs<'a>(stmts: &'a [Stmt<'a>], out: &mut Vec<&'a Expr<'a>>) {
    for stmt in stmts {
        match stmt {
            Stmt::Infer(i) => {
                for f in &i.fields {
                    if let InferField::Target(expr, _) = f {
                        out.push(expr);
                    }
                }
            }
            Stmt::Emit(e) => {
                for f in &e.fields {
                    if let EmitField::Target(expr, _) = f {
                        out.push(expr);
                    }
                }
            }
            Stmt::Action(a) => {
                for f in &a.fields {
                    if let ActionField::Target(expr, _) = f {
                        out.push(expr);
                    }
                }
            }
        }
    }
}

fn gather_rule_statements<'a>(rule: &'a RuleDecl<'a>, out: &mut Vec<&'a Stmt<'a>>) {
    match rule {
        RuleDecl::Evidence(e) => {
            out.extend(e.body.iter());
            if let Some(if_else) = &e.if_else {
                out.extend(if_else.then_body.iter());
                if let Some(else_body) = &if_else.else_body {
                    out.extend(else_body.iter());
                }
            }
        }
        RuleDecl::Decision(d) => {
            out.extend(d.body.iter());
            if let Some(if_else) = &d.if_else {
                out.extend(if_else.then_body.iter());
                if let Some(else_body) = &if_else.else_body {
                    out.extend(else_body.iter());
                }
            }
        }
    }
}

fn infer_target_scope_from_infer(stmt: &InferStmt<'_>, env: &RuleEnv) -> Option<ScopeType> {
    stmt.fields.iter().find_map(|f| match f {
        InferField::Target(expr, _) => infer_target_scope_expr(expr, env),
        _ => None,
    })
}

fn infer_target_scope_from_emit(stmt: &EmitStmt<'_>, env: &RuleEnv) -> Option<ScopeType> {
    stmt.fields.iter().find_map(|f| match f {
        EmitField::Target(expr, _) => infer_target_scope_expr(expr, env),
        _ => None,
    })
}

fn infer_target_scope_expr(expr: &Expr<'_>, env: &RuleEnv) -> Option<ScopeType> {
    match &expr.kind {
        ExprKind::Field { base, field } => {
            let ExprKind::Ident(id) = &base.kind else {
                return None;
            };
            let binding_ty = env.bindings.get(id.name)?;
            match binding_ty {
                BindingTy::Event(event) => resolve_metric_path(
                    EventOrBindingType::Event(event),
                    &format!("{}.{}", id.name, field.name),
                )
                .and_then(|(_, t)| match t {
                    FieldType::ScopeId(scope) => Some(scope),
                    _ => None,
                }),
                BindingTy::Cause(cause) => {
                    if field.name == "target" {
                        resolve_cause(cause.as_str())
                            .and_then(|c| if c.valid_scopes.len() == 1 { Some(c.valid_scopes[0]) } else { None })
                    } else {
                        None
                    }
                }
                BindingTy::Problem(problem) => {
                    if field.name == "target" {
                        resolve_problem(problem.as_str())
                            .and_then(|p| if p.valid_scopes.len() == 1 { Some(p.valid_scopes[0]) } else { None })
                    } else {
                        None
                    }
                }
            }
        }
        _ => None,
    }
}

fn correlate_binding_ty(c: &CorrelateBlock<'_>) -> Option<BindingTy> {
    match &c.source {
        AstCorrelateSource::Event(k) => Some(BindingTy::Event(EventType::new(kind_ident_name(k)))),
        AstCorrelateSource::Cause(k) => Some(BindingTy::Cause(CauseKind::new(k.name.to_string()))),
        AstCorrelateSource::Problem(k) => Some(BindingTy::Problem(ProblemKind::new(k.name.to_string()))),
    }
}

fn contains_present_absent(expr: &Expr<'_>) -> bool {
    match &expr.kind {
        ExprKind::Present(_) | ExprKind::Absent(_) => true,
        ExprKind::Unary { expr, .. } => contains_present_absent(expr),
        ExprKind::Binary { left, right, .. } => contains_present_absent(left) || contains_present_absent(right),
        ExprKind::Field { base, .. } => contains_present_absent(base),
        ExprKind::Call { callee, args } => contains_present_absent(callee) || args.iter().any(contains_present_absent),
        ExprKind::Index { base, index } => contains_present_absent(base) || contains_present_absent(index),
        _ => false,
    }
}

fn is_binding_time_probe(expr: &Expr<'_>, binding: &str) -> bool {
    if let ExprKind::Field { base, field } = &expr.kind
        && let ExprKind::Ident(id) = &base.kind
    {
        return id.name == binding && field.name == "time";
    }
    false
}

fn calculable_window(anchor_binding: &str, start: &Expr<'_>, end: &Expr<'_>) -> Option<(i64, i64)> {
    let back = as_anchor_offset(anchor_binding, start)?;
    let forward = as_anchor_offset(anchor_binding, end)?;
    if back > 0 || forward < 0 {
        return None;
    }
    Some((-back, forward))
}

fn ruleset_max_forward_window_ms(ruleset: &Ruleset<'_>) -> i64 {
    let mut max_forward = 0i64;
    for rule in &ruleset.rules {
        let anchor_binding = match rule {
            RuleDecl::Evidence(e) => e.anchor.binding.name,
            RuleDecl::Decision(d) => match &d.anchor {
                DecisionAnchor::Cause(c) => c.binding.name,
                DecisionAnchor::Problem(p) => p.binding.name,
            },
        };
        let correlates = match rule {
            RuleDecl::Evidence(e) => &e.correlates,
            RuleDecl::Decision(d) => &d.correlates,
        };
        for correlate in correlates {
            if let Some((_, forward)) = calculable_window(anchor_binding, &correlate.time.start, &correlate.time.end) {
                max_forward = max_forward.max(forward);
            }
        }
    }
    max_forward
}

fn as_anchor_offset(anchor_binding: &str, expr: &Expr<'_>) -> Option<i64> {
    match &expr.kind {
        ExprKind::Field { base, field } => {
            if let ExprKind::Ident(id) = &base.kind
                && id.name == anchor_binding
                && field.name == "time"
            {
                return Some(0);
            }
            None
        }
        ExprKind::Binary { op, left, right } => {
            if matches!(op, BinaryOp::Add | BinaryOp::Sub) {
                let lhs = as_anchor_offset(anchor_binding, left)?;
                let rhs = match &right.kind {
                    ExprKind::Duration(d) => d.millis,
                    _ => return None,
                };
                if *op == BinaryOp::Add {
                    Some(lhs + rhs)
                } else {
                    Some(lhs - rhs)
                }
            } else {
                None
            }
        }
        _ => None,
    }
}

fn check_no_calls(expr: &Expr<'_>, diags: &mut DiagBuffer) {
    match &expr.kind {
        ExprKind::Call { .. } => err(diags, "ADGL0501", "calls are not allowed in pure expression positions", expr.span),
        ExprKind::Unary { expr, .. } => check_no_calls(expr, diags),
        ExprKind::Binary { left, right, .. } => {
            check_no_calls(left, diags);
            check_no_calls(right, diags);
        }
        ExprKind::Field { base, .. } => check_no_calls(base, diags),
        ExprKind::Index { base, index } => {
            check_no_calls(base, diags);
            check_no_calls(index, diags);
        }
        _ => {}
    }
}

fn has_cycle(edges: &[Vec<usize>]) -> bool {
    let mut state = vec![0u8; edges.len()];
    for i in 0..edges.len() {
        if state[i] == 0 && dfs_cycle(i, edges, &mut state) {
            return true;
        }
    }
    false
}

fn dfs_cycle(node: usize, edges: &[Vec<usize>], state: &mut [u8]) -> bool {
    state[node] = 1;
    for &next in &edges[node] {
        if state[next] == 1 {
            return true;
        }
        if state[next] == 0 && dfs_cycle(next, edges, state) {
            return true;
        }
    }
    state[node] = 2;
    false
}

fn max_expr_depth_rule(rule: &RuleDecl<'_>) -> usize {
    let mut max_depth = 0usize;
    let mut visit = |expr: &Expr<'_>| {
        max_depth = max_depth.max(expr_depth(expr));
    };
    match rule {
        RuleDecl::Evidence(e) => {
            if let Some(pred) = &e.anchor.predicate {
                visit(pred);
            }
            for c in &e.correlates {
                visit(&c.time.probe);
                visit(&c.time.start);
                visit(&c.time.end);
            }
            for stmt in &e.body {
                visit_stmt_exprs(stmt, &mut visit);
            }
            if let Some(if_else) = &e.if_else {
                visit(&if_else.condition);
                for stmt in &if_else.then_body {
                    visit_stmt_exprs(stmt, &mut visit);
                }
                if let Some(else_body) = &if_else.else_body {
                    for stmt in else_body {
                        visit_stmt_exprs(stmt, &mut visit);
                    }
                }
            }
        }
        RuleDecl::Decision(d) => {
            match &d.anchor {
                DecisionAnchor::Cause(c) => visit(&c.predicate),
                DecisionAnchor::Problem(p) => {
                    if let Some(pred) = &p.predicate {
                        visit(pred);
                    }
                }
            }
            for c in &d.correlates {
                visit(&c.time.probe);
                visit(&c.time.start);
                visit(&c.time.end);
            }
            for stmt in &d.body {
                visit_stmt_exprs(stmt, &mut visit);
            }
            if let Some(if_else) = &d.if_else {
                visit(&if_else.condition);
                for stmt in &if_else.then_body {
                    visit_stmt_exprs(stmt, &mut visit);
                }
                if let Some(else_body) = &if_else.else_body {
                    for stmt in else_body {
                        visit_stmt_exprs(stmt, &mut visit);
                    }
                }
            }
        }
    }
    max_depth
}

fn visit_stmt_exprs(stmt: &Stmt<'_>, f: &mut impl FnMut(&Expr<'_>)) {
    match stmt {
        Stmt::Infer(i) => {
            for field in &i.fields {
                if let InferField::Target(expr, _) = field {
                    f(expr);
                }
            }
        }
        Stmt::Emit(e) => {
            for field in &e.fields {
                if let EmitField::Target(expr, _) = field {
                    f(expr);
                }
            }
        }
        Stmt::Action(a) => {
            for field in &a.fields {
                if let ActionField::Target(expr, _) = field {
                    f(expr);
                }
            }
        }
    }
}

fn expr_depth(expr: &Expr<'_>) -> usize {
    match &expr.kind {
        ExprKind::Unary { expr, .. } => 1 + expr_depth(expr),
        ExprKind::Binary { left, right, .. } => 1 + expr_depth(left).max(expr_depth(right)),
        ExprKind::Field { base, .. } => 1 + expr_depth(base),
        ExprKind::Call { callee, args } => {
            1 + args.iter().fold(expr_depth(callee), |acc, e| acc.max(expr_depth(e)))
        }
        ExprKind::Index { base, index } => 1 + expr_depth(base).max(expr_depth(index)),
        _ => 1,
    }
}

fn metric_path_from_expr(expr: &Expr<'_>) -> Option<MetricPath> {
    match &expr.kind {
        ExprKind::Field { base, field } => {
            if let ExprKind::Ident(id) = &base.kind {
                Some(MetricPath::new(format!("{}.{}", id.name, field.name)))
            } else {
                None
            }
        }
        ExprKind::Ident(id) => Some(MetricPath::new(id.name.to_string())),
        _ => None,
    }
}

fn action_name<'a>(action: &'a ActionStmt<'a>) -> &'a str {
    match &action.action {
        ActionName::Known(kind) => kind.as_str(),
        ActionName::Custom(id) => id.name,
    }
}

// ===== Helpers =====
fn parse_version(v: &str) -> u32 {
    let mut parts = v.split('.').filter_map(|s| s.parse::<u8>().ok());
    let major = parts.next().unwrap_or(1);
    let minor = parts.next().unwrap_or(0);
    let patch = parts.next().unwrap_or(0);
    ProgramImage::pack_version(major, minor, patch)
}

fn kind_ident_name(kind: &KindIdent<'_>) -> String {
    kind.segments.iter().map(|s| s.name).collect::<Vec<_>>().join(".")
}

fn err(diags: &mut DiagBuffer, code: &'static str, message: impl Into<String>, span: Span) {
    diags.push(Diagnostic::error(code, message, span));
}

fn warn(diags: &mut DiagBuffer, code: &'static str, message: impl Into<String>, span: Span) {
    diags.push(Diagnostic::warning(code, message, span));
}
