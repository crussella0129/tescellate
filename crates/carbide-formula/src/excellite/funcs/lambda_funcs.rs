//! `LAMBDA`, `LET`, and `LETREC` — the binding-and-abstraction core of
//! Carbide. These three `FuncImpl`s receive their args unevaluated (the
//! registry contract) so they can inspect parameter-name shapes and
//! control evaluation order.

use super::FunctionRegistry;
use crate::excellite::ast::Expr;
use crate::excellite::eval::eval;
use crate::excellite::lambda::Lambda;
use crate::{EvalCtx, EvalError, ScopedCtx};
use carbide_core::{CellValue, Env};
use std::collections::HashSet;
use std::sync::Arc;

/// `LAMBDA(p1, p2, ..., body)` — first-class function literal.
///
/// All args except the last are expected to be bare names (`Expr::Var`).
/// The last is the body, kept as `Expr` (not evaluated) — it will be
/// evaluated when the lambda is called.
pub fn lambda(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    if args.is_empty() {
        return Err(EvalError::BadArity {
            name: "LAMBDA".into(),
            want: ">=1".into(),
            got: 0,
        });
    }
    let (param_exprs, body_slice) = args.split_at(args.len() - 1);
    let mut params: Vec<String> = Vec::with_capacity(param_exprs.len());
    let mut seen = HashSet::new();
    for p in param_exprs {
        match p {
            Expr::Var(name) => {
                if !seen.insert(name.clone()) {
                    return Err(EvalError::Value(format!(
                        "LAMBDA: duplicate parameter `{name}`"
                    )));
                }
                params.push(name.clone());
            }
            other => {
                return Err(EvalError::Value(format!(
                    "LAMBDA: parameter must be a name, got {other:?}"
                )));
            }
        }
    }
    let body = body_slice[0].clone();
    let captured = ctx.env().unwrap_or_else(Env::empty_arc);
    Ok(CellValue::Function(Arc::new(Lambda {
        params,
        body,
        captured,
    })))
}

/// `LET(name1, value1, name2, value2, ..., body)` — Excel-sequential
/// binding form. Each subsequent value sees the prior bindings.
pub fn let_fn(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    validate_binding_shape("LET", args)?;
    let scope = Env::child_of(ctx.env().unwrap_or_else(Env::empty_arc));
    let body = args.last().unwrap();
    let pairs = &args[..args.len() - 1];

    let scoped = ScopedCtx {
        parent: ctx,
        scope: scope.clone(),
    };
    for chunk in pairs.chunks(2) {
        let name = expect_var_name("LET", &chunk[0])?;
        let value = eval(&chunk[1], &scoped)?;
        scope.insert(name, value);
    }
    eval(body, &scoped)
}

/// `LETREC(name1, value1, ..., body)` — recursive binding form.
///
/// Phase 1: insert every name as an `Empty` placeholder so lambdas that
/// reference siblings (or themselves) can capture the shared `Arc<Env>`.
/// Phase 2: evaluate each value in that shared scope and replace the
/// placeholder. Lambdas defined here see each other through the shared
/// env at *call* time — the lookup walks the env and finds the patched
/// value.
///
/// Non-lambda values that reference siblings will see the `Empty`
/// placeholder at definition time — this is documented; LETREC is for
/// recursive lambdas, use LET for plain bindings.
pub fn letrec(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
    validate_binding_shape("LETREC", args)?;
    let scope = Env::child_of(ctx.env().unwrap_or_else(Env::empty_arc));
    let body = args.last().unwrap();
    let pairs = &args[..args.len() - 1];

    // Phase 1: collect names + pre-insert placeholders.
    let mut names = Vec::with_capacity(pairs.len() / 2);
    for chunk in pairs.chunks(2) {
        let name = expect_var_name("LETREC", &chunk[0])?;
        scope.insert(name.clone(), CellValue::Empty);
        names.push(name);
    }
    // Phase 2: evaluate values in the shared scope; lambdas that capture
    // this scope (or any ancestor) can later resolve sibling names.
    let scoped = ScopedCtx {
        parent: ctx,
        scope: scope.clone(),
    };
    for (chunk, name) in pairs.chunks(2).zip(names) {
        let value = eval(&chunk[1], &scoped)?;
        scope.insert(name, value);
    }
    eval(body, &scoped)
}

/// Shared shape check: odd arity ≥ 3, alternating (name-expr, value-expr).
fn validate_binding_shape(name: &str, args: &[Expr]) -> Result<(), EvalError> {
    if args.len() < 3 || args.len() % 2 != 1 {
        return Err(EvalError::BadArity {
            name: name.into(),
            want: "odd, >=3 (pairs of name/value, then body)".into(),
            got: args.len(),
        });
    }
    Ok(())
}

fn expect_var_name(fn_name: &str, expr: &Expr) -> Result<String, EvalError> {
    match expr {
        Expr::Var(name) => Ok(name.clone()),
        other => Err(EvalError::Value(format!(
            "{fn_name}: expected a bare name, got {other:?} (use a name like `x`, not a cell ref or literal)"
        ))),
    }
}

pub fn register(r: &mut FunctionRegistry) {
    r.add("LAMBDA", lambda);
    r.add("LET", let_fn);
    r.add("LETREC", letrec);
}

// ===========================================================================
// W8 — Carbide data-science gamut.
//
// Each `#[test]` is a runnable doc example demonstrating one Carbide
// construct. The names align with the rows of the W8 table in the
// approved plan; future Carbide documentation will harvest formulas from
// here.
// ===========================================================================

#[cfg(test)]
mod tests {
    use crate::excellite::eval::eval;
    use crate::excellite::parse::parse;
    use crate::WorkbookEngine;
    use crate::{EvalCtx, EvalError};
    use carbide_core::{CellValue, SheetId};
    use carbide_tess::LatticeKind;
    use hashbrown::HashMap;

    // -- Mock evaluation context for cells-free tests ----------------------

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
            // Square range expansion, row-major. Mirrors the engine's
            // SheetEvalView for purposes of these tests.
            let (sc, sr) = split(start);
            let (ec, er) = split(end);
            let mut out = Vec::new();
            for r in sr.min(er)..=sr.max(er) {
                for c in sc.min(ec)..=sc.max(ec) {
                    let addr = format!("{}{r}", (b'A' + c) as char);
                    out.push(self.cells.get(&addr).cloned().unwrap_or(CellValue::Empty));
                }
            }
            Ok(out)
        }
    }

    fn split(addr: &str) -> (u8, u32) {
        let bytes = addr.as_bytes();
        let i = bytes.iter().position(|b| b.is_ascii_digit()).unwrap();
        (
            bytes[0] - b'A',
            std::str::from_utf8(&bytes[i..]).unwrap().parse().unwrap(),
        )
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
        eval(&parse(src).expect("parse"), ctx).expect("eval")
    }

    /// Engine-backed setup for tests that touch real cell ranges
    /// (`A:A` substitutes, `A1:A10`, etc.).
    fn engine_with_cells(cells: &[(&str, &str)]) -> (WorkbookEngine, SheetId) {
        let mut eng = WorkbookEngine::new();
        eng.new_workbook();
        let sid = eng.add_sheet("Sheet1", LatticeKind::Square);
        for (addr, src) in cells {
            eng.set_cell(sid, addr, Some(src)).unwrap();
        }
        (eng, sid)
    }

    fn close_to(a: f64, b: f64, eps: f64) -> bool {
        (a - b).abs() < eps
    }

    // ---- 1. LET / LAMBDA / LETREC core ----------------------------------

    #[test]
    fn let_basic_binding() {
        let ctx = mk(&[]);
        assert_eq!(ev("LET(x, 10, x+5)", &ctx), CellValue::Number(15.0));
    }

    #[test]
    fn let_sequential_chaining() {
        let ctx = mk(&[]);
        assert_eq!(ev("LET(x, 10, y, x*2, x+y)", &ctx), CellValue::Number(30.0));
    }

    #[test]
    fn lambda_simple_application() {
        let ctx = mk(&[]);
        assert_eq!(ev("(LAMBDA(x, x*2))(5)", &ctx), CellValue::Number(10.0));
    }

    #[test]
    fn let_bound_lambda_callable() {
        let ctx = mk(&[]);
        assert_eq!(
            ev("LET(f, LAMBDA(x, x+1), f(7))", &ctx),
            CellValue::Number(8.0)
        );
    }

    #[test]
    fn lambda_closes_over_env() {
        let ctx = mk(&[]);
        assert_eq!(
            ev("LET(n, 100, f, LAMBDA(x, x+n), f(5))", &ctx),
            CellValue::Number(105.0)
        );
    }

    // ---- 2. Higher-order helpers ----------------------------------------

    #[test]
    fn map_doubles_values() {
        let ctx = mk(&[]);
        let v = ev("MAP([1,2,3,4], LAMBDA(x, x*2))", &ctx);
        if let CellValue::Array(arr) = v {
            assert_eq!(arr.data.len(), 4);
            assert_eq!(arr.data[0], CellValue::Number(2.0));
            assert_eq!(arr.data[3], CellValue::Number(8.0));
        } else {
            panic!("expected array, got {v:?}");
        }
    }

    #[test]
    fn map_pairwise_two_arrays() {
        let ctx = mk(&[]);
        let v = ev("MAP([1,2,3], [10,20,30], LAMBDA(a, b, a+b))", &ctx);
        if let CellValue::Array(arr) = v {
            assert_eq!(
                arr.data,
                vec![
                    CellValue::Number(11.0),
                    CellValue::Number(22.0),
                    CellValue::Number(33.0),
                ]
            );
        } else {
            panic!("expected array");
        }
    }

    #[test]
    fn reduce_sum_of_squares() {
        let ctx = mk(&[]);
        assert_eq!(
            ev("REDUCE(0, [1,2,3,4], LAMBDA(a, x, a+x*x))", &ctx),
            CellValue::Number(30.0)
        );
    }

    #[test]
    fn scan_cumulative_sum() {
        let ctx = mk(&[]);
        if let CellValue::Array(arr) = ev("SCAN(0, [1,2,3,4], LAMBDA(a, x, a+x))", &ctx) {
            assert_eq!(
                arr.data,
                vec![
                    CellValue::Number(1.0),
                    CellValue::Number(3.0),
                    CellValue::Number(6.0),
                    CellValue::Number(10.0),
                ]
            );
        } else {
            panic!("expected array");
        }
    }

    #[test]
    fn byrow_sums() {
        let ctx = mk(&[]);
        if let CellValue::Array(arr) = ev("BYROW([[1,2,3],[4,5,6]], LAMBDA(row, SUM(row)))", &ctx) {
            assert_eq!(arr.shape(), (2, 1));
            assert_eq!(arr.data[0], CellValue::Number(6.0));
            assert_eq!(arr.data[1], CellValue::Number(15.0));
        } else {
            panic!("expected array");
        }
    }

    #[test]
    fn bycol_maxes() {
        let ctx = mk(&[]);
        if let CellValue::Array(arr) = ev("BYCOL([[1,5],[3,2],[4,4]], LAMBDA(col, MAX(col)))", &ctx)
        {
            assert_eq!(arr.shape(), (1, 2));
            assert_eq!(arr.data[0], CellValue::Number(4.0));
            assert_eq!(arr.data[1], CellValue::Number(5.0));
        } else {
            panic!("expected array");
        }
    }

    #[test]
    fn makearray_identity_matrix() {
        let ctx = mk(&[]);
        if let CellValue::Array(arr) = ev("MAKEARRAY(4, 4, LAMBDA(r, c, IF(r=c, 1, 0)))", &ctx) {
            assert_eq!(arr.shape(), (4, 4));
            for r in 0..4 {
                for c in 0..4 {
                    let want = if r == c { 1.0 } else { 0.0 };
                    assert_eq!(arr.get(r, c).unwrap(), &CellValue::Number(want));
                }
            }
        } else {
            panic!("expected array");
        }
    }

    // ---- 3. LETREC recursion --------------------------------------------

    #[test]
    fn letrec_factorial() {
        let ctx = mk(&[]);
        assert_eq!(
            ev(
                "LETREC(fact, LAMBDA(n, IF(n<=1, 1, n*fact(n-1))), fact(6))",
                &ctx
            ),
            CellValue::Number(720.0)
        );
    }

    #[test]
    fn letrec_fibonacci() {
        let ctx = mk(&[]);
        assert_eq!(
            ev(
                "LETREC(fib, LAMBDA(n, IF(n<=1, n, fib(n-1)+fib(n-2))), fib(10))",
                &ctx
            ),
            CellValue::Number(55.0)
        );
    }

    #[test]
    fn letrec_mutual_even_odd() {
        let ctx = mk(&[]);
        let src = "LETREC(\
            even, LAMBDA(n, IF(n=0, TRUE, odd(n-1))), \
            odd,  LAMBDA(n, IF(n=0, FALSE, even(n-1))), \
            even(10))";
        assert_eq!(ev(src, &ctx), CellValue::Bool(true));
    }

    /// Y-combinator factorial — the sharpest test of `Apply`. We don't
    /// rely on LETREC here; recursion is reached purely through
    /// fixed-point application of an explicit Y combinator.
    #[test]
    fn y_combinator_via_apply() {
        let ctx = mk(&[]);
        // Z-combinator (call-by-value fixed point):
        //   Z = λf. (λx. f(λv. x(x)(v))) (λx. f(λv. x(x)(v)))
        // Then  Z(LAMBDA(rec, LAMBDA(n, IF(n<=1, 1, n*rec(n-1)))))(5) == 120.
        let src = "LET(\
            Z, LAMBDA(f, \
                  (LAMBDA(x, f(LAMBDA(v, x(x)(v))))) \
                  (LAMBDA(x, f(LAMBDA(v, x(x)(v)))))), \
            fact, Z(LAMBDA(rec, LAMBDA(n, IF(n<=1, 1, n*rec(n-1))))), \
            fact(5))";
        assert_eq!(ev(src, &ctx), CellValue::Number(120.0));
    }

    // ---- 4. Data-science pipelines (cell-ranges via WorkbookEngine) -----

    #[test]
    fn zscore_normalization() {
        // Set A1..A10 to 1..10 (literals), then Z-score them in one shot.
        let cells: Vec<(String, String)> =
            (1..=10).map(|n| (format!("A{n}"), n.to_string())).collect();
        let pairs: Vec<(&str, &str)> = cells
            .iter()
            .map(|(a, s)| (a.as_str(), s.as_str()))
            .collect();
        let (mut eng, sid) = engine_with_cells(&pairs);
        eng.set_cell(
            sid,
            "B1",
            Some(
                "=LET(\
                    m, AVERAGE(A1:A10), \
                    s, STDEV(A1:A10), \
                    MAP(A1:A10, LAMBDA(x, (x-m)/s)))",
            ),
        )
        .unwrap();
        let cell = eng.get_cell(sid, "B1").unwrap();
        if let CellValue::Array(arr) = cell.value {
            assert_eq!(arr.shape(), (10, 1));
            // First entry: (1 - 5.5) / sqrt(82.5/9) ≈ -1.486_301.
            if let CellValue::Number(z1) = &arr.data[0] {
                assert!(close_to(*z1, -1.486_301, 1e-4), "got z1 = {z1}");
            } else {
                panic!("expected Number");
            }
            // Mean of z-scores is ~0.
            let mean_z: f64 = arr
                .data
                .iter()
                .filter_map(|v| match v {
                    CellValue::Number(n) => Some(*n),
                    _ => None,
                })
                .sum::<f64>()
                / 10.0;
            assert!(close_to(mean_z, 0.0, 1e-9));
        } else {
            panic!("expected array");
        }
    }

    #[test]
    fn groupby_count_via_unique_map_countif() {
        // Labels in A1..A8: a b a c b a c b. UNIQUE → [a,b,c]; counts → [3,3,2].
        let pairs = &[
            ("A1", "a"),
            ("A2", "b"),
            ("A3", "a"),
            ("A4", "c"),
            ("A5", "b"),
            ("A6", "a"),
            ("A7", "c"),
            ("A8", "b"),
        ];
        let (mut eng, sid) = engine_with_cells(pairs);
        eng.set_cell(
            sid,
            "B1",
            Some("=LET(keys, UNIQUE(A1:A8), MAP(keys, LAMBDA(k, COUNTIF(A1:A8, k))))"),
        )
        .unwrap();
        let cell = eng.get_cell(sid, "B1").unwrap();
        if let CellValue::Array(arr) = cell.value {
            assert_eq!(arr.shape(), (3, 1));
            assert_eq!(arr.data[0], CellValue::Integer(3));
            assert_eq!(arr.data[1], CellValue::Integer(3));
            assert_eq!(arr.data[2], CellValue::Integer(2));
        } else {
            panic!("expected array");
        }
    }

    #[test]
    fn filter_then_aggregate() {
        let pairs = &[
            ("A1", "1"),
            ("A2", "-2"),
            ("A3", "3"),
            ("A4", "-4"),
            ("A5", "5"),
        ];
        let (mut eng, sid) = engine_with_cells(pairs);
        // `A1:A5 > 0` doesn't broadcast over ranges yet (a separate
        // workstream); use an explicit MAP-lambda mask, which is also the
        // more general pattern.
        eng.set_cell(
            sid,
            "B1",
            Some(
                "=LET(\
                    mask, MAP(A1:A5, LAMBDA(x, x>0)), \
                    pos, FILTER(A1:A5, mask), \
                    AVERAGE(pos))",
            ),
        )
        .unwrap();
        let cell = eng.get_cell(sid, "B1").unwrap();
        // (1+3+5)/3 = 3
        assert_eq!(cell.value, CellValue::Number(3.0));
    }

    #[test]
    fn data_pipeline_demean_sum() {
        // demean and sum should be ~0 (modulo floating-point noise).
        let cells: Vec<(String, String)> =
            (1..=10).map(|n| (format!("A{n}"), n.to_string())).collect();
        let pairs: Vec<(&str, &str)> = cells
            .iter()
            .map(|(a, s)| (a.as_str(), s.as_str()))
            .collect();
        let (mut eng, sid) = engine_with_cells(&pairs);
        eng.set_cell(
            sid,
            "B1",
            Some(
                "=LET(\
                    m, AVERAGE(A1:A10), \
                    demeaned, MAP(A1:A10, LAMBDA(x, x-m)), \
                    SUM(demeaned))",
            ),
        )
        .unwrap();
        if let CellValue::Number(v) = eng.get_cell(sid, "B1").unwrap().value {
            assert!(close_to(v, 0.0, 1e-9), "expected ~0, got {v}");
        } else {
            panic!("expected number");
        }
    }

    // ---- 5. Stats verification ------------------------------------------

    #[test]
    fn median_matches_percentile_50() {
        let ctx = mk(&[]);
        let med = ev("MEDIAN([1, 2, 3, 4, 5, 6, 7, 8, 9, 10])", &ctx);
        let p50 = ev("PERCENTILE([1, 2, 3, 4, 5, 6, 7, 8, 9, 10], 0.5)", &ctx);
        assert_eq!(med, p50);
    }

    #[test]
    fn quartile_q1_q3() {
        let ctx = mk(&[]);
        // For data 1..=10 with Excel PERCENTILE.INC linear interpolation:
        // Q1 = 3.25, Q3 = 7.75.
        let q1 = ev("QUARTILE([1,2,3,4,5,6,7,8,9,10], 1)", &ctx);
        let q3 = ev("QUARTILE([1,2,3,4,5,6,7,8,9,10], 3)", &ctx);
        if let (CellValue::Number(a), CellValue::Number(b)) = (q1, q3) {
            assert!(close_to(a, 3.25, 1e-9));
            assert!(close_to(b, 7.75, 1e-9));
        } else {
            panic!("expected numbers");
        }
    }

    #[test]
    fn linear_regression_recovers_slope() {
        // y = 2x + 3, perfect fit → SLOPE→2, INTERCEPT→3.
        let xs: Vec<(String, String)> =
            (1..=10).map(|n| (format!("A{n}"), n.to_string())).collect();
        let ys: Vec<(String, String)> = (1..=10)
            .map(|n| (format!("B{n}"), (2 * n + 3).to_string()))
            .collect();
        let pairs: Vec<(&str, &str)> = xs
            .iter()
            .chain(ys.iter())
            .map(|(a, s)| (a.as_str(), s.as_str()))
            .collect();
        let (mut eng, sid) = engine_with_cells(&pairs);
        eng.set_cell(sid, "C1", Some("=SLOPE(B1:B10, A1:A10)"))
            .unwrap();
        eng.set_cell(sid, "C2", Some("=INTERCEPT(B1:B10, A1:A10)"))
            .unwrap();
        eng.set_cell(sid, "C3", Some("=FORECAST(100, B1:B10, A1:A10)"))
            .unwrap();
        if let CellValue::Number(s) = eng.get_cell(sid, "C1").unwrap().value {
            assert!(close_to(s, 2.0, 1e-9));
        } else {
            panic!("expected number for slope");
        }
        if let CellValue::Number(b) = eng.get_cell(sid, "C2").unwrap().value {
            assert!(close_to(b, 3.0, 1e-9));
        } else {
            panic!("expected number for intercept");
        }
        if let CellValue::Number(yhat) = eng.get_cell(sid, "C3").unwrap().value {
            // y(100) = 2*100 + 3 = 203
            assert!(close_to(yhat, 203.0, 1e-9));
        } else {
            panic!("expected number for forecast");
        }
    }

    #[test]
    fn corr_perfect_positive() {
        let ctx = mk(&[]);
        // y = 2x perfectly correlated.
        assert!(matches!(
            ev("CORREL([1,2,3,4,5,6,7,8,9,10], [2,4,6,8,10,12,14,16,18,20])", &ctx),
            CellValue::Number(v) if close_to(v, 1.0, 1e-12)
        ));
    }

    // ---- 6. Error paths --------------------------------------------------

    #[test]
    fn unbound_variable_errors_loudly() {
        let ctx = mk(&[]);
        let err = eval(&parse("LET(x, 10, y+5)").unwrap(), &ctx).unwrap_err();
        match err {
            EvalError::Ref(msg) => {
                assert!(
                    msg.contains("unbound") && msg.to_uppercase().contains("Y"),
                    "expected unbound-Y ref error, got {msg}"
                );
            }
            other => panic!("expected #REF! unbound, got {other:?}"),
        }
    }

    #[test]
    fn lambda_param_collision_rejected() {
        let ctx = mk(&[]);
        let err = eval(&parse("LAMBDA(x, x, x+x)").unwrap(), &ctx).unwrap_err();
        match err {
            EvalError::Value(msg) => assert!(msg.contains("duplicate")),
            other => panic!("expected duplicate-param error, got {other:?}"),
        }
    }

    #[test]
    fn lambda_non_var_param_rejected() {
        let ctx = mk(&[]);
        // `LAMBDA(42, x*2)` — first arg is a literal, not a name.
        let err = eval(&parse("LAMBDA(42, x*2)").unwrap(), &ctx).unwrap_err();
        match err {
            EvalError::Value(msg) => assert!(msg.contains("parameter must be a name")),
            other => panic!("expected non-Var-param error, got {other:?}"),
        }
    }

    #[test]
    fn letrec_non_lambda_recursive_value_uses_placeholder() {
        // `LETREC(a, b+1, b, 5, a)` — `a`'s value is evaluated when `b` is
        // still the Empty placeholder, so b coerces to 0, a := 1.
        // (LET would behave identically; this test documents that LETREC
        // doesn't magically make non-lambda forward refs work.)
        let ctx = mk(&[]);
        assert_eq!(ev("LETREC(a, b+1, b, 5, a)", &ctx), CellValue::Number(1.0));
    }
}
