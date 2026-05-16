//! Differential test — transpiled Carbide, compiled and run as a binary,
//! must equal the tree-walking interpreter. The interpreter is the oracle
//! (see `docs/all-rust-roadmap.md`, Track B).
//!
//! Mechanism: parse + interpret each formula in-process (the oracle), then
//! transpile all of them into one generated crate, `cargo run` it once, and
//! compare each printed result against the oracle's `{:?}` form. The corpus
//! and cell table are shared with `native_differential` via `common`.

mod common;

use std::fs;
use std::process::Command;

use tescellate_formula::excellite::eval::eval;
use tescellate_formula::excellite::parse::parse;
use tescellate_formula::transpile::rt::CellValue;
use tescellate_formula::transpile::{emit_formula_fn, MapCtx};

#[test]
fn transpiled_carbide_matches_interpreter() {
    // Shared context for the in-process oracle.
    let oracle_pairs: Vec<(&str, CellValue)> = common::CELLS
        .iter()
        .map(|(addr, v)| (*addr, CellValue::Number(*v)))
        .collect();
    let ctx = MapCtx::from_pairs(&oracle_pairs);

    // 1. Oracle: parse + interpret each formula in-process.
    let mut oracle = Vec::new();
    let mut exprs = Vec::new();
    for &src in common::CORPUS {
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
    for (addr, v) in common::CELLS {
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
