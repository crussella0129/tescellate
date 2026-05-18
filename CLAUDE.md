# CLAUDE.md — Tescellate

This file gives Claude Code (claude.ai/code) project-specific guidance. It overrides the global `~/.claude/CLAUDE.md` where they conflict.

## What this project is

Tescellate is a DAG-evaluated spreadsheet with **non-square tessellating cells** (squares, hexes, triangles, parallelograms, and beyond) and a **switchable formula language per cell** (Excel-lite, Python via PyO3, Rust via Rhai-preview + rustc-native).

**Read `PLAN.md` before doing non-trivial work.** It is the canonical architecture document and supersedes anything in this file if they diverge.

## Languages and stack

This is a **Rust-first** project; the global preference applies here strongly.

- **Rust core** (workspace under `crates/`) owns all evaluation, persistence, and tessellation math. New domain logic goes here, not in TypeScript.
- **Electron + React + TypeScript** under `apps/desktop/` is a thin renderer. It owns layout, drawing, and UI state — not workbook logic. Resist the temptation to put computation in the renderer.
- **Tauri port comes later** (Phase 5+). Don't preemptively avoid Electron-specific APIs; we'll deal with the port when the project earns it.

Formula engines live under `crates/tescellate-formula/src/<engine>/`. Each engine is gated behind a Cargo feature (`python`, `rhai`, `rustnative`) so the project builds without PyO3 / rustc-as-a-library available on a CI machine.

## Working norms

- **Workspace-aware tests.** Run `cargo test -p <crate>` for the crate you changed, not `cargo test` across the whole workspace, unless your change crosses crate boundaries.
- **Formatters/linters** (from global CLAUDE.md): `cargo fmt`, `cargo clippy`, and on the frontend `prettier` + `eslint` (configs to land with Phase 1 — until then, match existing style).
- **No premature abstraction.** The plan lists future tilings and engines, but only build what the current phase needs. Add the trait + first impl; don't pre-implement the third lattice "for symmetry".
- **The Lattice trait is load-bearing.** Changes to `tescellate-tess::Lattice` ripple into every renderer module and every formula engine. Discuss in PR before changing the trait signature.

## What NOT to do without asking

- **Don't add a third execution mode** (e.g., WASM-based Python, an LLM-based formula engine) without an explicit ask. The four engines in PLAN.md §6.2 are the scope.
- **Don't replace the JSON-RPC IPC** with a native Node module / FFI binding. The subprocess split is deliberate (see PLAN.md §2).
- **Don't change the file format** (`.tscl`) shape without bumping `manifest.json` version and adding an upgrade path.
- **Don't merge changes that introduce a new lattice without a `Lattice` impl + parser + renderer + at least one test sheet** — partial lattice support is worse than none.

## Build commands (placeholder — to be filled in as Phase 0 lands)

```bash
# Rust core
cargo build               # workspace
cargo test -p tescellate-core
cargo run --bin tescellate-cli -- --help

# Desktop app
cd apps/desktop
npm install
npm run dev               # vite + electron in dev mode
npm run build             # production build
```

## Running shell commands

The Bash / PowerShell tool starts in the repo root (`C:\Users\charl\Tescellate`) and its working directory persists between calls — **do not prepend `cd`** to commands. A `cd` combined with output redirection (`>`, `2>&1`) in a compound command trips a sandbox guard ("Compound command contains cd with output redirection — manual approval required to prevent path resolution bypass") and forces a needless approval prompt. Dropping the redundant `cd` avoids the guard entirely (it does not weaken it).

- `git`, `gh`, and any repo-root command: run bare — no `cd` prefix.
- `cargo` for the `apps/tescellate-ui` crate (it is its own `[workspace]`): pass `--manifest-path apps/tescellate-ui/Cargo.toml` — e.g. `cargo test --manifest-path apps/tescellate-ui/Cargo.toml` — rather than `cd apps/tescellate-ui && cargo test`.

## Repo identity

- Owner: `crussella0129`
- GitHub: `git@github.com:crussella0129/tescellate.git`
- License: MIT OR Apache-2.0 (dual)
- Default branch: `main`
