# Sprint 3 Research Report

## 1. Sprint Goal

Carbide Phase-2 hardening, scoped: ship **XLOOKUP** (the most-requested
Excel modern lookup) and **dotted-name function aliases**
(`STDEV.P` / `STDEV.S` / `VAR.P` / `VAR.S` / `COVARIANCE.P` /
`COVARIANCE.S` / `MODE.SNGL` / `RANK.EQ`). Pushes Carbide closer to the
launch brief's 95% Excel-coverage target without taking on the
cell-reference-return shape OFFSET / INDIRECT would require.

## 2. Existing Code Survey

| File | Relevance | Notes |
|------|-----------|-------|
| `crates/carbide-formula/src/excellite/funcs/lookup.rs` | high | Already has VLOOKUP, HLOOKUP, INDEX, MATCH, CHOOSE. Module doc comment says "XLOOKUP, OFFSET, INDIRECT come later when we have a cleaner cell-ref abstraction" — XLOOKUP itself returns values so we can land it now; only OFFSET/INDIRECT need the deferred ref-shape. Sprint 3 adds an `xlookup` function alongside the existing `vlookup`. |
| `crates/carbide-formula/src/excellite/funcs/stats.rs` | high | Has `STDEV`, `STDEVP`, `VAR`, `VARP`, `COVAR`, `COVARP`, `COVARS`, `MODE`, `RANK` already. Sprint 3 only adds the dotted-name aliases — same function pointers, new registry names. |
| `crates/carbide-formula/src/excellite/lex.rs` | medium | `lex_ident_or_ref` currently consumes `[A-Za-z]+[0-9A-Za-z_]*` for bare identifiers. The lexer must learn to accept `.UPPERCASE_RUN` continuation segments so `STDEV.P` lexes as one `Ident("STDEV.P")` rather than `Ident("STDEV") Dot Ident("P")`. Restrict the continuation to alphabetic chars after the dot to keep float-decimal parsing untouched. |
| `crates/carbide-formula/src/excellite/funcs/coerce.rs` | low | `compare`, `flatten`, `to_int`, `arity_range`, `arity_n` already exist. XLOOKUP needs `compare` (for match modes), `flatten` (for the column arrays), and `to_int` (for match_mode / search_mode). |
| `crates/carbide-formula/tests/reference_examples.rs` | medium | Where new function tests land; pattern is established (one #[test] per new function with handful of assertions). |

## 3. External Sources

- [Microsoft XLOOKUP signature](https://support.microsoft.com/en-us/office/xlookup-function-b7fd680e-6d10-43e6-84f9-88eae8bf5929) — `XLOOKUP(lookup_value, lookup_array, return_array, [if_not_found], [match_mode], [search_mode])`. match_mode: 0 (exact, default), -1 (exact-or-next-smaller), 1 (exact-or-next-larger), 2 (wildcard). search_mode: 1 (first→last, default), -1 (last→first), 2 (binary asc), -2 (binary desc). Sprint 3 implements 0 + -1 + 1 + the basic search modes; wildcard (2) deferred (needs glob support).
- [Microsoft "dotted name" aliases](https://support.microsoft.com/en-us/office/excel-functions-by-category-5f91f4e9-7b42-46d2-9bd1-63f26a86c0eb) — STDEV.S/STDEV.P, VAR.S/VAR.P, COVARIANCE.P/COVARIANCE.S, MODE.SNGL/MODE.MULT, RANK.EQ/RANK.AVG. The "modern" names introduced in Excel 2010 alongside the legacy STDEV/STDEVP/etc. Behavior matches the legacy names byte-for-byte; the dotted variants are pure aliases.

No external dep changes.

## 4. Risks, Unknowns, Dependencies

- **Risk — lexer dot continuation breaks ambiguous syntax:** the `.` is currently lex-claimed only by `lex_number` (decimal point / exponent sign). Allowing `.<letters>` after a bare ident is safe because the only place `.<letter>` follows an alphanumeric run is when the user wrote a dotted name. Float literals (`3.14`) start with a digit, not a letter, so they take the `lex_number` branch.
- **Risk — XLOOKUP wildcard (match_mode = 2)** needs a glob/regex engine. Deferred to a follow-up; sprint 3 errors with a clear message when `match_mode = 2` is passed.
- **Risk — XLOOKUP binary search (search_mode ±2)** assumes a sorted lookup_array. Sprint 3 implements it as a linear scan fallback (correct but not asymptotically optimal) and documents the deferral — the asymptotic improvement only matters on large workbooks, well past launch.
- **Risk — XLOOKUP `if_not_found`** — when the lookup misses, return `if_not_found` if provided, otherwise `#N/A`. Carbide today returns `EvalError::Ref` for VLOOKUP misses; XLOOKUP needs the `if_not_found` substitution path before the error.
- **Unknown — does `STDEV.P` collide with cell-address recognition?** No: cell-address recognition runs at the `Expr::CellRef("A1")`-shape level, after the lexer hands back an `Ident`. The dotted ident shape (`STDEV.P`) doesn't match the `Letters Digits` cell-address pattern.

## 5. Recommended Approach

**Primary — three commits, one PR.**

1. **T-301: Lexer dotted-identifier extension.** In
   `lex_ident_or_ref`, after the alphanumeric run, while the next char
   is `.` followed by an ASCII alphabetic, consume `.<letters>`
   (uppercased into the ident). One unit test in `lex::tests` for
   `STDEV.P → Ident("STDEV.P")`.
2. **T-302: XLOOKUP.** Add `xlookup` to `lookup.rs` with the full Excel
   signature. Implement match_mode 0 (exact), -1 (exact-or-smaller),
   1 (exact-or-larger). Implement search_mode 1 (first→last) and -1
   (last→first); search_mode ±2 falls back to the linear scan but
   accepts the parameter. match_mode 2 (wildcard) errors with a
   clear "wildcard match not yet supported" message. `if_not_found`
   path: when `args.len() >= 4` and lookup misses, return the
   evaluated `args[3]`; otherwise error with `#N/A`. Register
   `XLOOKUP`.
3. **T-303: Dotted-name aliases.** In `stats.rs::register`, add eight
   `r.add` calls: `STDEV.P → stdevp`, `STDEV.S → stdev`,
   `VAR.P → varp`, `VAR.S → var`, `COVARIANCE.P → covarp`,
   `COVARIANCE.S → covars`, `MODE.SNGL → mode`, `RANK.EQ → rank`.
4. **T-304: Tests.** Add to `reference_examples.rs`:
   - `xlookup_exact_match_default`
   - `xlookup_returns_if_not_found_when_provided`
   - `xlookup_errors_when_missing_and_no_default`
   - `xlookup_match_mode_exact_or_next_larger`
   - `xlookup_match_mode_exact_or_next_smaller`
   - `xlookup_search_mode_minus_one_finds_last_match`
   - `stdev_dot_p_aliases_stdevp`, `stdev_dot_s_aliases_stdev`,
     `var_dot_p_aliases_varp`, `var_dot_s_aliases_var`,
     `mode_sngl_aliases_mode`.
5. **T-305: CI gate + PR `carbide-v147-xlookup-dotted-names`.**

**Alternative considered — bundle OFFSET into this sprint.** Rejected: OFFSET returns a range/cell reference, which is a deeper engine change. Lookup's deferral comment is explicit about this. Sprint 4 (or later) can tackle the cell-ref abstraction and OFFSET/INDIRECT together.

**Rationale:** XLOOKUP is the highest-leverage missing function (modern Excel users reach for it first); the dotted aliases are cheap and remove a class of "why doesn't this work?" papercuts. Both stay engine-side — no UI surface to update.

## Artifacts
None.
