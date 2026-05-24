//! Phase 2 acceptance test: Conway's-Game-of-Life-on-hexes, single-step
//! recompute. Demonstrates the full Carbide hex stack — addressing, range
//! semantics, NEIGHBORS, DAG propagation — working end-to-end.
//!
//! Rule: **Bays' B2/S34** (the canonical hex variant of Conway's Life):
//!   - Birth: a dead cell with exactly 2 live neighbors becomes alive.
//!   - Survive: a live cell with 3 or 4 live neighbors stays alive.
//!   - All other cells die / stay dead.
//!
//! Initial pattern: a 3-cell line H(0,0), H(1,0), H(2,0). After one step
//! all three line cells die (under-stimulated) and four new cells are born
//! in the parallel positions H(1,-1), H(0,1), H(2,-1), H(1,1).

use carbide_core::CellValue;
use carbide_formula::WorkbookEngine;
use carbide_tess::LatticeKind;

/// Live cell value. Encoding as 1.0 / 0.0 lets `SUM(NEIGHBORS(...))` give
/// the live-neighbor count directly.
const ALIVE: f64 = 1.0;
const DEAD: f64 = 0.0;

/// Per-cell next-state formula in Carbide. Reads the cell's own value
/// (alive/dead) and the live-neighbor count.
fn step_formula(addr: &str) -> String {
    format!("=LET(n, SUM(NEIGHBORS({addr})), IF({addr}=1, IF(AND(n>=3,n<=4),1,0), IF(n=2,1,0)))")
}

#[test]
fn hex_conway_b2s34_single_step_3cell_line() {
    let mut eng = WorkbookEngine::new();
    eng.new_workbook();
    let sid = eng.add_sheet("Life", LatticeKind::HexPointy);

    // Initial state: 3-cell horizontal line.
    let line = ["H(0,0)", "H(1,0)", "H(2,0)"];
    for addr in line {
        eng.set_cell(sid, addr, Some(&ALIVE.to_string())).unwrap();
    }

    // Compute next state for every cell that could change.
    //
    // The change region under B2/S34 is the union of:
    //   - every live cell (S34 may kill it),
    //   - every dead cell adjacent to a live cell (B2 may birth it).
    //
    // For a 3-cell line that's the 3 live cells plus the 12 surrounding
    // dead cells (3 cells × 6 neighbors, minus the 2 cells they share as
    // neighbors of each other, leaves 16 neighbor-slots that resolve to
    // 12 distinct addresses). Enumerate them by hand:

    #[rustfmt::skip]
    let next_cells = [
        // Same row as the line (all die).
        ("H(0,0)",  DEAD),  // alive, 1 neighbor → dies
        ("H(1,0)",  DEAD),  // alive, 2 neighbors → dies (S34 needs 3-4)
        ("H(2,0)",  DEAD),  // alive, 1 neighbor → dies
        // "Above" row (negative r). 2 cells born.
        ("H(0,-1)", DEAD),  // dead, 1 neighbor → stays dead
        ("H(1,-1)", ALIVE), // dead, 2 neighbors → BIRTH
        ("H(2,-1)", ALIVE), // dead, 2 neighbors → BIRTH
        ("H(3,-1)", DEAD),  // dead, 1 neighbor → stays dead
        // "Below" row (positive r). 2 cells born.
        ("H(-1,1)", DEAD),  // dead, 1 neighbor → stays dead
        ("H(0,1)",  ALIVE), // dead, 2 neighbors → BIRTH
        ("H(1,1)",  ALIVE), // dead, 2 neighbors → BIRTH
        ("H(2,1)",  DEAD),  // dead, 1 neighbor → stays dead
        // Extreme west/east neighbors of the line.
        ("H(-1,0)", DEAD),  // dead, 1 neighbor → stays dead
        ("H(3,0)",  DEAD),  // dead, 1 neighbor → stays dead
    ];

    // Place each cell's next-state formula in a parallel "next" sheet
    // offset by r += 100, so we read the original state without writing
    // over it. Same lattice; addresses just live in a different region.
    for (addr, _expected) in next_cells.iter() {
        // Parse out (q, r) and shift r down by 100 for the formula cell.
        let (q, r) = parse_h(addr);
        let formula_addr = format!("H({q},{})", r + 100);
        let formula = step_formula(addr);
        eng.set_cell(sid, &formula_addr, Some(&formula))
            .unwrap_or_else(|e| panic!("set {formula_addr}: {formula}: {e}"));
    }

    // Assert: each "next" cell's value matches the expected state.
    for (addr, expected) in next_cells.iter() {
        let (q, r) = parse_h(addr);
        let formula_addr = format!("H({q},{})", r + 100);
        let v = eng.get_cell(sid, &formula_addr).unwrap().value;
        assert_eq!(
            v,
            CellValue::Number(*expected),
            "wrong next state at {addr} (expected {expected}, got {v:?})"
        );
    }
}

#[test]
fn hex_conway_dag_propagates_through_step() {
    // Confirm the DAG hooks up so that flipping one initial cell
    // re-evaluates every dependent next-state cell.
    let mut eng = WorkbookEngine::new();
    eng.new_workbook();
    let sid = eng.add_sheet("LifeDAG", LatticeKind::HexPointy);

    eng.set_cell(sid, "H(0,0)", Some(&ALIVE.to_string()))
        .unwrap();
    eng.set_cell(sid, "H(1,0)", Some(&ALIVE.to_string()))
        .unwrap();
    // Compute next state at H(1,-1): two live neighbors → birth.
    eng.set_cell(sid, "H(1,99)", Some(&step_formula("H(1,-1)")))
        .unwrap();
    assert_eq!(
        eng.get_cell(sid, "H(1,99)").unwrap().value,
        CellValue::Number(ALIVE)
    );

    // Kill H(0,0). Now H(1,-1) has 1 live neighbor → stays dead.
    eng.set_cell(sid, "H(0,0)", Some(&DEAD.to_string()))
        .unwrap();
    assert_eq!(
        eng.get_cell(sid, "H(1,99)").unwrap().value,
        CellValue::Number(DEAD),
        "DAG didn't propagate H(0,0) edit through NEIGHBORS"
    );
}

#[test]
fn hex_conway_lambda_form_runs() {
    // The same rule, expressed via LET + LAMBDA so the test exercises
    // the lambda machinery alongside NEIGHBORS. Address is passed as
    // text so the lambda can be applied at arbitrary cells.
    let mut eng = WorkbookEngine::new();
    eng.new_workbook();
    let sid = eng.add_sheet("LifeLambda", LatticeKind::HexPointy);

    eng.set_cell(sid, "H(0,0)", Some(&ALIVE.to_string()))
        .unwrap();
    eng.set_cell(sid, "H(1,0)", Some(&ALIVE.to_string()))
        .unwrap();
    eng.set_cell(sid, "H(2,0)", Some(&ALIVE.to_string()))
        .unwrap();

    // Apply the rule with a parameterized lambda. Pass the address as a
    // string so NEIGHBORS resolves it via the text-arg path.
    eng.set_cell(
        sid,
        "H(99,99)",
        Some(
            r#"=LET(
                live, LAMBDA(a, SUM(NEIGHBORS(a))),
                next, LAMBDA(a, IF(SUM(NEIGHBORS(a))=2, 1, 0)),
                next("H(1,-1)")
            )"#,
        ),
    )
    .unwrap();
    assert_eq!(
        eng.get_cell(sid, "H(99,99)").unwrap().value,
        CellValue::Number(ALIVE)
    );
}

/// Tiny helper for the test: pull (q, r) out of an "H(q,r)" string.
fn parse_h(s: &str) -> (i32, i32) {
    let inner = s
        .strip_prefix("H(")
        .and_then(|s| s.strip_suffix(')'))
        .unwrap();
    let (q, r) = inner.split_once(',').unwrap();
    (q.parse().unwrap(), r.parse().unwrap())
}
