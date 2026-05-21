//! Type coercion helpers shared across function implementations.
//! These mirror Excel's coercion rules so functions behave consistently.

use crate::excellite::ast::Expr;
use crate::excellite::eval::eval;
use crate::{EvalCtx, EvalError};
use tescellate_core::CellValue;

pub fn as_number(v: &CellValue) -> Option<f64> {
    match v {
        CellValue::Number(n) => Some(*n),
        CellValue::Integer(i) => Some(*i as f64),
        CellValue::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        _ => None,
    }
}

pub fn to_number(v: &CellValue) -> Result<f64, EvalError> {
    match v {
        CellValue::Number(n) => Ok(*n),
        CellValue::Integer(i) => Ok(*i as f64),
        CellValue::Bool(b) => Ok(if *b { 1.0 } else { 0.0 }),
        CellValue::Empty => Ok(0.0),
        CellValue::Text(s) => s
            .trim()
            .parse::<f64>()
            .map_err(|_| EvalError::Value(format!("cannot coerce {s:?} to number"))),
        CellValue::Error(e) => Err(EvalError::Value(format!("upstream error {e:?}"))),
        CellValue::Array(_) | CellValue::Pending => {
            Err(EvalError::Value("array/pending in scalar context".into()))
        }
        CellValue::Function(_) => Err(EvalError::Value(
            "function value in scalar context (use APPLY or pass to MAP/REDUCE)".into(),
        )),
        // A reference reaching the scalar coercion un-dereferenced is a
        // bug upstream — `deref_reference` should have resolved it to the
        // target's value before arithmetic. Surface it rather than guess.
        CellValue::Reference(_) => Err(EvalError::Value(
            "reference in scalar context (should have been dereferenced)".into(),
        )),
    }
}

pub fn to_bool(v: &CellValue) -> bool {
    match v {
        CellValue::Bool(b) => *b,
        CellValue::Number(n) => *n != 0.0,
        CellValue::Integer(i) => *i != 0,
        CellValue::Text(s) => !s.is_empty(),
        CellValue::Empty => false,
        _ => false,
    }
}

pub fn to_int(v: &CellValue) -> Result<i64, EvalError> {
    let n = to_number(v)?;
    if n.is_finite() {
        Ok(n.trunc() as i64)
    } else {
        Err(EvalError::Num)
    }
}

pub fn stringify(v: &CellValue) -> String {
    match v {
        CellValue::Text(s) => s.clone(),
        CellValue::Number(n) => format_number(*n),
        CellValue::Integer(i) => i.to_string(),
        CellValue::Bool(true) => "TRUE".into(),
        CellValue::Bool(false) => "FALSE".into(),
        CellValue::Empty => String::new(),
        CellValue::Error(e) => format!("{e:?}"),
        CellValue::Array(_) => "{array}".into(),
        CellValue::Pending => "...".into(),
        CellValue::Function(f) => f.debug_label(),
        // A bare reference stringifies to its canonical address text.
        CellValue::Reference(r) => match r {
            tescellate_core::RefShape::Cell(a) => a.clone(),
            tescellate_core::RefShape::Range(a, b) => format!("{a}:{b}"),
        },
    }
}

pub fn format_number(n: f64) -> String {
    if n == n.trunc() && n.abs() < 1e16 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

/// Evaluate `arg` and flatten the result into a sequence of scalars.
/// `Range` is expanded to a row-major scalar list; `Expr::Array` and
/// `CellValue::Array` are likewise flattened. Anything else passes through
/// as a single-element sequence.
pub fn flatten(arg: &Expr, ctx: &dyn EvalCtx) -> Result<Vec<CellValue>, EvalError> {
    match arg {
        Expr::Range(a, b) => ctx.range(a, b),
        _ => {
            let v = eval(arg, ctx)?;
            Ok(flatten_value(v))
        }
    }
}

pub fn flatten_value(v: CellValue) -> Vec<CellValue> {
    match v {
        CellValue::Array(arr) => arr.data,
        other => vec![other],
    }
}

pub fn each_numeric(
    args: &[Expr],
    ctx: &dyn EvalCtx,
    mut f: impl FnMut(f64),
) -> Result<(), EvalError> {
    for a in args {
        for v in flatten(a, ctx)? {
            if let Some(n) = as_number(&v) {
                f(n);
            }
        }
    }
    Ok(())
}

/// Cross-type comparison, Excel-style: number < text < bool, with each
/// type internally ordered naturally. Used for `<`, `>`, `=`, etc.
pub fn compare(a: &CellValue, b: &CellValue) -> std::cmp::Ordering {
    fn tag(v: &CellValue) -> u8 {
        match v {
            CellValue::Empty => 0,
            CellValue::Number(_) | CellValue::Integer(_) => 1,
            CellValue::Text(_) => 2,
            CellValue::Bool(_) => 3,
            _ => 4,
        }
    }
    let ta = tag(a);
    let tb = tag(b);
    if ta != tb {
        if matches!(a, CellValue::Empty)
            && matches!(b, CellValue::Number(_) | CellValue::Integer(_))
        {
            return compare(&CellValue::Number(0.0), b);
        }
        if matches!(b, CellValue::Empty)
            && matches!(a, CellValue::Number(_) | CellValue::Integer(_))
        {
            return compare(a, &CellValue::Number(0.0));
        }
        if matches!(a, CellValue::Empty) && matches!(b, CellValue::Text(_)) {
            return compare(&CellValue::Text(String::new()), b);
        }
        if matches!(b, CellValue::Empty) && matches!(a, CellValue::Text(_)) {
            return compare(a, &CellValue::Text(String::new()));
        }
        return ta.cmp(&tb);
    }
    use std::cmp::Ordering;
    match (a, b) {
        (CellValue::Number(x), CellValue::Number(y)) => x.partial_cmp(y).unwrap_or(Ordering::Equal),
        (CellValue::Integer(x), CellValue::Integer(y)) => x.cmp(y),
        (CellValue::Number(x), CellValue::Integer(y)) => {
            x.partial_cmp(&(*y as f64)).unwrap_or(Ordering::Equal)
        }
        (CellValue::Integer(x), CellValue::Number(y)) => {
            (*x as f64).partial_cmp(y).unwrap_or(Ordering::Equal)
        }
        (CellValue::Text(x), CellValue::Text(y)) => x.cmp(y),
        (CellValue::Bool(x), CellValue::Bool(y)) => x.cmp(y),
        _ => Ordering::Equal,
    }
}

pub fn arity_n(name: &str, args: &[Expr], n: usize) -> Result<(), EvalError> {
    if args.len() != n {
        return Err(EvalError::BadArity {
            name: name.into(),
            want: format!("{n}"),
            got: args.len(),
        });
    }
    Ok(())
}

pub fn arity_range(name: &str, args: &[Expr], min: usize, max: usize) -> Result<(), EvalError> {
    if args.len() < min || args.len() > max {
        return Err(EvalError::BadArity {
            name: name.into(),
            want: format!("{min}..={max}"),
            got: args.len(),
        });
    }
    Ok(())
}

pub fn arity_at_least(name: &str, args: &[Expr], min: usize) -> Result<(), EvalError> {
    if args.len() < min {
        return Err(EvalError::BadArity {
            name: name.into(),
            want: format!(">={min}"),
            got: args.len(),
        });
    }
    Ok(())
}
