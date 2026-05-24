//! Property-based tests for `.tscl` persistence — v14.
//!
//! A spreadsheet that loses data when saved and reopened is worse than
//! useless, so the `save` → `open` round-trip is a correctness property
//! that must hold for *every* workbook, not just the one shape the
//! existing `save_and_open_preserves_dependencies` unit test covers.
//!
//! These tests generate random workbooks, drive them through
//! `WorkbookEngine::save` (→ `tescellate_store`) and `WorkbookEngine::open`
//! (← `tescellate_store`), and assert:
//!
//!  * every cell's source text and computed value survive the round-trip;
//!  * the reopened workbook's DAG is functional — `open` rebuilds the
//!    dependency graph by re-parsing sources, so a post-open edit must
//!    still propagate to dependents;
//!  * a multi-sheet workbook mixing a square and a hex lattice round-trips
//!    intact, lattice kinds included.
//!
//! Determinism is fixed-seed so a CI failure always reproduces.

use std::path::PathBuf;

use tescellate_core::{CellValue, SheetId};
use tescellate_formula::WorkbookEngine;
use tescellate_tess::LatticeKind;

/// Fixed seed — a generative CI gate must reproduce byte-for-byte.
const SEED: u64 = 0x7E5C_E11A_7E5D_0001;
/// How many random workbooks to generate per property.
const WORKBOOKS: usize = 60;

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

/// A small positive integer literal, `1..=9`.
fn lit(rng: &mut Rng) -> u32 {
    1 + rng.below(9)
}

/// A random workbook as `(address, source)` pairs. `A1` is a literal and
/// `A2` always references `A1` (so there is a guaranteed dependency to
/// exercise after reopening); later cells are literals or arithmetic over
/// strictly-earlier cells — acyclic, and free of array literals so no cell
/// spills.
fn gen_workbook(rng: &mut Rng) -> Vec<(String, String)> {
    let n = 6 + rng.below(15) as usize; // 6..=20 cells
    let mut cells = vec![
        ("A1".to_string(), format!("={}", lit(rng))),
        ("A2".to_string(), format!("=A1 + {}", lit(rng))),
    ];
    for i in 2..n {
        let addr = format!("A{}", i + 1);
        let src = if rng.below(3) == 0 {
            format!("={}", lit(rng))
        } else {
            let a = rng.below(i as u32) + 1;
            let b = rng.below(i as u32) + 1;
            let o = ["+", "-", "*"][rng.below(3) as usize];
            format!("=A{a} {o} A{b}")
        };
        cells.push((addr, src));
    }
    cells
}

/// Build a fresh single-sheet (square) engine with `cells` applied.
fn build(cells: &[(String, String)]) -> (WorkbookEngine, SheetId) {
    let mut eng = WorkbookEngine::new();
    eng.new_workbook();
    let sid = eng.add_sheet("Sheet1", LatticeKind::Square);
    for (addr, src) in cells {
        eng.set_cell(sid, addr, Some(src)).expect("set_cell");
    }
    (eng, sid)
}

/// A unique-per-call temp `.tscl` path.
fn temp_tscl(tag: &str, i: usize) -> PathBuf {
    std::env::temp_dir().join(format!(
        "tescellate-persist-{tag}-{}-{i}.tscl",
        std::process::id()
    ))
}

#[test]
fn save_open_round_trips_values_and_sources() {
    let mut rng = Rng::new(SEED);
    for w in 0..WORKBOOKS {
        let cells = gen_workbook(&mut rng);
        let (eng, sid) = build(&cells);

        // Capture every cell's source + value before the round-trip.
        let before: Vec<(String, Option<String>, CellValue)> = cells
            .iter()
            .map(|(addr, _)| {
                let snap = eng.get_cell(sid, addr).expect("cell exists");
                (addr.clone(), snap.source, snap.value)
            })
            .collect();

        let path = temp_tscl("roundtrip", w);
        eng.save(&path).expect("save");
        let mut reopened = WorkbookEngine::new();
        reopened.open(&path).expect("open");
        std::fs::remove_file(&path).ok();

        for (addr, src, val) in &before {
            let snap = reopened
                .get_cell(sid, addr)
                .unwrap_or_else(|| panic!("workbook {w}: cell {addr} missing after reopen"));
            assert_eq!(
                &snap.source, src,
                "workbook {w}: source of {addr} changed across save/open",
            );
            assert_eq!(
                &snap.value, val,
                "workbook {w}: value of {addr} changed across save/open",
            );
        }
    }
}

#[test]
fn reopened_workbook_has_a_working_dag() {
    let mut rng = Rng::new(SEED);
    for w in 0..WORKBOOKS {
        let cells = gen_workbook(&mut rng);
        let (eng, sid) = build(&cells);

        let path = temp_tscl("dag", w);
        eng.save(&path).expect("save");
        let mut reopened = WorkbookEngine::new();
        reopened.open(&path).expect("open");
        std::fs::remove_file(&path).ok();

        // `A2` always reads `A1`. `open` rebuilds the DAG from sources, so
        // editing `A1` must recompute `A2` in the reopened workbook.
        let a2_before = reopened.get_cell(sid, "A2").expect("A2 exists").value;
        let changed = reopened
            .set_cell(sid, "A1", Some("=999"))
            .expect("set_cell after reopen");
        assert!(
            changed.iter().any(|c| c.address == "A2"),
            "workbook {w}: editing A1 did not recompute A2 — DAG not rebuilt on open",
        );
        let a2_after = reopened.get_cell(sid, "A2").expect("A2 exists").value;
        assert_ne!(
            a2_before, a2_after,
            "workbook {w}: A2 value did not change after editing A1 post-reopen",
        );
    }
}

#[test]
fn multi_sheet_and_hex_lattice_round_trip() {
    // A workbook with two sheets on two different lattices.
    let mut eng = WorkbookEngine::new();
    eng.new_workbook();
    let sq = eng.add_sheet("Numbers", LatticeKind::Square);
    let hx = eng.add_sheet("HexGrid", LatticeKind::HexPointy);

    eng.set_cell(sq, "A1", Some("=10")).unwrap();
    eng.set_cell(sq, "A2", Some("=A1 * 3")).unwrap();
    eng.set_cell(hx, "H(0,0)", Some("=7")).unwrap();
    eng.set_cell(hx, "H(1,-1)", Some("=H(0,0) + 5")).unwrap();

    let path = temp_tscl("multisheet", 0);
    eng.save(&path).unwrap();
    let mut reopened = WorkbookEngine::new();
    reopened.open(&path).unwrap();
    std::fs::remove_file(&path).ok();

    // Both sheets and both lattices survive: the hex cells are only
    // addressable at all if the hex lattice round-tripped.
    assert_eq!(
        reopened.get_cell(sq, "A2").expect("square A2").value,
        CellValue::Number(30.0),
    );
    assert_eq!(
        reopened.get_cell(hx, "H(1,-1)").expect("hex H(1,-1)").value,
        CellValue::Number(12.0),
    );

    // The reopened hex sheet's DAG is functional too.
    reopened.set_cell(hx, "H(0,0)", Some("=100")).unwrap();
    assert_eq!(
        reopened.get_cell(hx, "H(1,-1)").expect("hex H(1,-1)").value,
        CellValue::Number(105.0),
        "editing H(0,0) did not recompute H(1,-1) after reopen",
    );
}
