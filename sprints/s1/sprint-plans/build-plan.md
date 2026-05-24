Finalized - DO NOT EDIT

# Sprint 1 Build Plan

## Schema Tree

- **Sprint Goal:** v145 — wire `.tscl` Save/Open dialogs + localStorage autosave on top of v144's persistence infrastructure.
  - **Component A — Dialog dependencies + keymap**
    - T-101a: Add `rfd`, `base64`, `web-sys` deps to `apps/carbide-ui/Cargo.toml`.
    - T-101b: Add `Command::{Save, SaveAs, Open}` + Ctrl+S/Ctrl+Shift+S/Ctrl+O bindings + NAV_KEYS entries + SHORTCUTS rows.
    - T-101c: Add `RibbonAction::{Save, Open}` + File group as the leftmost ribbon group.
  - **Component B — Save / Open flow**
    - T-101d: `pending_open_bytes: Arc<Mutex<Option<Vec<u8>>>>` field on `CarbideApp`; drain at top of `update()`; route through `open_bytes` + `restore_state`.
    - T-101e: Save handler — `capture_state` → `engine.save_bytes` → `rfd::AsyncFileDialog::save_file().write(bytes)` (wasm via `spawn_local`, native via the sync `FileDialog` path).
    - T-101f: Open handler — `rfd::AsyncFileDialog::pick_file().read()` → push bytes into `pending_open_bytes` slot.
    - T-101g: Remove `#[allow(dead_code)]` from `capture_state` / `restore_state`.
  - **Component C — localStorage autosave**
    - T-102a: `state_io::autosave_to_local_storage(bytes)` (wasm32-gated, base64-encoded, 4 MiB cap, swallow failure).
    - T-102b: `state_io::load_from_local_storage() -> Option<Vec<u8>>`; rehydrate-on-boot in `CarbideApp::new` (skip seed demos when present).
    - T-102c: `dirty: bool` + `last_autosave: f64` + `mark_dirty(&mut self)` helper; centralized call sites at each mutation entry point.
    - T-102d: Per-frame `maybe_autosave(now)` — fires when `dirty && now - last_autosave > 2.0`.
  - **Component D — Verification + ship**
    - T-103: Full local CI gate (fmt, clippy, test, wasm build) on both workspaces.
    - T-104: Open + merge PR `webui-v145-save-open-autosave`.

## Execution Sequence

### T-101a: Dialog deps in `apps/carbide-ui/Cargo.toml`.
- **Touches:** `apps/carbide-ui/Cargo.toml`
- **Depends on:** (none)
- **Success criterion:** Adds `rfd = { version = "0.14", default-features = false }`, `base64 = "0.22"`, and a `[target.'cfg(target_arch = "wasm32")'.dependencies]` block for `web-sys` with features `["Window","Storage","HtmlAnchorElement","Blob","Url","Document"]` + `js-sys`. `cargo build --manifest-path apps/carbide-ui/Cargo.toml` and `cargo build --target wasm32-unknown-unknown --manifest-path apps/carbide-ui/Cargo.toml` both compile.
- **Notes:** Cap web-sys features tightly; `cargo tree -e features` should not show a transitive feature explosion.

### T-101b: Keymap commands + bindings.
- **Touches:** `apps/carbide-ui/src/keymap.rs`, `apps/carbide-ui/src/app.rs` (NAV_KEYS list)
- **Depends on:** T-101a
- **Success criterion:** `Command::{Save, SaveAs, Open}` exist; `command_for_key(Key::S, false, true, Mode::Navigating) == Some(Command::Save)` etc.; NAV_KEYS includes `(CTRL, S)`, `(CTRL_SHIFT, S)`, `(CTRL, O)`. SHORTCUTS gains three rows.
- **Notes:** Format `assert_eq!` calls on single lines to dodge the past sprint's CI rustfmt drift.

### T-101c: Ribbon File group.
- **Touches:** `apps/carbide-ui/src/ribbon.rs`
- **Depends on:** T-101b
- **Success criterion:** `RibbonAction::{Save, Open}` exist; the File group is the leftmost ribbon group with Save and Open buttons that emit those actions. `GROUP_WIDTHS` updated to add the File group width.
- **Notes:** Keep label-only buttons. Width can be conservative — sprint 0 trimmed several other groups, leaving room.

### T-101d: Async-open plumbing.
- **Touches:** `apps/carbide-ui/src/app.rs`
- **Depends on:** T-101a
- **Success criterion:** `CarbideApp` gains `pending_open_bytes: Arc<Mutex<Option<Vec<u8>>>>`. At the very top of `update()`, drain the slot: if `Some(bytes)`, call `engine.open_bytes(&bytes)` + `restore_state(ui_state_to_snapshot(&ui))`. Errors are logged via `tracing::warn!` or `eprintln!`, never panic.
- **Notes:** Use `Arc::new(Mutex::new(None))`. `std::sync::Mutex` is fine — egui is single-threaded but the wasm `spawn_local` future writes from a different scheduler; the Send/Sync of Arc<Mutex<_>> isn't required because both ends are on the same JS event-loop thread — but using a Mutex keeps the type Send for the foreseeable native path.

### T-101e: Save handler.
- **Touches:** `apps/carbide-ui/src/app.rs`
- **Depends on:** T-101b, T-101c, T-101d
- **Success criterion:** Dispatching `Command::Save` (from keymap or ribbon): builds bytes via `engine.save_bytes(&snapshot_to_ui_state(&self.capture_state()))`, opens a save dialog via `rfd::AsyncFileDialog::new().set_file_name("workbook.tscl").save_file()`, writes via `FileHandle::write`. wasm uses `wasm_bindgen_futures::spawn_local`; native uses an inline `pollster::block_on`-free path — `rfd::FileDialog` (sync) instead, behind `#[cfg(not(target_arch = "wasm32"))]`.
- **Notes:** Don't add pollster as a dep; keep the wasm vs native split via two arms.

### T-101f: Open handler.
- **Touches:** `apps/carbide-ui/src/app.rs`
- **Depends on:** T-101e
- **Success criterion:** Dispatching `Command::Open` opens the file picker filtered to `*.tscl`, reads the bytes, and pushes them into `pending_open_bytes`. Sync native arm uses `rfd::FileDialog::pick_file()` + `std::fs::read`; wasm arm uses `AsyncFileDialog::pick_file().read()` inside `spawn_local`.
- **Notes:** Lift autosave for 2 seconds after Open to avoid clobbering the load: `self.suppress_autosave_until = now + 2.0`.

### T-101g: Drop `#[allow(dead_code)]`.
- **Touches:** `apps/carbide-ui/src/app.rs`
- **Depends on:** T-101e, T-101f
- **Success criterion:** Both `capture_state` and `restore_state` lose their `#[allow(dead_code)]` attribute; clippy stays green (the methods are now called from T-101e and T-101f).
- **Notes:** No-brainer cleanup; included as its own task so the lint state change is reviewable.

### T-102a: localStorage write.
- **Touches:** `apps/carbide-ui/src/state_io.rs`
- **Depends on:** T-101a
- **Success criterion:** `pub fn autosave_to_local_storage(bytes: &[u8])` exists. wasm32-gated: base64-encodes, calls `window.local_storage()?.set_item("carbide.autosave.v1", &encoded)`, swallows failure. Native: no-op. Skips silently when `bytes.len() > 4 * 1024 * 1024`.
- **Notes:** Use `base64::engine::general_purpose::STANDARD.encode(bytes)`. Use `ok()` to swallow Result and `if let Some(window) = web_sys::window()` rather than `.unwrap()`.

### T-102b: localStorage read + boot rehydrate.
- **Touches:** `apps/carbide-ui/src/state_io.rs`, `apps/carbide-ui/src/app.rs`
- **Depends on:** T-102a
- **Success criterion:** `pub fn load_from_local_storage() -> Option<Vec<u8>>` exists (wasm32: reads the key, base64-decodes; otherwise `None`). `CarbideApp::new`: if `Some(bytes)`, call `engine.open_bytes(&bytes)` + `restore_state` on the resulting `UiSnapshot`; only seed the demo cells when no autosave was found.
- **Notes:** Make rehydrate failures non-fatal — corrupt localStorage shouldn't break the app.

### T-102c: Dirty flag + central mark_dirty.
- **Touches:** `apps/carbide-ui/src/app.rs`
- **Depends on:** T-102a
- **Success criterion:** `dirty: bool`, `last_autosave: f64`, `suppress_autosave_until: f64` fields exist. `mark_dirty(&mut self)` flips `dirty = true`. Called from each entry point that mutates engine or UI state (sample: `apply_command`, `paste_*`, `set_cell_source` site, format setters, widget setters, conditional rule editor commits, stage mode toggle, note edits, sheet switch, etc.).
- **Notes:** Audit by grep — every `self.engine.set_cell`, `self.widgets.set*`, `self.notes.set`, `self.cond_rules.push/remove`, `self.{,hex_,triangle_}<sheet>.formats.update`, `self.stage_mode = ...`, `self.metrics.set_{col_width,row_height}` site should call `mark_dirty` afterward. Centralizing the helper makes adding new mutation sites trivial in future sprints.

### T-102d: Per-frame autosave tick.
- **Touches:** `apps/carbide-ui/src/app.rs`
- **Depends on:** T-102c, T-102a
- **Success criterion:** `fn maybe_autosave(&mut self, now: f64)` exists. Called once per `update()` (after the open-drain). When `self.dirty && now >= self.suppress_autosave_until && now - self.last_autosave > 2.0`, snapshot via `capture_state` → bytes → `autosave_to_local_storage`; clear `dirty`; set `last_autosave = now`.
- **Notes:** Capture cost dominates here — running this every frame is still O(formatted cells), tiny for the launch demos. If it ever becomes a hotspot, gate behind a second-level "needs-snapshot" flag.

### T-103: Local CI gate.
- **Touches:** (verification)
- **Depends on:** T-101a..T-102d
- **Success criterion:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, plus the same on `apps/carbide-ui`, plus `cargo build --target wasm32-unknown-unknown --manifest-path apps/carbide-ui/Cargo.toml` — all green.
- **Notes:** Same gate as sprint 0.

### T-104: PR and merge.
- **Touches:** (git)
- **Depends on:** T-103
- **Success criterion:** Branch `webui-v145-save-open-autosave` pushed; PR opened; CI all-green; squash-merged to main.
- **Notes:** Confirm conclusion via `gh run list --branch ... --json status,conclusion` before merging.
