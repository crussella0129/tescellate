//! Formula engines for Tescellate. See PLAN.md §6.
//!
//! Phase 0 ships only the `FormulaEngine` trait and a placeholder
//! `ExcelLite` implementation. The real Excel-lite parser, Python (PyO3),
//! Rhai, and rustc-native engines land in their respective phases behind
//! Cargo features (`python`, `rhai`, `rustnative`).

use tescellate_core::{CellValue, EngineKind};
use thiserror::Error;

pub mod excellite;

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("parse error: {0}")]
    Message(String),
}

#[derive(Debug, Error)]
pub enum EvalError {
    #[error("eval error: {0}")]
    Message(String),
}

/// Opaque per-engine compiled artifact. Each engine owns its own variant.
/// Engines gated behind Cargo features add their variants under `cfg`.
#[derive(Debug, Clone)]
pub enum CompiledFormula {
    ExcelLite(excellite::Expr),
}

/// Context exposed to formulas at evaluation time. Phase 1 expands this
/// to expose cell reads, range reads, and lattice introspection.
pub struct EvalCtx<'a> {
    pub _marker: std::marker::PhantomData<&'a ()>,
}

pub trait FormulaEngine: Send + Sync {
    fn kind(&self) -> EngineKind;
    fn parse(&self, src: &str) -> Result<CompiledFormula, ParseError>;
    fn eval(&self, compiled: &CompiledFormula, ctx: &EvalCtx<'_>) -> Result<CellValue, EvalError>;
}
