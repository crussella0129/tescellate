# Sprint 2 Test Report

## Summary
- Unit tests: 248 passed / 0 failed / 248 total (2 net-new this sprint:
  `widgets_generic_with_hex_coord_round_trip`,
  `v145_snapshot_loads_square_widgets_via_alias`; 246 carried forward).
- Integration tests: 0 net-new (per-component checks covered by the
  unit tests above).
- E2E tests: manual (browser + native), unrun in CI. See `e2e-tests.md`.
- CI status: green. PR #177 passed all 7 checks (rustfmt+clippy,
  ubuntu/windows build+test, renderer, native-compile, python engine,
  wasm front-end). Squash-merged as `a33692f`.

## Failures
None.

## Technical Debt Identified
- **Triangle widgets are still square/hex only.** Triangle could pick
  up the same `Widgets<TriCoord>` field once a demo needs it; not on
  the launch critical path.
- **Slider / ProgressBar on hex** fall through to ordinary text render.
  A 36-point hexagon is too small for the egui slider thumb + value
  display. If a user wants a hex slider, the right shape is a vertical
  drag handle inside the hex polygon — different control widget, not a
  layout tweak.
- **Hex widget assignment from the ribbon** isn't wired. Today widgets
  on hex only arrive through code (the demo seed). The ribbon's
  `ToggleWidget`/`ToggleSlider`/etc. actions need a per-active-sheet
  branch — straightforward but cut from this sprint to keep the
  diff small.
- **The Score readout** at H(3, 2) is a simple `=H(2, 2)` mirror. A
  cumulative-score cell would need either an ACCUMULATE primitive (real-
  time mode roadmap) or a manual roll-history pattern; deferred.

## Coverage Observations
- The Widgets generalisation has full coverage on both coord types
  (square and hex). Triangle is uncovered because there's no
  triangle widget surface yet — would come for free when triangle
  widgets land.
- The hex render path is covered only by manual E2E. A headless egui
  harness would let us assert "Button rendered at expected rect"
  without launching winit; not yet worth the wiring.
