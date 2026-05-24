Finalized - DO NOT EDIT

# Sprint 10 Build Plan

## Schema Tree

- **Sprint Goal:** v153 — viewport-cull the square grid render.
  - **Component A — Visible-range helpers**
    - T-1001: `GridMetrics::visible_col_range` + `visible_row_range` + unit tests.
  - **Component B — Apply culling**
    - T-1002: Use the ranges in `draw_grid`'s four cell loops.
  - **Component C — Ship**
    - T-1003: CI gate + PR `webui-v153-viewport-culling`.

## Execution Sequence

### T-1001: Visible-range helpers in `GridMetrics`.
- **Touches:** `apps/carbide-ui/src/grid.rs`
- **Depends on:** (none)
- **Success criterion:** Two methods:
  - `visible_col_range(&self, origin_x: f32, clip_left: f32, clip_right: f32, cols: u32) -> (u32, u32)`
  - `visible_row_range(&self, origin_y: f32, clip_top: f32, clip_bottom: f32, rows: u32) -> (u32, u32)`
  
  Each walks the axis once accumulating cell positions, returns the inclusive index span of cells whose `[left, right]` (or `[top, bottom]`) overlaps the clip window. Empty axis → `(0, 0)`. The walk starts the position accumulator at `HEADER_W` / `HEADER_H` to match `col_left`/`row_top`. New unit tests: `visible_range_full_when_everything_fits`, `visible_range_windows_when_scrolled`, `visible_range_includes_boundary_straddle`.
- **Notes:** Inclusive overlap test: `cell_right >= rel_left && cell_left <= rel_right` where `rel_* = clip_* - origin_*`.

### T-1002: Apply culling in `draw_grid`.
- **Touches:** `apps/carbide-ui/src/app.rs`
- **Depends on:** T-1001
- **Success criterion:** Near the top of the paint section, compute `let (c0, c1) = self.metrics.visible_col_range(origin.x, ui.clip_rect().left(), ui.clip_rect().right(), COLS);` and the row analogue. Change the four `for r in 0..ROWS` / `for c in 0..COLS` loops (main paint, heavy-border, widget, header strips) to `for r in r0..=r1` / `for c in c0..=c1`. Build clean; the grid still renders correctly when scrolled (verified manually).
- **Notes:** The header strips iterate one axis each — row header uses `r0..=r1`, column header uses `c0..=c1`.

### T-1003: CI gate + PR.
- **Touches:** (verification + git)
- **Depends on:** T-1001, T-1002
- **Success criterion:** fmt + clippy (workspace + UI) + `cargo test --workspace` + wasm build all green. PR `webui-v153-viewport-culling` opened, CI passes, squash-merged.
