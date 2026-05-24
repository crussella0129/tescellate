//! Differential test — transpiled Carbide, compiled and run as a binary,
//! must equal the tree-walking interpreter. The interpreter is the oracle
//! (see `docs/all-rust-roadmap.md`, Track B).
//!
//! This file is the *hand-written corpus* side of the behavior gate; the
//! generated-formula side lives in `transpile_fuzz`. Both drive the shared
//! `common::run_differential` harness — transpile every case into one
//! crate, compile + run it once, and compare each result to `eval`.

mod common;

use carbide_formula::excellite::parse::parse;

#[test]
fn transpiled_carbide_matches_interpreter() {
    let cases: Vec<(String, _)> = common::CORPUS
        .iter()
        .map(|&src| {
            let expr = parse(src).unwrap_or_else(|e| panic!("parse `{src}`: {e}"));
            (format!("`{src}`"), expr)
        })
        .collect();
    common::run_differential("carbide_transpile_diff", &cases);
}
