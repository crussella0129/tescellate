# Carbide

**Carbide** is the formula language of [Tescellate](../../README.md) — a DAG-evaluated spreadsheet whose cells aren't stuck being squares. It looks like Excel from across the room: write `=SUM(A1:A10)` and it does what you'd expect. Up close it's something more — a small, lexically-scoped functional language with first-class lambdas, recursive bindings, dynamic arrays, and a roadmap that explicitly anticipates non-rectangular cell tilings.

This document set is the **foundation** of Carbide's specification: it pins down what the language is *today*, the way the way the spec is going to grow as new tessellations land, and the contract between the language and the formula engines (Excel-lite, Python, Rust) that implement it. It is written before the next major architecture move — non-rectangular cell tilings (hex, triangle, Voronoi, drawn) — so that those tilings can be designed against a stable language baseline rather than chasing a moving target.

## Where to start

The pages are ordered from most-load-bearing to most-speculative:

| | | |
|---|---|---|
| 1. | [**grammar.md**](grammar.md) | Lexer tokens, the Pratt parser, AST nodes, precedence, the `=` literal-vs-formula rule. |
| 2. | [**types.md**](types.md) | `CellValue` variants, the `Array` shape, `Function` and lambda semantics, type coercion rules, spill, errors, the lexical environment. |
| 3. | [**functions.md**](functions.md) | The ~90-function standard library, grouped by category, each with signature, returns, edge cases, and a runnable example sourced from the test gamut. |
| 4. | [**addressing.md**](addressing.md) | The cell-address syntax. Today only square `A1` / `AB42`; sketches for hex (axial), triangle, parallelogram, Voronoi, and drawn tilings. The page that closes the loop with the upcoming non-rectangular tessellation work. |
| 5. | [**interop.md**](interop.md) | The engine boundary. How Excel-lite, the planned PyO3 Python engine, and the planned Rhai/rustc Rust engine all see the same `CellValue` / `Array` / `Env` / lattice address. Where coordinate schemes leak through the engine boundary and how to keep that leakage tractable. |
| 6. | [**tessellations.md**](tessellations.md) | The deep dive on cell-shape tessellations: the regular/irregular family split, the 11 Archimedean tilings, Voronoi mode (seed distributions, addressing under floating-point drift, range-semantics breakdown), draw-and-validate-a-shape, and the aperiodic territory — Penrose tilings and the einstein hat / spectre tiles (Smith et al. 2023). Implementation cross-cuts (coordinate stability, neighbor enumeration, spatial indexing, persistence) and UX cross-cuts (wizard, drag-select, formula authoring).

## Design principles

These are non-negotiable; they motivate everything that follows.

- **One value type, runtime-tagged.** Carbide is dynamically typed. Every cell holds a `CellValue`. A single dispatch table per engine; no static type checker; users coming from Excel/Sheets see no surprises.
- **`=` is the discriminator.** Input starting with `=` is parsed as a formula. Anything else is a literal — number, boolean, or text — stored as the user typed it. The "your text became `#NAME?`" footgun does not exist here.
- **Lexical scope.** `LET`, `LAMBDA`, and `LETREC` introduce nested `Env` scopes. Lambdas capture their defining environment, *not* their call site. This matches every functional language readers may know and is the only sane semantics for closures over cell ranges.
- **Lattice-agnostic AST.** Cell references and ranges carry the address as an opaque string at the AST level; the lattice resolves them at eval time. The parser doesn't need to know whether `A1` lives on a square or hex sheet — only the lattice does. This is what makes [addressing.md](addressing.md) easy to extend.
- **Each engine owns its compiled artefact.** `CompiledFormula` is per-engine; only that engine can evaluate it. Cross-engine traffic happens at the `CellValue` boundary, not the AST boundary. A Python cell referenced by an Excel-lite cell sees the Python cell's `CellValue` result, not its AST.
- **Functions are values.** A `CellValue::Function` carries an `Arc<dyn CarbideFn>`. The concrete Lambda lives in the formula crate; the core only knows it has a callable thing with a debug label and an `as_any()` downcast. This is the seam that lets Python lambdas, native-compiled Rust lambdas, and Carbide-lite lambdas all coexist someday.

## Status as of this writing

Carbide currently runs through one engine, `excellite` (a hand-written Pratt parser + tree-walking evaluator), against one lattice (square). All the documents in this set describe that reality faithfully and call out what will change when:

- **Phase 2** lands the hex lattice and the PyO3 Python engine.
- **Phase 3** lands triangle and parallelogram lattices and the Rhai sandbox.
- **Phase 4** lands the rustc native-compile engine.
- **Phases 6–8** land the Archimedean configurator, Voronoi, and the draw-a-shape-and-validate tilings.

The roadmap is in the top-level [`PLAN.md`](../../PLAN.md); the docs here are the language-level companion to that file.

## Quick taste

If you've spent five minutes in Excel or Sheets you already know most of Carbide:

```excel
=10
="Hello, world"
=A1 + A2
=IF(B1 > 0, "positive", "non-positive")
=SUM(A1:A10)
=SPLIT("~", "Red~Orange~Yellow")
=UNIQUE(A1:A100)
```

The new bits — what this documentation set is here to define — are what bindings and anonymous functions add on top:

```excel
=LET(x, 10, x + 5)                                              → 15
=LET(f, LAMBDA(x, x*2), f(7))                                   → 14
=MAP(A1:A10, LAMBDA(x, x*x))                                    → squares of A1..A10
=REDUCE(0, A1:A10, LAMBDA(acc, x, acc + x))                     → sum of A1..A10
=LETREC(fact, LAMBDA(n, IF(n<=1, 1, n*fact(n-1))), fact(6))     → 720
=BYROW(A1:E10, LAMBDA(row, SUM(row)))                           → column of row-sums
=MAKEARRAY(5, 5, LAMBDA(r, c, IF(r=c, 1, 0)))                   → 5×5 identity matrix
```

And the analytical operations a data scientist actually wants:

```excel
=LET(
  mean, AVERAGE(A1:A10),
  sd,   STDEV(A1:A10),
  MAP(A1:A10, LAMBDA(x, (x - mean) / sd)))                      → z-scores
=SLOPE(B1:B10, A1:A10)                                          → OLS slope
=CORREL(A1:A10, B1:B10)                                         → Pearson r
=PERCENTILE(A1:A10, 0.5)                                        ≡ MEDIAN(A1:A10)
```

Everything you see above is a runnable test case in `crates/tescellate-formula/src/excellite/funcs/lambda_funcs.rs#tests`. The pattern is intentional: the language's spec and its test suite share the same examples.
