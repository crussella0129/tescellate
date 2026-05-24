//! Carbide → Rust transpiler. See `docs/all-rust-roadmap.md` (Track B).
//!
//! Lowers a Carbide `Expr` to Rust source that, once compiled, evaluates to
//! the same `CellValue` as the tree-walking interpreter. Transpiled code
//! calls the very same value-level primitives the interpreter uses
//! (`apply_binary_op`, `apply_unary_op`, `bare_range`, `apply_lambda`,
//! `number_binary_op`), reads cells and variables through the same
//! `EvalCtx` trait, and dispatches function calls into the same
//! `excellite::funcs::standard()` registry — so equivalence holds *by
//! construction*: there is one implementation of the semantics, not two.
//!
//! ## Type specialization (v9)
//!
//! Every `Expr` variant is lowered, so transpilation is total. On top of
//! that, the codegen is *typed*: `transpile_typed` tracks whether a subtree
//! is statically known to be a number ([`Ty::Number`]) or of unknown type
//! ([`Ty::Value`], a `CellValue`). A number-typed subtree — number
//! literals, and arithmetic over number-typed operands — is emitted as raw
//! `f64` code calling `number_binary_op`, skipping the `CellValue` boxing
//! and the `to_number` coercion the general path pays. Where a type isn't
//! statically known, the general `apply_*_op` / `CellValue` path is used.
//! `to_number` is inserted only at the boundary between the two — and
//! arithmetic binds *both* operands to locals before coercing either, so
//! coercion (which belongs to the operator, not the operand) never runs
//! ahead of evaluating the other operand: an error surfaces from exactly
//! the operand the interpreter's `eval(lhs)?; eval(rhs)?` order would
//! surface it from.
//!
//! Building on that, a subtree whose every leaf is a number literal is a
//! compile-time constant: [`const_fold`] evaluates it *during transpilation*
//! — through the interpreter's own `number_binary_op`, so the folded value
//! is bit-identical to a runtime evaluation — and the whole subtree
//! collapses to a single `f64` literal. A constant that would error at
//! runtime (a literal `1 / 0`, or an overflow to infinity) is deliberately
//! left unfolded, so the error still surfaces exactly as the interpreter
//! raises it.

pub mod rt;

#[cfg(feature = "native")]
pub mod native;

use std::collections::HashMap;

use crate::excellite::ast::{BinaryOp, Expr, UnaryOp};
use crate::excellite::eval::number_binary_op;
use crate::{EvalCtx, EvalError};
use carbide_core::CellValue;

/// The static type the codegen tracks for a subtree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ty {
    /// Statically a number — emitted as a raw `f64` expression.
    Number,
    /// Unknown type — emitted as a `CellValue` expression.
    Value,
}

/// Lower `expr` to a Rust expression of type `CellValue`.
///
/// The result is valid inside a function body that returns
/// `Result<CellValue, EvalError>` and has a `ctx: &dyn EvalCtx` in scope
/// (see [`emit_formula_fn`]): operator, cell-read, variable, call, and
/// apply lowerings end in `?`, so an evaluation error propagates exactly
/// as it does in the interpreter.
pub fn transpile_expr(expr: &Expr) -> String {
    let (code, ty) = transpile_typed(expr);
    as_value(&code, ty)
}

/// Lower `expr`, returning the Rust source and its static [`Ty`].
fn transpile_typed(expr: &Expr) -> (String, Ty) {
    // A fully-constant arithmetic subtree collapses to one `f64` literal,
    // computed now rather than on every evaluation.
    if let Some(v) = const_fold(expr) {
        return (format!("{v:?}f64"), Ty::Number);
    }
    match expr {
        Expr::Number(n) => (format!("{n:?}f64"), Ty::Number),
        Expr::Str(s) => (format!("CellValue::Text({s:?}.to_string())"), Ty::Value),
        Expr::Bool(b) => (format!("CellValue::Bool({b})"), Ty::Value),
        Expr::CellRef(addr) => (format!("ctx.cell({addr:?})?"), Ty::Value),
        Expr::Range(_, _) => ("bare_range()?".to_string(), Ty::Value),
        Expr::Array(rows) => (transpile_array(rows), Ty::Value),
        // Unary `-` / `+` always produce a number; specialize the operand.
        Expr::Unary(op, inner) => {
            let (ic, it) = transpile_typed(inner);
            let operand = as_number(&ic, it);
            let code = match op {
                UnaryOp::Neg => format!("(-({operand}))"),
                UnaryOp::Pos => operand,
            };
            (code, Ty::Number)
        }
        // Arithmetic over `f64` — the specialized numeric kernel. Both
        // operands are evaluated into locals (`a`, `b`) *first*, then each
        // is coerced to `f64` (a no-op when already a number). Binding
        // before coercing keeps the interpreter's `eval(lhs)?; eval(rhs)?`
        // order: coercing `a` must not run before `b` is evaluated, or an
        // error would surface from the wrong operand. Nested arithmetic
        // shadows `a`/`b` in its own block — correct and scope-local.
        Expr::Binary(op, lhs, rhs) if is_arithmetic(*op) => {
            let (lc, lt) = transpile_typed(lhs);
            let (rc, rt) = transpile_typed(rhs);
            let code = format!(
                "{{ let a = {lc}; let b = {rc}; number_binary_op({}, {}, {})? }}",
                binary_op_path(*op),
                as_number("a", lt),
                as_number("b", rt),
            );
            (code, Ty::Number)
        }
        // Concat / comparisons produce Text / Bool, not a number — they
        // stay on the general `CellValue` path.
        Expr::Binary(op, lhs, rhs) => {
            let (lc, lt) = transpile_typed(lhs);
            let (rc, rt) = transpile_typed(rhs);
            let code = format!(
                "apply_binary_op({}, {}, {})?",
                binary_op_path(*op),
                as_value(&lc, lt),
                as_value(&rc, rt),
            );
            (code, Ty::Value)
        }
        // A call dispatches into the shared `standard()` registry. Its
        // arguments are passed as reconstructed AST (`emit_expr_literal`),
        // not pre-evaluated values, so registry functions keep their lazy
        // argument semantics — `IF` / `AND` short-circuiting still works,
        // and `LET` / `LAMBDA` / `LETREC` evaluate their bodies themselves.
        Expr::Call(name, args) => (
            format!(
                "standard().call({name:?}, &[{}], ctx)?",
                emit_expr_list(args)
            ),
            Ty::Value,
        ),
        // A bare variable resolves against the lexical environment. At a
        // cell's top level there is no scope, so this is an unbound error.
        Expr::Var(name) => {
            let msg = format!("unbound: {name}");
            (
                format!("ctx.var({name:?}).ok_or_else(|| EvalError::Ref({msg:?}.to_string()))?"),
                Ty::Value,
            )
        }
        // Application of a callee value (e.g. an immediately-invoked
        // lambda): evaluate the callee, then the arguments, then dispatch
        // through the shared `apply_lambda` primitive.
        Expr::Apply(callee, args) => {
            let arg_srcs: Vec<String> = args.iter().map(transpile_expr).collect();
            (
                format!(
                    "apply_lambda({}, vec![{}], ctx)?",
                    transpile_expr(callee),
                    arg_srcs.join(", "),
                ),
                Ty::Value,
            )
        }
    }
}

/// Coerce a transpiled sub-expression to an `f64` expression. A `Number`
/// subtree is already an `f64`; a `Value` subtree is run through the
/// shared `to_number` coercion (exactly what the interpreter does).
fn as_number(code: &str, ty: Ty) -> String {
    match ty {
        Ty::Number => code.to_string(),
        Ty::Value => format!("to_number(&({code}))?"),
    }
}

/// Coerce a transpiled sub-expression to a `CellValue` expression.
fn as_value(code: &str, ty: Ty) -> String {
    match ty {
        Ty::Number => format!("CellValue::Number({code})"),
        Ty::Value => code.to_string(),
    }
}

fn is_arithmetic(op: BinaryOp) -> bool {
    matches!(
        op,
        BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Pow
    )
}

/// Evaluate a fully-constant arithmetic subtree at transpile time.
///
/// Returns `Some(v)` only when every leaf is a number literal *and* the
/// arithmetic succeeds. Folding goes through the interpreter's own
/// [`number_binary_op`], so a folded constant is bit-identical to what a
/// runtime evaluation would produce. A subtree that *would* error at
/// runtime — a literal `1 / 0`, or a `Pow` that overflows to infinity —
/// yields `None`: `number_binary_op` returns `Err`, `.ok()` drops it, and
/// the runtime call is left in place to raise the error exactly as the
/// interpreter does. Non-arithmetic nodes (cell reads, calls, concat,
/// comparisons) are never constant here and short-circuit to `None`.
fn const_fold(expr: &Expr) -> Option<f64> {
    match expr {
        Expr::Number(n) => Some(*n),
        Expr::Unary(UnaryOp::Neg, inner) => Some(-const_fold(inner)?),
        Expr::Unary(UnaryOp::Pos, inner) => const_fold(inner),
        Expr::Binary(op, lhs, rhs) if is_arithmetic(*op) => {
            number_binary_op(*op, const_fold(lhs)?, const_fold(rhs)?).ok()
        }
        _ => None,
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

    /// Single-letter A1 offset — enough to exercise OFFSET in the
    /// reference corpus. Columns are 0-based (`A`=0), rows 1-based.
    fn offset_addr(&self, addr: &str, dcol: i32, drow: i32) -> Result<String, EvalError> {
        let (col, row) = parse_a1(addr)?;
        let nc = col as i32 + dcol;
        let nr = row as i32 + drow;
        if nc < 0 || nr < 1 {
            return Err(addr_err(addr));
        }
        Ok(format!("{}{nr}", (b'A' + nc as u8) as char))
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
    fn unary_negation_specializes_to_f64() {
        // A non-constant operand: negation emits raw f64 — no `apply_unary_op`,
        // no `CellValue` box for the operand.
        assert_eq!(
            t("-A1"),
            r#"CellValue::Number((-(to_number(&(ctx.cell("A1")?))?)))"#
        );
    }

    #[test]
    fn arithmetic_specializes_and_nests() {
        // `A1 + A2 * A3` parses as `A1 + (A2 * A3)`; non-constant operands
        // stay as nested `number_binary_op` over raw f64s, each arithmetic
        // node binding its operands to `a`/`b` before coercing.
        let s = t("A1 + A2 * A3");
        assert!(s.starts_with(r#"CellValue::Number({ let a = ctx.cell("A1")?; let b = {"#));
        assert!(s.contains("number_binary_op(BinaryOp::Mul, to_number(&(a))?, to_number(&(b))?)?"));
        assert!(s.ends_with("number_binary_op(BinaryOp::Add, to_number(&(a))?, b)? })"));
    }

    #[test]
    fn constant_arithmetic_folds_at_transpile_time() {
        // Fully-constant subtrees are evaluated during transpilation.
        assert_eq!(t("1 + 2"), "CellValue::Number(3.0f64)");
        assert_eq!(t("2 ^ 10"), "CellValue::Number(1024.0f64)");
        assert_eq!(t("(3 + 4) * 2"), "CellValue::Number(14.0f64)");
        assert_eq!(t("-5"), "CellValue::Number(-5.0f64)");
        // Precedence holds: `1 + 2 * 3` folds to 7, not 9.
        assert_eq!(t("1 + 2 * 3"), "CellValue::Number(7.0f64)");
    }

    #[test]
    fn partial_constants_fold_only_the_constant_part() {
        // The constant factor `2 * 3` folds; the cell read stays a runtime op.
        assert_eq!(
            t("A1 + 2 * 3"),
            r#"CellValue::Number({ let a = ctx.cell("A1")?; let b = 6.0f64; number_binary_op(BinaryOp::Add, to_number(&(a))?, b)? })"#
        );
    }

    #[test]
    fn constant_that_would_error_is_left_unfolded() {
        // A literal `1 / 0` must still raise #DIV/0! at runtime, exactly as
        // the interpreter does — so it stays a runtime call, not a fold.
        assert_eq!(
            t("1 / 0"),
            "CellValue::Number({ let a = 1.0f64; let b = 0.0f64; number_binary_op(BinaryOp::Div, a, b)? })"
        );
    }

    #[test]
    fn cell_ref_in_arithmetic_coerces_with_to_number() {
        // A cell read is `Value`-typed, so it crosses into the numeric path
        // through `to_number` — exactly as the interpreter coerces it. Both
        // operands are bound to locals before either is coerced.
        assert_eq!(
            t("A1 + A2"),
            r#"CellValue::Number({ let a = ctx.cell("A1")?; let b = ctx.cell("A2")?; number_binary_op(BinaryOp::Add, to_number(&(a))?, to_number(&(b))?)? })"#
        );
    }

    #[test]
    fn concat_stays_on_the_value_path() {
        let s = t(r#""a" & "b""#);
        assert!(s.starts_with("apply_binary_op(BinaryOp::Concat, "));
        assert!(s.contains(r#"CellValue::Text("a".to_string())"#));
    }

    #[test]
    fn comparison_stays_on_the_value_path() {
        // `1 < 2` produces a Bool — number operands are wrapped back up.
        let s = t("1 < 2");
        assert!(s.starts_with("apply_binary_op(BinaryOp::Lt, "));
        assert!(s.contains("CellValue::Number(1.0f64)"));
        assert!(s.contains("CellValue::Number(2.0f64)"));
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
    fn call_in_arithmetic_coerces_with_to_number() {
        // A call is `Value`-typed; bound to `a`, then coerced. The literal
        // `1` is already `Number`-typed, so `b` needs no coercion.
        assert_eq!(
            t("SUM(A1) + 1"),
            r#"CellValue::Number({ let a = standard().call("SUM", &[Expr::CellRef("A1".to_string())], ctx)?; let b = 1.0f64; number_binary_op(BinaryOp::Add, to_number(&(a))?, b)? })"#
        );
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
