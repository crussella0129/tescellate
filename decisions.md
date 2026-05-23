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

## ADR-007 — Lexer accepts `.<letters>` continuations after idents (sprint 3)
**Date:** 2026-05-20
**Status:** Adopted, shipped in v147 (PR #178).

`lex_ident_or_ref` learned to consume `.<letters>` runs after the
alphanumeric identifier tail. This lets Excel's modern dotted-name
functions (STDEV.P, VAR.S, COVARIANCE.P, MODE.SNGL, RANK.EQ, …) lex
as a single `Ident` and register against the function registry under
their dotted spelling.

Why letters-only after the dot: float literals like `3.14` start with
a digit and take the `lex_number` branch, so they're unaffected.
Restricting the continuation to alphabetic chars also keeps the rule
visually distinct — `A1.X` doesn't get swallowed into a strange ident
because `A1` is a CellRef token before the dot rule runs.

## ADR-009 — Voronoi lattice carries its seed set; cells are bounded (sprint 5)
**Date:** 2026-05-20
**Status:** Adopted, shipped in v149 (PR #180).

`VoronoiLattice` stores `seeds: Vec<Point2>` + `bounds: Rect` as part of
its struct, unlike `SquareLattice` / `HexLattice` / `TriangleLattice`
which are uniform tilings parametrised only by a cell size. The seeds
ARE the lattice — every Voronoi cell is determined by the seed
configuration plus the bounding rectangle (used to clip otherwise-
unbounded cells against the demo region).

Why bounded: a real Voronoi cell on the convex hull of the seeds is
unbounded (extends to infinity). Clipping against `bounds` gives every
cell a finite convex polygon, which is correct for the "Static
Voronoi" demo the launch brief describes and keeps the polygon-as-
Vec<Point2> type signature consistent with the other lattices. If a
user later needs unbounded cells, the public API stays the same and
the internal algorithm swaps for a half-plane / ray-segment hybrid.

Why brute-force O(N²) Sutherland-Hodgman over Delaunay: smaller diff,
no external crate dep, fast enough for N ≤ 15. A Delaunay-driven
O(N log N) build can replace `vertices()` behind the same public API
when N grows past launch.

## ADR-008 — XLOOKUP ships parallel-array semantics; 2D-result variant deferred (sprint 3)
**Date:** 2026-05-20
**Status:** Adopted, shipped in v147.

Sprint 3's XLOOKUP requires `lookup_array.len() == result_array.len()`
and indexes into the flattened result by position. Excel's "return a
whole row from a 2D table" variant is a follow-up that pairs with the
cell-reference-shape work needed for OFFSET / INDIRECT.

Why this scope: the parallel-array form is the dominant use case (per
Microsoft's docs and StackOverflow patterns), the implementation is
clean, and shipping it now unblocks the 70-80% of XLOOKUP queries
without committing to a half-finished 2D shape that would need
backwards-incompat changes when the full ref system lands.

## ADR-010 — Voronoi Delaunay neighbors via the `delaunator` crate (sprint 15)
**Date:** 2026-05-21
**Status:** Adopted, shipping in v157.

`VoronoiLattice::neighbors` returned an every-other-seed placeholder.
Replaced with true Delaunay adjacency (two seeds neighbor iff their
Voronoi cells share an edge) computed via `delaunator = "1"`.

Why a crate over deriving adjacency from the existing Sutherland-Hodgman
clipping (user's call): `delaunator` is correct-by-construction, tiny,
and the triangulation is reusable if a future `vertices()` rewrite wants
O(N log N). Degenerate configs (<3 seeds, or all-collinear → empty
triangulation) fall back to the every-other-seed behavior.

## ADR-011 — Voronoi seed persistence stores the full lattice config on the Sheet (sprint 15, for the follow-up drag+persist sprint)
**Date:** 2026-05-21
**Status:** Accepted (design); implementation deferred to the drag+persist sprint.

The engine's `lattice_for` rebuilds the lattice from just `LatticeKind`
(`for_kind(Voronoi)` → default 8 seeds), so custom/dragged seeds can't
survive eval or save/load today. Decision: generalize the `Sheet` to
carry its lattice's full configuration (not just the kind), so dragged
Voronoi seeds persist and the engine's eval-time lattice matches the UI.
This is a `.tscl` format change — requires a `manifest.json` version bump
and an upgrade path (older files → default seeds). Larger than ADR-010;
sequenced as a separate sprint after v157.

## ADR-012 — Voronoi seed config lives on the `Sheet`; engine is the single source of truth (sprint 16)
**Date:** 2026-05-22
**Status:** Adopted, shipping in v158.

Implements the design accepted in ADR-011. The seed configuration is now a
persisted, single-sourced part of a Voronoi sheet:

- **`Sheet.lattice_config: Option<LatticeConfig>`** (`#[serde(default)]`,
  in `tescellate-core`) carries `LatticeConfig::Voronoi(VoronoiConfig {
  seeds: Vec<[f32;2]>, bounds: [f32;4] })`. `VoronoiConfig` is a serde-stable
  POD in `tescellate-tess` (no glam-serde dependency, keeps `workbook.json`
  reviewable); `to_lattice()` delegates to `VoronoiLattice::new` so the
  ADR-009 coincident/degenerate validation is reused. Uniform tilings keep
  `lattice_config == None`.
- **The engine is authoritative.** `lattice_for` builds the eval-time
  Voronoi lattice from the stored config (legacy `None` → default 8 seeds);
  `add_sheet` seeds a Voronoi sheet's config at creation; the UI's
  `voronoi_lattice` field is demoted to a *render cache* resynced from the
  engine (`voronoi_lattice` getter / `synced_voronoi_lattice` helper) after
  load and after every drag. This removes the pre-v158 dual-source split
  where the UI and engine held independent lattices.
- **`set_voronoi_seeds`** validates, stores the new seeds (bounds preserved),
  then resets + `rebuild_dag()` and recomputes the sheet. The rebuild is the
  key correctness point (plan-critic C-001): a seed move changes geometry but
  not formula text, so the static `:NEIGHBORS`/radius DAG edges resolved at
  edit time are stale; re-resolving every cell's deps against the new lattice
  is what makes neighbor-dependent cells re-evaluate. A corrupt config maps
  to a dedicated `SetCellError::BadLatticeConfig`, not `UnsupportedLattice`.
- **`.tscl` `FORMAT_VERSION` 1→2.** Seeds ride in `workbook.json`, so no new
  sidecar (cf. ADR-001). v1 files load with `lattice_config == None` via serde
  default; the bump exists so *older* builds reject v2 files rather than
  silently dropping seed data on a round-trip.

Scope (this sprint): seed *drag* only. Add/delete seeds, bounds editing, and
the px↔lattice-unit mapping under zoom (the seed-drag clamp assumes 1:1, no
zoom — C-006) are deferred. The next sprint is the Tescellate→Carbide rename,
which re-touches the format layer (`.tscl`→`.crbd`) — a conscious second,
mechanical format change after this v2 schema bump.
