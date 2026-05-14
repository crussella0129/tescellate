# Tescellate — Long-Term Architecture Plan

> A DAG-evaluated spreadsheet where cells are not stuck being squares.

This document is the canonical design. It is intentionally written for the **long-term shape** of the project even though only Phase 0–1 is being implemented up front. Decisions further out are sketches; revise them in this file as we learn.

---

## 1. Vision

Tescellate is a spreadsheet whose cells can be **any tessellating shape**: squares, parallelograms (rhombi), equilateral triangles, regular hexagons, mixed (semi-regular Archimedean) tilings, and — eventually — user-defined periodic tilings.

The reason this is interesting is not aesthetic; different tilings encode **different neighbor relations and adjacency arithmetic**, which makes them natural canvases for problems that fight against rectangular grids:

- **Hexes**: terrain/board games, cellular automata, spatial agent models, certain GIS workloads.
- **Triangles**: barycentric coordinates, color mixing, finite-element-style mesh prototyping.
- **Parallelograms**: isometric layouts, skewed coordinate systems, crystallography.
- **Squares**: everything Excel already does — we must remain competitive on this baseline.

On top of the geometry, formulas can be written in **multiple languages** with a switchable interpreter per cell:

- **Excel-lite** — familiar `=SUM(A1:A10)` style; the default.
- **Python** — embedded via PyO3, single in-process interpreter, full numpy/pandas access (the xlwings-style integration but built in from day one).
- **Rust** — a "preview / compile" two-tier model:
  - **Rhai preview** (Rust-like scripting, sandboxed, microsecond evals) for interactive editing.
  - **rustc native** — the *same source* can be promoted to a real `rustc` compile, observable in the UI (compile bar, errors surfaced inline), producing a cached dynamically-loaded `cdylib`.

### Non-goals (initial)

- Real-time multi-user collaboration. Designed-in (see §9) but Phase 5+.
- Mobile / touch-first UI.
- Replicating every Excel feature. We deliberately pick a focused subset and lean on the language plug-in story for everything else.
- Cloud sync / SaaS. Local-first, files-on-disk.

---

## 2. Stack at a glance

```
┌────────────────────────────────────────────────────────────────┐
│  Electron (Phase 1) → Tauri (Phase 5)                          │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │  Renderer (React + TypeScript + Vite)                    │  │
│  │   - Canvas 2D grid renderer (per-tessellation drawer)    │  │
│  │   - Formula bar + language switcher                      │  │
│  │   - Selection / range model (tessellation-aware)         │  │
│  └────────────────────────┬─────────────────────────────────┘  │
│                           │ ipcRenderer ↔ ipcMain               │
│  ┌────────────────────────┴─────────────────────────────────┐  │
│  │  Main process (Node.js, thin)                            │  │
│  │   - Spawns + supervises Rust core subprocess             │  │
│  │   - Routes JSON-RPC frames between renderer and core    │  │
│  └────────────────────────┬─────────────────────────────────┘  │
└───────────────────────────┼────────────────────────────────────┘
                            │ stdio / length-prefixed JSON-RPC
┌───────────────────────────┴────────────────────────────────────┐
│  tescellate-core (Rust, single binary)                         │
│   ┌──────────────────────────────────────────────────────────┐ │
│   │  tescellate-ipc      JSON-RPC server                     │ │
│   │  tescellate-core     Workbook / Sheet / Cell / DAG       │ │
│   │  tescellate-tess     Tessellation lattices + geometry    │ │
│   │  tescellate-formula  FormulaEngine trait + engines:      │ │
│   │     - excellite      (built-in)                          │ │
│   │     - python         (PyO3, embedded CPython)            │ │
│   │     - rhai           (sandboxed scripting)               │ │
│   │     - rustnative     (rustc + libloading, cached)        │ │
│   │  tescellate-store    .tscl file format (zip)             │ │
│   └──────────────────────────────────────────────────────────┘ │
└────────────────────────────────────────────────────────────────┘
```

Why this split:

- **Rust core does the work.** All evaluation, persistence, and tessellation math is in Rust. The Electron layer is a renderer + IPC pump. This lets us swap the UI shell (Electron → Tauri, later possibly a CLI / headless server) without rewriting logic.
- **Single Rust binary, not a Node native module.** Native modules complicate Electron distribution and don't play well with PyO3's interpreter ownership. A subprocess is boringly portable.
- **JSON-RPC over stdio** is the dumbest protocol that works. We can move to a Unix-socket/TCP transport later (Phase 5, when collaboration arrives) without touching call signatures.

---

## 3. Tessellation geometry

The novel part. Designed as a layered abstraction so adding a new tiling is a matter of implementing one trait.

### 3.1 The `Lattice` trait

```rust
// tescellate-tess/src/lib.rs (sketch)
pub trait Lattice {
    type Coord: Eq + Hash + Copy + Serialize + DeserializeOwned;

    /// Human-readable address for the cell, e.g. "A1", "H(2,-3)", "T(4,2,▲)".
    fn address(&self, c: Self::Coord) -> String;
    fn parse_address(&self, s: &str) -> Result<Self::Coord, AddressError>;

    /// Vertices of the cell in lattice space (pre-zoom). First vertex is canonical "top".
    fn vertices(&self, c: Self::Coord) -> SmallVec<[Point2; 8]>;
    fn centroid(&self, c: Self::Coord) -> Point2;

    /// Edge-adjacent neighbors. Each labeled with a direction enum specific to the lattice.
    fn neighbors(&self, c: Self::Coord) -> SmallVec<[(Direction, Self::Coord); 8]>;

    /// Iterate cells whose bounding boxes intersect a viewport — for rendering.
    fn cells_in_viewport(&self, view: Rect) -> Box<dyn Iterator<Item = Self::Coord> + '_>;

    /// Hit-test a point to a cell (for mouse selection).
    fn cell_at(&self, p: Point2) -> Option<Self::Coord>;

    /// Range semantics: given two anchor cells, define a region. Lattice-specific.
    fn range_between(&self, a: Self::Coord, b: Self::Coord) -> Box<dyn Region<Coord = Self::Coord>>;
}
```

Implementations land in `tescellate-tess`:

| Tiling | Coord scheme | Neighbors | Address syntax | Phase |
|---|---|---|---|---|
| Square | `(col: i32, row: i32)` | 4 (or 8 with diag) | `A1`, `AB42` (Excel-compatible) | 1 |
| Hex (pointy-top) | Axial `(q, r)` | 6 | `H(q,r)` or `Hq,r` (configurable) | 2 |
| Hex (flat-top) | Axial `(q, r)` rotated | 6 | same | 2 |
| Triangle | `(x, y, ▲/▼)` | 3 | `T(x,y,▲)` | 3 |
| Parallelogram (rhombic 60°) | `(u, v)` | 4 (edge) / 6 (vertex) | `P(u,v)` | 3 |
| Truncated square (4.8.8: oct+sq) | composite | 4 oct, 4 sq | `O(...)` / `S(...)` | 4 |
| Trihexagonal (3.6.3.6) | composite | mixed | dual-coord | 4 |
| Arbitrary Archimedean / user-defined | wallpaper-group generator | varies | user-defined | 6+ |

### 3.2 Address syntax & cross-tessellation references

A workbook can hold multiple sheets, each with its own lattice. A reference is `Sheet!Address` where `Address` is parsed by the destination sheet's lattice. Cross-sheet refs across different lattices are allowed but range arithmetic (`A1:B5` style) is only defined within a single lattice.

### 3.3 Ranges

For squares, a range is a rectangle. For other lattices, "range" needs explicit semantics:

- **Hex**: an axial-aligned parallelogram, a hexagonal region of radius `n`, or a free polygon defined by vertex cells. We will start with axial-aligned regions and add hex-radius later (`H(0,0):R3` for radius-3 disc).
- **Triangle**: a triangular region of side `n`, or a barycentric box.
- **Parallelogram**: native rectangle in (u,v) coords.

Each `Lattice` implementation owns its `Region` types. The formula layer sees `Range` as an opaque iterable of `CellRef`.

### 3.4 Geometry library

Pure Rust, no graphics dependencies in `tescellate-tess` — it returns vertex lists and the renderer in the frontend draws. This keeps the core embeddable in non-GUI contexts (CLI, server, tests).

---

## 4. Workbook / Sheet / Cell model

```rust
pub struct Workbook {
    pub id: WorkbookId,
    pub sheets: HashMap<SheetId, Sheet>,
    pub sheet_order: Vec<SheetId>,
    pub default_engine: EngineKind,
    pub meta: WorkbookMeta,
}

pub struct Sheet {
    pub id: SheetId,
    pub name: String,
    pub lattice: LatticeKind,                // enum dispatched to tescellate-tess
    pub cells: HashMap<CellCoord, Cell>,     // sparse
    pub style: SheetStyle,
}

pub struct Cell {
    pub coord: CellCoord,                    // lattice-typed
    pub source: Option<String>,              // formula text, None for blank
    pub engine: Option<EngineKind>,          // None → inherit from sheet/workbook
    pub compiled: Option<CompiledFormula>,   // cached, invalidated on edit
    pub value: CellValue,                    // last evaluated result
    pub deps: SmallVec<[CellRef; 4]>,        // outgoing references
    pub style: CellStyle,
}

pub enum CellValue {
    Empty,
    Number(f64),
    Integer(i64),
    Bool(bool),
    Text(String),
    Date(NaiveDate), Time(NaiveTime), DateTime(DateTime<Utc>),
    Array(Box<ArrayValue>),                  // spilled arrays a la Excel dynamic arrays
    Error(CellError),                        // #REF!, #CYCLE!, #DIV/0!, #LANG!, #COMPILE!
    Pending,                                 // async compile / eval in flight
}
```

`CellCoord` is the algebraic-data-type union of all lattice coord types — implemented as an enum so we can serialize uniformly.

---

## 5. DAG engine

A workbook is a dependency graph: cell → cells it reads. We maintain it explicitly.

### 5.1 Data structures

- `forward: HashMap<CellRef, SmallVec<[CellRef; 4]>>` — for each cell, what it depends on.
- `reverse: HashMap<CellRef, SmallVec<[CellRef; 8]>>` — for each cell, who depends on it (used to find dirty cells when an upstream changes).
- Strongly-connected components are detected lazily on insert; any cell in a non-trivial SCC gets `CellValue::Error(CellError::Cycle)`.

### 5.2 Recompute

On edit of cell `C`:

1. Reparse `C.source` via its engine, get a new `dep` set.
2. Update `forward[C]` and `reverse` deltas.
3. Re-check cycles touching `C`.
4. Compute the **dirty closure**: BFS over `reverse` from `C`.
5. Topologically order the dirty closure (Kahn's algorithm), evaluate.

### 5.3 Concurrency

Independent dirty cells (same topological layer) can be evaluated in parallel. The engine exposes a `rayon`-based parallel eval mode, gated behind a workbook setting because Python/PyO3 has GIL constraints (see §6.2).

### 5.4 Volatility & async

- **Volatile** functions (`NOW()`, `RAND()`, network fetches) are flagged at parse time and re-evaluated on any workbook recompute tick.
- **Async** results (long Python computation, native Rust compile) yield `CellValue::Pending` immediately and update asynchronously via an `cell_updated` push from core to renderer.

### 5.5 Errors

- `#REF!` — broken reference.
- `#CYCLE!` — participant in a cycle.
- `#DIV/0!`, `#NUM!`, `#VALUE!` — arithmetic.
- `#LANG!` — formula parse error in the active engine.
- `#COMPILE!` — engine-level compile failure (rustc errors stored on the cell for inspection).
- `#TIMEOUT!` — formula exceeded execution budget.

---

## 6. Formula language layer

### 6.1 The `FormulaEngine` trait

```rust
pub trait FormulaEngine: Send + Sync {
    fn kind(&self) -> EngineKind;

    fn parse(&self, src: &str, ctx: &ParseCtx) -> Result<CompiledFormula, ParseError>;

    /// Synchronous evaluation. May be called from many threads; impls handle their own locking.
    fn eval(&self, compiled: &CompiledFormula, ctx: &EvalCtx<'_>) -> Result<CellValue, EvalError>;

    /// Optional async path for long compiles (rustc) or long evals (Python heavy compute).
    fn eval_async(&self, ...) -> BoxFuture<'_, Result<CellValue, EvalError>> { ... }
}
```

`CompiledFormula` is opaque to the rest of the system; only the engine that produced it can evaluate it.

### 6.2 Engine implementations

**Excel-lite** (`tescellate-formula::excellite`)
- Hand-written Pratt parser → AST → bytecode.
- Supported in v0: arithmetic, comparison, boolean ops, references, ranges, ~60 functions (SUM, AVERAGE, IF, AND/OR/NOT, COUNT, INDEX, MATCH, VLOOKUP-equivalent for the active lattice, TEXT, CONCAT, NOW, TODAY, the trig family, basic stats).
- Functions are looked up in a registry that can be extended per-lattice (e.g., `NEIGHBORS(cell)` returns a range of edge-neighbors — generic over tessellation).

**Python** (`tescellate-formula::python`)
- One embedded CPython 3.12+ interpreter per core process, via PyO3.
- Each formula body is a function `def __formula__(ctx): ...` compiled with `Py_CompileString` and cached per source-hash.
- `ctx` exposes:
  - `ctx.cell("A1")` / `ctx.cell(coord)` → scalar value.
  - `ctx.range("A1:B10")` → numpy array (zero-copy where possible).
  - `ctx.lattice` → introspection of the active tiling, neighbor queries.
- **GIL is the bottleneck.** Parallel cell eval falls back to single-threaded for Python-engine cells; non-Python cells in the same layer can still parallelize. Long-term: per-formula sub-interpreters when Python 3.12+ PEP 684 is stable enough to embed.

**Rhai** (`tescellate-formula::rhai`)
- Sandboxed scripting, Rust-like syntax. Used as the **preview tier for the Rust engine**.
- Exposes the same `ctx` API as native Rust (where possible) so source promotes cleanly.

**Rust native** (`tescellate-formula::rustnative`)
- The same formula source as Rhai (subject to a documented compatible subset). User clicks "Compile native" on a cell (or workbook setting "auto-promote on idle").
- The engine:
  1. Generates a temp crate `formula_<hash>` with a `#[no_mangle] pub extern "Rust" fn __formula__(ctx: &Ctx) -> Value` entry point and `cdylib` crate-type.
  2. Invokes `rustc` (or `cargo build --release`) in a background tokio task.
  3. On success, `libloading::Library::new(...)` and store handle in an `LRU<Hash, Library>`.
  4. On failure, surface compiler diagnostics on the cell.
- Cache directory: `~/.tescellate/cache/native/<arch>/<rustc-version>/<formula-hash>.dylib|.so|.dll`.
- The UI surfaces compile status: a tiny progress chip on the cell, a compile log panel.
- **Security**: native compiled formulas run in-process; the workbook stores a trust manifest of approved formula hashes. Opening a workbook with un-trusted native formulas falls back to Rhai preview until the user approves.

### 6.3 Switching engines

- Engine can be set per-cell (overrides) and per-sheet (default) and per-workbook (root default).
- Formula bar shows the active engine and a chip to switch; switching reparses with the new engine.
- A formula can reference a cell evaluated by a different engine — the value crosses the engine boundary via the common `CellValue` enum.

---

## 7. IPC

### 7.1 Transport

Length-prefixed JSON-RPC 2.0 over the Rust core's stdio. Both directions support requests, responses, and notifications (server → client for async events).

Example methods (non-exhaustive):

| Method | Direction | Purpose |
|---|---|---|
| `workbook.open` | C→S | Load a `.tscl` file |
| `workbook.new` | C→S | Create workbook with a default lattice |
| `sheet.add` | C→S | Add sheet with a chosen lattice |
| `cell.set` | C→S | Set source + engine on a cell |
| `cell.get` | C→S | Fetch cell value/source/error |
| `range.snapshot` | C→S | Bulk fetch cells in a viewport |
| `cell.updated` | S→C (notif) | Cell value changed (async compile/eval done) |
| `compile.progress` | S→C (notif) | Native compile progress per formula |
| `workbook.save` | C→S | Save to disk |

### 7.2 Binary side-channel (later)

For viewport rendering with thousands of cells we will likely add a binary frame format (CBOR or postcard) for `range.snapshot` responses. Designed in but not in Phase 1.

---

## 8. Frontend

### 8.1 Stack

- **Electron** (Phase 1–4). Bundle size and memory are acceptable for desktop-first.
- **Vite + React + TypeScript** in the renderer.
- **Canvas 2D** for grid rendering. WebGL upgrade path (regl or PixiJS) reserved for Phase 4+ if hex worlds with 100k+ cells need it.
- State: **Zustand** for renderer-local UI state (selection, viewport, formula draft); the **Rust core owns truth** — the renderer is essentially a view over `range.snapshot` queries.

### 8.2 Rendering

Per-lattice drawer modules in `apps/desktop/src/render/<lattice>.ts`:

- Receives viewport (camera transform), zoom, cell value snapshot.
- Asks core (or a cached local mirror) for `vertices(coord)` and value for each visible cell.
- Draws polygons + value labels.

Hit-testing: implemented on the **core side** (`Lattice::cell_at`) and called via IPC during mouse interaction — debounced to ~120Hz. If this turns out to be a latency bottleneck, mirror a lightweight client-side hit-tester for the active lattice.

### 8.3 Formula bar

- A monospace input with syntax highlighting per engine (CodeMirror 6, language packs per engine).
- A language chip ("xl", "py", "rs"). Clicking opens a switcher.
- For `rs`: shows a "Compile native" button. Compile state visualized as a progress chip; errors expand in a panel below.
- For `py`: shows the active interpreter version and a "manage env" affordance (Phase 2+ — venv selection).

### 8.4 Selection model

`Selection` is an abstract region. Different lattices contribute selection-shape primitives:

- Squares: rectangle drag, ctrl-click discrete cells, row/col headers.
- Hexes: rectangular axial drag, radius drag (click + drag outward), lasso polygon.
- Triangles: triangular drag (matched triangles), lasso.
- Parallelograms: parallelogram drag.

The renderer translates user gestures into the lattice's region type; the formula bar emits the canonical address.

---

## 9. Persistence: the `.tscl` format

A zip archive (so it's git-diffable with a tool, scriptable, and inspectable) with this layout:

```
my_workbook.tscl
├── manifest.json
├── workbook.json
├── sheets/
│   ├── 01-sheet1.json
│   └── 02-sheet2.json
├── formulas/
│   ├── native/
│   │   ├── <hash>.rs           # source for compiled native formulas
│   │   └── <hash>.meta.json    # rustc version, compile time, trust state
│   └── python/
│       └── (cached bytecode is *not* shipped — recomputed on open)
├── trust.json                  # per-workbook trust list for native formulas
└── assets/
    └── (embedded images, etc.)
```

- `manifest.json` carries the format version, the set of lattices used, and the set of engines required so we can refuse to open a file that needs an unavailable engine (e.g., Python not present).
- Designed-for-collaboration: file format is content-addressable (cells, formulas referenced by hash) so a future CRDT layer (Phase 5+) can sit on top.

---

## 10. Repository layout

```
tescellate/
├── PLAN.md
├── CLAUDE.md
├── README.md
├── LICENSE-MIT
├── LICENSE-APACHE
├── .gitignore
├── Cargo.toml                  # workspace
├── crates/
│   ├── tescellate-core/
│   ├── tescellate-tess/
│   ├── tescellate-formula/
│   │   ├── src/lib.rs
│   │   ├── src/excellite/
│   │   ├── src/python/         # feature = "python"
│   │   ├── src/rhai/           # feature = "rhai"
│   │   └── src/rustnative/     # feature = "rustnative"
│   ├── tescellate-ipc/
│   ├── tescellate-store/
│   └── tescellate-cli/         # headless driver, for tests and scripting
└── apps/
    └── desktop/
        ├── package.json
        ├── vite.config.ts
        ├── electron/
        │   ├── main.ts
        │   └── preload.ts
        └── src/                # React renderer
            ├── App.tsx
            ├── components/
            ├── render/
            │   ├── square.ts
            │   ├── hex.ts
            │   └── ...
            └── ipc/
```

---

## 11. Roadmap

Each phase ends in something demonstrable. Phases are not calendar-bound.

### Phase 0 — Foundation (this PR)
- Repo, license, gitignore, plan, project CLAUDE.md.
- Cargo workspace with the crate skeletons compiling.
- Electron app skeleton: window opens, renderer talks to main, main spawns Rust core, core answers a `ping`.

### Phase 1 — Square grid MVP
- Square lattice in `tescellate-tess`.
- Excel-lite engine: arithmetic, references, ranges, ~30 functions including `SUM`, `AVG`, `IF`, `COUNT`, `INDEX`, `MATCH`.
- DAG engine + recompute.
- Renderer draws the square grid + selection + formula bar.
- Save/load `.tscl`.
- **Test**: replicate the classic "loan amortization table" sample.

### Phase 2 — Hex grid + Python
- Hex lattice (pointy-top first, flat-top as a workbook setting).
- `NEIGHBORS()`, `RADIUS()` generic functions in Excel-lite.
- PyO3 Python engine; numpy/pandas as the cell-range marshaling format.
- Selection model: rectangular and radius hex selection.
- **Test**: a Conway's-Game-of-Life-on-hexes sheet, single-step recompute.

### Phase 3 — Triangle + parallelogram + Rhai
- Triangle and parallelogram lattices.
- Rhai engine integrated, exposed as the `rs` chip in the formula bar (preview-only).
- Workbook-level engine defaults.
- **Test**: barycentric color mixer on a triangle sheet.

### Phase 4 — Native Rust + perf pass
- `rustnative` engine: rustc compile + libloading + cache + trust manifest.
- Per-cell engine override UI; native compile progress / errors.
- WebGL renderer for large sheets.
- Binary IPC side-channel for `range.snapshot`.
- **Test**: a 100k-cell hex automaton at interactive frame rates.

### Phase 5 — Tauri port + multi-user
- Tauri shell with the same renderer code.
- Move IPC transport from stdio to Unix socket / named pipe.
- Begin CRDT layer over the cell store; Phase 5.1 ships read-only shared sessions, 5.2 ships co-editing.

### Phase 6+ — Open tessellations + plugin API
- User-defined wallpaper-group tilings.
- Public plugin API for new lattices and new formula engines (e.g., a community Julia engine, a SQL engine, a Lean tactic engine).
- Charts / pivots.

---

## 12. Risks and open questions

- **PyO3 + Electron packaging on Windows.** Embedding CPython needs careful dynamic linking. Mitigation: ship Python alongside, pin to a known good build.
- **rustc compile latency as a UX problem.** Even with `cdylib` + incremental, compile times for a "live spreadsheet" are jarring. Mitigation: Rhai preview is the actual editing experience; native compile is opt-in and visually progress-tracked.
- **Sandbox boundary for Python and native Rust.** Both can do anything the host can do. Mitigation: native compile requires explicit trust, Python defaults to a restricted import allowlist with an opt-in to full.
- **"Range" semantics across lattices.** Particularly thorny for triangles. We may end up with multiple `Range` types and per-lattice function dispatch.
- **Address-syntax bikeshedding.** We'll prototype one syntax per lattice and iterate; the parser is owned by the lattice so changing it is local.
- **Tauri's renderer differences (system webview).** Some Canvas/WebGL features behave differently across Edge/WebKit/WebKitGTK; we'll watch for this when porting.

---

## 13. References worth tracking

- *Hexagonal Grids* (Red Blob Games) — definitive reference for coordinate systems.
- *Wallpaper groups* — classification of periodic tilings; basis for future "arbitrary tiling" support.
- xlwings — prior art for Python-in-spreadsheet UX patterns.
- Apache Arrow — likely interchange format for the Python ↔ Rust range marshaling.
- Pyodide, RustPython — alternatives we ruled out for v0 but may revisit.
