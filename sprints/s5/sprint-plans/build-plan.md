Finalized - DO NOT EDIT

# Sprint 5 Build Plan

## Schema Tree

- **Sprint Goal:** v149 — Voronoi lattice engine bringup (Lattice trait impl + LatticeHandle dispatch + address format). UI render + engine `add_sheet` integration land in v150.
  - **Component A — Module**
    - T-501: `voronoi.rs` module with `VoronoiCoord` + `VoronoiLattice`.
  - **Component B — Lattice trait impl**
    - T-502: All six trait methods + Sutherland-Hodgman vertex computation.
  - **Component C — Handle dispatch**
    - T-503: `LatticeKind::Voronoi`, `LatticeHandle::Voronoi`, `ParsedCoord::Voronoi` variants + every match arm.
  - **Component D — Tests + ship**
    - T-504: Unit tests for the lattice + the handle round-trips.
    - T-505: CI gate + PR `carbide-v149-voronoi-engine`.

## Execution Sequence

### T-501: New `voronoi.rs` module.
- **Touches:** `crates/tescellate-tess/src/voronoi.rs` (new)
- **Depends on:** (none)
- **Success criterion:** Module defines:
  - `VoronoiCoord(pub u32)` deriving `Debug + Clone + Copy + PartialEq + Eq + Hash + Serialize + Deserialize`.
  - `VoronoiLattice { seeds: Vec<Point2>, bounds: Rect }` with constructor `new(seeds, bounds) -> Result<Self, AddressError>` validating non-empty + pairwise-distinct seeds + finite bounds.
  - `VoronoiLattice::default()` builds an 8-seed launch configuration spread across a 400×400 bounding box (centred at origin).
  - Module is referenced from `lib.rs` via `pub mod voronoi`.

### T-502: `Lattice` trait impl + Sutherland-Hodgman.
- **Touches:** `crates/tescellate-tess/src/voronoi.rs`
- **Depends on:** T-501
- **Success criterion:** `impl Lattice for VoronoiLattice`:
  - `Coord = VoronoiCoord`.
  - `kind() -> LatticeKind::Voronoi`.
  - `address(c)` returns `"V(i)"`.
  - `parse_address("V(i)")` returns `VoronoiCoord(i)`; out-of-range index errors `OutOfRange`.
  - `centroid(c)` returns `self.seeds[c.0 as usize]`.
  - `vertices(c)` returns the Voronoi cell polygon as a `SmallVec<[Point2; 8]>`. Implementation: start with the 4 corners of `self.bounds`; for each other seed, intersect the polygon with the half-plane `{p : ‖p − seeds[c.0]‖ ≤ ‖p − seeds[j]‖}` via Sutherland-Hodgman.
  - `cell_at(p)` returns `Some(argmin VoronoiCoord)` when `p` is inside `bounds`, else `None`.
  - `neighbors(c)` returns every other seed paired with `Direction::N` (placeholder).
- **Notes:** Sutherland-Hodgman: iterate over edges of the current polygon; for each edge (a, b), check both endpoints against the half-plane; emit the surviving / clipped endpoints. ~50 lines.

### T-503: `LatticeKind::Voronoi` + `LatticeHandle::Voronoi` + `ParsedCoord::Voronoi`.
- **Touches:** `crates/tescellate-tess/src/lib.rs`
- **Depends on:** T-502
- **Success criterion:** All three enums gain a Voronoi variant. `LatticeHandle::for_kind(LatticeKind::Voronoi)` returns `Some(LatticeHandle::Voronoi(VoronoiLattice::default()))`. Every match in `LatticeHandle` and the canonicalisation paths handles the new variant. `lattice_distance` for Voronoi returns `Ref` (no canonical distance on a Voronoi diagram yet) — same shape as `Triangle`'s first-cut metric.

### T-504: Tests.
- **Touches:** `crates/tescellate-tess/src/voronoi.rs` (tests submodule), `crates/tescellate-tess/src/lib.rs` (handle tests)
- **Depends on:** T-502, T-503
- **Success criterion:** Five new tests pass:
  - `voronoi_cell_at_returns_nearest_seed`
  - `voronoi_address_round_trips`
  - `voronoi_vertices_are_inside_bounds`
  - `voronoi_centroid_is_seed_point`
  - `voronoi_handle_canonicalises`

### T-505: CI gate + PR.
- **Touches:** (verification + git)
- **Depends on:** T-501..T-504
- **Success criterion:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` all green. PR `carbide-v149-voronoi-engine` opened, CI green, squash-merged.
