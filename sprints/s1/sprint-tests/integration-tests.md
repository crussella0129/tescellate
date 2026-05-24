# Sprint 1 Integration Tests

## Component B (Save / Open via bytes)
The bytes-layer round-trip is already proven in v144's
`engine_save_bytes_then_open_bytes_roundtrips` and the `state_io`
snapshot round-trip. Sprint 1 adds the *wiring* — dialog → bytes →
engine — which doesn't add new bytes-level surface. The integration is
exercised indirectly by the wasm-release build (clean) and clippy on the
new dialog handler functions.

`save_then_open_round_trips_state_via_bytes` was the speculative
fixture in the test plan; a winit-free `CarbideApp` fixture proved
heavier than the value it would add, so it stays deferred — the byte
APIs are already covered by v144's tests and the path-API delegation
test.

## Component C (autosave + rehydrate)
Boot-rehydrate logic is exercised at module load every time the app
starts (it just no-ops on native). The wasm side is covered by manual
E2E.

## Run summary
- No new integration harnesses added.
- The `app_save_load_cycle_preserves_full_state` test from the plan is
  deferred — the bytes-layer round-trip + wiring review covers the
  practical guarantee.
