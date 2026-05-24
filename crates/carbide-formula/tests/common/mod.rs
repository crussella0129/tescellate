//! Shared fixtures for the differential tests — the formula corpus, the
//! cell table, and `run_differential`, the transpile → compile → compare
//! harness. Used by `transpile_differential` (the hand-written corpus, run
//! as a binary), `native_differential` (the corpus, compiled to a cdylib),
//! and `transpile_fuzz` (generated formulas). One corpus, one harness.
//!
//! `dead_code` is allowed module-wide: each differential test binary is a
//! separate crate that includes this module and uses a different subset of
//! it (`native_differential` reads only `CELLS`/`CORPUS`, never the
//! harness), which is the normal shape of a shared test-fixture module.
#![allow(dead_code)]

use std::fs;
use std::process::Command;

use tescellate_formula::excellite::ast::Expr;
use tescellate_formula::excellite::eval::eval;
use tescellate_formula::transpile::rt::CellValue;
use tescellate_formula::transpile::{emit_formula_fn, MapCtx};

/// Numeric cells the corpus reads. Both differential tests build a `MapCtx`
/// from this exact table on each side of the comparison.
pub const CELLS: &[(&str, f64)] = &[
    ("A1", 10.0),
    ("A2", 2.5),
    ("A3", 7.0),
    ("B1", 3.0),
    ("B2", 100.0),
    ("C1", 0.0),
];

/// The differential corpus — every Carbide construct the transpiler lowers
/// (v1–v4). Differential: expected values are never hard-coded; the
/// evaluation paths being compared must simply agree.
pub const CORPUS: &[&str] = &[
    // literals
    "1",
    "0",
    "2.5",
    r#""hello""#,
    r#""""#,
    "TRUE",
    "FALSE",
    // arithmetic + unary
    "1 + 2",
    "20 / 4",
    "2 ^ 10",
    "1 + 2 * 3",
    "(1 + 2) * 3",
    "-5",
    "-(3 + 4)",
    // runtime error
    "1 / 0",
    // concat + comparisons
    r#""a" & "b""#,
    "1 < 2",
    "2 = 2",
    "4 >= 4",
    // cell references
    "A1",
    "A2",
    "C1",
    "Z9",
    "A1 + A2",
    "B2 / A1",
    "A1 * A2 + A3",
    "-A1",
    "(A1 + A2) * B1",
    "A1 = 10",
    "A1 > B1",
    "Z9 + 1",
    r#"A1 & " items""#,
    // bare ranges (an error in both paths)
    "A1:A3",
    "B1:C1",
    // array literals
    "[1, 2, 3]",
    "[[1, 2], [3, 4]]",
    "[]",
    "[A1, A2, A3]",
    "[A1 + 1, A2 * 2]",
    "[[A1, B1], [A2, B2]]",
    // function calls: aggregates
    "SUM(1, 2, 3)",
    "SUM(A1, A2, A3)",
    "SUM(A1:A3)",
    "SUM([1, 2, 3])",
    "AVERAGE(A1:A3)",
    "COUNT(A1:A3)",
    "MIN(A1:A3)",
    "MAX(A1:A3)",
    "MAX(A1:C1)",
    // logical (short-circuiting)
    "IF(A1 > 5, 100, 0)",
    "IF(TRUE, 1, 2)",
    "AND(TRUE, FALSE)",
    "OR(FALSE, TRUE)",
    "NOT(TRUE)",
    "IFERROR(1 / 0, 99)",
    "IFS(FALSE, 1, TRUE, 2)",
    // math
    "ABS(-7)",
    "ROUND(3.14159, 2)",
    "SQRT(16)",
    "MOD(10, 3)",
    "POWER(2, 8)",
    // text
    r#"LEN("hello")"#,
    r#"UPPER("hi")"#,
    r#"LEFT("hello", 3)"#,
    // lookup / dynamic arrays
    "INDEX([[10, 20], [30, 40]], 2, 1)",
    r#"MATCH("b", ["a", "b", "c"])"#,
    "SEQUENCE(5)",
    "UNIQUE([1, 2, 2, 3])",
    // nested calls
    "SUM(A1:A3) + 1",
    "IF(A1 > 0, SUM(A1:A3), 0)",
    "ABS(A1 - B2)",
    "SUM(A1, AVERAGE(A2:A3))",
    // error paths
    "BOGUS(1)",
    "SQRT(-1)",
    // LET / LAMBDA / LETREC, lambdas, higher-order
    "LET(x, 10, x + 5)",
    "LET(x, 10, y, x * 2, x + y)",
    "LET(m, A1, m * 2)",
    "(LAMBDA(x, x * 2))(5)",
    "(LAMBDA(a, b, a + b))(3, 4)",
    "LET(f, LAMBDA(x, x + 1), f(7))",
    "LET(n, 100, g, LAMBDA(x, x + n), g(5))",
    "LETREC(fact, LAMBDA(n, IF(n <= 1, 1, n * fact(n - 1))), fact(5))",
    "MAP([1, 2, 3], LAMBDA(x, x * 2))",
    "REDUCE(0, [1, 2, 3, 4], LAMBDA(a, x, a + x))",
    "MAKEARRAY(2, 2, LAMBDA(r, c, r + c))",
    // bare variable (unbound at a cell's top level — an error, both paths)
    "x",
    // v6 — hardening: numeric, coercion, error-propagation, recursion edges
    "((((1 + 1) + 1) + 1) + 1)",
    "2 ^ 3 ^ 2",
    "2 ^ 2000",
    "2 ^ -10",
    "-0",
    "0 / 0",
    "100000 * 100000 * 100000",
    "TRUE + TRUE",
    "FALSE * 100",
    r#""5" + 3"#,
    r#""abc" + 1"#,
    // v10 regression — arithmetic operand-evaluation order. The lhs
    // evaluates fine but coerces to an error; the rhs *evaluates* to an
    // error. The interpreter evaluates rhs before coercing lhs, so it
    // surfaces `#DIV/0!`; the transpiler must bind both operands before
    // coercing either, or it would surface the lhs coercion error instead.
    r#"("x" & "y") * (1 / 0)"#,
    r#"1 = "1""#,
    r#""" & "" & "x""#,
    "IF(1 / 0, 1, 2)",
    "SUM(1 / 0, 2, 3)",
    "1 + BOGUS(2)",
    r#"IFERROR(SQRT(-1), "caught")"#,
    "LETREC(fib, LAMBDA(n, IF(n <= 1, n, fib(n - 1) + fib(n - 2))), fib(12))",
    "LETREC(down, LAMBDA(n, IF(n <= 0, 0, n + down(n - 1))), down(50))",
    "REDUCE(1, [1, 2, 3, 4, 5, 6, 7, 8], LAMBDA(a, x, a * x))",
    "MAKEARRAY(4, 4, LAMBDA(r, c, IF(r = c, 1, 0)))",
    "MAP([1, 2, 3, 4, 5], LAMBDA(x, x * x))",
    "SUM(SUM(A1:A3), MAX(B1, B2), MIN(A1, A2))",
    "LET(xs, [1, 2, 3, 4], SUM(MAP(xs, LAMBDA(v, v * v))))",
    "LET(adder, LAMBDA(x, LAMBDA(y, x + y)), (adder(10))(5))",
];

/// The shared cell context — `CELLS` as a `MapCtx`. The interpreter reads
/// from one of these in-process; every generated crate rebuilds the same
/// table (see `run_differential`), so both sides see an identical sheet.
fn cell_context() -> MapCtx {
    let pairs: Vec<(&str, CellValue)> = CELLS
        .iter()
        .map(|(addr, v)| (*addr, CellValue::Number(*v)))
        .collect();
    MapCtx::from_pairs(&pairs)
}

/// Differentially check a batch of `(label, expr)` cases against the
/// tree-walking interpreter — the oracle.
///
/// Every case is transpiled into one generated crate, which is compiled and
/// run exactly once; each transpiled result must `{:?}`-equal `eval`'s
/// result — value, type, *and* error. `label` is only used to identify a
/// case in a failure message (a source string for the corpus, a `{:?}` AST
/// dump for generated formulas).
///
/// `dir_name` names the generated crate's temp directory and package, so
/// callers running concurrently never share a generated crate.
pub fn run_differential(dir_name: &str, cases: &[(String, Expr)]) {
    let ctx = cell_context();

    // 1. Oracle: interpret every case in-process.
    let oracle: Vec<String> = cases
        .iter()
        .map(|(_, expr)| format!("{:?}", eval(expr, &ctx)))
        .collect();

    // 2. Transpile every case into one generated source file.
    let mut generated = String::from(
        "#![allow(unused_variables)]\n\
         use tescellate_formula::transpile::rt::*;\n\
         use tescellate_formula::transpile::MapCtx;\n\n",
    );
    for (i, (_, expr)) in cases.iter().enumerate() {
        generated.push_str(&emit_formula_fn(&format!("formula_{i}"), expr));
        generated.push('\n');
    }
    // The generated crate rebuilds the context from the same CELLS table.
    generated.push_str("fn context() -> MapCtx {\n    MapCtx::from_pairs(&[\n");
    for (addr, v) in CELLS {
        generated.push_str(&format!(
            "        ({addr:?}, CellValue::Number({v:?}f64)),\n"
        ));
    }
    generated.push_str("    ])\n}\n\n");
    generated.push_str("fn main() {\n    let ctx = context();\n");
    for i in 0..cases.len() {
        generated.push_str(&format!("    println!(\"{{:?}}\", formula_{i}(&ctx));\n"));
    }
    generated.push_str("}\n");

    // 3. Materialize a standalone crate. The temp dir is stable, so the
    //    crate's `target/` persists and re-runs are fast. `[workspace]`
    //    detaches it from any ancestor workspace.
    let formula_crate = env!("CARGO_MANIFEST_DIR"); // .../crates/tescellate-formula
    let crate_dir = std::env::temp_dir().join(dir_name);
    fs::create_dir_all(crate_dir.join("src")).expect("create generated crate dir");
    let cargo_toml = format!(
        "[package]\n\
         name = \"{dir_name}\"\n\
         version = \"0.0.0\"\n\
         edition = \"2021\"\n\n\
         [dependencies]\n\
         tescellate-formula = {{ path = {formula_crate:?} }}\n\n\
         [workspace]\n"
    );
    fs::write(crate_dir.join("Cargo.toml"), cargo_toml).expect("write generated Cargo.toml");
    fs::write(crate_dir.join("src/main.rs"), &generated).expect("write generated main.rs");

    // 4. Compile + run the generated crate.
    let out = Command::new("cargo")
        .args(["run", "--quiet"])
        .current_dir(&crate_dir)
        .output()
        .expect("invoke cargo on the generated crate");
    assert!(
        out.status.success(),
        "generated crate failed to build/run\n--- stderr ---\n{}\n--- generated source ---\n{generated}",
        String::from_utf8_lossy(&out.stderr),
    );

    // 5. Compare each printed result against the oracle.
    let stdout = String::from_utf8(out.stdout).expect("generated crate stdout is utf-8");
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(
        lines.len(),
        oracle.len(),
        "expected {} result lines, got {}\n{stdout}",
        oracle.len(),
        lines.len(),
    );

    let mut mismatches = Vec::new();
    for (i, (label, _)) in cases.iter().enumerate() {
        if oracle[i].as_str() != lines[i] {
            mismatches.push(format!(
                "  {label}\n      interpreter: {}\n      transpiled : {}",
                oracle[i], lines[i],
            ));
        }
    }
    assert!(
        mismatches.is_empty(),
        "transpiled output diverged from the interpreter ({} of {} cases):\n{}",
        mismatches.len(),
        cases.len(),
        mismatches.join("\n"),
    );
}
