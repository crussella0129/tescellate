# Sprint 0 Test Report

## Summary
- Unit tests: ~250 passed / 0 failed / ~250 total. New tests added this
  sprint: 9 (UiState default, store v1 round-trip, store v0 tolerance,
  engine byte API round-trip, engine path-API delegation, CellFormat
  JSON round-trip, Widgets every-kind round-trip, Color32 adapter shape
  check, state_io snapshot round-trip + default).
- Integration tests: 0 net-new — per-component integrations either
  covered by existing crate-level tests or deferred with their owning
  tasks (see `integration-tests.md`).
- E2E tests: N/A (deferred — see `e2e-tests.md`).
- CI status: green. PR #175 passed all 7 checks (rustfmt+clippy, ubuntu
  build+test, windows build+test, renderer, native-compile, python
  engine, wasm front-end). Squash-merged to main as `b7ffff1`.

## Failures
None.

## Technical Debt Identified
- `TescellateApp::capture_state` / `restore_state` are dead code until
  sprint 1 wires the dialog flow. They're marked `#[allow(dead_code)]`
  with an inline comment naming the follow-up; removing the attribute
  is the lint that will catch sprint 1 forgetting to wire them.
- The `state_io::UiSnapshot` does not capture selection/cursor state.
  Acceptable for now — selection is more annoying than useful to
  persist (a freshly-opened workbook lands the user on A1 regardless of
  where they left it), but the field would slot in cleanly if a user
  asks for it.
- `Widgets`, `FormatMap`, and the note maps all needed a Vec-of-pair
  adapter for non-string JSON keys. The four definitions are similar
  enough that a generic adapter module would deduplicate them; not
  worth the abstraction overhead at four sites.
- `Color32` round-trip is lossy for alpha != 255 (egui premultiplies
  internally). The UI never uses transparent fills today; the unit test
  documents the limitation and the opaque case is covered. If
  transparent cell fills land later, the adapter needs to store the
  premultiplied bytes directly.

## Coverage Observations
- The byte-API + serialization layer is fully covered (round trips
  through every interesting variant of CellFormat, every WidgetKind,
  non-empty FormatMap / Widgets / NoteMap, and conditional rules).
- Dialog / autosave flows are uncovered by tests in this sprint because
  they aren't yet implemented. Sprint 1 will add them.
