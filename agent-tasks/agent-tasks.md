# Agent Tasks (Persistent Backlog)

## Sprint 18 — Voronoi polish (v160)

- **T-001** — `fade_alpha(distance) -> u8` pure helper + 6 tests.
- **T-002** — Seed-handle pass uses `fade_alpha`; `seed_handle_alphas` pure wrapper + 4 tests (C-008 partial).
- **T-003** — `pop_out_widget_center` pure helper + 5 tests (lattice-space).
- **T-004** — Widget pass calls `pop_out_widget_center` when handle is visible; centroid fallback.
- **T-005** — `VoronoiConfig.frozen: bool` + serde-default + 3 tests.
- **T-006** — Engine `voronoi_frozen` getter / `set_voronoi_frozen` setter / `set_voronoi_seeds` preserves frozen + 4 tests.
- **T-007** — Ribbon "Freeze Seeds" toggle (contextual Voronoi group, hidden on other sheets).
- **T-008** — Render gate: seed-handle pass skipped when frozen; widget treats handle as never-visible.
- **T-009** — `pick_next_voronoi` pure helper (includes `current` exclusion guard) + 6 tests.
- **T-010** — `voronoi_advance` + `voronoi_visit_history` field; wired into `move_active`; history-clear on primary click + on seed-count change.
- **T-011** — ADR-014 in `decisions.md` (cross-refs ADR-005/009/010/012/013; UI-preference rationale for `frozen` round-trip).
