//! Carbide → Rust transpiler. See `docs/all-rust-roadmap.md` (Track B).
//!
//! Lowers a Carbide `Expr` to Rust source that, once compiled, evaluates to
//! the same `CellValue` as the tree-walking interpreter. Transpiled code
//! calls the very same value-level primitives the interpreter uses
//! (`apply_binary_op`, `apply_unary_op`), so equivalence holds *by
//! construction* for every subset implemented here — there is one
//! implementation of the operator semantics, not two.
//!
//! v1 subset: literals (`Number`, `Str`, `Bool`), `Unary`, `Binary`. The
//! remaining `Expr` variants return `TranspileError::Unsupported` until later
//! roadmap versions widen coverage (refs/ranges/arrays at v2, function calls
//! at v3, lambdas at v4).

pub mod rt;

use crate::excellite::ast::{BinaryOp, Expr, UnaryOp};

/// A formula the transpiler cannot yet lower.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TranspileError {
    #[error("transpile: unsupported expression — {0}")]
    Unsupported(String),
}

/// Lower `expr` to a Rust expression of type `CellValue`.
///
/// The result is valid inside a function body that returns
/// `Result<CellValue, EvalError>`: operator lowerings end in `?`, so an
/// evaluation error (`#DIV/0!`, `#NUM!`, …) propagates exactly as it does
/// in the interpreter.
pub fn transpile_expr(expr: &Expr) -> Result<String, TranspileError> {
    match expr {
        Expr::Number(n) => Ok(format!("CellValue::Number({n:?}f64)")),
        Expr::Str(s) => Ok(format!("CellValue::Text({s:?}.to_string())")),
        Expr::Bool(b) => Ok(format!("CellValue::Bool({b})")),
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
        Expr::CellRef(a) => unsupported(format!("cell reference `{a}`")),
        Expr::Range(a, b) => unsupported(format!("range `{a}:{b}`")),
        Expr::Array(_) => unsupported("array literal".into()),
        Expr::Var(n) => unsupported(format!("variable `{n}`")),
        Expr::Apply(..) => unsupported("function application".into()),
        Expr::Call(name, _) => unsupported(format!("function call `{name}`")),
    }
}

/// Emit a complete named Rust function that evaluates `expr`. The body is
/// the lowering from [`transpile_expr`]; the harness wraps a batch of these
/// into one crate for the differential test.
pub fn emit_formula_fn(name: &str, expr: &Expr) -> Result<String, TranspileError> {
    let body = transpile_expr(expr)?;
    Ok(format!(
        "pub fn {name}() -> Result<CellValue, EvalError> {{\n    Ok({body})\n}}\n"
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
        // The parser resolves precedence; the transpiler mirrors the tree.
        // `1 + 2 * 3` parses as `1 + (2 * 3)`.
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
        // `{:?}` on a String yields a valid Rust string literal even with
        // embedded quotes — constructed directly to avoid depending on the
        // Carbide lexer's escape rules.
        let e = Expr::Str("a\"b".to_string());
        assert_eq!(
            transpile_expr(&e).unwrap(),
            r#"CellValue::Text("a\"b".to_string())"#
        );
    }

    #[test]
    fn emit_named_function() {
        let f = emit_formula_fn("formula_0", &parse("1").unwrap()).unwrap();
        assert_eq!(
            f,
            "pub fn formula_0() -> Result<CellValue, EvalError> {\n    \
             Ok(CellValue::Number(1.0f64))\n}\n"
        );
    }

    #[test]
    fn unsupported_variants_error() {
        assert!(matches!(
            transpile_expr(&parse("A1").unwrap()),
            Err(TranspileError::Unsupported(_))
        ));
        assert!(matches!(
            transpile_expr(&parse("SUM(A1:A3)").unwrap()),
            Err(TranspileError::Unsupported(_))
        ));
    }
}
