//! Scale / stress tests for the engine — v16.
//!
//! Every other test in the suite uses small inputs — workbooks of a few
//! dozen cells, formulas a few levels deep. None of them probe what
//! happens at size. This file exercises the engine far beyond realistic
//! use, to confirm nothing recurses unboundedly or degrades pathologically.
//!
//! The DAG algorithms — cycle detection, dirty-closure, topological order
//! (`carbide-core::dag`) — are all *iterative*, so a deep dependency
//! chain walks without growing the stack. The recursive tree-walks that
//! remain (`eval`, reference collection) recurse on a single formula's
//! *AST depth*, which the parser caps so a pathologically deep formula is
//! rejected rather than crashing — `deeply_nested_formulas_are_bounded`
//! exercises both sides of that limit.
//! These tests are also a coarse perf-regression guard: an accidental
//! quadratic in recompute would balloon them from milliseconds to a
//! visible stall.

use carbide_core::{CellValue, SheetId};
use carbide_formula::WorkbookEngine;
use carbide_tess::LatticeKind;

/// Length of the linear dependency chain.
const CHAIN_LEN: usize = 2000;
/// Number of direct dependents of a single root cell.
const FAN_OUT: usize = 3000;
/// Term count for a deeply-nested formula that stays within the parser's
/// nesting limit — it must evaluate normally.
const NESTED_OK: usize = 100;
/// Term count for a formula nested far past the limit — it must be
/// rejected with a parse error, not crash the process.
const NESTED_OVER: usize = 5000;
/// Cell count of the large save/open workbook.
const WORKBOOK_CELLS: usize = 2500;

fn fresh_sheet() -> (WorkbookEngine, SheetId) {
    let mut eng = WorkbookEngine::new();
    eng.new_workbook();
    let sid = eng.add_sheet("Sheet1", LatticeKind::Square);
    (eng, sid)
}

#[test]
fn deep_dependency_chain_recomputes() {
    let (mut eng, sid) = fresh_sheet();
    // A1 = 1; A_i = A_{i-1} + 1 — a chain CHAIN_LEN cells long. Recompute
    // must walk it iteratively (topological order), never recursively.
    eng.set_cell(sid, "A1", Some("=1")).unwrap();
    for i in 2..=CHAIN_LEN {
        let src = format!("=A{} + 1", i - 1);
        eng.set_cell(sid, &format!("A{i}"), Some(&src)).unwrap();
    }
    let tail = format!("A{CHAIN_LEN}");
    assert_eq!(
        eng.get_cell(sid, &tail).unwrap().value,
        CellValue::Number(CHAIN_LEN as f64),
        "tail of a fresh {CHAIN_LEN}-cell chain",
    );

    // Editing the root must propagate the entire length of the chain.
    eng.set_cell(sid, "A1", Some("=1000")).unwrap();
    assert_eq!(
        eng.get_cell(sid, &tail).unwrap().value,
        CellValue::Number((1000 + CHAIN_LEN - 1) as f64),
        "tail after editing the root of a {CHAIN_LEN}-cell chain",
    );
}

#[test]
fn wide_fan_out_recomputes() {
    let (mut eng, sid) = fresh_sheet();
    // One root with FAN_OUT direct dependents — a single edit dirties all.
    eng.set_cell(sid, "A1", Some("=5")).unwrap();
    for i in 2..=FAN_OUT + 1 {
        eng.set_cell(sid, &format!("A{i}"), Some("=A1 * 2"))
            .unwrap();
    }
    let last = format!("A{}", FAN_OUT + 1);
    assert_eq!(
        eng.get_cell(sid, &last).unwrap().value,
        CellValue::Number(10.0),
    );

    let changed = eng.set_cell(sid, "A1", Some("=50")).unwrap();
    assert!(
        changed.len() >= FAN_OUT,
        "editing the root should recompute all {FAN_OUT} dependents, got {}",
        changed.len(),
    );
    assert_eq!(
        eng.get_cell(sid, &last).unwrap().value,
        CellValue::Number(100.0),
    );
}

/// `=1 + 1 + ... + 1` with `terms` ones — an AST `terms` levels deep.
fn additive_formula(terms: usize) -> String {
    let mut src = String::from("=1");
    for _ in 1..terms {
        src.push_str(" + 1");
    }
    src
}

#[test]
fn deeply_nested_formulas_are_bounded() {
    let (mut eng, sid) = fresh_sheet();

    // A formula nested within the parser's depth limit evaluates fine —
    // `eval` walks the whole AST recursively without issue.
    eng.set_cell(sid, "A1", Some(&additive_formula(NESTED_OK)))
        .expect("a formula within the nesting limit must parse");
    assert_eq!(
        eng.get_cell(sid, "A1").unwrap().value,
        CellValue::Number(NESTED_OK as f64),
        "a {NESTED_OK}-term additive formula",
    );

    // A formula nested far past the limit is rejected with a parse error.
    // Without the parser's depth cap, evaluating (or even dropping) the
    // resulting tree would overflow the stack and abort the process.
    let result = eng.set_cell(sid, "A2", Some(&additive_formula(NESTED_OVER)));
    assert!(
        result.is_err(),
        "a {NESTED_OVER}-deep formula must be rejected, not crash the engine",
    );
}

#[test]
fn large_workbook_saves_and_reopens() {
    let (mut eng, sid) = fresh_sheet();
    // A workbook of WORKBOOK_CELLS cells: two literal roots, the rest
    // formulas over them (so values stay bounded and the build is linear).
    for i in 1..=WORKBOOK_CELLS {
        let src = if i <= 2 {
            format!("={i}")
        } else {
            format!("=A1 * {i} + A2")
        };
        eng.set_cell(sid, &format!("A{i}"), Some(&src)).unwrap();
    }

    let path = std::env::temp_dir().join(format!("carbide-scale-{}.crbd", std::process::id(),));
    eng.save(&path).expect("save a large workbook");
    let mut reopened = WorkbookEngine::new();
    reopened.open(&path).expect("reopen a large workbook");
    std::fs::remove_file(&path).ok();

    for i in [1usize, 2, 137, 1000, WORKBOOK_CELLS] {
        let addr = format!("A{i}");
        assert_eq!(
            eng.get_cell(sid, &addr).unwrap().value,
            reopened.get_cell(sid, &addr).unwrap().value,
            "cell {addr} differs across save/open of a {WORKBOOK_CELLS}-cell workbook",
        );
    }
}
