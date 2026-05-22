# Agent Tasks (Persistent Backlog)

## Sprint 16 — Voronoi seed drag + persistence (v158)

- **T-006** — Bump `.tscl` `FORMAT_VERSION` 1→2 + doc + custom-seed round-trip test (tescellate-store).
- **T-007** — Pure `apply_seed_drag` helper that clamps a dragged seed into bounds (apps/tescellate-ui).
- **T-008** — Seed-handle draw + drag pass in `draw_voronoi_grid`; demote `voronoi_lattice` to engine-synced cache via pure `synced_voronoi_lattice` helper (apps/tescellate-ui).
- **T-009** — Record ADR-012 in `decisions.md`.
