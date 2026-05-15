# Carbide — Grammar

This page is the canonical reference for Carbide's surface syntax — what the lexer produces, what the parser builds, what nodes appear in the AST, and how `=`-vs-literal works.

Source files (Rust):

- Lexer: `crates/tescellate-formula/src/excellite/lex.rs`
- Parser: `crates/tescellate-formula/src/excellite/parse.rs`
- AST: `crates/tescellate-formula/src/excellite/ast.rs`

The parser is a hand-rolled Pratt parser. Cell references and ranges are carried in the AST as opaque address strings; the parser does not know which lattice the sheet uses. See [addressing.md](addressing.md) for the addressing layer.

## Literal vs formula

This is the single most consequential decision in the language and worth stating up front.

When a cell receives input, the orchestrator in `tescellate-formula::engine::set_cell` looks at the source string:

| Input starts with `=` | Treatment |
|---|---|
| Yes | Parsed as a Carbide formula. Anything that doesn't lex+parse cleanly stores as `CellError::Lang` / `CellError::Value`. |
| No | Stored as a literal value. The string is parsed *only* enough to decide: integer numeric, fractional numeric, `TRUE`/`FALSE` (case-insensitive), or text. The original source is preserved verbatim so re-clicking the cell shows what the user typed, not a re-formatted version. |

This is the rule Excel and Google Sheets use, and it's what makes the cell "type some text into B7" interaction work without needing a separate text-cell affordance.

## Lexer

The lexer is in `lex.rs`. It is bytewise (ASCII-aware), single-pass, and emits `Spanned<Token>` values with byte offsets.

### Tokens

```
Number   /[0-9]+(\.[0-9]+)?([eE][+-]?[0-9]+)?/
Str      "..." (Excel-style "" inside doubles itself)
Ident    /[A-Za-z_][A-Za-z0-9_]*/      (uppercased on emission)
CellRef  /[A-Za-z]+[0-9]+/             (uppercased on emission)
LParen   "("        RParen   ")"
LBracket "["        RBracket "]"
Comma    ","        Colon    ":"
Plus     "+"        Minus    "-"
Star     "*"        Slash    "/"
Caret    "^"        Amp      "&"
Eq       "="
NotEq    "<>"       Lt   "<"   Gt   ">"
LtEq     "<="       GtEq ">="
```

### Disambiguating cell refs and identifiers

The lexer looks at the *follow* character to choose between `CellRef` and `Ident`:

- `A1` — letters followed by digits → `CellRef("A1")`.
- `SUM` followed by `(`/non-digit → `Ident("SUM")`.
- `LOG10` — letters followed by digits → `CellRef("LOG10")`. **There is no way to use `LOG10` as a function name today.** This is a known sharp edge; if function names that end in digits become important we will revisit.
- Lowercase `a1` → `CellRef("A1")` (lexer uppercases).

### String escapes

Excel-style: `""` inside a string literal is one `"`. There are no `\n` / `\t` / `\\` escapes.

```excel
="he said ""hi"""       → he said "hi"
```

### Number lexing

Standard IEEE-754 decimal with optional exponent: `1`, `3.14`, `1.5e-3`, `1E10`. The literal `.` alone is rejected as a lex error.

### What the lexer does NOT do

- It does not understand `$` (no absolute references yet).
- It does not lex sheet-prefixed addresses (`Sheet1!A1`).
- It does not lex dots inside identifiers (so `STDEV.P` would lex as `Ident("STDEV") . Number(.P)` and fail — use `STDEVP`).

## AST

`Expr` is the only public AST type:

```rust
pub enum Expr {
    Number(f64),
    Str(String),
    Bool(bool),
    CellRef(String),
    Range(String, String),
    Array(Vec<Vec<Expr>>),
    Var(String),
    Apply(Box<Expr>, Vec<Expr>),
    Unary(UnaryOp, Box<Expr>),
    Binary(BinaryOp, Box<Expr>, Box<Expr>),
    Call(String, Vec<Expr>),
}
```

- `Number` / `Str` / `Bool` — literal values.
- `CellRef("A1")` / `Range("A1", "B5")` — lattice-opaque address strings.
- `Array([[…],…])` — array literal. Rows × cols; the parser enforces rectangular shape.
- `Var("X")` — bare identifier. Always resolves through the lexical environment at eval time. Emitted whenever an `Ident` is not followed by `(`.
- `Apply(callee, args)` — postfix application of any expression. The parser wraps every primary in `Apply` for each `(args)` suffix it sees, so `Y(F)(5)` parses as `Apply(Apply(Var("Y"), [Var("F")]), [Number(5.0)])`.
- `Unary` / `Binary` — operators.
- `Call(name, args)` — Ident-followed-by-`(`. The eval-time `Call` arm tries the function registry first; on miss, falls back to `Var(name)` + apply, so a LET-bound lambda can be called the same way.

### Why `Call` and `Apply` are distinct

`Call` carries a *name*. Several `FuncImpl`s — `IF`, `IFS`, `SWITCH`, `AND`, `OR`, `LET`, `LAMBDA`, `LETREC`, `MAP`, `REDUCE` — control the evaluation order of their arguments (short-circuiting, lazy bodies, per-element re-application). They are registered as `FuncImpl`s that take **unevaluated** `&[Expr]` arguments. `Call` preserves this contract.

`Apply` carries an *expression* in the callee slot. Its args are evaluated eagerly into `CellValue`s and the resulting `CellValue::Function` is invoked. If `Apply` were the only shape, `IF(cond, then_expr, else_expr)` would eagerly evaluate both branches before the conditional ran. Splitting the two preserves Excel/Sheets semantics.

## Parser

The parser is a Pratt parser (operator-precedence climbing). It is in `parse.rs`. The entry point `parse(src)` returns `Result<Expr, ParseError>`.

### Operator precedence (low → high)

| Level | Operators | Associativity |
|---|---|---|
| 1 | `=`  `<>`  `<`  `>`  `<=`  `>=` | left |
| 2 | `&` (concat) | left |
| 3 | `+`  `-` (binary) | left |
| 4 | `*`  `/` | left |
| 5 | `^` (exponent) | **right** |
| 6 | `+`  `-` (unary) | n/a (prefix) |
| 7 | `(args)` (apply-suffix) | left |
| 8 | `(literal | ref | range | array | call | var | (expr))` | — |

A consequence: `-2^2` is `4`, not `-4`. The unary minus binds tighter than `^`. This is Excel's rule and is a frequent surprise for users coming from mathematical notation.

### Apply-suffix loop

After parsing every prefix expression, the parser runs:

```text
while peek == LParen:
    bump '('
    args = comma-separated parse_expr(0)*
    expect ')'
    lhs = Apply(lhs, args)
```

This runs **before** the binary-operator loop. The consequence is that `F(x)(y) + 1` parses as `Apply(Apply(F, [x]), [y]) + 1`, not `Apply(F, [x])(y + 1)`. Left-associative application matches how every functional language readers may know does it.

### Array literals

Square brackets, comma-separated:

| Syntax | Shape | Notes |
|---|---|---|
| `[]` | 0×0 | Empty array. |
| `[a, b, c]` | 1×3 (row) | A row of three elements. |
| `[a]` | 1×1 | Single-element row. |
| `[[a, b], [c, d]]` | 2×2 | 2-D. Outer rows; inner cells. |
| `[[a, b], [c, d, e]]` | — | **Parse error** — ragged 2-D rejected at parse time. |
| `[A1, B3, C5]` | 1×3 | "Cell-list array" — same as any other array but elements happen to be `CellRef`s. Resolved at eval time. |

The decision between 1-D and 2-D is by inspecting the first element: if it is itself a 1-row `Array`, the whole expression is 2-D and every sibling must also be a 1-row array of the same width.

Elements can be any expression — literals, refs, ranges, calls, lambdas. `[SUM(A1:A5), B1, "hello"]` is valid.

### Ranges

`<addr> : <addr>` produces `Expr::Range(addr1, addr2)`. Only the parser knows ranges are different from cell-list arrays; downstream code treats them similarly through the `flatten()` helper.

A range outside a function call (e.g., a bare `A1:B5` as the whole formula) evaluates to `EvalError::Value`. Functions that take ranges (every aggregate, `MAP`, `REDUCE`, etc.) expand them through `flatten()`.

### Parse errors

`ParseError` carries `message: String` and `pos: usize` (byte offset into the source). Errors propagate up through `EvalError::Value` (and ultimately `CellError::Lang`) on the way to the renderer.

## Reserved words and keywords

There are no reserved words. Function names like `LAMBDA`, `LET`, `IF`, `MAP` are ordinary entries in the function registry; you could re-register them or shadow them with LET bindings. **You can shadow `LAMBDA` itself** in a `LET` — it'd be a footgun but the language doesn't stop you.

`TRUE` and `FALSE` are recognized in `parse_prefix` *before* the bare-identifier fallback, so they always lex to `Expr::Bool` regardless of what's in the lexical environment.

## Grammar (EBNF-ish)

For the formally inclined:

```ebnf
formula      = ["="] expr;
expr         = apply_expr {binop apply_expr};
apply_expr   = prefix {"(" arglist ")"};
prefix       = "+" expr
             | "-" expr
             | primary;
primary      = NUMBER | STRING | BOOL_LIT
             | CELLREF [":" CELLREF]
             | IDENT ["(" arglist ")"]               (* IDENT → Call if "(" follows, else Var *)
             | "(" expr ")"
             | "[" array_literal "]";
arglist      = [expr {"," expr}];
array_literal= /* empty */ | array_row {"," array_row} ;
array_row    = expr | "[" expr {"," expr} "]";
binop        = "=" | "<>" | "<" | ">" | "<=" | ">="
             | "&"
             | "+" | "-" | "*" | "/" | "^";
```

The `array_row` production has a quirk: a 1-D array `[a, b, c]` is parsed as a single `array_row` containing the comma-separated `expr`s. A 2-D array is decided post-hoc — if the first `array_row` is itself a single-element-row `[…]`, the whole literal is 2-D and every other `array_row` must be the same shape.

## Examples

```excel
=A1                                  CellRef("A1")
=A1:B5                               Range("A1", "B5")
=SUM(A1:B5)                          Call("SUM", [Range("A1","B5")])
=SUM(A1, B2)                         Call("SUM", [CellRef("A1"), CellRef("B2")])
="hello"                             Str("hello")
=TRUE                                Bool(true)
=42                                  Number(42.0)
=42                                  (without leading =, stored as literal Integer 42)
=[1, 2, 3]                           Array([[Number(1), Number(2), Number(3)]])
=[[1,2],[3,4]]                       Array([[…1,2…],[…3,4…]])
=LET(x, 10, x+5)                     Call("LET", [Var("X"), Number(10.0), Binary(Add, Var("X"), Number(5.0))])
=(LAMBDA(x, x*2))(5)                 Apply(Call("LAMBDA", [Var("X"), Binary(Mul, Var("X"), Number(2.0))]), [Number(5.0)])
=Y(F)(5)                             Apply(Apply(Var("Y"), [Var("F")]), [Number(5.0)])
=-2^2                                Binary(Pow, Unary(Neg, Number(2.0)), Number(2.0))    → 4
```

## What's deferred

| | Status |
|---|---|
| Absolute refs (`$A$1`) | Not parsed today. |
| Cross-sheet refs (`Sheet1!A1`) | Not parsed today. |
| R1C1 addressing | Not parsed today. |
| Dotted identifiers (`STDEV.P`) | Lex error today; use `STDEVP`. Will land when needed. |
| Named ranges | Not parsed; use `LET` for the same effect within one formula. |
| Multi-statement formulas | Not parsed; one expression per cell. |

See [addressing.md](addressing.md) for how the address-shape side of the parser will evolve when hex / triangle / Voronoi / drawn tilings arrive.
