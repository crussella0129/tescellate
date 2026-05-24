# Sprint 5 Test Report

## Summary
- Unit tests: 10 net-new in `carbide-tess` (7 in `voronoi::tests`, 2 in `handle_tests`, 1 implicit via existing `radius_count_matches_the_lattice_formula` etc. still green for non-Voronoi variants). All 57 tess tests pass.
- Integration tests: 0 net-new (LatticeHandle is the integration boundary; covered by the handle round-trip tests).
- E2E tests: N/A (engine-only sprint).
- CI status: green. PR #180 passed all 7 checks. Squash-merged as `7e6a8e9`.

## Failures
None.

## Technical Debt Identified
- **`neighbors(c)` returns every other seed** (Direction::N placeholder).
  Delaunay-correct adjacency — only seeds whose Voronoi cells share an
  edge — is the right shape for `NEIGHBORS(V(N))` in Carbide formulas.
  Deferred to v150.
- **`lattice_distance` for Voronoi** returns the Euclidean distance
  between seed centroids rounded to an integer. There's no canonical
  cell-step distance on a Voronoi diagram (cells have variable shape);
  if a user needs a graph-theoretic distance, the right fix is a
  Delaunay-graph BFS. Not on the launch critical path.
- **Unbounded cells** aren't supported — the impl clips every cell
  against `self.bounds`, which is correct for the "Static Voronoi"
  launch demo but wrong for the general unbounded Voronoi diagram. If
  the user later asks for arbitrary seed placement without a
  bounding rectangle, the public API stays the same; the internal
  algorithm needs a half-plane intersection that handles unbounded
  edges (ray-segment hybrid polygons).
- **Brute-force O(N²) cell construction.** Each `vertices(c)` call
  iterates every other seed. At N = 15 (well above the launch demo's
  ~8), this is 225 half-plane clips per cell × 8 cells = 1800
  operations per render frame. Acceptable today; a Delaunay-driven
  build is the O(N log N) replacement. Public API doesn't change when
  the implementation swaps.

## Coverage Observations
- Geometric correctness covered by inside-bounds + nearest-seed +
  centroid tests. The shape of the clipped polygon isn't asserted
  explicitly (just bounds + non-emptiness); a more exacting test would
  assert "the polygon contains the seed and doesn't contain any other
  seed", which is the defining property of a Voronoi cell.
- Handle dispatch fully covered for canonicalisation + neighbors.
  `enumerate_range`, `cells_within_addresses`, and `lattice_distance`
  for Voronoi land their first-cut implementations but only have
  compile-time coverage; the v150 follow-up should add unit tests
  there too.
