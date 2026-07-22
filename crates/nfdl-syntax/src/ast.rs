//! Real AST for production v1 (start of proper typed representation)
//! Focused on fields, types, simple expressions, validate for datagram protocols.
//! Now includes full state_machine support with states, transitions, guards and actions.

use ndsl_trivia::Span;

#[derive(Debug, Clone, PartialEq)]
pub enum NfdlType {
    U8,
    U16,
    U24,
    U32,
    Bytes { len: Expr },
    BytesRest,
    BytesEof,
    BytesStream, // bytes[stream] per spec for reassembly
    Bitfield { bits: u8 },
    MessageRef(String), // reference to another message, e.g. for loops
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Ident(String),
    Int(i64),
    /// String literal (e.g. plugin name in `invoke("dns_decompress", ...)`).
    Str(String),
    Binary {
        op: BinOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Unary {
        op: UnaryOp,
        expr: Box<Expr>,
    },
    Ternary {
        cond: Box<Expr>,
        then_branch: Box<Expr>,
        else_branch: Box<Expr>,
    },
    Coalesce {
        value: Box<Expr>,
        default: Box<Expr>,
    },
    Call {
        name: String,
        args: Vec<Expr>,
    },
    Tuple(Vec<Expr>),
    Field(Box<Expr>, String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum BinOp {
    // Logical
    Or,
    And,
    // Bitwise
    BitOr,
    BitXor,
    BitAnd,
    // Relational
    Eq,
    Ne, // !=
    Gt,
    Lt,
    Ge,
    Le,
    // Shift
    Shl,
    Shr,
    // Arithmetic
    Add,
    Sub,
    Mul,
    Div,
    Mod,
}
#[derive(Debug, Clone, PartialEq)]
pub enum UnaryOp {
    Not,    // !
    BitNot, // ~
    Neg,    // -
}

#[derive(Debug, Clone, PartialEq)]
pub struct Validate {
    pub expr: Expr,
    pub message: String,
    /// Source order within the enclosing body (for interleaved emission).
    pub order: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub name: String,
    pub ty: NfdlType,
    pub validate: Option<Validate>,
    pub conditional: Option<Expr>, // if cond
    /// Source order within the enclosing body (for interleaved emission).
    pub order: u32,
    /// Byte span covering the field production in the source text.
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Let {
    pub name: String,
    pub value: Expr,
    /// Source order within the enclosing body (for interleaved emission).
    pub order: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Carry {
    pub name: String,
    pub ty: NfdlType,
    pub init: Expr,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NextStmt {
    pub name: String,
    pub value: Expr,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Loop {
    pub name: String,
    pub carries: Vec<Carry>,
    pub condition: Expr,
    pub body: Vec<Field>,
    pub nexts: Vec<NextStmt>, // next statements collected from body
    /// Source order within the enclosing body (for interleaved emission).
    pub order: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    pub name: String,
    /// Outer doc-comment (`///`) immediately preceding this message, if any.
    pub doc: Option<String>,
    pub fields: Vec<Field>,
    pub lets: Vec<Let>,
    pub loops: Vec<Loop>,
    pub validates: Vec<Validate>,
    pub matches: Vec<Match>,
}

/// A `match <tag> { case N => { ... } default => { ... } }` tagged union.
/// Each arm carries a mini message-body (fields/lets/loops/validates). The
/// `case` value is `None` for the `default` arm.
#[derive(Debug, Clone, PartialEq)]
pub struct Match {
    pub tag: Expr,
    pub arms: Vec<MatchArm>,
    /// Source order within the enclosing body (for interleaved emission).
    pub order: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatchArm {
    pub case: Option<i64>,
    pub fields: Vec<Field>,
    pub lets: Vec<Let>,
    pub loops: Vec<Loop>,
    pub validates: Vec<Validate>,
    pub matches: Vec<Match>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Bind {
    pub layer: String,  // target message to parse from the payload bytes (e.g. "IPv4")
    pub source: String, // source message that owns the payload field (e.g. "TunnelMessage")
    pub field: String,  // payload field name in the source (e.g. "payload")
    pub when: Expr,     // dispatch condition evaluated against source fields
}

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Set { var: String, value: Expr },
    Emit { event: String },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Transition {
    pub from_state: Option<String>,
    pub msg_type: String,
    pub guard: Option<Expr>,
    pub to_state: String,
    pub actions: Vec<Action>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct State {
    pub name: String,
    pub transitions: Vec<Transition>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StateMachine {
    pub name: String,
    pub states: Vec<State>,
    pub initial: String,
    pub key: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Protocol {
    pub name: String,
    /// Outer doc-comment (`///`) immediately preceding this protocol, if any.
    pub doc: Option<String>,
    pub endian: String, // "big" | "little"
    pub mode: String,   // "datagram" | "stream"
    pub eof: String,    // EOF source for bytes[EOF]: "" | "on_fin" | "on_close" | "by_plugin(...)"
    pub messages: Vec<Message>,
    pub binds: Vec<Bind>,
    pub state_machines: Vec<StateMachine>,
}

impl Default for Protocol {
    fn default() -> Self {
        Self {
            name: String::new(),
            doc: None,
            endian: "big".to_string(),
            mode: "datagram".to_string(),
            eof: String::new(),
            messages: vec![],
            binds: vec![],
            state_machines: vec![],
        }
    }
}
