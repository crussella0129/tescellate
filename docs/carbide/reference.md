# The Carbide Formula Language — Reference

Carbide is Tescellate's default formula language: the Excel-lite engine
every workbook uses unless a cell opts into another engine (Python today;
Rhai and rust-native are planned). This document is the **foundation
reference** — the value model, syntax, and semantics. It is not the
exhaustive function catalogue, though every built-in is indexed below.

> **Verified.** Every example in this reference is reproduced and checked
> against the engine by `crates/tescellate-formula/tests/reference_examples.rs`.
> If the engine and this document disagree, CI fails — so the examples here
> cannot silently rot.

In a cell, a **formula begins with `=`**: `=A1+1` is a formula, while
`A1+1` (no `=`) is stored as literal text. The examples below show the
formula *body*; in a cell you would prefix each with `=`.

---

## Values

Every formula evaluates to a single value of one of these kinds:

| Kind        | Notes |
|-------------|-------|
| **Number**  | A 64-bit IEEE float. The result of all arithmetic. |
| **Integer** | A 64-bit signed integer. Produced by counting functions (`COUNT`, `COUNTA`, …); numerically interchangeable with Number. |
| **Text**    | A UTF-8 string. |
| **Boolean** | `TRUE` or `FALSE`. |
| **Array**   | A rectangular grid of values. When an array is a cell's result it *spills* into the neighbouring cells (see `PLAN.md` §6.2.2). |
| **Empty**   | A blank cell. Coerces to `0` in arithmetic and `""` in text. |
| **Error**   | A computation that could not produce a value — see [Errors](#errors). |

---

## Literals

```text
42            2.5           0.5            Number literals
"hello"       ""                           Text literals
TRUE          FALSE                        Boolean literals
[1, 2, 3]                                  a 1-D array literal
[[1, 2], [3, 4]]                           a 2-D array literal
[]                                         the empty array
```

---

## Operators

| Group           | Operators            | Notes |
|-----------------|----------------------|-------|
| Arithmetic      | `+` `-` `*` `/` `^`  | `^` is exponentiation. |
| Unary           | `-` `+`              | Prefix sign. |
| Concatenation   | `&`                  | Joins two values as text. |
| Comparison      | `=` `<>` `<` `>` `<=` `>=` | Yield a Boolean. `=` is equality, `<>` is inequality. |

**Precedence**, loosest to tightest:

1. comparison `=  <>  <  >  <=  >=`
2. concatenation `&`
3. additive `+  -`
4. multiplicative `*  /`
5. exponentiation `^`
6. unary `-  +`

`^` is **right-associative** (`2 ^ 3 ^ 2` is `2 ^ (3 ^ 2)`); every other
binary operator is left-associative. Unary `-` binds *tighter* than `^`,
so `-2 ^ 2` is `(-2) ^ 2`.

```text
1 + 2 * 3        => 7        (1 + 2) * 3      => 9
7 - 2 - 1        => 4        10 / 4           => 2.5
2 ^ 3 ^ 2        => 512      -2 ^ 2           => 4
"a" & "b"        => "ab"
1 < 2            => TRUE     2 = 2            => TRUE
3 <> 4           => TRUE     4 >= 4           => TRUE
```

---

## Cell references and ranges

A bare address is a **cell reference**; two addresses joined by `:` are a
**range**. Address syntax is lattice-specific — `A1` on a square sheet,
`H(q,r)` on a hex sheet (see `docs/carbide/addressing.md`).

```text
A1               the value of cell A1
A1 + A2          arithmetic across cells
SUM(A1:A3)       a function over a range
```

A reference to a blank cell reads as **Empty** (which is `0` in
arithmetic). Cell references are tracked in the workbook's dependency
graph, so editing a cell recomputes everything downstream.

---

## Functions

A function call is `NAME(arg, …)`. Names are case-sensitive and
upper-case. Carbide ships ~95 built-ins:

- **Aggregate** — `SUM` `AVERAGE`/`AVG` `COUNT` `COUNTA` `MIN` `MAX`
- **Logical** — `IF` `AND` `OR` `NOT` `XOR` `IFS` `SWITCH` `IFERROR`
  `IFNA` `ISBLANK` `ISNUMBER` `ISTEXT` `ISERROR` `TRUE` `FALSE`
- **Math** — `ABS` `ROUND` `CEILING` `FLOOR` `MOD` `POWER` `SQRT` `EXP`
  `LN` `LOG` `INT` `TRUNC` `SIGN` `PI`
- **Text** — `LEN` `UPPER` `LOWER` `PROPER` `TRIM` `LEFT` `RIGHT` `MID`
  `REPT` `EXACT` `SUBSTITUTE` `FIND` `SEARCH` `REPLACE` `CONCAT`
  `CONCATENATE` `TEXTJOIN` `JOIN` `SPLIT` `TEXTSPLIT` `VALUE` `TEXT`
  `CHAR` `CODE`
- **Statistics** — `STDEV` `STDEVP` `VAR` `VARP` `MEDIAN` `MODE`
  `PERCENTILE` `QUARTILE` `RANK` `CORREL` `COVAR` `COVARP` `COVARS`
  `SLOPE` `INTERCEPT` `FORECAST` `RSQ` (each takes one array/range)
- **Lookup** — `VLOOKUP` `HLOOKUP` `INDEX` `MATCH` `CHOOSE`
- **Dynamic arrays** — `UNIQUE` `COUNTUNIQUE` `SORT` `FILTER` `SEQUENCE`
  `TAKE` `DROP` `TRANSPOSE` `FLATTEN` `SETUNION` `SETDIFF` `SETINTERSECT`
  `SETSYMDIFF` `IN` `COUNTIF` `MAP` `REDUCE` `SCAN` `BYROW` `BYCOL`
  `MAKEARRAY`
- **Neighbourhood** (lattice-aware) — `NEIGHBORS` `RADIUS`
- **Binding & lambda** — `LET` `LAMBDA` `LETREC` (see below)

```text
SUM(1, 2, 3)        => 6        AVERAGE(2, 4, 6)   => 4
MIN(5, 1, 3)        => 1        MAX(5, 1, 3)       => 5
COUNT(1, 2, 3)      => 3        ABS(-7)            => 7
SQRT(16)            => 4        POWER(2, 8)        => 256
MOD(10, 3)          => 1        ROUND(1.23456, 2)  => 1.23
SIGN(-3)            => -1
IF(TRUE, 10, 20)    => 10       IF(FALSE, 10, 20)  => 20
AND(TRUE, TRUE)     => TRUE     OR(FALSE, TRUE)    => TRUE
NOT(TRUE)           => FALSE    IFERROR(1 / 0, 99) => 99
LEN("hello")        => 5        UPPER("hi")        => "HI"
LEFT("hello", 3)    => "hel"    RIGHT("hello", 2)  => "lo"
MEDIAN([1, 2, 3])   => 2
SEQUENCE(5)         => an array UNIQUE([1, 2, 2, 3]) => an array
```

Unknown function names are an error: `BOGUS(1)` → [`#NAME?`](#errors).

---

## Binding and lambda forms

`LET`, `LAMBDA`, and `LETREC` give Carbide names, first-class functions,
and recursion.

- **`LET(name, value, …, body)`** binds one or more `name = value` pairs,
  then evaluates `body` with them in scope.
- **`LAMBDA(param, …, body)`** is an anonymous function. Apply it by
  following it with `(args)`.
- **`LETREC(name, lambda, body)`** is like `LET` but the bound lambda may
  call *itself* — the foundation for recursion.

The higher-order array functions (`MAP`, `REDUCE`, `SCAN`, `BYROW`,
`BYCOL`, `MAKEARRAY`) take a `LAMBDA` as an argument.

```text
LET(x, 10, x + 5)                                       => 15
LET(x, 3, y, 4, x * y)                                  => 12
(LAMBDA(x, x * 2))(21)                                  => 42
LETREC(fact, LAMBDA(n, IF(n <= 1, 1, n * fact(n - 1))), fact(5))  => 120
SUM(MAP([1, 2, 3], LAMBDA(x, x * x)))                   => 14
REDUCE(0, [1, 2, 3, 4], LAMBDA(a, x, a + x))            => 10
```

---

## Errors

A formula that cannot produce a value evaluates to an **error value**.
A cell downstream of an error generally inherits it. The error kinds:

| Error            | Meaning |
|------------------|---------|
| `#REF!`          | A reference to a cell that cannot be resolved. |
| cycle            | The cell participates in a dependency cycle. |
| `#DIV/0!`        | Division by zero. |
| `#NUM!`          | A numeric result overflowed or is not a number (e.g. `SQRT(-1)`). |
| `#VALUE!`        | A type mismatch — e.g. text where a number was required. |
| `#NAME?`         | An unknown function or unbound name. |
| compile / lang   | An engine-level parse or compile failure. |
| timeout          | A formula exceeded its execution budget. |
| spill            | An array result collided with a non-empty cell. |

```text
1 / 0           => #DIV/0!
SQRT(-1)        => #NUM!
BOGUS(1)        => #NAME?  (unknown function)
x               => #NAME?  (unbound bare identifier)
```

---

## Limits

Formula **nesting depth is capped at 128 levels**. A formula nested deeper
than that is rejected with a parse error rather than risking a stack
overflow during evaluation — well beyond any hand-written formula
(Excel's classic limit was 64).

---

## See also

- `PLAN.md` §6.2 — the formula-engine architecture.
- `docs/carbide/addressing.md` — lattice-specific address syntax.
- `docs/all-rust-roadmap.md` — the Carbide → Rust transpiler.
