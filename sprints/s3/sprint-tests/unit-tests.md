# Sprint 3 Unit Tests

## T-301 (lexer)
- `lexes_dotted_identifier`: `STDEV.P` / `COVARIANCE.S` → single `Token::Ident` each. **pass**
- `dot_after_digit_still_lexes_a_number`: `2.5` → `Token::Number(2.5)` (regression guard for float decimals). **pass**

## T-302 (XLOOKUP)
- `xlookup_exact_match_default`: `XLOOKUP("b", ["a","b","c"], [10,20,30])` → 20. **pass**
- `xlookup_returns_if_not_found_when_provided`: miss returns the fallback value. **pass**
- `xlookup_errors_when_missing_and_no_default`: miss without fallback errors. **pass**
- `xlookup_match_mode_exact_or_next_larger`: match_mode=1 picks next-larger. **pass**
- `xlookup_match_mode_exact_or_next_smaller`: match_mode=-1 picks next-smaller. **pass**
- `xlookup_search_mode_minus_one_finds_last_match`: search_mode=-1 walks last→first. **pass**
- `xlookup_wildcard_match_mode_is_deferred`: match_mode=2 errors cleanly. **pass**

## T-303 (dotted aliases)
- `stdev_dot_p_aliases_stdevp`, `stdev_dot_s_aliases_stdev`,
  `var_dot_p_aliases_varp`, `var_dot_s_aliases_var`,
  `mode_sngl_aliases_mode`. All **pass**.

## Run summary
- `cargo test --workspace`: all green (23 result sections, 0 failures).
- New tests across this sprint: 14 (2 lexer + 7 XLOOKUP + 5 alias).
