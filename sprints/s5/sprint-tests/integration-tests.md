# Sprint 5 Integration Tests

The handle-level round-trip tests (`voronoi_handle_canonicalises`,
`voronoi_handle_neighbors_returns_other_seeds`) exercise the integration
path lattice ↔ LatticeHandle ↔ caller, since `LatticeHandle` is the
boundary the engine and formula crates consume.

Engine-side `add_sheet(LatticeKind::Voronoi)` integration is deferred
to v150 — there's no integration surface to exercise this sprint
beyond the LatticeHandle round-trips.

No new harness needed.
