# Sprint 4 End-to-End Tests

**Status:** possible (manual).

- `e2e_triangle_toggle_widget`: launch app, switch to triangle sheet,
  click the toggle at T(2, -1), verify the cell value flips
  TRUE ↔ FALSE.
- `e2e_triangle_widget_survives_autosave`: edit the toggle, wait 3 s,
  F5, verify state restored.
- `e2e_pre_v148_save_loads_with_empty_triangle_widgets`: open a v147-era
  `.tscl` (which doesn't carry the `triangle_widgets` field); verify
  the workbook loads cleanly and `triangle_widgets` defaults to empty
  (per the `#[serde(default)]` on the new field).
