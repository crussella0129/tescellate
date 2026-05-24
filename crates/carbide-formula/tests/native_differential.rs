//! Native differential test — v5's behavior gate.
//!
//! Compiles the whole formula corpus into one native `cdylib` and asserts
//! every formula's compiled result equals the tree-walking interpreter's.
//! The interpreter is the oracle (see `docs/all-rust-roadmap.md`, Track B).
//!
//! Feature-gated (`native`): the whole file compiles away without it.

#![cfg(feature = "native")]

mod common;

use std::sync::Arc;

use tescellate_formula::excellite::eval::eval;
use tescellate_formula::excellite::parse::parse;
use tescellate_formula::transpile::native::compile_program;
use tescellate_formula::transpile::rt::CellValue;
use tescellate_formula::transpile::MapCtx;

#[test]
fn natively_compiled_carbide_matches_interpreter() {
    // Shared context, identical on both sides.
    let pairs: Vec<(&str, CellValue)> = common::CELLS
        .iter()
        .map(|(addr, v)| (*addr, CellValue::Number(*v)))
        .collect();
    let ctx = MapCtx::from_pairs(&pairs);

    let exprs: Vec<(&str, _)> = common::CORPUS
        .iter()
        .map(|&src| {
            (
                src,
                parse(src).unwrap_or_else(|e| panic!("parse `{src}`: {e}")),
            )
        })
        .collect();

    // Oracle: the tree-walking interpreter.
    let oracle: Vec<String> = exprs
        .iter()
        .map(|(_, e)| format!("{:?}", eval(e, &ctx)))
        .collect();

    // Compile the entire corpus into one native cdylib.
    let refs: Vec<&_> = exprs.iter().map(|(_, e)| e).collect();
    let program = compile_program(&refs).expect("compile the corpus natively");
    assert_eq!(program.len(), exprs.len());

    let mut mismatches = Vec::new();
    for (i, (src, _)) in exprs.iter().enumerate() {
        let native = format!("{:?}", program.eval(i, &ctx));
        if native != oracle[i] {
            mismatches.push(format!(
                "  `{src}`\n      interpreter: {}\n      native     : {native}",
                oracle[i],
            ));
        }
    }
    assert!(
        mismatches.is_empty(),
        "natively compiled output diverged from the interpreter:\n{}",
        mismatches.join("\n"),
    );
}

#[test]
fn identical_corpora_hit_the_compile_cache() {
    let e = parse("SUM(1, 2, 3) + 4").unwrap();
    let first = compile_program(&[&e]).expect("compile");
    let second = compile_program(&[&e]).expect("recompile");
    assert!(
        Arc::ptr_eq(&first, &second),
        "an identical formula set should return the cached program, not recompile",
    );
}
