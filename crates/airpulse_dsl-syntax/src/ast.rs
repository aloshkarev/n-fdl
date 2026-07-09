//! AST nodes for ADGL (`docs/idea/spec/02-grammar.ebnf`).

use airpulse_dsl_types::{ActionKind, ScopeType, Severity};
use ndsl_diag::Span;

/// `Ident` token with source span.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ident<'a> {
    /// Identifier text.
    pub name: &'a str,
    /// Byte span in source.
    pub span: Span,
}

/// Dotted identifier (`KindIdent ::= Ident { "." Ident }`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KindIdent<'a> {
    /// Dotted path segments.
    pub segments: Vec<Ident<'a>>,
    /// Byte span in source.
    pub span: Span,
}

/// `StringLit` with unescaped text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StringLit {
    /// Unescaped string contents.
    pub value: String,
    /// Byte span in source.
    pub span: Span,
}

/// Integer literal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IntLit {
    /// Parsed `i64` value.
    pub value: i64,
    /// Byte span in source.
    pub span: Span,
}

/// Duration literal (`500ms`, `1s`, `2min`) normalized to milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DurationLit {
    /// Literal value in milliseconds.
    pub millis: i64,
    /// Byte span in source.
    pub span: Span,
}

/// `Ruleset ::= "ruleset" StringLit "{" RulesetHeader { Rule } "}"`
#[derive(Debug, Clone, PartialEq)]
pub struct Ruleset<'a> {
    /// Ruleset name.
    pub name: StringLit,
    /// Header fields.
    pub header: RulesetHeader<'a>,
    /// Rules in declaration order.
    pub rules: Vec<RuleDecl<'a>>,
    /// Byte span in source.
    pub span: Span,
}

/// `RulesetHeader ::= Version { Decl }`.
#[derive(Debug, Clone, PartialEq)]
pub struct RulesetHeader<'a> {
    /// `version = "x.y"`.
    pub version: StringLit,
    /// Zero or more declarations.
    pub decls: Vec<Decl<'a>>,
    /// Byte span in source.
    pub span: Span,
}

/// Header declarations.
#[derive(Debug, Clone, PartialEq)]
pub enum Decl<'a> {
    /// `requires = [StringLit, ...]`.
    Requires(RequiresDecl),
    /// `mutually_exclusive(Ident, ...)`.
    MutuallyExclusive(MutuallyExclusiveDecl<'a>),
}

/// `requires = [StringLit { "," StringLit }]`.
#[derive(Debug, Clone, PartialEq)]
pub struct RequiresDecl {
    /// Required capability strings.
    pub capabilities: Vec<StringLit>,
    /// Byte span in source.
    pub span: Span,
}

/// `mutually_exclusive(IdentList)`.
#[derive(Debug, Clone, PartialEq)]
pub struct MutuallyExclusiveDecl<'a> {
    /// Cause identifiers in one exclusivity group.
    pub idents: Vec<Ident<'a>>,
    /// Byte span in source.
    pub span: Span,
}

/// `Rule ::= EvidenceRule | DecisionRule`.
#[derive(Debug, Clone, PartialEq)]
pub enum RuleDecl<'a> {
    /// Evidence rule.
    Evidence(EvidenceRule<'a>),
    /// Decision rule.
    Decision(DecisionRule<'a>),
}

/// `EvidenceRule` production.
#[derive(Debug, Clone, PartialEq)]
pub struct EvidenceRule<'a> {
    /// Rule name.
    pub name: Ident<'a>,
    /// Scope.
    pub scope: ScopeType,
    /// Anchor declaration.
    pub anchor: AnchorBlock<'a>,
    /// Correlate declarations.
    pub correlates: Vec<CorrelateBlock<'a>>,
    /// Optional if/else block.
    pub if_else: Option<IfElseBlock<'a>>,
    /// Rule body statements (`infer` or `action`).
    pub body: Vec<Stmt<'a>>,
    /// Byte span in source.
    pub span: Span,
}

/// `DecisionRule` production.
#[derive(Debug, Clone, PartialEq)]
pub struct DecisionRule<'a> {
    /// Rule name.
    pub name: Ident<'a>,
    /// Scope.
    pub scope: ScopeType,
    /// Decision anchor.
    pub anchor: DecisionAnchor<'a>,
    /// Correlate declarations.
    pub correlates: Vec<CorrelateBlock<'a>>,
    /// Optional if/else block.
    pub if_else: Option<IfElseBlock<'a>>,
    /// Rule body statements (`emit` or `action`).
    pub body: Vec<Stmt<'a>>,
    /// Byte span in source.
    pub span: Span,
}

/// `AnchorBlock ::= "anchor" Ident ":" "event" "(" EventType ")" [ "{" [ Predicate ] "}" ]`.
#[derive(Debug, Clone, PartialEq)]
pub struct AnchorBlock<'a> {
    /// Binding name.
    pub binding: Ident<'a>,
    /// Event type identifier.
    pub event_type: KindIdent<'a>,
    /// Optional predicate expression.
    pub predicate: Option<Expr<'a>>,
    /// Byte span in source.
    pub span: Span,
}

/// Decision anchor variants.
#[derive(Debug, Clone, PartialEq)]
pub enum DecisionAnchor<'a> {
    /// `Cause(Ident) { Predicate }`.
    Cause(CauseAnchor<'a>),
    /// `Problem(Ident) [ { Predicate } ]`.
    Problem(ProblemAnchor<'a>),
}

/// `CauseAnchor` production.
#[derive(Debug, Clone, PartialEq)]
pub struct CauseAnchor<'a> {
    /// Binding name.
    pub binding: Ident<'a>,
    /// Cause kind name.
    pub cause: Ident<'a>,
    /// Required predicate.
    pub predicate: Expr<'a>,
    /// Byte span in source.
    pub span: Span,
}

/// `ProblemAnchor` production.
#[derive(Debug, Clone, PartialEq)]
pub struct ProblemAnchor<'a> {
    /// Binding name.
    pub binding: Ident<'a>,
    /// Problem kind name.
    pub problem: Ident<'a>,
    /// Optional predicate.
    pub predicate: Option<Expr<'a>>,
    /// Byte span in source.
    pub span: Span,
}

/// `CorrelateBlock` production.
#[derive(Debug, Clone, PartialEq)]
pub struct CorrelateBlock<'a> {
    /// Binding name.
    pub binding: Ident<'a>,
    /// Correlate source.
    pub source: CorrelateSource<'a>,
    /// Topology predicate call.
    pub topo: TopoPredicate<'a>,
    /// Time window expression.
    pub time: TimeWindow<'a>,
    /// Byte span in source.
    pub span: Span,
}

/// `CorrelateSource` variants.
#[derive(Debug, Clone, PartialEq)]
pub enum CorrelateSource<'a> {
    /// `event(EventType)`.
    Event(KindIdent<'a>),
    /// `Problem(Ident)`.
    Problem(Ident<'a>),
    /// `Cause(Ident)`.
    Cause(Ident<'a>),
}

/// `TopoPredicate ::= Ident "(" ExprList ")"`.
#[derive(Debug, Clone, PartialEq)]
pub struct TopoPredicate<'a> {
    /// Topology function name.
    pub name: Ident<'a>,
    /// Arguments.
    pub args: Vec<Expr<'a>>,
    /// Byte span in source.
    pub span: Span,
}

/// `TimeWindow ::= Expr "in" "[" Expr "," Expr "]"`.
#[derive(Debug, Clone, PartialEq)]
pub struct TimeWindow<'a> {
    /// Probe expression (left side of `in`).
    pub probe: Expr<'a>,
    /// Inclusive lower bound.
    pub start: Expr<'a>,
    /// Inclusive upper bound.
    pub end: Expr<'a>,
    /// Byte span in source.
    pub span: Span,
}

/// Statements that may appear in rule bodies.
#[derive(Debug, Clone, PartialEq)]
pub enum Stmt<'a> {
    /// `infer Cause(...) { ... }`.
    Infer(InferStmt<'a>),
    /// `emit Problem(...) { ... }`.
    Emit(EmitStmt<'a>),
    /// `action ...`.
    Action(ActionStmt<'a>),
}

/// `infer` statement.
#[derive(Debug, Clone, PartialEq)]
pub struct InferStmt<'a> {
    /// Cause kind.
    pub cause: Ident<'a>,
    /// Fields in declaration order.
    pub fields: Vec<InferField<'a>>,
    /// Byte span in source.
    pub span: Span,
}

/// `InferField` variants.
#[derive(Debug, Clone, PartialEq)]
pub enum InferField<'a> {
    /// `target: Expr`.
    Target(Expr<'a>, Span),
    /// `weight: (+|-) IntLit`.
    Weight {
        /// Signed integer magnitude.
        value: i64,
        /// Byte span.
        span: Span,
    },
    /// `evidence: [RefList]`.
    Evidence(Vec<Ident<'a>>, Span),
}

impl<'a> InferField<'a> {
    /// Returns `(value, span)` for `weight: (+|-) IntLit`.
    #[must_use]
    pub const fn weight_value(&self) -> Option<(i64, Span)> {
        match self {
            InferField::Weight { value, span } => Some((*value, *span)),
            _ => None,
        }
    }
}

/// `emit` statement.
#[derive(Debug, Clone, PartialEq)]
pub struct EmitStmt<'a> {
    /// Problem kind.
    pub problem: Ident<'a>,
    /// Fields in declaration order.
    pub fields: Vec<EmitField<'a>>,
    /// Byte span in source.
    pub span: Span,
}

/// `EmitField` variants.
#[derive(Debug, Clone, PartialEq)]
pub enum EmitField<'a> {
    /// `target: Expr`.
    Target(Expr<'a>, Span),
    /// `severity: Severity`.
    Severity(Severity, Span),
    /// `evidence: [RefList]`.
    Evidence(Vec<Ident<'a>>, Span),
    /// `sarif_id: StringLit`.
    SarifId(StringLit, Span),
}

/// Action function identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionName<'a> {
    /// Known v1 action kind.
    Known(ActionKind),
    /// Unknown/custom identifier (parser is name-agnostic).
    Custom(Ident<'a>),
}

/// `action` statement.
#[derive(Debug, Clone, PartialEq)]
pub struct ActionStmt<'a> {
    /// Action kind identifier.
    pub action: ActionName<'a>,
    /// Optional argument in `action x(arg)`.
    pub arg: Option<KindIdent<'a>>,
    /// Fields in declaration order.
    pub fields: Vec<ActionField<'a>>,
    /// Byte span in source.
    pub span: Span,
}

/// `ActionField` variants.
#[derive(Debug, Clone, PartialEq)]
pub enum ActionField<'a> {
    /// `target: Expr`.
    Target(Expr<'a>, Span),
    /// `reason: StringLit`.
    Reason(StringLit, Span),
    /// `evidence: [RefList]`.
    Evidence(Vec<Ident<'a>>, Span),
}

/// `if ... { ... } [ else { ... } ]`.
#[derive(Debug, Clone, PartialEq)]
pub struct IfElseBlock<'a> {
    /// Condition expression.
    pub condition: Expr<'a>,
    /// Then-branch statements.
    pub then_body: Vec<Stmt<'a>>,
    /// Optional else statements.
    pub else_body: Option<Vec<Stmt<'a>>>,
    /// Byte span in source.
    pub span: Span,
}

/// Expression with span.
#[derive(Debug, Clone, PartialEq)]
pub struct Expr<'a> {
    /// Expression kind.
    pub kind: ExprKind<'a>,
    /// Byte span in source.
    pub span: Span,
}

/// Expression grammar (`Expr` / `LogicOr` ... `Primary`).
#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind<'a> {
    /// Integer literal.
    Int(IntLit),
    /// Duration literal.
    Duration(DurationLit),
    /// String literal.
    String(StringLit),
    /// Identifier.
    Ident(Ident<'a>),
    /// Boolean literal.
    Bool(bool),
    /// `present(ident)`.
    Present(Ident<'a>),
    /// `absent(ident)`.
    Absent(Ident<'a>),
    /// Prefix unary operator.
    Unary {
        /// Operator.
        op: UnaryOp,
        /// Operand.
        expr: Box<Expr<'a>>,
    },
    /// Binary operator.
    Binary {
        /// Operator.
        op: BinaryOp,
        /// Left operand.
        left: Box<Expr<'a>>,
        /// Right operand.
        right: Box<Expr<'a>>,
    },
    /// Field access (`a.b`).
    Field {
        /// Base expression.
        base: Box<Expr<'a>>,
        /// Field name.
        field: Ident<'a>,
    },
    /// Function call postfix (`f(a, b)`).
    Call {
        /// Callee expression.
        callee: Box<Expr<'a>>,
        /// Call arguments.
        args: Vec<Expr<'a>>,
    },
    /// Index postfix (`a[b]`).
    Index {
        /// Base expression.
        base: Box<Expr<'a>>,
        /// Index expression.
        index: Box<Expr<'a>>,
    },
}

/// Unary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    /// `!` or `not`.
    Not,
}

/// Binary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    /// `||` or `or`.
    Or,
    /// `&&` or `and`.
    And,
    /// `==`.
    Eq,
    /// `!=`.
    Ne,
    /// `<`.
    Lt,
    /// `<=`.
    Le,
    /// `>`.
    Gt,
    /// `>=`.
    Ge,
    /// `in`.
    In,
    /// `+`.
    Add,
    /// `-`.
    Sub,
    /// `*`.
    Mul,
    /// `/`.
    Div,
    /// `%`.
    Rem,
}
