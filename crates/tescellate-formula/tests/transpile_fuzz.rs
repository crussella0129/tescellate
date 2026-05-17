//! Generative differential test — v10's behavior gate.
//!
//! The hand-written corpus in `transpile_differential` proves the
//! transpiler on a fixed set of formulas. This test widens the gate: a
//! seeded generator builds hundreds of *random* Carbide ASTs, and every
//! one must transpile-and-compile to the same result the tree-walking
//! interpreter produces — value, type, and error alike.
//!
//! It is the strongest available check on v9's typed codegen and constant
//! folding: those paths are equivalent to the interpreter *by construction*
//! (one shared numeric kernel), and a generative differential test is how
//! that claim is verified across an input space no hand-written corpus
//! could enumerate.
//!
//! Determinism is non-negotiable for a CI gate — the generator is driven by
//! a fixed-seed xorshift PRNG, so a failure always reproduces exactly. The
//! generated surface is first-order (no `LET`/`LAMBDA`/`Var`/`Apply`):
//! generating *well-scoped* higher-order formulas needs scope tracking and
//! is left to a later version; the corpus covers those by hand.

mod common;

use tescellate_formula::excellite::ast::{BinaryOp, Expr, UnaryOp};

/// Fixed seed — a generative CI gate must reproduce byte-for-byte.
const SEED: u64 = 0x853C_49E6_748F_EA9B;
/// How many random formulas to generate. All are batched into one crate,
/// so this trades directly against the test's one-time compile cost.
const GENERATED: usize = 300;
/// Maximum AST depth. `gen_expr` provably never exceeds it (see
/// `generated_trees_respect_the_depth_bound`).
const MAX_DEPTH: u32 = 3;

/// xorshift64 — a tiny, dependency-free, deterministic PRNG. Adequate for
/// spreading test inputs; not for anything cryptographic.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        // xorshift64 degenerates to all-zero from a zero state.
        Rng(if seed == 0 { 0xDEAD_BEEF } else { seed })
    }

    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    /// A value in `0..n`. `n` must be non-zero (every caller passes a
    /// constant or a non-empty slice length).
    fn below(&mut self, n: u32) -> u32 {
        (self.next() % u64::from(n)) as u32
    }

    /// A uniformly-chosen element of a non-empty slice.
    fn pick<'a, T>(&mut self, xs: &'a [T]) -> &'a T {
        &xs[self.below(xs.len() as u32) as usize]
    }
}

const BINOPS: [BinaryOp; 12] = [
    BinaryOp::Add,
    BinaryOp::Sub,
    BinaryOp::Mul,
    BinaryOp::Div,
    BinaryOp::Pow,
    BinaryOp::Concat,
    BinaryOp::Eq,
    BinaryOp::NotEq,
    BinaryOp::Lt,
    BinaryOp::Gt,
    BinaryOp::LtEq,
    BinaryOp::GtEq,
];

const UNOPS: [UnaryOp; 2] = [UnaryOp::Neg, UnaryOp::Pos];

/// Leaf numbers — a small spread including zero (to provoke `#DIV/0!`),
/// negatives, and fractions.
const NUMBERS: [f64; 10] = [0.0, 1.0, 2.0, 3.0, 4.0, -1.0, 0.5, 2.5, 10.0, 100.0];

/// Leaf strings — empty, plain text, and numeric-looking text to exercise
/// the `to_number` string-coercion path.
const STRINGS: [&str; 6] = ["", "a", "ab", "hello", "5", "3.5"];

/// Leaf cell addresses — `CELLS` from `common`, plus `Z9` (an empty cell).
const CELL_REFS: [&str; 7] = ["A1", "A2", "A3", "B1", "B2", "C1", "Z9"];

/// Functions to call, with `(name, min_args, max_args)`. Every name is a
/// real registry function (unknown-function parity is corpus-covered);
/// arity is sometimes deliberately under/over-filled so `#VALUE!`-style
/// argument errors are exercised differentially too.
const FUNCS: &[(&str, u32, u32)] = &[
    ("SUM", 1, 3),
    ("AVERAGE", 1, 3),
    ("COUNT", 1, 3),
    ("MIN", 1, 3),
    ("MAX", 1, 3),
    ("AND", 2, 2),
    ("OR", 2, 2),
    ("NOT", 1, 1),
    ("IF", 3, 3),
    ("ABS", 1, 1),
    ("SQRT", 1, 1),
    ("ROUND", 2, 2),
    ("MOD", 2, 2),
    ("POWER", 2, 2),
    ("LEN", 1, 1),
    ("UPPER", 1, 1),
];

/// Generate one random `Expr` of depth `<= depth`.
fn gen_expr(rng: &mut Rng, depth: u32) -> Expr {
    // Force a leaf at the depth limit; below it, lean toward leaves so a
    // few hundred formulas stay cheap to compile in one crate.
    if depth == 0 || rng.below(100) < 40 {
        return gen_leaf(rng);
    }
    match rng.below(10) {
        0..=4 => Expr::Binary(
            *rng.pick(&BINOPS),
            Box::new(gen_expr(rng, depth - 1)),
            Box::new(gen_expr(rng, depth - 1)),
        ),
        5..=6 => Expr::Unary(*rng.pick(&UNOPS), Box::new(gen_expr(rng, depth - 1))),
        7..=8 => gen_call(rng, depth),
        _ => gen_array(rng, depth),
    }
}

fn gen_leaf(rng: &mut Rng) -> Expr {
    match rng.below(4) {
        0 => Expr::Number(*rng.pick(&NUMBERS)),
        1 => Expr::Str((*rng.pick(&STRINGS)).to_string()),
        2 => Expr::Bool(rng.below(2) == 0),
        _ => Expr::CellRef((*rng.pick(&CELL_REFS)).to_string()),
    }
}

fn gen_call(rng: &mut Rng, depth: u32) -> Expr {
    let &(name, lo, hi) = rng.pick(FUNCS);
    let n = lo + rng.below(hi - lo + 1);
    let args = (0..n).map(|_| gen_expr(rng, depth - 1)).collect();
    Expr::Call(name.to_string(), args)
}

fn gen_array(rng: &mut Rng, depth: u32) -> Expr {
    if rng.below(2) == 0 {
        // A 1-D row of 1..=3 elements.
        let n = 1 + rng.below(3);
        let row = (0..n).map(|_| gen_expr(rng, depth - 1)).collect();
        Expr::Array(vec![row])
    } else {
        // A 2x2 grid — `vec!` evaluates elements left-to-right, so the four
        // `gen_expr` draws happen in a fixed order (determinism).
        Expr::Array(vec![
            vec![gen_expr(rng, depth - 1), gen_expr(rng, depth - 1)],
            vec![gen_expr(rng, depth - 1), gen_expr(rng, depth - 1)],
        ])
    }
}

/// Actual depth of an `Expr` — for the depth-bound invariant check.
fn depth_of(e: &Expr) -> u32 {
    match e {
        Expr::Number(_)
        | Expr::Str(_)
        | Expr::Bool(_)
        | Expr::CellRef(_)
        | Expr::Range(_, _)
        | Expr::Var(_) => 0,
        Expr::Unary(_, inner) => 1 + depth_of(inner),
        Expr::Binary(_, l, r) => 1 + depth_of(l).max(depth_of(r)),
        Expr::Call(_, args) | Expr::Apply(_, args) => {
            1 + args.iter().map(depth_of).max().unwrap_or(0)
        }
        Expr::Array(rows) => 1 + rows.iter().flatten().map(depth_of).max().unwrap_or(0),
    }
}

#[test]
fn generated_carbide_matches_interpreter() {
    let mut rng = Rng::new(SEED);
    let cases: Vec<(String, Expr)> = (0..GENERATED)
        .map(|_| {
            let e = gen_expr(&mut rng, MAX_DEPTH);
            // The `{:?}` AST dump is the case label in a failure message.
            (format!("{e:?}"), e)
        })
        .collect();
    common::run_differential("tescellate_transpile_fuzz", &cases);
}

#[test]
fn generator_is_deterministic() {
    // Same seed → byte-identical formula stream. A generative CI gate is
    // only meaningful if a failure reproduces exactly.
    let run = || {
        let mut rng = Rng::new(SEED);
        (0..GENERATED)
            .map(|_| format!("{:?}", gen_expr(&mut rng, MAX_DEPTH)))
            .collect::<Vec<_>>()
    };
    assert_eq!(run(), run());
}

#[test]
fn generated_trees_respect_the_depth_bound() {
    // A depth-bound bug would generate runaway formulas and blow up the
    // generated crate's compile time — guard the invariant directly.
    let mut rng = Rng::new(SEED);
    for _ in 0..GENERATED {
        let e = gen_expr(&mut rng, MAX_DEPTH);
        assert!(
            depth_of(&e) <= MAX_DEPTH,
            "generated tree exceeded MAX_DEPTH ({MAX_DEPTH}): {e:?}"
        );
    }
}
