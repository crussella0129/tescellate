# Architectural Decisions

## ADR-001 — `.tscl` v1 carries an opaque `ui.json` sidecar (sprint 0)
**Date:** 2026-05-20
**Status:** Adopted, shipped in v144 (PR #175).

The store crate's zip layout grew a third entry, `ui.json`, alongside the
existing `manifest.json` and `workbook.json`. `tescellate-store` exposes
this as a `UiState` newtype wrapping a `serde_json::Value` — the store
deliberately does **not** interpret the schema. The UI owns the typed
`UiSnapshot` in `apps/tescellate-ui/src/state_io.rs` and serializes /
deserializes it against the opaque blob.

Why opaque, not typed at the store level: the workbook engine doesn't
need to understand cell formatting, widget kinds, or stage flags to
persist them. Keeping the store schema-agnostic means UI-side schema
changes are confined to a single crate and don't require the
engine/store to track UI evolution.

Backward compatibility: `load_full` accepts v0 files (no `ui.json`) by
returning `UiState::default()`. `FORMAT_VERSION` bumped to 1; readers
that don't understand a higher future version error with
`StoreError::Version`.

## ADR-002 — Save / Open dialog wiring split across two PRs (sprint 0)
**Date:** 2026-05-20
**Status:** Adopted, infrastructure shipped in v144.

The original sprint 0 plan covered the full Priority-3 stack: store
schema bump, engine byte API, UI state capture, dialog flow, and
localStorage autosave. The sprint shipped the first three (T-001..T-006);
the dialog + autosave layer (T-101 + T-102 in the persistent backlog)
moves to sprint 1.

Rationale: the dialog flow is the largest unknown (rfd backend behavior
on wasm32, async-in-egui plumbing, base64 + localStorage quota
handling). Shipping the engine + serialization contract as its own
review unit means sprint 1 has a stable, byte-API-tested foundation to
wire against without churning the on-disk format mid-flight.

The `TescellateApp::capture_state` / `restore_state` methods are
gated `#[allow(dead_code)]` until sprint 1 calls them.
