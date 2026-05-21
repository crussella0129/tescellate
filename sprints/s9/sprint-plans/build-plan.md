Finalized - DO NOT EDIT

# Sprint 9 Build Plan

## Schema Tree

- **Sprint Goal:** v152 — Voronoi widgets (close four-lattice symmetry) + Carbide label fix.
  - **Component A — Carbide label**
    - T-901: `engine_label` "Excelite" → "Carbide".
  - **Component B — Voronoi widget field + persistence**
    - T-902: `voronoi_widgets: Widgets<VoronoiCoord>` on `TescellateApp` + `UiSnapshot`; capture/restore.
  - **Component C — Render**
    - T-903: `draw_voronoi_grid` widget dispatch (Button + Toggle) + text-pass widget-skip gate.
  - **Component D — Demo + tests + ship**
    - T-904: Demo Toggle seed on a Voronoi cell.
    - T-905: Tests (widgets round-trip + snapshot fixture).
    - T-906: CI gate + PR `webui-v152-voronoi-widgets`.

## Execution Sequence

### T-901: Carbide label.
- **Touches:** `apps/tescellate-ui/src/app.rs` (`engine_label`).
- **Depends on:** (none).
- **Success criterion:** `EngineKind::ExcelLite => "Carbide"`. Grep confirms no test pins "Excelite"; if one does, update it. Build clean.

### T-902: `voronoi_widgets` field + UiSnapshot.
- **Touches:** `apps/tescellate-ui/src/app.rs`, `apps/tescellate-ui/src/state_io.rs`.
- **Depends on:** (none).
- **Success criterion:** `voronoi_widgets: Widgets<VoronoiCoord>` on `TescellateApp` (init `Widgets::default()` + demo seed in T-904). `UiSnapshot.voronoi_widgets` with `#[serde(default)]`. `capture_state` clones it out; `restore_state` calls `replace_with`. Build clean.

### T-903: Voronoi render widget dispatch.
- **Touches:** `apps/tescellate-ui/src/app.rs` (`draw_voronoi_grid`).
- **Depends on:** T-902.
- **Success criterion:** The text pass skips widget cells (gate: `if self.voronoi_widgets.is_widget(coord) { continue; }`). A new widget pass renders Button (re-fires source) + Toggle (writes `bool_source`) in a `120 × 24` rect centred on the widget cell's centroid; Slider/ProgressBar fall through (ADR-006). Build clean.

### T-904: Demo Toggle seed.
- **Touches:** `apps/tescellate-ui/src/app.rs` (`TescellateApp::new`).
- **Depends on:** T-902.
- **Success criterion:** `voronoi_widgets` initialiser sets a Toggle on `VoronoiCoord(5)`; the Voronoi seed loop changes `V(5)` from `"Desert"` to `"FALSE"` so the checkbox renders unchecked. Build clean.

### T-905: Tests.
- **Touches:** `apps/tescellate-ui/src/widget.rs`, `apps/tescellate-ui/src/state_io.rs`.
- **Depends on:** T-902.
- **Success criterion:** `widgets_generic_with_voronoi_coord_round_trip` (Button + Toggle round-trip through JSON). `snapshot_round_trips_through_ui_state` fixture extended to populate `voronoi_widgets`. `cargo test -p tescellate-ui` green.

### T-906: CI gate + PR.
- **Touches:** (verification + git).
- **Depends on:** T-901..T-905.
- **Success criterion:** fmt + clippy (workspace + UI) + `cargo test --workspace` + wasm build all green. PR `webui-v152-voronoi-widgets` opened, CI passes, squash-merged.
