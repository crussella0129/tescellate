Finalized - DO NOT EDIT

# Sprint 9 Test Plan

## Unit Tests

### T-901 (Carbide label)
- Compile-time + grep guard (no test should pin "Excelite"). If a string
  assertion exists, update it to "Carbide".

### T-905 (round-trip)
- `widgets_generic_with_voronoi_coord_round_trip` (in `widget::tests`): a
  `Widgets<VoronoiCoord>` with Button + Toggle round-trips through JSON.
- `snapshot_round_trips_through_ui_state` (existing, extended): fixture
  populates `voronoi_widgets`; round-trip preserves it.

### T-902 / T-903 / T-904 (field + render + seed)
- Compile-time + manual / E2E.

## Integration Tests
- Covered by the unit-test round-trip; the per-lattice pattern is already
  exercised by square/hex/triangle.

## End-to-End Tests
- **Status:** possible (manual).
- `e2e_voronoi_toggle_widget`: launch app, Voronoi tab, click the toggle
  on V(5) — the checkbox flips TRUE/FALSE; no text bleeds behind it.
- `e2e_carbide_label`: open the language picker on any cell; the built-in
  engine reads "Carbide", not "Excelite".
