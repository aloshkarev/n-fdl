#![forbid(unsafe_code)]
#![warn(clippy::all)]

pub mod ast;
pub mod lexer;
pub mod parser;

pub use ast::*;
pub use lexer::{Lexer, Token};
pub use ndsl_diag::{DiagBuffer, Diagnostic, Severity, Span};
pub use parser::{ParseError, Parser}; // keep old Spec during migration
