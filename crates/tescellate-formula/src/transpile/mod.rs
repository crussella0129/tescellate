//! Carbide → Rust transpiler. See `docs/all-rust-roadmap.md` (Track B).
//!
//! Lowers a Carbide `Expr` to Rust source that, once compiled, evaluates to
//! the same `CellValue` as the tree-walking interpreter. Transpiled code
//! calls the very same value-level primitives the interpreter uses
//! (`apply_binary_op`, `apply_unary_op`, `bare_range`) and reads cells
//! through the same `EvalCtx` trait, so equivalence holds *by construction*
//! for every subset implemented here — there is one implementation of the
//! semantics, not two.
//!
//! Coverage: literals (`Number`, `Str`, `Bool`), `Unary`, `Binary`,
//! `CellRef`, `Range` (bare ranges are an error, as in the interpreter),
//! and `Array` literals. `Var` / `Apply` / `Call` return
//! `TranspileError::Unsupported` until v3 (function calls) and v4 (lambdas).

pub mod rt;

use std::collections::HashMap;

use crate::excellite::ast::{BinaryOp, Expr, UnaryOp};
use crate::{EvalCtx, EvalError};
use tescellate_core::CellValue;

/// A formula the transpiler cannot yet lower.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TranspileError {
    #[error("transpile: unsupported expression — {0}")]
    Unsupported(String),
}

/// Lower `expr` to a Rust expression of type `CellValue`.
///
/// The result is valid inside a function body that returns
/// `Result<CellValue, EvalError>` and has a `ctx: &dyn EvalCtx` in scope
/// (see [`emit_formula_fn`]): operator and cell-read lowerings end in `?`,
/// so an evaluation error propagates exactly as it does in the interpreter.
pub fn transpile_expr(expr: &Expr) -> Result<String, TranspileError> {
    match expr {
        Expr::Number(n) => Ok(format!("CellValue::Number({n:?}f64)")),
        Expr::Str(s) => Ok(format!("CellValue::Text({s:?}.to_string())")),
        Expr::Bool(b) => Ok(format!("CellValue::Bool({b})")),
        Expr::CellRef(addr) => Ok(format!("ctx.cell({addr:?})?")),
        Expr::Range(_, _) => Ok("bare_range()?".to_string()),
        Expr::Array(rows) => transpile_array(rows),
        Expr::Unary(op, inner) => {
            let inner = transpile_expr(inner)?;
            Ok(format!("apply_unary_op({}, {inner})?", unary_op_path(*op)))
        }
        Expr::Binary(op, lhs, rhs) => {
            let lhs = transpile_expr(lhs)?;
            let rhs = transpile_expr(rhs)?;
            Ok(format!(
                "apply_binary_op({}, {lhs}, {rhs})?",
                binary_op_path(*op)
            ))
        }
        Expr::Var(n) => unsupported(format!("variable `{n}`")),
        Expr::Apply(..) => unsupported("function application".into()),
        Expr::Call(name, _) => unsupported(format!("function call `{name}`")),
    }
}

/// Lower an array literal. Mirrors `excellite::eval::eval_array_literal`:
/// row-major flatten, `cols` taken from the first row — the parser already
/// rejects ragged arrays, so every row has the same length.
fn transpile_array(rows: &[Vec<Expr>]) -> Result<String, TranspileError> {
    if rows.is_empty() {
        return Ok("CellValue::Array(Box::new(Array::new(0, 0, Vec::new())))".to_string());
    }
    let nrows = rows.len();
    let ncols = rows[0].len();
    let mut elems = Vec::with_capacity(nrows * ncols);
    for row in rows {
        for e in row {
            elems.push(transpile_expr(e)?);
        }
    }
    Ok(format!(
        "CellValue::Array(Box::new(Array::new({nrows}, {ncols}, vec![{}])))",
        elems.join(", "),
    ))
}

/// Emit a complete named Rust function that evaluates `expr`. The function
/// takes `ctx: &dyn EvalCtx` — the same trait the interpreter reads cells
/// through — and is the unit the differential harness batches into a crate.
pub fn emit_formula_fn(name: &str, expr: &Expr) -> Result<String, TranspileError> {
    let body = transpile_expr(expr)?;
    Ok(format!(
        "pub fn {name}(ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {{\n    Ok({body})\n}}\n"
    ))
}

fn unsupported(what: String) -> Result<String, TranspileError> {
    Err(TranspileError::Unsupported(what))
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
    fn range(&self, _start: &str, _end: &str) -> Result<Vec<CellValue>, EvalError> {
        Err(EvalError::Value(
            "MapCtx::range — ranges-in-functions land at v3".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::excellite::parse::parse;

    fn t(src: &str) -> String {
        transpile_expr(&parse(src).unwrap()).unwrap()
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
        assert_eq!(
            transpile_expr(&e).unwrap(),
            r#"CellValue::Text("a\"b".to_string())"#
        );
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
    fn emit_named_function_takes_ctx() {
        let f = emit_formula_fn("formula_0", &parse("1").unwrap()).unwrap();
        assert_eq!(
            f,
            "pub fn formula_0(ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {\n    Ok(CellValue::Number(1.0f64))\n}\n"
        );
    }

    #[test]
    fn unsupported_variants_error() {
        // bare variable (Var) — lambdas land at v4
        assert!(matches!(
            transpile_expr(&parse("X").unwrap()),
            Err(TranspileError::Unsupported(_))
        ));
        // function call (Call) — lands at v3
        assert!(matches!(
            transpile_expr(&parse("SUM(A1:A3)").unwrap()),
            Err(TranspileError::Unsupported(_))
        ));
    }

    #[test]
    fn map_ctx_reads_and_defaults() {
        let ctx = MapCtx::from_pairs(&[("A1", CellValue::Number(9.0))]);
        assert_eq!(ctx.cell("A1").unwrap(), CellValue::Number(9.0));
        assert_eq!(ctx.cell("a1").unwrap(), CellValue::Number(9.0));
        assert_eq!(ctx.cell("Z9").unwrap(), CellValue::Empty);
    }
}
