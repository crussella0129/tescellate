# Sprint 5 Unit Tests

## T-502 (Lattice trait impl)
- `voronoi_cell_at_returns_nearest_seed`: a 4-seed unit Voronoi; `cell_at` picks the nearest. **pass**
- `voronoi_cell_at_returns_none_outside_bounds`: points outside `bounds` return `None`. **pass**
- `voronoi_vertices_are_inside_bounds`: every Sutherland-Hodgman vertex falls inside `bounds` (±epsilon for boundary clips). **pass**
- `voronoi_centroid_is_seed_point`: `centroid(VoronoiCoord(i)) == seeds[i]`. **pass**

## T-503 (Handle dispatch + address parser)
- `voronoi_address_round_trips`: `parse_address(address(c)) == c`. **pass**
- `voronoi_address_out_of_range_errors`: `V(99)` on a 4-seed lattice errors `OutOfRange`. **pass**
- `voronoi_rejects_coincident_seeds`: construction errors when two seeds collide. **pass**
- `default_has_eight_seeds`: the launch-demo default configuration has 8 seeds. **pass**
- `voronoi_handle_canonicalises`: `LatticeHandle::for_kind(Voronoi).canonicalize("V(3)") == "V(3)"`; V(99) errors. **pass**
- `voronoi_handle_neighbors_returns_other_seeds`: every-other-seed first-cut adjacency returns 7 entries for the default 8-seed config. **pass**

## Run summary
- `cargo test -p carbide-tess`: 57 passed, 0 failed (10 net-new this sprint).
- `cargo test --workspace`: 23 result sections, all green.
