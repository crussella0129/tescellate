//! Excel-lite engine. See PLAN.md §6.2.
//!
//! Phase 1 scope:
//! - literals: number, string, bool
//! - cell refs (`A1`) and ranges (`A1:B5`) — square lattice only
//! - unary `-`, `+`; binary `+ - * / ^ & = <> < > <= >=`
//! - parenthesised expressions
//! - function calls: SUM, AVERAGE, IF, COUNT, MIN, MAX, AND, OR, NOT, ABS
//!
//! Out of scope for Phase 1: absolute refs (`$A$1`), cross-sheet refs
//! (`Sheet1!A1`), defined names, array literals, dates, R1C1 style.

pub mod ast;
pub mod eval;
pub mod funcs;
pub mod lambda;
pub mod lex;
pub mod parse;

pub use ast::{BinaryOp, Expr, UnaryOp};
pub use eval::{collect_refs, eval, eval_error_to_cell_error};
pub use parse::parse;

use crate::{CompiledFormula, EvalCtx, EvalError, FormulaEngine, FormulaRef, ParseError};
use carbide_core::{CellValue, EngineKind};

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

    fn refs(&self, compiled: &CompiledFormula) -> Vec<FormulaRef> {
        match compiled {
            CompiledFormula::ExcelLite(expr) => {
                let mut out = Vec::new();
                collect_refs(expr, &mut out);
                out
            }
            #[cfg(feature = "python")]
            CompiledFormula::Python(_) => Vec::new(),
        }
    }

    fn eval(&self, compiled: &CompiledFormula, ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
        match compiled {
            CompiledFormula::ExcelLite(expr) => eval(expr, ctx),
            #[cfg(feature = "python")]
            CompiledFormula::Python(_) => Err(EvalError::Value(
                "internal: a Python formula reached the Excel-lite engine".into(),
            )),
        }
    }
}
