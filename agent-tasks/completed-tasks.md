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

## T-104 (sprint 1)
- **Description:** Pushed `webui-v145-save-open-autosave`, opened PR #176, CI all green, squash-merged to main as commit `28392ae`. Sprint 1 ships the full Priority-3 launch-brief surface (Save/Open dialogs + localStorage autosave) on top of v144.
- **Completed:** 2026-05-20T20:55:00Z
- **Files modified:** (git only)
- **Commit:** `bf932d3`

## T-201 (sprint 2) — Widgets<K> + hex_widgets + hex render dispatch + Hex Game seed
- **Description:** Generalised `Widgets` to coord-generic `Widgets<K>` (mirrors FormatMap<K>/NoteMap<K>); split `app.widgets` into `square_widgets: Widgets<(u32,u32)>` and `hex_widgets: Widgets<HexCoord>`. Hex render pass dispatches Button (re-fires source on click) + Toggle (writes `bool_source` on change); Slider/ProgressBar on hex deferred. UiSnapshot gained `square_widgets` (`#[serde(alias = "widgets")]` for v145 back-compat) and `hex_widgets`. Hex Game demo seed adds `H(2,2)=RANDBETWEEN(1,6)` (Button widget) and `H(3,2)=H(2,2)` (Score readout). New tests: `widgets_generic_with_hex_coord_round_trip`, `v145_snapshot_loads_square_widgets_via_alias`. 248/248 UI tests pass.
- **Completed:** 2026-05-20T21:25:00Z
- **Files modified:** apps/tescellate-ui/src/widget.rs, apps/tescellate-ui/src/state_io.rs, apps/tescellate-ui/src/app.rs
- **Commit:** `189fdbb`

## T-202+T-203 (sprint 2) — CI gate + ship
- **Description:** Local CI gate green (fmt, clippy, test, wasm build). PR #177 opened, all 7 CI checks green (rustfmt+clippy, ubuntu/windows build+test, renderer, native-compile, python engine, wasm front-end), squash-merged to main as `a33692f`.
- **Completed:** 2026-05-20T21:50:00Z
- **Files modified:** (verification + git)
- **Commit:** (sprint 2 cleanup commit `7f3d732`)

## T-301..T-305 (sprint 3) — Carbide P2 hardening: XLOOKUP + dotted aliases
- **Description:** Lexer accepts `.<letters>` continuations after the alphanumeric ident run so `STDEV.P` lexes as one Ident. XLOOKUP added in `lookup.rs` (full Excel signature: `lookup_value, lookup_array, return_array, [if_not_found], [match_mode], [search_mode]`; match_modes 0/-1/+1 implemented; wildcard mode=2 errors with clear message; search_modes ±1 implemented; ±2 accepted as linear-scan fallback; parallel-array semantics: lookup and result must be equal-length). Eight dotted-name aliases registered in `stats.rs`: STDEV.P/STDEV.S/VAR.P/VAR.S/COVARIANCE.P/COVARIANCE.S/MODE.SNGL/RANK.EQ. 14 new tests in `reference_examples.rs` (7 XLOOKUP, 5 aliases, plus 2 lexer tests in lex.rs).
- **Completed:** 2026-05-20T22:20:00Z
- **Files modified:** crates/tescellate-formula/src/excellite/lex.rs, crates/tescellate-formula/src/excellite/funcs/lookup.rs, crates/tescellate-formula/src/excellite/funcs/stats.rs, crates/tescellate-formula/tests/reference_examples.rs
- **Commit:** `025d2d8` (PR #178 squash-merged as `38fd3e5`)

## T-401..T-405 (sprint 4) — Triangle widgets (close ADR-005 follow-up)
- **Description:** Extended `Widgets<K>` to the triangle sheet. `TescellateApp` gains `triangle_widgets: Widgets<TriCoord>`; capture/restore round-trip the field; UiSnapshot gains `triangle_widgets` with `#[serde(default)]` so older snapshots tolerate the addition. `draw_triangle_grid` got a widget dispatch pass (Button + Toggle) before the in-cell edit overlay, mirroring the hex pattern from sprint 2. Slider/ProgressBar still fall through to text per ADR-006. Demo seed at T(2,-1) — a Toggle on a cell that starts FALSE. Tests: `widgets_generic_with_tri_coord_round_trip` + extended `snapshot_round_trips_through_ui_state` fixture. 249/249 UI tests pass.
- **Completed:** 2026-05-20T23:00:00Z
- **Files modified:** apps/tescellate-ui/src/app.rs, apps/tescellate-ui/src/state_io.rs, apps/tescellate-ui/src/widget.rs
- **Commit:** (PR #179 squash-merged as `b03fc73`)

## T-501..T-505 (sprint 5) — Voronoi lattice engine bringup
- **Description:** New `crates/tescellate-tess/src/voronoi.rs` module: `VoronoiCoord(u32)` + `VoronoiLattice { seeds, bounds }`. Lattice trait impl: `cell_at` via nearest-seed Euclidean, `centroid` returns seed, `vertices` via Sutherland-Hodgman polygon clipping of bounds against every other seed's perpendicular bisector, `neighbors` returns every other seed (Direction::N placeholder; Delaunay-correct adjacency deferred to v150). `LatticeKind::Voronoi`, `LatticeHandle::Voronoi`, `ParsedCoord::Voronoi` variants threaded through `lib.rs` (every match arm updated). Address format `V(N)`. 8-seed default config in a 400×400 box. 10 new unit tests + 2 handle round-trip tests. Engine `add_sheet` integration + UI render deferred to v150.
- **Completed:** 2026-05-20T23:45:00Z
- **Files modified:** crates/tescellate-tess/src/voronoi.rs (new), crates/tescellate-tess/src/lib.rs, crates/tescellate-core/src/extent.rs
- **Commit:** `e050c37` (PR #180 squash-merged as `7e6a8e9`)

## T-601..T-607 (sprint 6) — Voronoi UI: complete Demo C
- **Description:** Threaded the Voronoi engine bringup all the way through to the UI. `impl Coord for VoronoiCoord` in `selection.rs` (degenerate `min_max` / `rect_cells` — Voronoi is single-cell-selection only this sprint). `TescellateApp` gains `voronoi_lattice: VoronoiLattice` and `voronoi: Sheet<VoronoiCoord>` fields. `ActiveSheet::Voronoi`, `CellId::Voronoi(VoronoiCoord)`, `ActiveSheetTag::Voronoi` variants threaded through every match in `app.rs` and `state_io.rs`. New `draw_voronoi_grid` render fn: convex-polygon fill (per-seed palette), centroid text, selection stroke, double-click-to-edit overlay. 4th "Voronoi" tab added to the tab bar. Eight demo seeds (`V(0)=Plains, V(1)=Forest, V(2)=42, V(3)=Tundra, V(4)=V(2)+8, V(5)=Desert, V(6)=Coast, V(7)=Highlands`) showcase mixed labels + a formula. v151 follow-up: widgets / formatting / range selection / fill drag / copy-paste on Voronoi.
- **Completed:** 2026-05-21T01:30:00Z
- **Files modified:** apps/tescellate-ui/src/app.rs, apps/tescellate-ui/src/selection.rs, apps/tescellate-ui/src/state_io.rs
- **Commit:** (PR #181 squash-merged as `5cd2053`)

## T-801..T-804 (sprint 8) — visual polish from the v150 release review
- **Description:** Two fixes the user surfaced after running the v150 release build. (1) Hex `paint_hex` + triangle `draw_triangle_grid` text passes now skip widget cells, so a Toggle's bool source ("TRUE"/"FALSE") no longer prints behind the checkbox (square already had this gate from v141). (2) `TescellateApp::new` enforces a widget-fit sizing floor: square columns hosting a Slider/Button/ProgressBar bump to ≥ 160 px, host rows to ≥ 28 px, so the launch-demo sliders render legibly instead of cramped at the 64 px default. The floor runs before boot-rehydrate, so a user's saved column widths still win.
- **Completed:** 2026-05-21T02:15:00Z
- **Files modified:** apps/tescellate-ui/src/app.rs
- **Commit:** `75a38bd` (PR #182 squash-merged as `54d346f`)

## T-901..T-906 (sprint 9) — Voronoi widgets + Carbide label fix
- **Description:** (1) Renamed the language-picker label "Excelite" → "Carbide" in `engine_label` (the `EngineKind::ExcelLite` enum variant stays for serialization back-compat). (2) Closed the four-lattice widget symmetry: `voronoi_widgets: Widgets<VoronoiCoord>` on `TescellateApp` + `UiSnapshot` (`#[serde(default)]`); capture/restore round-trip; `draw_voronoi_grid` gained a widget-skip gate on the text pass + a Button/Toggle render pass inscribed (120×24) at each widget cell's centroid; clicks on widget cells route to the widget, not select/edit. Demo Toggle seeded on `V(5)` (cell now `"FALSE"`). Tests: `widgets_generic_with_voronoi_coord_round_trip` + extended snapshot fixture. 250/250 UI tests pass.
- **Completed:** 2026-05-21T02:55:00Z
- **Files modified:** apps/tescellate-ui/src/app.rs, apps/tescellate-ui/src/state_io.rs, apps/tescellate-ui/src/widget.rs
- **Commit:** `b685d32` (PR #183 squash-merged as `eec563c`)

## T-1001..T-1003 (sprint 10) — square-grid viewport culling
- **Description:** `GridMetrics::visible_col_range` / `visible_row_range` — single-pass incremental walks returning the inclusive index span of cells whose extent overlaps the scroll clip-rect (boundary-straddling cells included; empty axis → (0,0)). `draw_grid` computes `(c0,c1)`/`(r0,r1)` from `ui.clip_rect()` once and culls all four full-axis loops (main cell paint, heavy-border pass, widget pass, frozen row/col header strips) from `0..ROWS`/`0..COLS` to the visible window. Cuts painted cells from 52×200 = 10,400 to ~the on-screen window (≈1,600 at 1080p), the dominant per-frame cost the user reacted to. 4 new unit tests (full/scrolled/straddle/empty). Residual O(index) cost in `cell_rect` left for a possible prefix-sum follow-up.
- **Completed:** 2026-05-21T03:30:00Z
- **Files modified:** apps/tescellate-ui/src/grid.rs, apps/tescellate-ui/src/app.rs
- **Commit:** `34883e7` (PR #184 squash-merged as `b18ede1`)

## T-1201..T-1204 (sprint 12) — XLOOKUP wildcard match
- **Description:** Implemented XLOOKUP `match_mode = 2` (the deferred wildcard path from v147). New `wildcard_match(pattern, haystack)` helper in `lookup.rs` — Excel glob (`?` = one char, `*` = any run incl. empty, `~` escapes the next char), case-insensitive, anchored to the full string, via a two-pointer algorithm (no exponential blowup). Wired into `xlookup`: error guard removed, range check widened to `-1..=2`, match loop globs `stringify(needle)` against `stringify(candidate)`, first hit wins. 7 in-module unit tests + 4 reference-examples tests; the v147 deferred test replaced with passing-match assertions.
- **Completed:** 2026-05-21T04:10:00Z
- **Files modified:** crates/tescellate-formula/src/excellite/funcs/lookup.rs, crates/tescellate-formula/tests/reference_examples.rs
- **Commit:** `d61ddf3` (PR #185 squash-merged as `ac40d18`)

## T-1301..T-1303 (sprint 13) — cell_rect O(1) prefix-sum layout
- **Description:** Perf follow-up to v153's culling. New `GridLayout { col_lefts, row_tops }` (prefix-sum arrays) built once per frame by `GridMetrics::layout(cols, rows)` in one O(cols+rows) pass; O(1) accessors `col_left`/`row_top`/`col_width`/`row_height`/`cell_rect`. `draw_grid` builds it after the visible-range computation and indexes it in the four culled loops (main paint, border, widget, header strips) instead of `cell_rect`'s O(index) `col_left`/`row_top` walk — drops scrolled-to-bottom per-frame positioning from ~320K accumulation ops to ~1,600. `GridMetrics::cell_rect` unchanged for hit-testing / resize callers. 3 parity unit tests (default + overrides + accessor equality).
- **Completed:** 2026-05-21T04:45:00Z
- **Files modified:** apps/tescellate-ui/src/grid.rs, apps/tescellate-ui/src/app.rs
- **Commit:** `48bed76`

## T-001 (sprint 16) — VoronoiConfig + LatticeConfig serde PODs
- **Description:** Serde-stable `VoronoiConfig { seeds: Vec<[f32;2]>, bounds: [f32;4] }` + `enum LatticeConfig { Voronoi(VoronoiConfig) }` in `voronoi.rs` (no glam-serde dependency). `VoronoiConfig::to_lattice()` delegates to `VoronoiLattice::new` (reuses coincident/degenerate validation); `From<&VoronoiLattice>` for the reverse. Re-exported from tess `lib.rs`. 4 unit tests (happy, reject-coincident, lattice round-trip, JSON round-trip).
- **Completed:** 2026-05-22T16:00:00Z
- **Files modified:** crates/tescellate-tess/src/voronoi.rs, crates/tescellate-tess/src/lib.rs
- **Commit:** `cb84d6c`

## T-002 (sprint 16) — Sheet.lattice_config field
- **Description:** `#[serde(default)] pub lattice_config: Option<LatticeConfig>` on `core::Sheet`. Fixed all three `core::Sheet` struct-literal sites (core round-trip test, `engine.rs:236` `add_sheet_with_extent` → `None` for now/T-004, `store/src/lib.rs` `sample()` helper). 2 new core tests (config round-trip + legacy-missing-key → `None`).
- **Completed:** 2026-05-22T16:05:00Z
- **Files modified:** crates/tescellate-core/src/lib.rs, crates/tescellate-formula/src/engine.rs, crates/tescellate-store/src/lib.rs
- **Commit:** `4e0a052`

## T-003 (sprint 16) — lattice_for from stored config
- **Description:** `lattice_for` matches `(lattice, &lattice_config)`: Voronoi + `Some(Voronoi(cfg))` → `cfg.to_lattice()` (eval uses custom seeds); `None` → `for_kind` default 8 seeds. New `SetCellError::BadLatticeConfig(String)` for a corrupt config (NOT `UnsupportedLattice`). 3 unit tests (stored 3-seed config, None→default 8, corrupt→`BadLatticeConfig`).
- **Completed:** 2026-05-22T16:12:00Z
- **Files modified:** crates/tescellate-formula/src/engine.rs
- **Commit:** `ad93eb2`

## T-004 (sprint 16) — seed Voronoi config at sheet creation
- **Description:** `add_sheet_with_extent` now sets `lattice_config = Some(Voronoi(VoronoiConfig::from(&VoronoiLattice::default())))` for `LatticeKind::Voronoi`, `None` otherwise — engine + UI agree from boot. 2 unit tests (Voronoi seeded with default, Square stays `None`).
- **Completed:** 2026-05-22T16:18:00Z
- **Files modified:** crates/tescellate-formula/src/engine.rs
- **Commit:** `9220b78`

## T-005 (sprint 16) — set_voronoi_seeds + voronoi_lattice getter
- **Description:** `set_voronoi_seeds(sheet, seeds)` validates via `to_lattice` before mutating, preserves bounds, stores the new config, then resets + `rebuild_dag()` (re-resolves every cell's `:NEIGHBORS`/radius edges against the new geometry — the C-001 fix) and recomputes the sheet's cells (topo order + volatile pass), returning the changed refs. `voronoi_lattice(sheet)` getter returns the stored lattice (or default-8 if `None`), `None` for non-Voronoi. 8 unit tests incl. the deterministic Delaunay adjacency flip (V0 sum 20→30 on a seed move) proving stale-edge recompute is fixed.
- **Completed:** 2026-05-22T16:35:00Z
- **Files modified:** crates/tescellate-formula/src/engine.rs
- **Commit:** `d18e431`

## T-006 (sprint 16) — .tscl FORMAT_VERSION 1→2
- **Description:** `FORMAT_VERSION = 2` + module doc version-history (v2 = sheets gained `lattice_config`; serde-default upgrade path for v1 files; bump exists so older builds reject v2 rather than dropping seeds). New `voronoi_seeds_survive_full_round_trip` test; existing v0-load (`reads_v0_as_empty_ui_state`) and v99-reject (`refuses_unknown_format_version`) tests still green.
- **Completed:** 2026-05-22T16:42:00Z
- **Files modified:** crates/tescellate-store/src/lib.rs
- **Commit:** `60c2637`

## T-007 (sprint 16) — apply_seed_drag pure helper
- **Description:** `apply_seed_drag(seeds, idx, delta, bounds) -> Vec<[f32;2]>` translates seed `idx` by `delta`, clamps into `bounds` minus `VORONOI_DRAG_INSET`, out-of-range idx → unchanged. Pure (egui-free, no-zoom precondition documented per C-006); gated `#[allow(dead_code)]` until T-008 wires it. 3 unit tests (translate, clamp, oob noop).
- **Completed:** 2026-05-22T16:48:00Z
- **Files modified:** apps/tescellate-ui/src/app.rs
- **Commit:** `ab7c5df`

## T-008 (sprint 16) — seed-handle drag + engine-synced cache
- **Description:** `draw_voronoi_grid` gained a seed-handle pass — per-seed `ui.interact` draggable dot (highlighted while dragged); on drag, `apply_seed_drag` → `engine.set_voronoi_seeds` → on Ok resync `voronoi_lattice` via `synced_voronoi_lattice` + `mark_dirty` (autosave), on Err snap-back. `rebind_sheet_ids` Voronoi arm resyncs the cache from the loaded engine config (anti-drift, ADR-012). Removed the T-007 `dead_code` gate. 1 new test (`synced_voronoi_lattice_matches_engine_config`); 261 UI tests pass.
- **Completed:** 2026-05-22T16:58:00Z
- **Files modified:** apps/tescellate-ui/src/app.rs
- **Commit:** `68999a0`
