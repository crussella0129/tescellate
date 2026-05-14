//! Dynamic-array functions: UNIQUE, SORT, FILTER, SEQUENCE, TAKE, DROP,
//! TRANSPOSE, FLATTEN, COUNTUNIQUE.
//!
//! These functions produce arrays; when assigned to a cell they spill into
//! the adjacent grid (see PLAN.md §6.2.2).

use super::coerce::{arity_n, arity_range, compare, flatten, to_bool, to_int, to_number};
use super::FunctionRegistry;
use crate::excellite::ast::Expr;
use crate::excellite::eval::eval;
use crate::{EvalCtx, EvalError};
use tescellate_core::{Array, CellValue};

/// UNIQUE(array) — returns distinct values in first-seen order.
pub fn unique(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_n("UNIQUE", args, 1)?;
    let values = flatten(&args[0], ctx)?;
    let mut out: Vec<CellValue> = Vec::new();
    for v in values {
        if !out
            .iter()
            .any(|x| compare(x, &v) == std::cmp::Ordering::Equal)
        {
            out.push(v);
        }
    }
    Ok(CellValue::Array(Box::new(Array::col(out))))
}

/// COUNTUNIQUE(array) — number of distinct values.
pub fn countunique(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_n("COUNTUNIQUE", args, 1)?;
    let values = flatten(&args[0], ctx)?;
    let mut out: Vec<CellValue> = Vec::new();
    for v in values {
        if !out
            .iter()
            .any(|x| compare(x, &v) == std::cmp::Ordering::Equal)
        {
            out.push(v);
        }
    }
    Ok(CellValue::Integer(out.len() as i64))
}

/// SORT(array, [sort_order=1]) — 1 ascending, -1 descending.
pub fn sort(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_range("SORT", args, 1, 2)?;
    let mut values = flatten(&args[0], ctx)?;
    let order = if let Some(o) = args.get(1) {
        to_int(&eval(o, ctx)?)?
    } else {
        1
    };
    if order >= 0 {
        values.sort_by(compare);
    } else {
        values.sort_by(|a, b| compare(b, a));
    }
    Ok(CellValue::Array(Box::new(Array::col(values))))
}

/// FILTER(array, condition_array) — keeps elements where the condition is
/// truthy. The condition array must be the same length as the source.
pub fn filter(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_n("FILTER", args, 2)?;
    let values = flatten(&args[0], ctx)?;
    let mask = flatten(&args[1], ctx)?;
    if values.len() != mask.len() {
        return Err(EvalError::Value(format!(
            "FILTER: source and mask lengths differ ({} vs {})",
            values.len(),
            mask.len()
        )));
    }
    let kept: Vec<CellValue> = values
        .into_iter()
        .zip(mask)
        .filter_map(|(v, m)| if to_bool(&m) { Some(v) } else { None })
        .collect();
    Ok(CellValue::Array(Box::new(Array::col(kept))))
}

/// SEQUENCE(rows, [cols=1], [start=1], [step=1]).
pub fn sequence(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_range("SEQUENCE", args, 1, 4)?;
    let rows = to_int(&eval(&args[0], ctx)?)?.max(0) as usize;
    let cols = if let Some(c) = args.get(1) {
        to_int(&eval(c, ctx)?)?.max(0) as usize
    } else {
        1
    };
    let start = if let Some(s) = args.get(2) {
        to_number(&eval(s, ctx)?)?
    } else {
        1.0
    };
    let step = if let Some(s) = args.get(3) {
        to_number(&eval(s, ctx)?)?
    } else {
        1.0
    };
    let mut data = Vec::with_capacity(rows * cols);
    for i in 0..(rows * cols) {
        data.push(CellValue::Number(start + (i as f64) * step));
    }
    Ok(CellValue::Array(Box::new(Array::new(rows, cols, data))))
}

/// TAKE(array, n_rows, [n_cols]) — keep the first |n| rows/cols.
/// Negative n takes from the end.
pub fn take(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_range("TAKE", args, 2, 3)?;
    let arr = to_array(&args[0], ctx)?;
    let n_rows = to_int(&eval(&args[1], ctx)?)?;
    let n_cols = if let Some(c) = args.get(2) {
        to_int(&eval(c, ctx)?)?
    } else {
        arr.cols as i64
    };
    let (row_start, row_end) = slice_indices(arr.rows, n_rows);
    let (col_start, col_end) = slice_indices(arr.cols, n_cols);
    Ok(CellValue::Array(Box::new(sub_array(
        &arr, row_start, row_end, col_start, col_end,
    ))))
}

/// DROP(array, n_rows, [n_cols]) — discard the first |n| rows/cols.
pub fn drop_(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_range("DROP", args, 2, 3)?;
    let arr = to_array(&args[0], ctx)?;
    let n_rows = to_int(&eval(&args[1], ctx)?)?;
    let n_cols = if let Some(c) = args.get(2) {
        to_int(&eval(c, ctx)?)?
    } else {
        0
    };
    let row_range = drop_indices(arr.rows, n_rows);
    let col_range = drop_indices(arr.cols, n_cols);
    Ok(CellValue::Array(Box::new(sub_array(
        &arr,
        row_range.0,
        row_range.1,
        col_range.0,
        col_range.1,
    ))))
}

/// TRANSPOSE(array) — swap rows and columns.
pub fn transpose(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_n("TRANSPOSE", args, 1)?;
    let arr = to_array(&args[0], ctx)?;
    let mut data = Vec::with_capacity(arr.data.len());
    for c in 0..arr.cols {
        for r in 0..arr.rows {
            data.push(arr.get(r, c).cloned().unwrap_or(CellValue::Empty));
        }
    }
    Ok(CellValue::Array(Box::new(Array::new(
        arr.cols, arr.rows, data,
    ))))
}

/// FLATTEN(array) — collapse to a single column.
pub fn flatten_fn(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_n("FLATTEN", args, 1)?;
    let values = flatten(&args[0], ctx)?;
    Ok(CellValue::Array(Box::new(Array::col(values))))
}

fn to_array(arg: &Expr, ctx: &dyn EvalCtx) -> Result<Array, EvalError> {
    match arg {
        Expr::Range(a, b) => Ok(Array::col(ctx.range(a, b)?)),
        _ => match eval(arg, ctx)? {
            CellValue::Array(a) => Ok(*a),
            v => Ok(Array::new(1, 1, vec![v])),
        },
    }
}

fn sub_array(
    arr: &Array,
    row_start: usize,
    row_end: usize,
    col_start: usize,
    col_end: usize,
) -> Array {
    let rows = row_end.saturating_sub(row_start);
    let cols = col_end.saturating_sub(col_start);
    let mut data = Vec::with_capacity(rows * cols);
    for r in row_start..row_end {
        for c in col_start..col_end {
            data.push(arr.get(r, c).cloned().unwrap_or(CellValue::Empty));
        }
    }
    Array::new(rows, cols, data)
}

fn slice_indices(len: usize, n: i64) -> (usize, usize) {
    if n >= 0 {
        let take = (n as usize).min(len);
        (0, take)
    } else {
        let take = ((-n) as usize).min(len);
        (len - take, len)
    }
}

fn drop_indices(len: usize, n: i64) -> (usize, usize) {
    if n >= 0 {
        let skip = (n as usize).min(len);
        (skip, len)
    } else {
        let skip = ((-n) as usize).min(len);
        (0, len - skip)
    }
}

// --- Set operations -------------------------------------------------------

/// Helper: dedupe-in-place using `compare` for value equality.
fn push_unique(out: &mut Vec<CellValue>, v: CellValue) {
    if !out
        .iter()
        .any(|x| compare(x, &v) == std::cmp::Ordering::Equal)
    {
        out.push(v);
    }
}

fn contains(haystack: &[CellValue], needle: &CellValue) -> bool {
    haystack
        .iter()
        .any(|x| compare(x, needle) == std::cmp::Ordering::Equal)
}

/// SETUNION(a, b, ...) — distinct values across every argument, in first-seen order.
pub fn setunion(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    if args.is_empty() {
        return Err(EvalError::BadArity {
            name: "SETUNION".into(),
            want: ">=1".into(),
            got: 0,
        });
    }
    let mut out: Vec<CellValue> = Vec::new();
    for a in args {
        for v in flatten(a, ctx)? {
            push_unique(&mut out, v);
        }
    }
    Ok(CellValue::Array(Box::new(Array::col(out))))
}

/// SETDIFF(a, b) — values in `a` that are not in `b`, deduped, in first-seen order.
/// This is the standard set-difference operator: `a \ b`.
///
/// "Forward difference of two delimited strings, give uniques of 2nd" in the
/// Carbide template's vocabulary translates to `SETDIFF(2nd, 1st)`.
pub fn setdiff(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_n("SETDIFF", args, 2)?;
    let a_values = flatten(&args[0], ctx)?;
    let b_values = flatten(&args[1], ctx)?;
    let mut out: Vec<CellValue> = Vec::new();
    for v in a_values {
        if !contains(&b_values, &v) {
            push_unique(&mut out, v);
        }
    }
    Ok(CellValue::Array(Box::new(Array::col(out))))
}

/// SETINTERSECT(a, b) — values present in both `a` and `b`, deduped.
pub fn setintersect(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_n("SETINTERSECT", args, 2)?;
    let a_values = flatten(&args[0], ctx)?;
    let b_values = flatten(&args[1], ctx)?;
    let mut out: Vec<CellValue> = Vec::new();
    for v in a_values {
        if contains(&b_values, &v) {
            push_unique(&mut out, v);
        }
    }
    Ok(CellValue::Array(Box::new(Array::col(out))))
}

/// SETSYMDIFF(a, b) — symmetric difference: values in exactly one of `a` or `b`.
pub fn setsymdiff(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_n("SETSYMDIFF", args, 2)?;
    let a_values = flatten(&args[0], ctx)?;
    let b_values = flatten(&args[1], ctx)?;
    let mut out: Vec<CellValue> = Vec::new();
    for v in &a_values {
        if !contains(&b_values, v) {
            push_unique(&mut out, v.clone());
        }
    }
    for v in &b_values {
        if !contains(&a_values, v) {
            push_unique(&mut out, v.clone());
        }
    }
    Ok(CellValue::Array(Box::new(Array::col(out))))
}

/// IN(value, array) → boolean. True if `value` equals any element of `array`.
pub fn in_fn(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_n("IN", args, 2)?;
    let needle = eval(&args[0], ctx)?;
    let haystack = flatten(&args[1], ctx)?;
    Ok(CellValue::Bool(contains(&haystack, &needle)))
}

/// COUNTIF(range, criterion) — count of `range` elements equal to `criterion`.
/// Phase 1.5 supports equality only; comparison-string criteria
/// (`">10"`, `"<>foo"`) are a later expansion.
pub fn countif(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_n("COUNTIF", args, 2)?;
    let haystack = flatten(&args[0], ctx)?;
    let needle = eval(&args[1], ctx)?;
    let n = haystack
        .iter()
        .filter(|v| compare(v, &needle) == std::cmp::Ordering::Equal)
        .count();
    Ok(CellValue::Integer(n as i64))
}

pub fn register(r: &mut FunctionRegistry) {
    r.add("UNIQUE", unique);
    r.add("COUNTUNIQUE", countunique);
    r.add("SORT", sort);
    r.add("FILTER", filter);
    r.add("SEQUENCE", sequence);
    r.add("TAKE", take);
    r.add("DROP", drop_);
    r.add("TRANSPOSE", transpose);
    r.add("FLATTEN", flatten_fn);
    r.add("SETUNION", setunion);
    r.add("SETDIFF", setdiff);
    r.add("SETINTERSECT", setintersect);
    r.add("SETSYMDIFF", setsymdiff);
    r.add("IN", in_fn);
    r.add("COUNTIF", countif);
}
