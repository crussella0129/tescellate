Finalized - DO NOT EDIT

# Sprint 1 Test Plan

## Unit Tests

### T-101a (deps)
- `wasm_build_compiles_with_new_deps` — covered by T-103 build step.

### T-101b (keymap)
- `keymap_save_on_ctrl_s`: `command_for_key(Key::S, false, true, Mode::Navigating) == Some(Command::Save)`.
- `keymap_save_as_on_ctrl_shift_s`: `command_for_key(Key::S, true, true, Mode::Navigating) == Some(Command::SaveAs)`.
- `keymap_open_on_ctrl_o`: `command_for_key(Key::O, false, true, Mode::Navigating) == Some(Command::Open)`.
- `keymap_o_alone_does_nothing`: `command_for_key(Key::O, false, false, Mode::Navigating) == None`.

### T-101c (ribbon)
- Manual / visual — the ribbon's headless test surface didn't carry through sprint 0; no unit test added unless `ribbon.rs` already exercises a click harness.

### T-101d / T-101e / T-101f (Save / Open flows)
- Manual — dialog interactions are not unit-tested. Covered by §E2E.

### T-101g (allow attribute removal)
- `cargo clippy --all-targets -- -D warnings` covers it — if `capture_state`/`restore_state` slip back to unused, clippy errors.

### T-102a (autosave write)
- Native target: `autosave_to_local_storage_is_noop_on_native`: calling on native returns without panicking and has no observable side effect.
- `autosave_skips_when_over_cap`: passing > 4 MiB returns without panicking; no localStorage write on wasm.
- Wasm-side write is covered by §E2E.

### T-102b (rehydrate)
- `load_from_local_storage_returns_none_on_native`: native target returns `None`.
- Wasm-side load is covered by §E2E.

### T-102c (mark_dirty)
- `mark_dirty_is_idempotent`: calling twice keeps `dirty = true` (not panicking; no double-toggle).
- Spot-check via grep that each mutation site calls `mark_dirty` — covered by code review at commit time rather than a unit test.

### T-102d (autosave tick)
- `maybe_autosave_skips_before_threshold`: `dirty = true`, `last_autosave = now - 1.0`; `maybe_autosave(now)` doesn't fire.
- `maybe_autosave_fires_after_threshold`: `dirty = true`, `last_autosave = now - 3.0`; `maybe_autosave(now)` fires (we detect by checking `dirty` was cleared and `last_autosave` advanced).
- `maybe_autosave_respects_suppress`: `dirty = true`, `last_autosave = now - 10.0`, `suppress_autosave_until = now + 1.0`; doesn't fire.

## Integration Tests

### Component B (Save/Open flow)
- `save_then_open_round_trips_state_via_bytes`: build a stub TescellateApp or equivalent fixture, mutate state (set a widget, toggle stage mode), call the bytes-producing inner of Save, pass those bytes into the bytes-consuming inner of Open on a fresh app, assert state matches. (This is the same test the sprint 0 plan called out as "deferred" — sprint 1 ships it if a winit-free fixture works; otherwise the byte-API + state_io round-trips from v144 stand in.)

### Component C (autosave + rehydrate)
- Native-side stub via the no-op autosave path: verify the dirty-flag/debounce logic in isolation (covered by T-102d unit tests).
- Wasm-side: covered by §E2E.

## End-to-End Tests

- **Status:** possible (manual)
- `e2e_browser_save_open_roundtrip`: build wasm, serve, open app in browser, change a slider, Ctrl+S → download a `.tscl`, refresh, Ctrl+O → pick downloaded file, verify slider state restored.
- `e2e_browser_autosave_survives_refresh`: edit a cell, wait > 2s, F5, verify cell value restored from localStorage.
- `e2e_native_save_open_roundtrip`: `cargo run --manifest-path apps/tescellate-ui/Cargo.toml`, Ctrl+S → native dialog → write to disk; reopen; Ctrl+O → native dialog → restore.

Manual E2E is acceptable for this sprint; the unit + integration tests
prove the bytes layer round-trips and the dialog is mostly a thin
shell.
