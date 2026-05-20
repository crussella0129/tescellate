Finalized - DO NOT EDIT

# Sprint 6 Test Plan

## Unit Tests

### T-601 (Coord impl)
- `voronoi_coord_step_back_is_identity`: `VoronoiCoord(3).step_back()` returns the same coord.
- `voronoi_coord_min_max_is_single_cell`: `min_max(VoronoiCoord(2), VoronoiCoord(5))` returns `(VoronoiCoord(2), VoronoiCoord(2))`.

### T-602..T-606 (app fields + render + tab)
- Compile-time only — render-path testing without a winit event loop is impractical. `cargo build` (native + wasm) covers compilation; the E2E run covers behaviour.

## Integration Tests

- The engine ↔ tess ↔ UI handshake is covered by `cargo build --workspace`. No new integration harness this sprint.

## End-to-End Tests

- **Status:** possible (manual).
- `e2e_voronoi_tab_renders`: launch app, click the "Voronoi" tab; verify eight cell polygons render with values inside.
- `e2e_voronoi_cell_select`: click a Voronoi cell; verify the selection stroke moves to that cell and the formula bar shows `V(N)`.
- `e2e_voronoi_cell_edit`: F2 / double-click; type new value; Enter; verify update.
- `e2e_voronoi_saves_to_tscl`: edit a Voronoi cell, Ctrl+S, Ctrl+O, verify state restored.
