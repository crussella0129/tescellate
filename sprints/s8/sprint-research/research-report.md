# Sprint 8 Research Report

## 1. Sprint Goal

Two visual fixes the user surfaced during the v150 release-build review:

1. **Hex + triangle Toggle widgets show "TRUE"/"FALSE" through the
   checkbox.** The text-pass paints the cell value at the centroid; the
   widget pass paints the checkbox on top, and the bool-source text
   bleeds through. Square solved this in v141 with a widget-skip gate;
   hex/triangle never picked it up.
2. **Sliders / Buttons / Progress bars look cramped at default cell
   sizes.** Default column ~64 px; a slider wants ~140–160 px. The user
   wants the cells to enforce a widget-fit minimum.

Pure polish sprint — no lattice, schema, or engine change.

## Decisions Reviewed

- **ADR-006 (sprint 2)** — Hex Button + Toggle render dispatch. The
  text-bleed gap is the existing widget-skip gate not being applied in
  the hex/triangle paint paths; the fix is symmetric with square's.

## 2. Existing Code Survey

| File | Relevance | Notes |
|------|-----------|-------|
| `apps/tescellate-ui/src/app.rs` | high | `paint_hex` calls `hex_cell_text` unconditionally; `draw_triangle_grid` text-pass likewise. `TescellateApp::new` is where the col/row min-size bumps land (after the struct binds). |
| `apps/tescellate-ui/src/grid.rs` | low | `set_col_width` / `set_row_height` already clamp to per-axis minimums. |
| `apps/tescellate-ui/src/widget.rs` | low | `Widgets<K>::iter()` drives the sizing-floor walk. |

## 3. External Sources
None.

## 4. Risks, Unknowns, Dependencies

- **Risk — overriding user column widths.** The floor bump runs once in
  `new()`, BEFORE the boot-rehydrate path. If localStorage carries a
  snapshot, `restore_state` runs afterward and overwrites metrics with
  the user's saved widths — so the floor only affects fresh sessions /
  rehydrate-misses. Saved widths win.
- **Unknown — apply the floor to hex/triangle/Voronoi too?** No — those
  lattices have polygon cells sized by geometry, not column widths.
  Their inscribed widget rects are already fit by the polygon math.
  Sprint 8 only touches the square grid.

## 5. Recommended Approach

**One commit, two surgical changes.**

1. **T-801: Hex text-pass widget skip** in `paint_hex`.
2. **T-802: Triangle text-pass widget skip** in `draw_triangle_grid`.
3. **T-803: Square widget cell sizing floor** in `TescellateApp::new` — Slider/Button/ProgressBar host columns ≥ 160 px, host rows ≥ 28 px; Toggle stays default.
4. **T-804: CI gate + PR `webui-v151-widget-polish`.**

**Alternative considered — float the slider rect over the column** instead of enforcing the cell minimum. Rejected: breaks the widgets-live-in-cells model the user explicitly wants preserved.

**Rationale:** Both fixes are small and user-verifiable in another release run. Bigger items (Voronoi widgets, Delaunay-driven seeds, square-grid viewport culling) live in later sprints.

## Artifacts
None.
