# Carbide — Function Catalog

Every built-in function in the Carbide standard library, grouped by category. The implementation lives in `crates/tescellate-formula/src/excellite/funcs/`. Every function listed here is registered in `funcs::standard()` and routed through the `FunctionRegistry`.

Convention used throughout:

- **Signature**: `NAME(arg, …) → result_kind`. Optional args are in `[brackets]`. Variadics are `…name`.
- **Behaviour**: short prose. Edge cases are called out with **Edge:**.
- **Example**: a runnable formula. Where a test in `lambda_funcs.rs#tests` or `engine.rs#tests` matches, it's the example used.

Every function's first argument-or-args go through `flatten()` unless noted otherwise. `flatten()` accepts a `Range`, an `Array`, a `CellValue::Array`, a scalar, or any expression that evaluates to one of those.

Number-vs-Integer: aggregates and counts return `Integer` where natural (`COUNT`, `COUNTA`, `RANK`, `LEN`, `MATCH`, `IN`). Everything else is `Number`.

For the broader type-coercion rules see [types.md](types.md).

---

## Table of contents

1. [Bindings and abstraction (LET, LAMBDA, LETREC)](#bindings-and-abstraction)
2. [Higher-order helpers (MAP, REDUCE, SCAN, BYROW, BYCOL, MAKEARRAY)](#higher-order-helpers)
3. [Aggregates (SUM, AVERAGE, COUNT, COUNTA, MIN, MAX)](#aggregates)
4. [Logical (IF, IFS, SWITCH, IFERROR, IFNA, AND, OR, NOT, XOR, TRUE, FALSE, ISBLANK, ISNUMBER, ISTEXT, ISERROR)](#logical)
5. [Math (ABS, ROUND, CEILING, FLOOR, MOD, POWER, SQRT, EXP, LN, LOG, INT, TRUNC, SIGN, PI)](#math)
6. [Text (LEN, UPPER, LOWER, PROPER, TRIM, LEFT, RIGHT, MID, REPT, EXACT, SUBSTITUTE, FIND, SEARCH, REPLACE, CONCAT, TEXTJOIN, JOIN, SPLIT, TEXTSPLIT, VALUE, TEXT, CHAR, CODE)](#text)
7. [Lookup (VLOOKUP, HLOOKUP, INDEX, MATCH, CHOOSE)](#lookup)
8. [Dynamic-array (UNIQUE, COUNTUNIQUE, SORT, FILTER, SEQUENCE, TAKE, DROP, TRANSPOSE, FLATTEN)](#dynamic-array)
9. [Set operations (SETUNION, SETDIFF, SETINTERSECT, SETSYMDIFF, IN, COUNTIF)](#set-operations)
10. [Statistical (STDEV, STDEVP, VAR, VARP, MEDIAN, MODE, PERCENTILE, QUARTILE, RANK, CORREL, COVAR, COVARP, COVARS, SLOPE, INTERCEPT, FORECAST, RSQ)](#statistical)

---

## Bindings and abstraction

These three are the lexical-scope core. See [types.md § Lexical environment](types.md#lexical-environment) for the semantic model.

### `LET(name1, value1, [name2, value2, …], body) → any`

Bind values to names in sequence and evaluate `body`. Each value sees all previous bindings.

```excel
=LET(x, 10, y, x*2, x + y)        → 30
```

**Edge:** `LET(A1, 10, A1+5)` errors — `A1` lexes as a `CellRef`, not a `Var`. Use a name like `x`.

### `LAMBDA(p1, [p2, …], body) → function`

Build an anonymous function. Each `pᵢ` must be a bare name. The lambda captures the surrounding lexical environment by reference; siblings defined in a containing `LETREC` are visible by the time the lambda is called.

```excel
=(LAMBDA(x, x*2))(5)              → 10
=LET(f, LAMBDA(x, x+1), f(7))     → 8
```

**Edge:** duplicate parameter names error at definition time. Non-bare-name parameters (`LAMBDA(42, …)`) error at definition time.

### `LETREC(name1, value1, [name2, value2, …], body) → any`

Like `LET`, but every binding is pre-inserted with `Empty` placeholder, then patched with its actual value. The shared `Env` lets mutually-recursive and self-recursive lambdas resolve siblings by name.

```excel
=LETREC(fact, LAMBDA(n, IF(n<=1, 1, n*fact(n-1))), fact(6))       → 720
=LETREC(
   fib, LAMBDA(n, IF(n<=1, n, fib(n-1) + fib(n-2))),
   fib(10))                                                        → 55
=LETREC(
   even, LAMBDA(n, IF(n=0, TRUE,  odd(n-1))),
   odd,  LAMBDA(n, IF(n=0, FALSE, even(n-1))),
   even(10))                                                       → TRUE
```

**Edge:** non-lambda forward refs see `Empty`. `=LETREC(a, b+1, b, 5, a)` returns `1`, not `6`. Use `LET` if recursion isn't needed.

---

## Higher-order helpers

These functions consume a lambda — usually the last argument — and call it per element / row / column / coordinate. The lambda arg must evaluate to a `CellValue::Function`; passing a non-function returns `#VALUE!`.

### `MAP(array, […more_arrays], lambda) → array`

Apply `lambda` element-wise across one or more arrays of identical shape. Output shape equals input shape.

```excel
=MAP([1,2,3,4], LAMBDA(x, x*2))                       → [2, 4, 6, 8]
=MAP([1,2,3], [10,20,30], LAMBDA(a, b, a+b))          → [11, 22, 33]
```

**Edge:** mismatched input shapes → `#VALUE!`.

### `REDUCE(initial, array, lambda) → any`

Left fold. `lambda(acc, x) → acc'`.

```excel
=REDUCE(0, [1,2,3,4], LAMBDA(a, x, a + x*x))          → 30   (sum of squares)
```

### `SCAN(initial, array, lambda) → array`

Left fold keeping intermediates. Output is a column array of length = input length; the initial value is *not* included.

```excel
=SCAN(0, [1,2,3,4], LAMBDA(a, x, a+x))                → [1, 3, 6, 10]
```

### `BYROW(array, lambda) → column-array`

Apply `lambda(row_as_1xN_array)` to each row. Output is `rows × 1`.

```excel
=BYROW([[1,2,3],[4,5,6]], LAMBDA(row, SUM(row)))      → [6; 15]
```

### `BYCOL(array, lambda) → row-array`

Apply `lambda(col_as_Mx1_array)` to each column. Output is `1 × cols`.

```excel
=BYCOL([[1,5],[3,2],[4,4]], LAMBDA(col, MAX(col)))    → [4, 5]
```

### `MAKEARRAY(rows, cols, lambda) → array`

Build a `rows × cols` array by calling `lambda(r, c)` with 1-indexed coordinates.

```excel
=MAKEARRAY(4, 4, LAMBDA(r, c, IF(r=c, 1, 0)))         → 4×4 identity matrix
```

---

## Aggregates

Numerical aggregation across ranges/arrays. Non-numeric values are silently skipped (Excel convention) — text in a range does not stop a `SUM` from succeeding.

### `SUM(…values) → number`

Sum of every numeric value in every argument (after `flatten()`).

```excel
=SUM(A1:A10)                  =SUM([1,2,3])           =SUM(A1, A3, A5)
```

### `AVERAGE(…values) → number` *(alias: `AVG`)*

Arithmetic mean. **Edge:** empty input → `#DIV/0!`.

### `COUNT(…values) → integer`

Count of numeric values across all arguments. `Bool` counts as numeric (`TRUE`→1, `FALSE`→0).

### `COUNTA(…values) → integer`

Count of non-`Empty` values, regardless of type.

### `MIN(…values) → number`  •  `MAX(…values) → number`

Min/max over numeric values. **Edge:** all-empty input returns `0` (Excel-compatible).

---

## Logical

### `IF(condition, then, [else]) → any`

Short-circuit: only the chosen branch is evaluated.

```excel
=IF(A1>0, "pos", "neg")
```

**Edge:** `else` omitted → `FALSE` when `condition` is falsy.

### `IFS(c1, v1, c2, v2, …) → any`

Pick the first `vᵢ` whose `cᵢ` is truthy. **Edge:** no matching condition → `#VALUE!`.

### `SWITCH(value, c1, v1, c2, v2, …, [default]) → any`

Matches `value` against each `cᵢ` using `compare()` (Excel-style cross-type). **Edge:** no match and no default → `#VALUE!`.

### `IFERROR(value, fallback) → any`

Returns `fallback` if `value` evaluates to a `CellError` or an `EvalError`.

```excel
=IFERROR(A1/B1, 0)
```

### `IFNA(value, fallback) → any`

Like `IFERROR` but only matches `CellError::Ref` / lookup-miss. Use for the "value not found" case without swallowing real errors.

### `AND(…) → bool`  •  `OR(…) → bool`  •  `NOT(x) → bool`  •  `XOR(…) → bool`

Boolean short-circuiting (`AND`/`OR` stop on first decisive value). Inputs are coerced via `to_bool`. `XOR` returns true iff an odd number of arguments are truthy.

### `TRUE() → bool`  •  `FALSE() → bool`

Function forms; equivalent to the literal `TRUE` / `FALSE` keywords.

### `ISBLANK(value) → bool`

True iff `value` is `CellValue::Empty`. Notably **false** for `Text("")`.

### `ISNUMBER(value) → bool`  •  `ISTEXT(value) → bool`  •  `ISERROR(value) → bool`

Type predicates. `ISERROR` catches any `CellError` *or* a thrown `EvalError`.

---

## Math

### `ABS(x) → number`

Absolute value.

### `ROUND(x, [digits=0]) → number`

Half-away-from-zero rounding to `digits` decimal places (negative `digits` rounds left of the decimal).

### `CEILING(x, [significance=1]) → number`  •  `FLOOR(x, [significance=1]) → number`

Round toward / away from zero in steps of `significance`. **Edge:** `significance=0` → `0`.

### `MOD(n, d) → number`

Modulo with the Excel convention: result takes the *sign of the divisor*. So `MOD(-1, 3) = 2`. **Edge:** `d=0` → `#DIV/0!`.

### `POWER(base, exponent) → number`

`base^exponent`. **Edge:** non-finite result (overflow, 0^negative) → `#NUM!`. Same as the `^` operator.

### `SQRT(x) → number`

**Edge:** `x < 0` → `#NUM!`.

### `EXP(x) → number`  •  `LN(x) → number`

`e^x` and natural log. **Edge:** `LN(x ≤ 0)` → `#NUM!`.

### `LOG(x, [base=10]) → number`

**Edge:** `x ≤ 0`, `base ≤ 0`, or `base = 1` → `#NUM!`.

### `INT(x) → number`

Floor (Excel's rounding-down-toward-negative-infinity).

### `TRUNC(x) → number`

Truncation toward zero.

### `SIGN(x) → integer`

`-1`, `0`, or `1`.

### `PI() → number`

The constant.

---

## Text

### `LEN(text) → integer`

Number of Unicode characters. Not bytes.

### `UPPER(text)` • `LOWER(text)` • `PROPER(text)` → text

`PROPER` title-cases each whitespace-separated word.

### `TRIM(text) → text`

Excel-style: collapses every run of whitespace (internal and leading/trailing) to a single space, then strips ends.

### `LEFT(text, [n=1]) → text`  •  `RIGHT(text, [n=1]) → text`

Take the first / last `n` characters. **Edge:** `n` greater than length returns the whole string.

### `MID(text, start, length) → text`

1-indexed substring. **Edge:** `start < 1` → `#VALUE!`.

### `REPT(text, n) → text`

Concatenate `n` copies. **Edge:** `n < 0` → empty string.

### `EXACT(a, b) → bool`

Case-sensitive text equality.

### `SUBSTITUTE(text, old, new, [nth]) → text`

Replace `old` with `new` everywhere, or only the `nth` occurrence if given (1-indexed). **Edge:** `old=""` returns `text` unchanged.

### `FIND(needle, haystack, [start=1]) → integer`

Case-sensitive 1-indexed position. **Edge:** not found → `#VALUE!`.

### `SEARCH(needle, haystack, [start=1]) → integer`

Like `FIND` but case-insensitive. **Edge:** not found → `#VALUE!`.

### `REPLACE(text, start, num_chars, new_text) → text`

Replace `num_chars` characters starting at 1-indexed `start`.

### `CONCAT(…) → text`  *(alias: `CONCATENATE`)*

Concatenate every argument after `flatten()` and `stringify()`. Empty cells become empty strings (not skipped).

```excel
=CONCAT(A1:A10)
=CONCAT("year=", YEAR(TODAY()))   ← TODAY() lives in a future phase
```

### `TEXTJOIN(delimiter, ignore_empty, …values) → text`

Like `CONCAT` but with a delimiter and an `ignore_empty` flag.

```excel
=TEXTJOIN(", ", TRUE, "a", "", "b", "c")              → "a, b, c"
```

### `JOIN(delimiter, array) → text`

Delimited concatenation of an array (or any flattenable expression). Symmetric with `SPLIT`. **Does not** offer the `ignore_empty` knob — use `TEXTJOIN` if you need it.

```excel
=JOIN("~", ["Red","Orange","Yellow"])                 → "Red~Orange~Yellow"
=JOIN("~", A1:F1)
```

### `SPLIT(delimiter, text_or_range) → array`

Inverse of `JOIN`. Delimiter is the **first** argument (different from Sheets, which puts text first; chosen for symmetry with `JOIN`). The second argument may be a scalar text, a range, or an array — in which case `SPLIT` returns a 2-D array (one row per input cell, padded with `Empty` for uneven splits).

```excel
=SPLIT("~", "Red~Orange~Yellow~Green~Blue~Purple")    → 1×6 row
=SPLIT(",", A1:A4)                                    → 4×N (N = max splits)
=SPLIT("", "abc")                                     → 1×3 ["a","b","c"]
```

### `TEXTSPLIT(text, col_delim, [row_delim]) → array`

Excel-flavoured: splits `text` first by `row_delim` into rows, then each row by `col_delim` into columns. Padded rectangular result.

### `VALUE(text) → number`

Parse `text` as a number. **Edge:** unparseable text → `#VALUE!`.

### `TEXT(value, format) → text`

Minimal: stringifies `value`; the format string is accepted but ignored in this release. Full format-string support is a later phase.

### `CHAR(code) → text`

Unicode-code-point to single-char text.

### `CODE(text) → integer`

First Unicode code-point of `text`. **Edge:** empty string → `#VALUE!`.

---

## Lookup

### `VLOOKUP(needle, table, col_index, [range_lookup]) → any`

Exact-match search in the first column of `table`. Returns the value from the row's `col_index`-th column (1-indexed). The `range_lookup` arg is accepted for compatibility but ignored (always exact in v1).

```excel
=VLOOKUP("b", [["a",1],["b",2],["c",3]], 2)           → 2
```

**Edge:** not found → `#REF!`.

### `HLOOKUP(needle, table, row_index, [range_lookup]) → any`

Symmetric: search the first row, return from `row_index`-th row.

### `INDEX(array, row, [col]) → any`

1-indexed cell access. With one row index and a 1-row array, the index is the column. With both indices and a 2-D array, both apply.

```excel
=INDEX([[10,20],[30,40]], 2, 1)                       → 30
```

**Edge:** out-of-range indices → `#REF!`.

### `MATCH(needle, range, [match_type]) → integer`

1-indexed position of `needle` in `range` (after `flatten()`). `match_type` is accepted but only exact-match is supported in v1.

### `CHOOSE(index, val1, val2, …) → any`

Pick the `index`-th value (1-indexed). **Edge:** index out of range → `#REF!`.

---

## Dynamic-array

These build or transform arrays. When assigned to a cell, the result spills into adjacent cells; see [types.md § Spill](types.md#spill).

### `UNIQUE(array) → column-array`

First-seen-order deduplication. Equality uses `compare()` (Excel-style cross-type).

```excel
=UNIQUE(["a","b","a","c"])                            → ["a","b","c"]
```

### `COUNTUNIQUE(array) → integer`

Count of distinct values.

### `SORT(array, [order=1]) → column-array`

`order >= 0` ascending, `order < 0` descending. Sort uses `compare()`; equal-tag elements within a tag sort naturally.

### `FILTER(array, mask) → column-array`

Keep elements of `array` where the corresponding `mask` element is truthy. Mask must be the same length as input.

```excel
=FILTER([1,2,3,4], [TRUE,FALSE,TRUE,FALSE])           → [1, 3]
```

**Note:** range-vs-scalar broadcasting in binary ops (`A1:A10 > 0`) is not yet implemented. Build the mask explicitly with `MAP`:

```excel
=FILTER(A1:A10, MAP(A1:A10, LAMBDA(x, x>0)))
```

### `SEQUENCE(rows, [cols=1], [start=1], [step=1]) → array`

Arithmetic progression laid out as a `rows × cols` grid.

```excel
=SEQUENCE(5)                                          → column [1,2,3,4,5]
=SEQUENCE(2, 3, 10, 0.5)                              → [[10,10.5,11],[11.5,12,12.5]]
```

### `TAKE(array, n_rows, [n_cols]) → array`  •  `DROP(array, n_rows, [n_cols]) → array`

Keep / discard the first `n` rows (and optionally cols). Negative `n` operates from the end.

### `TRANSPOSE(array) → array`

Swap rows ↔ columns.

### `FLATTEN(array) → column-array`

Collapse any-shape array to a single column.

---

## Set operations

Set algebra over flattened arrays. Equality uses `compare()`; the result preserves the order each element was first seen.

### `SETUNION(a, [b, …]) → column-array`

Distinct values across every argument.

### `SETDIFF(a, b) → column-array`

`a \ b` — values in `a` not present in `b`, deduped.

```excel
=SETDIFF(SPLIT("~", "a~b~c~d"), SPLIT("~", "b~d"))    → ["a","c"]
```

### `SETINTERSECT(a, b) → column-array`

Values present in both. Deduped.

### `SETSYMDIFF(a, b) → column-array`

Symmetric difference: values in exactly one of `a` or `b`.

### `IN(value, array) → bool`

Boolean membership.

### `COUNTIF(range, criterion) → integer`

Count of elements in `range` equal to `criterion`. Comparison-string criteria (`">10"`, `"<>foo"`) are not yet supported; equality only.

---

## Statistical

Descriptive + bivariate. All consume `flatten()` and silently skip non-numeric values.

### Descriptive

| Function | Returns |
|---|---|
| `STDEV(array)` | Sample standard deviation (divisor `n-1`). **Edge:** `n < 2` → `#DIV/0!`. |
| `STDEVP(array)` | Population standard deviation (divisor `n`). **Edge:** empty → `#DIV/0!`. |
| `VAR(array)` | Sample variance. |
| `VARP(array)` | Population variance. |
| `MEDIAN(array)` | The middle value (mean of the two middles if `n` is even). |
| `MODE(array)` | Most-frequent value, first-seen wins on ties. **Edge:** every value unique → `#NUM!`. |
| `PERCENTILE(array, p)` | Linear interpolation, Excel-compatible (`PERCENTILE.INC`). `p ∈ [0,1]`. |
| `QUARTILE(array, q)` | `q ∈ {0,1,2,3,4}`; built on `PERCENTILE`. `QUARTILE(A, 0)=MIN`, `QUARTILE(A, 2)=MEDIAN`, `QUARTILE(A, 4)=MAX`. |
| `RANK(value, range, [order])` | 1-indexed rank. `order=0`/omitted ⇒ descending (largest is 1); nonzero ⇒ ascending. **Edge:** value not in range → `#NUM!`. |

```excel
=MEDIAN(A1:A100)               =PERCENTILE(A1:A100, 0.5)        identical
=QUARTILE([1,2,3,4,5,6,7,8,9,10], 1)                              → 3.25
```

### Bivariate

All take two co-shaped arrays. Pairs are formed by flattening both to the same length, then keeping only pairs where both elements are numeric.

| Function | Returns |
|---|---|
| `CORREL(xs, ys)` | Pearson correlation coefficient. **Edge:** `n < 2` or zero variance → `#DIV/0!`. |
| `COVAR(xs, ys)` | Population covariance (legacy Excel name). |
| `COVARP(xs, ys)` | Population covariance. |
| `COVARS(xs, ys)` | Sample covariance (divisor `n-1`). |
| `SLOPE(ys, xs)` | OLS slope `m` in `y = mx + b`. **Note** the Excel argument order: y's first. |
| `INTERCEPT(ys, xs)` | OLS intercept `b`. |
| `FORECAST(x, ys, xs)` | OLS prediction at `x`. |
| `RSQ(xs, ys)` | `CORREL(xs, ys)^2`. |

```excel
=SLOPE(B1:B10, A1:A10)                               → m
=INTERCEPT(B1:B10, A1:A10)                           → b
=FORECAST(100, B1:B10, A1:A10)                       → predict y at x=100
=CORREL([1..10], [2..20])                            → 1.0
```

---

## Deferred functions

The following are commonly requested but not yet implemented; each has a path to land in a future phase. Listed for users browsing for "where is X?":

| Excel/Sheets feature | Carbide status |
|---|---|
| Dotted stat names (`STDEV.P`, `VAR.P`, `COVARIANCE.S`) | Lexer needs identifier-dots support. Use `STDEVP`, `VARP`, `COVARS`. |
| Multi-variable `LINEST` | Phase 4+. |
| `OFFSET`, `INDIRECT`, `ADDRESS` | Need a typed `CellRef` value. Phase 3+. |
| `XLOOKUP` | Trivial; will likely land in Phase 2 with the wider lookup pass. |
| `NOW`, `TODAY`, `DATE`, `TIME`, `YEAR`, `MONTH`, `DAY`, `HOUR`, `MINUTE`, `SECOND`, `WEEKDAY` | Date/time support is its own phase — `CellValue::DateTime` exists in PLAN.md §4 but isn't wired yet. |
| `RAND`, `RANDBETWEEN`, `RANDARRAY` | Need a deterministic-with-respect-to-recalc seed strategy. |
| Financial (`PMT`, `PV`, `FV`, `NPV`, `IRR`) | Phase 4+. |
| Database (`DSUM`, `DCOUNT`, `DAVERAGE`) | Out of scope until typed columns arrive. |
| Regular expressions (`REGEXMATCH`, `REGEXEXTRACT`, `REGEXREPLACE`) | Need a regex dependency choice; Phase 3+. |
| Comparison-string `COUNTIF`/`SUMIF` criteria (`">10"`) | Trivial pass to add. |
| Array broadcasting in binary ops (`A1:A10 > 0` → mask array) | Documented limitation; use `MAP(..., LAMBDA(x, x > 0))`. |

When any of these lands it will be added to this catalog and to the registry.
