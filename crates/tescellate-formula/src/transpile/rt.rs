//! Runtime surface that transpiled Carbide code links against.
//!
//! Generated code emits `use tescellate_formula::transpile::rt::*;` and
//! references only the names below — the value/error types, the `EvalCtx`
//! trait it reads cells through, the operator enums, the shared
//! `apply_*_op` / `bare_range` primitives, and — for function calls — the
//! `Expr` AST type and the `standard()` function registry. Keeping this
//! surface small and explicit is what lets a generated crate stay trivial.

pub use crate::excellite::ast::{BinaryOp, Expr, UnaryOp};
pub use crate::excellite::eval::{apply_binary_op, apply_unary_op, bare_range};
pub use crate::excellite::funcs::standard;
pub use crate::{EvalCtx, EvalError};
pub use tescellate_core::{Array, CellValue};
