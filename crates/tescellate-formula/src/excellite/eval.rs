//! Evaluator for Excel-lite. Walks the AST, resolves cell/range references
//! through an `EvalCtx`, dispatches function calls through a small registry.
//!
//! Type coercion mirrors Excel:
//!   - arithmetic: bools → 0/1, numeric strings parsed, empty → 0
//!   - comparison: numbers numerically, strings lexicographically,
//!     mixed types compared by tag order (numbers < text < bools, like Excel)
//!   - concat (`&`): both sides stringified

use super::ast::{BinaryOp, Expr, UnaryOp};
use crate::{EvalCtx, EvalError};
use tescellate_core::{CellError, CellValue};

pub fn eval_error_to_cell_error(e: EvalError) -> CellError {
    match e {
        EvalError::Ref(_) => CellError::Ref,
        EvalError::DivZero => CellError::DivZero,
        EvalError::Num => CellError::Num,
        EvalError::Value(_) | EvalError::BadArity { .. } | EvalError::UnknownFn(_) => {
            CellError::Value
        }
    }
}

pub fn eval(expr: &Expr, ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    match expr {
        Expr::Number(n) => Ok(CellValue::Number(*n)),
        Expr::Str(s) => Ok(CellValue::Text(s.clone())),
        Expr::Bool(b) => Ok(CellValue::Bool(*b)),
        Expr::CellRef(addr) => ctx.cell(addr),
        Expr::Range(_, _) => Err(EvalError::Value(
            "ranges must appear inside a function (e.g. SUM)".into(),
        )),
        Expr::Unary(op, rhs) => {
            let v = eval(rhs, ctx)?;
            let n = to_number(&v)?;
            Ok(CellValue::Number(match op {
                UnaryOp::Neg => -n,
                UnaryOp::Pos => n,
            }))
        }
        Expr::Binary(op, lhs, rhs) => eval_binary(*op, lhs, rhs, ctx),
        Expr::Call(name, args) => eval_call(name, args, ctx),
    }
}

fn eval_binary(
    op: BinaryOp,
    lhs: &Expr,
    rhs: &Expr,
    ctx: &dyn EvalCtx,
) -> Result<CellValue, EvalError> {
    let l = eval(lhs, ctx)?;
    let r = eval(rhs, ctx)?;
    match op {
        BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Pow => {
            let a = to_number(&l)?;
            let b = to_number(&r)?;
            let v = match op {
                BinaryOp::Add => a + b,
                BinaryOp::Sub => a - b,
                BinaryOp::Mul => a * b,
                BinaryOp::Div => {
                    if b == 0.0 {
                        return Err(EvalError::DivZero);
                    }
                    a / b
                }
                BinaryOp::Pow => a.powf(b),
                _ => unreachable!(),
            };
            if v.is_nan() || v.is_infinite() {
                return Err(EvalError::Num);
            }
            Ok(CellValue::Number(v))
        }
        BinaryOp::Concat => Ok(CellValue::Text(format!(
            "{}{}",
            stringify(&l),
            stringify(&r)
        ))),
        BinaryOp::Eq
        | BinaryOp::NotEq
        | BinaryOp::Lt
        | BinaryOp::Gt
        | BinaryOp::LtEq
        | BinaryOp::GtEq => {
            let ord = compare(&l, &r);
            let truth = match op {
                BinaryOp::Eq => ord == std::cmp::Ordering::Equal,
                BinaryOp::NotEq => ord != std::cmp::Ordering::Equal,
                BinaryOp::Lt => ord == std::cmp::Ordering::Less,
                BinaryOp::Gt => ord == std::cmp::Ordering::Greater,
                BinaryOp::LtEq => ord != std::cmp::Ordering::Greater,
                BinaryOp::GtEq => ord != std::cmp::Ordering::Less,
                _ => unreachable!(),
            };
            Ok(CellValue::Bool(truth))
        }
    }
}

fn eval_call(name: &str, args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    match name {
        "SUM" => fold_numeric(args, ctx, 0.0, |acc, n| acc + n).map(CellValue::Number),
        "AVERAGE" => {
            let (sum, count) = sum_and_count(args, ctx)?;
            if count == 0 {
                return Err(EvalError::DivZero);
            }
            Ok(CellValue::Number(sum / count as f64))
        }
        "COUNT" => {
            let (_, count) = sum_and_count(args, ctx)?;
            Ok(CellValue::Number(count as f64))
        }
        "MIN" => fold_numeric(args, ctx, f64::INFINITY, f64::min)
            .map(|v| if v.is_finite() { v } else { 0.0 })
            .map(CellValue::Number),
        "MAX" => fold_numeric(args, ctx, f64::NEG_INFINITY, f64::max)
            .map(|v| if v.is_finite() { v } else { 0.0 })
            .map(CellValue::Number),
        "IF" => {
            if args.len() < 2 || args.len() > 3 {
                return Err(EvalError::BadArity {
                    name: "IF".into(),
                    want: "2..3".into(),
                    got: args.len(),
                });
            }
            let cond = to_bool(&eval(&args[0], ctx)?);
            if cond {
                eval(&args[1], ctx)
            } else if let Some(else_branch) = args.get(2) {
                eval(else_branch, ctx)
            } else {
                Ok(CellValue::Bool(false))
            }
        }
        "AND" => {
            for a in args {
                if !to_bool(&eval(a, ctx)?) {
                    return Ok(CellValue::Bool(false));
                }
            }
            Ok(CellValue::Bool(true))
        }
        "OR" => {
            for a in args {
                if to_bool(&eval(a, ctx)?) {
                    return Ok(CellValue::Bool(true));
                }
            }
            Ok(CellValue::Bool(false))
        }
        "NOT" => {
            if args.len() != 1 {
                return Err(EvalError::BadArity {
                    name: "NOT".into(),
                    want: "1".into(),
                    got: args.len(),
                });
            }
            Ok(CellValue::Bool(!to_bool(&eval(&args[0], ctx)?)))
        }
        "ABS" => {
            if args.len() != 1 {
                return Err(EvalError::BadArity {
                    name: "ABS".into(),
                    want: "1".into(),
                    got: args.len(),
                });
            }
            Ok(CellValue::Number(to_number(&eval(&args[0], ctx)?)?.abs()))
        }
        other => Err(EvalError::UnknownFn(other.into())),
    }
}

/// Iterate the numeric values implied by a list of expressions, expanding
/// ranges in-place and skipping non-numeric values (matches Excel SUM/COUNT
/// semantics where text in a range is silently skipped).
fn for_each_numeric(
    args: &[Expr],
    ctx: &dyn EvalCtx,
    mut f: impl FnMut(f64),
) -> Result<(), EvalError> {
    for arg in args {
        match arg {
            Expr::Range(a, b) => {
                for v in ctx.range(a, b)? {
                    if let Some(n) = as_number(&v) {
                        f(n);
                    }
                }
            }
            _ => {
                let v = eval(arg, ctx)?;
                if let Some(n) = as_number(&v) {
                    f(n);
                }
            }
        }
    }
    Ok(())
}

fn fold_numeric(
    args: &[Expr],
    ctx: &dyn EvalCtx,
    init: f64,
    op: impl Fn(f64, f64) -> f64,
) -> Result<f64, EvalError> {
    let mut acc = init;
    for_each_numeric(args, ctx, |n| acc = op(acc, n))?;
    Ok(acc)
}

fn sum_and_count(args: &[Expr], ctx: &dyn EvalCtx) -> Result<(f64, usize), EvalError> {
    let mut sum = 0.0;
    let mut count = 0;
    for_each_numeric(args, ctx, |n| {
        sum += n;
        count += 1;
    })?;
    Ok((sum, count))
}

fn as_number(v: &CellValue) -> Option<f64> {
    match v {
        CellValue::Number(n) => Some(*n),
        CellValue::Integer(i) => Some(*i as f64),
        CellValue::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        _ => None,
    }
}

fn to_number(v: &CellValue) -> Result<f64, EvalError> {
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
    }
}

fn to_bool(v: &CellValue) -> bool {
    match v {
        CellValue::Bool(b) => *b,
        CellValue::Number(n) => *n != 0.0,
        CellValue::Integer(i) => *i != 0,
        CellValue::Text(s) => !s.is_empty(),
        CellValue::Empty => false,
        _ => false,
    }
}

fn stringify(v: &CellValue) -> String {
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
    }
}

fn format_number(n: f64) -> String {
    if n == n.trunc() && n.abs() < 1e16 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

/// Cross-type comparison, Excel-style: number < text < bool, with each
/// type internally ordered naturally. Used for `<`, `>`, `=`, etc.
fn compare(a: &CellValue, b: &CellValue) -> std::cmp::Ordering {
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
        // Empty compares equal to 0 and to "" for the natural cases.
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
    match (a, b) {
        (CellValue::Number(x), CellValue::Number(y)) => {
            x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal)
        }
        (CellValue::Integer(x), CellValue::Integer(y)) => x.cmp(y),
        (CellValue::Number(x), CellValue::Integer(y)) => x
            .partial_cmp(&(*y as f64))
            .unwrap_or(std::cmp::Ordering::Equal),
        (CellValue::Integer(x), CellValue::Number(y)) => (*x as f64)
            .partial_cmp(y)
            .unwrap_or(std::cmp::Ordering::Equal),
        (CellValue::Text(x), CellValue::Text(y)) => x.cmp(y),
        (CellValue::Bool(x), CellValue::Bool(y)) => x.cmp(y),
        _ => std::cmp::Ordering::Equal,
    }
}

/// Walk the AST and collect every cell and range reference. Used by the
/// orchestrator to populate the DAG before evaluation.
pub fn collect_refs(expr: &Expr, out: &mut Vec<(String, Option<String>)>) {
    match expr {
        Expr::CellRef(a) => out.push((a.clone(), None)),
        Expr::Range(a, b) => out.push((a.clone(), Some(b.clone()))),
        Expr::Unary(_, e) => collect_refs(e, out),
        Expr::Binary(_, l, r) => {
            collect_refs(l, out);
            collect_refs(r, out);
        }
        Expr::Call(_, args) => {
            for a in args {
                collect_refs(a, out);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::excellite::parse::parse;
    use hashbrown::HashMap;

    struct MockCtx {
        cells: HashMap<String, CellValue>,
    }

    impl EvalCtx for MockCtx {
        fn cell(&self, addr: &str) -> Result<CellValue, EvalError> {
            Ok(self
                .cells
                .get(&addr.to_ascii_uppercase())
                .cloned()
                .unwrap_or(CellValue::Empty))
        }
        fn range(&self, start: &str, end: &str) -> Result<Vec<CellValue>, EvalError> {
            // Trivial mock: square-style range expansion over the cells we know.
            let (sc, sr) = split(start);
            let (ec, er) = split(end);
            let mut out = Vec::new();
            for r in sr.min(er)..=sr.max(er) {
                for c in sc.min(ec)..=sc.max(ec) {
                    let addr = format!("{}{r}", (b'A' + c) as char);
                    if let Some(v) = self.cells.get(&addr) {
                        out.push(v.clone());
                    } else {
                        out.push(CellValue::Empty);
                    }
                }
            }
            Ok(out)
        }
    }

    fn split(addr: &str) -> (u8, u32) {
        let bytes = addr.as_bytes();
        let split = bytes.iter().position(|b| b.is_ascii_digit()).unwrap();
        let col = bytes[0] - b'A';
        let row: u32 = std::str::from_utf8(&bytes[split..])
            .unwrap()
            .parse()
            .unwrap();
        (col, row)
    }

    fn mk(pairs: &[(&str, CellValue)]) -> MockCtx {
        MockCtx {
            cells: pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect(),
        }
    }

    fn ev(src: &str, ctx: &MockCtx) -> CellValue {
        eval(&parse(src).unwrap(), ctx).unwrap()
    }

    #[test]
    fn arithmetic() {
        let ctx = mk(&[]);
        assert_eq!(ev("1 + 2 * 3", &ctx), CellValue::Number(7.0));
        assert_eq!(ev("(1 + 2) * 3", &ctx), CellValue::Number(9.0));
        assert_eq!(ev("10 / 4", &ctx), CellValue::Number(2.5));
        assert_eq!(ev("2 ^ 10", &ctx), CellValue::Number(1024.0));
    }

    #[test]
    fn refs_and_sum() {
        let ctx = mk(&[
            ("A1", CellValue::Number(10.0)),
            ("A2", CellValue::Number(20.0)),
            ("A3", CellValue::Number(30.0)),
        ]);
        assert_eq!(ev("A1 + A2", &ctx), CellValue::Number(30.0));
        assert_eq!(ev("SUM(A1:A3)", &ctx), CellValue::Number(60.0));
        assert_eq!(ev("AVERAGE(A1:A3)", &ctx), CellValue::Number(20.0));
        assert_eq!(ev("COUNT(A1:A3)", &ctx), CellValue::Number(3.0));
        assert_eq!(ev("MAX(A1:A3)", &ctx), CellValue::Number(30.0));
        assert_eq!(ev("MIN(A1:A3)", &ctx), CellValue::Number(10.0));
    }

    #[test]
    fn if_branches() {
        let ctx = mk(&[("A1", CellValue::Number(5.0))]);
        assert_eq!(
            ev(r#"IF(A1>0, "pos", "neg")"#, &ctx),
            CellValue::Text("pos".into())
        );
        let ctx = mk(&[("A1", CellValue::Number(-5.0))]);
        assert_eq!(
            ev(r#"IF(A1>0, "pos", "neg")"#, &ctx),
            CellValue::Text("neg".into())
        );
    }

    #[test]
    fn div_zero_is_error() {
        let ctx = mk(&[]);
        assert!(matches!(
            eval(&parse("1/0").unwrap(), &ctx),
            Err(EvalError::DivZero)
        ));
    }

    #[test]
    fn concat() {
        let ctx = mk(&[]);
        assert_eq!(
            ev(r#""hi " & "world""#, &ctx),
            CellValue::Text("hi world".into())
        );
        assert_eq!(ev(r#""x=" & 7"#, &ctx), CellValue::Text("x=7".into()));
    }

    #[test]
    fn collect_refs_finds_all() {
        let mut refs = Vec::new();
        collect_refs(&parse("SUM(A1:B5) + IF(C1>0, D2, E3)").unwrap(), &mut refs);
        assert!(refs.contains(&("A1".to_string(), Some("B5".to_string()))));
        assert!(refs.contains(&("C1".to_string(), None)));
        assert!(refs.contains(&("D2".to_string(), None)));
        assert!(refs.contains(&("E3".to_string(), None)));
    }
}
