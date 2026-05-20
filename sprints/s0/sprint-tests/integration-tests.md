# Sprint 0 Integration Tests

## Component A (store layer)
The unit tests in T-003 cover the round-trip integration: a non-trivial
`UiState` survives `save_full_to_bytes → load_full_from_bytes`, and a
hand-built v0 zip loads with `UiState::default()`. No further harness
was required at this layer.

## Component B + C (engine + UI state)
The integration target named in the plan as `app_save_load_cycle_preserves_full_state`
is **deferred to sprint 1**. Reason: building a TescellateApp fixture in
a unit test requires an `eframe::CreationContext`, which in turn needs a
running winit event loop — possible in a headless wgpu test harness but
not justified ahead of the dialog wiring it would exercise.

The pieces are exercised individually: T-004 (engine byte API
round-trips) plus T-006 (state_io round-trip) prove the two halves
compose mechanically; the integration check lands when the dialog
flows are in place.

## Component D (save/open via dialog)
Deferred — sprint 1 owns the dialog wiring and its in-memory test harness.

## Component E (localStorage rehydrate)
Deferred — sprint 1.

## Run summary
- Workspace integration runs: piggybacked on `cargo test --workspace` —
  no separate harness was added in this sprint.
