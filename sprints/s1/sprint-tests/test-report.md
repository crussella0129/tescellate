# Sprint 1 Test Report

## Summary
- Unit tests: 246 passed / 0 failed / 246 total (3 net-new — keymap
  bindings, native autosave no-op, native load no-op; plus 243 carried
  forward from v144). Workspace-side: all crate tests still pass.
- Integration tests: 0 net-new (the byte-layer round-trip from v144 and
  the wasm-release build cover the practical surface; the speculative
  `save_then_open_round_trips_state_via_bytes` fixture was deferred —
  see `integration-tests.md`).
- E2E tests: manual (browser + native), unrun in CI. See `e2e-tests.md`.
- CI status: green. PR #176 passed all 7 checks (rustfmt+clippy, ubuntu
  build+test, windows build+test, renderer, native-compile, python
  engine, wasm front-end). Squash-merged as `28392ae`.

## Failures
None.

## Technical Debt Identified
- `mark_dirty` is called centrally at command and ribbon-action dispatch.
  This means non-mutating actions (Find, OpenHelp, Save itself) also flip
  the dirty bit. The 2-second debounce + the explicit
  `dirty = false` in Save/Open keep this from causing observable
  thrash, but a tighter audit could land in a follow-up — only true
  mutations flip the bit.
- Native and wasm Save/Open arms duplicate scaffolding (`engine.save_bytes`
  + dialog construction). A helper that returns the bytes + the
  filename would let the cfg-split shrink to one function each.
- `rebind_sheet_ids` picks the first sheet per lattice; a workbook with
  two Square sheets loses the binding to the second. Acceptable for the
  current launch (single-sheet-per-lattice in the demos) but a
  multi-sheet workbook will need a richer binding strategy.
- `rfd 0.14`'s build script unconditionally requires the `gtk3` or
  `xdg-portal` feature even on wasm32 targets. We picked `gtk3` to
  satisfy the script; the gtk system libs do not link on wasm32, but
  the workaround is brittle. rfd 0.15+ may fix this.
- `Color32` round-trip is still lossy for alpha != 255 (egui's
  premultiplied storage) — unchanged from v144.

## Coverage Observations
- The dialog and autosave flows have no unit tests for the dialog
  interaction itself (rfd's `AsyncFileDialog` is hard to mock). The
  byte-API layer it feeds into IS unit-tested. Practical coverage
  comes from the manual E2E run.
- localStorage paths are entirely wasm-only; native tests just confirm
  the no-op behavior. A `#[wasm_bindgen_test]` harness for the wasm
  paths is the next coverage win.
