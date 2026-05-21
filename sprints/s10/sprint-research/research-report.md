# Sprint 10 Research Report

## 1. Sprint Goal

Cull the square-grid render to the cells visible in the scroll viewport,
instead of painting all `COLS × ROWS = 52 × 200 = 10,400` cells every
frame. The headline perf fix the user reacted to during the v150
release-build review.

## Decisions Reviewed
- No prior ADR bears directly on render culling. May produce ADR-010 if
  the visible-range contract is worth pinning.

## 2. Existing Code Survey

| File | Relevance | Notes |
|------|-----------|-------|
| `apps/tescellate-ui/src/app.rs` | high | `draw_grid` (line 3250) has four `for r in 0..ROWS { for c in 0..COLS }` loops: main cell paint (3650), heavy-border pass (3706), widget pass (3811 — already short-circuits via `is_widget`), and the frozen row/col header strips (3916/3936). Main + border are the hot path. |
| `apps/tescellate-ui/src/grid.rs` | high | `GridMetrics` owns `col_left`/`row_top` (O(index) prefix sums), `col_width`/`row_height`, `cell_rect`. `clip_rect` available in `draw_grid` via `ui.clip_rect()`. Add `visible_col_range`/`visible_row_range` (incremental walks). |

## 3. External Sources
None.

## 4. Risks, Unknowns, Dependencies

- **Risk — off-by-one at the viewport edge.** A cell straddling the clip
  boundary must still paint. The walk includes any cell whose span
  overlaps `[clip_left, clip_right]` inclusively. Tests cover it.
- **Risk — active-cell ring / range border reference `cursor` directly,
  not loops** — keep working unchanged.
- **Risk — frozen headers** also loop 0..ROWS/0..COLS; cull those too.
- **Deferred:** `cell_rect(c,r)` calls `col_left(c)` + `row_top(r)`
  (O(index)). Culling the count 10,400 → ~1,600 is the dominant win
  (~6.5×); the residual per-cell accumulation still walks from 0. An
  O(1) prefix-sum cache is a clean sprint-11 follow-up if still hot.
- **Dependency — none new.**

## 5. Recommended Approach

1. **T-1001: `GridMetrics::visible_col_range` + `visible_row_range`** —
   take the grid origin coord, clip-rect bounds, axis count; return an
   inclusive `(start, end)` of indices whose span overlaps the viewport,
   via a single incremental pass. Unit tests: fully-visible, scrolled
   window, boundary straddle, empty axis.
2. **T-1002: Apply ranges in `draw_grid`** — compute `(c0,c1)`/`(r0,r1)`
   once, change the four loops to `r0..=r1` / `c0..=c1`.
3. **T-1003: CI gate + PR `webui-v153-viewport-culling`.**

**Alternative considered — full prefix-sum cache making `cell_rect` O(1).** Deferred: bigger `GridMetrics` change (cache invalidation on resize); culling alone delivers the user-felt win.

**Rationale:** Culling is the contained, high-leverage fix — 6.5× fewer painted cells with a small, tested diff.

## Artifacts
None.
