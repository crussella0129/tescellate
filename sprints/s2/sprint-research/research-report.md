# Sprint 2 Research Report

## 1. Sprint Goal

Ship the second half of launch Demo B (Hex Board Game): a clickable Roll
Dice button + a Score counter cell on the hex sheet. The blocker
called out in earlier sprint notes was "square-sheet widgets need
generalising to hex coords first." This sprint generalises `Widgets`
from its square-only `HashMap<(u32, u32), WidgetKind>` to a coord-generic
`Widgets<K>`, adds a `hex_widgets: Widgets<HexCoord>` field on
`CarbideApp`, extends the hex render pass to honour widget kinds for
those cells, threads the new field through `UiSnapshot`, and seeds the
demo with a Button on `H(2, 2)` (source `=RANDBETWEEN(1, 6)`) plus a
counter cell that aggregates rolls.

This wraps Demo B and lines up the remaining widget surface to be a
small follow-up (triangle widgets) rather than a load-bearing refactor.

## 2. Existing Code Survey

| File | Relevance | Notes |
|------|-----------|-------|
| `apps/carbide-ui/src/widget.rs` | high | `Widgets` (square-only) + `WidgetKind` enum + helpers (`bool_state/source`, `slider_value/source`, `progress_fraction`). Methods: `kind`, `is_widget`, `is_toggle/slider/button/progress_bar`, `set*`, `iter`, `replace_with`. Already serializes as Vec-of-pair via `WidgetsRepr`. Sprint 2 makes the inner map generic over `K`. |
| `apps/carbide-ui/src/app.rs` | high | `widgets: Widgets` field on CarbideApp; many use-sites (toggle/slider/button/progress setters in `apply_ribbon`; square-grid render path dispatches `WidgetKind`). Hex render path (`draw_hex_grid`) doesn't yet render widgets. The Hex Game demo seed currently has H(0,0)=label, H(0,2)=`=SUM(NEIGHBORS(...))`, plus six resource tiles. |
| `apps/carbide-ui/src/state_io.rs` | medium | `UiSnapshot` carries `widgets: Widgets` (square). Sprint 2 splits this into `square_widgets` + `hex_widgets` + (deferred) `triangle_widgets`. Bumps the snapshot schema; older snapshots tolerate the rename via `#[serde(alias)]` so a v145 autosave still loads. |
| `crates/carbide-tess/src/hex.rs` | low | `HexCoord { q, r }` has `Serialize`/`Deserialize`/`Hash`/`Eq`. Drops cleanly into `Widgets<HexCoord>`. |
| `apps/carbide-ui/src/ribbon.rs` | low | Ribbon's `ToggleWidget`/`ToggleSlider`/`ToggleButton`/`ToggleProgressBar` actions today only apply to the square sheet (the `apply_ribbon` arms check `ActiveSheet::Square`). Hex needs the same path; or we leave hex widget assignment to the seed and Carbide-driven flows for now and surface a follow-up in the report. |

## 3. External Sources

None needed — this is an in-repo refactor + demo seed addition. The
generic `Widgets<K>` mirrors `FormatMap<K>` (sprint 0) and `NoteMap<K>`
(also sprint 0), both of which already work for both square and hex via
the same `Coord` trait bounds (`Eq + Hash + Copy`).

## 4. Risks, Unknowns, Dependencies

- **Risk — `UiSnapshot` schema rename:** renaming the existing
  `widgets` field to `square_widgets` invalidates v145 autosaves
  silently (the renamed field defaults to empty). Mitigation: add
  `#[serde(alias = "widgets")]` on `square_widgets` so v145 saves
  rehydrate correctly. New `hex_widgets` defaults to empty for v145
  files — there's nothing to recover there.
- **Risk — hex render-pass interleave with selection-stroke pass:**
  the hex grid renders the hex shapes, then text, then borders, then
  selection outlines. The widget pass needs to sit between text and
  selection so widget controls are clickable but the selection
  stroke still draws on top. egui-side: use `ui.put(rect, widget)`
  with rect = hex bounding box centered on the centroid.
- **Risk — Button widget on hex needs `cell_source` access for label
  + re-fire semantics:** that already exists for square (`self.cell_source(c, r)` returns the cell's source string). Generalise to take a sheet/addr pair — `cell_source_addr(sheet_id, &addr)` — or duplicate the helper for hex. Pick the dedup'd path.
- **Risk — hex widget controls overlap the hex polygon edges:** square
  cells are axis-aligned rects, but a hex inscribed rectangle is
  ~80% of the hex's width. Inset the widget rect by `HEX_SIZE * 0.2`
  so it visually centres inside the hex.
- **Unknown — does the Slider widget look right on a hex?** A 36-point
  hex is small for a slider's thumb + value display. Probably fine for
  Button and Toggle, marginal for Slider, awkward for ProgressBar
  (read-only — a bar inside a hex looks fine actually). Sprint 2
  ships Button + Toggle for hex; defers Slider/ProgressBar on hex to a
  follow-up if a user asks (none have).
- **Dependency — none new.**

## 5. Recommended Approach

**Primary — generic `Widgets<K>` with hex specialization in this sprint, triangle deferred.**

1. **T-201a — Make `Widgets` generic over `K: Eq + Hash + Copy`.** The
   `WidgetsRepr` serde adapter becomes `WidgetsRepr<K>` with the same
   `serde(bound = ...)` pattern FormatMap uses. Existing methods keep
   the same names; their internal signatures change `(u32, u32)` → `K`.
2. **T-201b — In `CarbideApp`, rename `widgets: Widgets` →
   `square_widgets: Widgets<(u32, u32)>` and add
   `hex_widgets: Widgets<HexCoord>`.** Update every use site (~15 call
   sites in `app.rs`). The ribbon's `ToggleWidget`/`ToggleSlider`/
   `ToggleButton`/`ToggleProgressBar` arms continue to mutate
   `square_widgets` when `ActiveSheet::Square`. Hex widget edits go
   through the Carbide path (cell source = formula; the seed adds the
   widget by direct map insertion).
3. **T-201c — Hex render pass widget dispatch.** In `draw_hex_grid`,
   after the cell-text draw and before the selection-stroke pass,
   iterate `hex_widgets.iter()`. For each `(coord, kind)`:
   - Compute the hex centroid `(cx, cy)` via the existing `hex_lattice.centroid(coord)`.
   - Compute an inscribed rect (`HEX_SIZE * 1.4 × HEX_SIZE * 0.6`).
   - Dispatch the `WidgetKind` arms:
     - `Button`: `ui.put(rect, egui::Button::new(label))`. Label = `self.cell_source_hex(coord)`. On click, re-fire the source (the existing button re-fire dance used by square).
     - `Toggle`: `ui.put(rect, egui::Checkbox::new(&mut checked, ""))`. Checked = `bool_state(&value)`. On change, write `bool_source(checked)`.
     - `Slider` / `ProgressBar`: deferred (rendering challenges per §4). Render a `disabled` placeholder so the cell at least signals "this is a widget cell" — TBD whether to fall through to text rendering instead.
4. **T-201d — Capture/restore.** Rename `UiSnapshot::widgets` to
   `square_widgets` (with `#[serde(alias = "widgets")]`); add
   `hex_widgets: Widgets<HexCoord>` field with `#[serde(default)]`.
   Update `CarbideApp::capture_state` / `restore_state`.
5. **T-201e — Hex Game seed.** In `CarbideApp::new`, after the
   existing hex resource-tile seed:
   - Add a Roll Dice button at H(2, 2) with source `=RANDBETWEEN(1, 6)`.
   - Add a Score counter at H(2, 3) with source `=H(2, 2) + H(0, 2)` (or a similar accumulator pattern that re-evaluates on each dice roll because RANDBETWEEN is non-deterministic and `H(0, 2)` is the existing harvest sum).
   - Insert `(H(2, 2), WidgetKind::Button)` and `(H(0, 2), WidgetKind::ProgressBar { max: 20.0 })` into `hex_widgets` if ProgressBar can render acceptably on hex; otherwise leave H(0, 2) as a plain text cell.
6. **T-202 — CI gate + PR.** Same four-check gate as prior sprints.

**Alternative considered — hex-only buttons via a separate `hex_buttons: HashSet<HexCoord>` type, no generalization.** Smaller diff. Rejected: the launch brief explicitly calls for a "cross-sheet" widget surface, and the pattern `FormatMap<K>` / `NoteMap<K>` established by sprint 0 makes `Widgets<K>` the obvious shape; leaving `Widgets` square-only would be inconsistent and would just defer the same refactor.

**Rationale:** the refactor cost is bounded (one trait generalisation, one file's use-site updates, one render-pass extension). The launch demo completion is the user-visible payoff. Triangle widgets stay deferred — the Demo B brief doesn't need them, and triangle widgets aren't on the critical path for launch.

## Artifacts
None.
