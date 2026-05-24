Finalized - DO NOT EDIT

# Sprint 0 Build Plan

## Schema Tree

- **Sprint Goal:** `.crbd` persistence end-to-end in the egui/wasm UI (Ctrl+S save, Ctrl+O open, localStorage autosave; UiState round-trips).
  - **Component A — Store layer extension** (UiState payload in the zip; format-version bump v0→v1; backwards-compatible read of v0).
    - T-001: Add `UiState` opaque-JSON type to `carbide-store`.
    - T-002: Bump `FORMAT_VERSION` to 1; extend `save`/`load` signatures to carry `&UiState`/return `(Workbook, UiState)`; accept v0 as "no UiState".
    - T-003: Round-trip test in `carbide-store` for workbook + non-empty UiState; version-tolerance test (v0 read yields empty UiState).
  - **Component B — Engine byte API**
    - T-004: Add `WorkbookEngine::save_bytes(&UiState) -> Result<Vec<u8>>` and `open_bytes(&[u8]) -> Result<UiState>`; existing `save(path)/open(path)` delegate to these.
  - **Component C — UI capture/restore**
    - T-005: Derive `Serialize`/`Deserialize` on `WidgetKind`, `Widgets`, `CellFormat`, `Borders`, `HexBorders`, `Rule`, `Condition`, `NoteMap`, with a `Color32`↔`[u8;4]` adapter.
    - T-006: Create `apps/carbide-ui/src/state_io.rs` with `UiSnapshot` struct (the typed mirror of the store's opaque UiState) and `capture(&app) -> UiSnapshot` / `restore(&mut app, UiSnapshot)` pure functions. Map `UiSnapshot` ↔ `carbide_store::UiState` via `serde_json`.
  - **Component D — Save/Open keymap and dialog flow**
    - T-007: Add `carbide-store` dep + `rfd` dep + `base64` dep + `wasm-bindgen-futures` (already present) to `apps/carbide-ui/Cargo.toml`. Pin versions, configure features so wasm32 compiles.
    - T-008: Extend `keymap.rs` with `Command::Save`, `Command::SaveAs`, `Command::Open`; bind Ctrl+S, Ctrl+Shift+S, Ctrl+O; register them in `NAV_KEYS`.
    - T-009: Implement async save flow in `app.rs`: on Save, call `engine.save_bytes(&capture())`, hand bytes to `rfd::AsyncFileDialog::new().set_file_name("carbide.crbd").save_file().await?.write(bytes).await`. Use `wasm-bindgen-futures::spawn_local` on wasm and a `pollster::block_on` (or sync `rfd::FileDialog`) path on native.
    - T-010: Implement async open flow: on Open, `rfd::AsyncFileDialog::new().add_filter("Carbide", &["tscll", "tscl"]).pick_file().await?.read().await`, then `engine.open_bytes` + `state_io::restore`. Stash bytes in a `Arc<Mutex<Option<Vec<u8>>>>` and drain it at the start of each frame (egui can't await mid-update).
    - T-011: Add Ribbon "Save" and "Open" buttons that emit the same `RibbonAction`s and route through the same handlers.
  - **Component E — localStorage autosave + rehydrate**
    - T-012: `state_io::autosave_to_local_storage(bytes)` — base64 encode, `web_sys::window().local_storage()`, key `carbide.autosave.v1`. Skip silently if `>4 MiB` (toast hook reserved for later).
    - T-013: `state_io::load_from_local_storage() -> Option<Vec<u8>>` for boot path; in `CarbideApp::new`, prefer the autosave over the seed demos when present.
    - T-014: Wire dirty-tracking + debounce: `app.dirty: bool` flips on every user-initiated mutation; `app.last_autosave: f64` (egui frame time); every 2s of dirty-time, autosave and clear the flag. Don't autosave the unmodified seed.
  - **Component F — Verification + ship**
    - T-015: `cargo fmt --all`, `cargo clippy --all-targets -- -D warnings` on both workspaces (root + apps/carbide-ui), `cargo test -p carbide-store -p carbide-formula`, `cargo build --target wasm32-unknown-unknown --manifest-path apps/carbide-ui/Cargo.toml`. All four green.
    - T-016: Open PR `webui-v144-tscl-persistence` with the four-check expectation called out.

## Execution Sequence

### T-001: Add `UiState` opaque-JSON type to `carbide-store`.
- **Touches:** `crates/carbide-store/src/lib.rs`
- **Depends on:** (none)
- **Success criterion:** `pub type UiState = serde_json::Value;` (or a thin newtype with `Default`/`Serialize`/`Deserialize`) compiles. `UiState::default()` yields `Value::Object(Map::new())`.
- **Notes:** A `serde_json::Value` is sufficient — the store is intentionally opaque to the UI's schema. A newtype is nicer for documentation but adds boilerplate without changing behavior. Pick the newtype to leave room for v2 to attach metadata.

### T-002: Bump `FORMAT_VERSION` to 1; extend `save`/`load` to carry `UiState`.
- **Touches:** `crates/carbide-store/src/lib.rs`
- **Depends on:** T-001
- **Success criterion:** `FORMAT_VERSION = 1`. `save(workbook, &ui_state, writer)` and `load(reader) -> (Workbook, UiState)`. `save_to_bytes`/`load_from_bytes` updated accordingly. When loading v0, `UiState::default()` is returned without error.
- **Notes:** Write `ui.json` as a third entry in the zip (after `manifest.json` and `workbook.json`). Read path: if `by_name("ui.json")` errs with `FileNotFound`, treat as default. Manifest carries the version; v0 files are still readable.

### T-003: Round-trip + version-tolerance tests in `carbide-store`.
- **Touches:** `crates/carbide-store/src/lib.rs` (tests module)
- **Depends on:** T-002
- **Success criterion:** Two new `#[test]` functions: `round_trip_preserves_ui_state` and `reads_v0_as_empty_ui_state`. Both pass under `cargo test -p carbide-store`.
- **Notes:** Build a fake UiState `Value::Object` with at least one nested map and one array; assert equality after round-trip. For v0 tolerance, construct a v0 zip by hand (the existing `refuses_unknown_format_version` test already shows the pattern) — only the `format_version` field flips between v0 and the v1 read.

### T-004: Engine byte API.
- **Touches:** `crates/carbide-formula/src/engine.rs`
- **Depends on:** T-002
- **Success criterion:** `WorkbookEngine::save_bytes(&UiState) -> Result<Vec<u8>, SetCellError>` and `open_bytes(&[u8]) -> Result<UiState, SetCellError>` exist; existing `save(path)/open(path)` shell to them with `std::fs::read`/`std::fs::write`.
- **Notes:** `SetCellError::Io` already covers store errors; reuse it.

### T-005: Derive serde on UI-state-bearing types.
- **Touches:** `apps/carbide-ui/src/widget.rs`, `apps/carbide-ui/src/format.rs`, `apps/carbide-ui/src/conditional.rs`, `apps/carbide-ui/src/note.rs`
- **Depends on:** (none — parallel-safe with T-001..T-004)
- **Success criterion:** `WidgetKind`, `Widgets`, `CellFormat`, `Borders`, `HexBorders`, `Rule`, `Condition`, and `NoteMap` all `#[derive(Serialize, Deserialize)]`. `Color32` fields use a `mod color_rgba { fn serialize / deserialize ... }` adapter that round-trips as `[u8;4]`.
- **Notes:** Add `serde = { version = "1", features = ["derive"] }` and `serde_json = "1"` to `apps/carbide-ui/Cargo.toml`. Apply `#[serde(default)]` field-wise so missing fields tolerate forward drift.

### T-006: `state_io.rs` capture/restore.
- **Touches:** `apps/carbide-ui/src/state_io.rs` (new), `apps/carbide-ui/src/lib.rs` (module decl)
- **Depends on:** T-005
- **Success criterion:** `UiSnapshot { active_sheet, stage_mode, square_format/widgets/notes, hex_format/widgets/notes, triangle_format/widgets/notes, conditional_rules, is_fresh_seed: bool }` exists; `capture(&app) -> UiSnapshot` and `restore(&mut app, UiSnapshot)` round-trip every field. Snapshot ↔ `carbide_store::UiState` is a one-line `serde_json::to_value`/`from_value` each direction.
- **Notes:** Don't move data; clone. The functions are called rarely (on save / open / autosave-tick) so clone cost is irrelevant. Avoid touching `History` (intentionally not persisted).

### T-007: Dependencies in `apps/carbide-ui/Cargo.toml`.
- **Touches:** `apps/carbide-ui/Cargo.toml`
- **Depends on:** (none)
- **Success criterion:** `cargo build --manifest-path apps/carbide-ui/Cargo.toml` and `cargo build --target wasm32-unknown-unknown --manifest-path apps/carbide-ui/Cargo.toml` succeed.
- **Notes:** Pins: `carbide-store = { path = "../../crates/carbide-store" }`, `rfd = { version = "0.14", default-features = false }` (let the default backend ship; wasm picks the right one automatically), `base64 = "0.22"`, `serde = "1"` with derive, `serde_json = "1"`. Verify `cargo tree` on wasm32 has no `libz-sys`/`bzip2-sys`.

### T-008: Keymap commands + bindings.
- **Touches:** `apps/carbide-ui/src/keymap.rs`, `apps/carbide-ui/src/app.rs` (NAV_KEYS list)
- **Depends on:** (none)
- **Success criterion:** `Command::{Save, SaveAs, Open}` exist; `SHORTCUTS` table maps Ctrl+S, Ctrl+Shift+S, Ctrl+O respectively. NAV_KEYS includes `(CTRL, S)`, `(CTRL_SHIFT, S)`, `(CTRL, O)`. Unit test `nav(Key::S, true, false) == Some(Command::Save)` (etc.).
- **Notes:** Keep formatting tight — past CI fmt incidents show local fmt can diverge; explicit single-line `assert_eq!` saved a force-push last sprint.

### T-009: Save flow.
- **Touches:** `apps/carbide-ui/src/app.rs`
- **Depends on:** T-004, T-006, T-007, T-008
- **Success criterion:** Triggering `Command::Save` (Ctrl+S or ribbon) calls `engine.save_bytes(&capture().into())`, then opens a save dialog via `rfd::AsyncFileDialog::new().set_file_name("workbook.crbd").save_file()`, then writes bytes via `FileHandle::write`. On wasm: `spawn_local` the future. On native: `pollster::block_on` is too heavyweight — use the sync `rfd::FileDialog` instead behind `#[cfg(not(target_arch = "wasm32"))]`.
- **Notes:** Treat failures (user cancelled the dialog → `None`; write error) as silent for now; logging hook can be added in a follow-up.

### T-010: Open flow.
- **Touches:** `apps/carbide-ui/src/app.rs`
- **Depends on:** T-009
- **Success criterion:** Triggering `Command::Open` opens a file picker filtered to `*.crbd`, reads the bytes, calls `engine.open_bytes`, then `state_io::restore`. Bytes arrive asynchronously on wasm — staged through `Arc<Mutex<Option<Vec<u8>>>>` (call it `pending_open`), drained at the top of `update()`.
- **Notes:** While the open is in flight, suppress autosave (`self.suppress_autosave_until = now + 2.0`) so the in-flight load doesn't get clobbered.

### T-011: Ribbon Save/Open buttons.
- **Touches:** `apps/carbide-ui/src/ribbon.rs`
- **Depends on:** T-008
- **Success criterion:** First ribbon group "File" gains "Save" and "Open" buttons; clicking each emits `RibbonAction::Save`/`RibbonAction::Open`; `app.rs` dispatches to the same handler as the keymap.
- **Notes:** Add the new group widths to `GROUP_WIDTHS`. Keep label-only buttons; icons aren't part of this sprint.

### T-012: localStorage autosave write.
- **Touches:** `apps/carbide-ui/src/state_io.rs`
- **Depends on:** T-006, T-007
- **Success criterion:** `pub fn autosave_to_local_storage(bytes: &[u8])` exists. On wasm, base64-encodes and sets `carbide.autosave.v1`; on native, no-op. Returns `()`; failures are swallowed (autosave is best-effort). Skip when `bytes.len() > 4 * 1024 * 1024`.
- **Notes:** Use `base64::engine::general_purpose::STANDARD`.

### T-013: localStorage rehydrate on boot.
- **Touches:** `apps/carbide-ui/src/state_io.rs`, `apps/carbide-ui/src/app.rs` (`CarbideApp::new`)
- **Depends on:** T-012
- **Success criterion:** `pub fn load_from_local_storage() -> Option<Vec<u8>>` exists (None on native or absent). On boot, if `Some(bytes)`, call `engine.open_bytes` + `restore`; otherwise run the seed demos.
- **Notes:** Honor `is_fresh_seed` from the snapshot: if true, *also* run the seed demos before applying the snapshot — never the case after T-014 lands, but keep the invariant explicit so future seed changes don't cause silent demo loss.

### T-014: Dirty-tracking + debounce.
- **Touches:** `apps/carbide-ui/src/app.rs`
- **Depends on:** T-012
- **Success criterion:** A new `dirty: bool` and `last_autosave: f64` field on the app; every code path that mutates engine state or UI state flips dirty; once per frame, if dirty and elapsed > 2s, autosave + clear dirty. Seed demos *do not* flip dirty (they run before construction completes).
- **Notes:** Centralize the dirty flip: add a `mark_dirty(&mut self)` helper and call it from each mutation site. Resist the temptation to thread it through every keymap branch; one helper, called liberally, is fine.

### T-015: Run full local CI gate.
- **Touches:** (none, just runs)
- **Depends on:** T-001..T-014
- **Success criterion:** All of `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --workspace`, plus the equivalents under `apps/carbide-ui` (`cargo fmt`, `clippy`, `test`, `build --target wasm32-unknown-unknown`) — all exit zero.
- **Notes:** Run them in this order so the cheaper checks fail-fast.

### T-016: PR and merge.
- **Touches:** (git only)
- **Depends on:** T-015
- **Success criterion:** Branch `webui-v144-tscl-persistence` pushed; PR opened with the launch-brief Priority 3 callout; CI all-green; squash-merged; main updated.
- **Notes:** Confirm conclusion via `gh run list --branch ... --json status,conclusion` before merging — `gh run watch` exit code is unreliable on Windows (existing project rule).
