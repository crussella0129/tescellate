//! `WorkbookEngine` — the orchestrator that owns the workbook, the DAG, the
//! compiled-formula cache, and the registered formula engines. Drives the
//! parse → dep-collection → recompute loop.
//!
//! Lattice-agnostic at the engine layer: every address operation goes
//! through `LatticeHandle` (square + hex today; triangle / parallelogram
//! land in Phase 3). The eval view exposes `neighbors_of` / `cells_within`
//! so `NEIGHBORS()` and `RADIUS()` work uniformly regardless of lattice.

use crate::excellite::{eval_error_to_cell_error, ExcelLite};
use crate::{CompiledFormula, EvalCtx, EvalError, FormulaEngine, FormulaRef};
use hashbrown::HashMap;
use tescellate_core::{
    Cell, CellError, CellRef, CellValue, Dag, EngineKind, Sheet, SheetExtent, SheetId, Workbook,
    WorkbookId, WorkbookMeta,
};
use tescellate_tess::square::SquareCoord;
use tescellate_tess::voronoi::VoronoiLattice;
use tescellate_tess::{LatticeConfig, LatticeHandle, LatticeKind, ParsedCoord, VoronoiConfig};

pub struct WorkbookEngine {
    pub workbook: Workbook,
    dag: Dag,
    compiled: HashMap<CellRef, CompiledFormula>,
    engines: HashMap<EngineKind, Box<dyn FormulaEngine>>,
    /// Cells whose formula contains a *volatile* function (OFFSET /
    /// INDIRECT) — their dependencies aren't statically analyzable, so
    /// they must recompute on every edit. Unioned into each `set_cell`
    /// dirty closure. See PLAN.md §6.2 / the lookup.rs module docs.
    volatile: std::collections::HashSet<CellRef>,
    /// Cells promoted to the native compiled tier (see `promote_native`).
    /// A promoted cell's formula evaluates through its compiled `cdylib`
    /// rather than the interpreter.
    #[cfg(feature = "native")]
    native: HashMap<CellRef, std::sync::Arc<crate::transpile::native::NativeProgram>>,
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
    /// The cell's formula-engine override, if any. `None` means the
    /// cell inherits the sheet / workbook default. The UI uses this
    /// to show a per-cell language picker.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub engine: Option<EngineKind>,
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
        #[cfg(feature = "python")]
        engines.insert(EngineKind::Python, Box::new(crate::python::PythonEngine));
        Self {
            workbook: empty_workbook(),
            dag: Dag::new(),
            compiled: HashMap::new(),
            engines,
            volatile: std::collections::HashSet::new(),
            #[cfg(feature = "native")]
            native: HashMap::new(),
        }
    }

    pub fn new_workbook(&mut self) -> WorkbookId {
        self.workbook = empty_workbook();
        self.dag = Dag::new();
        self.compiled.clear();
        self.volatile.clear();
        #[cfg(feature = "native")]
        self.native.clear();
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

    /// Serialize the workbook (plus an opaque UI-state blob) to a `.tscl`
    /// zip and return the bytes. The DAG isn't stored — it can be
    /// reconstructed from cell sources on load. Target-agnostic; the UI
    /// uses this directly on wasm32 where the path-based API can't link.
    pub fn save_bytes(&self, ui: &tescellate_store::UiState) -> Result<Vec<u8>, SetCellError> {
        tescellate_store::save_full_to_bytes(&self.workbook, ui)
            .map_err(|e| SetCellError::Io(e.to_string()))
    }

    /// Deserialize a workbook + UI state from in-memory `.tscl` bytes.
    /// Rebuilds the DAG via [`Self::rebuild_dag`]. Returns the opaque UI
    /// state so the UI can restore its own per-sheet view.
    pub fn open_bytes(&mut self, bytes: &[u8]) -> Result<tescellate_store::UiState, SetCellError> {
        let (workbook, ui) = tescellate_store::load_full_from_bytes(bytes)
            .map_err(|e| SetCellError::Io(e.to_string()))?;
        self.workbook = workbook;
        self.dag = Dag::new();
        self.compiled.clear();
        #[cfg(feature = "native")]
        self.native.clear();
        self.rebuild_dag()?;
        Ok(ui)
    }

    /// Persist the workbook to a `.tscl` zip at `path`. Path-based wrapper
    /// over [`Self::save_bytes`] for native callers. Writes with
    /// `UiState::default()`; callers that own UI state should hand it to
    /// `save_bytes` directly.
    pub fn save(&self, path: &std::path::Path) -> Result<(), SetCellError> {
        let bytes = self.save_bytes(&tescellate_store::UiState::default())?;
        std::fs::write(path, bytes).map_err(|e| SetCellError::Io(e.to_string()))
    }

    /// Load a workbook from a `.tscl` file. Path-based wrapper over
    /// [`Self::open_bytes`]; the returned UI state (if any) is discarded.
    pub fn open(&mut self, path: &std::path::Path) -> Result<(), SetCellError> {
        let bytes = std::fs::read(path).map_err(|e| SetCellError::Io(e.to_string()))?;
        self.open_bytes(&bytes).map(|_ui| ())
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
            let lattice = self.lattice_for(sheet_id)?;
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
            for r in &refs {
                match r {
                    FormulaRef::Cell(a) => {
                        if let Ok(canon) = lattice.canonicalize(a) {
                            deps.push(CellRef::new(sheet_id, canon));
                        }
                    }
                    FormulaRef::Range(start, end) => {
                        if let Ok(addrs) = lattice.enumerate_range(start, end) {
                            for a in addrs {
                                deps.push(CellRef::new(sheet_id, a));
                            }
                        }
                    }
                    FormulaRef::Neighbors(a) => {
                        if let Ok(addrs) = lattice.neighbor_addresses(a) {
                            for a in addrs {
                                deps.push(CellRef::new(sheet_id, a));
                            }
                        }
                    }
                    FormulaRef::Radius(a, n) => {
                        if let Ok(addrs) = lattice.cells_within_addresses(a, *n) {
                            for a in addrs {
                                deps.push(CellRef::new(sheet_id, a));
                            }
                        }
                    }
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
        // Voronoi sheets are born with an explicit seed config (the default
        // 8-seed lattice) so the engine and the UI agree from boot and the
        // first drag mutates a concrete config (ADR-011/012). Uniform tilings
        // need no per-sheet config.
        let lattice_config = match lattice {
            LatticeKind::Voronoi => Some(LatticeConfig::Voronoi(VoronoiConfig::from(
                &VoronoiLattice::default(),
            ))),
            _ => None,
        };
        let sheet = Sheet {
            id,
            name: name.into(),
            lattice,
            extent,
            lattice_config,
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
        let lattice = self.lattice_for(sheet)?;
        let coord = lattice
            .parse_coord(addr)
            .map_err(|e| SetCellError::BadAddress(format!("{e}")))?;
        // Bounds check before any mutation.
        if let Some(sheet_ref) = self.workbook.sheets.get(&sheet) {
            if !sheet_ref.extent.contains(coord) {
                return Err(SetCellError::OutOfBounds(addr.to_string()));
            }
        }
        let canonical = lattice.format_coord(coord);
        let cref = CellRef::new(sheet, canonical.clone());
        // Re-editing a cell drops any native program compiled from its
        // previous formula — that program is now stale.
        #[cfg(feature = "native")]
        self.native.remove(&cref);

        // Excel/Sheets convention: input starting with `=` is a formula;
        // anything else is a literal value. Without this branch every cell
        // input is parsed as a formula and arbitrary text raises a parse
        // error — turning the spreadsheet into a graphing calculator.
        let (compiled, dep_addrs, literal_value) = match source {
            Some(src) if !src.trim().is_empty() => {
                if src.trim_start().starts_with('=') {
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
                    (Some(compiled), refs, None)
                } else {
                    // Plain text / number / boolean — store the parsed value
                    // directly, no formula, no deps.
                    (None, Vec::new(), Some(parse_literal(src)))
                }
            }
            _ => (None, Vec::new(), None),
        };

        // Resolve dep addresses through the lattice to canonical CellRefs.
        let mut deps: Vec<CellRef> = Vec::new();
        for r in &dep_addrs {
            match r {
                FormulaRef::Cell(a) => {
                    let canon = lattice
                        .canonicalize(a)
                        .map_err(|e| SetCellError::BadAddress(format!("{e}")))?;
                    deps.push(CellRef::new(sheet, canon));
                }
                FormulaRef::Range(start, end) => {
                    let addrs = lattice
                        .enumerate_range(start, end)
                        .map_err(|e| SetCellError::BadAddress(format!("{e}")))?;
                    for a in addrs {
                        deps.push(CellRef::new(sheet, a));
                    }
                }
                FormulaRef::Neighbors(a) => {
                    let addrs = lattice
                        .neighbor_addresses(a)
                        .map_err(|e| SetCellError::BadAddress(format!("{e}")))?;
                    for a in addrs {
                        deps.push(CellRef::new(sheet, a));
                    }
                }
                FormulaRef::Radius(a, n) => {
                    let addrs = lattice
                        .cells_within_addresses(a, *n)
                        .map_err(|e| SetCellError::BadAddress(format!("{e}")))?;
                    for a in addrs {
                        deps.push(CellRef::new(sheet, a));
                    }
                }
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

        // Track volatility: a formula mentioning OFFSET/INDIRECT recomputes
        // on every edit (its targets aren't statically known). Token check
        // on the source is sufficient — a false positive only costs an
        // extra recompute, never correctness.
        let is_volatile = source.is_some_and(|s| {
            let up = s.to_ascii_uppercase();
            up.contains("OFFSET(") || up.contains("INDIRECT(")
        });
        if is_volatile {
            self.volatile.insert(cref.clone());
        } else {
            self.volatile.remove(&cref);
        }
        // Pre-seed the value with the literal if there is one. For formulas
        // this stays Empty until `recompute` runs below and overwrites it.
        self.store_cell(
            sheet,
            &canonical,
            Cell {
                source: source.filter(|s| !s.trim().is_empty()).map(String::from),
                engine: None,
                value: literal_value.unwrap_or(CellValue::Empty),
            },
        );

        // Recompute the static dirty closure first, in topo order.
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
        // Volatile cells (OFFSET/INDIRECT) recompute in a SECOND pass —
        // after the static closure has settled — because their dynamic
        // targets may be among the cells just recomputed above. Running
        // them in the same pass risks reading a stale value (a volatile
        // cell has no static DAG edge, so topo-order can't sequence it
        // after its dynamic target).
        for c in self.volatile.clone() {
            if !changed.contains(&c) {
                self.recompute(&c);
                changed.push(c);
            }
        }
        Ok(changed)
    }

    pub fn get_cell(&self, sheet: SheetId, addr: &str) -> Option<CellSnapshot> {
        let lattice = self.lattice_for(sheet).ok()?;
        let canonical = lattice.canonicalize(addr).ok()?;
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
                engine: cell.engine,
            });
        }
        // Virtual spill cell?
        if let Some(virt) = spill.virtual_cells.get(&canonical) {
            return Some(CellSnapshot {
                address: canonical,
                source: None,
                value: virt.value.clone(),
                spilled_from: Some(virt.source.clone()),
                engine: None,
            });
        }
        None
    }

    /// The workbook-wide default engine — used by any cell that doesn't
    /// override it via [`Self::set_cell_engine`].
    pub fn default_engine(&self) -> EngineKind {
        self.workbook.default_engine
    }

    /// Change the workbook-wide default engine. Existing cells keep
    /// their explicit per-cell overrides; cells that inherit the
    /// default now compile through the new engine on next edit.
    pub fn set_default_engine(&mut self, engine: EngineKind) {
        self.workbook.default_engine = engine;
    }

    /// Set (or clear, with `engine = None`) the formula-engine override
    /// for a single cell. If the cell holds a formula source, it is
    /// re-parsed through the new engine and dependent cells are
    /// re-evaluated — same propagation as [`Self::set_cell`]. Cells
    /// without a source just store the engine choice for the next edit.
    ///
    /// Returns the list of cells whose values changed (empty if the
    /// cell had no formula source).
    pub fn set_cell_engine(
        &mut self,
        sheet: SheetId,
        addr: &str,
        engine: Option<EngineKind>,
    ) -> Result<Vec<CellRef>, SetCellError> {
        let lattice = self.lattice_for(sheet)?;
        let coord = lattice
            .parse_coord(addr)
            .map_err(|e| SetCellError::BadAddress(format!("{e}")))?;
        let canonical = lattice.format_coord(coord);

        let sheet_ref = self
            .workbook
            .sheets
            .get_mut(&sheet)
            .ok_or(SetCellError::NoSheet(sheet))?;
        let cell = sheet_ref
            .cells
            .entry(canonical.clone())
            .or_insert_with(Cell::blank);
        cell.engine = engine;
        // Capture the source so we can hand it back to `set_cell` for a
        // full re-parse + propagate without holding a borrow.
        let source = cell.source.clone();

        match source {
            Some(ref src) if !src.trim().is_empty() => {
                self.set_cell(sheet, &canonical, Some(src.as_str()))
            }
            _ => Ok(Vec::new()),
        }
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
        let lattice = self.lattice_for(sheet)?;
        let addrs = lattice
            .enumerate_range(start, end)
            .map_err(|e| SetCellError::BadAddress(format!("{e}")))?;
        let sheet_ref = self
            .workbook
            .sheets
            .get(&sheet)
            .ok_or(SetCellError::NoSheet(sheet))?;
        let spill = self.compute_spill_for(sheet);
        let mut out = Vec::new();
        for addr in addrs {
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
                    engine: cell.engine,
                });
            } else if let Some(virt) = spill.virtual_cells.get(&addr) {
                out.push(CellSnapshot {
                    address: addr,
                    source: None,
                    value: virt.value.clone(),
                    spilled_from: Some(virt.source.clone()),
                    engine: None,
                });
            }
        }
        Ok(out)
    }

    /// Walk every cell on the sheet whose value is an Array of size > 1×1
    /// and project it into the spill map: each non-source target becomes a
    /// virtual cell, or — if the target is occupied — the source flips to
    /// `#SPILL!` and no virtual cells are emitted.
    ///
    /// Spill is currently only defined on square sheets — the "paint a
    /// rectangle" semantics doesn't have an obvious analogue on hex /
    /// other lattices. On non-square sheets this returns an empty map
    /// and array-valued cells just stay in their source cell unspilled.
    /// (Future: a `SpillDirection` per lattice, e.g. q-then-r on hex.)
    fn compute_spill_for(&self, sheet: SheetId) -> SpillMap {
        let mut out = SpillMap::default();
        let lattice = match self.lattice_for(sheet) {
            Ok(l) => l,
            Err(_) => return out,
        };
        // Spill semantics: square-only for now. See doc comment above.
        if !matches!(lattice, LatticeHandle::Square(_)) {
            return out;
        }
        let sheet_ref = match self.workbook.sheets.get(&sheet) {
            Some(s) => s,
            None => return out,
        };
        for (addr, cell) in &sheet_ref.cells {
            let arr = match &cell.value {
                CellValue::Array(arr) if !arr.is_scalar() && !arr.is_empty() => arr,
                _ => continue,
            };
            let src_coord = match lattice.parse_coord(addr) {
                Ok(ParsedCoord::Square(c)) => c,
                _ => continue,
            };

            // Collect (offset, target_addr) pairs; check collisions.
            let mut targets = Vec::with_capacity(arr.len());
            let mut collision = false;
            for r in 0..arr.rows {
                for c in 0..arr.cols {
                    if r == 0 && c == 0 {
                        continue;
                    }
                    let target = ParsedCoord::Square(SquareCoord {
                        col: src_coord.col + c as i32,
                        row: src_coord.row + r as i32,
                    });
                    let target_addr = lattice.format_coord(target);
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
            let lattice = match self.lattice_for(cref.sheet) {
                Ok(l) => l,
                Err(_) => return,
            };
            let view = SheetEvalView {
                sheet: cref.sheet,
                workbook: &self.workbook,
                lattice,
            };
            // A promoted cell evaluates through its native compiled tier;
            // every other cell through the interpreter. The two are
            // equivalent — promotion changes how a formula runs, not what
            // it produces (see `promote_native`).
            #[cfg(feature = "native")]
            let result = match self.native.get(cref) {
                Some(program) => program.eval(0, &view),
                None => engine.eval(&compiled, &view),
            };
            #[cfg(not(feature = "native"))]
            let result = engine.eval(&compiled, &view);
            // A cell whose formula evaluates to a bare reference (e.g.
            // `=OFFSET(A1,1,1)` / `=INDIRECT("B2")`) shows the target's
            // value, not the reference itself — deref the final result.
            let derefed =
                result.and_then(|v| crate::excellite::funcs::coerce::deref_reference(v, &view));
            derefed.unwrap_or_else(|e| CellValue::Error(eval_error_to_cell_error(e)))
        };
        if let Some(sheet) = self.workbook.sheets.get_mut(&cref.sheet) {
            if let Some(cell) = sheet.cells.get_mut(&cref.address) {
                cell.value = value;
            }
        }
    }

    /// Promote a cell to the native compiled tier: transpile its formula to
    /// Rust, compile it to a `cdylib`, and route future evaluations of that
    /// cell through the compiled code. The cell's value is unchanged —
    /// compiled Carbide is equivalent to interpreted Carbide.
    #[cfg(feature = "native")]
    pub fn promote_native(&mut self, sheet: SheetId, addr: &str) -> Result<(), SetCellError> {
        let lattice = self.lattice_for(sheet)?;
        let canon = lattice
            .canonicalize(addr)
            .map_err(|e| SetCellError::BadAddress(format!("{e}")))?;
        let cref = CellRef::new(sheet, canon);
        let compiled = self
            .compiled
            .get(&cref)
            .ok_or_else(|| SetCellError::Native(format!("{addr}: no formula to compile")))?;
        let expr = compiled.as_excellite().ok_or_else(|| {
            SetCellError::Native("a Python formula cannot be compiled to native code".into())
        })?;
        let program = crate::transpile::native::compile_program(&[expr])
            .map_err(|e| SetCellError::Native(format!("{e}")))?;
        self.native.insert(cref.clone(), program);
        self.recompute(&cref);
        Ok(())
    }

    /// Drop a cell back to the interpreter, undoing `promote_native`.
    #[cfg(feature = "native")]
    pub fn demote_native(&mut self, sheet: SheetId, addr: &str) -> Result<(), SetCellError> {
        let lattice = self.lattice_for(sheet)?;
        let canon = lattice
            .canonicalize(addr)
            .map_err(|e| SetCellError::BadAddress(format!("{e}")))?;
        let cref = CellRef::new(sheet, canon);
        self.native.remove(&cref);
        self.recompute(&cref);
        Ok(())
    }

    fn lattice_for(&self, sheet: SheetId) -> Result<LatticeHandle, SetCellError> {
        let s = self
            .workbook
            .sheets
            .get(&sheet)
            .ok_or(SetCellError::NoSheet(sheet))?;
        // Voronoi sheets carry their seed config on the Sheet (ADR-011/012):
        // build the eval-time lattice from it so custom/dragged seeds drive
        // evaluation. A `None` config (legacy file) falls back to the default
        // 8-seed lattice. Uniform tilings ignore `lattice_config`.
        match (s.lattice, &s.lattice_config) {
            (LatticeKind::Voronoi, Some(LatticeConfig::Voronoi(cfg))) => {
                let lat = cfg
                    .to_lattice()
                    .map_err(|e| SetCellError::BadLatticeConfig(format!("{e}")))?;
                Ok(LatticeHandle::Voronoi(lat))
            }
            _ => LatticeHandle::for_kind(s.lattice)
                .ok_or(SetCellError::UnsupportedLattice(s.lattice)),
        }
    }

    /// Replace a Voronoi sheet's seed set, re-evaluating every cell against
    /// the new geometry. Bounds are preserved (this mutates seeds only).
    /// Returns the cells whose values may have changed.
    ///
    /// Correctness (ADR-012): a seed move changes geometry, not formula
    /// text, so the static `:NEIGHBORS`/radius DAG edges resolved at edit
    /// time are stale. We therefore rebuild the whole DAG — re-resolving
    /// every cell's dependencies against the *new* lattice via
    /// [`Self::rebuild_dag`] — and then recompute, rather than fanning out
    /// from the moved seed over the stale graph.
    pub fn set_voronoi_seeds(
        &mut self,
        sheet: SheetId,
        seeds: Vec<[f32; 2]>,
    ) -> Result<Vec<CellRef>, SetCellError> {
        // The target sheet must exist and be Voronoi.
        let existing = {
            let s = self
                .workbook
                .sheets
                .get(&sheet)
                .ok_or(SetCellError::NoSheet(sheet))?;
            if s.lattice != LatticeKind::Voronoi {
                return Err(SetCellError::UnsupportedLattice(s.lattice));
            }
            s.lattice_config.clone()
        };
        // Preserve the sheet's current bounds (default lattice's if unset).
        let bounds = match existing {
            Some(LatticeConfig::Voronoi(cfg)) => cfg.bounds,
            _ => VoronoiConfig::from(&VoronoiLattice::default()).bounds,
        };
        let cfg = VoronoiConfig { seeds, bounds };
        // Validate BEFORE mutating so a reject (coincident/degenerate) leaves
        // the stored config untouched.
        cfg.to_lattice()
            .map_err(|e| SetCellError::BadLatticeConfig(format!("{e}")))?;
        // Commit the new config.
        self.workbook
            .sheets
            .get_mut(&sheet)
            .expect("sheet existence checked above")
            .lattice_config = Some(LatticeConfig::Voronoi(cfg));

        // Re-resolve every cell's geometry-dependent dependencies against the
        // new lattice, then recompute. `volatile` is keyed by formula source
        // (unchanged by a seed move), so it is intentionally left intact.
        self.dag = Dag::new();
        self.compiled.clear();
        #[cfg(feature = "native")]
        self.native.clear();
        self.rebuild_dag()?;

        // Every cell on this sheet may have changed (its geometry moved), as
        // may their downstream dependents (possibly on other sheets). Build
        // the dirty set as the union of each sheet cell's dirty closure, in
        // topological order, mirroring `set_cell`'s recompute + volatile pass.
        let roots: Vec<CellRef> = self
            .workbook
            .sheets
            .get(&sheet)
            .map(|s| {
                s.cells
                    .keys()
                    .map(|a| CellRef::new(sheet, a.clone()))
                    .collect()
            })
            .unwrap_or_default();
        let mut dirty: Vec<CellRef> = Vec::new();
        let mut seen: std::collections::HashSet<CellRef> = std::collections::HashSet::new();
        for r in &roots {
            for c in self.dag.dirty_closure(r) {
                if seen.insert(c.clone()) {
                    dirty.push(c);
                }
            }
        }
        let order = self.dag.topo_order(dirty.clone());
        let mut changed = Vec::with_capacity(order.len());
        for c in &order {
            self.recompute(c);
            changed.push(c.clone());
        }
        for c in &dirty {
            if !order.contains(c) {
                self.recompute(c);
                changed.push(c.clone());
            }
        }
        for c in self.volatile.clone() {
            if !changed.contains(&c) {
                self.recompute(&c);
                changed.push(c);
            }
        }
        Ok(changed)
    }

    /// The current Voronoi lattice for `sheet`, built from its stored config
    /// (or the default 8-seed lattice if the config is absent). `None` for a
    /// non-Voronoi sheet. The UI uses this to keep its render cache in sync
    /// with the engine — the engine is the single source of truth (ADR-012).
    pub fn voronoi_lattice(&self, sheet: SheetId) -> Option<VoronoiLattice> {
        let s = self.workbook.sheets.get(&sheet)?;
        if s.lattice != LatticeKind::Voronoi {
            return None;
        }
        match &s.lattice_config {
            Some(LatticeConfig::Voronoi(cfg)) => cfg.to_lattice().ok(),
            _ => Some(VoronoiLattice::default()),
        }
    }
}

struct SheetEvalView<'a> {
    sheet: SheetId,
    workbook: &'a Workbook,
    lattice: LatticeHandle,
}

impl SheetEvalView<'_> {
    fn lookup(&self, canon: &str) -> CellValue {
        self.workbook
            .sheets
            .get(&self.sheet)
            .and_then(|s| s.cells.get(canon))
            .map(|c| c.value.clone())
            .unwrap_or(CellValue::Empty)
    }
}

impl EvalCtx for SheetEvalView<'_> {
    fn cell(&self, addr: &str) -> Result<CellValue, EvalError> {
        let canon = self
            .lattice
            .canonicalize(addr)
            .map_err(|e| EvalError::Ref(format!("{e}")))?;
        Ok(self.lookup(&canon))
    }

    fn range(&self, start: &str, end: &str) -> Result<Vec<CellValue>, EvalError> {
        let addrs = self
            .lattice
            .enumerate_range(start, end)
            .map_err(|e| EvalError::Ref(format!("{e}")))?;
        Ok(addrs.into_iter().map(|a| self.lookup(&a)).collect())
    }

    fn neighbors_of(&self, addr: &str) -> Result<Vec<CellValue>, EvalError> {
        let addrs = self
            .lattice
            .neighbor_addresses(addr)
            .map_err(|e| EvalError::Ref(format!("{e}")))?;
        Ok(addrs.into_iter().map(|a| self.lookup(&a)).collect())
    }

    fn cells_within(&self, addr: &str, radius: i64) -> Result<Vec<CellValue>, EvalError> {
        let addrs = self
            .lattice
            .cells_within_addresses(addr, radius)
            .map_err(|e| EvalError::Ref(format!("{e}")))?;
        Ok(addrs.into_iter().map(|a| self.lookup(&a)).collect())
    }

    fn lattice_distance(&self, a: &str, b: &str) -> Result<i64, EvalError> {
        self.lattice
            .lattice_distance(a, b)
            .map_err(|e| EvalError::Ref(format!("{e}")))
    }

    fn offset_addr(&self, addr: &str, dcol: i32, drow: i32) -> Result<String, EvalError> {
        self.lattice
            .offset(addr, dcol, drow)
            .map_err(|e| EvalError::Ref(format!("{e}")))
    }
}

/// Parse a literal cell input (anything not starting with `=`) into a
/// concrete `CellValue`. Mirrors Excel/Sheets: numeric strings become
/// numbers, `TRUE`/`FALSE` become booleans, everything else is text.
/// The user's original source string is stored separately, so what they
/// typed is what they see when they click the cell again.
fn parse_literal(src: &str) -> CellValue {
    let trimmed = src.trim();
    if trimmed.is_empty() {
        return CellValue::Empty;
    }
    if trimmed.eq_ignore_ascii_case("TRUE") {
        return CellValue::Bool(true);
    }
    if trimmed.eq_ignore_ascii_case("FALSE") {
        return CellValue::Bool(false);
    }
    if let Ok(n) = trimmed.parse::<f64>() {
        if n.is_finite() {
            return CellValue::Number(n);
        }
    }
    CellValue::Text(src.to_string())
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
    #[error("lattice config: {0}")]
    BadLatticeConfig(String),
    #[error("io: {0}")]
    Io(String),
    #[error("address {0} is outside this sheet's bounds")]
    OutOfBounds(String),
    #[cfg(feature = "native")]
    #[error("native: {0}")]
    Native(String),
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

    fn new_hex_sheet() -> (WorkbookEngine, SheetId) {
        let mut eng = WorkbookEngine::new();
        eng.new_workbook();
        let sid = eng.add_sheet("Hex1", LatticeKind::HexPointy);
        (eng, sid)
    }

    fn new_triangle_sheet() -> (WorkbookEngine, SheetId) {
        let mut eng = WorkbookEngine::new();
        eng.new_workbook();
        let sid = eng.add_sheet("Tri1", LatticeKind::Triangle);
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
    fn offset_resolves_target_on_square() {
        let (mut eng, sid) = new_sheet();
        eng.set_cell(sid, "B2", Some("=99")).unwrap();
        // Formula lives in D5 (not the anchor) to avoid a self-cycle.
        // OFFSET(A1, 1, 1) → B2 = 99.
        eng.set_cell(sid, "D5", Some("=OFFSET(A1, 1, 1)")).unwrap();
        assert_eq!(
            eng.get_cell(sid, "D5").unwrap().value,
            CellValue::Number(99.0)
        );
    }

    #[test]
    fn indirect_resolves_target_on_square() {
        let (mut eng, sid) = new_sheet();
        eng.set_cell(sid, "C3", Some("=7")).unwrap();
        eng.set_cell(sid, "A1", Some(r#"=INDIRECT("C3")"#)).unwrap();
        assert_eq!(
            eng.get_cell(sid, "A1").unwrap().value,
            CellValue::Number(7.0)
        );
    }

    #[test]
    fn offset_volatile_recomputes_on_unrelated_edit() {
        let (mut eng, sid) = new_sheet();
        eng.set_cell(sid, "B1", Some("=5")).unwrap();
        // D1 points dynamically at B1 via OFFSET — the static DAG has no
        // edge from B1 to D1, so only volatile recompute keeps it fresh.
        eng.set_cell(sid, "D1", Some("=OFFSET(A1, 0, 1)")).unwrap();
        assert_eq!(
            eng.get_cell(sid, "D1").unwrap().value,
            CellValue::Number(5.0)
        );
        // Edit the dynamic target; D1 must refresh even though nothing
        // statically links them.
        eng.set_cell(sid, "B1", Some("=42")).unwrap();
        assert_eq!(
            eng.get_cell(sid, "D1").unwrap().value,
            CellValue::Number(42.0)
        );
    }

    #[test]
    fn offset_on_hex_axial() {
        let (mut eng, sid) = new_hex_sheet();
        eng.set_cell(sid, "H(1,2)", Some("=88")).unwrap();
        // Formula in H(5,5) (not the anchor) to avoid a self-cycle.
        // OFFSET(H(0,0), rows=2, cols=1) → axial (q+1, r+2) = H(1,2).
        eng.set_cell(sid, "H(5,5)", Some("=OFFSET(H(0,0), 2, 1)"))
            .unwrap();
        assert_eq!(
            eng.get_cell(sid, "H(5,5)").unwrap().value,
            CellValue::Number(88.0)
        );
    }

    #[test]
    fn offset_on_voronoi_index() {
        let mut eng = WorkbookEngine::new();
        eng.new_workbook();
        let sid = eng.add_sheet("Vor1", LatticeKind::Voronoi);
        eng.set_cell(sid, "V(2)", Some("=55")).unwrap();
        // Formula in V(7) (not the anchor) to avoid a self-cycle.
        eng.set_cell(sid, "V(7)", Some("=OFFSET(V(0), 2, 0)"))
            .unwrap();
        assert_eq!(
            eng.get_cell(sid, "V(7)").unwrap().value,
            CellValue::Number(55.0)
        );
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
    fn engine_save_bytes_then_open_bytes_roundtrips() {
        let (mut eng, sid) = new_sheet();
        eng.set_cell(sid, "A1", Some("=10")).unwrap();
        eng.set_cell(sid, "B1", Some("=A1*3")).unwrap();

        let ui: tescellate_store::UiState = serde_json::json!({"answer": 42}).into();
        let bytes = eng.save_bytes(&ui).unwrap();

        let mut other = WorkbookEngine::new();
        let ui_back = other.open_bytes(&bytes).unwrap();
        assert_eq!(ui_back, ui);
        assert_eq!(
            other.get_cell(sid, "B1").unwrap().value,
            CellValue::Number(30.0)
        );
        // DAG was rebuilt — upstream edit propagates downstream.
        other.set_cell(sid, "A1", Some("=4")).unwrap();
        assert_eq!(
            other.get_cell(sid, "B1").unwrap().value,
            CellValue::Number(12.0)
        );
    }

    #[test]
    fn engine_path_api_uses_byte_api() {
        let (eng, _sid) = new_sheet();
        let path_bytes_via_save = eng
            .save_bytes(&tescellate_store::UiState::default())
            .unwrap();

        let tmp = std::env::temp_dir().join(format!(
            "tescellate-byte-api-{}-{}.tscl",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        eng.save(&tmp).unwrap();
        let on_disk = std::fs::read(&tmp).unwrap();
        std::fs::remove_file(&tmp).ok();
        // The two zips need not be byte-identical (zip metadata can drift)
        // but they must each load to the same workbook + empty UiState.
        let (wb_a, ui_a) = tescellate_store::load_full_from_bytes(&path_bytes_via_save).unwrap();
        let (wb_b, ui_b) = tescellate_store::load_full_from_bytes(&on_disk).unwrap();
        assert_eq!(wb_a.sheet_order, wb_b.sheet_order);
        assert_eq!(ui_a, ui_b);
        assert_eq!(ui_a, tescellate_store::UiState::default());
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

    #[test]
    fn literal_text_value() {
        let (mut eng, sid) = new_sheet();
        // Anything not starting with `=` is a literal. Plain text used to
        // raise a parse error because the engine tried to parse "hello" as
        // a formula; now it's stored as a Text value.
        eng.set_cell(sid, "A1", Some("hello world")).unwrap();
        let c = eng.get_cell(sid, "A1").unwrap();
        assert_eq!(c.value, CellValue::Text("hello world".into()));
        assert_eq!(c.source.as_deref(), Some("hello world"));
    }

    #[test]
    fn literal_numbers_and_booleans() {
        let (mut eng, sid) = new_sheet();
        eng.set_cell(sid, "A1", Some("42")).unwrap();
        assert_eq!(
            eng.get_cell(sid, "A1").unwrap().value,
            CellValue::Number(42.0)
        );
        eng.set_cell(sid, "A2", Some("3.5")).unwrap();
        assert_eq!(
            eng.get_cell(sid, "A2").unwrap().value,
            CellValue::Number(3.5)
        );
        eng.set_cell(sid, "A3", Some("TRUE")).unwrap();
        assert_eq!(
            eng.get_cell(sid, "A3").unwrap().value,
            CellValue::Bool(true)
        );
        eng.set_cell(sid, "A4", Some("false")).unwrap();
        assert_eq!(
            eng.get_cell(sid, "A4").unwrap().value,
            CellValue::Bool(false)
        );
    }

    #[test]
    fn literal_then_formula_dependency() {
        let (mut eng, sid) = new_sheet();
        eng.set_cell(sid, "A1", Some("10")).unwrap(); // literal number
        eng.set_cell(sid, "A2", Some("=A1*2")).unwrap(); // formula
        assert_eq!(
            eng.get_cell(sid, "A2").unwrap().value,
            CellValue::Number(20.0)
        );
        // Editing the literal flows through the DAG.
        eng.set_cell(sid, "A1", Some("100")).unwrap();
        assert_eq!(
            eng.get_cell(sid, "A2").unwrap().value,
            CellValue::Number(200.0)
        );
    }

    // ------------------------------------------------------------------
    // Carbide template recreation — end-to-end tests that mirror the
    // Google Sheets template the user shared, asserting our engine
    // reproduces each row's output exactly.
    //
    // Sheet URL:
    //   https://docs.google.com/spreadsheets/d/1fOlZ3YK-Fk7w4OU3tagHfohqSGkAPGL3CrboVoFcnZA
    // ------------------------------------------------------------------

    /// Operation 1: "Make A Commonly Delimited String".
    /// Inputs Red, Orange, Yellow, Green, Blue, Purple → joined with "~".
    #[test]
    fn carbide_op1_join_delimited_string() {
        let (mut eng, sid) = new_sheet();
        for (i, color) in ["Red", "Orange", "Yellow", "Green", "Blue", "Purple"]
            .into_iter()
            .enumerate()
        {
            let addr = format!("{}1", (b'A' + i as u8) as char);
            eng.set_cell(sid, &addr, Some(color)).unwrap();
        }
        eng.set_cell(sid, "G1", Some(r#"=JOIN("~", A1:F1)"#))
            .unwrap();
        assert_eq!(
            eng.get_cell(sid, "G1").unwrap().value,
            CellValue::Text("Red~Orange~Yellow~Green~Blue~Purple".into())
        );
    }

    /// Operation 2: "Split a Commonly Delimited String".
    /// The result is a 1×6 array that spills into B3..G3.
    #[test]
    fn carbide_op2_split_delimited_string() {
        let (mut eng, sid) = new_sheet();
        eng.set_cell(sid, "A3", Some("Red~Orange~Yellow~Green~Blue~Purple"))
            .unwrap();
        eng.set_cell(sid, "B3", Some(r#"=SPLIT("~", A3)"#)).unwrap();
        // B3 holds the array; C3..G3 are virtual spill cells.
        let snap = eng.snapshot_range(sid, "B3", "G3").unwrap();
        let map: hashbrown::HashMap<_, _> =
            snap.into_iter().map(|s| (s.address.clone(), s)).collect();
        assert!(matches!(map["B3"].value, CellValue::Array(_)));
        assert_eq!(map["C3"].value, CellValue::Text("Orange".into()));
        assert_eq!(map["D3"].value, CellValue::Text("Yellow".into()));
        assert_eq!(map["E3"].value, CellValue::Text("Green".into()));
        assert_eq!(map["F3"].value, CellValue::Text("Blue".into()));
        assert_eq!(map["G3"].value, CellValue::Text("Purple".into()));
        for addr in ["C3", "D3", "E3", "F3", "G3"] {
            assert_eq!(map[addr].spilled_from.as_deref(), Some("B3"));
        }
    }

    /// Operation 3: "Identify All Unique Items Within An Array Of Commonly
    /// Delimited Strings". Stack the strings into one big delimited string,
    /// split, dedupe, rejoin.
    #[test]
    fn carbide_op3_unique_across_array_of_delimited_strings() {
        let (mut eng, sid) = new_sheet();
        let inputs = [
            ("A5", "Red~Orange~Yellow"),
            ("B5", "Yellow~Green~Blue"),
            ("A6", "Green~Blue~Purple"),
            ("B6", "Green~Blue~Red"),
            ("A7", "Yellow~Blue~Yellow"),
            ("B7", "Yellow~Green~Purple"),
            ("A8", "Purple~Orange~Red"),
            ("B8", "Green~Green~Blue"),
        ];
        for (addr, s) in inputs {
            eng.set_cell(sid, addr, Some(s)).unwrap();
        }
        eng.set_cell(
            sid,
            "D5",
            Some(r#"=JOIN("~", UNIQUE(SPLIT("~", JOIN("~", A5:B8))))"#),
        )
        .unwrap();
        assert_eq!(
            eng.get_cell(sid, "D5").unwrap().value,
            CellValue::Text("Red~Orange~Yellow~Green~Blue~Purple".into())
        );
    }

    /// Operation 4: "Compare Two Commonly Delimited Strings, Give Uniques
    /// Of 2nd (String Forward Difference Operator)" — set difference.
    #[test]
    fn carbide_op4_forward_difference_of_delimited_strings() {
        let (mut eng, sid) = new_sheet();
        eng.set_cell(sid, "A10", Some("Red~Orange~Yellow~Green~Blue"))
            .unwrap();
        eng.set_cell(sid, "A11", Some("Red~Orange~Yellow~Green~Blue~Purple"))
            .unwrap();
        eng.set_cell(
            sid,
            "A12",
            Some(r#"=JOIN("~", SETDIFF(SPLIT("~", A11), SPLIT("~", A10)))"#),
        )
        .unwrap();
        assert_eq!(
            eng.get_cell(sid, "A12").unwrap().value,
            CellValue::Text("Purple".into())
        );
    }

    // ------------------------------------------------------------------
    // Edge-case gamut — make sure each operation handles boundary
    // conditions gracefully or documents the failure.
    // ------------------------------------------------------------------

    #[test]
    fn join_skips_nothing_includes_empties() {
        // JOIN doesn't filter empties — empty cells become empty segments.
        // Use TEXTJOIN with ignore_empty=TRUE if you want them skipped.
        let (mut eng, sid) = new_sheet();
        eng.set_cell(sid, "A1", Some("a")).unwrap();
        // B1 left empty
        eng.set_cell(sid, "C1", Some("c")).unwrap();
        eng.set_cell(sid, "D1", Some(r#"=JOIN("~", A1:C1)"#))
            .unwrap();
        assert_eq!(
            eng.get_cell(sid, "D1").unwrap().value,
            CellValue::Text("a~~c".into())
        );
        // TEXTJOIN with TRUE skips them.
        eng.set_cell(sid, "D2", Some(r#"=TEXTJOIN("~", TRUE, A1:C1)"#))
            .unwrap();
        assert_eq!(
            eng.get_cell(sid, "D2").unwrap().value,
            CellValue::Text("a~c".into())
        );
    }

    #[test]
    fn split_handles_empty_text_and_single_element() {
        let (mut eng, sid) = new_sheet();
        // Empty string → array with one empty piece.
        eng.set_cell(sid, "A1", Some(r#"=SPLIT(",", "")"#)).unwrap();
        if let CellValue::Array(arr) = eng.get_cell(sid, "A1").unwrap().value {
            assert_eq!(arr.cols, 1);
            assert_eq!(arr.data[0], CellValue::Text("".into()));
        } else {
            panic!("expected array");
        }
        // No delimiter present in input → 1-element array containing the input.
        eng.set_cell(sid, "A2", Some(r#"=SPLIT(",", "abc")"#))
            .unwrap();
        if let CellValue::Array(arr) = eng.get_cell(sid, "A2").unwrap().value {
            assert_eq!(arr.data, vec![CellValue::Text("abc".into())]);
        } else {
            panic!("expected array");
        }
    }

    #[test]
    fn split_multichar_delimiter() {
        let (mut eng, sid) = new_sheet();
        eng.set_cell(sid, "A1", Some(r#"=SPLIT(", ", "a, b, c")"#))
            .unwrap();
        if let CellValue::Array(arr) = eng.get_cell(sid, "A1").unwrap().value {
            assert_eq!(arr.cols, 3);
            assert_eq!(arr.data[0], CellValue::Text("a".into()));
            assert_eq!(arr.data[1], CellValue::Text("b".into()));
            assert_eq!(arr.data[2], CellValue::Text("c".into()));
        } else {
            panic!("expected array");
        }
    }

    #[test]
    fn unique_preserves_first_seen_order() {
        let (mut eng, sid) = new_sheet();
        eng.set_cell(
            sid,
            "A1",
            Some(r#"=JOIN(",", UNIQUE(["c", "a", "b", "a", "c", "d"]))"#),
        )
        .unwrap();
        assert_eq!(
            eng.get_cell(sid, "A1").unwrap().value,
            CellValue::Text("c,a,b,d".into())
        );
    }

    #[test]
    fn set_operations_basic() {
        let (mut eng, sid) = new_sheet();
        // SETUNION
        eng.set_cell(
            sid,
            "A1",
            Some(r#"=JOIN(",", SETUNION([1, 2, 3], [2, 3, 4]))"#),
        )
        .unwrap();
        assert_eq!(
            eng.get_cell(sid, "A1").unwrap().value,
            CellValue::Text("1,2,3,4".into())
        );
        // SETDIFF
        eng.set_cell(
            sid,
            "A2",
            Some(r#"=JOIN(",", SETDIFF([1, 2, 3, 4], [2, 4]))"#),
        )
        .unwrap();
        assert_eq!(
            eng.get_cell(sid, "A2").unwrap().value,
            CellValue::Text("1,3".into())
        );
        // SETINTERSECT
        eng.set_cell(
            sid,
            "A3",
            Some(r#"=JOIN(",", SETINTERSECT([1, 2, 3], [2, 3, 4]))"#),
        )
        .unwrap();
        assert_eq!(
            eng.get_cell(sid, "A3").unwrap().value,
            CellValue::Text("2,3".into())
        );
        // SETSYMDIFF
        eng.set_cell(
            sid,
            "A4",
            Some(r#"=JOIN(",", SETSYMDIFF([1, 2, 3], [2, 3, 4]))"#),
        )
        .unwrap();
        assert_eq!(
            eng.get_cell(sid, "A4").unwrap().value,
            CellValue::Text("1,4".into())
        );
    }

    #[test]
    fn set_operations_empty_inputs() {
        let (mut eng, sid) = new_sheet();
        // Empty 1st arg ⇒ empty result.
        eng.set_cell(sid, "A1", Some(r#"=JOIN(",", SETDIFF([], [1, 2]))"#))
            .unwrap();
        assert_eq!(
            eng.get_cell(sid, "A1").unwrap().value,
            CellValue::Text("".into())
        );
        // Empty 2nd arg ⇒ all of 1st, deduped.
        eng.set_cell(sid, "A2", Some(r#"=JOIN(",", SETDIFF([1, 2, 2, 3], []))"#))
            .unwrap();
        assert_eq!(
            eng.get_cell(sid, "A2").unwrap().value,
            CellValue::Text("1,2,3".into())
        );
    }

    #[test]
    fn in_predicate() {
        let (mut eng, sid) = new_sheet();
        eng.set_cell(sid, "A1", Some(r#"=IN("b", ["a", "b", "c"])"#))
            .unwrap();
        assert_eq!(
            eng.get_cell(sid, "A1").unwrap().value,
            CellValue::Bool(true)
        );
        eng.set_cell(sid, "A2", Some(r#"=IN("z", ["a", "b", "c"])"#))
            .unwrap();
        assert_eq!(
            eng.get_cell(sid, "A2").unwrap().value,
            CellValue::Bool(false)
        );
    }

    #[test]
    fn countif_equality() {
        let (mut eng, sid) = new_sheet();
        eng.set_cell(sid, "A1", Some("apple")).unwrap();
        eng.set_cell(sid, "A2", Some("banana")).unwrap();
        eng.set_cell(sid, "A3", Some("apple")).unwrap();
        eng.set_cell(sid, "A4", Some("cherry")).unwrap();
        eng.set_cell(sid, "B1", Some(r#"=COUNTIF(A1:A4, "apple")"#))
            .unwrap();
        assert_eq!(
            eng.get_cell(sid, "B1").unwrap().value,
            CellValue::Integer(2)
        );
    }

    #[test]
    fn join_split_round_trip_preserves_data() {
        let (mut eng, sid) = new_sheet();
        eng.set_cell(sid, "A1", Some("alpha|beta|gamma|delta"))
            .unwrap();
        // SPLIT then JOIN gives back the original.
        eng.set_cell(sid, "A2", Some(r#"=JOIN("|", SPLIT("|", A1))"#))
            .unwrap();
        assert_eq!(
            eng.get_cell(sid, "A2").unwrap().value,
            CellValue::Text("alpha|beta|gamma|delta".into())
        );
    }

    #[test]
    fn nested_dynamic_array_chain() {
        // SORT(UNIQUE(SPLIT(...))) — nested array-returning functions
        // should compose without spill confusion at intermediate steps.
        let (mut eng, sid) = new_sheet();
        eng.set_cell(sid, "A1", Some("banana,apple,cherry,apple,banana"))
            .unwrap();
        eng.set_cell(
            sid,
            "B1",
            Some(r#"=JOIN(",", SORT(UNIQUE(SPLIT(",", A1))))"#),
        )
        .unwrap();
        assert_eq!(
            eng.get_cell(sid, "B1").unwrap().value,
            CellValue::Text("apple,banana,cherry".into())
        );
    }

    // ------------------------------------------------------------------
    // Hex lattice — Phase 2 end-to-end coverage.
    // ------------------------------------------------------------------

    #[test]
    fn hex_literal_cell() {
        let (mut eng, sid) = new_hex_sheet();
        eng.set_cell(sid, "H(0,0)", Some("=42")).unwrap();
        let c = eng.get_cell(sid, "H(0,0)").unwrap();
        assert_eq!(c.value, CellValue::Number(42.0));
    }

    #[test]
    fn hex_arithmetic_across_cells() {
        let (mut eng, sid) = new_hex_sheet();
        eng.set_cell(sid, "H(0,0)", Some("=10")).unwrap();
        eng.set_cell(sid, "H(1,0)", Some("=20")).unwrap();
        eng.set_cell(sid, "H(0,1)", Some("=H(0,0) + H(1,0)"))
            .unwrap();
        assert_eq!(
            eng.get_cell(sid, "H(0,1)").unwrap().value,
            CellValue::Number(30.0)
        );
        // Upstream edit propagates.
        eng.set_cell(sid, "H(0,0)", Some("=100")).unwrap();
        assert_eq!(
            eng.get_cell(sid, "H(0,1)").unwrap().value,
            CellValue::Number(120.0)
        );
    }

    #[test]
    fn triangle_literal_round_trip() {
        let (mut eng, sid) = new_triangle_sheet();
        eng.set_cell(sid, "T(0,0)", Some("=42")).unwrap();
        // Canonical form survives whitespace / case-flexible parsing on
        // the read side.
        assert_eq!(
            eng.get_cell(sid, "T( 0 , 0 )").unwrap().value,
            CellValue::Number(42.0),
        );
        assert_eq!(
            eng.get_cell(sid, "T(0,0)").unwrap().address,
            "T(0,0)".to_string(),
        );
    }

    #[test]
    fn triangle_cross_cell_reference() {
        let (mut eng, sid) = new_triangle_sheet();
        eng.set_cell(sid, "T(0,0)", Some("=10")).unwrap();
        eng.set_cell(sid, "T(1,0)", Some("=T(0,0) * 5")).unwrap();
        assert_eq!(
            eng.get_cell(sid, "T(1,0)").unwrap().value,
            CellValue::Number(50.0),
        );
        // Edit propagates the dependency through the DAG.
        eng.set_cell(sid, "T(0,0)", Some("=20")).unwrap();
        assert_eq!(
            eng.get_cell(sid, "T(1,0)").unwrap().value,
            CellValue::Number(100.0),
        );
    }

    #[test]
    fn triangle_range_sum() {
        let (mut eng, sid) = new_triangle_sheet();
        // The block T(0,0):T(1,1) covers 4 triangles.
        eng.set_cell(sid, "T(0,0)", Some("=1")).unwrap();
        eng.set_cell(sid, "T(1,0)", Some("=2")).unwrap();
        eng.set_cell(sid, "T(0,1)", Some("=3")).unwrap();
        eng.set_cell(sid, "T(1,1)", Some("=4")).unwrap();
        eng.set_cell(sid, "T(3,3)", Some("=SUM(T(0,0):T(1,1))"))
            .unwrap();
        assert_eq!(
            eng.get_cell(sid, "T(3,3)").unwrap().value,
            CellValue::Number(10.0)
        );
    }

    #[test]
    fn hex_range_sum() {
        let (mut eng, sid) = new_hex_sheet();
        // Axial-aligned parallelogram H(0,0):H(1,1) covers 4 cells:
        // H(0,0), H(1,0), H(0,1), H(1,1).
        eng.set_cell(sid, "H(0,0)", Some("=1")).unwrap();
        eng.set_cell(sid, "H(1,0)", Some("=2")).unwrap();
        eng.set_cell(sid, "H(0,1)", Some("=3")).unwrap();
        eng.set_cell(sid, "H(1,1)", Some("=4")).unwrap();
        eng.set_cell(sid, "H(2,2)", Some("=SUM(H(0,0):H(1,1))"))
            .unwrap();
        assert_eq!(
            eng.get_cell(sid, "H(2,2)").unwrap().value,
            CellValue::Number(10.0)
        );
    }

    #[test]
    fn hex_neighbors_function() {
        let (mut eng, sid) = new_hex_sheet();
        // Fill the 6 neighbors of H(0,0) with 1..6.
        let neighbors = [
            "H(1,0)", "H(1,-1)", "H(0,-1)", "H(-1,0)", "H(-1,1)", "H(0,1)",
        ];
        for (i, addr) in neighbors.iter().enumerate() {
            eng.set_cell(sid, addr, Some(&format!("={}", i + 1)))
                .unwrap();
        }
        eng.set_cell(sid, "H(5,5)", Some("=SUM(NEIGHBORS(H(0,0)))"))
            .unwrap();
        assert_eq!(
            eng.get_cell(sid, "H(5,5)").unwrap().value,
            // 1 + 2 + 3 + 4 + 5 + 6
            CellValue::Number(21.0)
        );
    }

    #[test]
    fn hex_radius_function() {
        let (mut eng, sid) = new_hex_sheet();
        // Put 1 in H(0,0) and in each of its 6 neighbors; nothing else.
        eng.set_cell(sid, "H(0,0)", Some("=1")).unwrap();
        for addr in [
            "H(1,0)", "H(1,-1)", "H(0,-1)", "H(-1,0)", "H(-1,1)", "H(0,1)",
        ] {
            eng.set_cell(sid, addr, Some("=1")).unwrap();
        }
        // RADIUS(H(0,0), 1) returns the closed disc of 7 cells; SUM = 7.
        eng.set_cell(sid, "H(5,5)", Some("=SUM(RADIUS(H(0,0), 1))"))
            .unwrap();
        assert_eq!(
            eng.get_cell(sid, "H(5,5)").unwrap().value,
            CellValue::Number(7.0)
        );
    }

    #[test]
    fn square_neighbors_function() {
        // Same function works on square sheets — 4 cardinal neighbors.
        let (mut eng, sid) = new_sheet();
        eng.set_cell(sid, "A2", Some("=1")).unwrap(); // up
        eng.set_cell(sid, "C2", Some("=2")).unwrap(); // right (B2's east)
        eng.set_cell(sid, "B3", Some("=3")).unwrap(); // down
        eng.set_cell(sid, "A2", Some("=4")).unwrap(); // overwrite — testing dep tracking too
        eng.set_cell(sid, "Z10", Some("=SUM(NEIGHBORS(B2))"))
            .unwrap();
        // A2(4) + C2(2) + B3(3) + B1(empty=0) = 9.
        assert_eq!(
            eng.get_cell(sid, "Z10").unwrap().value,
            CellValue::Number(9.0)
        );
    }

    #[test]
    fn hex_save_and_open_round_trip() {
        let (mut eng, sid) = new_hex_sheet();
        eng.set_cell(sid, "H(0,0)", Some("=10")).unwrap();
        eng.set_cell(sid, "H(1,0)", Some("=H(0,0) * 2")).unwrap();
        let tmp = std::env::temp_dir().join(format!(
            "tescellate-hex-test-{}-{}.tscl",
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
        // Value preserved and dep chain rebuilt.
        assert_eq!(
            other.get_cell(sid, "H(1,0)").unwrap().value,
            CellValue::Number(20.0)
        );
        other.set_cell(sid, "H(0,0)", Some("=100")).unwrap();
        assert_eq!(
            other.get_cell(sid, "H(1,0)").unwrap().value,
            CellValue::Number(200.0)
        );
    }

    #[test]
    fn hex_extent_bounds_check() {
        use tescellate_core::{BoundedExtent, SheetExtent};
        let mut eng = WorkbookEngine::new();
        eng.new_workbook();
        let sid = eng.add_sheet_with_extent(
            "BoundedHex",
            LatticeKind::HexPointy,
            SheetExtent::Bounded(BoundedExtent::HexAxial {
                q_min: -2,
                q_max: 2,
                r_min: -2,
                r_max: 2,
            }),
        );
        // In-bounds is fine.
        eng.set_cell(sid, "H(0,0)", Some("=1")).unwrap();
        eng.set_cell(sid, "H(2,-2)", Some("=2")).unwrap();
        // Out-of-bounds rejected.
        assert!(matches!(
            eng.set_cell(sid, "H(5,0)", Some("=3")),
            Err(SetCellError::OutOfBounds(_))
        ));
    }

    #[test]
    fn hex_disc_extent_works() {
        use tescellate_core::{BoundedExtent, SheetExtent};
        let mut eng = WorkbookEngine::new();
        eng.new_workbook();
        let sid = eng.add_sheet_with_extent(
            "DiscHex",
            LatticeKind::HexPointy,
            SheetExtent::Bounded(BoundedExtent::HexRadius {
                center_q: 0,
                center_r: 0,
                radius: 1,
            }),
        );
        // H(0,0) and any axial neighbor are at distance ≤ 1.
        eng.set_cell(sid, "H(0,0)", Some("=1")).unwrap();
        eng.set_cell(sid, "H(1,0)", Some("=1")).unwrap();
        eng.set_cell(sid, "H(0,-1)", Some("=1")).unwrap();
        // Distance 2 is rejected.
        assert!(matches!(
            eng.set_cell(sid, "H(2,0)", Some("=1")),
            Err(SetCellError::OutOfBounds(_))
        ));
    }

    #[cfg(feature = "native")]
    #[test]
    fn promoted_cell_matches_interpreter_and_propagates() {
        let (mut eng, sid) = new_sheet();
        eng.set_cell(sid, "A1", Some("=10")).unwrap();
        eng.set_cell(sid, "A2", Some("=20")).unwrap();
        eng.set_cell(sid, "A3", Some("=A1 * A2 + 5")).unwrap();
        let interpreted = eng.get_cell(sid, "A3").unwrap().value;
        assert_eq!(interpreted, CellValue::Number(205.0));

        // Promote A3 to the native compiled tier — the value must not change.
        eng.promote_native(sid, "A3").unwrap();
        assert_eq!(eng.get_cell(sid, "A3").unwrap().value, interpreted);

        // A dependency edit still propagates: the promoted cell recomputes
        // through its native program against the live sheet context.
        eng.set_cell(sid, "A1", Some("=100")).unwrap();
        assert_eq!(
            eng.get_cell(sid, "A3").unwrap().value,
            CellValue::Number(2005.0)
        );
    }

    #[cfg(feature = "native")]
    #[test]
    fn editing_a_promoted_cell_drops_the_stale_native_program() {
        let (mut eng, sid) = new_sheet();
        eng.set_cell(sid, "A1", Some("=2 + 3")).unwrap();
        eng.promote_native(sid, "A1").unwrap();
        assert_eq!(
            eng.get_cell(sid, "A1").unwrap().value,
            CellValue::Number(5.0)
        );

        // Re-editing the formula must drop the now-stale native program —
        // otherwise A1 would keep evaluating the old `2 + 3`.
        eng.set_cell(sid, "A1", Some("=2 + 40")).unwrap();
        assert_eq!(
            eng.get_cell(sid, "A1").unwrap().value,
            CellValue::Number(42.0)
        );

        // Explicit demotion is also available — a no-op here (already
        // interpreted) and must leave the value correct.
        eng.demote_native(sid, "A1").unwrap();
        assert_eq!(
            eng.get_cell(sid, "A1").unwrap().value,
            CellValue::Number(42.0)
        );
    }

    #[test]
    fn set_cell_engine_stores_the_override_and_reports_no_value_change_for_blank() {
        let (mut eng, sid) = new_sheet();
        // A cell with no source — setting an engine just stores the
        // choice. No values change, so the returned dirty-cell list is
        // empty.
        let dirty = eng
            .set_cell_engine(sid, "A1", Some(EngineKind::Python))
            .unwrap();
        assert!(dirty.is_empty());
        assert_eq!(
            eng.get_cell(sid, "A1").map(|s| s.engine),
            Some(Some(EngineKind::Python)),
        );
    }

    #[test]
    fn set_cell_engine_clears_back_to_inheriting_the_default() {
        let (mut eng, sid) = new_sheet();
        eng.set_cell_engine(sid, "A1", Some(EngineKind::Python))
            .unwrap();
        // Clearing the override (None) returns the cell to inheriting
        // the workbook default.
        eng.set_cell_engine(sid, "A1", None).unwrap();
        assert_eq!(eng.get_cell(sid, "A1").and_then(|s| s.engine), None);
    }

    #[test]
    fn default_engine_round_trips_through_the_workbook_setter() {
        let mut eng = WorkbookEngine::new();
        eng.new_workbook();
        assert_eq!(eng.default_engine(), EngineKind::ExcelLite);
        // Changing the default doesn't disturb the per-cell overrides.
        eng.set_default_engine(EngineKind::Python);
        assert_eq!(eng.default_engine(), EngineKind::Python);
    }

    #[cfg(feature = "python")]
    #[test]
    fn python_engine_cell_evaluates() {
        let (mut eng, sid) = new_sheet();
        eng.workbook.default_engine = EngineKind::Python;
        eng.set_cell(sid, "A1", Some("=21 + 21")).unwrap();
        assert_eq!(
            eng.get_cell(sid, "A1").unwrap().value,
            CellValue::Integer(42)
        );
    }

    #[cfg(feature = "python")]
    #[test]
    fn python_cell_reads_another_cell_via_ctx() {
        let (mut eng, sid) = new_sheet();
        eng.workbook.default_engine = EngineKind::Python;
        eng.set_cell(sid, "A1", Some("=10.0")).unwrap();
        eng.set_cell(sid, "B1", Some("=ctx.cell('A1') * 2"))
            .unwrap();
        assert_eq!(
            eng.get_cell(sid, "B1").unwrap().value,
            CellValue::Number(20.0)
        );
    }

    // --- T-003: lattice_for builds from stored Voronoi config ---

    fn voronoi_sheet_with(eng: &mut WorkbookEngine, seeds: Vec<[f32; 2]>) -> SheetId {
        let sid = eng.add_sheet("Vor", LatticeKind::Voronoi);
        let cfg = tescellate_tess::VoronoiConfig {
            seeds,
            bounds: [-20.0, -20.0, 20.0, 20.0],
        };
        eng.workbook.sheets.get_mut(&sid).unwrap().lattice_config =
            Some(LatticeConfig::Voronoi(cfg));
        sid
    }

    #[test]
    fn lattice_for_uses_stored_voronoi_config() {
        let mut eng = WorkbookEngine::new();
        eng.new_workbook();
        let sid = voronoi_sheet_with(&mut eng, vec![[0.0, 0.0], [10.0, 0.0], [0.0, 10.0]]);
        match eng.lattice_for(sid).unwrap() {
            LatticeHandle::Voronoi(v) => assert_eq!(v.len(), 3),
            _ => panic!("expected a Voronoi handle"),
        }
    }

    #[test]
    fn lattice_for_voronoi_none_falls_back_default() {
        let mut eng = WorkbookEngine::new();
        eng.new_workbook();
        let sid = eng.add_sheet("Vor", LatticeKind::Voronoi);
        eng.workbook.sheets.get_mut(&sid).unwrap().lattice_config = None;
        match eng.lattice_for(sid).unwrap() {
            LatticeHandle::Voronoi(v) => assert_eq!(v.len(), 8),
            _ => panic!("expected a Voronoi handle"),
        }
    }

    #[test]
    fn lattice_for_corrupt_config_errors() {
        let mut eng = WorkbookEngine::new();
        eng.new_workbook();
        // Coincident seeds: a hand-edited / corrupt config.
        let sid = voronoi_sheet_with(&mut eng, vec![[1.0, 1.0], [1.0, 1.0]]);
        assert!(matches!(
            eng.lattice_for(sid),
            Err(SetCellError::BadLatticeConfig(_))
        ));
    }

    // --- T-004: Voronoi sheets are seeded with a default config at creation ---

    #[test]
    fn add_voronoi_sheet_seeds_default_config() {
        let mut eng = WorkbookEngine::new();
        eng.new_workbook();
        let sid = eng.add_sheet("V", LatticeKind::Voronoi);
        let expected = LatticeConfig::Voronoi(VoronoiConfig::from(&VoronoiLattice::default()));
        assert_eq!(
            eng.workbook.sheets.get(&sid).unwrap().lattice_config,
            Some(expected)
        );
    }

    #[test]
    fn add_square_sheet_has_no_lattice_config() {
        let mut eng = WorkbookEngine::new();
        eng.new_workbook();
        let sid = eng.add_sheet("S", LatticeKind::Square);
        assert_eq!(eng.workbook.sheets.get(&sid).unwrap().lattice_config, None);
    }

    // --- T-005: set_voronoi_seeds + voronoi_lattice getter ---

    fn stored_seeds(eng: &WorkbookEngine, sid: SheetId) -> Vec<[f32; 2]> {
        match &eng.workbook.sheets.get(&sid).unwrap().lattice_config {
            Some(LatticeConfig::Voronoi(cfg)) => cfg.seeds.clone(),
            other => panic!("expected a Voronoi config, got {other:?}"),
        }
    }

    #[test]
    fn set_voronoi_seeds_stores_and_returns_recomputed() {
        let mut eng = WorkbookEngine::new();
        eng.new_workbook();
        let sid = eng.add_sheet("V", LatticeKind::Voronoi);
        let new = vec![[0.0, 0.0], [20.0, 0.0], [0.0, 20.0]];
        let changed = eng.set_voronoi_seeds(sid, new.clone()).unwrap();
        assert_eq!(stored_seeds(&eng, sid), new);
        // No formula cells yet, so `changed` may be empty — the contract is
        // "the cells whose values may have changed", which is exercised by
        // the neighbor-dependent test below.
        let _ = changed;
    }

    #[test]
    fn set_voronoi_seeds_rejects_coincident_keeps_state() {
        let mut eng = WorkbookEngine::new();
        eng.new_workbook();
        let sid = eng.add_sheet("V", LatticeKind::Voronoi);
        let before = stored_seeds(&eng, sid);
        let bad = vec![[3.0, 3.0], [3.0, 3.0], [9.0, 0.0]];
        assert!(matches!(
            eng.set_voronoi_seeds(sid, bad),
            Err(SetCellError::BadLatticeConfig(_))
        ));
        // Rejected before mutation: config is unchanged.
        assert_eq!(stored_seeds(&eng, sid), before);
    }

    #[test]
    fn set_voronoi_seeds_preserves_bounds() {
        let mut eng = WorkbookEngine::new();
        eng.new_workbook();
        let sid = eng.add_sheet("V", LatticeKind::Voronoi);
        let bounds_before = match &eng.workbook.sheets.get(&sid).unwrap().lattice_config {
            Some(LatticeConfig::Voronoi(cfg)) => cfg.bounds,
            _ => unreachable!(),
        };
        eng.set_voronoi_seeds(sid, vec![[0.0, 0.0], [20.0, 0.0], [0.0, 20.0]])
            .unwrap();
        let bounds_after = match &eng.workbook.sheets.get(&sid).unwrap().lattice_config {
            Some(LatticeConfig::Voronoi(cfg)) => cfg.bounds,
            _ => unreachable!(),
        };
        assert_eq!(bounds_before, bounds_after);
    }

    #[test]
    fn set_voronoi_seeds_recomputes_neighbor_dependents() {
        // Deterministic Delaunay adjacency flip. Hull triangle V0/V1/V2 plus a
        // 4th seed. With V3 far at (50,50) it lies outside the circumcircle of
        // triangle (0,0)(20,0)(0,20) (centre (10,10), r≈14.1), so the Delaunay
        // diagonal is V1-V2 and V0 neighbors only {V1,V2}. Move V3 to the
        // interior point (7,7) and the triangulation becomes three triangles
        // sharing V3, so V0 now also neighbors V3.
        let mut eng = WorkbookEngine::new();
        eng.new_workbook();
        let sid = eng.add_sheet("V", LatticeKind::Voronoi);
        eng.set_voronoi_seeds(
            sid,
            vec![[0.0, 0.0], [20.0, 0.0], [0.0, 20.0], [50.0, 50.0]],
        )
        .unwrap();
        // Neighbor cells hold 10 each; V0 sums its neighbors.
        eng.set_cell(sid, "V(1)", Some("10")).unwrap();
        eng.set_cell(sid, "V(2)", Some("10")).unwrap();
        eng.set_cell(sid, "V(3)", Some("10")).unwrap();
        eng.set_cell(sid, "V(0)", Some("=SUM(NEIGHBORS(V(0)))"))
            .unwrap();
        // Config A: V0 neighbors {V1,V2} → 20.
        assert_eq!(
            eng.get_cell(sid, "V(0)").unwrap().value,
            CellValue::Number(20.0)
        );

        // Move V3 into the interior — V0 now neighbors {V1,V2,V3}.
        let changed = eng
            .set_voronoi_seeds(sid, vec![[0.0, 0.0], [20.0, 0.0], [0.0, 20.0], [7.0, 7.0]])
            .unwrap();
        assert_eq!(
            eng.get_cell(sid, "V(0)").unwrap().value,
            CellValue::Number(30.0),
            "V0 should re-sum its new neighbor set after the seed move"
        );
        assert!(
            changed.contains(&CellRef::new(sid, "V(0)".to_string())),
            "the recomputed set should include the neighbor-dependent cell"
        );
    }

    #[test]
    fn voronoi_lattice_getter_matches_config() {
        let mut eng = WorkbookEngine::new();
        eng.new_workbook();
        let sid = eng.add_sheet("V", LatticeKind::Voronoi);
        let seeds = vec![[0.0, 0.0], [20.0, 0.0], [0.0, 20.0]];
        eng.set_voronoi_seeds(sid, seeds.clone()).unwrap();
        let lat = eng
            .voronoi_lattice(sid)
            .expect("Voronoi sheet has a lattice");
        assert_eq!(lat.len(), 3);
        assert_eq!((lat.seeds[1].x, lat.seeds[1].y), (20.0, 0.0));
    }

    #[test]
    fn voronoi_lattice_none_for_square() {
        let mut eng = WorkbookEngine::new();
        eng.new_workbook();
        let sid = eng.add_sheet("S", LatticeKind::Square);
        assert!(eng.voronoi_lattice(sid).is_none());
    }

    #[test]
    fn voronoi_lattice_none_config_returns_default() {
        // A legacy-loaded Voronoi sheet whose config is absent → default 8.
        let mut eng = WorkbookEngine::new();
        eng.new_workbook();
        let sid = eng.add_sheet("V", LatticeKind::Voronoi);
        eng.workbook.sheets.get_mut(&sid).unwrap().lattice_config = None;
        assert_eq!(eng.voronoi_lattice(sid).unwrap().len(), 8);
    }

    // --- Integration (Component C+D): drag → persist → reload → eval lattice ---

    #[test]
    fn drag_then_persist_round_trip() {
        // The full path: set_voronoi_seeds → Sheet → workbook.json (save_bytes)
        // → reload (open_bytes) → eval-time lattice reflects the dragged seeds.
        let mut eng = WorkbookEngine::new();
        eng.new_workbook();
        let sid = eng.add_sheet("V", LatticeKind::Voronoi);
        let new = vec![[3.0, 4.0], [40.0, -8.0], [-12.0, 33.0]];
        eng.set_voronoi_seeds(sid, new.clone()).unwrap();

        let bytes = eng
            .save_bytes(&tescellate_store::UiState::default())
            .unwrap();

        let mut reloaded = WorkbookEngine::new();
        reloaded.open_bytes(&bytes).unwrap();
        let lat = reloaded
            .voronoi_lattice(sid)
            .expect("reloaded Voronoi sheet has a lattice");
        let got: Vec<[f32; 2]> = lat.seeds.iter().map(|s| [s.x, s.y]).collect();
        assert_eq!(got, new, "dragged seeds must survive save → reload");
    }
}
