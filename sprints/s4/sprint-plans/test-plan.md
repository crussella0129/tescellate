Finalized - DO NOT EDIT

# Sprint 4 Test Plan

## Unit Tests

### T-401 (field + UiSnapshot)
- Compile-time + the round-trip test below.

### T-402 (render)
- Manual / E2E — render path testing without a winit event loop is impractical; `cargo build` covers compilation.

### T-403 (demo seed)
- Manual / E2E.

### T-404 (round-trip)
- `widgets_generic_with_tri_coord_round_trip` (in `widget::tests`): a `Widgets<TriCoord>` with Button + Toggle round-trips through JSON.
- `snapshot_round_trips_through_ui_state` (existing, extended): fixture populates `triangle_widgets`; round-trip preserves it.

## Integration Tests

Covered by the unit-test round-trip — engine-side is unchanged from
sprint 3 and the per-lattice pattern is already exercised by hex/square.

## End-to-End Tests

- **Status:** possible (manual).
- `e2e_triangle_toggle_widget`: launch app, switch to triangle sheet,
  click the toggle at T(2, -1), verify the cell value flips
  TRUE ↔ FALSE.
- `e2e_triangle_widget_survives_autosave`: edit the toggle, wait 3 s,
  F5, verify state restored.
