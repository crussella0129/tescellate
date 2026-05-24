Finalized - DO NOT EDIT

# Sprint 0 Test Plan

## Unit Tests

### T-001 unit tests (`UiState` opaque type)
- `ui_state_default_is_empty_object`: `UiState::default()` ↔ `serde_json::json!({})`. No mocks.

### T-002 unit tests (`FORMAT_VERSION` bump + tri-entry zip)
- (covered by T-003 round-trip tests)

### T-003 unit tests (`carbide-store` round trip)
- `round_trip_preserves_ui_state`: `save_to_bytes(&wb, &json!({"answer": 42, "list": [1,2,3]})) → load_from_bytes` returns the same `(Workbook, UiState)`. Stub: existing `sample()` workbook fixture.
- `reads_v0_as_empty_ui_state`: hand-build a v0 zip (manifest.json with `format_version: 0`, workbook.json, no ui.json), load returns `(workbook, UiState::default())`. Mocks: zip writer (already used in `refuses_unknown_format_version`).
- `refuses_unknown_format_version_still_works`: keep the existing test green after the bump.

### T-004 unit tests (`WorkbookEngine` byte API)
- `engine_save_bytes_then_open_bytes_roundtrips`: build a small engine state, `save_bytes(&UiState::default())`, fresh engine, `open_bytes(bytes)`, assert cell values and sources match. No mocks.
- `engine_path_api_uses_byte_api`: stub a tempfile, call `save(path)`, read bytes from disk, compare to `save_bytes` output. (Trust-but-verify the delegation.)

### T-005 unit tests (serde derives + Color32 adapter)
- `cell_format_round_trips_through_json`: a `CellFormat` with every field set non-default, `serde_json::to_value` then `from_value`, assert equal. No mocks.
- `widgets_round_trip_with_every_kind`: a `Widgets` map containing one of each `WidgetKind` variant, round-trip via JSON. No mocks.
- `color32_serializes_as_rgba_array`: `Color32::from_rgba_unmultiplied(1,2,3,4)` → `[1,2,3,4]` and back. Pure adapter test.

### T-006 unit tests (`state_io::capture`/`restore`)
- `snapshot_roundtrips_an_empty_app`: `restore(&mut app, capture(&app))` is a no-op (deep equality on every persisted field).
- `snapshot_roundtrips_a_populated_app`: build an app fixture with one widget, one note, one conditional rule, one format override, stage_mode=true; `capture` → `serde_json` → `from_value` → `restore` to a fresh app; deep equality holds.

### T-007 unit tests (deps)
- `wasm_build_succeeds`: covered by T-015 build step (no unit test needed at the file level).

### T-008 unit tests (keymap)
- `keymap_save_on_ctrl_s`: `nav(Key::S, true, false) == Some(Command::Save)`.
- `keymap_save_as_on_ctrl_shift_s`: `nav(Key::S, true, true) == Some(Command::SaveAs)`.
- `keymap_open_on_ctrl_o`: `nav(Key::O, true, false) == Some(Command::Open)`.

### T-009 unit tests (save flow)
- Manual / integration only: dialog interactions are inherently mockable but the cost-benefit of stubbing `rfd::AsyncFileDialog` is poor for this sprint. Covered by E2E manual run in §E2E.

### T-010 unit tests (open flow)
- Same — covered by E2E.

### T-011 unit tests (ribbon)
- `ribbon_emits_save_action`: render the file group, find the Save button by label, simulate a click in egui-headless, assert `RibbonAction::Save` was returned. (egui's `Context::run` headless rendering is already used in the existing ribbon tests if any; if not, add a thin one.)
- `ribbon_emits_open_action`: same pattern with Open.

### T-012 unit tests (autosave write)
- `autosave_skips_when_payload_over_cap`: pass 5 MiB of zeros; ensure no panic; localStorage key is not set (read via web-sys; gated `#[cfg(target_arch = "wasm32")]` with a no-op stub on native).
- Wasm-only — add `#[wasm_bindgen_test]` and ensure the apps/carbide-ui Cargo.toml has `wasm-bindgen-test` as a dev-dep behind `cfg(target_arch = "wasm32")`. If wiring `wasm-bindgen-test` is more weight than this sprint warrants, *skip* the wasm-side tests and rely on the integration / E2E checks — note the deferral.

### T-013 unit tests (rehydrate)
- `boot_prefers_local_storage_over_seed_demos`: native-only test where `load_from_local_storage` is feature-gated to a fake that returns `Some(bytes)`; assert no Budget/Hex-Game seed cells exist after `new()`.
- `boot_falls_back_to_seed_when_no_autosave`: same shape, fake returns `None`; assert seed cells present.

### T-014 unit tests (dirty/debounce)
- `mark_dirty_is_idempotent`: calling twice doesn't change anything beyond the bool flip.
- `seed_demos_do_not_flip_dirty`: after `new()`, `app.dirty == false`. (Smoke test for the autosave invariant.)
- `autosave_debounce_threshold_is_2s`: set `last_autosave = now - 1.0`; `dirty = true`; `maybe_autosave(now)` is a no-op. Set `last_autosave = now - 3.0`; `maybe_autosave(now)` calls the autosave hook and clears dirty.

## Integration Tests

### Component A integration (store layer)
- `store_round_trips_full_ui_state_shape`: build a workbook with widgets-in-Workbook (none — UiState is the sibling) and a UiSnapshot with non-trivial content, save to bytes, load back, assert byte-for-byte deserialization of the snapshot matches. (Tightens T-003 with the actual schema rather than ad-hoc JSON.)

### Component B + C integration (engine + UI state)
- `app_save_load_cycle_preserves_full_state`: in-memory test that constructs `CarbideApp`, applies a sequence of user-style mutations (set a formula, add a widget, toggle stage mode, add a conditional rule), calls `engine.save_bytes(&capture().into())`, builds a fresh app, calls `engine.open_bytes` + `restore`, asserts identical state. Mocks: none; pure in-memory.

### Component D integration (save/open flow without dialog)
- The dialog itself is mocked behind a trait `FileDialog` (one method `save_bytes(&[u8], &str)`, one method `open_bytes() -> Option<Vec<u8>>`). The save and open flows call this trait; production uses an rfd-backed impl, tests use an in-memory impl. The in-memory impl tracks the last bytes written and feeds back arbitrary bytes for open.
- `save_routes_engine_bytes_through_dialog`: trigger Save command, assert the mock dialog received the same bytes as `engine.save_bytes`.
- `open_routes_dialog_bytes_through_engine`: feed the mock dialog the bytes from a prior save, trigger Open, assert the app state matches the saved state.

### Component E integration (localStorage rehydrate)
- Native-side via fake (the wasm-side via `wasm_bindgen_test` is deferred per T-012's note): `autosave_then_rehydrate_full_state` mirrors Component D's shape but goes through the localStorage path instead of the dialog path.

## End-to-End Tests

- **Status:** possible (manual)
- `e2e_browser_save_open_roundtrip`: Run `trunk serve` (or the project's equivalent wasm dev server); open the app in Chrome; click a Budget slider; press Ctrl+S; verify a `.tscl` download appears in the browser; refresh the page; press Ctrl+O; pick the downloaded file; verify the slider is at the position the user left it.
- `e2e_browser_autosave_survives_refresh`: edit a cell, wait 3 seconds, F5 the page, verify the cell value is restored.
- `e2e_native_save_open_roundtrip`: `cargo run --manifest-path apps/carbide-ui/Cargo.toml`; same sequence using the OS file dialogs; verify the on-disk `.tscl` opens cleanly.

Manual E2E is acceptable for this sprint; a Playwright/wdio harness for the wasm build is the right next-sprint target if we want CI-gated E2E.
