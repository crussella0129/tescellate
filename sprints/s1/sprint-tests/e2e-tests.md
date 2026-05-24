# Sprint 1 End-to-End Tests

**Status:** possible (manual).

CI runs all four checks against the actual wasm build, so the dialog +
autosave code is at least compilation- and lint-clean against the real
target. The remaining E2E targets need a human at a browser:

- `e2e_browser_save_open_roundtrip`: serve the wasm build, change a Budget
  slider, Ctrl+S → download a `.tscl`, refresh, Ctrl+O → pick the file,
  verify the slider returned to the saved position.
- `e2e_browser_autosave_survives_refresh`: edit any cell, wait > 2 s, F5,
  verify the cell value is restored from localStorage (boot rehydrate).
- `e2e_native_save_open_roundtrip`: `cargo run --manifest-path apps/carbide-ui/Cargo.toml`, Ctrl+S, write to disk, exit, restart, Ctrl+O, verify state restored.

Automated browser E2E (Playwright / wdio against the served wasm) is a
reasonable next-sprint addition once the launch demos are public — the
manual run is acceptable for sprint 1.
