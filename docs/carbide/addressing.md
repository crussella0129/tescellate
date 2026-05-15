# Carbide — Addressing

This page is where Carbide's language design closes the loop with Tescellate's tessellating cell shapes. It documents the current (square) addressing in full and sketches what changes when hex, triangle, parallelogram, Voronoi, and drawn tilings arrive. Those tilings will affect this page far more than they affect any other page in this docs set; everywhere else, the language stays the same.

Source files:

- Square lattice: `crates/tescellate-tess/src/square.rs`
- `Lattice` trait: `crates/tescellate-tess/src/lib.rs`
- Address parser at the formula level: parser emits `Expr::CellRef(String)` / `Expr::Range(String, String)` opaquely; the lattice resolves the strings at eval time.

## The contract

Every Carbide address is a **string** at the AST level. The parser doesn't know what lattice the sheet uses; it sees `A1` or `H(2,-3)` or `V<a1b2>` or `T(4,2,▲)` and emits `Expr::CellRef("A1")`. The lattice on the sheet (`Sheet.lattice` → a `LatticeKind`-dispatched `Lattice` impl in `tescellate-tess`) is what owns:

1. **Parsing**: `Lattice::parse_address(s) -> Result<Self::Coord, AddressError>` turns the user-typed string into the lattice's natural coordinate type.
2. **Formatting**: `Lattice::address(c) -> String` produces the canonical string form (used as the storage key in `Sheet.cells`).
3. **Geometry**: `vertices(c)`, `centroid(c)`, `neighbors(c)`, `cell_at(point)`.

This means a Carbide formula on a hex sheet, written `=SUM(H(0,0):H(3,3))`, is parsed *exactly* the same way as `=SUM(A1:D4)` on a square sheet — both produce `Expr::Range("H(0,0)", "H(3,3)")` / `Expr::Range("A1", "D4")`. The difference is in `SheetEvalView::range(start, end)`, which asks the lattice to enumerate the cells.

This contract is what lets the docs you're reading right now stay almost completely lattice-agnostic. The only language-level pages that grow when new tilings land are this one and the [interop](interop.md) page.

## Square — what exists today

`crates/tescellate-tess/src/square.rs`. Excel-compatible.

### Syntax

```text
ADDRESS  =  COLUMN ROW
COLUMN   =  /[A-Z]+/        bijective base-26: A..Z, AA..AZ, BA..ZZ, AAA..
ROW      =  /[1-9][0-9]*/   1-indexed
RANGE    =  ADDRESS ":" ADDRESS
```

Both endpoints of a range must be addresses on the same sheet's lattice. Cross-lattice ranges are a parse error (today: only one lattice exists; tomorrow: the parser surfaces this via the lattice's `parse_address` rejecting the other lattice's syntax).

### Encoding

Internally `SquareCoord { col: i32, row: i32 }` is 0-indexed. The display form is 1-indexed (Excel/Sheets convention). The conversion is in `square::format_a1` and `square::parse_a1`.

| Display | `col` | `row` |
|---|---|---|
| `A1` | 0 | 0 |
| `Z1` | 25 | 0 |
| `AA1` | 26 | 0 |
| `AB42` | 27 | 41 |
| `AZ1` | 51 | 0 |
| `BA1` | 52 | 0 |
| `ZZ1` | 701 | 0 |
| `AAA1` | 702 | 0 |

### Negative coordinates

Negative coords can exist internally (e.g., viewport math during scroll) but have no Excel-equivalent text. `format_a1` falls back to `[c<col>,r<row+1>]` for negative coords; `parse_a1` accepts the bracketed form. Users won't see this in normal use.

### Range semantics

`A1:B5` enumerates the 10 cells in the inclusive rectangular hull, row-major.

### Bounds

`Sheet.extent` is either `Unbounded` (any positive coordinate accepted) or `Bounded::Square { cols, rows }` (writes outside the rectangle return `SetCellError::OutOfBounds`). The extent is sheet-wide, configured by the New-Workbook wizard.

---

## Where this is going

The roadmap (PLAN.md §11) lands these lattices in order. Each entry below sketches:

- **Coord type** — the natural representation in `tescellate-tess`.
- **Address syntax** — what users type and see.
- **Neighbor count** — how many cells share an edge.
- **Range semantics** — what `A:B` means.
- **What changes in Carbide** — what the user notices in their formulas.

### Phase 2 — Hex (pointy-top and flat-top)

`LatticeKind::HexPointy` and `LatticeKind::HexFlat`. The cells are regular hexagons; the only difference between the two variants is whether the point or the flat is at the top of the screen.

| | |
|---|---|
| Coord type | Axial `{ q: i32, r: i32 }` (Red Blob Games convention). |
| Address syntax | `H(q,r)` — explicit signed integers. Examples: `H(0,0)`, `H(2,-3)`. Origin is the user's first-clicked cell when the wizard creates the sheet. |
| Neighbor count | 6 (NE, E, SE, SW, W, NW for pointy-top; rotated for flat-top). |
| Range semantics | `H(a,b):H(c,d)` is the **axial-aligned parallelogram** with corners at the two endpoints. Other shapes (hex disc, free polygon) get function-form constructors (`RADIUS(H(0,0), 3)` returns the radius-3 hex disc) rather than a new range syntax. |
| `NEIGHBORS(cell)` | Returns the 6 edge-neighbors as a `1×6` array. Available on every lattice; on hex sheets it returns 6 cells, on square sheets 4 (and 8 with a future `[diag]` arg). |
| What changes in Carbide | New syntax surface for cell refs (`H(q,r)`). Range arithmetic still works inside `SUM`/`AVERAGE`/`MAP`/etc., but its geometric meaning is "axial parallelogram", not "rectangle on screen". |

### Phase 3 — Triangle

`LatticeKind::Triangle`.

| | |
|---|---|
| Coord type | `{ x: i32, y: i32, up: bool }` — the triangular grid alternates up- and down-pointing triangles; the third component picks which. |
| Address syntax | `T(x,y,▲)` or `T(x,y,▽)`. The arrow is a literal character — the lexer accepts the Unicode arrows or ASCII `^`/`v` aliases as `T(x,y,^)` / `T(x,y,v)`. |
| Neighbor count | 3 (the three edge-neighbors; a triangle has fewer neighbors than a hex because it has fewer edges). |
| Range semantics | `T(a,b,o1):T(c,d,o2)` is a **triangular sub-region** of the input, side `max(|c-a|,|d-b|) + 1`. Mixed orientations in the endpoints determine whether the region is "right side up" or inverted. |
| What changes in Carbide | A second new ref syntax. The third component (`▲`/`▽`) is the first time addresses carry semantic content beyond row/column indices — formulas that switch on orientation can use a future `ORIENTATION(cell)` function. |

### Phase 3 — Parallelogram

`LatticeKind::Parallelogram`.

| | |
|---|---|
| Coord type | `{ u: i32, v: i32 }` — skewed grid coordinates. |
| Address syntax | `P(u,v)`. |
| Neighbor count | 4 by edge; 6 by vertex (function `EDGE_NEIGHBORS(c)` vs `VERTEX_NEIGHBORS(c)`). |
| Range semantics | `P(a,b):P(c,d)` is the rhombic region in `(u,v)` coords. |
| What changes in Carbide | New ref syntax. Range semantics are still rectangular (in skewed coords) — minimal surprise. |

### Phase 6 — Archimedean configurator (generic regular tilings)

When the user picks `Regular(VertexConfig)` other than the four primaries above, the cell shape is a composite of polygons (e.g., `4.8.8` is "octagons and squares"). Each cell-type in the tiling needs its own address namespace:

| | |
|---|---|
| Address syntax | `<Type>(i,j[,k])` where `<Type>` is a single-letter polygon code (`O` = octagon, `S` = square, `T` = triangle, `H` = hex, `D` = dodecagon). `4.8.8` cells address as `O(i,j)` / `S(i,j)`. |
| Range semantics | Per-type: `O(0,0):O(3,3)` is the octagon region; `S(0,0):S(3,3)` is the square region. **Mixed-type ranges are not currently planned**; if you want "every cell in a region", call a function like `CELLS_IN(centre, radius)` that returns a heterogeneous array. |
| What changes in Carbide | The biggest churn so far for formula authors: cell refs now carry a *type tag*. Existing functions transparently operate on `CellValue`s (so `SUM(O(0,0):O(5,5))` works), but lookups that assume a single coord type need to be revisited. The `NEIGHBORS(cell)` function returns heterogeneous types; the user can `FILTER` by type via a future `IS_TYPE(cell, "O")` predicate. |

### Phase 7 — Voronoi (irregular, seed-driven)

When the wizard's "Irregular → Random" option picks a Voronoi tessellation, each cell's identity is its seed point, not its coordinates in a regular grid.

| | |
|---|---|
| Coord type | `{ seed_hash: u64 }` — a stable hash of the seed's `(x, y)` so the address survives precision drift. The seed *position* is stored separately in `IrregularSpec::Voronoi { seeds }` on the sheet. |
| Address syntax | `V<8-char-hex>`. Example: `V<3a7c1e9b>`. The hex is the seed-hash prefix; collisions are detected at sheet load. |
| Neighbor count | Varies per cell (Voronoi cells can have any number of edge-neighbors). |
| Range semantics | **There is no range syntax for Voronoi cells.** Ranges presuppose a coordinate ordering; Voronoi cells have none intrinsic. Instead, the user writes `CELLS_WHERE(predicate)` or `CELLS_IN_REGION(rectangle)` for bulk operations. Per-cell access (`V<3a7c1e9b>`) and full-sheet iteration (`ALL_CELLS()` returning the array) work normally. |
| What changes in Carbide | **A big shift.** The user can no longer write `=SUM(A:A)`-style "column" formulas because there are no columns. The substitutes are `=SUM(ALL_CELLS())` and predicate-based selection. Most formulas that use cell refs by hand will need rewriting if they're moved to a Voronoi sheet; formulas that use higher-order helpers (`MAP`, `BYROW`, etc.) over array results adapt cleanly. |

### Phase 8 — Drawn (irregular, prototype-driven)

When the wizard's "Irregular → Drawn" option validates a user-drawn shape and tiles it.

| | |
|---|---|
| Coord type | `{ i: i32, j: i32, rotation: u8 }` — i/j are positions in the prototype's translation lattice; rotation enumerates the discrete rotational copies the validator produced. |
| Address syntax | `D(i,j,r)`. |
| Neighbor count | Per-prototype. The validator computes it once. |
| Range semantics | `D(a,b,_):D(c,d,_)` enumerates by translation-lattice indices. Rotation index is ignored for ranges. |
| What changes in Carbide | Less radical than Voronoi — the underlying translation lattice gives the cells a rectangular-shaped index space. Mostly the same flavour as parallelogram addresses. |

---

## What stays the same across every lattice

If you're trying to predict how a particular formula will behave on a hypothetical Voronoi sheet, this is the table to consult:

| Carbide feature | Lattice-dependence |
|---|---|
| Literals (numbers, strings, bools, arrays) | **None**. Identical everywhere. |
| Operators (`+ - * / ^ & = …`) | **None**. |
| Aggregates (`SUM`, `AVERAGE`, `MIN`, `MAX`, `COUNT`) | **None** at the formula level. They consume arrays, and `flatten()` works lattice-agnostically once `EvalCtx::range` resolves. |
| `LET` / `LAMBDA` / `LETREC` | **None**. The Env is lattice-free. |
| Higher-order (`MAP`, `REDUCE`, `BYROW`, `BYCOL`, `SCAN`, `MAKEARRAY`) | **None** at the formula level. `BYROW`/`BYCOL` assume a 2-D array shape and work on the array's *internal* row/col layout, which is independent of the underlying lattice geometry. |
| Stats, set ops, text | **None**. |
| Cell references | **Per-lattice syntax.** This is the only place where authoring changes. |
| Range references | **Lattice-dependent semantics.** Square = rectangle, hex = axial parallelogram, Voronoi = not allowed. |
| Bounds (`SheetExtent`) | **Per-lattice spec.** `BoundedExtent::Square { cols, rows }` → `BoundedExtent::HexAxial { q_min, q_max, … }` etc. |
| Geometric functions (`NEIGHBORS`, `RADIUS`, `CELLS_IN_REGION`, …) | **Lattice-defined.** The function dispatches through the lattice. Same function name, lattice-appropriate behaviour. |

In other words: the language is mostly insensitive to the lattice. The *parser* doesn't notice the lattice at all; only the *evaluator's `ctx.cell/range` lookups* and the *future geometric functions* do.

## Open questions for non-rectangular tilings

These are the design calls that will need to be made when each phase lands; flagged here so the docs turn doesn't have to invent them later.

1. **Cross-lattice formulas.** What happens when a formula on a hex sheet references a square sheet via `Sheet1!A1`? The current plan (PLAN.md §3.3) is to allow the cross-ref but disallow cross-lattice ranges. Worth re-confirming when Phase 2 lands.
2. **`NEIGHBORS(cell)` return type.** A `1×N` array where N is the per-cell neighbor count? Or a fixed-size structure that's empty in unused slots? Leaning toward the former for cross-lattice uniformity; finalise in Phase 2.
3. **Voronoi address stability.** If a seed's `(x,y)` is edited by 1e-12, does its hash change? Currently planned: quantise seed coords to a coarse grid before hashing so micro-edits don't change addresses. Validate when Phase 7 lands.
4. **Mixed-type Archimedean ranges.** Should `CELLS_IN(box)` be a magic shape-aware function, or should the user always write `UNION(O(box):O(box), S(box):S(box))`? The former is more usable; the latter is more uniform with how Carbide treats arrays.
5. **What does drag-select do on a hex sheet?** Today (square) the drag produces a rectangular hull. On hex it should snap to the axial-aligned hull of the dragged cells. The renderer change is per-lattice; this is more a UX note than an addressing note, but it's where users notice the lattice most.

When each phase ships, the corresponding row of [interop.md](interop.md) and the corresponding section here become canonical.
