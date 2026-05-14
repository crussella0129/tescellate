//! Math functions: ABS, ROUND, MOD, POWER, SQRT, EXP, LN, LOG, INT, TRUNC, SIGN.

use super::coerce::{arity_n, arity_range, to_number};
use super::FunctionRegistry;
use crate::excellite::ast::Expr;
use crate::excellite::eval::eval;
use crate::{EvalCtx, EvalError};
use tescellate_core::CellValue;

fn one_num(name: &str, args: &[Expr], ctx: &dyn EvalCtx) -> Result<f64, EvalError> {
    arity_n(name, args, 1)?;
    to_number(&eval(&args[0], ctx)?)
}

pub fn abs(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    Ok(CellValue::Number(one_num("ABS", args, ctx)?.abs()))
}

pub fn round(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_range("ROUND", args, 1, 2)?;
    let n = to_number(&eval(&args[0], ctx)?)?;
    let digits = if let Some(d) = args.get(1) {
        to_number(&eval(d, ctx)?)? as i32
    } else {
        0
    };
    let factor = 10f64.powi(digits);
    Ok(CellValue::Number((n * factor).round() / factor))
}

pub fn ceiling(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_range("CEILING", args, 1, 2)?;
    let n = to_number(&eval(&args[0], ctx)?)?;
    let sig = if let Some(d) = args.get(1) {
        to_number(&eval(d, ctx)?)?
    } else {
        1.0
    };
    if sig == 0.0 {
        return Ok(CellValue::Number(0.0));
    }
    Ok(CellValue::Number((n / sig).ceil() * sig))
}

pub fn floor(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_range("FLOOR", args, 1, 2)?;
    let n = to_number(&eval(&args[0], ctx)?)?;
    let sig = if let Some(d) = args.get(1) {
        to_number(&eval(d, ctx)?)?
    } else {
        1.0
    };
    if sig == 0.0 {
        return Ok(CellValue::Number(0.0));
    }
    Ok(CellValue::Number((n / sig).floor() * sig))
}

pub fn modulo(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_n("MOD", args, 2)?;
    let n = to_number(&eval(&args[0], ctx)?)?;
    let d = to_number(&eval(&args[1], ctx)?)?;
    if d == 0.0 {
        return Err(EvalError::DivZero);
    }
    let result = n - (n / d).floor() * d;
    Ok(CellValue::Number(result))
}

pub fn power(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_n("POWER", args, 2)?;
    let base = to_number(&eval(&args[0], ctx)?)?;
    let exp = to_number(&eval(&args[1], ctx)?)?;
    let v = base.powf(exp);
    if !v.is_finite() {
        return Err(EvalError::Num);
    }
    Ok(CellValue::Number(v))
}

pub fn sqrt(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    let n = one_num("SQRT", args, ctx)?;
    if n < 0.0 {
        return Err(EvalError::Num);
    }
    Ok(CellValue::Number(n.sqrt()))
}

pub fn exp(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    Ok(CellValue::Number(one_num("EXP", args, ctx)?.exp()))
}

pub fn ln(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    let n = one_num("LN", args, ctx)?;
    if n <= 0.0 {
        return Err(EvalError::Num);
    }
    Ok(CellValue::Number(n.ln()))
}

pub fn log(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_range("LOG", args, 1, 2)?;
    let n = to_number(&eval(&args[0], ctx)?)?;
    if n <= 0.0 {
        return Err(EvalError::Num);
    }
    let base = if let Some(b) = args.get(1) {
        to_number(&eval(b, ctx)?)?
    } else {
        10.0
    };
    if base <= 0.0 || base == 1.0 {
        return Err(EvalError::Num);
    }
    Ok(CellValue::Number(n.log(base)))
}

pub fn int(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    Ok(CellValue::Number(one_num("INT", args, ctx)?.floor()))
}

pub fn trunc(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    Ok(CellValue::Number(one_num("TRUNC", args, ctx)?.trunc()))
}

pub fn sign(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    let n = one_num("SIGN", args, ctx)?;
    Ok(CellValue::Integer(if n > 0.0 {
        1
    } else if n < 0.0 {
        -1
    } else {
        0
    }))
}

pub fn pi(args: &[Expr], _ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_n("PI", args, 0)?;
    Ok(CellValue::Number(std::f64::consts::PI))
}

pub fn register(r: &mut FunctionRegistry) {
    r.add("ABS", abs);
    r.add("ROUND", round);
    r.add("CEILING", ceiling);
    r.add("FLOOR", floor);
    r.add("MOD", modulo);
    r.add("POWER", power);
    r.add("SQRT", sqrt);
    r.add("EXP", exp);
    r.add("LN", ln);
    r.add("LOG", log);
    r.add("INT", int);
    r.add("TRUNC", trunc);
    r.add("SIGN", sign);
    r.add("PI", pi);
}
