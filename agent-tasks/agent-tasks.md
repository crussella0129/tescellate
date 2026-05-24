# Agent Tasks (Persistent Backlog)

## Sprint 17 — Voronoi interaction parity (v159)

- **T-001** — `Selection<C>` gains `extra` + `primary_cells()` accessor; migrate operational consumers in `app.rs` (copy/format/widget) to `primary_*`.
- **T-002** — `formula_mode::dispatch` shared helper + `Event<C>` enum.
- **T-003** — `formula_mode::event_from_response` testable translator; switch square/hex/triangle `draw_*_grid` formula-mode blocks to `dispatch`.
- **T-004** — `draw_voronoi_grid` calls `dispatch` (click-to-insert + drag-extend parity).
- **T-005** — `cells_in_screen_rect` pure helper (centroid-in-rect).
- **T-006A** — `draw_voronoi_grid` main-response drag populates `selection.extra` via the marquee helper (data path).
- **T-006B** — Seed-handle precedence rule + translucent marquee overlay.
- **T-007** — Diagnose & fix Voronoi name-box / formula-bar refresh (with case-(c) defer rule).
- **T-008** — ADR-013 in `decisions.md` (cross-refs ADR-005/009/012).
