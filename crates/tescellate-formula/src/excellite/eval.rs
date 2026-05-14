//! Evaluator for Excel-lite. Walks the AST, resolves cell/range references
//! through an `EvalCtx`, dispatches function calls through the `funcs::standard`
//! registry. Type coercion helpers live in `funcs::coerce`.

use super::ast::{BinaryOp, Expr, UnaryOp};
use super::funcs::{coerce::*, standard};
use crate::{EvalCtx, EvalError};
use tescellate_core::{Array, CellError, CellValue};

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

fn eval_array_literal(rows: &[Vec<Expr>], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    if rows.is_empty() {
        return Ok(CellValue::Array(Box::new(Array::new(0, 0, Vec::new()))));
    }
    let nrows = rows.len();
    let ncols = rows[0].len();
    let mut data = Vec::with_capacity(nrows * ncols);
    for row in rows {
        for e in row {
            data.push(eval(e, ctx)?);
        }
    }
    Ok(CellValue::Array(Box::new(Array::new(nrows, ncols, data))))
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
        Expr::Array(rows) => eval_array_literal(rows, ctx),
        Expr::Unary(op, rhs) => {
            let v = eval(rhs, ctx)?;
            let n = to_number(&v)?;
            Ok(CellValue::Number(match op {
                UnaryOp::Neg => -n,
                UnaryOp::Pos => n,
            }))
        }
        Expr::Binary(op, lhs, rhs) => eval_binary(*op, lhs, rhs, ctx),
        Expr::Call(name, args) => standard().call(name, args, ctx),
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
        Expr::Array(rows) => {
            for row in rows {
                for e in row {
                    collect_refs(e, out);
                }
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
        assert_eq!(ev("COUNT(A1:A3)", &ctx), CellValue::Integer(3));
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

    // Function library smoke tests.
    #[test]
    fn text_functions() {
        let ctx = mk(&[]);
        assert_eq!(ev(r#"LEN("hello")"#, &ctx), CellValue::Integer(5));
        assert_eq!(ev(r#"UPPER("hi")"#, &ctx), CellValue::Text("HI".into()));
        assert_eq!(ev(r#"LOWER("HI")"#, &ctx), CellValue::Text("hi".into()));
        assert_eq!(
            ev(r#"PROPER("hello world")"#, &ctx),
            CellValue::Text("Hello World".into())
        );
        assert_eq!(
            ev(r#"LEFT("hello", 3)"#, &ctx),
            CellValue::Text("hel".into())
        );
        assert_eq!(
            ev(r#"RIGHT("hello", 3)"#, &ctx),
            CellValue::Text("llo".into())
        );
        assert_eq!(
            ev(r#"MID("hello", 2, 3)"#, &ctx),
            CellValue::Text("ell".into())
        );
        assert_eq!(
            ev(r#"TRIM("  a   b  c  ")"#, &ctx),
            CellValue::Text("a b c".into())
        );
        assert_eq!(
            ev(r#"SUBSTITUTE("a-b-c", "-", "/")"#, &ctx),
            CellValue::Text("a/b/c".into())
        );
        assert_eq!(ev(r#"FIND("b", "abc")"#, &ctx), CellValue::Integer(2));
    }

    #[test]
    fn join_split_unique() {
        let ctx = mk(&[]);
        assert_eq!(
            ev(r#"JOIN("-", [1, 2, 3])"#, &ctx),
            CellValue::Text("1-2-3".into())
        );
        assert_eq!(
            ev(r#"TEXTJOIN(", ", TRUE, "a", "", "b", "c")"#, &ctx),
            CellValue::Text("a, b, c".into())
        );
        // SPLIT: delimiter first, then text (symmetric with JOIN).
        if let CellValue::Array(arr) = ev(r#"SPLIT("-", "a-b-c")"#, &ctx) {
            assert_eq!(arr.cols, 3);
            assert_eq!(arr.data[0], CellValue::Text("a".into()));
        } else {
            panic!("expected array");
        }
        // SPLIT across multiple cells returns a 2D array.
        if let CellValue::Array(arr) = ev(r#"SPLIT(",", ["a,b", "c,d,e"])"#, &ctx) {
            assert_eq!(arr.rows, 2);
            assert_eq!(arr.cols, 3);
            assert_eq!(arr.data[2], CellValue::Empty); // a,b padded
        } else {
            panic!("expected 2D array");
        }
        // UNIQUE filters duplicates.
        if let CellValue::Array(arr) = ev("UNIQUE([1, 2, 2, 3, 1])", &ctx) {
            assert_eq!(arr.rows, 3);
        } else {
            panic!("expected array");
        }
    }

    #[test]
    fn math_functions() {
        let ctx = mk(&[]);
        // 3.14 is what ROUND(π_approx, 2) returns; clippy gripes about the
        // literal "looking like" a constant — silence it locally.
        #[allow(clippy::approx_constant)]
        let want = 3.14;
        assert_eq!(ev("ROUND(3.14159, 2)", &ctx), CellValue::Number(want));
        assert_eq!(ev("MOD(10, 3)", &ctx), CellValue::Number(1.0));
        assert_eq!(ev("POWER(2, 8)", &ctx), CellValue::Number(256.0));
        assert_eq!(ev("SQRT(16)", &ctx), CellValue::Number(4.0));
        assert_eq!(ev("INT(3.7)", &ctx), CellValue::Number(3.0));
        assert_eq!(ev("ABS(-5)", &ctx), CellValue::Number(5.0));
    }

    #[test]
    fn lookup_functions() {
        let ctx = mk(&[]);
        // INDEX into a 2D array literal.
        assert_eq!(
            ev("INDEX([[10, 20], [30, 40]], 2, 1)", &ctx),
            CellValue::Number(30.0)
        );
        // MATCH on an array.
        assert_eq!(
            ev(r#"MATCH("b", ["a", "b", "c"])"#, &ctx),
            CellValue::Integer(2)
        );
        // VLOOKUP on a 2D array.
        assert_eq!(
            ev(r#"VLOOKUP("b", [["a", 1], ["b", 2], ["c", 3]], 2)"#, &ctx),
            CellValue::Number(2.0)
        );
    }

    #[test]
    fn dyn_array_functions() {
        let ctx = mk(&[]);
        // SEQUENCE generates 1..=5.
        if let CellValue::Array(arr) = ev("SEQUENCE(5)", &ctx) {
            assert_eq!(arr.rows, 5);
            assert_eq!(arr.cols, 1);
            assert_eq!(arr.data[0], CellValue::Number(1.0));
            assert_eq!(arr.data[4], CellValue::Number(5.0));
        } else {
            panic!("expected array");
        }
        // FILTER keeps elements where mask is true.
        if let CellValue::Array(arr) = ev("FILTER([1, 2, 3, 4], [TRUE, FALSE, TRUE, FALSE])", &ctx)
        {
            assert_eq!(arr.rows, 2);
            assert_eq!(arr.data[0], CellValue::Number(1.0));
            assert_eq!(arr.data[1], CellValue::Number(3.0));
        } else {
            panic!("expected array");
        }
    }

    #[test]
    fn logical_functions() {
        let ctx = mk(&[("A1", CellValue::Error(CellError::DivZero))]);
        assert_eq!(
            ev(r#"IFERROR(A1, "default")"#, &ctx),
            CellValue::Text("default".into())
        );
        assert_eq!(ev("ISBLANK(B1)", &ctx), CellValue::Bool(true));
        assert_eq!(ev("ISNUMBER(42)", &ctx), CellValue::Bool(true));
        assert_eq!(ev(r#"ISTEXT("hi")"#, &ctx), CellValue::Bool(true));
        assert_eq!(
            ev(r#"IFS(FALSE, "a", TRUE, "b")"#, &ctx),
            CellValue::Text("b".into())
        );
    }

    #[test]
    fn array_literals_as_input() {
        let ctx = mk(&[]);
        assert_eq!(ev("SUM([1, 2, 3])", &ctx), CellValue::Number(6.0));
        assert_eq!(ev("AVERAGE([10, 20, 30])", &ctx), CellValue::Number(20.0));
        assert_eq!(ev("MAX([3, 1, 4, 1, 5, 9])", &ctx), CellValue::Number(9.0));
    }

    #[test]
    fn cell_list_array() {
        let ctx = mk(&[
            ("A1", CellValue::Number(10.0)),
            ("B3", CellValue::Number(20.0)),
            ("C5", CellValue::Number(30.0)),
        ]);
        assert_eq!(ev("SUM([A1, B3, C5])", &ctx), CellValue::Number(60.0));
    }
}
