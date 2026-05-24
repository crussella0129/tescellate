//! `Lambda` — the concrete `CarbideFn` impl backing `LAMBDA` literals,
//! `LET`-bound function values, and `LETREC`-defined mutually-recursive
//! functions. See PLAN.md §6.2.4.
//!
//! A lambda is "just" a triple of (parameter names, body AST, captured
//! environment). `call` builds a fresh `Env` containing the arg bindings,
//! chains it to the captured env, wraps the caller in a `ScopedCtx`, and
//! recursively evaluates the body. Lexical scoping falls out for free.

use crate::excellite::ast::Expr;
use crate::excellite::eval::eval;
use crate::{EvalCtx, EvalError, ScopedCtx};
use carbide_core::{CarbideFn, CellValue, Env};
use std::any::Any;
use std::sync::Arc;

#[derive(Debug)]
pub struct Lambda {
    pub params: Vec<String>,
    pub body: Expr,
    pub captured: Arc<Env>,
}

impl CarbideFn for Lambda {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn debug_label(&self) -> String {
        format!("λ({}) → …", self.params.join(", "))
    }
}

impl Lambda {
    pub fn call(&self, args: Vec<CellValue>, outer: &dyn EvalCtx) -> Result<CellValue, EvalError> {
        if args.len() != self.params.len() {
            return Err(EvalError::BadArity {
                name: "<lambda>".into(),
                want: self.params.len().to_string(),
                got: args.len(),
            });
        }
        let scope = Env::child_of(self.captured.clone());
        for (name, value) in self.params.iter().zip(args) {
            scope.insert(name.clone(), value);
        }
        let ctx = ScopedCtx {
            parent: outer,
            scope,
        };
        eval(&self.body, &ctx)
    }
}
