# Sprint 3 Test Report

## Summary
- Unit tests: all green. 14 net-new this sprint (2 lexer + 7 XLOOKUP + 5 alias).
- Integration tests: 0 net-new (the new function tests in `reference_examples.rs` ARE the integration surface — they parse + eval real Carbide).
- E2E tests: N/A (engine-only sprint).
- CI status: green. PR #178 passed all 7 checks. Squash-merged as `38fd3e5`.

## Failures
None.

## Technical Debt Identified
- **XLOOKUP wildcard match (`match_mode = 2`)** is documented but deferred — needs a glob/regex backend. The current implementation errors cleanly so users get a clear "not yet supported" message instead of silently mis-matching.
- **XLOOKUP binary search (`search_mode ±2`)** accepts the parameter but linearly scans. The asymptotic improvement only matters on large workbooks past launch.
- **XLOOKUP "return a row of a 2D table"** — sprint 3 ships parallel-array semantics (1D lookup + 1D result, equal-length). The 2D return variant is a follow-up that fits naturally when the cell-reference-return abstraction lands (alongside OFFSET / INDIRECT).
- **OFFSET / INDIRECT** still pending — both need cell-reference-shape values that Carbide doesn't yet model as first-class.

## Coverage Observations
- The lexer dotted-identifier extension has explicit regression coverage for both the new behavior (STDEV.P → single ident) and the unchanged behavior (3.14 → number; 2.5 used in the actual test to dodge clippy's `approx_constant` lint).
- XLOOKUP covers all three implemented match-modes, the if_not_found path, both error paths (no fallback / wildcard not supported), and the reverse search direction. The 2D-result variant is uncovered (deferred).
- The eight dotted aliases are covered via pair-comparison tests (`STDEV.P([…]) ≈ STDEVP([…])`) — minimal but sufficient since the function pointers are literally the same.
