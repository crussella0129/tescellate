//! Runtime surface that transpiled Carbide code links against.
//!
//! Generated code emits `use tescellate_formula::transpile::rt::*;` and
//! references only the names below — `CellValue`, `EvalError`, the operator
//! enums, and the shared `apply_*_op` primitives. Keeping this surface small
//! and explicit is what lets a generated crate stay trivial.

pub use crate::excellite::ast::{BinaryOp, UnaryOp};
pub use crate::excellite::eval::{apply_binary_op, apply_unary_op};
pub use crate::EvalError;
pub use tescellate_core::CellValue;
