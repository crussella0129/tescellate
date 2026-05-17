//! Property-based tests for the DAG recompute engine — v12.
//!
//! `WorkbookEngine` recomputes incrementally: editing a cell dirties its
//! transitive dependents, topologically orders that dirty set, and
//! re-evaluates it. The existing engine tests check this on a handful of
//! fixed shapes; this file stresses it generatively.
//!
//! ## The order-independence property
//!
//! For an *acyclic* workbook the final value of every cell is fully
//! determined by the formulas — the order the cells were entered in cannot
//! matter. So: generate a random acyclic workbook, evaluate it once in
//! topological order (the oracle — each cell is final the moment it is
//! set, so no incremental recompute happens), then re-evaluate it under
//! several scrambled edit orders. A scrambled order forces cells to be set
//! *before* their dependencies — computing a provisional value that must
//! later be corrected by dirty-propagation — so the final values match the
//! oracle only if dirty-closure, topological ordering, and recompute are
//! all correct. A dropped dirty cell or a mis-ordered evaluation leaves a
//! stale value, and the comparison catches it.
//!
//! Workbooks are acyclic *by construction*: cell `A{i+1}` may reference
//! only cells `A1..A{i}`. That keeps the property clean — with a cycle the
//! engine breaks the loop at whichever edge closes it first, which *is*
//! order-dependent, so cycles are tested separately.
//!
//! Determinism is fixed-seed so a CI failure always reproduces.

use tescellate_core::{CellError, CellValue};
use tescellate_formula::WorkbookEngine;
use tescellate_tess::LatticeKind;

/// Fixed seed — a generative CI gate must reproduce byte-for-byte.
const SEED: u64 = 0x6C0F_FEE5_1234_ABCD;
/// How many random workbooks to generate.
const WORKBOOKS: usize = 100;

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

    /// A value in `0..n`. `n` must be non-zero.
    fn below(&mut self, n: u32) -> u32 {
        (self.next() % u64::from(n)) as u32
    }
}

/// An address `A{1..=max}` — a reference to an earlier cell.
fn reff(rng: &mut Rng, max: u32) -> String {
    format!("A{}", rng.below(max) + 1)
}

/// A small positive integer literal, `1..=9`.
fn lit(rng: &mut Rng) -> u32 {
    1 + rng.below(9)
}

fn op(rng: &mut Rng) -> &'static str {
    ["+", "-", "*"][rng.below(3) as usize]
}

/// A formula for cell index `i` (`i >= 1`), referencing only cells
/// `A1..A{i}` — so the dependency graph is acyclic by construction.
fn gen_formula(rng: &mut Rng, i: usize) -> String {
    let max = i as u32; // refs are A1..A{i}
    match rng.below(6) {
        0 => format!("={}", reff(rng, max)),
        1 => format!("=-{}", reff(rng, max)),
        2 => {
            let a = reff(rng, max);
            format!("={} + {}", a, lit(rng))
        }
        3 => {
            let a = reff(rng, max);
            let o = op(rng);
            let b = reff(rng, max);
            format!("={a} {o} {b}")
        }
        4 => {
            let a = reff(rng, max);
            let b = reff(rng, max);
            format!("={} * {} - {}", a, b, lit(rng))
        }
        _ => {
            let a = reff(rng, max);
            let b = reff(rng, max);
            let c = reff(rng, max);
            format!("=SUM({a}, {b}, {c})")
        }
    }
}

/// A random acyclic workbook as `(address, source)` pairs in index order.
/// Cell 0 is always a literal; later cells are literals (~1 in 4) or
/// formulas over strictly-earlier cells.
fn gen_workbook(rng: &mut Rng) -> Vec<(String, String)> {
    let n = 8 + rng.below(17) as usize; // 8..=24 cells
    (0..n)
        .map(|i| {
            let addr = format!("A{}", i + 1);
            let source = if i == 0 || rng.below(4) == 0 {
                format!("={}", lit(rng))
            } else {
                gen_formula(rng, i)
            };
            (addr, source)
        })
        .collect()
}

/// A Fisher-Yates shuffle of `0..n`.
fn shuffled(rng: &mut Rng, n: usize) -> Vec<usize> {
    let mut v: Vec<usize> = (0..n).collect();
    for i in (1..n).rev() {
        let j = rng.below((i + 1) as u32) as usize;
        v.swap(i, j);
    }
    v
}

/// Build a fresh engine, apply `cells` in the edit order given by `order`
/// (a permutation of cell indices), and return every cell's final value in
/// index order. Acyclic workbooks never error from `set_cell`.
fn run(cells: &[(String, String)], order: &[usize]) -> Vec<CellValue> {
    let mut eng = WorkbookEngine::new();
    eng.new_workbook();
    let sid = eng.add_sheet("Sheet1", LatticeKind::Square);
    for &idx in order {
        let (addr, src) = &cells[idx];
        eng.set_cell(sid, addr, Some(src))
            .expect("set_cell on an acyclic workbook never errors");
    }
    // Read back in index order — independent of the edit order above.
    cells
        .iter()
        .map(|(addr, _)| eng.get_cell(sid, addr).expect("cell exists").value)
        .collect()
}

#[test]
fn recompute_is_order_independent() {
    let mut rng = Rng::new(SEED);
    for w in 0..WORKBOOKS {
        let cells = gen_workbook(&mut rng);
        let n = cells.len();

        // Oracle: edits applied in topological (index) order. Each cell is
        // final the moment it is set, so incremental recompute never runs.
        let oracle = run(&cells, &(0..n).collect::<Vec<_>>());

        // Variants: reverse order plus random shuffles. These force cells
        // to be set before their dependencies, exercising the full
        // dirty-closure → topo-order → recompute pipeline.
        let mut orders: Vec<Vec<usize>> = vec![(0..n).rev().collect()];
        for _ in 0..3 {
            orders.push(shuffled(&mut rng, n));
        }
        for order in &orders {
            let got = run(&cells, order);
            assert_eq!(
                got, oracle,
                "workbook {w}: recompute diverged under edit order {order:?}\n\
                 cells: {cells:?}"
            );
        }
    }
}

#[test]
fn generated_workbooks_have_dependencies() {
    // The order-independence property is only meaningful if cells actually
    // depend on one another — guard against a degenerate (all-literal)
    // generator. A formula carrying a dependency contains a cell address,
    // so its source contains an `A`; a bare literal (`=7`) does not.
    let mut rng = Rng::new(SEED);
    let mut with_deps = 0usize;
    let mut total = 0usize;
    for _ in 0..WORKBOOKS {
        let cells = gen_workbook(&mut rng);
        total += cells.len();
        with_deps += cells.iter().filter(|(_, s)| s.contains('A')).count();
    }
    assert!(
        with_deps * 2 >= total,
        "only {with_deps}/{total} generated cells carry a dependency"
    );
}

#[test]
fn cycles_are_detected_without_hanging() {
    // A dependency cycle of length `k`: A1→A2→…→A{k}→A1. The engine must
    // catch it at the edge that closes the loop and flag that cell rather
    // than recursing forever — so this test simply *completing* proves
    // there is no infinite loop, and the assertion proves the flag.
    for &k in &[2usize, 3, 5, 8, 12] {
        let mut eng = WorkbookEngine::new();
        eng.new_workbook();
        let sid = eng.add_sheet("Sheet1", LatticeKind::Square);
        for i in 1..k {
            let src = format!("=A{}", i + 1);
            eng.set_cell(sid, &format!("A{i}"), Some(&src)).unwrap();
        }
        // The closing edge. `set_cell` still returns `Ok`: the cycle is
        // caught internally and the cell is stored with a `Cycle` error.
        eng.set_cell(sid, &format!("A{k}"), Some("=A1")).unwrap();
        let closing = eng.get_cell(sid, &format!("A{k}")).unwrap().value;
        assert!(
            matches!(closing, CellValue::Error(CellError::Cycle)),
            "k={k}: expected the cycle-closing cell to be flagged, got {closing:?}"
        );
    }

    // A self-reference is the degenerate 1-cycle.
    let mut eng = WorkbookEngine::new();
    eng.new_workbook();
    let sid = eng.add_sheet("Sheet1", LatticeKind::Square);
    eng.set_cell(sid, "A1", Some("=A1 + 1")).unwrap();
    assert!(matches!(
        eng.get_cell(sid, "A1").unwrap().value,
        CellValue::Error(CellError::Cycle)
    ));
}
