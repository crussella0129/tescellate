Finalized - DO NOT EDIT

# Sprint 8 Build Plan

## Schema Tree

- **Sprint Goal:** v151 — polish fixes from the v150 release-build review.
  - **Component A — Text-bleed**
    - T-801: Hex `paint_hex` skips the cell-text pass when the cell is a widget.
    - T-802: `draw_triangle_grid` text loop skips widget cells.
  - **Component B — Widget sizing floor**
    - T-803: Square widget cells get min col width 160 px / min row height 28 px at startup.
  - **Component C — Ship**
    - T-804: CI gate + PR `webui-v151-widget-polish`.

## Execution Sequence

### T-801: Hex text-pass widget skip.
- **Touches:** `apps/carbide-ui/src/app.rs` (`paint_hex`).
- **Depends on:** (none).
- **Success criterion:** `paint_hex` computes `let suppress_text = self.hex_widgets.is_widget(coord);` and empties the text when true. Build clean.

### T-802: Triangle text-pass widget skip.
- **Touches:** `apps/carbide-ui/src/app.rs` (`draw_triangle_grid`).
- **Depends on:** (none).
- **Success criterion:** Triangle text-pass `if self.triangle_widgets.is_widget(coord) { continue; }`. Build clean.

### T-803: Square widget cell sizing floor.
- **Touches:** `apps/carbide-ui/src/app.rs` (`CarbideApp::new`).
- **Depends on:** (none).
- **Success criterion:** After the struct literal binds `this`, walk `this.square_widgets.iter()`; for each Slider/Button/ProgressBar, ensure the host column width is ≥ 160 px and the host row height is ≥ 28 px. Toggle cells use the default. Build clean.

### T-804: CI gate + PR.
- **Touches:** (verification + git).
- **Depends on:** T-801..T-803.
- **Success criterion:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, plus UI clippy/test, plus `cargo build --target wasm32-unknown-unknown` all green. PR `webui-v151-widget-polish` opened, CI passes, squash-merged.
