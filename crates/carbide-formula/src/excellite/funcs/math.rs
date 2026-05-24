//! Math functions: ABS, ROUND, MOD, POWER, SQRT, EXP, LN, LOG, INT,
//! TRUNC, SIGN, PI, RAND, RANDBETWEEN.

use std::sync::atomic::{AtomicU64, Ordering};

use super::coerce::{arity_n, arity_range, to_number};
use super::FunctionRegistry;
use crate::excellite::ast::Expr;
use crate::excellite::eval::eval;
use crate::{EvalCtx, EvalError};
use carbide_core::CellValue;

/// Process-wide PRNG state shared by `RAND` and `RANDBETWEEN`. The
/// state is mutated on every call (consecutive calls inside the same
/// recompute return distinct values, matching what spreadsheet users
/// expect). The engine is single-threaded so `Relaxed` ordering is
/// fine; the atomic is just for `static mut` avoidance.
///
/// Seeded with a fixed non-zero constant — deterministic across the
/// first call sequence after process start, which is what the tests
/// rely on. A future enhancement can mix in the system clock when
/// available (wasm needs a JS interop for that).
static RNG_STATE: AtomicU64 = AtomicU64::new(0x9E3779B97F4A7C15);

/// One step of an xorshift64* generator. Cheap, no dependencies,
/// passes BigCrush — overkill for spreadsheet dice but free.
fn next_u64() -> u64 {
    let mut s = RNG_STATE.load(Ordering::Relaxed);
    if s == 0 {
        s = 1;
    }
    s ^= s << 13;
    s ^= s >> 7;
    s ^= s << 17;
    RNG_STATE.store(s, Ordering::Relaxed);
    s.wrapping_mul(0x2545F491_4F6CDD1D)
}

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

/// `RAND()` — pseudo-random float in `[0, 1)`. Each call advances the
/// generator, so two `RAND()` calls in the same recompute return
/// different values.
pub fn rand(args: &[Expr], _ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_n("RAND", args, 0)?;
    // Top 53 bits of a u64 give a uniform-ish float in [0, 1).
    let bits = next_u64() >> 11;
    Ok(CellValue::Number(bits as f64 / (1u64 << 53) as f64))
}

/// `RANDBETWEEN(min, max)` — pseudo-random integer in `[min, max]`
/// inclusive. Errors when `max < min`. Returned as
/// `CellValue::Integer` so downstream `IF` / arithmetic doesn't have
/// to coerce.
pub fn randbetween(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_n("RANDBETWEEN", args, 2)?;
    let min = to_number(&eval(&args[0], ctx)?)?.floor() as i64;
    let max = to_number(&eval(&args[1], ctx)?)?.floor() as i64;
    if max < min {
        return Err(EvalError::Value("RANDBETWEEN: max must be >= min".into()));
    }
    let range = (max - min + 1) as u64;
    let value = min + (next_u64() % range) as i64;
    Ok(CellValue::Integer(value))
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
    r.add("RAND", rand);
    r.add("RANDBETWEEN", randbetween);
}
