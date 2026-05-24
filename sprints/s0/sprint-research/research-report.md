# Sprint 0 Research Report

## 1. Sprint Goal

Ship Priority 3 from the launch brief: end-to-end `.tscl` persistence in the
pure-Rust egui/WebAssembly UI. Concretely:

- **Ctrl+S** in the app produces a downloadable `.tscl` file (browser blob,
  native file dialog where applicable).
- A **File → Open / Ctrl+O** action lets the user load a `.tscl` back, with the
  workbook *and* per-sheet UI state (formats, widgets, notes, conditional
  rules, stage flags) round-tripping faithfully.
- **localStorage autosave** keeps the current workbook safe across browser
  reloads; on boot the app rehydrates from localStorage and skips the
  Budget/Hex-Game seed demos when an autosave is present.

This makes the three launch demos *actually shareable* (the user can craft
one, save the file, post it to a teammate) — the unlock the launch brief
calls out as the bridge between "HyperCard-with-spreadsheet-syntax" and
"thing you'd ship a demo of."

## 2. Existing Code Survey

| File | Relevance | Notes |
|------|-----------|-------|
| `crates/carbide-store/src/lib.rs` | high | Already implements the `.tscl` zip format: `manifest.json` + `workbook.json`, deflate-compressed. `save`/`load` take `Write+Seek`/`Read+Seek`; helpers `save_to_bytes`/`load_from_bytes` for in-memory round trips. `FORMAT_VERSION = 0`. Tests already exercise round trip + unknown-version refusal. |
| `crates/carbide-formula/src/engine.rs` | high | `WorkbookEngine::save(path)` / `open(path)` exist but use `std::fs::File` — **path-based, won't link on wasm32**. Need byte-oriented variants that delegate to `carbide_store::{save_to_bytes, load_from_bytes}`. |
| `crates/carbide-core/...` | high | `Workbook { id, meta, default_engine, sheet_order, sheets }` and `Cell { source, engine, value }` are already `Serialize`/`Deserialize`. This is the cargo we ship through the format. |
| `apps/carbide-ui/src/app.rs` | high | The egui app holds `WorkbookEngine` *plus* an `ActiveSheet`, three `Sheet<C>` selection bundles, three `FormatMap`s, three `Widgets`, three `NoteMap`s, conditional `Rule` lists, the `History`, the Stage Mode flag, and view geometry. **None of this UI state currently round-trips through `Workbook`.** Anything we want to survive a save needs to ride a sibling JSON inside the zip. |
| `apps/carbide-ui/src/format.rs` | high | `FormatMap` + `CellFormat` + `Borders` + `HexBorders`. egui's `Color32` is the awkward field (it's not `Serialize` by default in our egui pin) — we serialize as `[u8;4]`. |
| `apps/carbide-ui/src/widget.rs` | high | `Widgets` is `HashMap<(u32,u32), WidgetKind>`; `WidgetKind` already has the variants `Toggle/Slider/Button/ProgressBar`. Add `#[derive(Serialize, Deserialize)]` and it round-trips. |
| `apps/carbide-ui/src/note.rs` | medium | Small map of per-cell note strings. Trivial to serialize. |
| `apps/carbide-ui/src/conditional.rs` | medium | `Rule { condition, format }`; same `Color32` pattern as `format.rs`. |
| `apps/carbide-ui/src/history.rs` | low | Undo stack — **intentionally not persisted**. New session starts with an empty history. |
| `apps/carbide-ui/src/keymap.rs` | medium | Commands enum + `SHORTCUTS` table. Need to add `Save`, `SaveAs`, `Open` commands and bind Ctrl+S / Ctrl+Shift+S / Ctrl+O. egui already swallows Ctrl+S by default; we shadow it with `consume_key`. |
| `apps/carbide-ui/src/ribbon.rs` | medium | Save/Open/New buttons live here; add `RibbonAction::Save / SaveAs / Open / New`. |
| `apps/carbide-ui/Cargo.toml` | high | Currently depends on core/tess/formula; **does not** depend on `carbide-store`. Adding the store pulls `zip` and `serde_json` in. Both compile to wasm32-unknown-unknown (pure-Rust deflate via `flate2`'s rust_backend feature; `zip` has the `deflate-flate2` feature that selects it). Need to verify nothing in the dependency tree drags in a native-only crate (e.g. `bzip2-sys`). |

## 3. External Sources

- [web-sys: HtmlAnchorElement + Blob](https://rustwasm.github.io/wasm-bindgen/api/web_sys/struct.HtmlAnchorElement.html) — programmatic download pattern: build a `Blob` from the bytes, `URL::create_object_url_with_blob`, set `<a download>` href, `.click()`, revoke the URL. This is what we'll use for save-to-disk on wasm.
- [web-sys: HtmlInputElement file picker](https://rustwasm.github.io/wasm-bindgen/api/web_sys/struct.HtmlInputElement.html) — `<input type="file" accept=".tscl">` plus a `change` listener that pulls the chosen `File`, hands it to a `FileReader`, and resolves a `js_sys::Promise` with the bytes. Async — must be awaited via `wasm-bindgen-futures::spawn_local`.
- [web-sys: window().local_storage()](https://rustwasm.github.io/wasm-bindgen/api/web_sys/struct.Storage.html) — `set_item(key, value)`/`get_item(key)`. Values are JS strings, so a `.tscl` zip needs base64 encoding for transport. Quota is browser-defined (Chrome ~10 MiB per origin); we'll cap at 5 MiB and surface a non-blocking toast if a workbook exceeds it.
- [rfd crate](https://docs.rs/rfd/latest/rfd/) — cross-platform native file dialog with a wasm backend. The wasm backend ends up calling the same Blob/Input dance we'd write by hand. Pulling rfd in means the UI gets native dialogs *and* wasm dialogs from one API. **Decision: use rfd; only fall through to raw web-sys if rfd's wasm path can't trigger a download from a non-user-gesture context.**
- [zip crate features](https://docs.rs/zip/latest/zip/) — confirms `deflate-flate2` selects the pure-Rust deflate path; that's what carbide-store currently uses, so wasm is already supported transitively.

## 4. Risks, Unknowns, Dependencies

- **Risk — UI state schema drift:** adding `ui.json` to the zip means any future change to FormatMap / Widgets / Conditional rules is a format-version bump. Mitigation: encapsulate the UI state in a single versioned `UiState` struct with `#[serde(default)]` on every field so v1 readers tolerate v2 files missing fields and vice versa.
- **Risk — egui `Color32` serde:** the current `Color32` field on `CellFormat` is not directly `Serialize`. Need a `#[serde(with = "...")]` adapter that round-trips as a 4-byte RGBA array. This is small but touches every `CellFormat` field across `format.rs` and `conditional.rs`.
- **Risk — autosave clobbering a fresh demo seed:** the app seeds Budget + Hex-Game demos on first load. If we autosave aggressively, the seed becomes the autosave after one frame and the user never sees a fresh seed again. Mitigation: tag the seed workbook with a "is_fresh_seed" flag in UiState; only persist after the *first* user-initiated edit.
- **Risk — Ctrl+S browser default:** browsers map Ctrl+S to "save page as HTML". We swallow it via egui's `consume_key` in NAV_KEYS. Already the pattern for Ctrl+F / Ctrl+Z; same mechanism extends cleanly.
- **Risk — `zip` on wasm32 with `flate2` default features pulls in `miniz_oxide`:** want to confirm by inspecting `cargo tree`. If a non-rust backend sneaks in we explicitly select `flate2 = { version = "...", default-features = false, features = ["rust_backend"] }` indirectly via `zip`'s feature flags.
- **Unknown — async open() in egui:** egui is immediate-mode; we can't `await` inside `update()`. Pattern: `spawn_local` the future, write the resulting bytes into an `Arc<Mutex<Option<Vec<u8>>>>` field, and check it on the next frame. Standard for egui+wasm; doable but worth a small helper.
- **Unknown — rfd's behavior with non-user-gesture triggers:** browser security requires a user gesture for download/upload. Egui dispatches synthetic events; need to confirm rfd's wasm backend treats an egui-handled Ctrl+S as a valid gesture. If not, we fall back to attaching a hidden HTML input element to the DOM and triggering it inside an `egui::InputState` callback that's still on the user-input stack.
- **Dependency — bumping `manifest.json` format_version:** the project CLAUDE.md is explicit ("Don't change the file format shape without bumping `manifest.json` version and adding an upgrade path"). We bump to `FORMAT_VERSION = 1`, store-side reads v0 as "no ui.json present" (Default UiState).

## 5. Recommended Approach

**Primary — phased rollout in a single sprint, with the store layer extended once and consumed by both the UI and the existing engine path-API.**

1. **Engine API** — add `WorkbookEngine::save_bytes() -> Result<Vec<u8>>` and `WorkbookEngine::open_bytes(&[u8])`. The existing `save(path)/open(path)` keep working (they shell to the new methods).
2. **Store extension** — bump `FORMAT_VERSION` to 1; add an optional `ui.json` member; `save`/`load` take an extra `&UiState`/return `(Workbook, UiState)`. `UiState` is an opaque-to-engine JSON value (the engine doesn't interpret it; the UI does). Engine callers that don't have a UiState pass `UiState::default()` — i.e. an empty object — which is forwards-compatible.
3. **UI capture/restore** — `apps/carbide-ui` adds `state_io.rs`:
   - `capture(&self) -> UiState` snapshots the three FormatMaps, three Widgets, three NoteMaps, the conditional rule lists, the active sheet enum, stage_mode, and view geometry.
   - `restore(&mut self, UiState)` writes them back.
   - Both are pure functions over the app struct; no rendering.
4. **Ctrl+S / Save** — keymap adds `Command::Save`. On wasm: build bytes via `engine.save_bytes()` + `state_io::capture`, hand to `rfd::AsyncFileDialog::new().save_file()`. On native: same, but rfd opens a native dialog.
5. **Ctrl+O / Open** — same pattern, mirror direction. On wasm: rfd async; on resolved bytes, call `engine.open_bytes()` + `state_io::restore`. Lift the autosave debounce briefly to avoid the open immediately overwriting itself.
6. **localStorage autosave** — `state_io::autosave_to_local_storage()` runs on a debounced dirty flag (every 2s if the workbook changed). Stores base64-encoded `.tscl` bytes under key `carbide.autosave.v1`. On app boot, before seeding demos, look up the key; if present, restore from it.
7. **Tests** —
   - `carbide-store`: round-trip a workbook + non-empty UiState through `save_to_bytes`/`load_from_bytes`.
   - `carbide-store`: refuse format_version 99; tolerate format_version 0 (yields `UiState::default()`).
   - `carbide-ui` unit tests for `state_io::capture` / `restore` round trip a stub app fixture.
8. **CI gate** — workspace test, clippy, fmt, and `cargo build --target wasm32-unknown-unknown --manifest-path apps/carbide-ui/Cargo.toml`. All four must pass.

**Alternative considered — two-file split (engine `.tscl` + UI `.tscl-ui` sidecar).** Cleaner separation, but doubles the user-visible artifact and complicates "send a teammate the file" — they'd lose styling. Rejected: the file format already supports multi-entry zip; we use it.

**Rationale:** the store layer is the right home for the schema; the UI already knows how to flatten its state but doesn't need to know about zip framing. The format-version bump is unavoidable but the existing `Version(u32)` error path is exactly the upgrade path the project CLAUDE.md asks for.

## Artifacts

- `external-notes.md` — captured external-source highlights (web-sys patterns and rfd backend behavior summary).
