# Sprint 9 Research Report

## 1. Sprint Goal

Two things this sprint:

1. **Voronoi widgets** — close the four-lattice widget symmetry. Square,
   hex, and triangle all carry `Widgets<K>`; the Voronoi sheet (shipped
   in v150) doesn't yet. Add `voronoi_widgets: Widgets<VoronoiCoord>`,
   render Button + Toggle inscribed in each Voronoi polygon, thread
   through `UiSnapshot`, and seed a demo Toggle.
2. **Carbide label fix** — the per-cell language picker shows "Excelite"
   (a typo for the old engine name) where it should read "Carbide", the
   project's name for the built-in formula language. One-line label
   change in `engine_label`; the `EngineKind::ExcelLite` enum variant
   stays unchanged (serialization back-compat).

## Decisions Reviewed

- **ADR-005 (sprint 2)** — `Widgets<K>` lattice-generic pattern. Sprint 9
  instantiates the fourth (and final, for now) coord type, `VoronoiCoord`.
- **ADR-006 (sprint 2)** — Button + Toggle ship; Slider/ProgressBar
  fall through to text on non-rectangular cells. Voronoi inherits the
  same deferral (its polygons vary in size; a slider inside an irregular
  polygon is awkward).
- **ADR-001 (sprint 0)** — `.tscl` `ui.json` schema evolution: new
  `UiSnapshot` field gets `#[serde(default)]`.

## 2. Existing Code Survey

| File | Relevance | Notes |
|------|-----------|-------|
| `apps/carbide-ui/src/app.rs` | high | `draw_voronoi_grid` (from v150) needs a widget-dispatch pass + the cell-text pass needs the widget-skip gate (mirrors the v151 hex/triangle fix). `CarbideApp` gains `voronoi_widgets: Widgets<VoronoiCoord>`. capture/restore + the `new` seed. `engine_label` (line ~5862) carries the "Excelite" → "Carbide" rename. |
| `apps/carbide-ui/src/state_io.rs` | medium | `UiSnapshot` gains `voronoi_widgets: Widgets<VoronoiCoord>` with `#[serde(default)]`. |
| `apps/carbide-ui/src/widget.rs` | low | `Widgets<K>` already generic; `VoronoiCoord` satisfies the bounds. No change. |

## 3. External Sources
None.

## 4. Risks, Unknowns, Dependencies

- **Risk — Voronoi polygon is large; inscribed widget rect needs sizing.**
  Voronoi cells vary in area; a fixed inscribed rect (like hex's
  `HEX_SIZE * 1.4 × 0.6`) doesn't map. Approach: centre a fixed
  `120 × 24` widget rect on the cell's centroid. For tiny cells it may
  overflow slightly; acceptable for the demo. A polygon-aware inscribed-
  rect fit is future work.
- **Risk — `engine_label` is also used for the workbook-default display
  and the "(default)" suffix.** Changing "Excelite" → "Carbide" updates
  every site that calls `engine_label` consistently — which is what we
  want (the label should read "Carbide" everywhere).
- **Unknown — does any test assert the string "Excelite"?** Grep first;
  if a test pins the old label, update it.

## 5. Recommended Approach

1. **T-901: `engine_label` rename** — `EngineKind::ExcelLite => "Carbide"`. Grep for any test/string dependency on "Excelite" and update.
2. **T-902: `voronoi_widgets` field + UiSnapshot** — add the field to `CarbideApp` and `UiSnapshot` (`#[serde(default)]`); capture/restore round-trip.
3. **T-903: Voronoi render widget dispatch** — in `draw_voronoi_grid`, add the widget-skip gate to the text pass + a Button/Toggle render pass inscribed at each widget cell's centroid.
4. **T-904: Demo seed** — a Toggle on one Voronoi cell (e.g. `V(5)`), seeded FALSE.
5. **T-905: Tests** — `widgets_generic_with_voronoi_coord_round_trip` + extend the snapshot round-trip fixture.
6. **T-906: CI gate + PR `webui-v152-voronoi-widgets`.**

**Rationale:** Voronoi widgets complete the per-lattice symmetry (the natural close to the ADR-005 arc), and the Carbide label is a cheap correctness fix the user spotted live — bundling them keeps the PR count down without muddying review (both are small, UI-surface changes).

## Artifacts
None.
