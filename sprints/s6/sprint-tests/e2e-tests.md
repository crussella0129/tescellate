# Sprint 6 End-to-End Tests

**Status:** possible (manual).

- `e2e_voronoi_tab_renders`: launch app (native or wasm), click the
  "Voronoi" tab, verify eight cell polygons render with their values
  (Plains, Forest, 42, Tundra, 50 from `=V(2) + 8`, Desert, Coast,
  Highlands) visible inside.
- `e2e_voronoi_cell_select`: click a cell, verify the selection stroke
  moves to that cell and the formula bar shows `V(N)`.
- `e2e_voronoi_cell_edit`: double-click a cell, type a new value,
  press Enter, verify the cell value updates.
- `e2e_voronoi_formula_re_evals`: edit `V(2)` to a new number, verify
  `V(4)` (which is `=V(2) + 8`) updates in step.
- `e2e_voronoi_saves_to_tscl`: edit a Voronoi cell, Ctrl+S → save,
  Ctrl+O → reload, verify the Voronoi sheet rehydrates with the
  edited value.

Automated browser E2E (Playwright / wdio against the served wasm) is
still the right next-sprint addition once the launch demos go public.
