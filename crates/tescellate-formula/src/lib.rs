//! Formula engines for Tescellate. See PLAN.md §6.

use std::sync::Arc;
use tescellate_core::{CellValue, EngineKind, Env};
use thiserror::Error;

pub mod engine;
pub mod excellite;
pub mod transpile;

#[cfg(feature = "python")]
pub mod python;

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
    /// A Python formula — the raw source, run by the embedded interpreter
    /// on each eval (see `python::PythonEngine`).
    #[cfg(feature = "python")]
    Python(String),
}

impl CompiledFormula {
    /// The Excel-lite AST, if this is an Excel-lite formula.
    pub fn as_excellite(&self) -> Option<&excellite::Expr> {
        match self {
            CompiledFormula::ExcelLite(expr) => Some(expr),
            #[cfg(feature = "python")]
            CompiledFormula::Python(_) => None,
        }
    }
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
    /// Edge-neighbors of `addr`, returned as `CellValue`s in canonical
    /// neighbor order (lattice-specific). Default impl errors so engines
    /// must opt in by providing a real implementation when the lattice
    /// supports it; this is what `NEIGHBORS()` and `RADIUS()` consult.
    fn neighbors_of(&self, _addr: &str) -> Result<Vec<CellValue>, EvalError> {
        Err(EvalError::Value(
            "neighbors are not available on this lattice".into(),
        ))
    }
    /// Every cell within `radius` edge-steps of `addr` (inclusive of the
    /// center). Same opt-in pattern as `neighbors_of`.
    fn cells_within(&self, _addr: &str, _radius: i64) -> Result<Vec<CellValue>, EvalError> {
        Err(EvalError::Value(
            "RADIUS is not available on this lattice".into(),
        ))
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
    fn neighbors_of(&self, addr: &str) -> Result<Vec<CellValue>, EvalError> {
        self.parent.neighbors_of(addr)
    }
    fn cells_within(&self, addr: &str, radius: i64) -> Result<Vec<CellValue>, EvalError> {
        self.parent.cells_within(addr, radius)
    }
}

/// A statically-knowable cell-dependency. Each variant is what the
/// engine layer expands into concrete dep cells (using the lattice for
/// `Neighbors` / `Radius`).
///
/// Dynamic references — `NEIGHBORS(some_lambda_arg)` or `RADIUS(text, n)`
/// where the address arrives at eval-time — can't be expanded statically
/// and don't appear in this list. Those formulas still *evaluate* fine,
/// they just won't propagate through the DAG when their dynamically-
/// resolved deps change. The deterministic literal-address form is what
/// users should reach for when DAG propagation matters.
#[derive(Debug, Clone, PartialEq)]
pub enum FormulaRef {
    Cell(String),
    Range(String, String),
    Neighbors(String),
    Radius(String, i64),
}

pub trait FormulaEngine: Send + Sync {
    fn kind(&self) -> EngineKind;
    fn parse(&self, src: &str) -> Result<CompiledFormula, ParseError>;
    /// Cell, range, NEIGHBORS, and RADIUS references the formula reads.
    /// The orchestrator uses this to update the DAG before evaluating.
    fn refs(&self, compiled: &CompiledFormula) -> Vec<FormulaRef>;
    fn eval(&self, compiled: &CompiledFormula, ctx: &dyn EvalCtx) -> Result<CellValue, EvalError>;
}
