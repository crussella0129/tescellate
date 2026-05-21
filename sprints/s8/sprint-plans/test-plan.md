Finalized - DO NOT EDIT

# Sprint 8 Test Plan

## Unit Tests
- Compile-time only; render-path testing without a winit event loop is
  impractical. `cargo build` (native + wasm) covers compilation.

## Integration Tests
- Covered by manual E2E.

## End-to-End Tests
- **Status:** possible (manual).
- `e2e_triangle_toggle_no_text_bleed`: launch app, switch to Tri demo,
  observe T(2, -1) — the checkbox renders alone, no TRUE/FALSE text
  peeking through. Click to flip; same visual result.
- `e2e_hex_toggle_no_text_bleed`: hex doesn't seed a Toggle at boot, so
  vacuous until a hex Toggle exists; the fix is in place when one lands.
- `e2e_square_slider_readable_at_launch`: launch app, switch to Budget,
  verify Rent/Food/Transport/Savings Goal sliders render with thumb +
  numeric value visible (column 1 ≥ 160 px, rows 4-7 ≥ 28 px on first
  launch).
