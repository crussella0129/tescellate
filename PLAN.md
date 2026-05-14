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

### 3.2 Two families of tiling: regular and irregular

Tescellate exposes two top-level tessellation **families**, each with its own creation UI and runtime properties. Both implement the `Lattice` trait, so the rest of the system (DAG, formulas, persistence) is family-agnostic.

#### Regular tilings — uniform, parameterized

"Regular" here is the colloquial sense: every cell uses the same shape, or a fixed *combination* of shapes that repeats vertex-by-vertex. Mathematically these are the **11 Archimedean tilings** (3 regular + 8 semi-regular). A regular tiling is fully described by its **vertex configuration** — the cyclic list of polygons meeting at each vertex:

| Notation | Meaning | Tiling |
|---|---|---|
| `4.4.4.4` | four squares at every vertex | Square grid |
| `6.6.6` | three hexagons | Hex grid |
| `3.3.3.3.3.3` | six triangles | Triangle grid |
| `4.8.8` | a square and two octagons | Octagon + square |
| `3.6.3.6` | triangle, hex, triangle, hex | Trihexagonal |
| `3.4.6.4` | triangle, square, hexagon, square | Rhombitrihexagonal |
| `3.12.12` | triangle and two dodecagons | Truncated hex |
| ... | (eleven total) | |

The creation UI is a **polygon configurator**:
- Pick polygon types (3, 4, 6, 8, 12 sides — the only regular polygons that tile)
- Compose them around a vertex (drag-into-circle UI) — the configurator validates that interior angles sum to 360°
- Or pick a preset from a gallery of the 11 tilings
- Or input the vertex-configuration string directly (`4.8.8`)

Adjacent secondary modes:
- **Side-count knob**: lock to one polygon family, slide between 3/4/6 sides to switch between triangle, square, hex.
- **Construction-line drawing**: free-draw straight lines and let the system snap to the nearest valid Archimedean tiling. Useful for exploration.

Internally a regular tiling is `LatticeSpec::Regular(vertex_config: VertexConfig)`. The compiled `Lattice` impl is selected from a registry of the 11 hand-tuned implementations; rare custom-but-still-Archimedean cases use a generic Archimedean lattice that walks the wallpaper-group orbit.

#### Irregular tilings — non-uniform, cell-by-cell

"Irregular" means every cell can be a different shape. There is no global vertex configuration; each cell carries its own geometry. Two creation modes:

- **Random / seeded** — the user provides a seed-point distribution (uniformly random, Poisson disk, or imported points) over a bounded region; the system computes a Voronoi tessellation. Each cell's identity is its seed point. Addresses look like `V<seed-hash>` or `V(x,y)`. Stable across re-renders because the seed list is part of the workbook.
- **Drawn / validated** — the user draws a candidate cell shape (polygon). The system **validates tileability** and reports one of:
  - ✅ Tiles (with a translation/rotation rule preview).
  - ✏️ Doesn't tile but the *closest tileable adjustment* is shown (snap-suggest), with the perturbation magnitude in the UI. User accepts the suggestion or rejects it.
  - ❌ No tileable shape nearby (e.g., regular heptagon). UI explains why (interior-angle constraint, etc.).

The tileability check is deep math — practical scope for v1:
- **Convex polygons**: any triangle and any quadrilateral tile. Convex hexagons tile only in the three Reinhardt families. Convex pentagons: exactly 15 known families. Convex 7+: never. We hard-code these cases.
- **Concave polygons**: undecidable in general; we ship heuristics that catch the common cases (rep-tiles, polyomino-style shapes on a sub-grid). False negatives ("we couldn't verify") are acceptable; false positives ("we said it tiles but it doesn't") are not.
- **Aperiodic / substitution tilings** (Penrose, "hat" tile, etc.): out of scope until well after Phase 6. Reserved as a separate exploratory phase.

Internally an irregular tiling is `LatticeSpec::Irregular(IrregularSpec)`. The compiled `Lattice` impl precomputes geometry into a spatial index (R-tree / kd-tree on bounding boxes) so `cell_at` and `cells_in_viewport` stay fast — there is no per-pixel formula evaluation.

#### `LatticeSpec` — the on-disk shape

```rust
pub enum LatticeSpec {
    Regular(VertexConfig),
    Irregular(IrregularSpec),
}

pub struct VertexConfig {
    /// Cyclic list of polygon side-counts around one vertex, e.g. [4,8,8].
    pub polygons: SmallVec<[u8; 6]>,
    /// Edge length in lattice units.
    pub edge_length: f32,
    /// Global rotation of the tiling in radians.
    pub orientation: f32,
}

pub enum IrregularSpec {
    Voronoi {
        seeds: Vec<Point2>,
        bounds: Rect,
        seed_source: SeedSource, // Random{seed}, PoissonDisk{r, seed}, Imported{name}
    },
    Drawn {
        prototype: Vec<Point2>,   // the validated polygon
        rule: TilingRule,         // translation + rotation lattice that lays it out
        bounds: Rect,
    },
}
```

`LatticeSpec` is stored in `Sheet.lattice`; it's what the workbook file persists. The runtime `Lattice` impl is built from the spec on sheet load and cached.

#### Performance contract (why this isn't "formula-on-every-mouse-move")

A natural worry with "formula-based tilings" is that every `cell_at(p)` would run user code at 120 Hz. The `LatticeSpec` design avoids this: specs are **data**, not formulas. The compiler turns a spec into native Rust code (regular cases) or a precomputed spatial index (irregular cases). Per-mouse-move work stays O(log n) with no interpreter on the hot path.

Where a formula language *does* meet the geometry layer is in **range queries inside formulas**: `NEIGHBORS(C5)` or `RADIUS(C5, 3)` ask the lattice for its topology, but those are bounded operations called from the formula engine, not the renderer.

Implementations land in `tescellate-tess`:

| `LatticeSpec` | Coord | Neighbors | Address | Phase |
|---|---|---|---|---|
| `Regular(4.4.4.4)` — square | `(col, row)` | 4 (or 8 with diag) | `A1`, `AB42` | 1 |
| `Regular(6.6.6)` — hex pointy | Axial `(q, r)` | 6 | `H(q,r)` | 2 |
| `Regular(6.6.6)` — hex flat | Axial `(q, r)` rotated | 6 | `H(q,r)` | 2 |
| `Regular(3.3.3.3.3.3)` — tri | `(x, y, ▲/▼)` | 3 | `T(x,y,▲)` | 3 |
| `Regular(parallelogram)` | `(u, v)` | 4 (edge) | `P(u,v)` | 3 |
| `Regular(4.8.8)` — oct+square | composite | 4 / 4 | `O(i,j)` / `S(i,j)` | 4 |
| `Regular(3.6.3.6)` — trihex | composite | mixed | dual-coord | 4 |
| Generic `Regular(*)` Archimedean | wallpaper-group | varies | configured | 6 |
| `Irregular::Voronoi` | seed-hash | varies | `V<id>` | 6 |
| `Irregular::Drawn` | prototype index `(i,j,rot)` | per-prototype | configured | 7 |

### 3.3 Address syntax & cross-tessellation references

A workbook can hold multiple sheets, each with its own lattice. A reference is `Sheet!Address` where `Address` is parsed by the destination sheet's lattice. Cross-sheet refs across different lattices are allowed but range arithmetic (`A1:B5` style) is only defined within a single lattice.

### 3.4 Ranges

For squares, a range is a rectangle. For other lattices, "range" needs explicit semantics:

- **Hex**: an axial-aligned parallelogram, a hexagonal region of radius `n`, or a free polygon defined by vertex cells. We will start with axial-aligned regions and add hex-radius later (`H(0,0):R3` for radius-3 disc).
- **Triangle**: a triangular region of side `n`, or a barycentric box.
- **Parallelogram**: native rectangle in (u,v) coords.

Each `Lattice` implementation owns its `Region` types. The formula layer sees `Range` as an opaque iterable of `CellRef`.

### 3.5 Geometry library

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
- Hand-written Pratt parser → AST → tree-walking evaluator.
- Long-term goal: **feature parity with Excel, Google Sheets, and OpenOffice Calc** — hundreds of functions across math/trig, statistics, logical, text, date/time, lookup, dynamic-array, financial, information, and database categories. Built incrementally; the architecture below makes adding a function a 10-line change rather than a redesign.

Built and growing across phases:

| Category | Phase 1 | Phase 1.5 | Later |
|---|---|---|---|
| Math | `+ - * / ^`, ABS | ROUND, MOD, POWER, SQRT, EXP, LN, LOG | trig family, INT, TRUNC, SIGN, RANDARRAY |
| Logical | IF, AND, OR, NOT | IFERROR, IFNA, IFS, SWITCH | XOR, ISBLANK/NUMBER/TEXT/ERROR |
| Aggregates | SUM, AVERAGE, COUNT, MIN, MAX | | STDEV, VAR, MEDIAN, MODE, PERCENTILE, RANK |
| Text | & (concat) | LEFT, RIGHT, MID, LEN, UPPER, LOWER, PROPER, TRIM, SUBSTITUTE, FIND, SEARCH, REPLACE, TEXTJOIN, TEXTSPLIT, CONCAT | TEXTBEFORE, TEXTAFTER, REGEX*, NUMBERVALUE |
| Lookup | | VLOOKUP, INDEX, MATCH | XLOOKUP, HLOOKUP, OFFSET, INDIRECT, ROW, COLUMN, FILTER (filter is dyn-array) |
| Dyn arrays | | UNIQUE, SORT, FILTER, SEQUENCE, TAKE, DROP | SORTBY, CHOOSEROWS, CHOOSECOLS, VSTACK, HSTACK, TOROW, TOCOL, WRAPROWS, EXPAND |
| Date/time | | | NOW, TODAY, DATE, TIME, YEAR/MONTH/DAY, HOUR/MINUTE/SECOND, WEEKDAY, NETWORKDAYS, EDATE |
| Financial | | | PMT, PV, FV, NPV, IRR, RATE |

### 6.2.1 Arrays, array literals, and spill

Arrays are first-class. There are three array-related shapes in the language:

1. **`CellValue::Array`** — a value that carries `(rows, cols, data: Vec<CellValue>)`. The lingua franca for everything array-shaped: function results, range reads, literal arrays.

2. **Range references (`A1:B5`)** — references to a rectangular block of cells. Evaluated to an `Array` value when used in a position that expects one.

3. **Array literals** — written `[…]` in source:
   - `[1, 2, 3]` — 1×3 row of numbers.
   - `[A1, B3, C5]` — 1×3 row, but elements are cell references that resolve to those cells' current values. This is the **"cell-list array"** — the analogue of `A1:B5` for *non-rectangular* selections, which the irregular tilings (Phase 7+) need.
   - `[[1,2,3],[4,5,6]]` — 2×3 array. Each inner `[…]` is a row.
   - Inner elements can be any expression: literals, refs, ranges, calls. `[SUM(A1:A5), B1, "hello"]` is fine.
   - Ragged arrays (`[[1,2],[3,4,5]]`) are a parse error.

Function call sites that take ranges (`SUM(A1:A10)`) accept array literals interchangeably (`SUM([1,2,3])`, `SUM([A1, B3, C5])`).

### 6.2.2 Spill (dynamic arrays)

When a formula evaluates to an `Array` of size > 1×1, it **spills** into adjacent cells, matching modern Excel and Sheets behaviour:

- The source cell stores the formula and the full `Array` value.
- Cells `(source.col + Δc, source.row + Δr)` for the array's shape are *spill targets*. They render the array's element at that offset and carry a `spilled_from` marker pointing at the source.
- Spill targets are **virtual** — they don't live in `Sheet.cells`. The renderer materializes them from the source's array value at snapshot time, so the source cell stays the single source of truth and the on-disk format doesn't need to know about spill.
- **Collision rule**: if any non-empty cell sits inside the would-be spill region, the source cell's value becomes `CellValue::Error(CellError::Spill)` and *nothing* spills. The collision auto-resolves when the user clears the blocking cell.
- **Editing a spilled cell**: writing a new source into a spill target breaks the spill — the new edit takes precedence and the old source flips to `#SPILL!`. Excel does the same; saves us a footgun-avoidance branch.
- **DAG**: spill targets don't appear in the DAG. A formula `=B3` that reads a spilled cell registers a dependency on `B3`'s coordinate; recompute walks through the source cell because `B3`'s value comes from it via `snapshot`.

### 6.2.3 Function registry

Functions live in `excellite::funcs::<category>` modules:

```rust
// excellite/funcs/text.rs
pub fn left(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> { … }
pub fn right(args: &[Expr], ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> { … }
pub fn register(r: &mut FunctionRegistry) {
    r.add("LEFT", left);
    r.add("RIGHT", right);
    // …
}
```

A single `FunctionRegistry` per engine holds the lookup table. The Excel-lite engine constructs one at startup from `funcs::math::register`, `funcs::text::register`, etc. Per-lattice extensions (`NEIGHBORS(cell)`, `RADIUS(cell, n)`) plug into the same registry from `tescellate-tess`.

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

### Phase 6 — Regular tilings: the Archimedean configurator
- Generic `Regular(*)` lattice driven by a `VertexConfig` (vertex-configuration string like `4.8.8`).
- Polygon-configurator UI: pick polygons, drag-into-vertex, see preview, validate interior-angle sum.
- Construction-line drawing mode: free-draw lines, snap to the nearest valid Archimedean tiling.
- Gallery of all 11 Archimedean tilings as one-click presets.
- **Test**: build a `4.8.8` sheet from the configurator UI and run a flood-fill formula across it.

### Phase 7 — Irregular tilings: Voronoi
- `Irregular::Voronoi` lattice with a configurable seed distribution (random / Poisson disk / imported points).
- Precomputed spatial index for fast `cell_at` and `cells_in_viewport`.
- Address syntax for seed-hashed cells; stable across file reload.
- **Test**: GIS-style sheet — drop seed points, color by formula over cell area.

### Phase 8 — Irregular tilings: draw-and-validate
- Drawing surface for a candidate cell shape.
- Tileability validator: hard-coded coverage of the convex cases (triangles, quads, the 15 pentagon families, the three convex-hexagon families) + a heuristic concave checker.
- "Closest tileable adjustment" suggestion: minimal-perturbation snap.
- **Test**: draw an L-shaped tromino; system verifies it tiles and lays out a sheet.

### Phase 9+ — Aperiodic + plugin API
- Aperiodic tilings via substitution and matching rules (Penrose, "hat", etc.) — exploratory.
- Public plugin API for new `LatticeSpec` variants and new formula engines (community Julia engine, SQL engine, Lean tactic engine, etc.).
- Charts / pivots.

---

## 12. Risks and open questions

- **PyO3 + Electron packaging on Windows.** Embedding CPython needs careful dynamic linking. Mitigation: ship Python alongside, pin to a known good build.
- **rustc compile latency as a UX problem.** Even with `cdylib` + incremental, compile times for a "live spreadsheet" are jarring. Mitigation: Rhai preview is the actual editing experience; native compile is opt-in and visually progress-tracked.
- **Sandbox boundary for Python and native Rust.** Both can do anything the host can do. Mitigation: native compile requires explicit trust, Python defaults to a restricted import allowlist with an opt-in to full.
- **"Range" semantics across lattices.** Particularly thorny for triangles. We may end up with multiple `Range` types and per-lattice function dispatch.
- **Address-syntax bikeshedding.** We'll prototype one syntax per lattice and iterate; the parser is owned by the lattice so changing it is local.
- **Tauri's renderer differences (system webview).** Some Canvas/WebGL features behave differently across Edge/WebKit/WebKitGTK; we'll watch for this when porting.
- **Tileability decision in irregular-drawn mode.** Provably hard for arbitrary shapes. Mitigation: ship hard-coded coverage for the well-classified convex cases first, treat the heuristic concave checker as best-effort with explicit "couldn't verify" output, and reserve aperiodic for a later exploratory phase.
- **Voronoi cell stability across edits.** Adding/removing seeds reshuffles neighbors of nearby cells, which is a recompute storm. Mitigation: track cell identity by seed-point hash (not by region), so unchanged seeds keep stable IDs and only directly-affected cells are dirty.

---

## 13. References worth tracking

- *Hexagonal Grids* (Red Blob Games) — definitive reference for coordinate systems.
- *Wallpaper groups* — classification of periodic tilings; basis for future "arbitrary tiling" support.
- xlwings — prior art for Python-in-spreadsheet UX patterns.
- Apache Arrow — likely interchange format for the Python ↔ Rust range marshaling.
- Pyodide, RustPython — alternatives we ruled out for v0 but may revisit.
