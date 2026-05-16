//! Differential test — the behavior gate for the Carbide transpiler.
//!
//! For every formula in the corpus, the transpiled-and-compiled result must
//! equal the tree-walking interpreter's result. The interpreter is the
//! oracle (see `docs/all-rust-roadmap.md`, Track B).
//!
//! Mechanism: parse + interpret each formula in-process (the oracle), then
//! transpile all of them into one generated crate, `cargo run` it once, and
//! compare each printed result against the oracle's `{:?}` form. Comparing
//! `Debug` strings works because `CellValue` / `EvalError` derive `Debug`,
//! so equal values format identically across the process boundary.

use std::fs;
use std::process::Command;

use tescellate_formula::excellite::eval::eval;
use tescellate_formula::excellite::parse::parse;
use tescellate_formula::transpile::emit_formula_fn;
use tescellate_formula::transpile::rt::CellValue;
use tescellate_formula::{EvalCtx, EvalError};

/// v1 corpus — literals, unary, every binary operator, and the error and
/// coercion edge cases. No cell references (the transpiler does not lower
/// them until v2), so an `EvalCtx` is never actually consulted.
const CORPUS: &[&str] = &[
    // literals
    "1",
    "0",
    "42",
    "2.5",
    "3.14159",
    "100000",
    r#""hello""#,
    r#""""#,
    "TRUE",
    "FALSE",
    // arithmetic
    "1 + 2",
    "10 - 3",
    "4 * 5",
    "20 / 4",
    "2 ^ 10",
    "10 / 4",
    "2 ^ 0.5",
    "1 + 2 * 3",
    "(1 + 2) * 3",
    "((1 + 2) * (3 + 4))",
    // unary
    "-5",
    "+7",
    "-(3 + 4)",
    "-(-8)",
    // runtime errors
    "1 / 0",
    "0 / 0",
    // concat
    r#""a" & "b""#,
    r#""x=" & 7"#,
    "1 & 2",
    // comparisons
    "1 < 2",
    "3 > 5",
    "2 = 2",
    "2 <> 3",
    "4 >= 4",
    "5 <= 4",
    "7 > 7",
    // coercion — whatever the interpreter does, the transpiler must match
    "1 + TRUE",
    "TRUE = TRUE",
    r#""apple" < "banana""#,
];

/// A context with no cells. The v1 subset never reads one; the methods
/// error loudly so a regression that *does* reach for a cell is obvious.
struct NoCtx;

impl EvalCtx for NoCtx {
    fn cell(&self, _addr: &str) -> Result<CellValue, EvalError> {
        Err(EvalError::Ref(
            "no cells in the v1 differential corpus".into(),
        ))
    }
    fn range(&self, _start: &str, _end: &str) -> Result<Vec<CellValue>, EvalError> {
        Err(EvalError::Ref(
            "no ranges in the v1 differential corpus".into(),
        ))
    }
}

#[test]
fn transpiled_carbide_matches_interpreter() {
    // 1. Oracle: parse + interpret each formula in-process.
    let mut oracle = Vec::new();
    let mut exprs = Vec::new();
    for &src in CORPUS {
        let expr = parse(src).unwrap_or_else(|e| panic!("parse `{src}`: {e}"));
        let got = eval(&expr, &NoCtx);
        oracle.push(format!("{got:?}"));
        exprs.push((src, expr));
    }

    // 2. Transpile every formula into one generated source file.
    let mut generated = String::from("use tescellate_formula::transpile::rt::*;\n\n");
    for (i, (src, expr)) in exprs.iter().enumerate() {
        let func = emit_formula_fn(&format!("formula_{i}"), expr)
            .unwrap_or_else(|e| panic!("transpile `{src}`: {e}"));
        generated.push_str(&func);
        generated.push('\n');
    }
    generated.push_str("fn main() {\n");
    for i in 0..exprs.len() {
        generated.push_str(&format!("    println!(\"{{:?}}\", formula_{i}());\n"));
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
