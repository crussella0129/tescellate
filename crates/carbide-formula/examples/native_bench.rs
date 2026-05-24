//! Benchmark — interpreted vs natively-compiled Carbide evaluation.
//!
//! Run:
//! ```text
//! cargo run --release -p carbide-formula --example native_bench --features native
//! ```
//!
//! The interpreter walks the AST on every evaluation; the native tier bakes
//! the structure into compiled machine code. Two formulas are measured:
//!
//! 1. *Cell-bound* — arithmetic over cell references. Every leaf is a
//!    `ctx.cell` dynamic dispatch plus a map lookup, shared by both paths,
//!    so the native win is bounded to removing the AST-walk dispatch.
//! 2. *Constant-bearing* — the same shape but carrying constant
//!    sub-expressions (compound-growth factors). v9's transpiler folds
//!    those at compile time, so the native path skips arithmetic the
//!    interpreter repeats on every single evaluation.
//!
//! See the recorded result at the bottom of this file.

use std::hint::black_box;
use std::time::{Duration, Instant};

use carbide_formula::excellite::eval::eval;
use carbide_formula::excellite::parse::parse;
use carbide_formula::transpile::native::compile_program;
use carbide_formula::transpile::rt::CellValue;
use carbide_formula::transpile::MapCtx;

const ITERS: u32 = 2_000_000;

/// Time `ITERS` interpreted and native evaluations of `formula`, print the
/// per-eval cost and the speedup, and return the one-time compile cost.
fn bench(label: &str, formula: &str, ctx: &MapCtx) -> Duration {
    let expr = parse(formula).expect("parse the benchmark formula");

    // Interpreted tier: walk the AST every iteration.
    let start = Instant::now();
    for _ in 0..ITERS {
        black_box(eval(black_box(&expr), ctx).unwrap());
    }
    let interpreted = start.elapsed();

    // Native tier: the one-time compile, then the evaluations.
    let start = Instant::now();
    let program = compile_program(&[&expr]).expect("compile the formula natively");
    let compile = start.elapsed();
    let start = Instant::now();
    for _ in 0..ITERS {
        black_box(program.eval(0, ctx).unwrap());
    }
    let native = start.elapsed();

    let interp_ns = interpreted.as_nanos() as f64 / f64::from(ITERS);
    let native_ns = native.as_nanos() as f64 / f64::from(ITERS);

    println!("{label}");
    println!("  formula     : {formula}");
    println!("  interpreted : {interp_ns:8.1} ns / eval");
    println!("  native      : {native_ns:8.1} ns / eval");
    println!("  speedup     : {:.2}x", interp_ns / native_ns);
    compile
}

fn main() {
    let ctx = MapCtx::from_pairs(&[
        ("A1", CellValue::Number(10.0)),
        ("A2", CellValue::Number(2.5)),
        ("A3", CellValue::Number(7.0)),
        ("B1", CellValue::Number(3.0)),
        ("B2", CellValue::Number(100.0)),
    ]);
    println!("iterations   : {ITERS}\n");

    // Cell-bound: every leaf is a cell read, a cost both paths share.
    let cold = bench(
        "[cell-bound]",
        "((A1 + A2) * B1 - A3) / B2 + A1 * A2 - A3 ^ 2",
        &ctx,
    );
    println!();
    // Constant-bearing: `(1 + 0.07) ^ 5` and `(1 + 0.07) ^ 3` are pure
    // constants — v9 folds each to one literal at transpile time, so the
    // native path never recomputes the compound-growth factors.
    let warm = bench(
        "[constant-bearing]",
        "A1 * (1 + 0.07) ^ 5 + A2 * (1 + 0.07) ^ 3 - B1",
        &ctx,
    );

    println!(
        "\ncompile cost : {:.0} ms total, two formulas (one-time; \
         ~12 s on a cold cdylib build, second build reuses the cargo cache)",
        (cold + warm).as_secs_f64() * 1000.0,
    );
}

// ---------------------------------------------------------------------------
// Representative result (release, x86-64 Windows; ~10% run-to-run noise):
//
//   [cell-bound]        interpreted ~625 ns, native ~520 ns  (~1.2x)
//   [constant-bearing]  interpreted ~380 ns, native ~195 ns  (~2.0x)
//
// The cell-bound formula is dominated by `ctx.cell` dynamic dispatch and
// map lookups — work both paths share — so removing the AST-walk dispatch
// is a modest win. The constant-bearing formula is where v9 pays off: the
// transpiler folds the `(1 + 0.07) ^ n` growth factors to literals, so the
// native path does two multiplies, one add, one subtract and three cell
// reads, while the interpreter re-walks (and re-computes) the constant
// `Pow`/`Add` subtrees on every evaluation — around a 2x speedup.
// ---------------------------------------------------------------------------
