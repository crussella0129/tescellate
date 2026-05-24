# Carbide — The All-Rust Roadmap

> Goal: a single all-Rust application — unique in what it is — where the Carbide
> formula language **transpiles to Rust** rather than only being interpreted.

This is the **execution roadmap** for that goal. It is a companion to two existing docs:

- [`PLAN.md`](../PLAN.md) — the canonical product architecture (what Carbide *is*).
- [`docs/rust-native.md`](rust-native.md) — the architectural analysis of *how Rust* the
  stack could be (the L0→L3 levels).

`rust-native.md` answers "could it be all Rust?" (yes). This document answers
"**in what order do we get there, and how is each step proven correct?**"

It is written to be executed iteratively: one version at a time, each version
**plan → build → test**, with testing the heaviest part of every cycle.

---

## The goal, decomposed

"An all-Rust application that is unique, and has the Carbide language transpile to it"
is three commitments:

1. **All-Rust** — drive the stack from L0 (Electron host + TypeScript renderer) toward
   L3 (native Rust GUI, no WebView), per `rust-native.md`.
2. **Unique** — Carbide's identity is non-square tessellating cells + a switchable,
   *compilable* per-cell formula language. Everything below serves that identity; none
   of it dilutes it.
3. **Carbide transpiles** — Carbide is interpreted today (Pratt parser → AST →
   tree-walking `eval` in `crates/carbide-formula/src/excellite/`). Add a backend
   that lowers Carbide → **Rust source**, compiled natively. The interpreter stays as
   the instant-feedback *preview* tier; the transpiler is the *native* tier — the exact
   two-tier model `PLAN.md` §6.2 already describes for the Rust engine, now applied to
   Carbide itself.

---

## Two tracks

| Track | What | Where it lives |
|---|---|---|
| **B — Carbide transpiler** | Carbide AST → Rust source → `rustc` → cached `cdylib` | `crates/carbide-formula` (new `transpile` module, feature-gated) |
| **A — UI rustification** | L0 → L1 (Tauri) → L2 (Rust/WASM renderer) → L3 (native Rust GUI) | `apps/` |

**Track B goes first.** Two reasons:

1. The global `CLAUDE.md` is explicit that Electron-first is a deliberate choice and the
   Tauri port is a *later* optimization, taken once the app is functionally mature.
   Track A is not yet "earned"; rushing it would contradict a standing preference.
2. Track B is pure Rust-core work — it never touches the UI stack — and it is
   exceptionally testable (see [Testing](#testing-strategy--the-interpreter-is-the-oracle)).

Track A begins at **v7**, after the engine is mature.

### Relationship to `PLAN.md` Phase 4

`PLAN.md` §6.2 Phase 4 specs a `rustnative` engine that compiles a *Rust-syntax* formula
language (Rhai-preview + `rustc`-native). This roadmap's transpiler is sharper: it
compiles **Carbide itself** — the default language every workbook already uses — so the
native tier benefits *every* existing formula, not only ones rewritten in Rust syntax.
Track B absorbs and supersedes Phase 4's engine work; the `rustc` + `libloading` + cache
+ trust-manifest *machinery* Phase 4 describes is reused verbatim (see v5).

---

## Working method

Every version is one full cycle:

1. **Plan** — a short written scope at the top of the version's PR description: the AST
   surface it covers, the new public types, the test cases it must pass.
2. **Build** — the implementation, on a feature branch, in coherent commits.
3. **Test** — the heaviest part. Two gates, both required before merge:
   - **CI gate** — `.github/workflows/ci.yml`: `cargo build/test/fmt/clippy` across the
     workspace + `npm` typecheck/build for the renderer. Green on every supported OS.
   - **Behavior gate** — *actual* behavior, not just "the code compiles": for Track B,
     the differential corpus (below); for Track A, a running-app smoke test.

A version does not merge until both gates pass. Versions are not calendar-bound.

---

## Version sequence

| Version | Title | Track | Gate |
|---|---|---|---|
| **v0** | Roadmap + CI pipeline | — | CI green on `main` |
| **v1** | Transpiler skeleton + arithmetic core | B | differential: literals + operators |
| **v2** | Transpiler: references, ranges, arrays | B | differential: refs/ranges/arrays |
| **v3** | Transpiler: the function stdlib | B | differential: full function corpus |
| **v4** | Transpiler: LET / LAMBDA / LETREC + higher-order | B | differential: 28-case lambda gamut |
| **v5** | Native compile pipeline + trust | B | end-to-end: workbook runs the compiled tier |
| **v6** | Transpiler hardening + perf | B | differential corpus 100% + benchmark |
| **v7** | L1 — Tauri host | A | smoke: app runs, all IPC paths work |
| **v8** | L2 — Rust/WASM renderer | A | smoke: feature parity with the TS renderer |
| **v9** | L3 — native Rust GUI; convergence | A | smoke: single all-Rust binary, Carbide compiled |

> **Execution note (post-v6).** v0–v6 — Track B, the Carbide transpiler — shipped as
> planned. The loop then continued on engine work it can verify headlessly: wiring the
> native tier into `WorkbookEngine` (the deferred v5 follow-on), the PyO3 Python engine,
> type-specialized transpiler codegen with transpile-time constant folding (~2x on
> constant-bearing formulas), then generative differential testing — a seeded random
> Carbide-AST fuzzer (which immediately caught an operand-evaluation-order bug in the
> v9 codegen), extended to the higher-order `LET`/`LAMBDA`/`MAP`/`REDUCE` surface, then
> property-based testing of the DAG recompute engine (incremental recompute proven
> order-independent on random workbooks, cycle detection hardened), of the
> tessellation lattices (geometric invariants fuzzed across square and hex), and of
> `.crbd` persistence (save/open round-trip fidelity across random workbooks), capped
> by an end-to-end smoke test driving the real `carbide-core` binary over its
> JSON-RPC stdio protocol, scale/stress tests that surfaced and fixed an
> unbounded-recursion crash on pathologically deep formulas, and a cross-engine
> differential proving the Carbide and Python formula engines agree. v18 then wrote
> the Carbide language reference (`docs/carbide/reference.md`) with every example
> CI-verified. All taken ahead of Track A. The UI rustification (v7–v9 above) is still
> the plan, but its *actual-behavior* gate needs a running GUI, so it is deferred to a
> session where that can be driven and verified interactively.

> **Execution note (front-end rebuild, post-v18).** A physical test of the Electron app
> confirmed the engine works but the front-end was a rough prototype. The decision:
> rather than polish the Electron renderer or take the Tauri (L1) step, rebuild the
> front-end directly as a pure-Rust application — `apps/carbide-ui`, an egui/eframe
> app compiled to WebAssembly. This supersedes Track A (v7–v9) and `rust-native.md`'s
> "L1 then spike L2" recommendation: egui builds the *same* codebase to both WASM and
> native, so 100% Rust is reached now, without translation debt. A new self-paced loop
> drives it in numbered versions; v1 scaffolded the crate — eframe and the whole engine
> (`carbide-core`/`tess`/`store`/`formula`) compile to `wasm32-unknown-unknown` —
> with a working grid that renders engine-computed cell values.

### v0 — Roadmap + CI pipeline *(this version)*

- This document.
- `.github/workflows/ci.yml` — the repo's first CI. `lint` (fmt + clippy), `test`
  (build + test, Ubuntu **and** Windows), `frontend` (typecheck + build).
- **Done when:** CI is green on `main`.

### v1 — Transpiler skeleton + arithmetic core

- New module `crates/carbide-formula/src/transpile/` — always compiled (lightweight,
  pure-Rust codegen with no external-toolchain needs, so CI covers it by default).
  Transpiled code and the interpreter share the value-level primitives
  (`apply_binary_op` / `apply_unary_op`), so they are equivalent by construction.
  Feature-gating waits for v5's runtime compile pipeline.
- Lower `Expr::{Number, Str, Bool, Unary, Binary}` → a Rust expression string.
- The **differential harness**: collect a batch of Carbide formulas, generate one
  micro-crate that exposes each as a function, compile it once, run all, and assert each
  result equals the tree-walking interpreter's result.
- **Done when:** every arithmetic/literal formula transpiles, compiles, and runs equal
  to the interpreter.

### v2 — Transpiler: references, ranges, arrays

- Lower `Expr::{CellRef, Range, Array}`.
- Define the **runtime ABI** the transpiled code calls — how compiled Carbide reads
  cells/ranges from an `EvalCtx`-shaped handle. This is the load-bearing interface
  decision of Track B; design it once, here.
- **Done when:** differential-equal on ref/range/array formulas.

### v3 — Transpiler: the function stdlib

- Lower `Expr::Call(name, args)`. Transpiled code calls a runtime support library;
  prefer reusing the existing `excellite::funcs::` implementations directly over
  re-implementing ~90 functions.
- **Done when:** the existing function-level test corpus passes through the transpiler
  differentially.

### v4 — Transpiler: LET / LAMBDA / LETREC + higher-order

- Lower `Expr::{Var, Apply}`, the `LET`/`LAMBDA`/`LETREC` forms, and MAP/REDUCE/SCAN/
  BYROW/BYCOL/MAKEARRAY. Carbide closures → Rust closures or generated functions;
  recursion via the LETREC placeholder-patch pattern (see `Carbide 1.7` design).
- **Done when:** the 28-case lambda/stats gamut is differential-equal.

### v5 — Native compile pipeline + trust

- Wire transpiled Rust → `cargo`/`rustc` → cached `cdylib` → `libloading`, with the
  per-workbook trust manifest. This is `PLAN.md` Phase 4's machinery, reused.
- Expose a **compiled-Carbide execution tier**: a cell can be promoted from interpreted
  (preview) to compiled (native); workbook setting for auto-promote-on-idle.
- Exercise the compile path on **both** Windows and Linux — `rustc` invocation, dynamic
  library extension, and cache keys all diverge per-OS.
- **Done when:** a real workbook of Carbide formulas runs end-to-end through the
  compiled tier with interpreter-equal results, and cache hit/miss behaves correctly.

### v6 — Transpiler hardening + perf

- Error parity (`#DIV/0!`, `#VALUE!`, `#NUM!`, …), volatile functions, numeric edge
  cases. The compiled tier must reproduce the interpreter's *errors*, not only its
  successes.
- Benchmark compiled vs interpreted — quantify the payoff.
- **Done when:** the entire differential corpus is 100% green through the compiled tier
  and a benchmark is recorded.

### v7–v9 — UI rustification *(Track A — sketched; detailed when reached)*

Follow the `rust-native.md` migration schema: **v7** L0→L1 (Tauri host, renderer
unchanged), **v8** L1→L2 (Rust/WASM renderer via Dioxus or Leptos — all first-party code
becomes Rust), **v9** L2→L3 (native Rust GUI via egui — no WebView). v9 is the
convergence point: a single all-Rust binary in which Carbide transpiles to Rust and runs
compiled.

---

## Testing strategy — the interpreter is the oracle

The tree-walking interpreter is mature and well-tested (145+ workspace tests). Track B's
correctness is therefore defined *relative to it*:

> For every formula in the test corpus, the transpiled-and-compiled result must equal
> the interpreted result — value, type, and error.

This **differential test** is the behavior gate for every Track B version. Its corpus is
free: the existing `excellite` tests are reused as differential cases, and each new
version widens the AST surface the corpus exercises. The harness compiles one crate per
run (not one per formula) to keep it fast enough for CI.

Three layers of testing, all required:

1. **CI** — `cargo build/test/fmt/clippy` + renderer typecheck/build, every push and PR,
   on every supported OS.
2. **Differential behavior** — transpiled output vs. interpreter output, the Track B
   behavior gate.
3. **Smoke** — for Track A and v5+, a real run: drive the CLI / the app with concrete
   workbooks and confirm observed behavior, not just a passing unit test. (Unit tests
   verify code; smoke tests verify the *feature*.)

---

## Definition of done

The north star is reached when **all** hold:

- The application is a single Rust binary — no Electron, no Node, no TypeScript (L3).
- Carbide formulas transpile to Rust and execute through the compiled tier; the
  interpreter remains only as the instant-preview tier.
- The differential corpus is 100% green: compiled Carbide is provably equivalent to
  interpreted Carbide.
- Tessellating cells and the switchable formula language — the things that make
  Carbide *unique* — are intact and unbroken.
