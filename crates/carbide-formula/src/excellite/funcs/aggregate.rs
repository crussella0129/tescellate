//! Aggregate functions: SUM, AVERAGE, COUNT, MIN, MAX, COUNTA.

use super::coerce::{each_numeric, flatten};
use super::FunctionRegistry;
use crate::excellite::ast::Expr;
use crate::{EvalCtx, EvalError};
use tescellate_core::CellValue;

pub fn sum(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    let mut acc = 0.0;
    each_numeric(args, ctx, |n| acc += n)?;
    Ok(CellValue::Number(acc))
}

pub fn average(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    let mut sum = 0.0;
    let mut count = 0usize;
    each_numeric(args, ctx, |n| {
        sum += n;
        count += 1;
    })?;
    if count == 0 {
        return Err(EvalError::DivZero);
    }
    Ok(CellValue::Number(sum / count as f64))
}

pub fn count(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    let mut c = 0i64;
    each_numeric(args, ctx, |_| c += 1)?;
    Ok(CellValue::Integer(c))
}

/// COUNTA: count of non-empty values (text included, unlike COUNT).
pub fn counta(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    let mut c = 0i64;
    for a in args {
        for v in flatten(a, ctx)? {
            if !matches!(v, CellValue::Empty) {
                c += 1;
            }
        }
    }
    Ok(CellValue::Integer(c))
}

pub fn min(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    let mut acc = f64::INFINITY;
    each_numeric(args, ctx, |n| acc = acc.min(n))?;
    Ok(CellValue::Number(if acc.is_finite() { acc } else { 0.0 }))
}

pub fn max(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    let mut acc = f64::NEG_INFINITY;
    each_numeric(args, ctx, |n| acc = acc.max(n))?;
    Ok(CellValue::Number(if acc.is_finite() { acc } else { 0.0 }))
}

pub fn register(r: &mut FunctionRegistry) {
    r.add("SUM", sum);
    r.add("AVERAGE", average);
    r.add("AVG", average);
    r.add("COUNT", count);
    r.add("COUNTA", counta);
    r.add("MIN", min);
    r.add("MAX", max);
}
