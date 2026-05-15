# Tescellate — The All-Rust Path

> Could Tescellate be written entirely in Rust? Yes. Should it? Mostly. When? Mostly Phase 5; the rest is research.

This document is a deliberate one-off. The Carbide language docs in [`docs/carbide/`](carbide/README.md) describe how things are. This doc describes how things *could* be — specifically, the architectural question of pushing Rust further up the stack until everything below Electron itself (and maybe Electron too) is replaced.

It is structured as four "levels of Rust-ness" — `L0` through `L3` — each a coherent stopping point with its own pros, cons, and tooling maturity. The repo is at L0 today; L1 is on the roadmap as Phase 5; L2 and L3 are research territory.

---

## TL;DR

| Level | Stack | Tested | Bundle | Recommendation |
|---|---|---|---|---|
| **L0** (today) | Rust core + Electron main + React/TS renderer | ✅ shipping | ~150 MB | What we have. Stay here for Phases 1–4. |
| **L1** | Rust core + **Tauri** host + React/TS renderer | mature | ~25 MB | Already PLAN.md Phase 5. Lowest-cost, highest-yield migration. |
| **L2** | Rust core + Tauri + **Rust→WASM renderer** | viable | ~25–35 MB | Worth exploring after L1. Significant rewrite, real upside on type sharing. |
| **L3** | Rust core + **native Rust GUI** (no web platform) | possible | ~10–20 MB | Big rewrite; pay the cost only if specific limits force it. |

The line counts give the scale of the work. Today:

- **Rust**: 7,233 lines across the 6 workspace crates. This is the data + compute + language + IPC + persistence layer. **Already 100% Rust.**
- **TypeScript / TSX**: 1,556 lines across `apps/desktop/`. This is the entire non-Rust surface.

So "could we rewrite everything in Rust" is really "could we rewrite ~1,500 lines of TS in Rust." The answer is yes; the interesting question is *which Rust* and *what we give up*.

---

## What's already Rust

Every crate under `crates/` is pure Rust:

| Crate | Lines | What it owns |
|---|---|---|
| `tescellate-core` | ~600 | `Workbook`, `Sheet`, `Cell`, `CellValue`, `Array`, `Env`, `Dag`, the persistence types. |
| `tescellate-tess` | ~200 | The `Lattice` trait and the `SquareLattice` impl. |
| `tescellate-formula` | ~4,200 | The Carbide engine — lexer, Pratt parser, evaluator, `Lambda`, the function registry, ~90 standard-library functions, the `WorkbookEngine` orchestrator, spill rendering. |
| `tescellate-ipc` | ~120 | LSP-style framed JSON-RPC server (just framing — the dispatch is in `cli`). |
| `tescellate-store` | ~200 | The `.tscl` zip-archive reader/writer. |
| `tescellate-cli` | ~250 | The `tescellate-core` binary that Electron spawns. Dispatches JSON-RPC. |

All cell logic, every formula, every value, every coordinate system, every snapshot, every save/load operation — entirely Rust. There is nothing about the *language* or the *spreadsheet model* that isn't already in Rust.

## What's not Rust

Two thin shells:

### Electron main process (`apps/desktop/electron/`)

| File | Lines | What it does |
|---|---|---|
| `main.ts` | ~190 | Spawns the Rust binary, multiplexes JSON-RPC frames, owns the `BrowserWindow`, builds the native menu, wires File→New/Open/Save dialogs. |
| `preload.ts` | ~30 | `contextBridge` exposing a typed `window.tescellate.coreRequest()` to the renderer. |

About 220 lines of Node.js glue. This is the part the Tauri port (L1, below) replaces wholesale.

### Renderer (`apps/desktop/src/`)

| File | Lines | What it does |
|---|---|---|
| `App.tsx` | ~290 | The top-level component: holds activeCell, pickPreview, selectionRange, editing state; wires the wizard, formula bar, grid, keyboard shortcuts. |
| `components/GridCanvas.tsx` | ~290 | The Canvas 2D grid renderer. Draws cells, the active ring, the rectangular selection hull, headers, values. Handles mouseDown/Move/Up for drag-select and onKeyDown for type-to-edit. |
| `components/FormulaBar.tsx` | ~80 | Controlled `<input>` with the engine chip, address chip, spill-source tag. |
| `components/WizardModal.tsx` | ~210 | The new-workbook wizard. Tessellation cards, extent inputs (which round-trip through `formula.eval` for arithmetic), name. |
| `ipc.ts` | ~80 | Typed wrapper over `window.tescellate.coreRequest`. |
| `address.ts` | ~40 | Square `A1`/`AB42` formatting + parsing, mirrored from the Rust side. |
| `types.ts`, `styles.css`, `main.tsx`, `global.d.ts` | ~370 | TypeScript scaffolding, dark-theme CSS, React entry point. |

About 1,360 lines of renderer code. This is the work L2 / L3 would replace.

---

## The four levels

### L0 — Today

```
┌──────────────────────────────────────────────┐
│ Electron (Chromium + Node.js)                │
│  ┌────────────────────────────────────────┐  │
│  │ React renderer (TS, ~1.4k lines)       │  │
│  └──────────────┬─────────────────────────┘  │
│                 │ ipcRenderer.invoke()        │
│  ┌──────────────┴─────────────────────────┐  │
│  │ Electron main (Node.js, ~220 lines)    │  │
│  └──────────────┬─────────────────────────┘  │
└─────────────────┼────────────────────────────┘
                  │ stdio JSON-RPC frames
┌─────────────────┴────────────────────────────┐
│ tescellate-core (Rust binary, ~7.2k lines)   │
└──────────────────────────────────────────────┘
```

- **Pros**: Battle-tested everything. HMR via Vite (sub-second iteration on UI changes). Mature DevTools. CSS, fonts, accessibility, animations all work out of the box.
- **Cons**: Bundle size (~150 MB compressed Electron app). Two language runtimes. Three IPC hops (renderer → main → core binary). Boundary serialization (every cell snapshot round-trips through JSON twice). Two type systems that have to be kept manually in sync (`tescellate-formula::CellSnapshot` mirrored by `ipc.ts::CellSnapshot`).
- **Maintenance cost**: ~10% of every feature change touches both sides of the boundary. Drag-select took two passes mostly because of TS-side event-ordering bugs that Rust would have caught at the type level.

### L1 — Tauri host (PLAN.md Phase 5)

```
┌──────────────────────────────────────────────┐
│ Tauri (Rust shell, ~5 MB)                    │
│  ┌────────────────────────────────────────┐  │
│  │ System WebView (Edge/WebKit/WebKitGTK) │  │
│  │  ┌──────────────────────────────────┐  │  │
│  │  │ React renderer (TS, ~1.4k lines) │  │  │
│  │  └─────────────┬────────────────────┘  │  │
│  └────────────────┼───────────────────────┘  │
│                   │ tauri.invoke('name', …)   │
│  ┌────────────────┴───────────────────────┐  │
│  │ Tauri main (Rust, ~200 lines)          │  │
│  │   + tescellate-core (linked as a lib)  │  │
│  └────────────────────────────────────────┘  │
└──────────────────────────────────────────────┘
```

- **The Electron main process disappears.** The Rust core is *linked into* the Tauri main rather than spawned as a subprocess. `WorkbookEngine` becomes a normal Rust value owned by the Tauri app state.
- **The JSON-RPC stdio layer disappears.** Tauri's `#[tauri::command]` macro generates the renderer↔Rust bridge automatically; the renderer calls `invoke('cell_set', {sheet, address, source})` and a Rust function fires.
- **The renderer is unchanged.** Same React, same Vite HMR, same TS. The only file that meaningfully changes is `ipc.ts` (now wraps `tauri.invoke` instead of `window.tescellate.coreRequest`).
- **Native integration** via Tauri's bundled crates: `muda` for menus, `rfd` (or `tauri-plugin-dialog`) for file dialogs, `tao` for window management. All the things Electron's main process did, done in Rust.
- **Bundle**: ~25 MB instead of ~150 MB. Faster startup (sub-second cold start vs. ~3s for Electron). No bundled Chromium — uses whatever WebView is on the host (Edge on Windows, WebKit on macOS, WebKitGTK on Linux).

**Tooling state: mature.** Tauri 2.x ships, used by production apps. The migration is well-trodden; the Tauri CLI scaffolds an Electron-equivalent project structure.

**Migration effort estimate: 1–2 weeks.** Most of it is testing across the three platforms because system WebViews diverge on edge cases (CSS layouts, canvas behaviour on HiDPI, IME). The Rust changes are mechanical.

**What you give up**: nothing meaningful for a power user, but cross-platform WebView differences become *your* problem instead of Electron's (Electron pins Chromium, Tauri uses what's there). For a spreadsheet — almost no Chrome-specific features used — this is fine.

### L2 — Rust→WASM renderer

```
┌──────────────────────────────────────────────┐
│ Tauri                                        │
│  ┌────────────────────────────────────────┐  │
│  │ System WebView                         │  │
│  │  ┌──────────────────────────────────┐  │  │
│  │  │ Renderer (Rust→WASM, Dioxus/     │  │  │
│  │  │  Leptos/Sycamore, ~1.5k lines)   │  │  │
│  │  └─────────────┬────────────────────┘  │  │
│  └────────────────┼───────────────────────┘  │
│                   │ tauri.invoke() — or       │
│                   │ direct WASM↔native call   │
│  ┌────────────────┴───────────────────────┐  │
│  │ Tauri main + tescellate-core           │  │
│  └────────────────────────────────────────┘  │
└──────────────────────────────────────────────┘
```

- **The renderer becomes Rust.** Frameworks: **Dioxus** (React-flavoured, VDOM), **Leptos** (Solid-flavoured, fine-grained signals), **Sycamore** (Solid-flavoured too), **Yew** (Elm-ish, older).
- **The system WebView stays.** All UI is still DOM under the hood — these frameworks render to the browser DOM via WASM. Canvas drawing for the grid uses the browser's HTML5 Canvas API via `wasm-bindgen` bindings.
- **Type sharing is free.** The renderer imports `CellSnapshot`, `CellValue`, `LatticeKind` directly from `tescellate-core`. No `ipc.ts` mirror. Bugs like the `Number(5.0) vs Integer(5)` distinction can't get lost in JSON serde.
- **CSS still works.** WebView is still rendering DOM; styling is unchanged. The wizard's CSS, the formula bar's flex layout — all still CSS.

**Tooling state: mid.** Dioxus 0.6+ is solidly usable; the dx CLI does hot reload (slower than Vite, but functional — full WASM rebuild is 5–30s vs Vite's <1s). Leptos has the cleanest signal-based reactivity. Both can target Tauri.

**Migration effort estimate: 4–8 weeks.** Every React component needs a port. Most are mechanical (Dioxus's `rsx!` macro is structurally close to JSX), but the canvas drawing in `GridCanvas.tsx` is the trickiest part — accessing `CanvasRenderingContext2D` from Rust/WASM is well-supported but every API call is a `wasm-bindgen` call, which adds friction.

**What you give up**:
- Vite's sub-second HMR. Dioxus's hot reload is functional but feels heavier.
- The vast `npm` ecosystem (date pickers, syntax highlighters, etc.). The Rust→WASM ecosystem covers the basics but has fewer ready-made widgets.
- Browser DevTools are still useful for inspecting the rendered DOM, but stepping into Rust source needs the experimental Chrome DWARF support.

**What you gain**:
- One language end-to-end. Refactoring across the renderer/core boundary works through `cargo`'s type checker.
- Shared validators, formatters, parsers. The `address.ts` file goes away — `tescellate-tess::square::format_a1` is used directly.
- Smaller team friction: the Rust people on the project can fix renderer bugs without context-switching to TypeScript.

### L3 — Native Rust GUI (no web platform)

```
┌──────────────────────────────────────────────┐
│ Native window (winit / tao)                  │
│  ┌────────────────────────────────────────┐  │
│  │ Native Rust GUI:                       │  │
│  │   egui, Slint, Iced, Floem, Xilem, …   │  │
│  │   Drawing: tiny-skia / wgpu / vello    │  │
│  └─────────────────┬──────────────────────┘  │
│                    │ direct fn calls          │
│  ┌─────────────────┴──────────────────────┐  │
│  │ tescellate-core                        │  │
│  └────────────────────────────────────────┘  │
└──────────────────────────────────────────────┘
```

- **The WebView is gone.** No DOM, no CSS, no Chromium/WebKit. Drawing happens directly to a Rust-controlled framebuffer or GPU surface.
- **The renderer is a Rust GUI framework.** Options, with current maturity:

| Framework | Style | Maturity | Notes |
|---|---|---|---|
| **egui** | Immediate-mode | high | Mature, simple. UI is procedural; redraws every frame. Excellent for tool-like apps. The grid renderer is a natural fit. Less idiomatic for the wizard modal. |
| **Slint** | Declarative DSL + Rust | high | Mature, commercial backing. `.slint` files for layout, Rust for logic. Built-in styling system. Closest to "Qt for Rust". |
| **Iced** | Elm-style | mid | Functional, declarative. Good for typical app shells; canvas integration via `iced::widget::canvas`. |
| **Floem** | Signals + composition | mid (rising) | Reactive (fine-grained signals like Leptos). Used by Lapce editor. Promising. |
| **Xilem** | Reactive | early | Linebender's next-gen GUI (the team behind Druid). Bleeding edge. |
| **GPUI** | Reactive + GPU | mid | The Zed editor's framework. Open source 2024. Powerful but tied to Zed's release cadence. |
| **Dioxus Desktop** | React-like + native | mid | Same Dioxus as L2 but rendering through wgpu instead of DOM. Newer; less battle-tested. |
| **gtk-rs / Qt bindings** | Native widget kits | high | Mature widget kits, but bindings to non-Rust code. Less idiomatic Rust. |
| **bevy_ui** | ECS-driven | mid | Game-engine UI. Powerful for visualisation; unusual for productivity apps. |

For the grid drawing specifically, the choice is **tiny-skia** (CPU 2D), **wgpu**-based custom shaders (GPU), or **vello** (GPU-vector, bleeding edge but the future of high-perf 2D in Rust). egui has its own rendering layer built on `epaint` which handles most of this.

- **System integration via standalone crates**: `muda` for menus, `rfd` for file dialogs, `winit` or `tao` for windowing, `accesskit` for accessibility, `cosmic-text` for text shaping (Unicode-correct, BiDi, IME).

- **Bundle**: 10–20 MB. Cold start under 200 ms. No system WebView dependency.

**Tooling state: variable.** egui, Slint, and Iced are production-ready; the rest are in flux. The 2D drawing primitives (tiny-skia, wgpu) are mature.

**Migration effort estimate: 8–16 weeks.** This is roughly a full rewrite of the UI:

- The grid renderer ports straightforwardly to egui/`epaint` or a custom wgpu pass — it's already drawing primitives.
- The wizard, formula bar, menus all need to be re-expressed in the framework's idioms.
- CSS goes away; styling becomes Rust code (egui themes, Slint stylesheets, Iced style trait impls).
- Accessibility, IME support, copy/paste, drag-and-drop — each becomes a project. The web platform makes these free; native Rust GUI makes you do them.

**What you give up**:
- All CSS — every visual style becomes framework-specific Rust.
- Animations — most Rust GUI frameworks have simple timer/easing primitives but nothing like CSS animations.
- Accessibility-as-default. `accesskit` exists and is improving, but you have to wire it up explicitly.
- IME for non-Latin scripts works in webviews automatically. In native Rust GUI it depends on framework support (egui has it; some others are catching up).
- Web-platform conveniences like Markdown rendering (would need a `pulldown-cmark` + custom renderer), syntax highlighting (`syntect`), complex text layout.
- Hot reload during dev (egui's "compile-time" UI is fast to recompile, but you don't get DOM-style "edit-and-see").
- DevTools (each framework has its own inspector; none are as mature as Chrome DevTools).

**What you gain**:
- Single binary, ~15 MB.
- Sub-200ms cold start.
- No process boundaries anywhere — the renderer can directly read `Workbook` state through a `&` or `Arc`.
- Lower memory overhead.
- "Just Rust" — every contributor only needs Rust toolchain.
- For Tescellate specifically: the formula engine, the lattice math, and the grid rendering align naturally with immediate-mode GUI. The wizard and other modal UI are less natural.

---

## Subsystem-by-subsystem mapping

| Subsystem | L0 (today) | L1 (Tauri) | L2 (Tauri + WASM renderer) | L3 (native) |
|---|---|---|---|---|
| `WorkbookEngine` | Rust crate | same | same | same |
| Formula parser / eval | Rust crate | same | same | same |
| `.tscl` save/load | Rust crate | same | same | same |
| Process boundary | stdio JSON-RPC | Tauri command | Tauri command (or WASM↔native via shared types) | none — direct fn call |
| Serialization at boundary | JSON | JSON (Tauri serde) | optional (can use bincode / direct Rust types) | none |
| Renderer state | React `useState` | same | Rust framework state (Dioxus hooks / Leptos signals) | Rust framework state |
| Canvas drawing | HTML5 Canvas 2D | same | HTML5 Canvas via `wasm-bindgen` | tiny-skia / vello / framework primitives |
| Text input (formula bar) | HTML `<input>` | same | HTML `<input>` via Dioxus | framework-specific TextEdit |
| Modal dialogs (wizard) | React + CSS | same | Rust→WASM + CSS | framework Window or modal layer |
| Application menu | Electron Menu API | `muda` (via Tauri) | `muda` | `muda` (standalone) |
| File dialogs | Electron `dialog` | `rfd` | `rfd` | `rfd` |
| Window mgmt | Electron `BrowserWindow` | `tao` | `tao` | `winit` |
| Hot reload | Vite (sub-second) | Vite | Dioxus `dx serve` (5–30 s) | framework-specific or none |
| Bundle size | ~150 MB | ~25 MB | ~30 MB | ~15 MB |
| Cold-start time | ~3 s | ~1 s | ~1 s | <0.2 s |
| Type sharing across boundary | manual (TS mirror of Rust types) | manual still | automatic | n/a — no boundary |
| Accessibility | browser-default | browser-default | browser-default | `accesskit`, per-framework |
| Languages in dev | Rust + TS | Rust + TS | Rust only | Rust only |

---

## Challenges (the honest list)

Things that look easy on paper and aren't:

### Renderer parity

A 1,400-line React renderer takes more than 1,400 lines of Rust to replicate. The web platform provides a lot for free: text input handling (IME, accents, paste), clipboard, drag-and-drop, focus management, accessibility tree, ARIA, RTL layout. Every native Rust GUI framework reimplements a subset; none reimplements all.

### Drag-select / event coordination

The drag-select that PR #1 shipped — `mouseDown` on canvas, window-level `mouseMove`/`mouseUp` so the drag tracks outside the canvas, blur-on-canvas-click race detection in the formula bar — is the kind of thing the DOM event model + React's synthetic events make tractable. Native Rust GUIs typically funnel all events through one queue; coordinating "this event is for the canvas but only because focus is on the input" needs explicit state machines.

### Hot reload

Vite gives sub-second iteration. Dioxus's `dx serve` is workable but slower (the WASM crate rebuilds). Native Rust GUIs depend on the framework: egui apps recompile in 2–10s for incremental changes; "edit-and-see" workflows are weaker.

### Test infrastructure

The renderer has no UI tests today (this is a known gap regardless of level). Whichever level we end at, we'd want to add headless tests. The web platform has Playwright / Vitest / Puppeteer; the Rust GUI ecosystem has thinner equivalents (`egui_kittest`, framework-specific harnesses).

### Plugin / extension story

If Tescellate ever grows a "user-installable extensions" feature (color themes, custom formulas, embedded charts), the web platform has obvious answers — JS bundles, declarative manifests, sandboxed iframes. The native-Rust path would need a dynamic-loading scheme via `libloading` or a WASM plugin host (the Phase 4 rustc-native engine already needs this for compiled formulas; the same machinery could host UI plugins). It's tractable but more work.

### Cross-platform polish

Web tech papers over platform differences; native Rust GUI exposes them. macOS expects native menubar at the screen top, not the window; Windows expects Mica blur for modern apps; Linux varies wildly by compositor. Each framework handles this differently and incompletely.

### Engine boundaries

The Phase 2 PyO3 engine and the Phase 4 rustc-native engine both already live in the Rust core. They benefit from L1+ trivially — no JSON marshaling of cell ranges. At L3, they could potentially share a memory page with the renderer for zero-copy NumPy array rendering. This is the most compelling specific argument for going past L1.

---

## Schema — what the migration actually looks like

### L0 → L1 (Tauri)

This is roughly the work PLAN.md Phase 5 already commits to.

1. `cargo new --bin apps/desktop-tauri` (or similar). Pull in `tauri`, `tauri-build`, `tauri-plugin-dialog`, `tauri-plugin-shell` if needed.
2. Add `tescellate-core`, `tescellate-formula`, `tescellate-store` as path dependencies. `WorkbookEngine` becomes managed Tauri state via `app.manage(...)`.
3. Translate every JSON-RPC method to a `#[tauri::command] fn cell_set(state: State<...>, ...) -> Result<..., String>`. The dispatcher in `tescellate-cli/src/main.rs` becomes a set of command functions; the body of each is mostly unchanged.
4. Replace `electron/main.ts` with `apps/desktop-tauri/src-tauri/src/main.rs`. The menu builder (~80 lines of TS) becomes ~80 lines of Rust via `tauri::Menu::new()` and `muda`.
5. The renderer's `ipc.ts` swaps `window.tescellate.coreRequest(payload)` for `@tauri-apps/api`'s `invoke('cell_set', payload)`. Every call site stays the same shape.
6. Bundle config: `tauri.conf.json` instead of `electron-vite.config.ts` for window setup, menu, file associations. Vite still drives the renderer build.
7. Delete `apps/desktop/electron/` and the Electron deps from `apps/desktop/package.json`. The renderer build is unchanged.

The `tescellate-cli` binary can stay around for headless / CLI use (the smoke tests). It just stops being the Electron child process.

**Risk surface**: WebView differences across platforms. Practical mitigation: target Edge WebView2 on Windows (Chromium-equivalent), WebKit on macOS (well-understood), WebKitGTK on Linux (where it gets hairy — older distros ship old WebKitGTK; document a min-version requirement).

### L1 → L2 (Rust→WASM renderer)

1. Pick a framework. Recommend **Dioxus** for "I want React but Rust" or **Leptos** for "I want fine-grained reactivity." Both have Tauri integration.
2. New crate `crates/tescellate-renderer-wasm` (or just `apps/desktop-renderer`). `cdylib` target, depends on the framework.
3. Port `App.tsx` → `App.rs`. State migrates from `useState` to framework primitives (Dioxus `use_signal`, Leptos `RwSignal`).
4. Port `GridCanvas.tsx` → `GridCanvas.rs`. The Canvas 2D drawing code is ~95% mechanical translation: `ctx.fillRect(x, y, w, h)` becomes `ctx.fill_rect(x, y, w, h)` via `web-sys`. The DPR scaling, the helper `cellRect(c)`, the cursor highlight all port directly.
5. Port `FormulaBar.tsx` → `FormulaBar.rs`. The controlled `<input>` is a Dioxus / Leptos input with bound state. Caret manipulation via the same `selectionStart`/`setSelectionRange` browser APIs through `web-sys`.
6. Port `WizardModal.tsx` → `WizardModal.rs`. CSS stays in `.css` files; the framework just emits class names.
7. `ipc.ts` deletes itself. The renderer imports `tescellate_core::{CellSnapshot, CellValue, ...}` directly from a path-dependency. `invoke` calls still go through Tauri's bridge, but now with `serde`-derived types on both sides — type safety end-to-end.

The Dioxus path keeps the React mental model. The Leptos path is a paradigm shift (fine-grained signals, no VDOM diff); cleaner long-term but a learning curve.

**Risk surface**: WASM build times, framework maturity, ecosystem gaps for specific widgets. Practical mitigation: do this AFTER L1 lands and you've absorbed the Tauri migration; build a feature spike (port `GridCanvas` alone) before committing.

### L2 → L3 (native Rust GUI, no WebView)

This is the rewrite. There is no incremental path — the rendering layer fundamentally changes.

1. Pick a framework. For Tescellate specifically, **egui** is the natural fit for the grid (immediate-mode, simple, the canvas already redraws-on-every-frame style). **Slint** is the most "Excel-looking" if visual fidelity to existing UI is the goal. **Floem** or **Xilem** if you want to be on the front of the Rust GUI wave.
2. New crate, `cdylib` or `bin` target. Drop Tauri; keep `tescellate-core`/`tescellate-formula`/`tescellate-store` as path deps.
3. Reimplement `GridCanvas` in the framework's drawing primitives. egui: build a `Painter` and call `painter.rect_filled(...)`, `painter.text(...)`. The cell helpers from `address.ts` already live in Rust (`square::format_a1`); use them.
4. Reimplement `FormulaBar` with the framework's TextEdit widget. Caret manipulation, IME, copy/paste all framework-provided (egui has solid versions; others vary).
5. Reimplement the wizard. Modal windows in egui are `egui::Window::new(...).open(&mut open)`; in Slint they're `Dialog { ... }`.
6. Reimplement the application menu via `muda`. macOS, Windows, Linux versions all work.
7. Reimplement file dialogs via `rfd`. Same calls as Tauri's plugin.
8. Choose a 2D drawing backend if the framework doesn't bundle one — `tiny-skia` for CPU, `vello`/`wgpu` for GPU.
9. Wire up `accesskit` for screen-reader support. (egui has an `accesskit` integration; Slint has its own.)
10. Build the test harness from scratch (or use `egui_kittest`).

**Risk surface**: time. This is 2–4 months of one engineer's focused work.

---

## When does each level make sense?

| If your concern is | Pick |
|---|---|
| Iteration speed today | L0 (stay). |
| Bundle size, startup time, "feels lean" | L1. |
| Refactoring across the renderer/core boundary | L2. |
| One language, fewer moving parts | L2 or L3. |
| Maximum performance / no WebView / single binary | L3. |
| Plugin ecosystem with web compatibility | L0 / L1 (web platform helps). |
| Power-user spreadsheet UX (charts, custom widgets) | L0 / L1 / L2 (web ecosystem). |
| Embedded / kiosk deployment, no JS runtime | L3. |
| You really like Rust and want to scratch that itch | L3, but only after L1. |

---

## Where this leaves PLAN.md

PLAN.md §11 Phase 5 already says "Tauri port + multi-user collaboration." That's the L1 step. **No change needed** to the existing roadmap to enact this document.

L2 would slot in as **Phase 5.5** — between Tauri (Phase 5) and the rustc native engine (Phase 4 chronologically but the engine work is independent of UI). L3 would be **Phase 10+**, considered only if specific limits (sub-100ms cold start, embedded deployment, plugin sandbox issues) force the move.

The recommendation: **commit to L1 (Phase 5), spike L2 if Phase 5 lands cleanly and the team has appetite, defer L3 until there's a concrete reason.**

The reason the answer isn't "yes, do L3 now" is *not* that it's technically impossible. It is. The reason is that the web-platform UI we have today is doing real work — drag-select, IME, CSS-styled modals, hot-reload — that we'd be paying to reimplement. The marginal value of "one language all the way" doesn't beat the marginal cost of "and we'd have to write our own font shaper" today.

The reason the answer also isn't "no, stay at L0 forever" is that L1's bundle-size and startup-time wins are large, the migration is well-trodden, and PLAN.md already commits to it. L1 is happening.

L2 is the genuinely interesting research question. It's where end-to-end type sharing across the renderer/core boundary unlocks a meaningfully different development experience. Worth a spike.

L3 is fine. Just not yet.

---

## References

- **Tauri**: https://tauri.app/ — Rust shell for web-tech renderers.
- **Dioxus**: https://dioxuslabs.com/ — Rust→WASM (React-style) for web and native.
- **Leptos**: https://leptos.dev/ — Rust→WASM (Solid-style fine-grained signals).
- **egui**: https://www.egui.rs/ — immediate-mode Rust GUI.
- **Slint**: https://slint.dev/ — declarative GUI for Rust.
- **Iced**: https://iced.rs/ — Elm-style Rust GUI.
- **Floem**: https://github.com/lapce/floem — signals-based, used by Lapce.
- **muda**: https://github.com/tauri-apps/muda — cross-platform menus.
- **rfd**: https://github.com/PolyMeilex/rfd — cross-platform file dialogs.
- **tiny-skia**, **wgpu**, **vello** — 2D drawing in Rust.
- **accesskit**: https://accesskit.dev/ — accessibility tree for native apps.
- **cosmic-text**: https://github.com/pop-os/cosmic-text — Unicode-correct text shaping.
