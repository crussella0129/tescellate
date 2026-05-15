# Carbide — Values, Arrays, Coercion, Spill, Errors, Scope

This page is the reference for the dynamic type system: what values exist, how they relate, when one type silently becomes another, and what happens when a single cell's result is bigger than one cell.

Source files:

- `CellValue` + `Array` + `CarbideFn`: `crates/tescellate-core/src/value.rs`
- `CellError`: `crates/tescellate-core/src/cell.rs`
- `Env` (lexical environment): `crates/tescellate-core/src/env.rs`
- Coercion: `crates/tescellate-formula/src/excellite/funcs/coerce.rs`
- Spill: `crates/tescellate-formula/src/engine.rs::compute_spill_for`

## `CellValue`

```rust
pub enum CellValue {
    Empty,
    Number(f64),
    Integer(i64),
    Bool(bool),
    Text(String),
    Array(Box<Array>),
    Error(CellError),
    Pending,
    Function(Arc<dyn CarbideFn>),
}
```

| Variant | When you see it |
|---|---|
| `Empty` | Cell never written, or formula returns nothing. Coerces to `0` in arithmetic and `""` in concat. |
| `Number(f64)` | Default numeric. All fractional literals, all formula numerics. |
| `Integer(i64)` | Whole-number results from `COUNT`, `COUNTUNIQUE`, `RANK`, `LEN`, `MATCH`, `IN`, `ISBLANK`/`ISNUMBER`/`ISTEXT`/`ISERROR`. Integer-shaped values stay `Integer` until something forces them to `Number`. |
| `Bool(bool)` | Comparison results, `TRUE`/`FALSE` literals, `AND`/`OR`/`NOT`/`XOR`/`IS…`. |
| `Text(String)` | String literals, `CONCAT`, `TEXTJOIN`, `JOIN`, `UPPER`, `LOWER`, `STRINGIFY`-ish operations. |
| `Array(Box<Array>)` | Multi-element result. See [Array](#array) below. |
| `Error(CellError)` | Something failed. See [Errors](#errors). |
| `Pending` | Async eval / native compile in flight. Phase 4+. |
| `Function(Arc<dyn CarbideFn>)` | First-class function value. See [Function](#function). |

`CellValue` derives `Clone`, `Default` (= `Empty`), and `Debug`. `PartialEq` and serde are hand-implemented; see [Equality and persistence](#equality-and-persistence).

## Array

```rust
pub struct Array {
    pub rows: usize,
    pub cols: usize,
    pub data: Vec<CellValue>,    // row-major
}
```

- **Row-major**: element at `(r, c)` lives at `data[r * cols + c]`.
- A `1×1` array is *not* the same as a scalar — it still spills as a 1-cell region.
- A `0×0` array (`Array::new(0, 0, vec![])`) is the empty array. Returned by e.g. `SETDIFF([], anything)` or an empty `[]` literal.
- Construction helpers: `Array::row(vec![…])`, `Array::col(vec![…])`, `Array::from_2d(vec![vec![…], vec![…]])` (which validates rectangular shape and returns `ShapeError::Ragged` otherwise).
- Access: `arr.get(r, c) -> Option<&CellValue>`, `arr.shape() -> (rows, cols)`, `arr.iter() -> slice::Iter`.

### Where arrays come from

| Source | Shape |
|---|---|
| Array literal `[a, b, c]` | 1×3 row |
| Array literal `[[a,b],[c,d]]` | 2×2 |
| `SPLIT(delim, text)` | 1×N |
| `SPLIT(delim, range_of_texts)` | M×N |
| `UNIQUE(values)` | K×1 column (K ≤ input) |
| `SORT(values)` | K×1 column |
| `FILTER(values, mask)` | K×1 column |
| `SEQUENCE(rows, cols, …)` | rows×cols |
| `MAP(arrayA, [..arraysN], lambda)` | same shape as inputs |
| `BYROW(arr, lambda)` | rows×1 |
| `BYCOL(arr, lambda)` | 1×cols |
| `MAKEARRAY(rows, cols, lambda)` | rows×cols |
| `SCAN(initial, arr, lambda)` | N×1 |
| `TRANSPOSE(arr)` | swap rows/cols |
| `TAKE`/`DROP` | sub-shape |
| `SETUNION`/`SETDIFF`/`SETINTERSECT`/`SETSYMDIFF` | K×1 |

A range reference like `A1:B5` is *not* an `Array` at the AST level — it's a `Range`. It becomes a flat `Vec<CellValue>` (via `EvalCtx::range`) only when a function calls `flatten()` on it. Functions that take "an array" generally accept a `Range`, an `Array`, a scalar, or a single-cell ref; `flatten()` normalises all of them.

## Function

```rust
pub trait CarbideFn: Send + Sync + Any + Debug {
    fn as_any(&self) -> &dyn Any;
    fn debug_label(&self) -> String;
}
```

A `CellValue::Function(arc)` holds an opaque `Arc<dyn CarbideFn>`. The concrete type Carbide ships today is `excellite::lambda::Lambda`:

```rust
pub struct Lambda {
    pub params: Vec<String>,
    pub body: Expr,
    pub captured: Arc<Env>,
}
```

- `params` are the bound names (in order).
- `body` is the unevaluated AST.
- `captured` is the lexical environment at definition time. The closure is *lexically* scoped: the lambda's free variables resolve against this env, not the caller's. See [Lexical environment](#lexical-environment).

**Equality**: `Function(a) == Function(b)` iff `Arc::ptr_eq(a, b)`. Two textually-identical `LAMBDA(x, x+1)` expressions allocated independently compare *unequal*. This is cheap and stable; the DAG keys on `CellRef`, not value identity, so it never matters in practice.

**Calling**: `Lambda::call(args: Vec<CellValue>, outer: &dyn EvalCtx)` enforces arity, builds an `Env` with the args bound by name (chained to `captured`), wraps `outer` in a `ScopedCtx`, and recursively `eval`s the body. The same call path is shared by direct `Apply`, by the `Call → Var fallback` for LET-bound names, and by every higher-order helper (`MAP`, `REDUCE`, `BYROW`, `BYCOL`, `SCAN`, `MAKEARRAY`).

**Where lambdas live**: in cell values transiently (because formula results can be functions) and in `Env` bindings (because `LET(f, LAMBDA(…), …)` binds `f` to the lambda). The `Lambda` type lives in `tescellate-formula`, not core; that's why core exposes the trait object `CarbideFn` and not the concrete type.

## Equality and persistence

The full equality table on `CellValue`:

| | |
|---|---|
| `Empty == Empty` | true |
| `Number(a) == Number(b)` | IEEE-754 `==` (so `NaN != NaN`, `-0.0 == 0.0`) |
| `Integer(a) == Integer(b)` | exact |
| `Number(a) == Integer(b)` | **false** — different variants compare unequal even if their numerical values match. Coerce explicitly if you want value equality. |
| `Bool(a) == Bool(b)` | exact |
| `Text(a) == Text(b)` | exact (Unicode codepoint by codepoint) |
| `Array(a) == Array(b)` | shape equal AND every element equal |
| `Error(a) == Error(b)` | variant equal |
| `Pending == Pending` | true |
| `Function(a) == Function(b)` | `Arc::ptr_eq(a, b)` |
| mixed variants | false |

(Excel's comparison operators, by contrast, do cross-type numeric coercion — see [Comparison rules](#comparison-rules). Internal `PartialEq` and user-visible `=` / `<>` are separate concerns.)

**Persistence**: `CellValue` round-trips through `.tscl` zip files via custom serde. Every variant survives a save/load *except* `Function`, which serialises to `{"kind":"function","value":{"label":"λ(x) → …"}}` and deserialises as `CellError::StaleFunction`. Lambdas can't be carried across processes — their bodies are AST nodes and their captured envs may reference engine-specific values — but the cell's source string (e.g., `=LAMBDA(x, x+1)`) is preserved, and `engine::rebuild_dag` re-evaluates every cell with a source after a load, which restores live `Function` values. The user sees `#STALE!` only if the engine for that formula isn't compiled into the build that's opening the file.

## Coercion

Functions and operators that need a specific type coerce their inputs through one of four helpers in `funcs/coerce.rs`:

### `to_number(v: &CellValue) -> Result<f64, EvalError>`

| Input | Output |
|---|---|
| `Number(n)` | `n` |
| `Integer(i)` | `i as f64` |
| `Bool(true)` | `1.0` |
| `Bool(false)` | `0.0` |
| `Empty` | `0.0` |
| `Text("3.14")` | `3.14` — parsed as `f64`; non-numeric text → `#VALUE!` |
| `Error(e)` | propagates as `EvalError::Value` |
| `Array(_)` | `#VALUE!` "array in scalar context" |
| `Function(_)` | `#VALUE!` "function in scalar context" |
| `Pending` | `#VALUE!` |

### `to_bool(v: &CellValue) -> bool`

Doesn't fail (returns `false` for unknown shapes). Used by `IF`, `AND`, `OR`, `NOT`, `IFS`, `FILTER` mask values.

| Input | Output |
|---|---|
| `Bool(b)` | `b` |
| `Number(n)` | `n != 0.0` |
| `Integer(i)` | `i != 0` |
| `Text(s)` | `!s.is_empty()` |
| `Empty` | `false` |
| anything else | `false` |

### `to_int(v: &CellValue) -> Result<i64, EvalError>`

`to_number(v)?` then `trunc() as i64`. Used for indices in `INDEX`, `MID`, `MAKEARRAY`, `QUARTILE`, `TAKE`/`DROP`, etc.

### `stringify(v: &CellValue) -> String`

Total. Used by `&` (concat), `CONCAT`, `TEXTJOIN`, `JOIN`, `SUBSTITUTE`, etc.

| Input | Output |
|---|---|
| `Text(s)` | `s.clone()` |
| `Number(n)` | integer form if `n == trunc(n)` and `\|n\| < 1e16`, else `{n}` (Rust's default `Display`) |
| `Integer(i)` | `i.to_string()` |
| `Bool(true)` / `Bool(false)` | `"TRUE"` / `"FALSE"` |
| `Empty` | `""` |
| `Error(e)` | `format!("{e:?}")` — for diagnostics; users see the rendered `#NAME!` label in the UI |
| `Array(_)` | `"{array}"` — sentinel; arrays should be flattened before concat |
| `Pending` | `"..."` |
| `Function(f)` | `f.debug_label()` (e.g., `"λ(x) → …"`) |

### `flatten(arg: &Expr, ctx: &dyn EvalCtx) -> Result<Vec<CellValue>, EvalError>`

The universal "make this iterable" helper. Given any `Expr` (range, array literal, scalar, call returning array, …), returns a flat `Vec<CellValue>`. Functions that operate "across every element" (SUM, AVERAGE, UNIQUE, JOIN, SETDIFF, MAP, etc.) all go through this.

### Implicit Number-vs-Integer

Arithmetic operators always produce `Number`, even when both operands are `Integer`. Functions that count produce `Integer`. There is no automatic narrowing back — `=COUNT(A:A) + 1` is `Number`, not `Integer`. This matters only for the equality table; for every other purpose, `Number(5.0)` and `Integer(5)` behave identically.

## Comparison rules

The binary operators `=` `<>` `<` `>` `<=` `>=` use `funcs::coerce::compare`, which is Excel-style cross-type:

1. Both numeric (`Number` or `Integer`, mixed OK): compare as `f64`.
2. Both `Text`: compare by Rust string ordering (UTF-8 lexicographic, codepoint by codepoint).
3. Both `Bool`: compare as numbers (`false < true`).
4. Cross-type — the type *tag* orders the comparison: `Empty < Numeric < Text < Bool`. So `1 < "a"` is `TRUE`, `"a" < TRUE` is `TRUE`. **Excel does the same**; this is not the place to deviate.
5. `Empty` against `Numeric` is treated as `Number(0)` for comparison (`Empty = 0` is `TRUE`).
6. `Empty` against `Text` is treated as `""` (`Empty = ""` is `TRUE`).
7. Anything else: the type tag wins.

This is **not** the same as `PartialEq` on `CellValue`. `PartialEq` is strict variant-equality (for the DAG and for tests). The `=` operator inside a formula runs `compare(..) == Ordering::Equal`, which is the cross-type version.

## Errors

```rust
pub enum CellError {
    Ref, Cycle, DivZero, Num, Value,
    Lang(String),
    Compile(String),
    Timeout,
    Spill,
    StaleFunction,
}
```

| Variant | Rendered | Cause |
|---|---|---|
| `Ref` | `#REF!` | Broken cell reference, unbound `Var`, range-end resolution miss. |
| `Cycle` | `#CYCLE!` | Cell participates in a dependency cycle. The DAG rejects edge additions that would close a cycle and marks the *adding* cell. |
| `DivZero` | `#DIV/0!` | Self-explanatory. |
| `Num` | `#NUM!` | Numeric domain error: `SQRT(-1)`, `LOG(0)`, `LOG(base, 1)`, `POWER` overflow. |
| `Value` | `#VALUE!` | Catch-all for "wrong shape" or "wrong type" — text where a number was needed, array in scalar context, bad arity if not caught by `BadArity`. |
| `Lang(s)` | `#LANG!` | Parse error in the formula language. The `s` carries the parser's diagnostic for tooling; the user sees `#LANG!`. |
| `Compile(s)` | `#COMPILE!` | Reserved for Phase 4 native-Rust compilation errors. Not produced today. |
| `Timeout` | `#TIMEOUT!` | Reserved for Phase 4 execution-budget enforcement. Not produced today. |
| `Spill` | `#SPILL!` | A cell's array result tried to spill into an already-occupied cell. See [Spill](#spill). |
| `StaleFunction` | `#STALE!` | A `Function` value was deserialised from `.tscl` and hasn't been re-evaluated yet. Transient: rebuild_dag clears it. |

The `EvalError` type used internally (`EvalError::Ref`, `EvalError::Value`, `EvalError::BadArity { name, want, got }`, etc.) is converted to `CellError` via `excellite::eval::eval_error_to_cell_error` when stored on a cell. Parse errors (`ParseError`) and out-of-bounds writes (`SetCellError::OutOfBounds`) bypass that path and are stored as `CellError::Lang` and `CellError::Ref` respectively.

## Spill

When a cell's result is an `Array` of size > 1×1, the array "spills" into adjacent cells. This matches modern Excel and Google Sheets dynamic arrays.

- **The source cell** stores the full `Array` value and the formula.
- **Spill cells** are *virtual*: they don't live in `Sheet.cells`. They're materialised on read by `WorkbookEngine::snapshot_range` from the source's array. The on-disk format never has to know about spill.
- **Collision rule**: if any target cell of a spill is already occupied (has a `source`), the source cell flips to `CellError::Spill` and *no* virtual cells are emitted. Clearing the blocker auto-restores the spill.
- **DAG**: spill cells don't appear in the DAG. A formula `=B3` that points at a spilled cell registers `B3` as a dep; B3 isn't a real entry, but `snapshot_range`'s spill expansion fills its value from the source array on read. (Subtlety: this means dirty-closure recomputation triggered by writing the source doesn't reach formulas that read spill cells. Listed as a known limitation; the practical effect today is that you may need to re-edit a dependent formula once. Fix lands when we revisit DAG-as-graph for cross-sheet edges.)
- **Editing a spill cell**: typing a new source into a virtual cell makes that cell real and instantly causes the source's array to collide → `#SPILL!`. Excel does the same.

`Function`-valued sources are not currently special-cased for spill; a cell `=LAMBDA(x, x+1)` has shape `1×1` (the function itself, not its application result) and doesn't spill. The function is what's stored, not the body.

## Lexical environment

Every cell evaluation starts with no lexical scope: `EvalCtx::var(name)` returns `None`, `EvalCtx::env()` returns `None`. The functions that introduce scope are `LAMBDA`, `LET`, and `LETREC`.

```rust
pub struct Env {
    pub bindings: Arc<RwLock<HashMap<String, CellValue>>>,
    pub parent:   Option<Arc<Env>>,
}
```

A scope is an `Arc<Env>`. Lookups walk the parent chain (lexical). Insertions are O(1) into the local `HashMap`.

### LET — sequential

```excel
=LET(x, 10, y, x*2, x + y)        → 30
```

`LET` creates one new child `Env` and evaluates each `(name, value)` pair in order, inserting the result before the next pair runs. The body sees every binding. **Each value sees previous bindings but not later ones.**

### LAMBDA — closure capture

```excel
=LET(
  n, 100,
  add_n, LAMBDA(x, x + n),
  add_n(5))                       → 105
```

When `LAMBDA` runs, it captures the *current* `EvalCtx::env()` as its `Lambda::captured`. The captured `Arc<Env>` is shared by reference with whatever else is using it — that's how LETREC's mutual recursion works.

### LETREC — placeholder-then-patch

```excel
=LETREC(
  even, LAMBDA(n, IF(n=0, TRUE,  odd(n-1))),
  odd,  LAMBDA(n, IF(n=0, FALSE, even(n-1))),
  even(10))                       → TRUE
```

`LETREC` runs in two phases over the bindings:

1. Insert every name with `CellValue::Empty` as a placeholder, all in the same `Env`.
2. Evaluate each value in that env. Lambdas constructed during phase 2 capture the shared `Arc<Env>` — and because phase 2 mutates the env's bindings in place (through the `Arc<RwLock>`), every lambda's view of every name is patched as values come in.

When a lambda is later *called*, it looks up sibling names through its captured env, which by then has all the bindings filled in. That's how recursion and mutual recursion work.

**Non-lambda values that forward-reference siblings see the placeholder.** A `LETREC(a, b+1, b, 5, a)` evaluates `b+1` with `b` still `Empty` (= 0), so `a` is 1, not 6. This is intentional and documented; use plain `LET` if you don't need recursion.

### Scope chains

`ScopedCtx<'a> { parent: &'a dyn EvalCtx, scope: Arc<Env> }` is how the evaluator threads the scope chain through `eval`. Every `LAMBDA::call`, every `LET`/`LETREC` body, and every higher-order helper builds a fresh `ScopedCtx` over the previous one. Cell-and-range lookups always delegate to the *original* context (the `SheetEvalView` on the workbook); only variable lookups walk the scope chain.

## Quick reference: how to express common patterns

```excel
=LET(x, EXPR, …)                   bind x for the body
=LAMBDA(x, EXPR)                   anonymous function
=LET(f, LAMBDA(x, EXPR), f(arg))   apply a named lambda
=LETREC(name, LAMBDA(…), …)        recursive / mutually-recursive lambdas

=MAP(arr, LAMBDA(x, EXPR))         element-wise transform
=REDUCE(init, arr, LAMBDA(a,x, …)) left fold
=SCAN(init, arr, LAMBDA(a,x, …))   left fold keeping intermediates
=BYROW(arr, LAMBDA(row, EXPR))     reduce each row
=BYCOL(arr, LAMBDA(col, EXPR))     reduce each column
=MAKEARRAY(n, m, LAMBDA(r,c, …))   build an array from coords (1-indexed)
```

For the full function library see [functions.md](functions.md).
