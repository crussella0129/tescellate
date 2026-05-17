//! Generative differential test — the behavior gate, v10 + v11.
//!
//! A seeded generator builds random Carbide ASTs, and every one must
//! transpile-and-compile to the same result the tree-walking interpreter
//! produces — value, type, and error alike. It explores an input space no
//! hand-written corpus could enumerate, and is the strongest check on the
//! transpiler's correctness-by-construction claim.
//!
//! v10 fuzzed the first-order surface. v11 adds the *higher-order* surface
//! — `LET`, `LAMBDA`, bound-variable references and applications, `MAP`,
//! `REDUCE` — the most intricate part of the transpiler, previously only
//! corpus-tested. The generator tracks a lexical scope so every `Var` it
//! emits is bound, and uses a monotonic counter for fresh, collision-free
//! binding names (so no generated name ever shadows another).
//!
//! Two invariants keep the test sound:
//!  * **No lambda-valued results.** Lambdas are emitted only into consumed
//!    positions (a `LET` binding, a `MAP`/`REDUCE` argument, an immediately
//!    -applied callee) — never returned by `expr` — so a formula's result
//!    is always an ordinary `CellValue`, which `{:?}`-compares cleanly.
//!  * **Depth-bounded.** Every form generates its children within a
//!    strictly smaller budget, so `expr(MAX_DEPTH)` provably yields a tree
//!    of depth `<= MAX_DEPTH` (see `generated_trees_respect_the_depth_bound`).
//!
//! `LETREC` and `Apply`-of-parenthesised-lambda are corpus-covered; the
//! generator targets the compositional `LET`/`LAMBDA`/`MAP`/`REDUCE`
//! surface plus immediately-invoked lambdas. Determinism is fixed-seed so a
//! CI failure always reproduces.

mod common;

use tescellate_formula::excellite::ast::{BinaryOp, Expr, UnaryOp};

/// Fixed seed — a generative CI gate must reproduce byte-for-byte.
const SEED: u64 = 0x853C_49E6_748F_EA9B;
/// How many random formulas to generate. Higher-order formulas serialize
/// to more code than v10's first-order ones, so this is lower than v10's
/// count; all are still batched into one crate.
const GENERATED: usize = 180;
/// Maximum AST depth. `expr` provably never exceeds it.
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

    /// A value in `0..n`. `n` must be non-zero.
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

/// Leaf numbers — a small spread including zero, negatives, and fractions.
const NUMBERS: [f64; 10] = [0.0, 1.0, 2.0, 3.0, 4.0, -1.0, 0.5, 2.5, 10.0, 100.0];

/// Leaf strings — empty, plain text, and numeric-looking text to exercise
/// the `to_number` string-coercion path.
const STRINGS: [&str; 6] = ["", "a", "ab", "hello", "5", "3.5"];

/// Leaf cell addresses — `CELLS` from `common`, plus `Z9` (an empty cell).
const CELL_REFS: [&str; 7] = ["A1", "A2", "A3", "B1", "B2", "C1", "Z9"];

/// First-order registry functions, with `(name, min_args, max_args)`.
/// Higher-order forms (`LET`/`LAMBDA`/`MAP`/`REDUCE`) are built by the
/// dedicated methods below, not drawn from this table.
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

/// A name bound in the lexical scope the generator tracks.
#[derive(Clone)]
struct Binding {
    name: String,
    /// `Some(arity)` if bound to a lambda; `None` if bound to a value.
    lambda_arity: Option<u32>,
}

/// The generator: a PRNG plus a monotonic counter for fresh binding names.
struct Gen {
    rng: Rng,
    fresh: u32,
}

impl Gen {
    fn new(seed: u64) -> Self {
        Gen {
            rng: Rng::new(seed),
            fresh: 0,
        }
    }

    /// A fresh, collision-free identifier — lowercase, so it can never
    /// collide with an (uppercase) registry function name.
    fn fresh_name(&mut self, prefix: &str) -> String {
        self.fresh += 1;
        format!("{prefix}{}", self.fresh)
    }

    /// Generate an `Expr` of depth `<= depth`, well-scoped against `scope`.
    /// Never returns a lambda-valued expression — see the module docs.
    fn expr(&mut self, depth: u32, scope: &[Binding]) -> Expr {
        if depth == 0 || self.rng.below(100) < 40 {
            return self.leaf(scope);
        }
        // The arm weights lean toward higher-order forms — that is v11's
        // focus. `map`/`reduce` and the immediately-invoked lambda embed a
        // `LAMBDA` node, which itself needs a level, so arms 16..=19 (the
        // ones that build them) are only on the menu at `depth >= 2`.
        let arms = if depth >= 2 { 20 } else { 16 };
        match self.rng.below(arms) {
            0..=3 => Expr::Binary(
                *self.rng.pick(&BINOPS),
                Box::new(self.expr(depth - 1, scope)),
                Box::new(self.expr(depth - 1, scope)),
            ),
            4 => Expr::Unary(
                *self.rng.pick(&UNOPS),
                Box::new(self.expr(depth - 1, scope)),
            ),
            5..=6 => self.call(depth, scope),
            7 => self.array(depth, scope),
            8..=13 => self.let_form(depth, scope),
            14..=15 => self.apply_bound(depth, scope),
            16..=17 => self.map_or_reduce(depth, scope),
            _ => self.iife(depth, scope),
        }
    }

    fn leaf(&mut self, scope: &[Binding]) -> Expr {
        // ~30% a bound *value* variable when one is in scope. Lambda-typed
        // bindings are never read as bare `Var`s — that would yield a
        // lambda-valued result; they are only ever *applied* (see
        // `apply_bound`).
        let values: Vec<&str> = scope
            .iter()
            .filter(|b| b.lambda_arity.is_none())
            .map(|b| b.name.as_str())
            .collect();
        if !values.is_empty() && self.rng.below(10) < 3 {
            return Expr::Var((*self.rng.pick(&values)).to_string());
        }
        match self.rng.below(4) {
            0 => Expr::Number(*self.rng.pick(&NUMBERS)),
            1 => Expr::Str((*self.rng.pick(&STRINGS)).to_string()),
            2 => Expr::Bool(self.rng.below(2) == 0),
            _ => Expr::CellRef((*self.rng.pick(&CELL_REFS)).to_string()),
        }
    }

    /// A first-order registry call, e.g. `SUM(..)`.
    fn call(&mut self, depth: u32, scope: &[Binding]) -> Expr {
        let &(name, lo, hi) = self.rng.pick(FUNCS);
        let n = lo + self.rng.below(hi - lo + 1);
        let args = (0..n).map(|_| self.expr(depth - 1, scope)).collect();
        Expr::Call(name.to_string(), args)
    }

    fn array(&mut self, depth: u32, scope: &[Binding]) -> Expr {
        if self.rng.below(2) == 0 {
            let n = 1 + self.rng.below(3);
            let row = (0..n).map(|_| self.expr(depth - 1, scope)).collect();
            Expr::Array(vec![row])
        } else {
            Expr::Array(vec![
                vec![self.expr(depth - 1, scope), self.expr(depth - 1, scope)],
                vec![self.expr(depth - 1, scope), self.expr(depth - 1, scope)],
            ])
        }
    }

    /// A `LAMBDA(p1, .., body)` of `arity` parameters. `body_budget` caps
    /// the body's depth, so the returned node has depth `<= 1 + body_budget`.
    /// Parameters enter the body's scope as value bindings.
    fn lambda(&mut self, body_budget: u32, scope: &[Binding], arity: u32) -> Expr {
        let params: Vec<String> = (0..arity).map(|_| self.fresh_name("p")).collect();
        let mut inner = scope.to_vec();
        for p in &params {
            inner.push(Binding {
                name: p.clone(),
                lambda_arity: None,
            });
        }
        let body = self.expr(body_budget, &inner);
        let mut args: Vec<Expr> = params.into_iter().map(Expr::Var).collect();
        args.push(body);
        Expr::Call("LAMBDA".to_string(), args)
    }

    /// A `LET(name, value, body)`. The binding is a lambda when the depth
    /// budget has room for one (`depth >= 2`); otherwise a plain value. The
    /// body sees `name` in scope.
    fn let_form(&mut self, depth: u32, scope: &[Binding]) -> Expr {
        let name = self.fresh_name("v");
        let lambda_binding = depth >= 2 && self.rng.below(2) == 0;
        let (val, arity) = if lambda_binding {
            let arity = 1 + self.rng.below(2);
            (self.lambda(depth - 2, scope, arity), Some(arity))
        } else {
            (self.expr(depth - 1, scope), None)
        };
        let mut inner = scope.to_vec();
        inner.push(Binding {
            name: name.clone(),
            lambda_arity: arity,
        });
        let body = self.expr(depth - 1, &inner);
        Expr::Call("LET".to_string(), vec![Expr::Var(name), val, body])
    }

    /// `MAP(array, lambda/1)` or `REDUCE(init, array, lambda/2)`. Only
    /// reached at `depth >= 2`.
    fn map_or_reduce(&mut self, depth: u32, scope: &[Binding]) -> Expr {
        let arr = self.array(depth - 1, scope);
        if self.rng.below(2) == 0 {
            let lam = self.lambda(depth - 2, scope, 1);
            Expr::Call("MAP".to_string(), vec![arr, lam])
        } else {
            let init = self.expr(depth - 1, scope);
            let lam = self.lambda(depth - 2, scope, 2);
            Expr::Call("REDUCE".to_string(), vec![init, arr, lam])
        }
    }

    /// An immediately-invoked lambda: `(LAMBDA(..))(args)`, lowered through
    /// `Expr::Apply`. Only reached at `depth >= 2`.
    fn iife(&mut self, depth: u32, scope: &[Binding]) -> Expr {
        let arity = 1 + self.rng.below(2);
        let lam = self.lambda(depth - 2, scope, arity);
        let args = (0..arity).map(|_| self.expr(depth - 1, scope)).collect();
        Expr::Apply(Box::new(lam), args)
    }

    /// Apply an in-scope lambda binding by name — `Call(name, args)`, which
    /// the registry resolves through its `Call`→`Var` fallback. When no
    /// lambda is in scope there is nothing to apply, so fall back to a
    /// binary expression.
    fn apply_bound(&mut self, depth: u32, scope: &[Binding]) -> Expr {
        let lambdas: Vec<(String, u32)> = scope
            .iter()
            .filter_map(|b| b.lambda_arity.map(|a| (b.name.clone(), a)))
            .collect();
        if lambdas.is_empty() {
            return Expr::Binary(
                *self.rng.pick(&BINOPS),
                Box::new(self.expr(depth - 1, scope)),
                Box::new(self.expr(depth - 1, scope)),
            );
        }
        let (name, arity) = self.rng.pick(&lambdas).clone();
        let args = (0..arity).map(|_| self.expr(depth - 1, scope)).collect();
        Expr::Call(name, args)
    }
}

/// Whether `e` contains a higher-order construct anywhere — a `LET`,
/// `LAMBDA`, `MAP`, `REDUCE`, or an `Apply`. Used to confirm the generator
/// is actually exercising the v11 surface, not degenerating to leaves.
fn contains_higher_order(e: &Expr) -> bool {
    match e {
        Expr::Apply(..) => true,
        Expr::Call(name, args) => {
            matches!(name.as_str(), "LET" | "LAMBDA" | "MAP" | "REDUCE")
                || args.iter().any(contains_higher_order)
        }
        Expr::Binary(_, l, r) => contains_higher_order(l) || contains_higher_order(r),
        Expr::Unary(_, inner) => contains_higher_order(inner),
        Expr::Array(rows) => rows.iter().flatten().any(contains_higher_order),
        _ => false,
    }
}

/// Actual depth of an `Expr` — for the depth-bound invariant check. The
/// `Apply` arm counts the callee, not only the arguments.
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
        Expr::Call(_, args) => 1 + args.iter().map(depth_of).max().unwrap_or(0),
        Expr::Apply(callee, args) => {
            let args_depth = args.iter().map(depth_of).max().unwrap_or(0);
            1 + depth_of(callee).max(args_depth)
        }
        Expr::Array(rows) => 1 + rows.iter().flatten().map(depth_of).max().unwrap_or(0),
    }
}

#[test]
fn generated_carbide_matches_interpreter() {
    let mut gen = Gen::new(SEED);
    let cases: Vec<(String, Expr)> = (0..GENERATED)
        .map(|_| {
            let e = gen.expr(MAX_DEPTH, &[]);
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
        let mut gen = Gen::new(SEED);
        (0..GENERATED)
            .map(|_| format!("{:?}", gen.expr(MAX_DEPTH, &[])))
            .collect::<Vec<_>>()
    };
    assert_eq!(run(), run());
}

#[test]
fn generator_exercises_higher_order_forms() {
    // A differential test over generated formulas is only as good as the
    // generator's reach — guard against it silently collapsing to a
    // first-order (or all-leaf) generator.
    let mut gen = Gen::new(SEED);
    let higher_order = (0..GENERATED)
        .filter(|_| contains_higher_order(&gen.expr(MAX_DEPTH, &[])))
        .count();
    assert!(
        higher_order >= GENERATED * 2 / 5,
        "only {higher_order}/{GENERATED} generated formulas were higher-order"
    );
}

#[test]
fn generated_trees_respect_the_depth_bound() {
    // A depth-bound bug would generate runaway formulas and blow up the
    // generated crate's compile time — guard the invariant directly.
    let mut gen = Gen::new(SEED);
    for _ in 0..GENERATED {
        let e = gen.expr(MAX_DEPTH, &[]);
        assert!(
            depth_of(&e) <= MAX_DEPTH,
            "generated tree exceeded MAX_DEPTH ({MAX_DEPTH}): {e:?}"
        );
    }
}
