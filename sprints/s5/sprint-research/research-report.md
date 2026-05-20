# Sprint 5 Research Report

## 1. Sprint Goal

Start the Voronoi-lattice bringup (Demo C from the launch brief, which
the user reaffirmed as first-class). Scope cut tight: this sprint
ships the **engine-side foundation** — types, Lattice trait impl,
address format, LatticeHandle dispatch — with **no UI render and no
engine `add_sheet` integration yet**. The follow-up sprint (v150) wires
those layers on top of the stable engine surface.

Why not all-in-one: a complete Voronoi feature touches new types, a
new polygon-computation algorithm, parser changes, render path
changes, and demo seeds. Splitting at the LatticeHandle line means
each piece is reviewable on its own merits and we get a stable
geometric primitive committed before any consumer leans on it.

## Decisions Reviewed

- **None — Voronoi is the first new lattice since the project's
  inception.** Sprint 5's choices set the precedent for any future
  lattice (Delaunay triangulation, Penrose tiling, etc.).

## 2. Existing Code Survey

| File | Relevance | Notes |
|------|-----------|-------|
| `crates/tescellate-tess/src/lib.rs` | high | Defines `LatticeHandle`, `LatticeKind`, `ParsedCoord`, `Lattice` trait, `Point2`, `Rect`, `AddressError`, `Direction`. Sprint 5 adds a `Voronoi` variant to each of `LatticeHandle`, `LatticeKind`, `ParsedCoord`. The `Lattice` trait stays unchanged. |
| `crates/tescellate-tess/src/hex.rs` | medium | Template for a non-trivial Lattice impl. `HexLattice` carries `size` + `orientation`; `cell_at` does cube rounding; `vertices` computes 6 corners around the centroid. Voronoi follows the same shape but `cell_at` is "nearest seed" and `vertices` is a clipped polygon. |
| `crates/tescellate-tess/src/triangle.rs` | low | Another Lattice template. Useful for the parse/format pattern (`T(col,row)` ↔ `TriCoord`). |
| `crates/tescellate-tess/Cargo.toml` | low | Already depends on `glam` (for `Vec2` → `Point2`), `smallvec`, `serde`, `thiserror`. Sprint 5 adds nothing new — Voronoi computation is handwritten with no crate. |

## 3. External Sources

- [Sutherland–Hodgman polygon clipping](https://en.wikipedia.org/wiki/Sutherland%E2%80%93Hodgman_algorithm) — the algorithm used to compute each Voronoi cell as the intersection of half-planes ("closer to seed S than to seed T") clipped against the bounding rectangle. Pure-geometry, ~50 lines of Rust at small N. Sprint 5's brute-force O(N²) implementation is acceptable for the launch demo (<= 15 seeds); a Delaunay-driven algorithm can replace it later without changing the public API.
- [Voronoi cell — definition](https://en.wikipedia.org/wiki/Voronoi_diagram) — the cell of seed S is `{p : ‖p − S‖ ≤ ‖p − T‖ for all other T}`. Each `(S, T)` pair contributes one half-plane bounded by the perpendicular bisector of ST.

No external crate dep this sprint.

## 4. Risks, Unknowns, Dependencies

- **Risk — bounded vs unbounded cells.** A Voronoi cell can be unbounded; clipping against the lattice's `bounds: Rect` makes every cell finite. The launch demo is "Static Voronoi" with seeds inside a known rectangle, so the bounded form is correct. Document the bounded assumption in the type's doc comment.
- **Risk — degenerate seed configurations (coincident or near-coincident seeds).** Sutherland-Hodgman produces correct output for distinct seeds; for coincident seeds the bisector is degenerate. Mitigation: validate seeds are pairwise distinct in `VoronoiLattice::new`; return `AddressError::OutOfRange` for `V(N)` when `N >= seeds.len()`.
- **Risk — `Direction` enum doesn't have Voronoi labels.** Voronoi neighbors don't have canonical compass directions — each cell has variable neighbor count and adjacency is purely topological. Approach: pick a placeholder `Direction::N` for every Voronoi neighbor (cheaply correct since callers that care about direction are square/hex-specific). The `NEIGHBORS()` Carbide function only needs the values, not the labels.
- **Risk — performance.** O(N² × C) for vertex computation where C is the clipped polygon size. At N = 15 seeds this is ~225 half-plane clips × ~4-6 polygon edges each = ~1500 operations per cell, called once per render frame. Acceptable for launch; switch to a Delaunay-driven O(N log N) build if performance bites.
- **Unknown — should the lattice carry its own seeds, or take them at construction time only?** Decision: seeds are part of the lattice struct (`VoronoiLattice { seeds, bounds }`). This is asymmetric with HexLattice (uniform geometry, no per-cell state) but necessary because Voronoi cells aren't determined by the lattice rule alone — the seed configuration IS the lattice.
- **Dependency — no new crates.**

## 5. Recommended Approach

**Primary — engine plumbing only, no integration.**

1. **T-501: New `voronoi.rs` module in `tescellate-tess`** with `VoronoiCoord(pub u32)`, `VoronoiLattice { seeds: Vec<Point2>, bounds: Rect }`, and `VoronoiLattice::new(seeds, bounds) -> Result<Self, …>` (rejects coincident seeds; rejects empty seed list).
2. **T-502: `Lattice` impl for `VoronoiLattice`.**
   - `kind` returns `LatticeKind::Voronoi`.
   - `address(VoronoiCoord(i)) -> "V(i)"`.
   - `parse_address("V(i)") -> VoronoiCoord(i)` with `OutOfRange` if `i >= seeds.len()`.
   - `centroid(c) -> seeds[c.0]`.
   - `vertices(c) -> ` Sutherland-Hodgman clipping of `bounds` against every `(seeds[c.0], seeds[j])` bisector for `j != c.0`.
   - `cell_at(p) -> ` argmin over seed distances; returns `None` if `p` is outside `bounds`.
   - `neighbors(c) -> ` simplification: every other seed (Direction::N placeholder). True Delaunay neighbors lands in v150.
3. **T-503: `LatticeKind::Voronoi` variant** in `lib.rs`. `LatticeHandle::Voronoi(VoronoiLattice)` variant. `for_kind(LatticeKind::Voronoi)` constructs a default 8-seed configuration in a 400×400 bounding box. `ParsedCoord::Voronoi(VoronoiCoord)`. Update every `LatticeHandle` method's match (parse_coord, format_coord, canonicalize, enumerate_range, neighbor_addresses, cells_within_addresses, lattice_distance).
4. **T-504: Tests** for the new module:
   - `voronoi_cell_at_returns_nearest_seed`
   - `voronoi_address_round_trips`
   - `voronoi_vertices_are_nonempty_and_inside_bounds`
   - `voronoi_centroid_is_seed_point`
   - `voronoi_handle_canonicalises_addresses`
5. **T-505: CI gate + PR `carbide-v149-voronoi-engine`** (engine-only).

**Alternative considered — full Voronoi (engine + add_sheet + UI render) in one sprint.** Rejected: bigger diff, harder review, mixes geometric algorithm work with UI render-pass work and engine sheet plumbing. Splitting at the LatticeHandle line keeps each sprint scoped to one concern.

**Alternative considered — pull in the `voronator` crate.** Rejected for this sprint: a crate dep is non-trivial to vet across wasm32 + native CI; handwritten Sutherland-Hodgman is small enough that the cost-benefit favors no new dep. If the Delaunay-driven O(N log N) algorithm becomes necessary later, swap implementations behind the same public API.

**Rationale:** Voronoi is the last big launch demo. Engine-side is the foundation; doing it carefully (with tests, scoped diff) is worth a focused sprint. The UI render lands fast on top of a stable engine surface.

## Artifacts
None.
