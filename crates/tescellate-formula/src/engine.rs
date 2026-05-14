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
    Cell, CellError, CellRef, CellValue, Dag, EngineKind, Sheet, SheetId, Workbook, WorkbookId,
    WorkbookMeta,
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

    pub fn add_sheet(&mut self, name: impl Into<String>, lattice: LatticeKind) -> SheetId {
        let id = SheetId(self.workbook.sheets.len() as u32 + 1);
        let sheet = Sheet {
            id,
            name: name.into(),
            lattice,
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
        let cell = self.workbook.sheets.get(&sheet)?.cells.get(&canonical)?;
        Some(CellSnapshot {
            address: canonical,
            source: cell.source.clone(),
            value: cell.value.clone(),
        })
    }

    /// Bulk snapshot every populated cell whose canonical address falls in
    /// the rectangular range `start..=end` (square-lattice semantics).
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
        let mut out = Vec::new();
        for c in square_range(a, b) {
            let addr = lattice.address(c);
            if let Some(cell) = sheet_ref.cells.get(&addr) {
                out.push(CellSnapshot {
                    address: addr,
                    source: cell.source.clone(),
                    value: cell.value.clone(),
                });
            }
        }
        Ok(out)
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
}
