# Sprint 2 Unit Tests

## T-201a (Widgets generic)
- `widgets_generic_with_hex_coord_round_trip`: `Widgets<HexCoord>` with Toggle + Button entries, JSON round-trip preserves both. **pass**
- Existing square widget tests (`set_and_query_widget_cells`, `widgets_round_trip_with_every_kind`, etc.) all still pass under `Widgets<(u32, u32)>`. **pass**

## T-201d (Snapshot back-compat)
- `snapshot_round_trips_through_ui_state` (existing, updated): fixture populates `square_widgets` + `hex_widgets`; round-trip preserves both. **pass**
- `v145_snapshot_loads_square_widgets_via_alias`: hand-built v145 JSON (key `widgets`) loads cleanly into `UiSnapshot.square_widgets` via `#[serde(alias)]`. **pass**

## T-201b / T-201c / T-201e (App field plumbing, hex render, demo seed)
- Compile-time + manual visual. Covered by `cargo build` + the wasm build + manual E2E.

## Run summary
- `cargo test --manifest-path apps/carbide-ui/Cargo.toml --lib`: **248 passed, 0 failed** (2 net-new this sprint).
- `cargo test --workspace`: all green.
