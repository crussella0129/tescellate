# Sprint 6 Integration Tests

The integration surface is the engine ↔ tess ↔ UI handshake exercised
implicitly by `cargo build --workspace`. The Voronoi tab's render path
calls `voronoi_lattice.vertices(coord)` / `centroid(coord)` / `cell_at(p)`
from the sprint-5 trait impl; the engine's `set_cell(voronoi_sheet, "V(N)", …)`
path goes through the sprint-5 `LatticeHandle::Voronoi` dispatch. Both
sides are unit-tested in their respective crates; this sprint glues
them together.

No new integration harness added.
