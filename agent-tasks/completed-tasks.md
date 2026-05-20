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
