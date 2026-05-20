Finalized - DO NOT EDIT

# Sprint 3 Build Plan

## Schema Tree

- **Sprint Goal:** v147 — XLOOKUP + dotted-name aliases (STDEV.P/S, VAR.P/S, COVARIANCE.P/S, MODE.SNGL, RANK.EQ).
  - **Component A — Lexer**
    - T-301: `lex_ident_or_ref` accepts `.<letters>` continuations after the alphanumeric run. Unit test.
  - **Component B — XLOOKUP**
    - T-302: `xlookup` function in `lookup.rs`; register as `XLOOKUP`.
  - **Component C — Dotted aliases**
    - T-303: Eight `r.add` calls in `stats.rs::register` to alias the dotted names to their existing implementations.
  - **Component D — Tests + ship**
    - T-304: Reference-examples tests for XLOOKUP + aliases.
    - T-305: CI gate + open + merge PR `carbide-v147-xlookup-dotted-names`.

## Execution Sequence

### T-301: Lexer dotted-identifier extension.
- **Touches:** `crates/tescellate-formula/src/excellite/lex.rs`
- **Depends on:** (none)
- **Success criterion:** After the alphanumeric/underscore run in `lex_ident_or_ref`, loop while next bytes are `b'.'` followed by an ASCII alphabetic char; consume `.<letters>` (uppercased into the ident). New unit test `lexes_dotted_identifier` asserts `lex("STDEV.P")` produces `[Token::Ident("STDEV.P")]`; existing tests still pass (float `3.14` keeps lexing as a Number).
- **Notes:** Only allow letters (no digits) after the dot — that's enough for Excel's dotted-name conventions and keeps the rule predictable.

### T-302: XLOOKUP.
- **Touches:** `crates/tescellate-formula/src/excellite/funcs/lookup.rs`
- **Depends on:** T-301
- **Success criterion:** `pub fn xlookup(args, ctx) -> Result<CellValue>` exists. Validates 3..=6 args via `arity_range`. Resolves: `needle = eval(args[0])`, `lookup = flatten(args[1])`, `result = to_array_2d(args[2])` (so 2D return arrays work), optional `if_not_found = eval(args[3])` else `EvalError::Ref("XLOOKUP: not found")`, `match_mode = to_int(eval(args[4])).unwrap_or(0)`, `search_mode = to_int(eval(args[5])).unwrap_or(1)`. Implements match_mode 0/-1/1 + search_mode 1/-1; ±2 falls through to linear scan (accepted, documented); 2 errors with "wildcard match not yet supported". Registered in `register()`.
- **Notes:** Match-not-found path: if `if_not_found` was provided (args[3]), return its value; else `Err(Ref(...))`. The lookup_array dimension and result_array dimension must match for the function to index the result correctly — if `result` is 2D with multiple cols, return the full row as an Array; otherwise return the scalar value.

### T-303: Dotted-name aliases.
- **Touches:** `crates/tescellate-formula/src/excellite/funcs/stats.rs`
- **Depends on:** T-301
- **Success criterion:** `register()` gains `r.add("STDEV.P", stdevp);` `r.add("STDEV.S", stdev);` `r.add("VAR.P", varp);` `r.add("VAR.S", var);` `r.add("COVARIANCE.P", covarp);` `r.add("COVARIANCE.S", covars);` `r.add("MODE.SNGL", mode);` `r.add("RANK.EQ", rank);`. Builds clean.
- **Notes:** Function-pointer aliases — no behavior change vs the legacy spellings.

### T-304: Tests in `reference_examples.rs`.
- **Touches:** `crates/tescellate-formula/tests/reference_examples.rs`
- **Depends on:** T-302, T-303
- **Success criterion:** Eleven new `#[test]` functions (six XLOOKUP, five alias) pass under `cargo test -p tescellate-formula`. Existing tests stay green.
- **Notes:** XLOOKUP tests need an array literal for the lookup_array + return_array, e.g. `XLOOKUP("b", ["a","b","c"], [1,2,3])`. The reference doc gets the new examples in a follow-up; this sprint adds tests + code.

### T-305: CI gate + PR.
- **Touches:** (verification + git)
- **Depends on:** T-301..T-304
- **Success criterion:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` all green. Branch pushed; PR opened; CI green; squash-merged.
- **Notes:** Engine-only sprint — no wasm build delta. Still verify `cargo build --target wasm32-unknown-unknown --manifest-path apps/tescellate-ui/Cargo.toml` for completeness.
