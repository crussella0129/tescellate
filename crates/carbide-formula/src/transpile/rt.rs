//! Runtime surface that transpiled Carbide code links against.
//!
//! Generated code emits `use tescellate_formula::transpile::rt::*;` and
//! references only the names below — the value/error types, the `EvalCtx`
//! trait it reads cells and variables through, the operator enums, the
//! shared `apply_*` / `bare_range` / `number_binary_op` / `to_number`
//! primitives, and — for function calls — the `Expr` AST type and the
//! `standard()` function registry. Keeping this surface small and explicit
//! is what lets a generated crate stay trivial.

pub use crate::excellite::ast::{BinaryOp, Expr, UnaryOp};
pub use crate::excellite::eval::{
    apply_binary_op, apply_lambda, apply_unary_op, bare_range, number_binary_op,
};
pub use crate::excellite::funcs::coerce::to_number;
pub use crate::excellite::funcs::standard;
pub use crate::{EvalCtx, EvalError};
pub use tescellate_core::{Array, CellValue};
