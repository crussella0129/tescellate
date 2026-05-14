//! Statistical and light-analytic functions for data-science workflows.
//! Descriptive: STDEV/STDEVP, VAR/VARP, MEDIAN, MODE, PERCENTILE, QUARTILE, RANK.
//! Bivariate: CORREL, COVAR/COVARP, SLOPE, INTERCEPT, FORECAST, RSQ.
//!
//! All consume `flatten(arg, ctx)` and filter out non-numeric values
//! (Excel convention). Bivariate functions take two co-shaped arrays.

use super::coerce::{arity_n, arity_range, as_number, flatten, to_int, to_number};
use super::FunctionRegistry;
use crate::excellite::ast::Expr;
use crate::excellite::eval::eval;
use crate::{EvalCtx, EvalError};
use tescellate_core::CellValue;

// ---------- helpers --------------------------------------------------------

/// Pull every numeric value out of `arg` (range, array, scalar).
fn numbers(arg: &Expr, ctx: &dyn EvalCtx) -> Result<Vec<f64>, EvalError> {
    Ok(flatten(arg, ctx)?.iter().filter_map(as_number).collect())
}

fn require_one(name: &str, args: &[Expr]) -> Result<(), EvalError> {
    arity_n(name, args, 1)
}

fn mean(xs: &[f64]) -> Option<f64> {
    if xs.is_empty() {
        None
    } else {
        Some(xs.iter().sum::<f64>() / xs.len() as f64)
    }
}

fn variance(xs: &[f64], population: bool) -> Option<f64> {
    let m = mean(xs)?;
    let n = xs.len();
    let denom = if population { n as f64 } else { (n - 1) as f64 };
    if denom <= 0.0 {
        return None;
    }
    let sum_sq: f64 = xs.iter().map(|x| (x - m).powi(2)).sum();
    Some(sum_sq / denom)
}

// ---------- descriptive ----------------------------------------------------

pub fn stdev(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    require_one("STDEV", args)?;
    let xs = numbers(&args[0], ctx)?;
    if xs.len() < 2 {
        return Err(EvalError::DivZero);
    }
    Ok(CellValue::Number(variance(&xs, false).unwrap().sqrt()))
}

pub fn stdevp(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    require_one("STDEVP", args)?;
    let xs = numbers(&args[0], ctx)?;
    if xs.is_empty() {
        return Err(EvalError::DivZero);
    }
    Ok(CellValue::Number(variance(&xs, true).unwrap().sqrt()))
}

pub fn var(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    require_one("VAR", args)?;
    let xs = numbers(&args[0], ctx)?;
    if xs.len() < 2 {
        return Err(EvalError::DivZero);
    }
    Ok(CellValue::Number(variance(&xs, false).unwrap()))
}

pub fn varp(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    require_one("VARP", args)?;
    let xs = numbers(&args[0], ctx)?;
    if xs.is_empty() {
        return Err(EvalError::DivZero);
    }
    Ok(CellValue::Number(variance(&xs, true).unwrap()))
}

pub fn median(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    require_one("MEDIAN", args)?;
    let mut xs = numbers(&args[0], ctx)?;
    if xs.is_empty() {
        return Err(EvalError::DivZero);
    }
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = xs.len();
    let med = if n % 2 == 1 {
        xs[n / 2]
    } else {
        (xs[n / 2 - 1] + xs[n / 2]) / 2.0
    };
    Ok(CellValue::Number(med))
}

pub fn mode(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    require_one("MODE", args)?;
    let xs = numbers(&args[0], ctx)?;
    if xs.is_empty() {
        return Err(EvalError::Num);
    }
    // Most-frequent value, first-seen wins on ties. Floats are compared
    // by bit pattern for stable counting; equal numerics group as expected.
    let mut counts: Vec<(f64, usize, usize)> = Vec::new(); // (value, first-index, count)
    for (idx, &x) in xs.iter().enumerate() {
        if let Some(entry) = counts
            .iter_mut()
            .find(|(v, _, _)| v.to_bits() == x.to_bits())
        {
            entry.2 += 1;
        } else {
            counts.push((x, idx, 1));
        }
    }
    let best = counts
        .iter()
        .max_by_key(|(_, _, count)| *count)
        .ok_or(EvalError::Num)?;
    if best.2 <= 1 {
        return Err(EvalError::Num); // Excel: no mode when every value is unique.
    }
    // Tiebreak: among modal values, return the first-seen one.
    let max_count = best.2;
    let mode_val = counts
        .iter()
        .filter(|(_, _, c)| *c == max_count)
        .min_by_key(|(_, idx, _)| *idx)
        .unwrap()
        .0;
    Ok(CellValue::Number(mode_val))
}

/// PERCENTILE with Excel-compatible linear interpolation (PERCENTILE.INC).
/// `p` ∈ [0, 1]. Reference: Excel docs for PERCENTILE.
pub fn percentile(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_n("PERCENTILE", args, 2)?;
    let mut xs = numbers(&args[0], ctx)?;
    let p = to_number(&eval(&args[1], ctx)?)?;
    if !(0.0..=1.0).contains(&p) {
        return Err(EvalError::Num);
    }
    if xs.is_empty() {
        return Err(EvalError::Num);
    }
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = xs.len();
    if n == 1 {
        return Ok(CellValue::Number(xs[0]));
    }
    let rank = p * (n as f64 - 1.0);
    let lo = rank.floor() as usize;
    let hi = rank.ceil() as usize;
    if lo == hi {
        return Ok(CellValue::Number(xs[lo]));
    }
    let frac = rank - lo as f64;
    Ok(CellValue::Number(xs[lo] + frac * (xs[hi] - xs[lo])))
}

/// QUARTILE(array, q) with q ∈ {0, 1, 2, 3, 4}. Built on PERCENTILE.
pub fn quartile(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_n("QUARTILE", args, 2)?;
    let q = to_int(&eval(&args[1], ctx)?)?;
    if !(0..=4).contains(&q) {
        return Err(EvalError::Num);
    }
    let p = q as f64 / 4.0;
    percentile(&[args[0].clone(), Expr::Number(p)], ctx)
}

/// RANK(value, range, [order]) — order=0/omitted ⇒ descending (largest is 1);
/// nonzero ⇒ ascending. Excel-style ties: same rank, next ranks skipped.
pub fn rank(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_range("RANK", args, 2, 3)?;
    let v = to_number(&eval(&args[0], ctx)?)?;
    let xs = numbers(&args[1], ctx)?;
    let descending = if let Some(arg) = args.get(2) {
        to_int(&eval(arg, ctx)?)? == 0
    } else {
        true
    };
    if !xs.contains(&v) {
        return Err(EvalError::Num);
    }
    let strictly_better = xs
        .iter()
        .filter(|&&x| if descending { x > v } else { x < v })
        .count();
    Ok(CellValue::Integer(strictly_better as i64 + 1))
}

// ---------- bivariate ------------------------------------------------------

fn pair_numbers(
    name: &str,
    args: &[Expr],
    ctx: &dyn EvalCtx,
) -> Result<(Vec<f64>, Vec<f64>), EvalError> {
    arity_n(name, args, 2)?;
    let a = flatten(&args[0], ctx)?;
    let b = flatten(&args[1], ctx)?;
    if a.len() != b.len() {
        return Err(EvalError::Value(format!(
            "{name}: input arrays must have the same length ({} vs {})",
            a.len(),
            b.len()
        )));
    }
    let mut xs = Vec::with_capacity(a.len());
    let mut ys = Vec::with_capacity(b.len());
    for (av, bv) in a.iter().zip(b.iter()) {
        if let (Some(x), Some(y)) = (as_number(av), as_number(bv)) {
            xs.push(x);
            ys.push(y);
        }
    }
    Ok((xs, ys))
}

pub fn correl(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    let (xs, ys) = pair_numbers("CORREL", args, ctx)?;
    let n = xs.len();
    if n < 2 {
        return Err(EvalError::DivZero);
    }
    let mx = mean(&xs).unwrap();
    let my = mean(&ys).unwrap();
    let mut num = 0.0;
    let mut sx = 0.0;
    let mut sy = 0.0;
    for (x, y) in xs.iter().zip(ys.iter()) {
        num += (x - mx) * (y - my);
        sx += (x - mx).powi(2);
        sy += (y - my).powi(2);
    }
    let denom = (sx * sy).sqrt();
    if denom == 0.0 {
        return Err(EvalError::DivZero);
    }
    Ok(CellValue::Number(num / denom))
}

fn covariance(xs: &[f64], ys: &[f64], population: bool) -> Option<f64> {
    let n = xs.len();
    if n == 0 {
        return None;
    }
    let denom = if population { n as f64 } else { (n - 1) as f64 };
    if denom <= 0.0 {
        return None;
    }
    let mx = mean(xs)?;
    let my = mean(ys)?;
    let sum: f64 = xs
        .iter()
        .zip(ys.iter())
        .map(|(x, y)| (x - mx) * (y - my))
        .sum();
    Some(sum / denom)
}

pub fn covar(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    // Excel's COVAR is the population covariance (legacy name).
    let (xs, ys) = pair_numbers("COVAR", args, ctx)?;
    covariance(&xs, &ys, true)
        .map(CellValue::Number)
        .ok_or(EvalError::DivZero)
}

pub fn covarp(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    let (xs, ys) = pair_numbers("COVARP", args, ctx)?;
    covariance(&xs, &ys, true)
        .map(CellValue::Number)
        .ok_or(EvalError::DivZero)
}

pub fn covars(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    // Sample covariance — sometimes spelled COVARIANCE.S.
    let (xs, ys) = pair_numbers("COVARS", args, ctx)?;
    covariance(&xs, &ys, false)
        .map(CellValue::Number)
        .ok_or(EvalError::DivZero)
}

/// Single-variable OLS slope: m in y = mx + b.
/// SLOPE(known_ys, known_xs) — Excel argument order.
pub fn slope(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    let (ys, xs) = pair_numbers("SLOPE", args, ctx)?;
    let n = xs.len();
    if n < 2 {
        return Err(EvalError::DivZero);
    }
    let mx = mean(&xs).unwrap();
    let my = mean(&ys).unwrap();
    let mut num = 0.0;
    let mut denom = 0.0;
    for (x, y) in xs.iter().zip(ys.iter()) {
        num += (x - mx) * (y - my);
        denom += (x - mx).powi(2);
    }
    if denom == 0.0 {
        return Err(EvalError::DivZero);
    }
    Ok(CellValue::Number(num / denom))
}

pub fn intercept(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    let (ys, xs) = pair_numbers("INTERCEPT", args, ctx)?;
    let n = xs.len();
    if n < 2 {
        return Err(EvalError::DivZero);
    }
    let mx = mean(&xs).unwrap();
    let my = mean(&ys).unwrap();
    let mut num = 0.0;
    let mut denom = 0.0;
    for (x, y) in xs.iter().zip(ys.iter()) {
        num += (x - mx) * (y - my);
        denom += (x - mx).powi(2);
    }
    if denom == 0.0 {
        return Err(EvalError::DivZero);
    }
    let slope = num / denom;
    Ok(CellValue::Number(my - slope * mx))
}

/// FORECAST(x, known_ys, known_xs) — predict y at x via single-variable OLS.
pub fn forecast(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_n("FORECAST", args, 3)?;
    let x = to_number(&eval(&args[0], ctx)?)?;
    let slope_v = match slope(&[args[1].clone(), args[2].clone()], ctx)? {
        CellValue::Number(n) => n,
        _ => return Err(EvalError::Num),
    };
    let intercept_v = match intercept(&[args[1].clone(), args[2].clone()], ctx)? {
        CellValue::Number(n) => n,
        _ => return Err(EvalError::Num),
    };
    Ok(CellValue::Number(slope_v * x + intercept_v))
}

pub fn rsq(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    let r = match correl(args, ctx)? {
        CellValue::Number(n) => n,
        _ => return Err(EvalError::Num),
    };
    Ok(CellValue::Number(r * r))
}

pub fn register(r: &mut FunctionRegistry) {
    r.add("STDEV", stdev);
    r.add("STDEVP", stdevp);
    r.add("VAR", var);
    r.add("VARP", varp);
    r.add("MEDIAN", median);
    r.add("MODE", mode);
    r.add("PERCENTILE", percentile);
    r.add("QUARTILE", quartile);
    r.add("RANK", rank);
    r.add("CORREL", correl);
    r.add("COVAR", covar);
    r.add("COVARP", covarp);
    r.add("COVARS", covars);
    r.add("SLOPE", slope);
    r.add("INTERCEPT", intercept);
    r.add("FORECAST", forecast);
    r.add("RSQ", rsq);
}
