# Completed Tasks Log (Append-Only)

## T-001 (sprint 0)
- **Description:** Add `UiState` opaque-JSON type to `tescellate-store`.
- **Completed:** 2026-05-20T17:40:00Z
- **Files modified:** crates/tescellate-store/src/lib.rs
- **Commit:** `4b0aa3d`

## T-002 (sprint 0)
- **Description:** Bump `FORMAT_VERSION` to 1; introduce `save_full` / `load_full` carrying `UiState`; tolerate v0 reads (no `ui.json` → `UiState::default()`).
- **Completed:** 2026-05-20T17:45:00Z
- **Files modified:** crates/tescellate-store/src/lib.rs
- **Commit:** `715c4ec`

## T-003 (sprint 0)
- **Description:** Round-trip + v0-tolerance + UiState default tests in `tescellate-store`.
- **Completed:** 2026-05-20T17:50:00Z
- **Files modified:** crates/tescellate-store/src/lib.rs
- **Commit:** `04c2d90`

## T-004 (sprint 0)
- **Description:** `WorkbookEngine::save_bytes` / `open_bytes` byte API; path-API now delegates; serde_json moved to dev-deps for engine tests.
- **Completed:** 2026-05-20T18:00:00Z
- **Files modified:** crates/tescellate-formula/src/engine.rs, crates/tescellate-formula/Cargo.toml
- **Commit:** `e17f6de`

## T-005 (sprint 0)
- **Description:** Serde derives on UI types (WidgetKind/Widgets, CellFormat/Borders/HexBorders/HAlign/VAlign/FontSize/NumberFormat, FormatMap, Condition/Rule, NoteMap). Color32 round-trips as `[r,g,b,a]` via `to_srgba_unmultiplied`. Widgets serializes as a Vec of (cell, kind) pairs (JSON object keys must be strings).
- **Completed:** 2026-05-20T18:25:00Z
- **Files modified:** apps/tescellate-ui/Cargo.toml, apps/tescellate-ui/src/format.rs, apps/tescellate-ui/src/widget.rs, apps/tescellate-ui/src/conditional.rs, apps/tescellate-ui/src/note.rs
- **Commit:** `712f626`

## T-006 (sprint 0)
- **Description:** `state_io.rs` with `UiSnapshot` + `ActiveSheetTag` + JSON adapters. Capture/restore methods on `TescellateApp` (gated `#[allow(dead_code)]` until the dialog wiring lands). Adds Vec-of-pair adapters to FormatMap so non-string-key HashMaps round-trip through JSON; new public `iter()`/`replace_with()` helpers on FormatMap/Widgets/NoteMap/GridMetrics for state-IO callers.
- **Completed:** 2026-05-20T18:50:00Z
- **Files modified:** apps/tescellate-ui/src/state_io.rs, apps/tescellate-ui/src/lib.rs, apps/tescellate-ui/src/app.rs, apps/tescellate-ui/src/format.rs, apps/tescellate-ui/src/widget.rs, apps/tescellate-ui/src/note.rs, apps/tescellate-ui/src/grid.rs, apps/tescellate-ui/Cargo.toml
- **Commit:** `b0d4cfe`

## T-015 (sprint 0)
- **Description:** Local CI gate run — `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -D warnings`, `cargo test --workspace`, plus the same on `apps/tescellate-ui`, plus `cargo build --target wasm32-unknown-unknown` on the UI crate. All green. Two fmt drifts fixed (engine.rs, widget.rs, state_io.rs); one clippy `bool_assert_comparison` fixed.
- **Completed:** 2026-05-20T18:55:00Z
- **Files modified:** crates/tescellate-formula/src/engine.rs, apps/tescellate-ui/src/widget.rs, apps/tescellate-ui/src/state_io.rs
- **Commit:** `5e8b46c`

## T-016 (sprint 0)
- **Description:** Pushed `webui-v144-tscl-persistence`, opened PR #175, CI fully green (7 checks: rustfmt+clippy, ubuntu build+test, windows build+test, renderer, native compile, python engine, wasm front-end), squash-merged to main as commit `b7ffff1`.
- **Completed:** 2026-05-20T19:00:00Z
- **Files modified:** (git only)
- **Commit:** `d9bca5c`

## T-101a (sprint 1)
- **Description:** Add `rfd 0.14` (default features on native; `gtk3` feature gated to wasm32 to satisfy rfd's build script with no system-lib link), `base64 0.22`, `wasm-bindgen 0.2`, `js-sys 0.3`, `web-sys 0.3` (Window, Storage, Document, HtmlAnchorElement, Blob, BlobPropertyBag, Url features) to `apps/tescellate-ui/Cargo.toml`. Both native and wasm32 builds succeed.
- **Completed:** 2026-05-20T19:15:00Z
- **Files modified:** apps/tescellate-ui/Cargo.toml
- **Commit:** `291a5bb`

## T-101b (sprint 1)
- **Description:** `Command::{Save,SaveAs,Open}` variants + Ctrl+S/Ctrl+Shift+S/Ctrl+O bindings in `keymap::navigating`; NAV_KEYS list extended; SHORTCUTS table gains three rows. Match arms added to `apply_command` dispatching to stub `handle_save`/`handle_open` (real bodies land in T-101e/T-101f). Unit test `save_open_keymap_bindings` passes.
- **Completed:** 2026-05-20T19:25:00Z
- **Files modified:** apps/tescellate-ui/src/keymap.rs, apps/tescellate-ui/src/app.rs
- **Commit:** `07aa911`

## T-101c (sprint 1)
- **Description:** Ribbon File group added at index 0 (leftmost). Save + Open buttons emit `RibbonAction::{Save,Open}`. GROUP_WIDTHS gains the File row (108px), all subsequent group indices shifted by 1 in both the inline and overflow-menu render paths. App-side `apply_ribbon_action` routes the new actions to the stub handlers.
- **Completed:** 2026-05-20T19:30:00Z
- **Files modified:** apps/tescellate-ui/src/ribbon.rs, apps/tescellate-ui/src/app.rs
- **Commit:** `2f56f82`

## T-101d/e/f/g (sprint 1)
- **Description:** Full Save / Open dialog wiring. `pending_open_bytes: Arc<Mutex<Option<Vec<u8>>>>` on `TescellateApp`; `drain_pending_open` runs at the top of `update()` and routes through `engine.open_bytes` + `restore_state`. Save handler builds bytes via `engine.save_bytes(&snapshot_to_ui_state(&capture_state()))`; native uses sync `rfd::FileDialog::save_file()` + `std::fs::write`, wasm uses `rfd::AsyncFileDialog` + `FileHandle::write` under `spawn_local`. Open mirrors it (native sync read; wasm async into the bytes slot). `#[allow(dead_code)]` dropped from `capture_state`/`restore_state` — both methods are now live.
- **Completed:** 2026-05-20T19:50:00Z
- **Files modified:** apps/tescellate-ui/src/app.rs
- **Commit:** `371fe3f`

## T-102a/b/c/d (sprint 1)
- **Description:** localStorage autosave end-to-end. `state_io::{autosave_to_local_storage, load_from_local_storage}` (wasm-gated via base64 + web-sys; native no-op). Boot rehydrate in `TescellateApp::new` — if a saved autosave is present, swap the seed-demo workbook for the saved one and replay UI snapshot. Three new app fields: `dirty: bool`, `last_autosave: f64`, `suppress_autosave_until: f64`. `mark_dirty()` called centrally at command + ribbon-action dispatch (broad coverage with a tiny over-fire cost balanced by debounce). `maybe_autosave(now)` runs at end of each frame; persists when `dirty && now - last_autosave > 2.0`. Save and Open both clear `dirty` + advance `last_autosave` so they don't race the debounce; Open also bumps `suppress_autosave_until` by 2 s. Engine workbook swap re-binds UI sheet IDs by lattice via new `rebind_sheet_ids` helper, and clears `History` to keep undo from reaching into the prior workbook (added `History::clear`).
- **Completed:** 2026-05-20T20:15:00Z
- **Files modified:** apps/tescellate-ui/src/state_io.rs, apps/tescellate-ui/src/app.rs, apps/tescellate-ui/src/history.rs
- **Commit:** `2803222`

## T-103 (sprint 1)
- **Description:** Local CI gate, all green: `cargo fmt --all --check` (workspace + UI), `cargo clippy --all-targets -- -D warnings` (workspace + UI), `cargo test --workspace` (every crate's tests pass; UI lib at 246 cases), `cargo build --target wasm32-unknown-unknown --release` (production wasm bundle builds clean).
- **Completed:** 2026-05-20T20:25:00Z
- **Files modified:** (verification only)
- **Commit:** `8ce323c`
