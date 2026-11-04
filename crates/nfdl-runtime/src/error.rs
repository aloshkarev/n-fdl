use nfdl_bytecode::BytecodeError;
use nfdl_syntax::ParseError;
use nfdl_vm::VmError;

#[derive(Debug, Clone)]
pub enum RuntimeError {
    Parse(String),
    Constraint(String),
    Malformed(String),
    NeedMoreBytes,
    LimitExceeded(String),
    WithLocation {
        msg: String,
        pos: usize,
    },
    /// A cap was hit. Not a cause and not an absent decode.
    Partial {
        cap: &'static str,
        kept: usize,
        dropped: usize,
    },
}

impl From<ParseError> for RuntimeError {
    fn from(e: ParseError) -> Self {
        RuntimeError::Parse(format!("{:?}", e))
    }
}

impl From<BytecodeError> for RuntimeError {
    fn from(e: BytecodeError) -> Self {
        match e {
            BytecodeError::Constraint(s) => RuntimeError::Constraint(s),
            BytecodeError::LimitExceeded(s) => RuntimeError::LimitExceeded(s),
        }
    }
}

impl From<VmError> for RuntimeError {
    fn from(e: VmError) -> Self {
        match e {
            VmError::LimitExceeded(s) => RuntimeError::LimitExceeded(s),
        }
    }
}

impl From<RuntimeError> for String {
    fn from(e: RuntimeError) -> Self {
        format!("{:?}", e)
    }
}

impl std::fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RuntimeError::Parse(s) => write!(f, "parse error: {}", s),
            RuntimeError::Constraint(s) => write!(f, "constraint: {}", s),
            RuntimeError::Malformed(s) => write!(f, "malformed: {}", s),
            RuntimeError::NeedMoreBytes => write!(f, "need more bytes"),
            RuntimeError::LimitExceeded(s) => write!(f, "limit exceeded: {}", s),
            RuntimeError::WithLocation { msg, pos } => write!(f, "error at {}: {}", pos, msg),
            RuntimeError::Partial { cap, kept, dropped } => {
                write!(
                    f,
                    "coverage=partial cap={cap} kept={kept} dropped={dropped}"
                )
            }
        }
    }
}

impl std::error::Error for RuntimeError {}
