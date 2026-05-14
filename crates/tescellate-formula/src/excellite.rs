//! Excel-lite engine — Phase 0 placeholder; real parser lands in Phase 1.

use crate::{CompiledFormula, EvalCtx, EvalError, FormulaEngine, ParseError};
use tescellate_core::{CellValue, EngineKind};

pub struct ExcelLite;

impl FormulaEngine for ExcelLite {
    fn kind(&self) -> EngineKind {
        EngineKind::ExcelLite
    }

    fn parse(&self, _src: &str) -> Result<CompiledFormula, ParseError> {
        Ok(CompiledFormula::Stub)
    }

    fn eval(
        &self,
        _compiled: &CompiledFormula,
        _ctx: &EvalCtx<'_>,
    ) -> Result<CellValue, EvalError> {
        Ok(CellValue::Empty)
    }
}
