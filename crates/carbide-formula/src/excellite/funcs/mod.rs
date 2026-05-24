//! Function library for the Excel-lite engine. See PLAN.md §6.2.3.
//!
//! Functions are grouped by category into their own modules; each module
//! exposes a `register(&mut FunctionRegistry)` that adds its functions to
//! the shared registry. Adding a new function is:
//!   1. Write `pub fn <name>(args, ctx) -> Result<CellValue, EvalError>`.
//!   2. Add `r.add("<NAME>", <name>)` in that module's `register`.

use crate::excellite::ast::Expr;
use crate::{EvalCtx, EvalError};
use carbide_core::CellValue;
use std::collections::HashMap;
use std::sync::OnceLock;

pub mod aggregate;
pub mod coerce;
pub mod date;
pub mod dynarray;
pub mod lambda_funcs;
pub mod logical;
pub mod lookup;
pub mod math;
pub mod neighborhood;
pub mod stats;
pub mod text;

pub type FuncImpl = fn(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError>;

pub struct FunctionRegistry {
    funcs: HashMap<&'static str, FuncImpl>,
}

impl FunctionRegistry {
    pub fn new() -> Self {
        Self {
            funcs: HashMap::new(),
        }
    }

    pub fn add(&mut self, name: &'static str, f: FuncImpl) {
        self.funcs.insert(name, f);
    }

    pub fn call(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &dyn EvalCtx,
    ) -> Result<CellValue, EvalError> {
        match self.funcs.get(name) {
            Some(f) => f(args, ctx),
            None => Err(EvalError::UnknownFn(name.to_string())),
        }
    }
}

impl Default for FunctionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// The standard registry — every built-in function in one place. Initialized
/// once per process; lattice-specific extensions can be added by re-building
/// (Phase 2+).
pub fn standard() -> &'static FunctionRegistry {
    static REGISTRY: OnceLock<FunctionRegistry> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let mut r = FunctionRegistry::new();
        aggregate::register(&mut r);
        date::register(&mut r);
        dynarray::register(&mut r);
        lambda_funcs::register(&mut r);
        logical::register(&mut r);
        lookup::register(&mut r);
        math::register(&mut r);
        neighborhood::register(&mut r);
        stats::register(&mut r);
        text::register(&mut r);
        r
    })
}
