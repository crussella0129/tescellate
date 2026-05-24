# Architectural Decisions

## ADR-001 — `.tscl` v1 carries an opaque `ui.json` sidecar (sprint 0)
**Date:** 2026-05-20
**Status:** Adopted, shipped in v144 (PR #175).

The store crate's zip layout grew a third entry, `ui.json`, alongside the
existing `manifest.json` and `workbook.json`. `carbide-store` exposes
this as a `UiState` newtype wrapping a `serde_json::Value` — the store
deliberately does **not** interpret the schema. The UI owns the typed
`UiSnapshot` in `apps/carbide-ui/src/state_io.rs` and serializes /
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

The `CarbideApp::capture_state` / `restore_state` methods are
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
This is a `.crbd` format change — requires a `manifest.json` version bump
and an upgrade path (older files → default seeds). Larger than ADR-010;
sequenced as a separate sprint after v157.

## ADR-012 — Voronoi seed config lives on the `Sheet`; engine is the single source of truth (sprint 16)
**Date:** 2026-05-22
**Status:** Adopted, shipping in v158.

Implements the design accepted in ADR-011. The seed configuration is now a
persisted, single-sourced part of a Voronoi sheet:

- **`Sheet.lattice_config: Option<LatticeConfig>`** (`#[serde(default)]`,
  in `carbide-core`) carries `LatticeConfig::Voronoi(VoronoiConfig {
  seeds: Vec<[f32;2]>, bounds: [f32;4] })`. `VoronoiConfig` is a serde-stable
  POD in `carbide-tess` (no glam-serde dependency, keeps `workbook.json`
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
- **`.crbd` `FORMAT_VERSION` 1→2.** Seeds ride in `workbook.json`, so no new
  sidecar (cf. ADR-001). v1 files load with `lattice_config == None` via serde
  default; the bump exists so *older* builds reject v2 files rather than
  silently dropping seed data on a round-trip.

Scope (this sprint): seed *drag* only. Add/delete seeds, bounds editing, and
the px↔lattice-unit mapping under zoom (the seed-drag clamp assumes 1:1, no
zoom — C-006) are deferred. The next sprint is the Tescellate→Carbide rename,
which re-touches the format layer (`.tscl`→`.crbd`) — a conscious second,
mechanical format change after this v2 schema bump.

## ADR-013 — Voronoi interaction parity: `Selection.extra` + screen-rect marquee + shared formula-mode dispatch (sprint 17)
**Date:** 2026-05-23
**Status:** Adopted, shipping in v159.

Voronoi shipped (v149/v151) without the interaction-layer parity that
square/hex/triangle have: no formula-mode click-to-insert, no range
selection by drag, no name-box / formula-bar wiring (the Voronoi arm of
the name box was literally `=> {}`). The root cause wasn't four bugs —
the `formula_mode` helpers were already lattice-agnostic and just weren't
*called* from `draw_voronoi_grid`, and `Coord` for `VoronoiCoord` is
intentionally degenerate (rect ranges don't fit a non-grid tessellation —
ADR-005's lattice-generic precedent assumed rect-indexed coords).

This sprint:

- **`Selection<C>` gains `extra: SmallVec<[C; 4]>`** as an explicit-set
  escape hatch. `cells()` returns rect + extra (for render — the marquee
  outlines need every selected cell); a new **`primary_cells()` /
  `primary_contains()`** returns rect-only (the pre-v159 semantics) and
  every existing operational consumer (copy/paste, format apply, widget
  apply, border-edit, find filter) migrates to `primary_*` so Voronoi
  marquee extras don't silently fan out into pipelines that aren't yet
  multi-cell-ready. v161+ migrates those operational paths to extras as
  each format/widget pipeline gains Voronoi support.
- **`formula_mode::dispatch` + `event_from_response`** centralise the
  duplicated `if is_formula_buffer { drag_started/dragged/clicked }`
  block. Each `draw_*_grid` builds an `Event<C>` from `response.{clicked,
  drag_started, dragged, drag_stopped}()` via the pure
  `event_from_response` (unit-testable without an egui context — closes
  the critic-C-002 regression gap) and feeds it to `dispatch`. Voronoi
  joins via the same single call.
- **Screen-rect marquee** is the unified range rule for any lattice without
  rect-indexed coords. `cells_in_screen_rect(lattice, screen_rect, origin)`
  iterates the lattice's cells and keeps those whose centroid maps into
  the screen rect — built on the existing `Lattice::centroid` primitive,
  no new geometry math. Seed-handle drags take precedence (a press within
  5px of any seed centroid skips the marquee — the seed-handle pass
  handles it instead).
- **Name-box parity** — Voronoi's TextEdit name box is now wired the same
  way square's is: displays the active `V(n)` address on selection
  change, parses + jumps on Enter, shows an "N cells" hint when a marquee
  is active.

**Constraints honored:** square + hex + triangle full feature parity (the
shared `dispatch` is a code-extraction refactor, behaviour unchanged);
cross-lattice paste degrades formulas to values (selection-object rule,
untouched); engine / DAG / store untouched (pure UI + helper sprint);
no `app.rs` module split (function extraction inside `app.rs` +
`formula_mode.rs`).

**Cross-references:** ADR-005 (lattice-generic precedent — same "implement
the trait, inherit the behavior" promise, now extended from
storage/widgets/formats to interaction), ADR-009 (Voronoi geometry
primitives — `centroid`, `cell_at`, `vertices` — used by the marquee
rule), ADR-012 (Sheet-authoritative seed config the marquee enumerates).

**Deferred to v161+:** routing the format/widget/copy-paste/conditional-
formatting paths through the trait (today they stay one-per-lattice and
use `primary_cells()` to ignore marquee extras); literal merge of the
four `draw_*_grid` functions into one parameterised `draw_lattice_panel`.
The next sprint (v160) is the Tescellate→Carbide rename, applied to the
already-unified renderer for a smaller diff.

## ADR-014 — Voronoi polish: seed-handle fade + widget pop-out + persisted freeze + Enter-advance history rule (sprint 18)
**Date:** 2026-05-23
**Status:** Adopted, shipping in v160.

After v159 the user surfaced four Voronoi-side polish gaps; this ADR
documents the four locked design rules that close them.

**F1 — Seed-handle proximity fade.** Each seed's render alpha is a
piecewise function of cursor distance: `[<=40px] = 225`, `[>=80px] = 0`,
linear in between (`fade_alpha` pure helper). The 5-px hit area stays
alive throughout — a click/drag where the handle was last seen still
works. The per-frame per-seed decision is wrapped by `seed_handle_alphas`
(pure, no egui — closes the testability gap of v159's all-or-nothing
render bool). Priority order is `dragged > fade > no-hover`: a seed
being dragged renders at full opacity even if the cursor leaves the
panel, so the user doesn't lose sight of the handle mid-drag.

**F1b — Widget pop-out under handle visibility.** When the seed handle
becomes visible (cursor near), the cell's widget (Toggle/Button) is
offset to a side so its click target isn't occluded by the handle. The
`pop_out_widget_center` helper tries `[+x, -x, +y, -y]` offsets at 30 px
(lattice units, ADR-013 C-006 no-zoom precondition); the first candidate
where `cell_at(p) == Some(self_coord)` (i.e. still inside this cell's
polygon, ADR-009) wins. Falls back to centroid when no side has
clearance. When the widget cell is the editing cell, pop-out is skipped
— the edit overlay continues to anchor at centroid.

**F2 — Freeze seeds (persisted on the `Sheet`).** A new
`VoronoiConfig.frozen: bool` with `#[serde(default)]` rides in
`workbook.json` alongside the seeds. **No `.crbd` version bump** — the v2
schema (ADR-012) already accepts new optional fields this way. `frozen`
is classified as a **UI preference**, not user data: if an older v2
reader opens a frozen workbook and re-saves without the flag, the worst
case is the user can drag seeds they meant to lock (recoverable, not
data-destructive — different from ADR-012's seed-data case where losing
the geometry would silently corrupt the sheet, which is why ADR-012
*did* bump the version). When frozen: the seed-handle pass is skipped
entirely (no draw, no `ui.interact`) and the widget pass treats handle
as never-visible (widgets render at centroid). Toggle lives on a
contextual "Voronoi" ribbon group, visible only on `ActiveSheet::Voronoi`.

**F3 — Enter = commit-and-advance with the user's history rule.** On
Voronoi, Enter advances to a Delaunay neighbor (via ADR-010) that
**isn't in the last `N-1` visits**, where `N` is the current seed count.
Tie-break when multiple candidates qualify: **closest centroid**
(Euclidean), then lowest `VoronoiCoord` index for determinism. Returns
`None` when no candidate qualifies — Enter stops. Property: Enter
traverses each cell at most once before stopping. The `pick_next_voronoi`
helper is pure; `voronoi_advance_step` composes it with the engine
(testable with just a `WorkbookEngine` + `VecDeque`). The visit history
clears on (a) **primary cell-select click** (a deliberate selection
restarts traversal — distinguishes from formula-mode-click-as-reference
and widget clicks, which do NOT clear); (b) **workbook reload via
`rebind_sheet_ids`** (seed count may have changed; OOR `VoronoiCoord`s
in stale history would point at seeds that no longer exist).

**Cross-references:** **ADR-005** (lattice-generic precedent — same
"add the trait impl, inherit the behavior" promise, here extended into
per-lattice render polish); **ADR-009** (Voronoi bounded-cell guarantee
— F1b's `cell_at` containment + F3's `centroid` semantics both depend on
it); **ADR-010** (Delaunay adjacency — F3's neighbor walk uses it);
**ADR-012** (Sheet-authoritative config — F2's `frozen` flag rides in
the same struct as seeds); **ADR-013** (selection model + dispatch —
ADR-014 builds on the seed-drag and marquee foundations laid there).

**Deferred to v161+:** Carbide rename (now the v161 sprint). Add/delete
seeds (would change `N` and require history to track the size delta
within a session). Bounds editing. Per-cell widget anchor configuration
(pop-out direction is fully automatic in v160). Widget pop-out for
non-Voronoi lattices (the problem doesn't exist there — no draggable
handles to clear).

## ADR-015 — Tescellate → Carbide rename + stale-app cleanup (sprint 19)
**Date:** 2026-05-24
**Status:** Adopted, shipping in v161.

The locked carbide-rebrand direction (memory: `project_carbide_rebrand_and_native`) executed as a single sprint applied to the foundations laid by v158-v160.

**Renaming:**
- Every `tescellate-*` crate → `carbide-*` (5 crates: `core`, `tess`, `formula`, `store`, `cli`).
- `apps/tescellate-ui` → `apps/carbide-ui`.
- Every "Tescellate" / "tescellate" string in prose / UI / comments / docs / CI → "Carbide" / "carbide".
- `use tescellate_X::…` → `use carbide_X::…`.
- File extension `.tscl` → `.crbd`.

**Mechanics:**
The string sweep used `git ls-files | … | xargs perl -i -pe 's/Tescellate/Carbide/g; s/tescellate/carbide/g'`, with three explicit exclusions: (a) all of `agent-tasks/` (the historical task ledger preserves what happened pre-rename); (b) `.gitignore` (hand-edited for the dual-extension + dual-cache-path rules); (c) the three deliberately-preserved repo-URL sites (`Cargo.toml` `repository`, `CLAUDE.md` Repo identity, `apps/carbide-ui/src/app.rs` About hyperlink) — these keep pointing at `…/tescellate` until a tiny follow-up commit AFTER the user runs `gh repo rename`. After the sweep, `decisions.md`'s ADR-001 title was hand-restored to `.tscl v1` (historical), and one ADR-012 body line (`Tescellate→Carbide` referring to *this* rename) was restored from the broken `Carbide→Carbide`. Lockfiles (`Cargo.lock` + `apps/carbide-ui/Cargo.lock`) were deleted and regenerated by `cargo build`.

**Dual-extension load (back-compat):**
The on-disk zip schema (`manifest.json` + `workbook.json` + `ui.json`, `FORMAT_VERSION = 2`) is **unchanged** — only the file-name suffix flips. The UI's Open dialog accepts both `["crbd", "tscl"]`; Save defaults to `["crbd"]`. `.gitignore` ignores BOTH `*.tscl` and `*.crbd` (with `!examples/*.tscl` / `!examples/*.crbd` allowlists). The store crate's `load_full_from_bytes` is byte-driven (extension-agnostic by design), so old `.tscl` files load identically — pinned by the new `old_tscl_bytes_still_load_after_rename` test and by the committed `examples/sprint19-back-compat.tscl` artifact (used in the visual checkpoint).

**No `FORMAT_VERSION` bump.** ADR-001's rule "bump on schema change" applies to the *zip schema*, not the file-name suffix. ADR-012's body explicitly anticipated this: *"the next sprint (v161) is the Tescellate→Carbide rename, which re-touches the format layer (`.tscl`→`.crbd`) — a conscious second, mechanical format change after this v2 schema bump."* Cross-references: **ADR-001** (file-format schema layering), **ADR-012** (this sprint's anticipated counterpart), **ADR-013** (selection model — untouched), **ADR-014** (Voronoi polish — untouched).

**Stale-app cleanup:**
- **Deleted `apps/desktop`** (the Electron renderer last touched 2026-05-15; superseded by `apps/carbide-ui` per ADR-013's native-first direction). Frees up the flaky `renderer typecheck + build` CI job that auth-flaked in v159.
- **Deleted `crates/tescellate-ipc`** (the JSON-RPC server; only consumer was the Electron renderer).
- **`carbide-cli` collapsed to a `--help` stub** — the crate stays as a structural entry point for future headless eval / validate / batch work, but for now just prints a usage message. No engine coverage lost: the deleted `crates/tescellate-cli/tests/e2e.rs` exercised `set_cell`/`get_cell`/SUM/range/DivZero/JSON-RPC framing; the first five are covered in-process by `carbide-formula::engine::tests`, and the JSON-RPC framing test is meaningless once the IPC crate is gone.

**Cache-path verification (C-002):**
Pre-sweep `grep -rE "\.tescellate[/-]" --include="*.rs"` returned no runtime cache-path strings — only `.gitignore` referenced `.tescellate-cache/`. The sweep flipped doc mentions; `.gitignore` now lists BOTH `.tescellate-cache/` and `.carbide-cache/`. No runtime migration needed.

**Post-merge user-driven repo rename:**
The GitHub repo rename (`crussella0129/tescellate` → `carbide`) is a separate human-gated step (locked via 2026-05-24 AskUserQuestion). The sequence:
1. v161 PR merges (this sprint).
2. User runs `gh repo rename crussella0129/tescellate carbide`.
3. I push a tiny follow-up commit flipping the three preserved URL sites (`Cargo.toml`, `CLAUDE.md`, the About-box hyperlink) from `…/tescellate` to `…/carbide`.
4. I fix the local remote URL: `git remote set-url origin git@github.com:crussella0129/carbide.git`.

GitHub auto-redirects renamed-repo URLs indefinitely; the 404 window between merge and step 3 is theoretical. The principled rule (URLs flip AFTER the underlying repo rename) is the locked sequence.

**Deferred to v162+:**
- CSV / `.xlsx` import (calamine + csv crates) — original rebrand memo's post-rename item.
- Remaining trait unification (format / widgets / copy-paste through `CellInteract`) — per ADR-013's deferral.
- Behavioural changes; v161 is pure rebrand + cleanup.
