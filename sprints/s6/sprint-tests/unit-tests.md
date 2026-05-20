# Sprint 6 Unit Tests

## T-601 (Coord impl for VoronoiCoord)
- Compile-time verified via `cargo build --workspace`. The `impl Coord` is
  deliberately degenerate (single-cell only); explicit unit tests aren't
  meaningful here — the contract is "compiles and behaves as a single-cell-
  only Coord", which the rest of the UI surface relies on transitively.

## T-602 / T-603 / T-604 / T-605 / T-606 (App fields + render + tab)
- Compile-time only via `cargo build` (native + wasm) — render-path
  testing without a winit event loop is impractical. Manual E2E covers
  the user-visible behaviour.

## Run summary
- `cargo test --workspace`: all green (no new tests in this sprint;
  the existing 23 result sections pass unchanged).
- `cargo test --manifest-path apps/tescellate-ui/Cargo.toml --lib`:
  unchanged pass count from v149 (249 — the Voronoi rendering surface
  isn't unit-test-friendly without a winit event loop).
