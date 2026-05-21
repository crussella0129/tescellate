Finalized - DO NOT EDIT

# Sprint 10 Test Plan

## Unit Tests

### T-1001 (visible-range helpers)
- `visible_range_full_when_everything_fits`: a clip window wider than the
  whole grid returns `(0, COLS-1)` / `(0, ROWS-1)`.
- `visible_range_windows_when_scrolled`: with the grid origin scrolled
  left/up (origin coord negative), the returned start index is > 0 and
  the end index < the axis max — a proper interior window.
- `visible_range_includes_boundary_straddle`: a cell whose left edge is
  just left of the clip boundary but whose right edge crosses into the
  viewport is included (no blank edge strip).
- `visible_range_empty_axis_is_zero_zero`: `cols = 0` → `(0, 0)`.

### T-1002 (apply culling)
- Compile-time + manual E2E — render-path verification needs the running
  app.

## Integration Tests
- Covered by the unit tests on the range math + the manual render check.

## End-to-End Tests
- **Status:** possible (manual).
- `e2e_grid_renders_when_scrolled`: launch app, scroll the Budget sheet
  right + down, verify cells render fully (no blank strips at the
  viewport edges, no missing cells at the scroll boundary).
- `e2e_perf_improved`: subjectively confirm the debug-build frame rate
  is materially better than v152 (the grid no longer paints 10,400
  cells/frame).
