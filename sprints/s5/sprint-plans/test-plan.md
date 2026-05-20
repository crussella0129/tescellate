Finalized - DO NOT EDIT

# Sprint 5 Test Plan

## Unit Tests

### T-501 (module + types)
- Compile-time + the round-trip tests below.

### T-502 (Lattice trait impl + Sutherland-Hodgman)
- `voronoi_cell_at_returns_nearest_seed`: build a 4-seed Voronoi with seeds at (0,0), (10,0), (0,10), (10,10) and bounds = a 20×20 box around origin. `cell_at((1, 1))` → `Some(VoronoiCoord(0))` (closest to (0,0)).
- `voronoi_cell_at_returns_none_outside_bounds`: same lattice; `cell_at((100, 100))` → `None`.
- `voronoi_vertices_are_inside_bounds`: same lattice; for every seed, every vertex returned by `vertices` lies within `bounds` (inclusive — boundary vertices are clipped against the box).
- `voronoi_centroid_is_seed_point`: for every seed index `i`, `centroid(VoronoiCoord(i)) == seeds[i]`.

### T-503 (Handle dispatch)
- `voronoi_address_round_trips`: `parse_address(address(c)) == c` for `c = VoronoiCoord(0..N)`.
- `voronoi_address_out_of_range_errors`: `parse_address("V(99)")` on a 4-seed lattice → `Err(OutOfRange)`.
- `voronoi_handle_canonicalises`: `LatticeHandle::for_kind(LatticeKind::Voronoi).unwrap().canonicalize("V(3)") == "V(3)"` (default config has 8 seeds, so V(3) is valid).

### T-504 (catch-all)
- Existing lattice tests (`enumerate_range`, `neighbor_addresses`, etc.) still pass for the existing variants — adding the Voronoi arm shouldn't disturb them.

## Integration Tests

Engine-side `add_sheet(LatticeKind::Voronoi)` integration is deferred to
v150; there's no integration surface to exercise this sprint beyond the
LatticeHandle round-trip (covered by unit tests).

## End-to-End Tests

- **Status:** N/A. Engine-only sprint; no user-facing surface lands.
  Demo C E2E happens once v150 wires the UI render.
