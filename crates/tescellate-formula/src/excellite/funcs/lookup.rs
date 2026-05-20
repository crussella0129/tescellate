//! Lookup functions: VLOOKUP, HLOOKUP, INDEX, MATCH, CHOOSE, XLOOKUP.
//!
//! XLOOKUP (v147) supports `match_mode` 0/-1/+1 and `search_mode` ±1.
//! Wildcard match (`match_mode = 2`) errors with a clear message —
//! a glob/regex backend is the natural follow-up. Binary search modes
//! (`search_mode = ±2`) accept the parameter but currently fall back to
//! a linear scan; the asymptotic improvement only matters on large
//! workbooks past launch.
//!
//! OFFSET and INDIRECT still come later — they return cell-reference
//! shapes the engine doesn't yet model as first-class values.

use super::coerce::{arity_range, compare, flatten, to_int};
use super::FunctionRegistry;
use crate::excellite::ast::Expr;
use crate::excellite::eval::eval;
use crate::{EvalCtx, EvalError};
use tescellate_core::{Array, CellValue};

/// Evaluate `arg` to a 2D array. Range and ArrayLit are accepted naturally;
/// scalar values become 1×1 arrays.
fn to_array_2d(arg: &Expr, ctx: &dyn EvalCtx) -> Result<Array, EvalError> {
    match arg {
        Expr::Range(a, b) => {
            // We don't know the rect shape here; approximate as a column.
            // Range expansion gives row-major (col-major in our convention is
            // wrong) — keep it as a 1D column for now. INDEX/MATCH use this
            // shape; if cols > 1 the caller has to handle it specifically.
            let data = ctx.range(a, b)?;
            let len = data.len();
            Ok(Array::col(data).normalise_if_empty(len))
        }
        _ => {
            let v = eval(arg, ctx)?;
            match v {
                CellValue::Array(a) => Ok(*a),
                other => Ok(Array::new(1, 1, vec![other])),
            }
        }
    }
}

trait ArrayExt {
    fn normalise_if_empty(self, len: usize) -> Self;
}

impl ArrayExt for Array {
    fn normalise_if_empty(self, len: usize) -> Self {
        if self.rows == 0 && self.cols == 0 && len > 0 {
            Array::col(self.data)
        } else {
            self
        }
    }
}

/// VLOOKUP(needle, table, col_index, [range_lookup]).
/// Phase 1.5 supports exact match only; `range_lookup` is accepted but ignored.
pub fn vlookup(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_range("VLOOKUP", args, 3, 4)?;
    let needle = eval(&args[0], ctx)?;
    let table = to_array_2d(&args[1], ctx)?;
    let col_idx = to_int(&eval(&args[2], ctx)?)?;
    if col_idx < 1 || col_idx as usize > table.cols {
        return Err(EvalError::Value(format!(
            "VLOOKUP: col {col_idx} out of range 1..={}",
            table.cols
        )));
    }
    for r in 0..table.rows {
        if let Some(cell) = table.get(r, 0) {
            if compare(cell, &needle) == std::cmp::Ordering::Equal {
                return Ok(table
                    .get(r, (col_idx - 1) as usize)
                    .cloned()
                    .unwrap_or(CellValue::Empty));
            }
        }
    }
    Err(EvalError::Ref("VLOOKUP: not found".into()))
}

pub fn hlookup(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_range("HLOOKUP", args, 3, 4)?;
    let needle = eval(&args[0], ctx)?;
    let table = to_array_2d(&args[1], ctx)?;
    let row_idx = to_int(&eval(&args[2], ctx)?)?;
    if row_idx < 1 || row_idx as usize > table.rows {
        return Err(EvalError::Value(format!(
            "HLOOKUP: row {row_idx} out of range 1..={}",
            table.rows
        )));
    }
    for c in 0..table.cols {
        if let Some(cell) = table.get(0, c) {
            if compare(cell, &needle) == std::cmp::Ordering::Equal {
                return Ok(table
                    .get((row_idx - 1) as usize, c)
                    .cloned()
                    .unwrap_or(CellValue::Empty));
            }
        }
    }
    Err(EvalError::Ref("HLOOKUP: not found".into()))
}

/// INDEX(array, row, [col]). Both indices are 1-based; passing 0 returns the
/// full row/column as a 1D array.
pub fn index(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_range("INDEX", args, 2, 3)?;
    let arr = to_array_2d(&args[0], ctx)?;
    let row = to_int(&eval(&args[1], ctx)?)?;
    let col = if let Some(c) = args.get(2) {
        to_int(&eval(c, ctx)?)?
    } else {
        // Single-axis indexing: pick column if 1×N, else row.
        if arr.rows == 1 {
            row
        } else {
            0
        }
    };
    let row_idx = if arr.rows == 1 { 1 } else { row };
    let col_idx = if arr.cols == 1 || col == 0 { 1 } else { col };
    if row_idx < 1 || (row_idx as usize) > arr.rows || col_idx < 1 || (col_idx as usize) > arr.cols
    {
        return Err(EvalError::Ref(format!(
            "INDEX: ({row}, {col}) out of {}×{}",
            arr.rows, arr.cols
        )));
    }
    Ok(arr
        .get((row_idx - 1) as usize, (col_idx - 1) as usize)
        .cloned()
        .unwrap_or(CellValue::Empty))
}

/// MATCH(needle, range, [match_type]). Phase 1.5 supports exact match (type=0).
pub fn match_(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_range("MATCH", args, 2, 3)?;
    let needle = eval(&args[0], ctx)?;
    let values = flatten(&args[1], ctx)?;
    let _mode = if let Some(m) = args.get(2) {
        to_int(&eval(m, ctx)?)?
    } else {
        1
    };
    for (i, v) in values.iter().enumerate() {
        if compare(v, &needle) == std::cmp::Ordering::Equal {
            return Ok(CellValue::Integer((i + 1) as i64));
        }
    }
    Err(EvalError::Ref("MATCH: not found".into()))
}

/// XLOOKUP(lookup_value, lookup_array, return_array, [if_not_found],
///         [match_mode], [search_mode]).
///
/// match_mode (default 0):
///   0  — exact match (returns `if_not_found` / `#N/A` on miss)
///  -1  — exact, else the next-smaller value
///   1  — exact, else the next-larger value
///   2  — wildcard (deferred — errors here)
///
/// search_mode (default 1):
///   1  — first-to-last
///  -1  — last-to-first
///  ±2  — binary search (accepted; falls back to linear scan today)
pub fn xlookup(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_range("XLOOKUP", args, 3, 6)?;
    let needle = eval(&args[0], ctx)?;
    let lookup = flatten(&args[1], ctx)?;
    // Parallel-array semantics: lookup_array and result_array are
    // treated as parallel sequences regardless of their 2D shape. The
    // "return a row of a 2D table" XLOOKUP variant is a follow-up.
    let result = flatten(&args[2], ctx)?;
    if lookup.len() != result.len() {
        return Err(EvalError::Value(format!(
            "XLOOKUP: lookup_array length {} != result_array length {}",
            lookup.len(),
            result.len()
        )));
    }
    let match_mode = if let Some(arg) = args.get(4) {
        to_int(&eval(arg, ctx)?)?
    } else {
        0
    };
    let search_mode = if let Some(arg) = args.get(5) {
        to_int(&eval(arg, ctx)?)?
    } else {
        1
    };

    if match_mode == 2 {
        return Err(EvalError::Value(
            "XLOOKUP: wildcard match (match_mode = 2) not yet supported".into(),
        ));
    }
    if !(-1..=1).contains(&match_mode) {
        return Err(EvalError::Value(format!(
            "XLOOKUP: match_mode {match_mode} out of range -1..=2"
        )));
    }
    if !(-2..=2).contains(&search_mode) || search_mode == 0 {
        return Err(EvalError::Value(format!(
            "XLOOKUP: search_mode {search_mode} out of range ±1/±2"
        )));
    }

    // Iteration order: -1 / -2 walk last-to-first, otherwise first-to-last.
    // (Binary search modes fall back to linear scan today; the asymptotic
    // improvement is on the deferred list.)
    let indices: Box<dyn Iterator<Item = usize>> = if search_mode < 0 {
        Box::new((0..lookup.len()).rev())
    } else {
        Box::new(0..lookup.len())
    };

    // Track the best-fit index for match_mode -1 / 1 so we can return
    // the closest matching value if no exact match is found.
    let mut exact: Option<usize> = None;
    let mut best_fit: Option<usize> = None;

    for i in indices {
        let v = &lookup[i];
        match compare(v, &needle) {
            std::cmp::Ordering::Equal => {
                exact = Some(i);
                break;
            }
            std::cmp::Ordering::Less if match_mode == -1 => {
                // Keep the largest value that is still <= needle.
                let beats = match best_fit {
                    None => true,
                    Some(b) => compare(&lookup[b], v) == std::cmp::Ordering::Less,
                };
                if beats {
                    best_fit = Some(i);
                }
            }
            std::cmp::Ordering::Greater if match_mode == 1 => {
                // Keep the smallest value that is still >= needle.
                let beats = match best_fit {
                    None => true,
                    Some(b) => compare(&lookup[b], v) == std::cmp::Ordering::Greater,
                };
                if beats {
                    best_fit = Some(i);
                }
            }
            _ => {}
        }
    }

    let hit = exact.or(best_fit);
    let Some(idx) = hit else {
        // Fallback path: return `if_not_found` if the caller supplied one,
        // otherwise the canonical #N/A error.
        return if let Some(arg) = args.get(3) {
            eval(arg, ctx)
        } else {
            Err(EvalError::Ref("XLOOKUP: not found".into()))
        };
    };

    Ok(result.get(idx).cloned().unwrap_or(CellValue::Empty))
}

/// CHOOSE(index, val1, val2, ...).
pub fn choose(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::BadArity {
            name: "CHOOSE".into(),
            want: ">=2".into(),
            got: args.len(),
        });
    }
    let i = to_int(&eval(&args[0], ctx)?)?;
    if i < 1 || (i as usize) >= args.len() {
        return Err(EvalError::Ref(format!("CHOOSE: index {i} out of range")));
    }
    eval(&args[i as usize], ctx)
}

pub fn register(r: &mut FunctionRegistry) {
    r.add("VLOOKUP", vlookup);
    r.add("HLOOKUP", hlookup);
    r.add("INDEX", index);
    r.add("MATCH", match_);
    r.add("CHOOSE", choose);
    r.add("XLOOKUP", xlookup);
}
