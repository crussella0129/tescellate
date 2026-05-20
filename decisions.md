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

## ADR-003 — `rfd` carries `gtk3` feature on wasm32 (sprint 1)
**Date:** 2026-05-20
**Status:** Adopted, shipped in v145 (PR #176).

`rfd 0.14`'s build script unconditionally panics if neither `gtk3` nor
`xdg-portal` is enabled, even when targeting wasm32 where the
wasm-specific codepath runs and no native backend is linked.
`xdg-portal` brings a transitive requirement on `tokio` or `async-std`;
`gtk3` does not. We pick `gtk3` on wasm32 only — the gtk system libs
never link on wasm32 because all the gtk-using rfd code is
`cfg(not(target_arch = "wasm32"))`-gated. Native targets use the
default backend selection (ComDlg32 on Windows, native dialog on macOS,
gtk3 on Linux).

Revisit if rfd 0.15+ ships a "wasm-only" feature that skips the build
script check.

## ADR-004 — `mark_dirty` over-fires; autosave debounce absorbs the cost (sprint 1)
**Date:** 2026-05-20
**Status:** Adopted, shipped in v145.

`mark_dirty()` is called centrally at command-dispatch and
ribbon-action-dispatch — once per user action, not once per genuine
state mutation. That means Save itself, Find, Help, Zoom, etc. all flip
the dirty bit even though they don't change persistent state. The 2 s
`maybe_autosave` debounce + the explicit `dirty = false` in Save / Open
makes the over-fire cost a wash in practice (a no-op redundant write).
This is a deliberate trade against the alternative of threading
mark_dirty through every individual mutation site, which is the kind of
audit that rots silently when a new mutation site is added without
updating it.

If autosave becomes a hotspot in the future, the right tightening is a
"genuinely-mutating commands" allowlist in `apply()`, not adding
mark_dirty to each call site.

## ADR-005 — `Widgets<K>` follows the lattice-generic pattern (sprint 2)
**Date:** 2026-05-20
**Status:** Adopted, shipped in v146 (PR #177).

`Widgets` joined `FormatMap<K>` and `NoteMap<K>` as a lattice-generic
collection keyed by an `Eq + Hash + Copy` coord type. The square sheet
uses `Widgets<(u32, u32)>`, the hex sheet uses `Widgets<HexCoord>`.
This is consistent with the per-lattice surface established in sprint 0
and means future widget surfaces (triangle, eventually Voronoi) plug in
without further refactor.

JSON encoding is a `Vec<(K, WidgetKind)>` via a `WidgetsRepr<K>` adapter —
HashMap keys can't be tuples/structs in JSON. `UiSnapshot.widgets`
became `square_widgets` with `#[serde(alias = "widgets")]` so v145
autosaves continue to load.

## ADR-006 — Hex widgets ship Button + Toggle; Slider/ProgressBar deferred (sprint 2)
**Date:** 2026-05-20
**Status:** Adopted, shipped in v146.

The hex render path dispatches Button (re-fires source on click) and
Toggle (writes `bool_source` on change) for cells in `hex_widgets`.
Slider and ProgressBar fall through to the ordinary text render —
36-point hexagons don't accommodate the egui slider thumb + value
display. If a real use case lands, the right answer is a different
control shape (vertical handle inside the hex polygon), not a layout
tweak to the existing rectangular slider.
