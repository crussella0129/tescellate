# Carbide

(Under Construction)

A spreadsheet where cells are not stuck being squares and the flow of time itself becomes a tool.

Carbide is a DAG-evaluated workbook with: 

- **Tessellating Cell Shapes** — squares, hexagons, triangles and Voronoi cells, with unique conditional formatting capabilities in some cases
- **Switchable formula language per cell**: Excel-style, Python (embedded via PyO3), or Rust (Rhai-preview with an optional rustc-compiled native path)
- **Programmable Widgets & Real-Time vs. Static Functionality: Build actual mock applications and games from your spreadsheets. Make them usable with 'stage mode' - hides all ribbons and editing tools.

> **Status: Phase 0** — foundation only. Nothing user-visible works yet. See [`PLAN.md`](./PLAN.md) for the architecture and roadmap.

## Why?

Different tilings encode different neighbor relations. Spreadsheets are how non-programmers do computation, but they're locked to a 4-neighbor grid. Hexes are the natural home for board games, cellular automata, and a lot of GIS. Triangles encode barycentric problems cleanly. Parallelograms suit isometric and crystallographic layouts.

The formula-language story is the other half: keep the Excel ergonomic on-ramp for casual users, give Python power users the in-cell `numpy` they already want, and give Rust developers a path from "scripted preview" all the way down to "real `rustc`-compiled code" without leaving the workbook.

## Quick links

- [`PLAN.md`](./PLAN.md) — the canonical architecture document.
- [`CLAUDE.md`](./CLAUDE.md) — project-specific guidance for Claude Code.

## Building

Build steps will be filled in as Phase 0 lands. Requires:

- Rust (stable, 1.75+)
- Node.js 20+
- (For Python engine, later) CPython 3.12+ headers/libs

## License

Dual-licensed under either of:

- Apache License, Version 2.0 ([`LICENSE-APACHE`](./LICENSE-APACHE))
- MIT license ([`LICENSE-MIT`](./LICENSE-MIT))

at your option.
