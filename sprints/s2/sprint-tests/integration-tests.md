# Sprint 2 Integration Tests

## Component A (Widgets generic)
The hex-coord round-trip unit test exercises the generic surface
end-to-end. The square-coord variant is already covered by tests
carried forward from sprint 0. No additional integration harness needed.

## Component C (Persistence — alias + new field)
The `v145_snapshot_loads_square_widgets_via_alias` unit test is the load-
bearing integration check: it proves a real v145-format snapshot loads
into the v146 schema without data loss. No additional harness needed.

## Component B + D (hex render dispatch + demo seed)
Render-path testing without a winit event loop is impractical. The wasm
release build (clean) covers compilation + link; the click semantics are
covered by manual E2E.

## Run summary
- No new integration harnesses added.
