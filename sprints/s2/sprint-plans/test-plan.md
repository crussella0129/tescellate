Finalized - DO NOT EDIT

# Sprint 2 Test Plan

## Unit Tests

### T-201a (Widgets generic)
- `widgets_generic_with_hex_coord_round_trip`: Build a `Widgets<HexCoord>`, insert Toggle + Button entries, JSON round-trip via `serde_json::to_value` / `from_value`, assert the map matches.
- Existing square widget tests (`set_and_query_widget_cells`, `widgets_round_trip_with_every_kind`, etc.) continue to pass with the type now `Widgets<(u32, u32)>`.

### T-201b (App field plumbing)
- Compile-time only; covered by `cargo build` + `cargo clippy`.

### T-201c (Hex render dispatch)
- Manual / E2E — render-path testing without a winit event loop is impractical. The render code itself is checked by `cargo build`.

### T-201d (Snapshot back-compat)
- `snapshot_roundtrips_through_ui_state` (existing): update fixture to populate `square_widgets` + `hex_widgets`; assert round-trip preserves both.
- `v145_snapshot_loads_square_widgets_via_alias`: hand-build a JSON snapshot with the old `widgets` key (the v145 schema), `serde_json::from_value` to `UiSnapshot`, assert `square_widgets` is populated from the alias.

### T-201e (Demo seed)
- Manual / E2E — verifies the hex sheet renders the dice button on launch.

## Integration Tests

### Component A + C (Widgets generic + snapshot)
- The unit tests above cover the load-bearing surface. No additional integration harness needed.

### Component B (hex render)
- Covered by E2E.

### Component D (Hex Game demo)
- E2E only.

## End-to-End Tests

- **Status:** possible (manual).
- `e2e_hex_dice_button_rolls`: launch the app (native or wasm), switch to Hex Game sheet, click the dice button at H(2,2), verify H(2,2) value changes to a new integer in [1, 6]. Click again — verify it rolls a different value at least once over 5 clicks.
- `e2e_hex_widget_survives_autosave`: launch wasm app, click dice, wait 3 s, F5; verify the rolled value rehydrates (widget metadata survives via hex_widgets in UiSnapshot).
- `e2e_v145_save_loads_square_widgets`: open a v145-era `.tscl` file (or carry one from CI); verify the file's `widgets` field rehydrates as `square_widgets` (via `#[serde(alias)]`).
