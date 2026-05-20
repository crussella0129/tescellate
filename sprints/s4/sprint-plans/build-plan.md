Finalized - DO NOT EDIT

# Sprint 4 Build Plan

## Schema Tree

- **Sprint Goal:** v148 — extend `Widgets<K>` to the triangle sheet, completing the per-lattice widget surface (ADR-005 follow-up).
  - **Component A — Field + persistence**
    - T-401: `triangle_widgets: Widgets<TriCoord>` on `TescellateApp`; UiSnapshot gains `triangle_widgets` with `#[serde(default)]`; capture/restore round-trip.
  - **Component B — Render**
    - T-402: Triangle render dispatch for Button + Toggle in `draw_triangle_grid`.
  - **Component C — Demo touch**
    - T-403: Seed one triangle cell as a Toggle widget so launch screenshots show the surface in use.
  - **Component D — Tests + ship**
    - T-404: Unit tests for the new field round-trip.
    - T-405: CI gate + PR `webui-v148-triangle-widgets`.

## Execution Sequence

### T-401: `triangle_widgets` field + UiSnapshot field.
- **Touches:** `apps/tescellate-ui/src/app.rs`, `apps/tescellate-ui/src/state_io.rs`
- **Depends on:** (none)
- **Success criterion:** `triangle_widgets: Widgets<TriCoord>` on `TescellateApp`, initialised `Widgets::default()` in `new`. `UiSnapshot.triangle_widgets: Widgets<TriCoord>` with `#[serde(default)]`. `capture_state` clones the field into the snapshot; `restore_state` calls `replace_with` on it. Build clean.
- **Notes:** Use `TriCoord` from `tescellate_tess::triangle` — already imported at the top of `app.rs`.

### T-402: Triangle render dispatch.
- **Touches:** `apps/tescellate-ui/src/app.rs`
- **Depends on:** T-401
- **Success criterion:** A new widget pass in `draw_triangle_grid` (after the text pass, before the in-cell edit overlay) iterates `self.triangle_widgets.iter()`. For each (coord, kind):
  - Centroid via `self.triangle_lattice.centroid(coord)`.
  - Inscribed rect: `Rect::from_center_size(center, vec2(TRIANGLE_SIDE * 0.7, TRIANGLE_SIDE * 0.4))`.
  - Button → `egui::Button::new(label)`; on click, re-fire source (mirrors hex Button).
  - Toggle → `egui::Checkbox`; on change, write `bool_source`.
  - Slider / ProgressBar fall through (same deferral as hex per ADR-006).
- **Notes:** Re-firing the source uses `engine.set_cell(self.triangle.sheet_id, &triangle_address(coord), Some(source.as_str()))`.

### T-403: Demo seed — a Toggle widget on a triangle cell.
- **Touches:** `apps/tescellate-ui/src/app.rs`
- **Depends on:** T-402
- **Success criterion:** `triangle_widgets` initialiser inserts `(TriCoord::new(2, -1), WidgetKind::Toggle)` (outside the existing seed pattern). The triangle demo seeds also gain `("T(2,-1)", "FALSE")` so the checkbox starts unchecked. Build clean.
- **Notes:** Visual verification deferred to manual E2E.

### T-404: Tests.
- **Touches:** `apps/tescellate-ui/src/widget.rs`, `apps/tescellate-ui/src/state_io.rs`
- **Depends on:** T-401
- **Success criterion:** New test `widgets_generic_with_tri_coord_round_trip` asserts a `Widgets<TriCoord>` with Button + Toggle entries JSON round-trips. Existing `snapshot_round_trips_through_ui_state` fixture extended to populate `triangle_widgets`; assertion added that the field round-trips. `cargo test -p tescellate-ui` green.

### T-405: CI gate + PR.
- **Touches:** (verification + git)
- **Depends on:** T-401..T-404
- **Success criterion:** `cargo fmt --all --check`, `cargo clippy` (workspace + UI), `cargo test --workspace`, `cargo build --target wasm32-unknown-unknown` all green. PR `webui-v148-triangle-widgets` opened, CI passes, squash-merged.
