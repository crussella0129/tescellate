//! Evaluator for Excel-lite. Walks the AST, resolves cell/range references
//! through an `EvalCtx`, dispatches function calls through the `funcs::standard`
//! registry. Type coercion helpers live in `funcs::coerce`.

use super::ast::{BinaryOp, Expr, UnaryOp};
use super::funcs::{coerce::*, standard};
use crate::{EvalCtx, EvalError, FormulaRef};
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
        Expr::Range(_, _) => bare_range(),
        Expr::Array(rows) => eval_array_literal(rows, ctx),
        Expr::Unary(op, rhs) => {
            let v = eval(rhs, ctx)?;
            apply_unary_op(*op, v)
        }
        Expr::Binary(op, lhs, rhs) => eval_binary(*op, lhs, rhs, ctx),
        Expr::Call(name, args) => {
            // Registry lookup first; if the name isn't a registered
            // FuncImpl, fall back to looking it up as a lexical variable
            // — if that's a Function, treat as Apply. This is what makes
            // `f(5)` work whether `f` is `SUM` or a LET-bound lambda.
            match standard().call(name, args, ctx) {
                Err(EvalError::UnknownFn(_)) => match ctx.var(name) {
                    Some(CellValue::Function(_)) => {
                        let callee = Expr::Var(name.clone());
                        eval(&Expr::Apply(Box::new(callee), args.clone()), ctx)
                    }
                    _ => Err(EvalError::UnknownFn(name.clone())),
                },
                other => other,
            }
        }
        Expr::Var(name) => ctx
            .var(name)
            .ok_or_else(|| EvalError::Ref(format!("unbound: {name}"))),
        Expr::Apply(callee, args) => {
            let callee_v = eval(callee, ctx)?;
            let arg_vs: Vec<CellValue> = args
                .iter()
                .map(|a| eval(a, ctx))
                .collect::<Result<_, _>>()?;
            apply_lambda(callee_v, arg_vs, ctx)
        }
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
    apply_binary_op(op, l, r)
}

/// The value of a bare range expression: there is none. Ranges are only
/// meaningful as function arguments. Shared by the interpreter and
/// transpiled code so the two produce an identical error.
pub fn bare_range() -> Result<CellValue, EvalError> {
    Err(EvalError::Value(
        "ranges must appear inside a function (e.g. SUM)".into(),
    ))
}

/// Apply an already-evaluated callee value to already-evaluated argument
/// values. Shared by the interpreter's `Apply` arm and transpiled code
/// (`crate::transpile`), so an immediately-invoked lambda runs the same way
/// whichever path reaches it.
pub fn apply_lambda(
    callee: CellValue,
    args: Vec<CellValue>,
    ctx: &dyn EvalCtx,
) -> Result<CellValue, EvalError> {
    match callee {
        CellValue::Function(arc) => arc
            .as_any()
            .downcast_ref::<crate::excellite::lambda::Lambda>()
            .ok_or_else(|| EvalError::Value("cannot apply this function value here".into()))?
            .call(args, ctx),
        other => Err(EvalError::Value(format!("not a function: {other:?}"))),
    }
}

/// Apply a unary operator to an already-evaluated value.
///
/// Shared by the tree-walking interpreter (`eval`) and transpiled code
/// (`crate::transpile`), so the two paths cannot diverge — there is
/// exactly one implementation of the operator semantics.
pub fn apply_unary_op(op: UnaryOp, v: CellValue) -> Result<CellValue, EvalError> {
    let n = to_number(&v)?;
    Ok(CellValue::Number(match op {
        UnaryOp::Neg => -n,
        UnaryOp::Pos => n,
    }))
}

/// Apply a binary operator to two already-evaluated values. Shared by the
/// interpreter and transpiled code — see `apply_unary_op`.
pub fn apply_binary_op(op: BinaryOp, l: CellValue, r: CellValue) -> Result<CellValue, EvalError> {
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

/// Walk the AST and collect every statically-knowable cell, range,
/// NEIGHBORS, and RADIUS reference. Used by the orchestrator to populate
/// the DAG before evaluation.
///
/// `NEIGHBORS(<CellRef>)` and `RADIUS(<CellRef>, <int>)` emit dedicated
/// `FormulaRef` variants so the engine layer (which knows the lattice)
/// can expand them into concrete neighbor / disc cells.
///
/// Dynamic forms — `NEIGHBORS("A1")`, `NEIGHBORS(x)` where `x` is a
/// lambda variable — fall through to the plain argument walk and don't
/// produce a `FormulaRef`. They still evaluate correctly at runtime,
/// but the DAG won't propagate through them when the underlying cells
/// change. This is the documented limit on dynamic addressing.
pub fn collect_refs(expr: &Expr, out: &mut Vec<FormulaRef>) {
    match expr {
        Expr::CellRef(a) => out.push(FormulaRef::Cell(a.clone())),
        Expr::Range(a, b) => out.push(FormulaRef::Range(a.clone(), b.clone())),
        Expr::Unary(_, e) => collect_refs(e, out),
        Expr::Binary(_, l, r) => {
            collect_refs(l, out);
            collect_refs(r, out);
        }
        Expr::Call(name, args) => {
            // Lattice-aware calls: emit a dedicated FormulaRef so the
            // engine layer can expand the neighborhood through the
            // lattice. Fall through to the generic arg walk for the
            // dynamic-address forms so other refs inside still register.
            if name == "NEIGHBORS" && args.len() == 1 {
                if let Expr::CellRef(addr) = &args[0] {
                    out.push(FormulaRef::Neighbors(addr.clone()));
                    return;
                }
            }
            if name == "RADIUS" && (args.len() == 1 || args.len() == 2) {
                if let Expr::CellRef(addr) = &args[0] {
                    let n = match args.get(1) {
                        None => 1,
                        Some(Expr::Number(n)) if n.fract() == 0.0 => *n as i64,
                        Some(Expr::Unary(UnaryOp::Neg, inner)) => match inner.as_ref() {
                            Expr::Number(n) if n.fract() == 0.0 => -(*n as i64),
                            _ => {
                                // Dynamic n; can't expand statically.
                                for a in args {
                                    collect_refs(a, out);
                                }
                                return;
                            }
                        },
                        _ => {
                            for a in args {
                                collect_refs(a, out);
                            }
                            return;
                        }
                    };
                    out.push(FormulaRef::Radius(addr.clone(), n));
                    return;
                }
            }
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
        Expr::Apply(callee, args) => {
            collect_refs(callee, out);
            for a in args {
                collect_refs(a, out);
            }
        }
        // Var carries a lexical-environment lookup, not a cell reference.
        // Lambda parameter names and LET-bound names live here.
        Expr::Var(_) => {}
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
        assert!(refs.contains(&FormulaRef::Range("A1".into(), "B5".into())));
        assert!(refs.contains(&FormulaRef::Cell("C1".into())));
        assert!(refs.contains(&FormulaRef::Cell("D2".into())));
        assert!(refs.contains(&FormulaRef::Cell("E3".into())));
    }

    #[test]
    fn collect_refs_finds_neighbors() {
        let mut refs = Vec::new();
        collect_refs(
            &parse("NEIGHBORS(A1) + NEIGHBORS(H(2,3))").unwrap(),
            &mut refs,
        );
        assert!(refs.contains(&FormulaRef::Neighbors("A1".into())));
        assert!(refs.contains(&FormulaRef::Neighbors("H(2,3)".into())));
    }

    #[test]
    fn collect_refs_finds_radius() {
        let mut refs = Vec::new();
        collect_refs(
            &parse("RADIUS(A1, 2) + RADIUS(H(0,0), 3)").unwrap(),
            &mut refs,
        );
        assert!(refs.contains(&FormulaRef::Radius("A1".into(), 2)));
        assert!(refs.contains(&FormulaRef::Radius("H(0,0)".into(), 3)));
    }

    #[test]
    fn collect_refs_radius_default_arg_is_one() {
        let mut refs = Vec::new();
        collect_refs(&parse("RADIUS(A1)").unwrap(), &mut refs);
        assert!(refs.contains(&FormulaRef::Radius("A1".into(), 1)));
    }

    #[test]
    fn collect_refs_skips_dynamic_neighbors_arg() {
        // NEIGHBORS("A1") — text arg — can't be statically expanded.
        // The arg itself contains no CellRef, so no FormulaRef is emitted.
        let mut refs = Vec::new();
        collect_refs(&parse(r#"NEIGHBORS("A1")"#).unwrap(), &mut refs);
        assert!(refs.is_empty());
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
