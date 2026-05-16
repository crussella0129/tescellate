//! Differential test — the behavior gate for the Carbide transpiler.
//!
//! For every formula in the corpus, the transpiled-and-compiled result must
//! equal the tree-walking interpreter's result. The interpreter is the
//! oracle (see `docs/all-rust-roadmap.md`, Track B).
//!
//! Both sides read cells through the same `MapCtx`, built from the same
//! `CELLS` table, so the comparison isolates the transpiler. Mechanism:
//! parse + interpret each formula in-process (the oracle), then transpile
//! all of them into one generated crate, `cargo run` it once, and compare
//! each printed result against the oracle's `{:?}` form.

use std::fs;
use std::process::Command;

use tescellate_formula::excellite::eval::eval;
use tescellate_formula::excellite::parse::parse;
use tescellate_formula::transpile::rt::CellValue;
use tescellate_formula::transpile::{emit_formula_fn, MapCtx};

/// Numeric cells the corpus reads. Both the in-process oracle and the
/// generated crate build a `MapCtx` from this exact table.
const CELLS: &[(&str, f64)] = &[
    ("A1", 10.0),
    ("A2", 2.5),
    ("A3", 7.0),
    ("B1", 3.0),
    ("B2", 100.0),
    ("C1", 0.0),
];

/// v1 + v2 corpus. v2 adds cell references, bare ranges (an error), and
/// array literals. Differential: expected values are never hard-coded —
/// the two evaluation paths must simply agree.
const CORPUS: &[&str] = &[
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
    // v2 — cell references
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
    // v2 — bare ranges (an error in both paths)
    "A1:A3",
    "B1:C1",
    // v2 — array literals
    "[1, 2, 3]",
    "[[1, 2], [3, 4]]",
    "[]",
    "[A1, A2, A3]",
    "[A1 + 1, A2 * 2]",
    "[[A1, B1], [A2, B2]]",
    // v3 — function calls: aggregates
    "SUM(1, 2, 3)",
    "SUM(A1, A2, A3)",
    "SUM(A1:A3)",
    "SUM([1, 2, 3])",
    "AVERAGE(A1:A3)",
    "COUNT(A1:A3)",
    "MIN(A1:A3)",
    "MAX(A1:A3)",
    "MAX(A1:C1)",
    // v3 — logical (short-circuiting)
    "IF(A1 > 5, 100, 0)",
    "IF(TRUE, 1, 2)",
    "AND(TRUE, FALSE)",
    "OR(FALSE, TRUE)",
    "NOT(TRUE)",
    "IFERROR(1 / 0, 99)",
    "IFS(FALSE, 1, TRUE, 2)",
    // v3 — math
    "ABS(-7)",
    "ROUND(3.14159, 2)",
    "SQRT(16)",
    "MOD(10, 3)",
    "POWER(2, 8)",
    // v3 — text
    r#"LEN("hello")"#,
    r#"UPPER("hi")"#,
    r#"LEFT("hello", 3)"#,
    // v3 — lookup / dynamic arrays
    "INDEX([[10, 20], [30, 40]], 2, 1)",
    r#"MATCH("b", ["a", "b", "c"])"#,
    "SEQUENCE(5)",
    "UNIQUE([1, 2, 2, 3])",
    // v3 — nested calls
    "SUM(A1:A3) + 1",
    "IF(A1 > 0, SUM(A1:A3), 0)",
    "ABS(A1 - B2)",
    "SUM(A1, AVERAGE(A2:A3))",
    // v3 — error paths
    "BOGUS(1)",
    "SQRT(-1)",
    // v4 — LET / LAMBDA / LETREC, lambdas, higher-order
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
    // v4 — bare variable (unbound at a cell's top level — an error, both paths)
    "x",
];

#[test]
fn transpiled_carbide_matches_interpreter() {
    // Shared context for the in-process oracle.
    let oracle_pairs: Vec<(&str, CellValue)> = CELLS
        .iter()
        .map(|(addr, v)| (*addr, CellValue::Number(*v)))
        .collect();
    let ctx = MapCtx::from_pairs(&oracle_pairs);

    // 1. Oracle: parse + interpret each formula in-process.
    let mut oracle = Vec::new();
    let mut exprs = Vec::new();
    for &src in CORPUS {
        let expr = parse(src).unwrap_or_else(|e| panic!("parse `{src}`: {e}"));
        let got = eval(&expr, &ctx);
        oracle.push(format!("{got:?}"));
        exprs.push((src, expr));
    }

    // 2. Transpile every formula into one generated source file.
    let mut generated = String::from(
        "#![allow(unused_variables)]\n\
         use tescellate_formula::transpile::rt::*;\n\
         use tescellate_formula::transpile::MapCtx;\n\n",
    );
    for (i, (_, expr)) in exprs.iter().enumerate() {
        let func = emit_formula_fn(&format!("formula_{i}"), expr);
        generated.push_str(&func);
        generated.push('\n');
    }
    // The generated crate builds the same context from the same CELLS.
    generated.push_str("fn context() -> MapCtx {\n    MapCtx::from_pairs(&[\n");
    for (addr, v) in CELLS {
        generated.push_str(&format!(
            "        ({addr:?}, CellValue::Number({v:?}f64)),\n"
        ));
    }
    generated.push_str("    ])\n}\n\n");
    generated.push_str("fn main() {\n    let ctx = context();\n");
    for i in 0..exprs.len() {
        generated.push_str(&format!("    println!(\"{{:?}}\", formula_{i}(&ctx));\n"));
    }
    generated.push_str("}\n");

    // 3. Materialize a standalone crate. The temp dir is stable, so the
    //    crate's `target/` persists and re-runs are fast. `[workspace]`
    //    detaches it from any ancestor workspace.
    let formula_crate = env!("CARGO_MANIFEST_DIR"); // .../crates/tescellate-formula
    let crate_dir = std::env::temp_dir().join("tescellate_transpile_diff");
    fs::create_dir_all(crate_dir.join("src")).expect("create generated crate dir");
    let cargo_toml = format!(
        "[package]\n\
         name = \"transpile_diff\"\n\
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
    for (i, (src, _)) in exprs.iter().enumerate() {
        if oracle[i].as_str() != lines[i] {
            mismatches.push(format!(
                "  `{src}`\n      interpreter: {}\n      transpiled : {}",
                oracle[i], lines[i],
            ));
        }
    }
    assert!(
        mismatches.is_empty(),
        "transpiled output diverged from the interpreter:\n{}",
        mismatches.join("\n"),
    );
}
