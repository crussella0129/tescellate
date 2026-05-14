//! `WorkbookEngine` — the orchestrator that owns the workbook, the DAG, the
//! compiled-formula cache, and the registered formula engines. Drives the
//! parse → dep-collection → recompute loop.
//!
//! Phase 1: single-sheet, square-only. The orchestrator is structured so
//! that adding hex/triangle sheets in Phase 2+ is a matter of branching on
//! `Sheet.lattice` when resolving addresses; the rest of this file already
//! works lattice-agnostically.

use crate::excellite::{eval_error_to_cell_error, ExcelLite};
use crate::{CompiledFormula, EvalCtx, EvalError, FormulaEngine};
use hashbrown::HashMap;
use tescellate_core::{
    Cell, CellError, CellRef, CellValue, Dag, EngineKind, Sheet, SheetExtent, SheetId, Workbook,
    WorkbookId, WorkbookMeta,
};
use tescellate_tess::square::{SquareCoord, SquareLattice};
use tescellate_tess::{Lattice, LatticeKind};

pub struct WorkbookEngine {
    pub workbook: Workbook,
    dag: Dag,
    compiled: HashMap<CellRef, CompiledFormula>,
    engines: HashMap<EngineKind, Box<dyn FormulaEngine>>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct CellSnapshot {
    pub address: String,
    pub source: Option<String>,
    pub value: CellValue,
    /// When set, this snapshot is for a virtual cell painted by another
    /// cell's spilled array. The renderer styles it differently and the
    /// formula bar shows the source cell's formula. PLAN.md §6.2.2.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub spilled_from: Option<String>,
}

impl Default for WorkbookEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkbookEngine {
    pub fn new() -> Self {
        let mut engines: HashMap<EngineKind, Box<dyn FormulaEngine>> = HashMap::new();
        engines.insert(EngineKind::ExcelLite, Box::new(ExcelLite));
        Self {
            workbook: empty_workbook(),
            dag: Dag::new(),
            compiled: HashMap::new(),
            engines,
        }
    }

    pub fn new_workbook(&mut self) -> WorkbookId {
        self.workbook = empty_workbook();
        self.dag = Dag::new();
        self.compiled.clear();
        self.workbook.id
    }

    /// Evaluate a standalone Excel-lite expression — no cell-ref resolution,
    /// no workbook context. Used by wizard inputs that accept arithmetic
    /// formulas (e.g. cell-count fields like `100*100`).
    pub fn eval_literal(&self, source: &str) -> Result<CellValue, SetCellError> {
        let body = source.strip_prefix('=').unwrap_or(source);
        let engine = self
            .engines
            .get(&EngineKind::ExcelLite)
            .ok_or(SetCellError::NoEngine(EngineKind::ExcelLite))?;
        let compiled = engine
            .parse(body)
            .map_err(|e| SetCellError::Parse(e.to_string()))?;
        struct Empty;
        impl EvalCtx for Empty {
            fn cell(&self, _: &str) -> Result<CellValue, EvalError> {
                Ok(CellValue::Empty)
            }
            fn range(&self, _: &str, _: &str) -> Result<Vec<CellValue>, EvalError> {
                Ok(Vec::new())
            }
        }
        engine
            .eval(&compiled, &Empty)
            .map_err(|e| SetCellError::Parse(e.to_string()))
    }

    /// Persist the workbook to a `.tscl` zip at `path`. The DAG itself
    /// isn't stored — it can be reconstructed from cell sources on load.
    pub fn save(&self, path: &std::path::Path) -> Result<(), SetCellError> {
        let file = std::fs::File::create(path).map_err(|e| SetCellError::Io(e.to_string()))?;
        tescellate_store::save(&self.workbook, file).map_err(|e| SetCellError::Io(e.to_string()))
    }

    /// Load a workbook from a `.tscl` file. Rebuilds the in-memory DAG by
    /// re-parsing every cell's source; trusts the persisted values so we
    /// don't have to re-evaluate just to display them.
    pub fn open(&mut self, path: &std::path::Path) -> Result<(), SetCellError> {
        let file = std::fs::File::open(path).map_err(|e| SetCellError::Io(e.to_string()))?;
        let workbook = tescellate_store::load(file).map_err(|e| SetCellError::Io(e.to_string()))?;
        self.workbook = workbook;
        self.dag = Dag::new();
        self.compiled.clear();
        self.rebuild_dag()?;
        Ok(())
    }

    fn rebuild_dag(&mut self) -> Result<(), SetCellError> {
        // Snapshot the (sheet, addr, source) tuples we need to re-parse so
        // we can mutate self.dag / self.compiled without aliasing the
        // workbook. Iterate by sheet to keep the lattice handle local.
        let entries: Vec<(SheetId, String, String, EngineKind)> = self
            .workbook
            .sheets
            .values()
            .flat_map(|sheet| {
                let default_engine = self.workbook.default_engine;
                sheet.cells.iter().filter_map(move |(addr, cell)| {
                    let src = cell.source.clone()?;
                    let engine = cell.engine.unwrap_or(default_engine);
                    Some((sheet.id, addr.clone(), src, engine))
                })
            })
            .collect();

        for (sheet_id, addr, source, engine_kind) in entries {
            let lattice = self.square_lattice_for(sheet_id)?;
            let engine = self
                .engines
                .get(&engine_kind)
                .ok_or(SetCellError::NoEngine(engine_kind))?;
            let compiled = match engine.parse(&source) {
                Ok(c) => c,
                Err(_) => continue, // skip cells with stale parse errors; they keep their saved value
            };
            let refs = engine.refs(&compiled);
            let cref = CellRef::new(sheet_id, addr.clone());

            let mut deps = Vec::new();
            for (start, end) in &refs {
                if let Some(end) = end {
                    let a = match lattice.parse_address(start) {
                        Ok(c) => c,
                        Err(_) => continue,
                    };
                    let b = match lattice.parse_address(end) {
                        Ok(c) => c,
                        Err(_) => continue,
                    };
                    for c in square_range(a, b) {
                        deps.push(CellRef::new(sheet_id, lattice.address(c)));
                    }
                } else {
                    let c = match lattice.parse_address(start) {
                        Ok(c) => c,
                        Err(_) => continue,
                    };
                    deps.push(CellRef::new(sheet_id, lattice.address(c)));
                }
            }
            // Cycles in saved data shouldn't have been possible at save time,
            // but be defensive: skip cells that would introduce one.
            if self.dag.set_deps(&cref, deps).is_err() {
                continue;
            }
            self.compiled.insert(cref, compiled);
        }
        Ok(())
    }

    pub fn add_sheet(&mut self, name: impl Into<String>, lattice: LatticeKind) -> SheetId {
        self.add_sheet_with_extent(name, lattice, SheetExtent::Unbounded)
    }

    pub fn add_sheet_with_extent(
        &mut self,
        name: impl Into<String>,
        lattice: LatticeKind,
        extent: SheetExtent,
    ) -> SheetId {
        let id = SheetId(self.workbook.sheets.len() as u32 + 1);
        let sheet = Sheet {
            id,
            name: name.into(),
            lattice,
            extent,
            cells: HashMap::new(),
        };
        self.workbook.sheets.insert(id, sheet);
        self.workbook.sheet_order.push(id);
        id
    }

    /// Apply a new formula to a cell. Returns the list of cells (including
    /// the edited one) whose values changed and should be redrawn.
    pub fn set_cell(
        &mut self,
        sheet: SheetId,
        addr: &str,
        source: Option<&str>,
    ) -> Result<Vec<CellRef>, SetCellError> {
        let lattice = self.square_lattice_for(sheet)?;
        let coord = lattice
            .parse_address(addr)
            .map_err(|e| SetCellError::BadAddress(format!("{e}")))?;
        // Bounds check before any mutation.
        if let Some(sheet_ref) = self.workbook.sheets.get(&sheet) {
            if !sheet_ref.extent.contains_square(coord.col, coord.row) {
                return Err(SetCellError::OutOfBounds(addr.to_string()));
            }
        }
        let canonical = lattice.address(coord);
        let cref = CellRef::new(sheet, canonical.clone());

        // Parse and collect deps.
        let (compiled, dep_addrs) = match source {
            Some(src) if !src.trim().is_empty() => {
                let engine_kind = self
                    .workbook
                    .sheets
                    .get(&sheet)
                    .and_then(|s| s.cells.get(&canonical))
                    .and_then(|c| c.engine)
                    .unwrap_or(self.workbook.default_engine);
                let engine = self
                    .engines
                    .get(&engine_kind)
                    .ok_or(SetCellError::NoEngine(engine_kind))?;
                let compiled = engine
                    .parse(src)
                    .map_err(|e| SetCellError::Parse(e.to_string()))?;
                let refs = engine.refs(&compiled);
                (Some(compiled), refs)
            }
            _ => (None, Vec::new()),
        };

        // Resolve dep addresses through the lattice to canonical CellRefs.
        let mut deps: Vec<CellRef> = Vec::new();
        for (start, end) in &dep_addrs {
            if let Some(end) = end {
                let a = lattice
                    .parse_address(start)
                    .map_err(|e| SetCellError::BadAddress(format!("{e}")))?;
                let b = lattice
                    .parse_address(end)
                    .map_err(|e| SetCellError::BadAddress(format!("{e}")))?;
                for c in square_range(a, b) {
                    deps.push(CellRef::new(sheet, lattice.address(c)));
                }
            } else {
                let c = lattice
                    .parse_address(start)
                    .map_err(|e| SetCellError::BadAddress(format!("{e}")))?;
                deps.push(CellRef::new(sheet, lattice.address(c)));
            }
        }

        // Update DAG, rolling back the formula on cycle.
        let cycle = self.dag.set_deps(&cref, deps.clone()).is_err();
        if cycle {
            // Drop any prior compiled form; mark the cell as cyclic.
            self.compiled.remove(&cref);
            self.store_cell(
                sheet,
                &canonical,
                Cell {
                    source: source.map(|s| s.to_string()),
                    engine: None,
                    value: CellValue::Error(CellError::Cycle),
                },
            );
            return Ok(vec![cref]);
        }

        // Persist source / compiled form.
        if let Some(compiled) = compiled {
            self.compiled.insert(cref.clone(), compiled);
        } else {
            self.compiled.remove(&cref);
        }
        self.store_cell(
            sheet,
            &canonical,
            Cell {
                source: source.filter(|s| !s.trim().is_empty()).map(String::from),
                engine: None,
                value: CellValue::Empty,
            },
        );

        // Recompute the dirty closure.
        let dirty = self.dag.dirty_closure(&cref);
        let order = self.dag.topo_order(dirty.clone());
        let mut changed = Vec::with_capacity(order.len());
        for c in &order {
            self.recompute(c);
            changed.push(c.clone());
        }
        // Any node not topologically reachable (orphan cycles, etc.) still
        // needs a refresh. Recompute remaining dirty nodes in arbitrary order.
        for c in &dirty {
            if !order.contains(c) {
                self.recompute(c);
                changed.push(c.clone());
            }
        }
        Ok(changed)
    }

    pub fn get_cell(&self, sheet: SheetId, addr: &str) -> Option<CellSnapshot> {
        let lattice = self.square_lattice_for(sheet).ok()?;
        let coord = lattice.parse_address(addr).ok()?;
        let canonical = lattice.address(coord);
        let spill = self.compute_spill_for(sheet);
        // Source cell?
        if let Some(cell) = self.workbook.sheets.get(&sheet)?.cells.get(&canonical) {
            let value = if spill.collisions.contains(&canonical) {
                CellValue::Error(CellError::Spill)
            } else {
                cell.value.clone()
            };
            return Some(CellSnapshot {
                address: canonical,
                source: cell.source.clone(),
                value,
                spilled_from: None,
            });
        }
        // Virtual spill cell?
        if let Some(virt) = spill.virtual_cells.get(&canonical) {
            return Some(CellSnapshot {
                address: canonical,
                source: None,
                value: virt.value.clone(),
                spilled_from: Some(virt.source.clone()),
            });
        }
        None
    }

    /// Bulk snapshot the requested range. Emits stored cells, the source
    /// cells' #SPILL! state when they collide, and the virtual spilled
    /// cells whose source happens to paint into the range.
    pub fn snapshot_range(
        &self,
        sheet: SheetId,
        start: &str,
        end: &str,
    ) -> Result<Vec<CellSnapshot>, SetCellError> {
        let lattice = self.square_lattice_for(sheet)?;
        let a = lattice
            .parse_address(start)
            .map_err(|e| SetCellError::BadAddress(format!("{e}")))?;
        let b = lattice
            .parse_address(end)
            .map_err(|e| SetCellError::BadAddress(format!("{e}")))?;
        let sheet_ref = self
            .workbook
            .sheets
            .get(&sheet)
            .ok_or(SetCellError::NoSheet(sheet))?;
        let spill = self.compute_spill_for(sheet);
        let mut out = Vec::new();
        for c in square_range(a, b) {
            let addr = lattice.address(c);
            if let Some(cell) = sheet_ref.cells.get(&addr) {
                let value = if spill.collisions.contains(&addr) {
                    CellValue::Error(CellError::Spill)
                } else {
                    cell.value.clone()
                };
                out.push(CellSnapshot {
                    address: addr,
                    source: cell.source.clone(),
                    value,
                    spilled_from: None,
                });
            } else if let Some(virt) = spill.virtual_cells.get(&addr) {
                out.push(CellSnapshot {
                    address: addr,
                    source: None,
                    value: virt.value.clone(),
                    spilled_from: Some(virt.source.clone()),
                });
            }
        }
        Ok(out)
    }

    /// Walk every cell on the sheet whose value is an Array of size > 1×1
    /// and project it into the spill map: each non-source target becomes a
    /// virtual cell, or — if the target is occupied — the source flips to
    /// `#SPILL!` and no virtual cells are emitted.
    fn compute_spill_for(&self, sheet: SheetId) -> SpillMap {
        let mut out = SpillMap::default();
        let lattice = match self.square_lattice_for(sheet) {
            Ok(l) => l,
            Err(_) => return out,
        };
        let sheet_ref = match self.workbook.sheets.get(&sheet) {
            Some(s) => s,
            None => return out,
        };
        for (addr, cell) in &sheet_ref.cells {
            let arr = match &cell.value {
                CellValue::Array(arr) if !arr.is_scalar() && !arr.is_empty() => arr,
                _ => continue,
            };
            let src_coord = match lattice.parse_address(addr) {
                Ok(c) => c,
                Err(_) => continue,
            };

            // Collect (offset, target_addr) pairs; check collisions.
            let mut targets = Vec::with_capacity(arr.len());
            let mut collision = false;
            for r in 0..arr.rows {
                for c in 0..arr.cols {
                    if r == 0 && c == 0 {
                        continue;
                    }
                    let target = SquareCoord {
                        col: src_coord.col + c as i32,
                        row: src_coord.row + r as i32,
                    };
                    let target_addr = lattice.address(target);
                    // Collision: target is already a stored cell with a source
                    // (a stored cell with no source — i.e. just a value left
                    // over from clearing a formula — is treated as empty for
                    // spill purposes).
                    if let Some(existing) = sheet_ref.cells.get(&target_addr) {
                        if existing.source.is_some() {
                            collision = true;
                            break;
                        }
                    }
                    let value = arr.get(r, c).cloned().unwrap_or(CellValue::Empty);
                    targets.push((target_addr, value));
                }
                if collision {
                    break;
                }
            }
            if collision {
                out.collisions.insert(addr.clone());
            } else {
                for (taddr, value) in targets {
                    out.virtual_cells.insert(
                        taddr,
                        VirtualCell {
                            value,
                            source: addr.clone(),
                        },
                    );
                }
            }
        }
        out
    }

    fn store_cell(&mut self, sheet: SheetId, addr: &str, cell: Cell) {
        if let Some(s) = self.workbook.sheets.get_mut(&sheet) {
            s.cells.insert(addr.to_string(), cell);
        }
    }

    fn recompute(&mut self, cref: &CellRef) {
        // No formula → value already represents user-typed literal or blank.
        let compiled = match self.compiled.get(cref) {
            Some(c) => c.clone(),
            None => return,
        };
        let engine_kind = self
            .workbook
            .sheets
            .get(&cref.sheet)
            .and_then(|s| s.cells.get(&cref.address))
            .and_then(|c| c.engine)
            .unwrap_or(self.workbook.default_engine);
        let engine = match self.engines.get(&engine_kind) {
            Some(e) => e,
            None => return,
        };

        let value = {
            let view = SheetEvalView {
                sheet: cref.sheet,
                workbook: &self.workbook,
                lattice: SquareLattice::default(),
            };
            engine
                .eval(&compiled, &view)
                .unwrap_or_else(|e| CellValue::Error(eval_error_to_cell_error(e)))
        };
        if let Some(sheet) = self.workbook.sheets.get_mut(&cref.sheet) {
            if let Some(cell) = sheet.cells.get_mut(&cref.address) {
                cell.value = value;
            }
        }
    }

    fn square_lattice_for(&self, sheet: SheetId) -> Result<SquareLattice, SetCellError> {
        let s = self
            .workbook
            .sheets
            .get(&sheet)
            .ok_or(SetCellError::NoSheet(sheet))?;
        match s.lattice {
            LatticeKind::Square => Ok(SquareLattice::default()),
            other => Err(SetCellError::UnsupportedLattice(other)),
        }
    }
}

struct SheetEvalView<'a> {
    sheet: SheetId,
    workbook: &'a Workbook,
    lattice: SquareLattice,
}

impl EvalCtx for SheetEvalView<'_> {
    fn cell(&self, addr: &str) -> Result<CellValue, EvalError> {
        let coord = self
            .lattice
            .parse_address(addr)
            .map_err(|e| EvalError::Ref(format!("{e}")))?;
        let canon = self.lattice.address(coord);
        Ok(self
            .workbook
            .sheets
            .get(&self.sheet)
            .and_then(|s| s.cells.get(&canon))
            .map(|c| c.value.clone())
            .unwrap_or(CellValue::Empty))
    }

    fn range(&self, start: &str, end: &str) -> Result<Vec<CellValue>, EvalError> {
        let a = self
            .lattice
            .parse_address(start)
            .map_err(|e| EvalError::Ref(format!("{e}")))?;
        let b = self
            .lattice
            .parse_address(end)
            .map_err(|e| EvalError::Ref(format!("{e}")))?;
        let mut out = Vec::new();
        for c in square_range(a, b) {
            let canon = self.lattice.address(c);
            let v = self
                .workbook
                .sheets
                .get(&self.sheet)
                .and_then(|s| s.cells.get(&canon))
                .map(|cell| cell.value.clone())
                .unwrap_or(CellValue::Empty);
            out.push(v);
        }
        Ok(out)
    }
}

fn square_range(a: SquareCoord, b: SquareCoord) -> Vec<SquareCoord> {
    let (c0, c1) = (a.col.min(b.col), a.col.max(b.col));
    let (r0, r1) = (a.row.min(b.row), a.row.max(b.row));
    let mut out = Vec::with_capacity(((c1 - c0 + 1) * (r1 - r0 + 1)) as usize);
    for r in r0..=r1 {
        for c in c0..=c1 {
            out.push(SquareCoord { col: c, row: r });
        }
    }
    out
}

#[derive(Default)]
struct SpillMap {
    /// Address → virtual cell painted by a source's array.
    virtual_cells: HashMap<String, VirtualCell>,
    /// Source addresses whose spill region collides with an occupied cell.
    collisions: std::collections::HashSet<String>,
}

struct VirtualCell {
    value: CellValue,
    source: String,
}

fn empty_workbook() -> Workbook {
    Workbook {
        id: WorkbookId(1),
        meta: WorkbookMeta {
            title: "untitled".into(),
            created_at: "1970-01-01T00:00:00Z".into(),
            format_version: 0,
        },
        default_engine: EngineKind::ExcelLite,
        sheet_order: Vec::new(),
        sheets: HashMap::new(),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SetCellError {
    #[error("no sheet {0:?}")]
    NoSheet(SheetId),
    #[error("no registered engine for {0:?}")]
    NoEngine(EngineKind),
    #[error("address: {0}")]
    BadAddress(String),
    #[error("parse: {0}")]
    Parse(String),
    #[error("lattice {0:?} not yet supported in Phase 1")]
    UnsupportedLattice(LatticeKind),
    #[error("io: {0}")]
    Io(String),
    #[error("address {0} is outside this sheet's bounds")]
    OutOfBounds(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_sheet() -> (WorkbookEngine, SheetId) {
        let mut eng = WorkbookEngine::new();
        eng.new_workbook();
        let sid = eng.add_sheet("Sheet1", LatticeKind::Square);
        (eng, sid)
    }

    #[test]
    fn literal_cell() {
        let (mut eng, sid) = new_sheet();
        eng.set_cell(sid, "A1", Some("=42")).unwrap();
        let cell = eng.get_cell(sid, "A1").unwrap();
        assert_eq!(cell.value, CellValue::Number(42.0));
    }

    #[test]
    fn sum_with_recompute() {
        let (mut eng, sid) = new_sheet();
        eng.set_cell(sid, "A1", Some("=10")).unwrap();
        eng.set_cell(sid, "A2", Some("=20")).unwrap();
        eng.set_cell(sid, "A3", Some("=SUM(A1:A2)")).unwrap();
        assert_eq!(
            eng.get_cell(sid, "A3").unwrap().value,
            CellValue::Number(30.0)
        );
        // Edit upstream and see downstream recompute.
        let changed = eng.set_cell(sid, "A1", Some("=100")).unwrap();
        assert!(changed.iter().any(|c| c.address == "A3"));
        assert_eq!(
            eng.get_cell(sid, "A3").unwrap().value,
            CellValue::Number(120.0)
        );
    }

    #[test]
    fn cycle_marks_cell_error() {
        let (mut eng, sid) = new_sheet();
        eng.set_cell(sid, "A1", Some("=B1+1")).unwrap();
        eng.set_cell(sid, "B1", Some("=A1+1")).unwrap();
        let v = eng.get_cell(sid, "B1").unwrap().value;
        assert!(matches!(v, CellValue::Error(CellError::Cycle)));
    }

    #[test]
    fn dependency_chain() {
        let (mut eng, sid) = new_sheet();
        eng.set_cell(sid, "A1", Some("=5")).unwrap();
        eng.set_cell(sid, "A2", Some("=A1*2")).unwrap();
        eng.set_cell(sid, "A3", Some("=A2+1")).unwrap();
        assert_eq!(
            eng.get_cell(sid, "A3").unwrap().value,
            CellValue::Number(11.0)
        );
        eng.set_cell(sid, "A1", Some("=100")).unwrap();
        assert_eq!(
            eng.get_cell(sid, "A3").unwrap().value,
            CellValue::Number(201.0)
        );
    }

    #[test]
    fn empty_cell_clears_formula() {
        let (mut eng, sid) = new_sheet();
        eng.set_cell(sid, "A1", Some("=10")).unwrap();
        eng.set_cell(sid, "A2", Some("=A1*2")).unwrap();
        eng.set_cell(sid, "A1", Some("")).unwrap();
        // A1 source cleared; A2 references empty cell → coerced to 0 in arithmetic.
        assert_eq!(
            eng.get_cell(sid, "A2").unwrap().value,
            CellValue::Number(0.0)
        );
    }

    #[test]
    fn snapshot_range_returns_populated_cells_only() {
        let (mut eng, sid) = new_sheet();
        eng.set_cell(sid, "A1", Some("=1")).unwrap();
        eng.set_cell(sid, "B2", Some("=2")).unwrap();
        let snap = eng.snapshot_range(sid, "A1", "C3").unwrap();
        assert_eq!(snap.len(), 2);
    }

    #[test]
    fn save_and_open_preserves_dependencies() {
        let (mut eng, sid) = new_sheet();
        eng.set_cell(sid, "A1", Some("=10")).unwrap();
        eng.set_cell(sid, "A2", Some("=A1*2")).unwrap();

        let tmp = std::env::temp_dir().join(format!(
            "tescellate-test-{}-{}.tscl",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        eng.save(&tmp).unwrap();

        let mut other = WorkbookEngine::new();
        other.open(&tmp).unwrap();
        std::fs::remove_file(&tmp).ok();

        // Saved value comes back as-is...
        assert_eq!(
            other.get_cell(sid, "A2").unwrap().value,
            CellValue::Number(20.0)
        );
        // ...and the DAG was rebuilt, so an upstream edit propagates.
        other.set_cell(sid, "A1", Some("=100")).unwrap();
        assert_eq!(
            other.get_cell(sid, "A2").unwrap().value,
            CellValue::Number(200.0)
        );
    }

    #[test]
    fn array_formula_spills_into_neighbours() {
        let (mut eng, sid) = new_sheet();
        eng.set_cell(sid, "A1", Some("=SEQUENCE(3)")).unwrap();
        // Source cell holds the array value...
        let snap_a1 = eng.get_cell(sid, "A1").unwrap();
        assert!(matches!(snap_a1.value, CellValue::Array(_)));
        assert!(snap_a1.spilled_from.is_none());
        // ...and A2/A3 are virtual spill targets.
        let snap_a2 = eng.get_cell(sid, "A2").unwrap();
        assert_eq!(snap_a2.value, CellValue::Number(2.0));
        assert_eq!(snap_a2.spilled_from.as_deref(), Some("A1"));
        let snap_a3 = eng.get_cell(sid, "A3").unwrap();
        assert_eq!(snap_a3.value, CellValue::Number(3.0));
        assert_eq!(snap_a3.spilled_from.as_deref(), Some("A1"));
        // A4 (outside the spill) is still nothing.
        assert!(eng.get_cell(sid, "A4").is_none());
    }

    #[test]
    fn spill_collision_marks_source_with_spill_error() {
        let (mut eng, sid) = new_sheet();
        eng.set_cell(sid, "A3", Some("=999")).unwrap(); // blocker
        eng.set_cell(sid, "A1", Some("=SEQUENCE(5)")).unwrap();
        let snap = eng.get_cell(sid, "A1").unwrap();
        assert_eq!(snap.value, CellValue::Error(CellError::Spill));
        // A2 doesn't exist as a virtual cell once collision is detected.
        assert!(eng.get_cell(sid, "A2").is_none());
        // The blocker is still itself.
        assert_eq!(
            eng.get_cell(sid, "A3").unwrap().value,
            CellValue::Number(999.0)
        );
    }

    #[test]
    fn spill_2d_paints_a_rectangle() {
        let (mut eng, sid) = new_sheet();
        eng.set_cell(sid, "A1", Some("=[[1,2],[3,4]]")).unwrap();
        let snap = eng.snapshot_range(sid, "A1", "B2").unwrap();
        let map: hashbrown::HashMap<_, _> =
            snap.into_iter().map(|s| (s.address.clone(), s)).collect();
        assert!(matches!(map["A1"].value, CellValue::Array(_)));
        assert_eq!(map["A2"].value, CellValue::Number(3.0));
        assert_eq!(map["B1"].value, CellValue::Number(2.0));
        assert_eq!(map["B2"].value, CellValue::Number(4.0));
        for (addr, snap) in &map {
            if addr == "A1" {
                assert!(snap.spilled_from.is_none());
            } else {
                assert_eq!(snap.spilled_from.as_deref(), Some("A1"));
            }
        }
    }
}
