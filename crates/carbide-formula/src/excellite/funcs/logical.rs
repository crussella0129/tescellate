//! Logical functions: IF, AND, OR, NOT, IFS, SWITCH, IFERROR, IFNA,
//! ISBLANK, ISNUMBER, ISTEXT, ISERROR, XOR, TRUE, FALSE (as functions).

use super::coerce::{arity_n, arity_range, to_bool};
use super::FunctionRegistry;
use crate::excellite::ast::Expr;
use crate::excellite::eval::eval;
use crate::{EvalCtx, EvalError};
use carbide_core::CellValue;

pub fn if_(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_range("IF", args, 2, 3)?;
    let cond = to_bool(&eval(&args[0], ctx)?);
    if cond {
        eval(&args[1], ctx)
    } else if let Some(else_branch) = args.get(2) {
        eval(else_branch, ctx)
    } else {
        Ok(CellValue::Bool(false))
    }
}

pub fn and(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    for a in args {
        if !to_bool(&eval(a, ctx)?) {
            return Ok(CellValue::Bool(false));
        }
    }
    Ok(CellValue::Bool(true))
}

pub fn or(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    for a in args {
        if to_bool(&eval(a, ctx)?) {
            return Ok(CellValue::Bool(true));
        }
    }
    Ok(CellValue::Bool(false))
}

pub fn not(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_n("NOT", args, 1)?;
    Ok(CellValue::Bool(!to_bool(&eval(&args[0], ctx)?)))
}

pub fn xor(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    let mut c = 0u32;
    for a in args {
        if to_bool(&eval(a, ctx)?) {
            c += 1;
        }
    }
    Ok(CellValue::Bool(c % 2 == 1))
}

/// IFS(cond1, val1, cond2, val2, ...). First true cond wins.
pub fn ifs(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    if args.is_empty() || args.len() % 2 != 0 {
        return Err(EvalError::BadArity {
            name: "IFS".into(),
            want: "even, >=2".into(),
            got: args.len(),
        });
    }
    let mut i = 0;
    while i + 1 < args.len() {
        if to_bool(&eval(&args[i], ctx)?) {
            return eval(&args[i + 1], ctx);
        }
        i += 2;
    }
    Err(EvalError::Value("IFS: no matching condition".into()))
}

/// SWITCH(value, case1, result1, case2, result2, ..., [default]).
pub fn switch_(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    if args.len() < 3 {
        return Err(EvalError::BadArity {
            name: "SWITCH".into(),
            want: ">=3".into(),
            got: args.len(),
        });
    }
    let needle = eval(&args[0], ctx)?;
    let mut i = 1;
    while i + 1 < args.len() {
        let case = eval(&args[i], ctx)?;
        if super::coerce::compare(&needle, &case) == std::cmp::Ordering::Equal {
            return eval(&args[i + 1], ctx);
        }
        i += 2;
    }
    // Trailing odd-positioned arg is the default.
    if i < args.len() {
        return eval(&args[i], ctx);
    }
    Err(EvalError::Value("SWITCH: no matching case".into()))
}

pub fn iferror(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_n("IFERROR", args, 2)?;
    match eval(&args[0], ctx) {
        Ok(CellValue::Error(_)) => eval(&args[1], ctx),
        Ok(v) => Ok(v),
        Err(_) => eval(&args[1], ctx),
    }
}

pub fn ifna(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_n("IFNA", args, 2)?;
    // We don't have an N/A variant yet; treat #REF! / lookup misses as N/A.
    match eval(&args[0], ctx) {
        Ok(CellValue::Error(carbide_core::CellError::Ref)) => eval(&args[1], ctx),
        Ok(v) => Ok(v),
        Err(_) => eval(&args[1], ctx),
    }
}

pub fn isblank(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_n("ISBLANK", args, 1)?;
    let v = eval(&args[0], ctx)?;
    Ok(CellValue::Bool(matches!(v, CellValue::Empty)))
}

pub fn isnumber(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_n("ISNUMBER", args, 1)?;
    let v = eval(&args[0], ctx)?;
    Ok(CellValue::Bool(matches!(
        v,
        CellValue::Number(_) | CellValue::Integer(_)
    )))
}

pub fn istext(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_n("ISTEXT", args, 1)?;
    let v = eval(&args[0], ctx)?;
    Ok(CellValue::Bool(matches!(v, CellValue::Text(_))))
}

pub fn iserror(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_n("ISERROR", args, 1)?;
    Ok(CellValue::Bool(matches!(
        eval(&args[0], ctx),
        Ok(CellValue::Error(_)) | Err(_)
    )))
}

pub fn true_(args: &[Expr], _ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_n("TRUE", args, 0)?;
    Ok(CellValue::Bool(true))
}

pub fn false_(args: &[Expr], _ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_n("FALSE", args, 0)?;
    Ok(CellValue::Bool(false))
}

pub fn register(r: &mut FunctionRegistry) {
    r.add("IF", if_);
    r.add("AND", and);
    r.add("OR", or);
    r.add("NOT", not);
    r.add("XOR", xor);
    r.add("IFS", ifs);
    r.add("SWITCH", switch_);
    r.add("IFERROR", iferror);
    r.add("IFNA", ifna);
    r.add("ISBLANK", isblank);
    r.add("ISNUMBER", isnumber);
    r.add("ISTEXT", istext);
    r.add("ISERROR", iserror);
    r.add("TRUE", true_);
    r.add("FALSE", false_);
}
