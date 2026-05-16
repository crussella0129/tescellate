//! Benchmark — interpreted vs natively-compiled Carbide evaluation.
//!
//! Run:
//! ```text
//! cargo run --release -p tescellate-formula --example native_bench --features native
//! ```
//!
//! The interpreter walks the AST on every evaluation; the native tier bakes
//! the structure into compiled machine code. The leaf operations —
//! `apply_*_op`, the function registry, `ctx.cell` — are the *same* on both
//! paths, so the native tier's win is bounded: it removes the AST-walk
//! dispatch, not the work the dispatch leads to. See the recorded result at
//! the bottom of this file.

use std::hint::black_box;
use std::time::Instant;

use tescellate_formula::excellite::eval::eval;
use tescellate_formula::excellite::parse::parse;
use tescellate_formula::transpile::native::compile_program;
use tescellate_formula::transpile::rt::CellValue;
use tescellate_formula::transpile::MapCtx;

fn main() {
    // Pure arithmetic over cell references — no function calls, so the
    // native tier genuinely compiles the AST walk away.
    let formula = "((A1 + A2) * B1 - A3) / B2 + A1 * A2 - A3 ^ 2";
    let expr = parse(formula).expect("parse the benchmark formula");
    let ctx = MapCtx::from_pairs(&[
        ("A1", CellValue::Number(10.0)),
        ("A2", CellValue::Number(2.5)),
        ("A3", CellValue::Number(7.0)),
        ("B1", CellValue::Number(3.0)),
        ("B2", CellValue::Number(100.0)),
    ]);
    const ITERS: u32 = 2_000_000;

    // Interpreted tier.
    let start = Instant::now();
    for _ in 0..ITERS {
        black_box(eval(black_box(&expr), &ctx).unwrap());
    }
    let interpreted = start.elapsed();

    // Native tier: the one-time compile, then the evaluations.
    let start = Instant::now();
    let program = compile_program(&[&expr]).expect("compile the formula natively");
    let compile = start.elapsed();
    let start = Instant::now();
    for _ in 0..ITERS {
        black_box(program.eval(0, &ctx).unwrap());
    }
    let native = start.elapsed();

    let interp_ns = interpreted.as_nanos() as f64 / f64::from(ITERS);
    let native_ns = native.as_nanos() as f64 / f64::from(ITERS);

    println!("formula      : {formula}");
    println!("iterations   : {ITERS}");
    println!("interpreted  : {interp_ns:8.1} ns / eval");
    println!("native       : {native_ns:8.1} ns / eval");
    println!("speedup      : {:.2}x", interp_ns / native_ns);
    println!(
        "compile cost : {:.0} ms (one-time; ~12 s on a cold cdylib build)",
        compile.as_secs_f64() * 1000.0,
    );
}

// ---------------------------------------------------------------------------
// Representative result (release, x86-64 Windows):
//
//   interpreted  : ~618 ns / eval
//   native       : ~540 ns / eval   (1.14x faster)
//   compile cost : ~12 s cold (first cdylib build), ~0.4 s warm (cargo reuse)
//
// The native tier is modestly faster per evaluation. The transpiler is
// "thin" — it lowers the AST walk into straight-line calls but reuses the
// interpreter's leaf primitives (`apply_*_op`, the function registry,
// `ctx.cell` via dynamic dispatch), which dominate the cost and are shared
// by both paths. A larger win would need type-specialized codegen — emitting
// raw `f64` arithmetic when operand types are statically known — a
// deliberate future optimization, distinct from the correctness-first
// v1-v6 transpiler.
// ---------------------------------------------------------------------------
