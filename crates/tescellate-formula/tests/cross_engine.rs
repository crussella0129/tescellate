//! Cross-engine differential test — v17.
//!
//! Tescellate's defining promise is a *switchable per-cell formula
//! language*: the same computation can be written in Carbide (the
//! Excel-lite engine) or in Python, and a cell does not care which. That
//! promise only holds if the two engines *agree*. The Carbide engine is
//! exhaustively tested (corpus + two fuzzers); the Python engine (v8) is
//! the least-covered. This test pins them against each other.
//!
//! It generates random abstract arithmetic/comparison expressions and
//! renders each one twice — once as a Carbide formula, once as a Python
//! expression — then evaluates both over the same cell context and
//! asserts the results agree.
//!
//! Two facts shape the design:
//!  * **Carbide is all-`f64`; Python distinguishes `int` and `float`.**
//!    `2 + 2` is `Number(4.0)` in Carbide but `Integer(4)` in Python — not
//!    a bug. Results are therefore compared *numerically*, not by variant.
//!  * **Precedence differs** — notably `-2 ** 2` is `-4` in Python but
//!    `(-2) ^ 2 = 4` in Carbide. The renderer fully parenthesizes every
//!    node, so the abstract tree's structure — not either language's
//!    precedence — decides evaluation order. Comparison operands avoid the
//!    power operator (the one non-IEEE-exact op) so a ULP-level difference
//!    can never flip a boolean result.
//!
//! Feature-gated (`python`); determinism is fixed-seed.

#![cfg(feature = "python")]

use tescellate_core::CellValue;
use tescellate_formula::excellite::eval::eval;
use tescellate_formula::excellite::parse::parse;
use tescellate_formula::python::eval_python_with_ctx;
use tescellate_formula::transpile::MapCtx;

/// Fixed seed — a generative CI gate must reproduce byte-for-byte.
const SEED: u64 = 0xC0FF_EE57_17C7_055E;
/// Random expressions generated.
const GENERATED: usize = 200;
/// Maximum abstract-expression depth.
const MAX_DEPTH: u32 = 3;

/// The cell table both engines read. Carbide sees `f64`s; the Python
/// bridge hands each cell to the formula as a `float`.
const CELLS: &[(&str, f64)] = &[
    ("A1", 10.0),
    ("A2", 2.5),
    ("A3", 7.0),
    ("A4", -3.0),
    ("A5", 4.0),
    ("A6", 0.5),
];

/// Non-negative leaf literals — negatives are produced by `Neg` nodes, so
/// a literal never needs a leading `-`.
const LEAF_NUMS: [f64; 7] = [0.0, 1.0, 2.0, 3.0, 5.0, 0.5, 2.5];

/// xorshift64 — a tiny, dependency-free, deterministic PRNG.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
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

    fn below(&mut self, n: u32) -> u32 {
        (self.next() % u64::from(n)) as u32
    }
}

#[derive(Clone, Copy)]
enum Arith {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
}

#[derive(Clone, Copy)]
enum Cmp {
    Lt,
    Gt,
    Le,
    Ge,
    Eq,
    Ne,
}

/// An abstract expression — rendered into both Carbide and Python source.
enum Ex {
    Num(f64),
    /// Index into `CELLS`.
    Cell(usize),
    Neg(Box<Ex>),
    Bin(Arith, Box<Ex>, Box<Ex>),
    Compare(Cmp, Box<Ex>, Box<Ex>),
}

/// Generate an arithmetic expression (numeric result). When `allow_pow` is
/// false the power operator is excluded — used for comparison operands, so
/// only IEEE-exact ops (`+ - * /`) feed a boolean and a ULP difference can
/// never flip the result.
fn gen_arith(rng: &mut Rng, depth: u32, allow_pow: bool) -> Ex {
    if depth == 0 || rng.below(100) < 42 {
        return if rng.below(2) == 0 {
            Ex::Num(LEAF_NUMS[rng.below(LEAF_NUMS.len() as u32) as usize])
        } else {
            Ex::Cell(rng.below(CELLS.len() as u32) as usize)
        };
    }
    match rng.below(if allow_pow { 12 } else { 10 }) {
        0..=1 => Ex::Neg(Box::new(gen_arith(rng, depth - 1, allow_pow))),
        2..=3 => bin(rng, Arith::Add, depth, allow_pow),
        4..=5 => bin(rng, Arith::Sub, depth, allow_pow),
        6..=7 => bin(rng, Arith::Mul, depth, allow_pow),
        8..=9 => {
            // The divisor is a non-zero literal, so a generated formula
            // never hits a divide-by-zero in one engine but not the other.
            let lhs = gen_arith(rng, depth - 1, allow_pow);
            let divisor = [1.0, 2.0, 4.0, 5.0][rng.below(4) as usize];
            Ex::Bin(Arith::Div, Box::new(lhs), Box::new(Ex::Num(divisor)))
        }
        _ => {
            // Power with a small non-negative integer exponent — always a
            // finite real, in both languages, for these small bases.
            let base = gen_arith(rng, depth - 1, allow_pow);
            let exp = f64::from(rng.below(4)); // 0..=3
            Ex::Bin(Arith::Pow, Box::new(base), Box::new(Ex::Num(exp)))
        }
    }
}

fn bin(rng: &mut Rng, op: Arith, depth: u32, allow_pow: bool) -> Ex {
    Ex::Bin(
        op,
        Box::new(gen_arith(rng, depth - 1, allow_pow)),
        Box::new(gen_arith(rng, depth - 1, allow_pow)),
    )
}

/// Generate a whole formula — usually arithmetic, sometimes a single
/// comparison of two power-free arithmetic operands.
fn gen_formula(rng: &mut Rng) -> Ex {
    if rng.below(10) < 3 {
        let op = [Cmp::Lt, Cmp::Gt, Cmp::Le, Cmp::Ge, Cmp::Eq, Cmp::Ne][rng.below(6) as usize];
        Ex::Compare(
            op,
            Box::new(gen_arith(rng, MAX_DEPTH - 1, false)),
            Box::new(gen_arith(rng, MAX_DEPTH - 1, false)),
        )
    } else {
        gen_arith(rng, MAX_DEPTH, true)
    }
}

#[derive(Clone, Copy)]
enum Lang {
    Carbide,
    Python,
}

/// Render `e` fully parenthesized in `lang`. Full parenthesization means
/// the two languages' differing precedence and associativity never come
/// into play — the abstract tree alone fixes the evaluation order.
fn render(e: &Ex, lang: Lang) -> String {
    match e {
        Ex::Num(n) => {
            if n.fract() == 0.0 {
                format!("{}", *n as i64)
            } else {
                format!("{n}")
            }
        }
        Ex::Cell(i) => match lang {
            Lang::Carbide => CELLS[*i].0.to_string(),
            Lang::Python => format!("ctx.cell('{}')", CELLS[*i].0),
        },
        Ex::Neg(inner) => format!("(-{})", render(inner, lang)),
        Ex::Bin(op, l, r) => {
            let sym = match (op, lang) {
                (Arith::Add, _) => "+",
                (Arith::Sub, _) => "-",
                (Arith::Mul, _) => "*",
                (Arith::Div, _) => "/",
                (Arith::Pow, Lang::Carbide) => "^",
                (Arith::Pow, Lang::Python) => "**",
            };
            format!("({} {} {})", render(l, lang), sym, render(r, lang))
        }
        Ex::Compare(op, l, r) => {
            let sym = match (op, lang) {
                (Cmp::Lt, _) => "<",
                (Cmp::Gt, _) => ">",
                (Cmp::Le, _) => "<=",
                (Cmp::Ge, _) => ">=",
                (Cmp::Eq, Lang::Carbide) => "=",
                (Cmp::Eq, Lang::Python) => "==",
                (Cmp::Ne, Lang::Carbide) => "<>",
                (Cmp::Ne, Lang::Python) => "!=",
            };
            format!("({} {} {})", render(l, lang), sym, render(r, lang))
        }
    }
}

/// The numeric value of a `CellValue`, bridging Carbide's all-`f64` model
/// and Python's `int`/`float` distinction.
fn as_number(v: &CellValue) -> Option<f64> {
    match v {
        CellValue::Number(n) => Some(*n),
        CellValue::Integer(i) => Some(*i as f64),
        _ => None,
    }
}

/// Whether the two engines' results agree. Numbers compare with a small
/// relative tolerance (the power operator routes through `f64::powf`,
/// which is not guaranteed correctly-rounded); booleans, text, and empties
/// compare exactly; and two errors count as agreement (neither engine
/// produced a value).
fn agree(
    carbide: &Result<CellValue, impl std::fmt::Debug>,
    python: &Result<CellValue, impl std::fmt::Debug>,
) -> bool {
    match (carbide, python) {
        (Ok(c), Ok(p)) => match (c, p) {
            (CellValue::Bool(a), CellValue::Bool(b)) => a == b,
            (CellValue::Text(a), CellValue::Text(b)) => a == b,
            (CellValue::Empty, CellValue::Empty) => true,
            _ => match (as_number(c), as_number(p)) {
                (Some(a), Some(b)) => (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1.0),
                _ => false,
            },
        },
        (Err(_), Err(_)) => true,
        _ => false,
    }
}

fn context() -> MapCtx {
    let pairs: Vec<(&str, CellValue)> = CELLS
        .iter()
        .map(|(addr, v)| (*addr, CellValue::Number(*v)))
        .collect();
    MapCtx::from_pairs(&pairs)
}

#[test]
fn carbide_and_python_engines_agree() {
    let ctx = context();
    let mut rng = Rng::new(SEED);
    let mut mismatches = Vec::new();

    for _ in 0..GENERATED {
        let e = gen_formula(&mut rng);
        let carbide_src = render(&e, Lang::Carbide);
        let python_src = render(&e, Lang::Python);

        let carbide = parse(&carbide_src)
            .map_err(|err| format!("parse: {err}"))
            .and_then(|ast| eval(&ast, &ctx).map_err(|err| format!("eval: {err:?}")));
        let python = eval_python_with_ctx(&python_src, &ctx);

        if !agree(&carbide, &python) {
            mismatches.push(format!(
                "  carbide `{carbide_src}` => {carbide:?}\n  python  `{python_src}` => {python:?}",
            ));
        }
    }

    assert!(
        mismatches.is_empty(),
        "the Carbide and Python engines disagreed on {} of {GENERATED} formulas:\n{}",
        mismatches.len(),
        mismatches.join("\n"),
    );
}

#[test]
fn generator_is_deterministic() {
    let run = || {
        let mut rng = Rng::new(SEED);
        (0..GENERATED)
            .map(|_| render(&gen_formula(&mut rng), Lang::Carbide))
            .collect::<Vec<_>>()
    };
    assert_eq!(run(), run(), "the generator must reproduce byte-for-byte");
}

#[test]
fn generator_exercises_the_cross_engine_surface() {
    // The differential is only meaningful if the generator actually
    // produces cell reads, comparisons, and the power operator — guard
    // against it collapsing to a trivial literal generator.
    let mut rng = Rng::new(SEED);
    let (mut cells, mut comparisons, mut powers) = (0, 0, 0);
    for _ in 0..GENERATED {
        let carbide = render(&gen_formula(&mut rng), Lang::Carbide);
        if carbide.contains('A') {
            cells += 1;
        }
        if carbide.contains('<') || carbide.contains('>') || carbide.contains(" = ") {
            comparisons += 1;
        }
        if carbide.contains('^') {
            powers += 1;
        }
    }
    assert!(cells >= 40, "too few formulas read cells: {cells}");
    assert!(
        comparisons >= 15,
        "too few comparison formulas: {comparisons}"
    );
    assert!(powers >= 15, "too few power-operator formulas: {powers}");
}
