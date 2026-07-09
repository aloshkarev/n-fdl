#![deny(unsafe_code)]
#![warn(missing_docs)]

//! ADGL syntax crate (`airpulse_dsl_syntax`): AST + parser for
//! `docs/idea/spec/02-grammar.ebnf`.
//!
//! Implementation note: lexing uses a small hybrid approach (manual scanner with
//! targeted winnow helpers), and parsing uses a token-stream recursive descent.

pub mod ast;
mod parser;

pub use ast::*;
pub use parser::{line_col, parse_expression, parse_ruleset};
