# Sprint 1 Research Report

## 1. Sprint Goal

Wire the user-facing surface for `.tscl` persistence on top of the v144
infrastructure: **Save / SaveAs / Open** commands (Ctrl+S, Ctrl+Shift+S,
Ctrl+O) plus a ribbon File group, both fronted by an OS-native file
dialog on desktop and an HTML file picker / download blob on wasm.
Underneath, run a **localStorage autosave** with a 2-second dirty-debounce
that rehydrates the workbook on boot before the seed demos fire.

Ship as v145. Reuses the v144 byte API (`WorkbookEngine::save_bytes` /
`open_bytes`) and `state_io::{capture_state, restore_state}` — no engine
changes expected. (See sprint 0's research report for the underlying
schema rationale and ADR-001/002.)

## 2. Existing Code Survey

| File | Relevance | Notes |
|------|-----------|-------|
| `apps/carbide-ui/src/state_io.rs` | high | v144 shipped this with `UiSnapshot`, `snapshot_to_ui_state`, `ui_state_to_snapshot`, and the `vec_pair` HashMap-key adapters. Sprint 1 extends it with `autosave_to_local_storage`, `load_from_local_storage`. |
| `apps/carbide-ui/src/app.rs` | high | `CarbideApp::capture_state` / `restore_state` already exist (`#[allow(dead_code)]` — sprint 1 calls them and removes the attribute). Save/Open command handling lands here; needs a `pending_open_bytes: Arc<Mutex<Option<Vec<u8>>>>` field for the async open flow, plus `dirty: bool` and `last_autosave: f64` for debounce. |
| `apps/carbide-ui/src/keymap.rs` | high | Already surveyed: `Command` enum with 20+ variants. Add `Save`, `SaveAs`, `Open`; bind in `navigating(...)` at the same precedence as `Copy`/`Paste`. Add to NAV_KEYS list in `app.rs` to ensure egui's stock Ctrl+S/Ctrl+O bindings get shadowed. Add SHORTCUTS rows. |
| `apps/carbide-ui/src/ribbon.rs` | medium | Past sprints added/tuned groups; the pattern for new buttons + `RibbonAction` variants is well-established. Add a File group as the leftmost ribbon group with Save / Open buttons. |
| `apps/carbide-ui/Cargo.toml` | high | Add `rfd = { version = "0.14", default-features = false }` and `base64 = "0.22"`. `wasm-bindgen-futures` already there. `web-sys` needs to come in with the `Storage` + `Window` features for localStorage. |
| `crates/carbide-formula/src/engine.rs` | low | Read-only — `save_bytes` / `open_bytes` already exist. No changes expected. |

## 3. External Sources

- [rfd 0.14 docs.rs](https://docs.rs/rfd/0.14/rfd/) — `AsyncFileDialog::new().set_file_name(name).save_file()` returns `Option<FileHandle>` on every backend. `FileHandle::write(&[u8])` is async; resolves a future on both wasm and native. `pick_file()` + `FileHandle::read()` is the mirror. The wasm backend triggers `<input type=file>` programmatically and uses `URL.createObjectURL` for downloads — exactly the dance ADR sprint-0 expected.
- [rfd wasm32 feature gating](https://github.com/PolyMeilex/rfd/blob/master/Cargo.toml) — confirms no native-only sys deps on wasm32. Default features include `gtk3` on Linux; we keep `default-features = false` to skip GTK pull on Linux native and rely on the file-dialog crate's GTK-less path. On Windows the native backend uses ComDlg32 with no crate sys deps.
- [base64 0.22](https://docs.rs/base64/0.22/base64/) — `engine::general_purpose::STANDARD.encode(&[u8])` / `.decode(&str)` is the API. No-std-friendly; compiles to wasm32 without features.
- [web-sys 0.3 — Storage](https://rustwasm.github.io/wasm-bindgen/api/web_sys/struct.Storage.html) — `window.local_storage()? -> Option<Storage>`. `Storage::set_item(key, value)` / `get_item(key)` returning `Result<Option<String>, JsValue>`. Quota errors raise `QUOTA_EXCEEDED_ERR`; we catch the Err and downgrade to no-op.
- [eframe wasm template](https://github.com/emilk/eframe_template) — the canonical "async work + egui" pattern uses `Arc<Mutex<Option<T>>>` plus `wasm_bindgen_futures::spawn_local`; the frame loop drains the slot at the top of `update()`. Production-tested.

## 4. Risks, Unknowns, Dependencies

- **Risk — rfd's GTK pull on Linux native:** `rfd = { default-features = false }` on Linux removes GTK; under that config it falls back to xdg-portal or a stubbed dialog. Mitigation: project doesn't target Linux native UI today (the wasm + Windows-native cases are the priority); document the deferral if it bites a Linux user.
- **Risk — `web-sys` feature set explosion:** every web-sys item needs its feature flag. Mitigation: enable only `Window`, `Storage`, `HtmlAnchorElement`, `Blob`, `Url`, `Document` — the smallest set that compiles. Audit with `cargo tree -e features` after.
- **Risk — autosave thrashes on every keystroke:** the dirty flag flips on every cell mutation, and a flurry of typing could trigger many saves. The 2s debounce mitigates but a long burst of edits still produces saves once every 2s. Acceptable for v145 — `setItem` on small payloads is sub-millisecond.
- **Risk — autosave races with explicit Open:** an Open in flight + an autosave tick = the just-loaded state could be over-written by an earlier dirty flag. Mitigation: clear `dirty` *immediately* on Open and suppress autosave for 2s afterward via `suppress_autosave_until: f64`.
- **Risk — egui captures Ctrl+S itself for shortcuts:** the `NAV_KEYS` consume_key pattern from the existing Ctrl+F / Ctrl+Z bindings is the proven hammer. Add `(CTRL, S)`, `(CTRL_SHIFT, S)`, `(CTRL, O)` to NAV_KEYS so egui never sees them raw.
- **Unknown — rfd's user-gesture requirement on wasm:** browsers only allow download / open within a synchronous handler chain from a user gesture. egui's input is dispatched inside `update()` which itself runs in response to a redraw event scheduled by user input. In practice this works (proven by other egui+rfd apps); fallback if it doesn't is a hidden `<input>` element + manual click() inside an egui input callback.
- **Dependency — none new beyond rfd, base64, web-sys.**

## 5. Recommended Approach

**Primary — split into two PRs, both shipping in sprint 1.**

1. **v145a: dialog wiring** (T-101 from the persistent backlog)
   - Add `rfd` + `web-sys` deps with the feature trim above.
   - Add `Command::{Save, SaveAs, Open}` + keymap bindings + NAV_KEYS entries + SHORTCUTS table rows.
   - Add `RibbonAction::{Save, Open}` + File group buttons.
   - In `app.rs`: dispatch handler that calls `engine.save_bytes(&snapshot_to_ui_state(&capture_state()))` and pipes the bytes to `rfd::AsyncFileDialog`. Mirror for Open: `pending_open_bytes: Arc<Mutex<Option<Vec<u8>>>>`, drained at the top of `update()`, calls `engine.open_bytes` + `restore_state(ui_state_to_snapshot(&ui))`.
   - Remove the two `#[allow(dead_code)]` attributes on `capture_state`/`restore_state`.

2. **v145b: localStorage autosave** (T-102)
   - Add `base64` dep.
   - In `state_io.rs`: `autosave_to_local_storage(bytes: &[u8])` (≤4 MiB cap, swallow failure) and `load_from_local_storage() -> Option<Vec<u8>>`. Both `#[cfg(target_arch = "wasm32")]`-gated, with native-side no-ops.
   - In `app.rs`: `dirty: bool` field + `mark_dirty(&mut self)` helper + central call sites at each mutation entry point (paste, set_cell, format change, widget add/remove, conditional rule edit, stage toggle, note edit). `last_autosave: f64`. Every frame: `if self.dirty && now - self.last_autosave > 2.0 { autosave; clear dirty }`.
   - In `CarbideApp::new`: try `load_from_local_storage` first; if it yields bytes, `open_bytes` + `restore_state` on the just-loaded engine and skip the demo seeds.

**Alternative considered** — ship both pieces as one PR. Faster to land but doubles the review surface. Rejected: each piece has independent test exposure and the autosave can be bisected to v145b if it regresses something subtle (e.g. dirty-flag wiring missing a mutation site).

**Rationale:** the v144 contract (`UiState`, `UiSnapshot`, `capture_state`/`restore_state`) is the load-bearing API. Sprint 1 is mostly call-site wiring + a few new dep entries. Keeping the dialog flow and the autosave flow as separate PRs preserves bisect granularity.

## Artifacts
None new this sprint; sprint 0's research notes remain authoritative for
the underlying schema.
