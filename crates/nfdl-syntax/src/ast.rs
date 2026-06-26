//! Real AST for production v1 (start of proper typed representation)
//! Focused on fields, types, simple expressions, validate for datagram protocols.
//! Now includes full state_machine support with states, transitions, guards and actions.

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
}

#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub name: String,
    pub ty: NfdlType,
    pub validate: Option<Validate>,
    pub conditional: Option<Expr>, // if cond
}

#[derive(Debug, Clone, PartialEq)]
pub struct Let {
    pub name: String,
    pub value: Expr,
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
}

#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    pub name: String,
    pub fields: Vec<Field>,
    pub lets: Vec<Let>,
    pub loops: Vec<Loop>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Bind {
    pub outer: String,
    pub inner: String,
    pub when: Expr,
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
    pub endian: String, // "big" | "little"
    pub mode: String,   // "datagram" | "stream"
    pub messages: Vec<Message>,
    pub binds: Vec<Bind>,
    pub state_machines: Vec<StateMachine>,
}

impl Default for Protocol {
    fn default() -> Self {
        Self {
            name: String::new(),
            endian: "big".to_string(),
            mode: "datagram".to_string(),
            messages: vec![],
            binds: vec![],
            state_machines: vec![],
        }
    }
}
