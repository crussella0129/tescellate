//! Excel-lite engine. See PLAN.md §6.2.
//!
//! Phase 1 scope:
//! - literals: number, string, bool
//! - cell refs (`A1`) and ranges (`A1:B5`) — square lattice only
//! - unary `-`, `+`; binary `+ - * / ^ & = <> < > <= >=`
//! - parenthesised expressions
//! - function calls: `SUM`, `AVERAGE`, `IF`, `COUNT`, `MIN`, `MAX`
//!
//! Out of scope for Phase 1: absolute refs (`$A$1`), cross-sheet refs
//! (`Sheet1!A1`), defined names, array literals, dates, R1C1 style.

pub mod ast;
pub mod lex;
pub mod parse;

pub use ast::{BinaryOp, Expr, UnaryOp};
pub use parse::parse;

use crate::{CompiledFormula, EvalCtx, EvalError, FormulaEngine, ParseError};
use tescellate_core::{CellValue, EngineKind};

pub struct ExcelLite;

impl FormulaEngine for ExcelLite {
    fn kind(&self) -> EngineKind {
        EngineKind::ExcelLite
    }

    fn parse(&self, src: &str) -> Result<CompiledFormula, ParseError> {
        // Tolerate a leading `=` so a formula can be entered as the user types it.
        let body = src.strip_prefix('=').unwrap_or(src);
        let expr = parse(body).map_err(|e| ParseError::Message(e.to_string()))?;
        Ok(CompiledFormula::ExcelLite(expr))
    }

    fn eval(
        &self,
        _compiled: &CompiledFormula,
        _ctx: &EvalCtx<'_>,
    ) -> Result<CellValue, EvalError> {
        // Evaluator lands in the next commit; for now report not-yet-implemented.
        Err(EvalError::Message("eval pending phase-1 evaluator".into()))
    }
}
