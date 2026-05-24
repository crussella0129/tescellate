# Sprint 4 Unit Tests

## T-404 (round-trip)
- `widgets_generic_with_tri_coord_round_trip` (in `widget::tests`): a `Widgets<TriCoord>` with Button + Toggle round-trips through JSON. **pass**
- `snapshot_round_trips_through_ui_state` (existing, extended): fixture populates `triangle_widgets`; round-trip preserves it. **pass**

## T-401/T-402/T-403 (field + render + seed)
- Compile-time + manual / E2E. `cargo build` (native + wasm) clean.

## Run summary
- `cargo test --manifest-path apps/carbide-ui/Cargo.toml --lib`: **249 passed, 0 failed** (1 net-new + 248 carried).
- `cargo test --workspace`: all green.
