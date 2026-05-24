Finalized - DO NOT EDIT

# Sprint 6 Build Plan

## Schema Tree

- **Sprint Goal:** v150 — wire the Voronoi engine bringup all the way through to the UI; complete Demo C with a 4th sheet tab, polygon rendering, and minimal interaction.
  - **Component A — Coord plumbing**
    - T-601: `impl Coord for VoronoiCoord` in `selection.rs` (degenerate arithmetic; single-cell selection only).
  - **Component B — App-level fields + active sheet**
    - T-602: `voronoi_lattice`, `voronoi: Sheet<VoronoiCoord>` fields on `CarbideApp`; engine.add_sheet seeds them.
    - T-603: `ActiveSheet::Voronoi`, `CellId::Voronoi(VoronoiCoord)` variants + every match arm.
  - **Component C — Render**
    - T-604: `draw_voronoi_grid` function: polygon paint, text, selection stroke, click hit-test, in-cell edit overlay.
  - **Component D — Demo + tab + ship**
    - T-605: Demo C seed cells.
    - T-606: 4th tab in the tab bar; update-dispatch routes to the new render fn.
    - T-607: CI gate + PR `webui-v150-voronoi-ui`.

## Execution Sequence

### T-601: `impl Coord for VoronoiCoord`.
- **Touches:** `apps/carbide-ui/src/selection.rs`
- **Depends on:** (none)
- **Success criterion:** Compiles. `step_back` returns self; `min_max(a, b)` returns `(a, a)` (single-cell only); `rect_cells` returns `vec![self]`; `rect_contains` is `false` (no range concept).
- **Notes:** Selection on Voronoi is single-cell only.

### T-602: CarbideApp fields.
- **Touches:** `apps/carbide-ui/src/app.rs`
- **Depends on:** T-601
- **Success criterion:** `voronoi_lattice: VoronoiLattice` + `voronoi: Sheet<VoronoiCoord>` fields exist; initialised in `new` after the Triangle sheet is created. `engine.add_sheet("Voronoi", LatticeKind::Voronoi)` returns the sheet id used by `Sheet<VoronoiCoord>`.

### T-603: `ActiveSheet::Voronoi` + `CellId::Voronoi`.
- **Touches:** `apps/carbide-ui/src/app.rs`
- **Depends on:** T-602
- **Success criterion:** Both enums gain a Voronoi variant. Every `match self.active` site has a Voronoi arm (pass-through to `draw_voronoi_grid` in render; no-op for square-grid-specific paths like fill drag). Build clean.

### T-604: `draw_voronoi_grid`.
- **Touches:** `apps/carbide-ui/src/app.rs`
- **Depends on:** T-602, T-603
- **Success criterion:** New `draw_voronoi_grid` method renders polygons + text + selection stroke. Click hit-test via `voronoi_lattice.cell_at(local)`. In-cell edit overlay sized to a small rect at the centroid.

### T-605: Demo C seed cells.
- **Touches:** `apps/carbide-ui/src/app.rs`
- **Depends on:** T-602
- **Success criterion:** Eight `engine.set_cell(voronoi_sheet, "V(N)", Some(...))` calls populate `V(0)..V(7)` with demo values. Build clean.

### T-606: Tab bar + dispatch.
- **Touches:** `apps/carbide-ui/src/app.rs`
- **Depends on:** T-603, T-604
- **Success criterion:** A 4th "Voronoi" tab appears in the tab bar; clicking it sets `self.active = ActiveSheet::Voronoi` and routes render to `draw_voronoi_grid`.

### T-607: CI gate + PR.
- **Touches:** (verification + git)
- **Depends on:** T-601..T-606
- **Success criterion:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, plus UI clippy/test, plus `cargo build --target wasm32-unknown-unknown` all green. PR `webui-v150-voronoi-ui` opened, CI passes, squash-merged.
