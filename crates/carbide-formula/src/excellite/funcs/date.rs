//! Date functions: DATE, YEAR, MONTH, DAY.
//!
//! Dates are represented as a serial day count, integer or floating-
//! point. `DATE(year, month, day)` returns the serial; `YEAR`,
//! `MONTH`, `DAY` extract the components back.
//!
//! Epoch: **1899-12-30 = day 0** (so `DATE(1900, 1, 1) = 2`). Excel
//! uses 1900-01-01 = day 1 *with* the well-known 1900 leap-year bug
//! that adds a phantom 1900-02-29 — we deliberately do NOT replicate
//! that bug. Spreadsheets shared between Carbide and Excel will
//! agree on every date except those in January–February 1900, which
//! is fine for the intended audience.
//!
//! Calendar math uses Howard Hinnant's `days_from_civil` /
//! `civil_from_days` algorithms — short, branchless, proven against
//! every date in the proleptic Gregorian calendar.
//!
//! `TODAY()` and `NOW()` need access to wall-clock time and are
//! deferred until `EvalCtx` exposes a clock — sketched in PLAN.md.

use super::coerce::{arity_n, to_int};
use super::FunctionRegistry;
use crate::excellite::ast::Expr;
use crate::excellite::eval::eval;
use crate::{EvalCtx, EvalError};
use carbide_core::CellValue;

const fn days_from_civil_const(y: i32, m: i32, d: i32) -> i32 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y / 400 } else { (y - 399) / 400 };
    let yoe = (y - era * 400) as u32;
    let mp = if m > 2 { m - 3 } else { m + 9 } as u32;
    let doy = (153 * mp + 2) / 5 + d as u32 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe as i32 - 719468
}

fn days_from_civil(y: i32, m: i32, d: i32) -> i32 {
    days_from_civil_const(y, m, d)
}

fn civil_from_days(z: i32) -> (i32, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 {
        z / 146097
    } else {
        (z - 146096) / 146097
    };
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y_in_era = yoe as i32;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = y_in_era + era * 400 + if m <= 2 { 1 } else { 0 };
    (y, m, d)
}

/// Convert a `(year, month, day)` tuple to a Carbide serial.
fn serial_of(year: i32, month: i32, day: i32) -> i32 {
    days_from_civil(year, month, day) - days_from_civil(1899, 12, 30)
}

/// Convert a Carbide serial back to `(year, month, day)`.
fn date_of(serial: i32) -> (i32, u32, u32) {
    civil_from_days(serial + days_from_civil(1899, 12, 30))
}

pub fn date(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_n("DATE", args, 3)?;
    let y = to_int(&eval(&args[0], ctx)?)? as i32;
    let m = to_int(&eval(&args[1], ctx)?)? as i32;
    let d = to_int(&eval(&args[2], ctx)?)? as i32;
    Ok(CellValue::Integer(serial_of(y, m, d) as i64))
}

fn serial_from_value(value: &CellValue) -> Result<i32, EvalError> {
    match value {
        CellValue::Integer(i) => Ok(*i as i32),
        CellValue::Number(n) if n.is_finite() => Ok(n.floor() as i32),
        _ => Err(EvalError::Value(
            "expected a date serial (number)".to_string(),
        )),
    }
}

pub fn year(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_n("YEAR", args, 1)?;
    let s = serial_from_value(&eval(&args[0], ctx)?)?;
    Ok(CellValue::Integer(date_of(s).0 as i64))
}

pub fn month(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_n("MONTH", args, 1)?;
    let s = serial_from_value(&eval(&args[0], ctx)?)?;
    Ok(CellValue::Integer(date_of(s).1 as i64))
}

pub fn day(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_n("DAY", args, 1)?;
    let s = serial_from_value(&eval(&args[0], ctx)?)?;
    Ok(CellValue::Integer(date_of(s).2 as i64))
}

pub fn register(r: &mut FunctionRegistry) {
    r.add("DATE", date);
    r.add("YEAR", year);
    r.add("MONTH", month);
    r.add("DAY", day);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epoch_day_zero_is_1899_12_30() {
        assert_eq!(serial_of(1899, 12, 30), 0);
        assert_eq!(date_of(0), (1899, 12, 30));
    }

    #[test]
    fn january_1_1900_is_day_2() {
        // Excel's day 1 with its leap bug = our day 2 without it.
        // Spreadsheets shared between the two agree from 1900-03-01.
        assert_eq!(serial_of(1900, 1, 1), 2);
        assert_eq!(date_of(2), (1900, 1, 1));
    }

    #[test]
    fn march_1_1900_aligns_with_excel() {
        // After the Excel phantom leap, both calendars agree.
        let s = serial_of(1900, 3, 1);
        // Excel: 1900-03-01 = day 61. Our shift puts it at 61 too —
        // Excel's day-1 base + 60 days vs ours (1899-12-30 base + 61
        // days = 1900-03-01).
        assert_eq!(s, 61);
    }

    #[test]
    fn modern_date_round_trips() {
        // A date well past Excel's leap-bug zone.
        let s = serial_of(2024, 3, 15);
        let (y, m, d) = date_of(s);
        assert_eq!((y, m, d), (2024, 3, 15));
    }

    #[test]
    fn leap_day_2024() {
        // Feb 29 2024 is a real leap day. The round-trip must hold.
        let s = serial_of(2024, 2, 29);
        assert_eq!(date_of(s), (2024, 2, 29));
        // March 1 is one day later.
        assert_eq!(date_of(s + 1), (2024, 3, 1));
    }

    #[test]
    fn negative_year() {
        // Proleptic Gregorian: dates before year 1 are valid serials.
        let s = serial_of(1, 1, 1);
        assert!(s < 0);
        let (y, m, d) = date_of(s);
        assert_eq!((y, m, d), (1, 1, 1));
    }
}
