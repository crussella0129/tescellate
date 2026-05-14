//! Formula engines for Tescellate. See PLAN.md §6.

use std::sync::Arc;
use tescellate_core::{CellValue, EngineKind, Env};
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
///
/// `var` and `env` are default-None so existing impls (e.g. `SheetEvalView`
/// in `engine.rs`) keep compiling. `ScopedCtx` is the wrapper used by
/// `LAMBDA`, `LET`, and `LETREC` to introduce a fresh lexical scope.
pub trait EvalCtx {
    fn cell(&self, addr: &str) -> Result<CellValue, EvalError>;
    fn range(&self, start: &str, end: &str) -> Result<Vec<CellValue>, EvalError>;
    /// Resolve a bare identifier as a lexical variable. Default `None`
    /// means "no lexical scope here", which is correct for the bare
    /// `SheetEvalView` (sheets don't define names; only LET/LAMBDA do).
    fn var(&self, _name: &str) -> Option<CellValue> {
        None
    }
    /// The lexical environment this context exposes, or `None` at the
    /// top-level. `LAMBDA` captures this snapshot at definition time so
    /// the lambda body sees the right bindings when later called.
    fn env(&self) -> Option<Arc<Env>> {
        None
    }
}

/// Wraps an `EvalCtx` with an additional `Env` scope. Used by `LAMBDA`,
/// `LET`, and `LETREC` to introduce parameter / binding scopes that
/// shadow but don't disturb the parent context's cell-and-range view.
pub struct ScopedCtx<'a> {
    pub parent: &'a dyn EvalCtx,
    pub scope: Arc<Env>,
}

impl<'a> EvalCtx for ScopedCtx<'a> {
    fn cell(&self, addr: &str) -> Result<CellValue, EvalError> {
        self.parent.cell(addr)
    }
    fn range(&self, start: &str, end: &str) -> Result<Vec<CellValue>, EvalError> {
        self.parent.range(start, end)
    }
    fn var(&self, name: &str) -> Option<CellValue> {
        self.scope.lookup(name).or_else(|| self.parent.var(name))
    }
    fn env(&self) -> Option<Arc<Env>> {
        Some(self.scope.clone())
    }
}

pub trait FormulaEngine: Send + Sync {
    fn kind(&self) -> EngineKind;
    fn parse(&self, src: &str) -> Result<CompiledFormula, ParseError>;
    /// Cell and range references the formula reads. The orchestrator uses
    /// this to update the DAG before evaluating.
    fn refs(&self, compiled: &CompiledFormula) -> Vec<(String, Option<String>)>;
    fn eval(&self, compiled: &CompiledFormula, ctx: &dyn EvalCtx) -> Result<CellValue, EvalError>;
}
