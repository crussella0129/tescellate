# Sprint 0 Unit Tests

Each test from the plan's §Unit Tests was implemented inline with its
owning task; the consolidated state matches the test-plan call-outs.

## T-001 — `UiState` opaque type
- `ui_state_default_is_empty_object` — pass

## T-003 — Store round-trip + version tolerance
- `round_trip_preserves_ui_state` — pass
- `reads_v0_as_empty_ui_state` — pass
- `refuses_unknown_format_version` (existing, kept green after FORMAT_VERSION bump) — pass

## T-004 — Engine byte API
- `engine_save_bytes_then_open_bytes_roundtrips` — pass
- `engine_path_api_uses_byte_api` — pass
- `save_and_open_preserves_dependencies` (existing, still green via byte-API delegation) — pass
- `hex_save_and_open_round_trip` (existing) — pass

## T-005 — Serde derives + Color32 adapter
- `cell_format_round_trips_through_json` — pass
- `widgets_round_trip_with_every_kind` — pass
- `color32_serializes_as_rgba_array` — pass

## T-006 — `state_io` capture/restore
- `snapshot_round_trips_through_ui_state` — pass
- `empty_ui_state_yields_default_snapshot` — pass

## Deferred unit tests (sprint 1)
The following were specified in the plan but moved to sprint 1 along with
their owning tasks:

- T-008 keymap commands (Save / SaveAs / Open) — gated on the dialog
  wiring being present.
- T-011 ribbon Save/Open emit tests.
- T-012/T-013 localStorage / autosave / boot-rehydrate tests.
- T-014 dirty-flag + 2s debounce tests.

## Run summary
- `cargo test --workspace`: ~250 test cases across all crates, all passing.
- `cargo test --manifest-path apps/tescellate-ui/Cargo.toml --lib`: 243 passed, 0 failed.
