# Sprint 2 End-to-End Tests

**Status:** possible (manual).

- `e2e_hex_dice_button_rolls`: launch the app (native or wasm), switch to
  the Hex Game sheet, click the dice button at H(2, 2), verify the cell
  value changes to a new integer in [1, 6]. Click 5 times — verify at
  least one of the rolls differs from the prior (RANDBETWEEN advances
  the PRNG each call).
- `e2e_hex_widget_survives_autosave`: launch wasm app, click dice, wait
  3 s, F5; verify the dice cell's last-rolled value rehydrates from
  localStorage and the button still works on the rehydrated workbook.
- `e2e_v145_save_loads_square_widgets`: load a v145-era `.crbd` (any
  workbook saved before this PR); verify the file's `widgets` field
  rehydrates as `square_widgets` (per the `#[serde(alias)]` we added).

Automated browser E2E (Playwright / wdio against the served wasm) is a
reasonable next-sprint addition once the launch demos are public — the
manual run is acceptable for sprint 2.
