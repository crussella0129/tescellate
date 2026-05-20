Finalized - DO NOT EDIT

# Sprint 2 Build Plan

## Schema Tree

- **Sprint Goal:** v146 — generalize `Widgets` to a coord-parametric `Widgets<K>` and complete Hex Game demo with a Roll Dice button on the hex sheet.
  - **Component A — Widget generalization**
    - T-201a: `Widgets<K>` + `WidgetsRepr<K>` (coord-generic; serde bound mirrors FormatMap).
    - T-201b: Rename `app.widgets` → `app.square_widgets`; add `app.hex_widgets: Widgets<HexCoord>`. Update all call sites.
  - **Component B — Hex render pass**
    - T-201c: In `draw_hex_grid`, dispatch widget kinds (Button + Toggle) for cells in `hex_widgets`; defer Slider/ProgressBar on hex.
  - **Component C — Persistence**
    - T-201d: Update `UiSnapshot`: rename `widgets` → `square_widgets` (with `#[serde(alias = "widgets")]` for v145 back-compat); add `hex_widgets: Widgets<HexCoord>` field with `#[serde(default)]`. Update `capture_state` / `restore_state`.
  - **Component D — Hex Game demo seed**
    - T-201e: Add Roll Dice button + Score counter to the existing Hex Game seed in `TescellateApp::new`.
  - **Component E — Verification + ship**
    - T-202: Local CI gate (fmt, clippy, test, wasm build).
    - T-203: Open + merge PR `webui-v146-hex-widgets-dice`.

## Execution Sequence

### T-201a: Generalize `Widgets` over coord `K`.
- **Touches:** `apps/tescellate-ui/src/widget.rs`
- **Depends on:** (none)
- **Success criterion:** `Widgets<K>` and `WidgetsRepr<K>` compile with `K: Eq + std::hash::Hash + Copy`. The existing square use site keeps working via type inference (`Widgets<(u32, u32)>`). The serde derive uses `serde(bound = ...)` mirroring FormatMap. Existing widget tests pass.
- **Notes:** Methods that took `cell: (u32, u32)` now take `cell: K`. Use `K` in `set_toggle`/`set_slider`/`set_button`/`set_progress_bar`/etc.

### T-201b: Plumb `hex_widgets` through TescellateApp.
- **Touches:** `apps/tescellate-ui/src/app.rs`
- **Depends on:** T-201a
- **Success criterion:** App field renamed and a new `hex_widgets: Widgets<HexCoord>` added; every existing call site updated to refer to `square_widgets`. `apply_ribbon`'s widget setters continue to target the square sheet (the ribbon today operates on the active sheet's selection; widget setters were already gated to Square). Build is clean.
- **Notes:** Audit `self.widgets.` → `self.square_widgets.` everywhere; the count is ~15 sites per earlier grep. Initialize `hex_widgets: Widgets::default()` in `new` (the demo seed inserts entries in T-201e).

### T-201c: Hex render pass widget dispatch.
- **Touches:** `apps/tescellate-ui/src/app.rs`
- **Depends on:** T-201b
- **Success criterion:** In `draw_hex_grid`, a new widget-dispatch pass iterates `self.hex_widgets.iter()`. For `WidgetKind::Button`, render `egui::Button` inscribed in the hex centroid; click re-fires the cell's source through the engine (matches square Button semantics). For `WidgetKind::Toggle`, render `egui::Checkbox`; on change, write `bool_source`. `Slider` / `ProgressBar` fall through to default text rendering (deferred).
- **Notes:** Inscribed rect: `Rect::from_center_size(centroid, vec2(HEX_SIZE * 1.4, HEX_SIZE * 0.6))`. Place between the text pass and the selection-stroke pass.

### T-201d: UiSnapshot capture/restore for hex_widgets.
- **Touches:** `apps/tescellate-ui/src/state_io.rs`, `apps/tescellate-ui/src/app.rs`
- **Depends on:** T-201a, T-201b
- **Success criterion:** `UiSnapshot.widgets` renamed to `square_widgets` with `#[serde(alias = "widgets")]`. New `hex_widgets: Widgets<HexCoord>` field with `#[serde(default)]`. `capture_state` clones both fields out; `restore_state` calls `replace_with` on each. A v145 autosave (with the old `widgets` key) still loads its square widgets via the alias.
- **Notes:** The existing `snapshot_roundtrips_through_ui_state` test should still pass — update the test to assert on `square_widgets` (the new name) and add a hex_widgets entry to the test fixture so the new field is exercised.

### T-201e: Hex Game demo seed — Roll Dice + Score.
- **Touches:** `apps/tescellate-ui/src/app.rs`
- **Depends on:** T-201c, T-201d
- **Success criterion:** After the existing hex resource-tile seed in `TescellateApp::new`, add `engine.set_cell(hex_sheet, "H(2,2)", Some("=RANDBETWEEN(1,6)"))` (dice cell, Button widget) and `engine.set_cell(hex_sheet, "H(3,2)", Some("=H(2,2)"))` (Score readout). `hex_widgets.set_button(HexCoord::new(2, 2), true)`. Build clean.
- **Notes:** Don't collide with existing seed cells: H(0,0), H(1,0), H(-1,0), H(0,1), H(0,-1), H(1,-1), H(-1,1), H(0,2). H(2,2) and H(3,2) are free.

### T-202: Local CI gate.
- **Touches:** (verification)
- **Depends on:** T-201a..T-201e
- **Success criterion:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, plus the same on `apps/tescellate-ui`, plus `cargo build --target wasm32-unknown-unknown --release --manifest-path apps/tescellate-ui/Cargo.toml`. All green.

### T-203: PR + merge.
- **Touches:** (git)
- **Depends on:** T-202
- **Success criterion:** Branch `webui-v146-hex-widgets-dice` pushed; PR opened; CI green on all 7 checks; squash-merged to main.
- **Notes:** Confirm conclusion via `gh run list --branch ... --json status,conclusion` before merging.
