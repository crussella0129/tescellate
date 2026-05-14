//! Formula engines for Tescellate. See PLAN.md §6.

use tescellate_core::{CellValue, EngineKind};
use thiserror::Error;

pub mod engine;
pub mod excellite;

pub use engine::{CellSnapshot, WorkbookEngine};

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("parse error: {0}")]
    Message(String),
}

#[derive(Debug, Error, Clone, PartialEq)]
pub enum EvalError {
    #[error("#REF! {0}")]
    Ref(String),
    #[error("#DIV/0!")]
    DivZero,
    #[error("#NUM!")]
    Num,
    #[error("#VALUE! {0}")]
    Value(String),
    #[error("unknown function: {0}")]
    UnknownFn(String),
    #[error("bad arity for {name}: got {got}, want {want}")]
    BadArity {
        name: String,
        want: String,
        got: usize,
    },
}

/// Opaque per-engine compiled artifact. Each engine owns its own variant;
/// engines gated behind cargo features add variants under `cfg`.
#[derive(Debug, Clone)]
pub enum CompiledFormula {
    ExcelLite(excellite::Expr),
}

/// What every formula engine needs to read from the surrounding workbook.
/// Additional engine-specific shapes (numpy arrays for Python, etc.) go
/// on top of this trait via downcasting in their own modules.
pub trait EvalCtx {
    fn cell(&self, addr: &str) -> Result<CellValue, EvalError>;
    fn range(&self, start: &str, end: &str) -> Result<Vec<CellValue>, EvalError>;
}

pub trait FormulaEngine: Send + Sync {
    fn kind(&self) -> EngineKind;
    fn parse(&self, src: &str) -> Result<CompiledFormula, ParseError>;
    /// Cell and range references the formula reads. The orchestrator uses
    /// this to update the DAG before evaluating.
    fn refs(&self, compiled: &CompiledFormula) -> Vec<(String, Option<String>)>;
    fn eval(&self, compiled: &CompiledFormula, ctx: &dyn EvalCtx) -> Result<CellValue, EvalError>;
}
