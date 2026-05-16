//! Carbide → Rust transpiler. See `docs/all-rust-roadmap.md` (Track B).
//!
//! Lowers a Carbide `Expr` to Rust source that, once compiled, evaluates to
//! the same `CellValue` as the tree-walking interpreter. Transpiled code
//! calls the very same value-level primitives the interpreter uses
//! (`apply_binary_op`, `apply_unary_op`, `bare_range`, `apply_lambda`),
//! reads cells and variables through the same `EvalCtx` trait, and
//! dispatches function calls into the same `excellite::funcs::standard()`
//! registry — so equivalence holds *by construction*: there is one
//! implementation of the semantics, not two.
//!
//! As of v4 every `Expr` variant is lowered — transpilation is **total**,
//! so `transpile_expr` is infallible and returns a plain `String`.

pub mod rt;

use std::collections::HashMap;

use crate::excellite::ast::{BinaryOp, Expr, UnaryOp};
use crate::{EvalCtx, EvalError};
use tescellate_core::CellValue;

/// Lower `expr` to a Rust expression of type `CellValue`.
///
/// The result is valid inside a function body that returns
/// `Result<CellValue, EvalError>` and has a `ctx: &dyn EvalCtx` in scope
/// (see [`emit_formula_fn`]): operator, cell-read, variable, call, and
/// apply lowerings end in `?`, so an evaluation error propagates exactly
/// as it does in the interpreter.
pub fn transpile_expr(expr: &Expr) -> String {
    match expr {
        Expr::Number(n) => format!("CellValue::Number({n:?}f64)"),
        Expr::Str(s) => format!("CellValue::Text({s:?}.to_string())"),
        Expr::Bool(b) => format!("CellValue::Bool({b})"),
        Expr::CellRef(addr) => format!("ctx.cell({addr:?})?"),
        Expr::Range(_, _) => "bare_range()?".to_string(),
        Expr::Array(rows) => transpile_array(rows),
        Expr::Unary(op, inner) => format!(
            "apply_unary_op({}, {})?",
            unary_op_path(*op),
            transpile_expr(inner),
        ),
        Expr::Binary(op, lhs, rhs) => format!(
            "apply_binary_op({}, {}, {})?",
            binary_op_path(*op),
            transpile_expr(lhs),
            transpile_expr(rhs),
        ),
        // A call dispatches into the shared `standard()` registry. Its
        // arguments are passed as reconstructed AST (`emit_expr_literal`),
        // not pre-evaluated values, so registry functions keep their lazy
        // argument semantics — `IF` / `AND` short-circuiting still works,
        // and `LET` / `LAMBDA` / `LETREC` evaluate their bodies themselves.
        Expr::Call(name, args) => {
            format!(
                "standard().call({name:?}, &[{}], ctx)?",
                emit_expr_list(args)
            )
        }
        // A bare variable resolves against the lexical environment. At a
        // cell's top level there is no scope, so this is an unbound error
        // — exactly what the interpreter produces.
        Expr::Var(name) => {
            let msg = format!("unbound: {name}");
            format!("ctx.var({name:?}).ok_or_else(|| EvalError::Ref({msg:?}.to_string()))?")
        }
        // Application of a callee value (e.g. an immediately-invoked
        // lambda): evaluate the callee, then the arguments, then dispatch
        // through the shared `apply_lambda` primitive.
        Expr::Apply(callee, args) => {
            let arg_srcs: Vec<String> = args.iter().map(transpile_expr).collect();
            format!(
                "apply_lambda({}, vec![{}], ctx)?",
                transpile_expr(callee),
                arg_srcs.join(", "),
            )
        }
    }
}

/// Lower an array literal. Mirrors `excellite::eval::eval_array_literal`:
/// row-major flatten, `cols` taken from the first row — the parser already
/// rejects ragged arrays, so every row has the same length.
fn transpile_array(rows: &[Vec<Expr>]) -> String {
    if rows.is_empty() {
        return "CellValue::Array(Box::new(Array::new(0, 0, Vec::new())))".to_string();
    }
    let nrows = rows.len();
    let ncols = rows[0].len();
    let mut elems = Vec::with_capacity(nrows * ncols);
    for row in rows {
        for e in row {
            elems.push(transpile_expr(e));
        }
    }
    format!(
        "CellValue::Array(Box::new(Array::new({nrows}, {ncols}, vec![{}])))",
        elems.join(", "),
    )
}

/// Serialize `expr` back to Rust source that *constructs* the equivalent
/// `Expr` value. Used to hand a `Call`'s argument ASTs to the function
/// registry, which evaluates them itself.
fn emit_expr_literal(expr: &Expr) -> String {
    match expr {
        Expr::Number(n) => format!("Expr::Number({n:?}f64)"),
        Expr::Str(s) => format!("Expr::Str({s:?}.to_string())"),
        Expr::Bool(b) => format!("Expr::Bool({b})"),
        Expr::CellRef(a) => format!("Expr::CellRef({a:?}.to_string())"),
        Expr::Range(a, b) => {
            format!("Expr::Range({a:?}.to_string(), {b:?}.to_string())")
        }
        Expr::Array(rows) => {
            let rows_src: Vec<String> = rows
                .iter()
                .map(|row| format!("vec![{}]", emit_expr_list(row)))
                .collect();
            format!("Expr::Array(vec![{}])", rows_src.join(", "))
        }
        Expr::Var(n) => format!("Expr::Var({n:?}.to_string())"),
        Expr::Unary(op, inner) => format!(
            "Expr::Unary({}, Box::new({}))",
            unary_op_path(*op),
            emit_expr_literal(inner),
        ),
        Expr::Binary(op, lhs, rhs) => format!(
            "Expr::Binary({}, Box::new({}), Box::new({}))",
            binary_op_path(*op),
            emit_expr_literal(lhs),
            emit_expr_literal(rhs),
        ),
        Expr::Apply(callee, args) => format!(
            "Expr::Apply(Box::new({}), vec![{}])",
            emit_expr_literal(callee),
            emit_expr_list(args),
        ),
        Expr::Call(name, args) => {
            format!(
                "Expr::Call({name:?}.to_string(), vec![{}])",
                emit_expr_list(args)
            )
        }
    }
}

/// Comma-join the reconstructed AST source of each expression in `exprs`.
fn emit_expr_list(exprs: &[Expr]) -> String {
    exprs
        .iter()
        .map(emit_expr_literal)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Emit a complete named Rust function that evaluates `expr`. The function
/// takes `ctx: &dyn EvalCtx` — the same trait the interpreter reads cells
/// through — and is the unit the differential harness batches into a crate.
pub fn emit_formula_fn(name: &str, expr: &Expr) -> String {
    let body = transpile_expr(expr);
    format!(
        "pub fn {name}(ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {{\n    Ok({body})\n}}\n"
    )
}

fn unary_op_path(op: UnaryOp) -> &'static str {
    match op {
        UnaryOp::Neg => "UnaryOp::Neg",
        UnaryOp::Pos => "UnaryOp::Pos",
    }
}

fn binary_op_path(op: BinaryOp) -> &'static str {
    match op {
        BinaryOp::Add => "BinaryOp::Add",
        BinaryOp::Sub => "BinaryOp::Sub",
        BinaryOp::Mul => "BinaryOp::Mul",
        BinaryOp::Div => "BinaryOp::Div",
        BinaryOp::Pow => "BinaryOp::Pow",
        BinaryOp::Concat => "BinaryOp::Concat",
        BinaryOp::Eq => "BinaryOp::Eq",
        BinaryOp::NotEq => "BinaryOp::NotEq",
        BinaryOp::Lt => "BinaryOp::Lt",
        BinaryOp::Gt => "BinaryOp::Gt",
        BinaryOp::LtEq => "BinaryOp::LtEq",
        BinaryOp::GtEq => "BinaryOp::GtEq",
    }
}

/// A minimal in-memory [`EvalCtx`] backed by a cell map. Gives transpiled
/// and interpreted code an identical view of the sheet — the differential
/// harness builds one of these on each side of the comparison — and is a
/// building block for headless evaluation of transpiled formulas.
#[derive(Debug, Default, Clone)]
pub struct MapCtx {
    cells: HashMap<String, CellValue>,
}

impl MapCtx {
    /// Build a context from `(address, value)` pairs. Addresses are
    /// upper-cased, matching the lexer's canonical form.
    pub fn from_pairs(pairs: &[(&str, CellValue)]) -> Self {
        let cells = pairs
            .iter()
            .map(|(addr, value)| (addr.to_ascii_uppercase(), value.clone()))
            .collect();
        Self { cells }
    }
}

impl EvalCtx for MapCtx {
    fn cell(&self, addr: &str) -> Result<CellValue, EvalError> {
        Ok(self
            .cells
            .get(&addr.to_ascii_uppercase())
            .cloned()
            .unwrap_or_default())
    }

    /// Rectangular range over single-letter columns — enough for the
    /// differential corpus and headless use; multi-letter columns are
    /// out of `MapCtx`'s scope.
    fn range(&self, start: &str, end: &str) -> Result<Vec<CellValue>, EvalError> {
        let (sc, sr) = parse_a1(start)?;
        let (ec, er) = parse_a1(end)?;
        let mut out = Vec::new();
        for row in sr.min(er)..=sr.max(er) {
            for col in sc.min(ec)..=sc.max(ec) {
                let addr = format!("{}{row}", (b'A' + col) as char);
                out.push(self.cells.get(&addr).cloned().unwrap_or_default());
            }
        }
        Ok(out)
    }
}

fn addr_err(addr: &str) -> EvalError {
    EvalError::Ref(format!("MapCtx: unsupported address `{addr}`"))
}

/// Parse a single-letter A1 address (`"B7"`) into `(col, row)`.
fn parse_a1(addr: &str) -> Result<(u8, u32), EvalError> {
    match addr.bytes().next() {
        Some(c) if c.is_ascii_alphabetic() => {
            let col = c.to_ascii_uppercase() - b'A';
            match addr[1..].parse::<u32>() {
                Ok(row) => Ok((col, row)),
                Err(_) => Err(addr_err(addr)),
            }
        }
        _ => Err(addr_err(addr)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::excellite::parse::parse;

    fn t(src: &str) -> String {
        transpile_expr(&parse(src).unwrap())
    }

    #[test]
    fn literals() {
        assert_eq!(t("1"), "CellValue::Number(1.0f64)");
        assert_eq!(t("2.5"), "CellValue::Number(2.5f64)");
        assert_eq!(t(r#""hi""#), r#"CellValue::Text("hi".to_string())"#);
        assert_eq!(t("TRUE"), "CellValue::Bool(true)");
        assert_eq!(t("FALSE"), "CellValue::Bool(false)");
    }

    #[test]
    fn unary_negation() {
        assert_eq!(
            t("-5"),
            "apply_unary_op(UnaryOp::Neg, CellValue::Number(5.0f64))?"
        );
    }

    #[test]
    fn binary_nests_by_parse_precedence() {
        let s = t("1 + 2 * 3");
        assert!(s.starts_with("apply_binary_op(BinaryOp::Add, CellValue::Number(1.0f64), "));
        assert!(s.contains(
            "apply_binary_op(BinaryOp::Mul, CellValue::Number(2.0f64), \
             CellValue::Number(3.0f64))?"
        ));
        assert!(s.ends_with(")?"));
    }

    #[test]
    fn string_literals_are_escaped() {
        let e = Expr::Str("a\"b".to_string());
        assert_eq!(transpile_expr(&e), r#"CellValue::Text("a\"b".to_string())"#);
    }

    #[test]
    fn cell_ref_reads_through_ctx() {
        assert_eq!(t("A1"), r#"ctx.cell("A1")?"#);
    }

    #[test]
    fn bare_range_lowers_to_shared_error() {
        assert_eq!(t("A1:A3"), "bare_range()?");
    }

    #[test]
    fn array_literal_1d() {
        let s = t("[1, 2, 3]");
        assert!(s.starts_with("CellValue::Array(Box::new(Array::new(1, 3, vec!["));
        assert!(s.contains("CellValue::Number(1.0f64)"));
        assert!(s.contains("CellValue::Number(3.0f64)"));
        assert!(s.ends_with("])))"));
    }

    #[test]
    fn array_literal_2d() {
        let s = t("[[1, 2], [3, 4]]");
        assert!(s.starts_with("CellValue::Array(Box::new(Array::new(2, 2, vec!["));
        assert!(s.contains("CellValue::Number(4.0f64)"));
    }

    #[test]
    fn empty_array() {
        assert_eq!(
            t("[]"),
            "CellValue::Array(Box::new(Array::new(0, 0, Vec::new())))"
        );
    }

    #[test]
    fn array_of_cell_refs() {
        let s = t("[A1, A2]");
        assert!(s.contains(r#"ctx.cell("A1")?"#));
        assert!(s.contains(r#"ctx.cell("A2")?"#));
    }

    #[test]
    fn function_call_dispatches_to_registry() {
        let s = t("SUM(A1, A2)");
        assert!(s.starts_with(r#"standard().call("SUM", &["#));
        assert!(s.contains(r#"Expr::CellRef("A1".to_string())"#));
        assert!(s.contains(r#"Expr::CellRef("A2".to_string())"#));
        assert!(s.ends_with("], ctx)?"));
    }

    #[test]
    fn call_nested_in_arithmetic() {
        let s = t("SUM(A1) + 1");
        assert!(s.starts_with(r#"apply_binary_op(BinaryOp::Add, standard().call("SUM","#));
        assert!(s.ends_with("CellValue::Number(1.0f64))?"));
    }

    #[test]
    fn call_args_reconstruct_nested_ast() {
        // `IF`'s branches must reach the registry unevaluated.
        let s = t("IF(A1, SUM(A2:A3), 0)");
        assert!(s.contains(r#"Expr::CellRef("A1".to_string())"#));
        assert!(s.contains(r#"Expr::Call("SUM".to_string()"#));
        assert!(s.contains(r#"Expr::Range("A2".to_string(), "A3".to_string())"#));
        assert!(s.contains("Expr::Number(0.0f64)"));
    }

    #[test]
    fn var_reads_through_ctx() {
        assert_eq!(
            transpile_expr(&Expr::Var("X".to_string())),
            r#"ctx.var("X").ok_or_else(|| EvalError::Ref("unbound: X".to_string()))?"#
        );
    }

    #[test]
    fn apply_invokes_lambda() {
        // (LAMBDA(x, x))(5) → apply_lambda(<lambda value>, vec![5], ctx)?
        let apply = Expr::Apply(
            Box::new(Expr::Call(
                "LAMBDA".to_string(),
                vec![Expr::Var("X".to_string()), Expr::Var("X".to_string())],
            )),
            vec![Expr::Number(5.0)],
        );
        let s = transpile_expr(&apply);
        assert!(s.starts_with(r#"apply_lambda(standard().call("LAMBDA","#));
        assert!(s.contains("vec![CellValue::Number(5.0f64)]"));
        assert!(s.ends_with(", ctx)?"));
    }

    #[test]
    fn emit_named_function_takes_ctx() {
        let f = emit_formula_fn("formula_0", &parse("1").unwrap());
        assert_eq!(
            f,
            "pub fn formula_0(ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {\n    Ok(CellValue::Number(1.0f64))\n}\n"
        );
    }

    #[test]
    fn map_ctx_reads_and_defaults() {
        let ctx = MapCtx::from_pairs(&[("A1", CellValue::Number(9.0))]);
        assert_eq!(ctx.cell("A1").unwrap(), CellValue::Number(9.0));
        assert_eq!(ctx.cell("a1").unwrap(), CellValue::Number(9.0));
        assert_eq!(ctx.cell("Z9").unwrap(), CellValue::Empty);
    }

    #[test]
    fn map_ctx_range_walks_rectangle() {
        let ctx = MapCtx::from_pairs(&[
            ("A1", CellValue::Number(1.0)),
            ("A2", CellValue::Number(2.0)),
            ("A3", CellValue::Number(3.0)),
        ]);
        let got = ctx.range("A1", "A3").unwrap();
        assert_eq!(
            got,
            vec![
                CellValue::Number(1.0),
                CellValue::Number(2.0),
                CellValue::Number(3.0),
            ]
        );
        assert_eq!(ctx.range("B1", "B2").unwrap(), vec![CellValue::Empty; 2]);
    }
}
