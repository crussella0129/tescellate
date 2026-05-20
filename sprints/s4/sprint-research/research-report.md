# Sprint 4 Research Report

## 1. Sprint Goal

Close the ADR-005 follow-up: extend `Widgets<K>` to the triangle sheet
so all three lattices carry the same widget surface. The launch-brief
demo set (Budget / Hex Game / Voronoi) doesn't strictly need triangle
widgets, but completing the per-lattice surface removes the asymmetry
that would otherwise appear odd in docs and screenshots.

Pure follow-up sprint — no new lattice, no new schema work, no new deps.

## Decisions Reviewed

- **ADR-005 (sprint 2)** — `Widgets<K>` lattice-generic pattern. Sprint 4 instantiates the third coord type (`TriCoord`), closing the explicit follow-up noted in that ADR.
- **ADR-006 (sprint 2)** — Hex widgets ship Button + Toggle; Slider / ProgressBar deferred because the per-cell footprint is too small. Triangle has the same constraint (56-side equilateral, similar inscribed-rect area), so sprint 4 inherits the same deferral.
- **ADR-001 (sprint 0)** — `.tscl` `ui.json` sidecar schema. Adding `triangle_widgets` to `UiSnapshot` is the established schema-evolution pattern: new field with `#[serde(default)]` so older snapshots tolerate the addition.

## 2. Existing Code Survey

| File | Relevance | Notes |
|------|-----------|-------|
| `apps/tescellate-ui/src/app.rs` | high | Has `square_widgets: Widgets<(u32, u32)>` (sprint 0) and `hex_widgets: Widgets<HexCoord>` (sprint 2). Sprint 4 adds `triangle_widgets: Widgets<TriCoord>` in the same shape. Render path lives in `draw_triangle_grid` around line 4220; insert the widget dispatch right before the in-cell edit overlay (line 4482), mirroring the hex pass added in sprint 2. |
| `apps/tescellate-ui/src/state_io.rs` | medium | `UiSnapshot` carries `square_widgets` + `hex_widgets` already. Add `triangle_widgets: Widgets<TriCoord>` with `#[serde(default)]` (no alias needed — this field is brand new, older snapshots default to empty). |
| `apps/tescellate-ui/src/widget.rs` | low | `Widgets<K>` is already coord-generic from sprint 2; `TriCoord` already has the required derives (`Eq + Hash + Copy + Serialize`). No changes here. |
| `crates/tescellate-tess/src/triangle.rs` | low | `TriangleLattice::centroid(coord)` and `TriCoord::new` already exist. `triangle_in_view` is the visibility predicate. |

## 3. External Sources
None — pure in-repo extension.

## 4. Risks, Unknowns, Dependencies

- **Risk — triangle render pass layering:** the triangle widget pass needs to slot between the existing text pass and the selection-stroke pass, same as hex. Insert at app.rs:4480 (before in-cell-edit overlay) — sprint 2 used the same insertion point in `draw_hex_grid`.
- **Risk — TriCoord widgets shape collision with seed cells:** the existing triangle demo seeds T(col, row) cells "up"/"dn"/labels. The new widget needs to land on a coord that doesn't have a seed value. Pick `TriCoord::new(2, -1)` (outside the existing seed pattern) as the v148 demo button cell.
- **Unknown — does the triangle's inscribed rect fit a button?** Triangles are 56-side equilateral, so the inscribed rectangle is roughly `36 × 24` (rough — exact dimensions in code). Button + Toggle should fit; Slider / ProgressBar fall through to text render (same deferral as hex per ADR-006).
- **Dependency — none new.**

## 5. Recommended Approach

**Primary — one focused commit, mirroring the sprint 2 hex pattern.**

1. **T-401: `triangle_widgets` field + plumbing.** Add
   `triangle_widgets: Widgets<TriCoord>` to `TescellateApp`. Initialize
   `Widgets::default()` in `new`. Update `capture_state` and
   `restore_state` to round-trip the field. Update `UiSnapshot` to
   carry `triangle_widgets: Widgets<TriCoord>` with
   `#[serde(default)]`.
2. **T-402: Triangle render dispatch.** In `draw_triangle_grid`, after
   the text pass and before the in-cell edit overlay, iterate
   `self.triangle_widgets.iter()`. For each `(coord, kind)`:
   - Compute centroid via `self.triangle_lattice.centroid(coord)`.
   - Compute inscribed rect: `Rect::from_center_size(center, vec2(TRIANGLE_SIDE * 0.7, TRIANGLE_SIDE * 0.4))`.
   - Dispatch Button (re-fires source) + Toggle (writes `bool_source`); Slider / ProgressBar fall through to text per ADR-006.
3. **T-403: Optional demo seed.** Tag one of the existing triangle demo cells (`T(2, -1)` works — outside the existing "up"/"dn" pattern) with `WidgetKind::Toggle`. Minimal — just so launch screenshots show a triangle widget at all.
4. **T-404: Tests.** Add `widgets_generic_with_tri_coord_round_trip` to `widget::tests`. Update `snapshot_round_trips_through_ui_state` to include `triangle_widgets` in the fixture.
5. **T-405: CI gate + PR `webui-v148-triangle-widgets`.**

**Alternative considered — bundle XLOOKUP wildcard match (P2 from launch brief) into this sprint.** Rejected: different code paths, different review surface, and wildcard match is a non-trivial glob engine. Stays on the backlog.

**Rationale:** Triangle widgets close the per-lattice symmetry loop. They're a 30-minute change at sprint 2's pace and remove a "but why does triangle behave differently?" papercut. Bigger items (Voronoi lattice, OFFSET/INDIRECT, wildcard XLOOKUP) deserve their own sprint cycles.

## Artifacts
None.
