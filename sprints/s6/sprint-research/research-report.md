# Sprint 6 Research Report

## 1. Sprint Goal

Complete launch Demo C by wiring the Voronoi engine bringup (shipped
in v149) all the way through to the egui UI. After this sprint the
user can switch to a "Voronoi" tab and see eight Voronoi cells
rendered as polygons with their cell values inside — fully read-only
or basic-interactive, with widgets / advanced formatting deferred to a
v151 follow-up.

## Decisions Reviewed

- **ADR-009 (sprint 5)** — `VoronoiLattice` carries `seeds` + `bounds`
  as struct state; cells are bounded against `Rect`. Sprint 6 consumes
  this without modification.
- **ADR-001 (sprint 0)** — `.crbd` `ui.json` sidecar schema. Sprint 6
  follows the established schema-evolution pattern: any new UiSnapshot
  field gets `#[serde(default)]` so older snapshots tolerate the
  addition.
- **ADR-005 (sprint 2) / ADR-006 (sprint 2)** — `Widgets<K>` lattice-
  generic pattern + hex/triangle Button+Toggle deferral. Sprint 6
  defers Voronoi widgets entirely (no `voronoi_widgets` field this
  sprint); the natural fit is `Widgets<VoronoiCoord>` if a real demo
  needs it later.

## 2. Existing Code Survey

| File | Relevance | Notes |
|------|-----------|-------|
| `apps/carbide-ui/src/app.rs` | high | The render path. Add `voronoi_lattice: VoronoiLattice`, `voronoi: Sheet<VoronoiCoord>` fields; `ActiveSheet::Voronoi`, `CellId::Voronoi(VoronoiCoord)` variants; a `draw_voronoi_grid` function modeled on `draw_hex_grid` (vertex polygons, text overlay, selection stroke). The tab bar lives ~line 5001; add a 4th tab. update() dispatch on `self.active` ~line 4800 needs the Voronoi arm. |
| `crates/carbide-formula/src/engine.rs` | low | `add_sheet(name, LatticeKind::Voronoi)` already works through the sprint-5 `LatticeHandle::for_kind` path — no engine changes needed. |
| `crates/carbide-tess/src/voronoi.rs` | medium | The Lattice trait impl is already complete. The UI calls `centroid` (for text placement), `vertices` (for polygon paint), `cell_at` (for click hit-test) — all exist. |
| `apps/carbide-ui/src/selection.rs` | medium | `Sheet<C>` is generic over `Coord`; `VoronoiCoord` needs to satisfy the `Coord` bound. Already does (`Eq + Hash + Copy + Serialize` from sprint 5). |
| `apps/carbide-ui/src/state_io.rs` | low | `UiSnapshot` keeps its current fields; sprint 6 only adds a (deferred) `voronoi_widgets` placeholder if needed. For minimum viable Demo C, nothing extra. |

## 3. External Sources
None — pure in-repo wiring.

## 4. Risks, Unknowns, Dependencies

- **Risk — `Coord` trait + `Selection<C>` need `step_back`/`min_max` and friends to work with `VoronoiCoord`.** `Coord` is implemented for the lattice-specific coords in `selection.rs`. Need to add the `impl Coord for VoronoiCoord` block: arithmetic operations are nonsensical (seeds aren't ordered geometrically), so most methods fall through to no-op / identity. Selection on the Voronoi sheet is single-cell only this sprint.
- **Risk — addresses with parens `V(0)` collide with the formula-mode parenthesis matching.** The Carbide parser already handles `H(q,r)` and `T(col,row)`, both of which carry parens, so `V(N)` follows the same shape and inherits the existing handling. Sanity-check by writing a `V(0) + V(1)` formula in the test plan.
- **Risk — render-pass tab-bar layout.** The existing tab bar shows three tabs; adding a fourth pushes them tighter. Acceptable; egui's tab layout handles variable widths.
- **Unknown — selection model.** Voronoi cells aren't arranged in a grid so range selection (`V(0):V(3)`) is awkward. Sprint 6 ships single-cell selection only; range selection is "future work" if a user needs it.
- **Unknown — fill drag, formula highlight marquee, cut/copy/paste on Voronoi.** All non-trivial; sprint 6 defers them. The Voronoi sheet supports: click to select, view cell value, edit cell value through the formula bar. That's enough for Demo C.
- **Dependency — none new.**

## 5. Recommended Approach

**Primary — one PR, scoped to "Demo C renders and you can interact with cells".**

1. **T-601: `Coord for VoronoiCoord`** in `selection.rs`. `step_back` returns the same coord (no spatial ordering); `min_max` is `(self, self)` (single-cell range only); `rect_cells` returns `[self]`. Most methods are degenerate but compile-correct.
2. **T-602: CarbideApp fields.** Add `voronoi_lattice: VoronoiLattice`, `voronoi: Sheet<VoronoiCoord>`. Initialise in `new` with `LatticeHandle::for_kind(LatticeKind::Voronoi).unwrap()` to get the default 8-seed config. `engine.add_sheet("Voronoi", LatticeKind::Voronoi)` returns the sheet id used by the new `Sheet<VoronoiCoord>`.
3. **T-603: `ActiveSheet::Voronoi`, `CellId::Voronoi(VoronoiCoord)`** variants. Every existing match on `ActiveSheet` needs an arm; most fall through to a Voronoi-specific call (e.g. `draw_voronoi_grid` in render, `voronoi_address` for the address bar).
4. **T-604: `draw_voronoi_grid` function.** Mirror `draw_hex_grid`'s structural pattern: allocate painter + interaction rect, resolve click position via `voronoi_lattice.cell_at`, first pass paint cell polygons, second pass text at centroid, third pass selection stroke, in-cell edit overlay.
5. **T-605: Demo C seed cells.** Eight cells matching the default 8-seed config.
6. **T-606: Tab bar + ActiveSheet dispatch.** Add "Voronoi" tab; route the active-sheet match to the new `draw_voronoi_grid`.
7. **T-607: CI gate + PR `webui-v150-voronoi-ui`** with the four-check gate.

**Deferred to v151:**
- Voronoi widgets (`Widgets<VoronoiCoord>`).
- Voronoi formats / notes / conditional rules.
- Range selection.
- Fill drag.
- Cut/copy/paste involving Voronoi.
- Delaunay-correct neighbors.

**Rationale:** Demo C's load-bearing feature is "you can see Voronoi cells with values inside them and click them to select." The rest is parity work that doesn't unlock anything launch-critical.

## Artifacts
None.
