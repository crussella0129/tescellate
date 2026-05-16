//! Runtime surface that transpiled Carbide code links against.
//!
//! Generated code emits `use tescellate_formula::transpile::rt::*;` and
//! references only the names below — the value type, the error type, the
//! `EvalCtx` trait it reads cells through, the operator enums, and the
//! shared `apply_*_op` / `bare_range` primitives. Keeping this surface
//! small and explicit is what lets a generated crate stay trivial.

pub use crate::excellite::ast::{BinaryOp, UnaryOp};
pub use crate::excellite::eval::{apply_binary_op, apply_unary_op, bare_range};
pub use crate::{EvalCtx, EvalError};
pub use tescellate_core::{Array, CellValue};
