//! Text functions: LEN, LEFT, RIGHT, MID, UPPER, LOWER, PROPER, TRIM,
//! SUBSTITUTE, FIND, SEARCH, REPLACE, CONCAT, TEXTJOIN, TEXTSPLIT, JOIN,
//! SPLIT, REPT, EXACT, VALUE.

use super::coerce::{arity_at_least, arity_n, arity_range, flatten, stringify, to_int, to_number};
use super::FunctionRegistry;
use crate::excellite::ast::Expr;
use crate::excellite::eval::eval;
use crate::{EvalCtx, EvalError};
use tescellate_core::{Array, CellValue};

fn one_str(name: &str, args: &[Expr], ctx: &dyn EvalCtx) -> Result<String, EvalError> {
    arity_n(name, args, 1)?;
    Ok(stringify(&eval(&args[0], ctx)?))
}

pub fn len(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    Ok(CellValue::Integer(
        one_str("LEN", args, ctx)?.chars().count() as i64,
    ))
}

pub fn upper(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    Ok(CellValue::Text(one_str("UPPER", args, ctx)?.to_uppercase()))
}

pub fn lower(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    Ok(CellValue::Text(one_str("LOWER", args, ctx)?.to_lowercase()))
}

/// Title-case each whitespace-separated word.
pub fn proper(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    let s = one_str("PROPER", args, ctx)?;
    let mut out = String::with_capacity(s.len());
    let mut new_word = true;
    for ch in s.chars() {
        if ch.is_whitespace() {
            out.push(ch);
            new_word = true;
        } else if new_word {
            out.extend(ch.to_uppercase());
            new_word = false;
        } else {
            out.extend(ch.to_lowercase());
        }
    }
    Ok(CellValue::Text(out))
}

pub fn trim(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    let s = one_str("TRIM", args, ctx)?;
    // Excel TRIM collapses internal whitespace runs to a single space.
    let mut out = String::with_capacity(s.len());
    let mut prev_ws = false;
    for ch in s.trim().chars() {
        if ch.is_whitespace() {
            if !prev_ws {
                out.push(' ');
            }
            prev_ws = true;
        } else {
            out.push(ch);
            prev_ws = false;
        }
    }
    Ok(CellValue::Text(out))
}

pub fn left(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_range("LEFT", args, 1, 2)?;
    let s = stringify(&eval(&args[0], ctx)?);
    let n = if let Some(a) = args.get(1) {
        to_int(&eval(a, ctx)?)?.max(0) as usize
    } else {
        1
    };
    Ok(CellValue::Text(s.chars().take(n).collect()))
}

pub fn right(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_range("RIGHT", args, 1, 2)?;
    let s = stringify(&eval(&args[0], ctx)?);
    let n = if let Some(a) = args.get(1) {
        to_int(&eval(a, ctx)?)?.max(0) as usize
    } else {
        1
    };
    let total = s.chars().count();
    let skip = total.saturating_sub(n);
    Ok(CellValue::Text(s.chars().skip(skip).collect()))
}

pub fn mid(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_n("MID", args, 3)?;
    let s = stringify(&eval(&args[0], ctx)?);
    let start = to_int(&eval(&args[1], ctx)?)?;
    let len = to_int(&eval(&args[2], ctx)?)?.max(0) as usize;
    if start < 1 {
        return Err(EvalError::Value("MID: start must be >= 1".into()));
    }
    let skip = (start - 1) as usize;
    Ok(CellValue::Text(s.chars().skip(skip).take(len).collect()))
}

pub fn rept(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_n("REPT", args, 2)?;
    let s = stringify(&eval(&args[0], ctx)?);
    let n = to_int(&eval(&args[1], ctx)?)?.max(0) as usize;
    Ok(CellValue::Text(s.repeat(n)))
}

pub fn exact(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_n("EXACT", args, 2)?;
    let a = stringify(&eval(&args[0], ctx)?);
    let b = stringify(&eval(&args[1], ctx)?);
    Ok(CellValue::Bool(a == b))
}

pub fn substitute(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_range("SUBSTITUTE", args, 3, 4)?;
    let text = stringify(&eval(&args[0], ctx)?);
    let old = stringify(&eval(&args[1], ctx)?);
    let new = stringify(&eval(&args[2], ctx)?);
    if old.is_empty() {
        return Ok(CellValue::Text(text));
    }
    let result = if let Some(nth_arg) = args.get(3) {
        let nth = to_int(&eval(nth_arg, ctx)?)?;
        if nth < 1 {
            return Err(EvalError::Value("SUBSTITUTE: instance must be >= 1".into()));
        }
        replace_nth(&text, &old, &new, nth as usize)
    } else {
        text.replace(&old, &new)
    };
    Ok(CellValue::Text(result))
}

fn replace_nth(text: &str, old: &str, new: &str, nth: usize) -> String {
    let mut count = 0;
    let mut out = String::with_capacity(text.len());
    let mut start = 0;
    for (idx, _) in text.match_indices(old) {
        count += 1;
        if count == nth {
            out.push_str(&text[start..idx]);
            out.push_str(new);
            start = idx + old.len();
            break;
        }
    }
    out.push_str(&text[start..]);
    out
}

/// FIND(needle, haystack, [start_pos]) — 1-indexed position, case-sensitive.
pub fn find(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_range("FIND", args, 2, 3)?;
    let needle = stringify(&eval(&args[0], ctx)?);
    let haystack = stringify(&eval(&args[1], ctx)?);
    let start: usize = if let Some(s) = args.get(2) {
        to_int(&eval(s, ctx)?)?.max(1) as usize
    } else {
        1
    };
    let skip_chars = start - 1;
    let after: String = haystack.chars().skip(skip_chars).collect();
    match after.find(&needle) {
        Some(byte_pos) => {
            let char_pos = after[..byte_pos].chars().count() + skip_chars + 1;
            Ok(CellValue::Integer(char_pos as i64))
        }
        None => Err(EvalError::Value("FIND: not found".into())),
    }
}

/// SEARCH is FIND but case-insensitive.
pub fn search(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_range("SEARCH", args, 2, 3)?;
    let needle = stringify(&eval(&args[0], ctx)?).to_lowercase();
    let haystack = stringify(&eval(&args[1], ctx)?).to_lowercase();
    let start: usize = if let Some(s) = args.get(2) {
        to_int(&eval(s, ctx)?)?.max(1) as usize
    } else {
        1
    };
    let skip_chars = start - 1;
    let after: String = haystack.chars().skip(skip_chars).collect();
    match after.find(&needle) {
        Some(byte_pos) => {
            let char_pos = after[..byte_pos].chars().count() + skip_chars + 1;
            Ok(CellValue::Integer(char_pos as i64))
        }
        None => Err(EvalError::Value("SEARCH: not found".into())),
    }
}

/// REPLACE(text, start_pos, num_chars, new_text).
pub fn replace(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_n("REPLACE", args, 4)?;
    let text: Vec<char> = stringify(&eval(&args[0], ctx)?).chars().collect();
    let start = to_int(&eval(&args[1], ctx)?)?;
    let nchars = to_int(&eval(&args[2], ctx)?)?.max(0) as usize;
    let new = stringify(&eval(&args[3], ctx)?);
    if start < 1 {
        return Err(EvalError::Value("REPLACE: start must be >= 1".into()));
    }
    let skip = (start - 1) as usize;
    let mut out: String = text.iter().take(skip).collect();
    out.push_str(&new);
    out.extend(text.iter().skip(skip + nchars));
    Ok(CellValue::Text(out))
}

/// CONCAT(arg1, arg2, ...) — concatenates arguments, including arrays and ranges.
pub fn concat(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_at_least("CONCAT", args, 1)?;
    let mut out = String::new();
    for a in args {
        for v in flatten(a, ctx)? {
            out.push_str(&stringify(&v));
        }
    }
    Ok(CellValue::Text(out))
}

/// TEXTJOIN(delimiter, ignore_empty, arg1, ...).
pub fn textjoin(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_at_least("TEXTJOIN", args, 3)?;
    let delim = stringify(&eval(&args[0], ctx)?);
    let ignore_empty = super::coerce::to_bool(&eval(&args[1], ctx)?);
    let mut parts = Vec::new();
    for a in &args[2..] {
        for v in flatten(a, ctx)? {
            let s = stringify(&v);
            if ignore_empty && s.is_empty() {
                continue;
            }
            parts.push(s);
        }
    }
    Ok(CellValue::Text(parts.join(&delim)))
}

/// JOIN(delimiter, array) — Sheets-flavoured alias used widely.
pub fn join(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_at_least("JOIN", args, 2)?;
    let delim = stringify(&eval(&args[0], ctx)?);
    let mut parts = Vec::new();
    for a in &args[1..] {
        for v in flatten(a, ctx)? {
            parts.push(stringify(&v));
        }
    }
    Ok(CellValue::Text(parts.join(&delim)))
}

/// SPLIT(text, delimiter) → 1×N array.
pub fn split(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_n("SPLIT", args, 2)?;
    let text = stringify(&eval(&args[0], ctx)?);
    let delim = stringify(&eval(&args[1], ctx)?);
    let parts: Vec<CellValue> = if delim.is_empty() {
        text.chars()
            .map(|c| CellValue::Text(c.to_string()))
            .collect()
    } else {
        text.split(&delim)
            .map(|s| CellValue::Text(s.to_string()))
            .collect()
    };
    Ok(CellValue::Array(Box::new(Array::row(parts))))
}

/// TEXTSPLIT(text, col_delim, [row_delim]) → 2D array if row_delim provided.
pub fn textsplit(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_range("TEXTSPLIT", args, 2, 3)?;
    let text = stringify(&eval(&args[0], ctx)?);
    let col_delim = stringify(&eval(&args[1], ctx)?);
    let row_delim = args
        .get(2)
        .map(|a| eval(a, ctx))
        .transpose()?
        .map(|v| stringify(&v));
    let rows_text: Vec<&str> = if let Some(rd) = &row_delim {
        if rd.is_empty() {
            vec![text.as_str()]
        } else {
            text.split(rd.as_str()).collect()
        }
    } else {
        vec![text.as_str()]
    };
    let mut rows = Vec::with_capacity(rows_text.len());
    let mut cols = 0;
    for row_str in rows_text {
        let parts: Vec<CellValue> = if col_delim.is_empty() {
            vec![CellValue::Text(row_str.to_string())]
        } else {
            row_str
                .split(&col_delim)
                .map(|s| CellValue::Text(s.to_string()))
                .collect()
        };
        cols = cols.max(parts.len());
        rows.push(parts);
    }
    // Pad short rows so the array is rectangular.
    let mut data = Vec::with_capacity(rows.len() * cols);
    for mut row in rows {
        while row.len() < cols {
            row.push(CellValue::Empty);
        }
        data.extend(row);
    }
    Ok(CellValue::Array(Box::new(Array::new(
        rows_text_count(&text, &row_delim),
        cols,
        data,
    ))))
}

fn rows_text_count(text: &str, row_delim: &Option<String>) -> usize {
    match row_delim {
        Some(rd) if !rd.is_empty() => text.split(rd.as_str()).count(),
        _ => 1,
    }
}

/// VALUE(text) → number; reverse of TEXT.
pub fn value_fn(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_n("VALUE", args, 1)?;
    let s = stringify(&eval(&args[0], ctx)?);
    let n = s
        .trim()
        .parse::<f64>()
        .map_err(|_| EvalError::Value(format!("VALUE: {s:?} not numeric")))?;
    Ok(CellValue::Number(n))
}

/// TEXT(value, format) — minimal: supports passing through and basic numeric.
pub fn text_fn(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_n("TEXT", args, 2)?;
    let v = eval(&args[0], ctx)?;
    let _fmt = stringify(&eval(&args[1], ctx)?); // format string ignored in Phase 1.5
                                                 // For now we honour just the basic case: stringify.
    Ok(CellValue::Text(stringify(&v)))
}

pub fn char_fn(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_n("CHAR", args, 1)?;
    let n = to_number(&eval(&args[0], ctx)?)? as u32;
    let ch = char::from_u32(n).ok_or(EvalError::Num)?;
    Ok(CellValue::Text(ch.to_string()))
}

pub fn code_fn(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    arity_n("CODE", args, 1)?;
    let s = stringify(&eval(&args[0], ctx)?);
    let ch = s
        .chars()
        .next()
        .ok_or(EvalError::Value("CODE: empty string".into()))?;
    Ok(CellValue::Integer(ch as i64))
}

pub fn register(r: &mut FunctionRegistry) {
    r.add("LEN", len);
    r.add("UPPER", upper);
    r.add("LOWER", lower);
    r.add("PROPER", proper);
    r.add("TRIM", trim);
    r.add("LEFT", left);
    r.add("RIGHT", right);
    r.add("MID", mid);
    r.add("REPT", rept);
    r.add("EXACT", exact);
    r.add("SUBSTITUTE", substitute);
    r.add("FIND", find);
    r.add("SEARCH", search);
    r.add("REPLACE", replace);
    r.add("CONCAT", concat);
    r.add("CONCATENATE", concat);
    r.add("TEXTJOIN", textjoin);
    r.add("JOIN", join);
    r.add("SPLIT", split);
    r.add("TEXTSPLIT", textsplit);
    r.add("VALUE", value_fn);
    r.add("TEXT", text_fn);
    r.add("CHAR", char_fn);
    r.add("CODE", code_fn);
}
