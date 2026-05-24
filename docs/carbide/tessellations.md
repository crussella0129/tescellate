# Carbide — Tessellations

This page is the deep dive on cell-shape tessellations. The companion [addressing.md](addressing.md) covers the *syntax* of cell references per lattice; this page covers the *math, UX, and implementation considerations* behind the lattices themselves. Read this when you want to know:

- What's a regular tiling vs an irregular one, in the senses Carbide cares about.
- How Voronoi tessellations work and what's hard about them.
- What the "einstein hat" tile is and why anyone cares.
- Which lattices exist today, which are next, and which are research-grade.

Source files for the parts that exist:

- `crates/carbide-tess/src/lib.rs` — the `Lattice` trait and `LatticeKind` enum.
- `crates/carbide-tess/src/square.rs` — the only concrete implementation today.
- PLAN.md §3.2 — original architectural framing of the regular/irregular split.

---

## Table of contents

1. [The two families](#the-two-families)
2. [Regular tilings — Archimedean catalog](#regular-tilings--archimedean-catalog)
3. [Irregular: Voronoi (seed-driven)](#irregular-voronoi-seed-driven)
4. [Irregular: Drawn (validate-a-shape)](#irregular-drawn-validate-a-shape)
5. [Aperiodic tilings (Penrose, the einstein hat)](#aperiodic-tilings-penrose-the-einstein-hat)
6. [Implementation cross-cuts](#implementation-cross-cuts)
7. [UX cross-cuts](#ux-cross-cuts)
8. [Roadmap](#roadmap)
9. [Open questions](#open-questions)

---

## The two families

Carbide's `LatticeKind` enum splits the world into two families, each with its own creation UI in the New-Workbook wizard:

| Family | Description | Mathematical sense | Carbide examples |
|---|---|---|---|
| **Regular** | Every cell uses the same shape, or a *fixed combination* of shapes that repeats vertex-by-vertex. Uniform in colloquial sense. | The 11 Archimedean tilings (3 regular + 8 semi-regular). All periodic. | Square (`4.4.4.4`), hex (`6.6.6`), triangle (`3.3.3.3.3.3`), oct+square (`4.8.8`), trihexagonal (`3.6.3.6`), … |
| **Irregular** | Every cell can be a different shape. No global vertex configuration. | Voronoi cells, hand-drawn prototiles, and (eventually) aperiodic tilings. | Voronoi from random seeds; user-draws-a-shape with tileability validation; Penrose; einstein hat. |

The split is conceptual — for the user. The runtime treats both families uniformly: each is a `Lattice` impl that knows how to format/parse addresses, enumerate neighbors, hit-test a point, and produce vertices. The DAG, the formula evaluator, and the persistence layer never see the difference.

Why this matters for Carbide-the-language: every regular tiling has a clean coordinate system (integers along a couple of axes), so cell-ref syntax and range semantics fall out naturally. **Irregular tilings break this assumption**, in two different ways depending on which sub-family. That breakage is what most of this page is about.

---

## Regular tilings — Archimedean catalog

The 11 Archimedean tilings are the *only* edge-to-edge tilings of the plane by regular polygons in which every vertex has the same configuration. They are characterised by a **vertex configuration**: the cyclic list of polygon side-counts meeting at each vertex.

| Notation | Polygons at each vertex | Common name | Implementation phase |
|---|---|---|---|
| `4.4.4.4` | 4 squares | Square grid | **Phase 1 — shipped** |
| `6.6.6` | 3 hexagons | Hex grid (regular) | Phase 2 (pointy + flat orientations) |
| `3.3.3.3.3.3` | 6 triangles | Triangle grid | Phase 3 |
| (parallelogram, skewed `4.4.4.4`) | 4 quads | Parallelogram grid | Phase 3 |
| `4.8.8` | square + 2 octagons | Truncated square | Phase 6 |
| `3.6.3.6` | triangle, hex, triangle, hex | Trihexagonal | Phase 6 |
| `3.4.6.4` | triangle, square, hexagon, square | Rhombitrihexagonal | Phase 6 |
| `3.12.12` | triangle + 2 dodecagons | Truncated hexagonal | Phase 6 |
| `4.6.12` | square, hexagon, dodecagon | Truncated trihexagonal | Phase 6 |
| `3.3.3.3.6` | 4 triangles + 1 hex | Snub trihexagonal | Phase 6 |
| `3.3.4.3.4` | 2 squares + 3 triangles | Snub square | Phase 6 |
| `3.3.3.4.4` | 3 triangles + 2 squares | Elongated triangular | Phase 6 |

The three "regular" tilings (squares, hexes, triangles) are the ones every vertex configuration has only one polygon type. The other eight are "semi-regular" — they mix polygon shapes but every vertex sees the same combination.

Coordinate scheme for Archimedean tilings, in general: **two integer axes** plus, for composite tilings, a **single-letter polygon code** picking which of the cell-types at a given lattice position. So for `4.8.8`:

```
O(i, j) — octagon at lattice position (i, j)
S(i, j) — square between four octagons at (i, j)
```

The same `(i, j)` namespace serves both types, distinguished by the prefix. See [addressing.md § Phase 6](addressing.md#phase-6--archimedean-configurator-generic-regular-tilings) for the full syntax.

The creation UI is a **vertex configurator**: drag polygons (3, 4, 6, 8, 12 sides — the only regular polygons that tile) around a vertex; the configurator validates interior-angle-sum = 360° and rejects non-Archimedean configurations. A preset gallery covers the 11 named tilings.

These are all **periodic**: the tiling has translation symmetry. That gives them clean coordinate schemes and finite per-cell neighbor counts (3, 4, 6, or 8 depending on the polygon).

---

## Irregular: Voronoi (seed-driven)

A Voronoi tessellation partitions the plane into convex polygonal cells, one per "seed" point, such that every point inside a cell is closer to that cell's seed than to any other seed. It's the dual of a Delaunay triangulation.

### Properties

- **Cells are convex polygons.** Most have 5–8 edges in practice; theoretically unbounded (a cell can have any number of edges, equal to its number of Delaunay neighbors).
- **Cells at the convex hull are unbounded.** Carbide clips them to the workbook's viewport rectangle, so every cell has finite extent for rendering.
- **No coordinate system is intrinsic.** Cells are identified by their seed, not by an `(i, j)` index. Order and adjacency are emergent properties of the seed distribution.
- **Adding/removing a seed reshuffles neighbors.** A new seed near an existing region carves cells out of nearby existing cells. The local neighborhood changes; far-away cells are unaffected.

### Coordinate scheme

Cells are addressed by a **stable hash of the seed point**, not by spatial coords:

```rust
struct VoronoiCoord {
    seed_hash: u64,   // hash of the seed's (x, y), quantised to a coarse grid
}
```

Address syntax: `V<8-hex-chars>`, e.g. `V<3a7c1e9b>`.

The hash is computed from the seed's `(x, y)` *quantised* to a coarse-enough grid that micro-edits (sub-pixel drag, floating-point drift, file load round-trips) don't change the address. The quantum is configurable; the default is roughly `1e-9 × workbook-extent`, which gives ~30 bits of position precision and ~32 bits of address salt. Hash collisions are detected at sheet load and rejected — a wizard-generated sheet won't ever collide; a user-imported seed list might, in which case the lattice nudges the colliding seed and tells the user.

### Seed-distribution modes

The wizard exposes three ways to populate seeds for a Voronoi sheet:

| Mode | What it does | Carbide-side coord shape |
|---|---|---|
| **Uniform random** | Sample `N` seeds uniformly across the workbook's bounds. Fast; results have characteristic Voronoi "splotchiness" (cells of widely varying area). | Same — every seed is independent. |
| **Poisson-disk** | Sample `N` seeds with a minimum inter-seed distance `r`, so cell areas are more uniform. Slower (rejection sampling); gives "organic but even" tessellations. | Same. |
| **Imported points** | Use a user-provided list of `(x, y)` seeds. Coords accepted as a 2-column array (e.g. `[[10, 20], [15, 30], …]` typed into a temporary cell range, or loaded from a `.csv`). | Same — addresses computed by hashing each imported point. |

The seed list lives in `Sheet.lattice` (as `IrregularSpec::Voronoi { seeds: Vec<Point2>, bounds, seed_source }`) and is part of the workbook file. The Voronoi diagram itself is recomputed from the seeds on workbook open and cached in memory; the cache invalidates when seeds change.

### Range semantics — they break

`A1:B5` on a square sheet means a rectangle. On a Voronoi sheet, there is no notion of "the cell between V<a1> and V<b5> in some natural order" because there is no natural order. Two seeds chosen by lexicographic hash order might be on opposite sides of the workbook.

The substitutes for range operations on Voronoi sheets:

| Want | Use |
|---|---|
| "Every cell" | `ALL_CELLS()` — returns a flat array of every cell's value. |
| "Cells inside a rectangle" | `CELLS_IN_REGION(top_left, bottom_right)` — geometric query, returns an array. |
| "Cells matching a predicate" | `CELLS_WHERE(LAMBDA(cell, predicate))` — filter, returns an array. |
| "Neighbors of this cell" | `NEIGHBORS(V<id>)` — same function name as on every other lattice; on Voronoi it returns the variable-length list of edge-neighbors. |
| "Cells within distance d of a point" | `CELLS_NEAR(point, distance)`. |

Most formulas that use `MAP`, `REDUCE`, `BYROW`, `SUM`, `AVERAGE`, etc. **work unchanged** on Voronoi sheets — they consume arrays, and the array is what the geometric helpers return. Only formulas that hand-write cell-range literals (`A1:A10`, `H(0,0):H(3,3)`) need rewriting.

### Spatial indexing

`Lattice::cell_at(p: Point2)` on a Voronoi lattice requires a nearest-neighbor query in the seed set. Naive linear scan is O(N) per hit-test — too slow for a 1000-cell sheet at 120 Hz mouse-move. The implementation will use:

- A **k-d tree** built once on workbook load over the seed points. O(log N) per query, O(N log N) to build.
- Or a **grid spatial hash** with cell size ~`r` (the Poisson-disk min distance), also O(log N)-ish.

`Lattice::cells_in_viewport(view: Rect)` enumerates seeds inside `view` (k-d tree range query) and clips their Voronoi cells to the viewport. The clipped polygons get sent to the renderer.

### What this looks like in Carbide

```excel
=COUNT(ALL_CELLS())                                                  → number of seeds
=AVERAGE(CELLS_IN_REGION([0,0], [100,100]))                          → average value over a region
=MAP(NEIGHBORS(V<3a7c1e9b>), LAMBDA(c, c))                           → cell array of neighbors of V<3a7c1e9b>
=LET(
  high, CELLS_WHERE(LAMBDA(c, c > 100)),
  COUNT(high))                                                       → how many cells exceed 100
```

Per-cell formulas in a Voronoi cell work the same way as anywhere else (`=ROUND(V<3a7c1e9b> * 2)` is fine). It's only when you want to operate over multiple cells that the address layer changes.

---

## Irregular: Drawn (validate-a-shape)

The wizard's "draw your own" mode lets the user sketch a candidate cell shape and asks Carbide: *does this tile the plane?*

### What's tractable

| Shape class | Tileability | Implementation |
|---|---|---|
| Any **triangle** | Always tiles (six copies meet at every vertex). | Hard-coded. |
| Any **quadrilateral** (convex or concave) | Always tiles (four copies meet at every vertex). | Hard-coded. |
| **Convex pentagons** | Exactly 15 known families (most recent discovery: 2017). | Hard-coded check against each family. |
| **Convex hexagons** | Three families (Reinhardt 1918). | Hard-coded. |
| **Convex heptagons or more** | Never tile. | Reject with reason. |
| **Concave polygons** | Provably undecidable in general. Many practical cases (rep-tiles, polyomino-shaped pieces on a sub-grid, L-trominoes) do tile. | Heuristic checker; false negatives ("we couldn't verify") are acceptable, false positives are not. |

### Address scheme

Once a drawn shape passes validation, the lattice generates a translation lattice and (if needed) a rotation set for laying out copies of the prototile. Addresses are:

```
D(i, j, r)   — prototile copy at translation lattice position (i, j) with rotation r
```

where `r ∈ 0..number_of_rotations`. For a "vanilla" tile (no rotations needed), `r` is always `0` and the address shortens to `D(i, j)`. The lattice's neighbor function uses the prototile's edge-adjacency rules, which the validator computed once.

### "Closest tileable adjustment"

The wizard goes further: if the drawn shape doesn't tile, the validator can often *suggest* a small adjustment that does. The math:

- For convex polygons, the validator computes the smallest perturbation (in the L² norm over vertex positions) needed to land in one of the 15+3 known families.
- For concave polygons, it tries small edge-length adjustments to make the shape match a rep-tile or polyomino pattern.
- The UI shows the original drawn shape, the suggested adjustment, and the magnitude of the perturbation. The user accepts or rejects.

### What this looks like in Carbide

Drawn-tile addresses work like square addresses with a third index:

```excel
=D(0,0,0)                                                            single cell
=SUM(D(0,0,0):D(3,3,0))                                              16-cell sum (single-rotation column)
=NEIGHBORS(D(2,2,0))                                                 prototile-defined neighbor list
```

Most formulas — aggregates, MAP/REDUCE, stats, set ops — work without modification.

---

## Aperiodic tilings (Penrose, the einstein hat)

The most exotic family. **An aperiodic tiling has no translational symmetry**: there is no vector you can shift the tiling by and have it land on itself. Combined with the constraint that every region of the tiling appears infinitely often (so the tiling is *self-similar* in a precise sense), aperiodic tilings have remarkable mathematical properties — including a hierarchical structure that turns out to be the only practical way to address their cells.

Aperiodic tilings are **research-grade** for Carbide: implementing them is doable but the design questions around addressing, formula authoring, and rendering are open. They are scheduled for **Phase 9+** with the understanding that the spec may shift.

### Penrose tilings (P3)

Discovered by Roger Penrose in 1974. The most common form, the P3 tiling, uses two prototiles:

- **Thin rhombus** (acute angle 36°)
- **Fat rhombus** (acute angle 72°)

Both rhombs have the same edge length. The matching rules (which edges can touch which) force aperiodicity.

The tiling can be generated by **substitution**: each rhomb gets replaced by a fixed pattern of smaller rhombs of both types. After N substitutions you have a finite patch of the tiling; the limit is the infinite Penrose tiling.

| | |
|---|---|
| Number of prototiles | 2 (thin + fat rhomb) |
| Address scheme | Hierarchical: `Pen<depth-N><path-through-substitution-tree>` — each cell is a leaf of the substitution tree. Compact in practice (~log₂(N) bytes per cell at depth N). |
| Neighbor count per cell | 4 (each rhomb has 4 edges) |
| Range semantics | None usable. Use predicate-based selection. |

### The einstein hat tile (and spectre)

In **March 2023**, David Smith, Joseph Samuel Myers, Craig Kaplan, and Chaim Goodman-Strauss published the first known **aperiodic monotile** — a *single* shape (no second prototile needed) that tiles the plane only aperiodically. The shape is a 13-sided polygon called "the hat".

The name *einstein* is a math pun: "ein Stein" = "one stone" in German, meaning "one shape". (Albert is unrelated.) The discovery answered a 60-year-old open question.

In **May 2023**, the same team produced "the spectre" — a closely-related shape that tiles aperiodically without needing reflections. The hat tile, by contrast, requires that some copies be mirror-images of others.

| | Hat | Spectre |
|---|---|---|
| Prototiles | 1 | 1 |
| Reflections required | yes | no |
| Edges | 13 | 14 |
| Substitution rule | yes | yes |
| Generated by | Smith et al., March 2023 | Smith et al., May 2023 |

For Carbide's purposes the hat and spectre behave identically: a single-prototile substitution tiling. Addressing follows the same path-through-substitution-tree scheme as Penrose, with the difference that the prototile is fixed (no thin/fat distinction).

Implementation notes:

- The substitution rule replaces one hat with a fixed pattern of 4 hats (technically: an "H7/H8/F/P/T" expansion in the published terminology, but it's mechanically one rule). Recursive application generates the tiling.
- Rendering a finite patch is straightforward: substitute to the needed depth, then enumerate the leaf tiles and draw their polygons.
- Neighbor enumeration: per the published edge-adjacency rules of the substitution. There is no neat formula; the implementation will likely precompute neighbors for the first N depths and use a lookup.

### What Carbide loses on aperiodic sheets

Even more than Voronoi:

1. **No row/column.** Drag-select would have to produce a geometric region; the user clicks-and-drags out a rectangle and Carbide returns "every cell whose centroid is in that rectangle".
2. **No translation invariance.** Formulas like "shift this pattern by (i+1, j)" don't compose — there's no meaningful translation. Substitution-level operations replace them: "go to my parent in the substitution tree".
3. **Self-similarity-aware queries.** `CELLS_AT_DEPTH(n)`, `PARENT(cell)`, `CHILDREN(cell)` open a hierarchical traversal API that doesn't apply on periodic sheets. These would be aperiodic-only stdlib additions.

### What it's good for

Aperiodic tilings are not just mathematical curiosities. They appear in:

- **Quasicrystals** (Shechtman, Nobel Prize 2011) — 3D analogs of Penrose tilings show up in nature, and the 2D versions are used in materials-science simulations.
- **Antenna arrays / metasurfaces** — aperiodic patterns avoid the resonance peaks of periodic gratings.
- **Decorative architecture** — the Islamic geometric tradition (girih tiles) anticipates Penrose by 500 years.
- **Procedural content generation** — game/level design where periodic repetition would be visually obvious.

A Carbide sheet on an einstein-hat tiling is a niche tool. But it's the only spreadsheet that can express it.

---

## Implementation cross-cuts

The properties that vary across lattices and the engineering knobs Carbide turns.

### Coordinate stability under edits

| Lattice | What can change a cell's address |
|---|---|
| Square | Nothing. `A1` is `A1` for the life of the workbook. |
| Hex / tri / parallelogram | Same — coords are integer indices. |
| Archimedean composite | Same — composite indices plus type tag. |
| Voronoi | Editing a seed *position* changes that seed's hash (and thus address). Adding/removing seeds doesn't change other seeds' addresses (they're independent hashes). |
| Drawn (monohedral) | Address is `D(i,j,r)`. Stable unless the user changes the prototile (which destroys the lattice and creates a new one). |
| Aperiodic | Address is a substitution-tree path. Stable for any cell in a fixed-depth patch; deepening the substitution adds children, doesn't move existing addresses. |

This matters because **formulas reference cells by address**. If a Voronoi seed gets edited and its address changes, every formula referencing the old address breaks. The plan: the wizard's "edit seeds" mode shows the user every formula that references a seed whose hash will change, and offers to rewrite them.

### Neighbor enumeration

| Lattice | Neighbor count | Computation |
|---|---|---|
| Square | 4 (edge) or 8 (with diagonals) | Constant offsets. |
| Hex | 6 | Constant offsets in axial coords. |
| Triangle | 3 | Depends on triangle orientation; constant offsets. |
| Parallelogram | 4 edge / 6 vertex | Constant offsets. |
| Archimedean composite | Per-polygon-type | Lookup table per tiling. |
| Voronoi | Variable, typically 5–8 | From Delaunay triangulation (built once on load). |
| Drawn | Per-prototile | From the validator's edge-adjacency analysis. |
| Aperiodic | Per-tile, varies by tile shape | From the substitution rule's neighbor structure. |

`NEIGHBORS(cell)` is a uniformly-named function across every lattice — the user doesn't think about which lattice they're on, they just ask for neighbors. The return type is always a `1×N` `Array` of cell-valued references.

### Range semantics

| Lattice | What `A:B` means |
|---|---|
| Square | Rectangular hull. |
| Hex (axial) | Axial-aligned parallelogram. |
| Triangle | Triangular region. |
| Parallelogram | Rhombic region. |
| Archimedean composite | Per-polygon-type rectangular hull in lattice coords. |
| Voronoi | **Undefined.** Use `CELLS_IN_REGION` or `CELLS_WHERE`. |
| Drawn | Rectangular hull in `(i, j)` coords; rotation index ignored. |
| Aperiodic | **Undefined.** Use `CELLS_AT_DEPTH`, `CELLS_IN_REGION`, predicate selection. |

The general principle: a range is a *cheap, syntactic* shorthand for an array of cells, which only makes sense if the lattice has an obvious bidirectional traversal between two endpoint addresses. Lattices without that fall back to function-form selectors.

### Spatial indexing

Every lattice needs `cell_at(point: Point2) -> Option<Coord>` for mouse hit-testing. The implementation strategies:

| Lattice | `cell_at` complexity |
|---|---|
| Square / hex / tri / parallelogram | O(1). Inverse of the lattice transform. |
| Archimedean composite | O(1). Inverse of the composite lattice transform plus a type-tag check. |
| Voronoi | O(log N) via k-d tree or grid spatial hash over seeds. |
| Drawn | O(1). Inverse of the translation lattice plus a rotation check. |
| Aperiodic | O(depth × log N) via a precomputed quadtree over the patch. |

Acceptable thresholds: **120 Hz for mouse-move** (every cell hit-test should be < 8 ms; in practice we want < 100 µs). Voronoi at 10k cells fits comfortably; aperiodic at 100k tiles is the worst case but still meets the budget.

### Rendering

`Lattice::cells_in_viewport(view: Rect)` yields the cells whose bounding boxes intersect the view. For periodic lattices this is a small integer math problem. For Voronoi it's a k-d tree range query plus clipping the result polygons to `view`. For aperiodic it's a depth-first walk of the substitution tree pruned by intersection with `view`.

Rendering cost per cell is dominated by Canvas 2D `lineTo` calls, so a 10k-cell viewport at 60 Hz is the practical upper bound for the current renderer. WebGL via `regl` / PixiJS (PLAN.md §8) pushes that to 100k+ for periodic lattices; aperiodic stays cheaper because the substitution tree gives you free LOD (don't substitute below pixel size).

### Persistence

A workbook with an irregular tessellation needs to store enough on disk that the lattice can be reconstructed on load:

| Lattice | What's stored in `.tscl` |
|---|---|
| Square / hex / tri / parallelogram / composite | `LatticeKind` enum variant + extent params. ~50 bytes. |
| Voronoi | `LatticeKind::Voronoi { seeds: Vec<Point2>, bounds, seed_source }`. Linear in seed count; ~16 bytes per seed. |
| Drawn | `LatticeKind::Drawn { prototype: Vec<Point2>, rule: TilingRule, bounds }`. ~100 bytes per prototype vertex. |
| Aperiodic | `LatticeKind::Aperiodic { kind: enum, max_depth }`. Tiny — the substitution rule is hard-coded in the binary; only the depth and bounds matter. |

The persistence layer doesn't recompute the tessellation on save; that's a load-time operation.

---

## UX cross-cuts

How the user *experiences* each lattice from the front end.

### The wizard

The New-Workbook wizard's "Tessellation" step shows a card grid. Phase 1 has only Square enabled; later phases unlock cards in the order described in [Roadmap](#roadmap). Each card briefly describes its lattice:

| Card | Phase | One-line pitch |
|---|---|---|
| Square | 1 | Classic spreadsheet grid. 4 neighbours. |
| Hex (pointy) | 2 | Hexagons, point at the top. 6 neighbours. |
| Hex (flat) | 2 | Hexagons, flat at the top. 6 neighbours. |
| Triangle | 3 | Alternating up/down triangles. 3 neighbours. |
| Parallelogram | 3 | Skewed grid for isometric layouts. |
| Archimedean (configurator) | 6 | Pick polygons; the wizard builds a tiling. |
| Voronoi (random) | 7 | Seeded irregular tessellation. |
| Voronoi (Poisson-disk) | 7 | Seeded, but cells are area-uniform. |
| Voronoi (import points) | 7 | You supply the seed list. |
| Drawn (validate-a-shape) | 8 | Draw a polygon, we verify it tiles. |
| Penrose | 9+ | Aperiodic rhombus tiling. |
| Einstein hat | 9+ | Aperiodic single-tile tessellation. |

The "Extent" step depends on the chosen lattice:

| Lattice | Bounded extent UI |
|---|---|
| Square | Cols × rows (formula inputs). |
| Hex | Radius (formula input). |
| Triangle | Side length. |
| Parallelogram | u × v dimensions. |
| Archimedean | Per-polygon-type cell-count caps. |
| Voronoi | Seed count + region bounds. |
| Drawn | Translation-lattice extent + rotation count. |
| Aperiodic | Substitution depth (each level multiplies tile count by a constant). |

### Formula authoring

The user notices the lattice when they type a cell reference. The formula bar auto-completes addresses based on the active lattice; clicking a cell during edit-mode inserts that lattice's address syntax (`A1` on square, `H(2,-3)` on hex, `V<3a7c1e9b>` on Voronoi, etc.). For lattices without range syntax (Voronoi, aperiodic), drag-select inserts a `CELLS_IN_REGION(...)` call instead of `A1:B5`.

### Drag-select

| Lattice | Drag-select hull |
|---|---|
| Square / parallelogram | Rectangle. |
| Hex | Axial-aligned parallelogram (Phase 2 implementation). |
| Triangle | Triangular region. |
| Archimedean composite | Per-polygon-type rectangular hull (or composite). |
| Voronoi | Rectangular geometric region; selected cells are those whose centroid is inside. |
| Drawn | Rectangular in translation coords. |
| Aperiodic | Geometric region; selection is the array of cells whose centroid is inside. |

For irregular lattices, drag-select translates to `CELLS_IN_REGION(top_left, bottom_right)` in the formula bar.

---

## Roadmap

The order matches PLAN.md §11. Each lattice unlocks formula-authoring on top of the language baseline that already exists.

| Phase | Lattice work | Carbide-language impact |
|---|---|---|
| 1 ✅ | Square. | Square address syntax (this doc's starting point). |
| 2 | Hex (pointy + flat), `NEIGHBORS`/`RADIUS`. PyO3 Python engine. | `H(q,r)` address syntax. Hex-radius range function. |
| 3 | Triangle + parallelogram lattices. Rhai sandbox. | `T(x,y,▲)` + `P(u,v)` syntaxes. Triangular range. |
| 4 | rustc native compile. WebGL renderer. | None at the language level. |
| 5 | Tauri port + collaboration. | None. |
| 6 | Archimedean configurator. | `O(i,j)` / `S(i,j)` / per-polygon syntaxes. Vertex-configuration spec. |
| 7 | Voronoi. | `V<hash>` address. `ALL_CELLS`, `CELLS_IN_REGION`, `CELLS_WHERE`, `CELLS_NEAR`. |
| 8 | Drawn (validate-a-shape). | `D(i,j,r)` syntax. Tileability validator UI. |
| 9+ | Aperiodic (Penrose, einstein hat). | Hierarchical addressing. `CELLS_AT_DEPTH`, `PARENT`, `CHILDREN`. Reusable across both aperiodic kinds. |

Each phase adds at most one row to [addressing.md](addressing.md) and one row to [interop.md](interop.md). The bulk of this document — `tessellations.md` — gets richer as each phase lands.

---

## Open questions

Genuine design calls deferred to when each phase starts. Listed so the docs turn doesn't have to re-invent them.

1. **Voronoi seed-hash quantum.** What's the right grid resolution for the seed-hash quantisation? Too coarse → false collisions across user-imported point sets. Too fine → micro-edits change addresses. Likely tied to the workbook's bounded extent.

2. **Voronoi address rewriting on seed edit.** When a seed's position changes meaningfully enough to change its hash, what does the engine do to formulas referencing the old address? Options: (a) auto-rewrite to the new hash (silent — risky), (b) prompt the user (annoying for a 10k-seed sheet), (c) leave the formulas broken (`#REF!`) until the user fixes them. Probably (b) with a "rewrite all" affordance.

3. **Voronoi cell identity vs seed identity.** If a user deletes a seed, neighbor cells expand to cover the gap. Are formulas that referenced the deleted seed's *position* now broken, or do they "follow" the position into the neighbor that now contains it? Likely the former — addresses are by hash, not by region.

4. **Penrose / einstein hat address compactness.** Substitution-tree paths can get long (one byte per substitution level, N levels deep). A 10-level Penrose has 10⁵-ish tiles and 80-bit addresses. Is that acceptable, or do we need a hash-and-table scheme? Worth measuring once we have a real renderer.

5. **`CELLS_IN_REGION` precision.** On a Voronoi or aperiodic sheet, "centroid inside the rectangle" is one rule. Are there cases where the user wants "any vertex inside" or "any edge crosses"? Probably make `CELLS_IN_REGION` default to centroid and offer `CELLS_OVERLAPPING_REGION` as a stricter variant.

6. **Hat vs spectre default.** When the wizard's "einstein hat" card is selected, do we ship hat (requires reflections, more historically significant) or spectre (no reflections, slightly more recent) as the default? Probably hat (the famous one) with spectre as a toggle.

7. **Lattice-specific stdlib functions.** `NEIGHBORS`, `RADIUS`, `CELLS_NEAR`, `CELLS_AT_DEPTH`, `PARENT`, `CHILDREN` are functions whose behaviour depends on the lattice. Should they live in `excellite::funcs::lattice` (engine-level) and dispatch on the active sheet, or should the lattice itself contribute them to a per-sheet registry? Latter is cleaner; finalise in Phase 2 when the first non-square lattice arrives.

8. **Cross-lattice references.** A workbook with both a hex sheet and a Voronoi sheet — can a hex formula reference `Voronoi1!V<3a7c1e9b>`? Yes (cell-value reads always work). Cross-lattice *ranges* are forbidden (no shared coordinate system). Already documented in addressing.md but worth re-confirming.

When each phase ships, these get answered and the corresponding sections of this page (and [addressing.md](addressing.md), [interop.md](interop.md)) become canonical.

---

## References for the curious

- Grünbaum & Shephard, *Tilings and Patterns* (1987) — the standard reference for periodic + Archimedean tilings.
- Red Blob Games, *Hexagonal Grids* — practical coordinate-system reference for the hex lattice (axial vs offset vs cube).
- Penrose (1974) — *The Role of Aesthetics in Pure and Applied Mathematical Research*. The original Penrose paper.
- Smith, Myers, Kaplan, Goodman-Strauss (2023) — *[An aperiodic monotile](https://arxiv.org/abs/2303.10798)*. The einstein-hat paper.
- Smith, Myers, Kaplan, Goodman-Strauss (2023) — *[A chiral aperiodic monotile](https://arxiv.org/abs/2305.17743)*. The spectre paper.
- Senechal, *Quasicrystals and Geometry* (1995) — for the connection between aperiodic tilings and physics.
