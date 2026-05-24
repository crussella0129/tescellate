# Carbide — Engine Interop

Carbide is the *language*. The thing that parses and executes it is an *engine*. Today one engine ships — `excellite` — and it implements every feature in the rest of this docs set. Two more engines are on the roadmap:

- **Python engine** (PyO3, Phase 2). Lets users write Python in a cell; cell formulas like `=py: import statistics; statistics.mean(ctx.range("A1:A10"))` would route through it. The exact surface is TBD when Phase 2 starts.
- **Rust engine** (Phase 4). Two-tier: Rhai for fast preview, rustc-compiled `cdylib` for production. Promotes the same source.

This page documents the contract every engine shares — what they have to do, what they get for free, and where coordinate schemes leak through the boundary. It's the single page that has to be right *before* non-rectangular tilings land, because cross-lattice + cross-engine interaction multiplies surface area fast.

Source files:

- Engine trait: `crates/carbide-formula/src/lib.rs` — `pub trait FormulaEngine`, `pub trait EvalCtx`, `pub trait CarbideFn`.
- Engine orchestrator: `crates/carbide-formula/src/engine.rs` — `WorkbookEngine` owns the engines registry, the DAG, the spill machinery.

## The four boundaries

Every Carbide engine sits at the intersection of four boundaries. Get any of them wrong and the system stops composing.

```text
                 ┌──────────────────────────────────┐
                 │ user formula:  "=py: mean(A1:A10)"  ← AST boundary
                 └────────────────┬─────────────────┘
                                  │
              ┌───────────────────▼────────────────────┐
              │ engine.parse(src)   ←─ produces a       │
              │  CompiledFormula::<EngineVariant>(…)    │  ← per-engine artefact boundary
              └───────────────────┬────────────────────┘
                                  │
                                  ▼
              ┌─────────────────────────────────────────┐
              │ engine.eval(compiled, &dyn EvalCtx)     │  ← evaluation boundary
              │   reads ctx.cell / ctx.range /          │
              │     ctx.var / ctx.env                   │
              │   returns CellValue                     │  ← value boundary
              └─────────────────────────────────────────┘
```

1. **AST boundary** — what the orchestrator passes to the engine. Per-engine; engines may have different ASTs.
2. **Compiled-artefact boundary** — `CompiledFormula` is the engine-opaque payload stored alongside the cell.
3. **Evaluation boundary** — every engine sees the same `EvalCtx` trait (`cell`, `range`, `var`, `env`).
4. **Value boundary** — every engine produces `CellValue`s and consumes them.

The first two are *per-engine*. The last two are *shared*. Cross-engine traffic happens entirely through `CellValue` and `EvalCtx`; no engine sees another engine's AST or compiled form.

## The engine trait

```rust
pub trait FormulaEngine: Send + Sync {
    fn kind(&self) -> EngineKind;
    fn parse(&self, src: &str) -> Result<CompiledFormula, ParseError>;
    fn refs(&self, compiled: &CompiledFormula) -> Vec<(String, Option<String>)>;
    fn eval(
        &self,
        compiled: &CompiledFormula,
        ctx: &dyn EvalCtx,
    ) -> Result<CellValue, EvalError>;
}
```

- `kind()` — the `EngineKind` enum (`ExcelLite`, `Python`, `Rhai`, `RustNative`). Stored on every cell whose engine isn't the workbook default.
- `parse(src)` — engine-specific lex+parse. Returns `CompiledFormula` (which is an enum with one variant per engine; only that engine can interpret its variant).
- `refs(compiled)` — every `(addr, Option<end>)` pair the formula reads. The orchestrator uses this to populate the DAG *before* evaluation. **This is the only meta-introspection the orchestrator needs**: it doesn't need to understand the engine's AST, only what cells the formula reads.
- `eval(compiled, ctx)` — produce a `CellValue`. Sees the workbook through `EvalCtx`.

`EngineKind` is a closed enum today (4 variants). When a future "engine" arrives that doesn't fit, this will need to grow; engines are not pluggable from outside the crate.

## The shared boundaries

### `EvalCtx`

```rust
pub trait EvalCtx {
    fn cell(&self, addr: &str) -> Result<CellValue, EvalError>;
    fn range(&self, start: &str, end: &str) -> Result<Vec<CellValue>, EvalError>;
    fn var(&self, _name: &str) -> Option<CellValue> { None }
    fn env(&self) -> Option<Arc<Env>> { None }
}
```

- `cell(addr)` — read a cell by its lattice-canonical address. Returns `CellValue::Empty` for cells that don't exist; `EvalError::Ref` is reserved for genuinely malformed addresses (e.g., addressing a cell whose lattice can't parse the string).
- `range(start, end)` — enumerate every cell in the range, row-major. Order matters; `JOIN`, `SCAN`, `BYROW`, etc. rely on it.
- `var(name)` — lexical-variable lookup. Defaults to `None`; only `ScopedCtx` overrides.
- `env(&self)` — the current lexical environment, if any. `LAMBDA` captures this at definition time.

The default-`None` `var`/`env` design means every existing `EvalCtx` impl (including the workbook's `SheetEvalView`) keeps compiling when scope-introducing constructs are added.

### `CellValue`

The value boundary. See [types.md](types.md) for the full variant list. Two things matter for engines specifically:

- **Every engine must produce `CellValue`s.** A Python cell whose body returns a `numpy.ndarray` must marshal to `CellValue::Array(…)` at the engine boundary.
- **Every engine must consume `CellValue`s.** A Python formula that reads `ctx.range("A1:A10")` gets a `Vec<CellValue>`; the engine wraps that into a numpy/pandas shape on the Python side.

`CellValue::Function(Arc<dyn CarbideFn>)` is the trickiest variant for cross-engine traffic. See [Cross-engine functions](#cross-engine-functions).

### `CarbideFn`

```rust
pub trait CarbideFn: Send + Sync + Any + Debug {
    fn as_any(&self) -> &dyn Any;
    fn debug_label(&self) -> String;
}
```

The opaque-callable trait. Every concrete lambda type (the current `excellite::lambda::Lambda`, future `python::PyLambda`, future `rust_native::CompiledFn`) implements it. Engines that *receive* a `Function` value downcast via `as_any` to a concrete type they understand; on miss, they error with `#VALUE!`.

This is the seam that lets `MAP(arr, LAMBDA(...))` work today and lets `MAP(arr, py:lambda x: x*2)` work in Phase 2 *without* the language having to acknowledge Python at the AST level.

## Cross-engine functions

In Phase 1.7 we have one engine, so this is conceptual today. It will be real in Phase 2.

When `MAP` (an excellite `FuncImpl`) sees a `CellValue::Function(arc)`, it does:

```rust
let lambda = arc.as_any().downcast_ref::<excellite::lambda::Lambda>()
    .ok_or_else(|| EvalError::Value("MAP: function value is not a Carbide lambda"))?;
lambda.call(args, ctx)
```

In Phase 2, when `MAP` sees a Function whose concrete type is *not* an `excellite::Lambda` (e.g., a `PyLambda`), the downcast misses and `MAP` errors. To fix this without coupling every `FuncImpl` to every engine, the plan is to add a small bridge method to `CarbideFn`:

```rust
trait CarbideFn {
    fn as_any(&self) -> &dyn Any;
    fn debug_label(&self) -> String;
    /// Apply this function to a list of values. The default does the
    /// `Lambda` downcast; engine-specific impls override.
    fn apply(&self, args: Vec<CellValue>, ctx: &dyn EvalCtx)
        -> Result<CellValue, EvalError> { … }
}
```

This is the change that lets `MAP` (and every higher-order helper) accept Python lambdas. When Phase 2 starts, the implementation work is: (a) add `apply()` to `CarbideFn`, (b) implement it on `PyLambda` via the PyO3 GIL handshake. The language doesn't change.

## Cross-lattice + cross-engine

When non-rectangular tilings arrive, the matrix is:

|              | Square sheet | Hex sheet | Voronoi sheet |
|---|---|---|---|
| Excel-lite engine | works | works (hex addr syntax) | partial (no range syntax) |
| Python engine | Phase 2 | Phase 2 | Phase 7+ |
| Rust engine | Phase 4 | Phase 4 | Phase 7+ |

The address strings in `CellRef`/`Range` are opaque to every engine. **Every engine just hands the address string to `EvalCtx::cell(addr)` and gets back a `CellValue`.** So an Excel-lite formula `=H(0,0)` works on a hex sheet today (the lattice parses the string; the engine never inspects it). A Python formula `ctx.cell("H(0,0)")` will work the same way.

The one place engines need lattice awareness is when surfacing **geometric** functions like `NEIGHBORS(cell)`. These are registered by the lattice itself into a per-lattice extension of the function registry. Excel-lite has access to them by virtue of running on a sheet whose lattice contributes them; Python will need a parallel mechanism (`ctx.lattice.neighbors(cell)` exposed through PyO3).

## The DAG

The dependency graph (`carbide-core::dag::Dag`) is engine-agnostic. Its nodes are `CellRef { sheet: SheetId, address: String }`; its edges are populated from `FormulaEngine::refs(compiled)`. The DAG doesn't know whether the address `H(0,0)` is a hex axial coord or a square address, and it doesn't care — it just treats the string as a node identifier.

This means cross-engine recompute works trivially: editing a Python cell that an Excel-lite cell depends on triggers the Excel-lite cell's `engine.eval(...)` because the DAG fired the dirty-closure walk and reached it. Both engines see the new value through `EvalCtx::cell`.

## Persistence

The on-disk `.crbd` format stores `Sheet.cells: HashMap<String, Cell>`, where each `Cell` carries:

- `source: Option<String>` — the user's typed source.
- `engine: Option<EngineKind>` — `None` means "use the workbook default".
- `value: CellValue` — last computed result.

`CompiledFormula` is **not** persisted. On workbook open, `engine::rebuild_dag` walks every cell with a `source`, asks the appropriate engine to `parse` it, populates the DAG. This is what restores live `CellValue::Function` values after `Function` round-trips as `#STALE!`.

Engines that need their own persistent state (e.g., the planned Rust native engine caches `cdylib`s in `~/.carbide/cache/native/`) own that cache outside the workbook file. The `.crbd` format itself is engine-neutral.

## Numbered guarantees

When implementing a new engine, these are the invariants to uphold:

1. `parse(src)` is **deterministic** for a given engine version. Two parses of the same source produce equivalent `CompiledFormula`s (which need not be `==`, but should evaluate identically).
2. `refs(compiled)` returns **every** address the formula could read. False negatives produce stale dirty-closures and silent bugs. False positives are OK (extra DAG edges → extra recomputes, no incorrectness).
3. `eval(compiled, ctx)` is **a pure function** of `(compiled, ctx)`. Engines must not rely on hidden state (per-call mutability is fine; per-process global state is not). The exception is `Pending`-returning async evaluation, which is reserved for future phases.
4. Returned `CellValue`s are **fully owned**. The engine cannot return a `CellValue` that borrows from `ctx` or from `compiled`.
5. Errors are surfaced via `EvalError`, never via `panic!`. The orchestrator wraps `EvalError` into `CellError`; a panic in the engine crashes the core binary.

## Open questions for Phase 2 onward

These are the design calls that will need to be made when the second engine arrives:

1. **Python source prefix.** Today, `=` is the formula discriminator. For Python, the leading sentinel could be `=py:` or a per-cell engine selector via the formula bar's engine chip (which exists in the UI but is read-only). Likely: chip-driven, with `=` still meaning "parse with the cell's current engine".
2. **Python value marshalling.** `CellValue::Number` ↔ Python `float`, `CellValue::Array` ↔ NumPy `ndarray`, `CellValue::Function` ↔ a PyO3 callable wrapper. Arrow as the wire format if zero-copy becomes a goal.
3. **GIL coordination.** PyO3 holds the GIL during eval. Parallel DAG evaluation (currently a future enhancement) has to serialise Python cells while parallelising others.
4. **Engine-specific imports / dependencies.** A Python formula that needs `pandas` — does Carbide ship pandas? Does the workbook declare its dependencies in `manifest.json`? Likely the latter; `manifest.json` already lists the engines required to open the file.
5. **Cross-engine `Function` apply.** As described above, `CarbideFn::apply` is the planned escape hatch. Worth re-checking the trait shape when PyLambda is real and we have a second concrete implementation to compare against.

When Phase 2 lands, these answers become canonical, and an `engines.md` companion to this page documents engine-specific behaviour (Python value mapping, Rust cdylib trust manifest, etc.) without polluting the language-level docs.
