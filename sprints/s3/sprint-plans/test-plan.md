Finalized - DO NOT EDIT

# Sprint 3 Test Plan

## Unit Tests

### T-301 (lexer)
- `lexes_dotted_identifier`: `lex("STDEV.P")` → `[Token::Ident("STDEV.P")]`.
- `dot_after_digit_still_lexes_a_number`: `lex("3.14")` → `[Token::Number(3.14)]` (regression guard).
- `cell_ref_does_not_swallow_following_dot`: `lex("A1.X")` → `[Token::CellRef("A1"), …]` (the `.X` becomes a separate token or fails to lex — either is acceptable; the test asserts the CellRef is intact).

### T-302 (XLOOKUP)
- `xlookup_exact_match_default`: `XLOOKUP("b", ["a","b","c"], [10,20,30])` → 20.
- `xlookup_returns_if_not_found_when_provided`: `XLOOKUP("z", ["a","b","c"], [10,20,30], "missing")` → "missing".
- `xlookup_errors_when_missing_and_no_default`: `XLOOKUP("z", ["a","b","c"], [10,20,30])` → error.
- `xlookup_match_mode_exact_or_next_larger`: `XLOOKUP(2.5, [1,2,3], [10,20,30], "", 1)` → 30.
- `xlookup_match_mode_exact_or_next_smaller`: `XLOOKUP(2.5, [1,2,3], [10,20,30], "", -1)` → 20.
- `xlookup_search_mode_minus_one_finds_last_match`: `XLOOKUP("a", ["a","b","a"], [1,2,3], "", 0, -1)` → 3.

### T-303 (dotted aliases)
- `stdev_dot_p_aliases_stdevp`: `STDEV.P([1,2,3,4,5])` ≈ `STDEVP([1,2,3,4,5])`.
- `stdev_dot_s_aliases_stdev`: `STDEV.S([1,2,3,4,5])` ≈ `STDEV([1,2,3,4,5])`.
- `var_dot_p_aliases_varp`: `VAR.P([1,2,3,4,5])` ≈ `VARP([1,2,3,4,5])`.
- `var_dot_s_aliases_var`: `VAR.S([1,2,3,4,5])` ≈ `VAR([1,2,3,4,5])`.
- `mode_sngl_aliases_mode`: `MODE.SNGL([1,2,2,3])` == `MODE([1,2,2,3])`.

## Integration Tests

The reference-examples test file IS the integration surface for new
Carbide functions. No additional harness needed.

## End-to-End Tests

- **Status:** N/A. Engine-only changes; no user-facing dialog or render
  surface lands in this sprint.
