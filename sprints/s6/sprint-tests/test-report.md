# Sprint 6 Test Report

## Summary
- Unit tests: 0 net-new this sprint; the existing 23 workspace result
  sections all pass (no regressions). UI lib test count unchanged from
  v149 (249 — the Voronoi rendering surface isn't unit-test-friendly
  without a winit event loop).
- Integration tests: 0 net-new — engine ↔ tess ↔ UI integration is
  covered by `cargo build --workspace` and the per-crate unit tests
  from sprints 5 and 0.
- E2E tests: manual (browser + native). See `e2e-tests.md`.
- CI status: green. PR #181 passed all 7 checks (rustfmt+clippy,
  ubuntu/windows build+test, renderer, native-compile, python engine,
  wasm front-end). Squash-merged as `5cd2053`.

## Failures
None.

## Technical Debt Identified
- **`voronoi_cell_text` warning fixed (was dead code in v149)** — the
  render fn now calls it. No new warnings.
- **`impl Coord for VoronoiCoord` is degenerate** by design. If a real
  use case lands for range selection on Voronoi (e.g. lasso-style
  multi-seed select), the impl needs revisiting — sprint 6 ships only
  the single-cell-selection contract.
- **30+ match arms gained `ActiveSheet::Voronoi => {}` no-op arms** in
  command-handler functions where Voronoi has no equivalent operation
  (fill drag, range navigation, format propagation). Each one is a
  v151 follow-up trigger if a user reaches for it on the Voronoi sheet.
- **`set_note` for Voronoi cells is a no-op.** Notes can be persisted
  in the future via a `voronoi_notes: NoteMap<VoronoiCoord>` field
  following the sprint-2 hex / sprint-4 triangle pattern.
- **`rebind_sheet_ids` for Voronoi** picks the first sheet with
  `LatticeKind::Voronoi` it finds in `sheet_order`. Consistent with the
  sprint-1 rebind pattern; same caveats apply.

## Coverage Observations
- The Voronoi UI is covered only by the compile-time gate + manual E2E.
- The engine + lattice layer has full unit-test coverage from v149.
- Persistence path (Save/Open round-tripping a Voronoi sheet) is
  uncovered by unit tests. Manual E2E is `e2e_voronoi_saves_to_tscl`.
