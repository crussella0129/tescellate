//! Shared fixtures for the differential tests — the formula corpus and the
//! cell table used by both `transpile_differential` (transpiled source run
//! as a binary) and `native_differential` (transpiled source compiled to a
//! native cdylib). One corpus, one source of truth.

/// Numeric cells the corpus reads. Both differential tests build a `MapCtx`
/// from this exact table on each side of the comparison.
pub const CELLS: &[(&str, f64)] = &[
    ("A1", 10.0),
    ("A2", 2.5),
    ("A3", 7.0),
    ("B1", 3.0),
    ("B2", 100.0),
    ("C1", 0.0),
];

/// The differential corpus — every Carbide construct the transpiler lowers
/// (v1–v4). Differential: expected values are never hard-coded; the
/// evaluation paths being compared must simply agree.
pub const CORPUS: &[&str] = &[
    // literals
    "1",
    "0",
    "2.5",
    r#""hello""#,
    r#""""#,
    "TRUE",
    "FALSE",
    // arithmetic + unary
    "1 + 2",
    "20 / 4",
    "2 ^ 10",
    "1 + 2 * 3",
    "(1 + 2) * 3",
    "-5",
    "-(3 + 4)",
    // runtime error
    "1 / 0",
    // concat + comparisons
    r#""a" & "b""#,
    "1 < 2",
    "2 = 2",
    "4 >= 4",
    // cell references
    "A1",
    "A2",
    "C1",
    "Z9",
    "A1 + A2",
    "B2 / A1",
    "A1 * A2 + A3",
    "-A1",
    "(A1 + A2) * B1",
    "A1 = 10",
    "A1 > B1",
    "Z9 + 1",
    r#"A1 & " items""#,
    // bare ranges (an error in both paths)
    "A1:A3",
    "B1:C1",
    // array literals
    "[1, 2, 3]",
    "[[1, 2], [3, 4]]",
    "[]",
    "[A1, A2, A3]",
    "[A1 + 1, A2 * 2]",
    "[[A1, B1], [A2, B2]]",
    // function calls: aggregates
    "SUM(1, 2, 3)",
    "SUM(A1, A2, A3)",
    "SUM(A1:A3)",
    "SUM([1, 2, 3])",
    "AVERAGE(A1:A3)",
    "COUNT(A1:A3)",
    "MIN(A1:A3)",
    "MAX(A1:A3)",
    "MAX(A1:C1)",
    // logical (short-circuiting)
    "IF(A1 > 5, 100, 0)",
    "IF(TRUE, 1, 2)",
    "AND(TRUE, FALSE)",
    "OR(FALSE, TRUE)",
    "NOT(TRUE)",
    "IFERROR(1 / 0, 99)",
    "IFS(FALSE, 1, TRUE, 2)",
    // math
    "ABS(-7)",
    "ROUND(3.14159, 2)",
    "SQRT(16)",
    "MOD(10, 3)",
    "POWER(2, 8)",
    // text
    r#"LEN("hello")"#,
    r#"UPPER("hi")"#,
    r#"LEFT("hello", 3)"#,
    // lookup / dynamic arrays
    "INDEX([[10, 20], [30, 40]], 2, 1)",
    r#"MATCH("b", ["a", "b", "c"])"#,
    "SEQUENCE(5)",
    "UNIQUE([1, 2, 2, 3])",
    // nested calls
    "SUM(A1:A3) + 1",
    "IF(A1 > 0, SUM(A1:A3), 0)",
    "ABS(A1 - B2)",
    "SUM(A1, AVERAGE(A2:A3))",
    // error paths
    "BOGUS(1)",
    "SQRT(-1)",
    // LET / LAMBDA / LETREC, lambdas, higher-order
    "LET(x, 10, x + 5)",
    "LET(x, 10, y, x * 2, x + y)",
    "LET(m, A1, m * 2)",
    "(LAMBDA(x, x * 2))(5)",
    "(LAMBDA(a, b, a + b))(3, 4)",
    "LET(f, LAMBDA(x, x + 1), f(7))",
    "LET(n, 100, g, LAMBDA(x, x + n), g(5))",
    "LETREC(fact, LAMBDA(n, IF(n <= 1, 1, n * fact(n - 1))), fact(5))",
    "MAP([1, 2, 3], LAMBDA(x, x * 2))",
    "REDUCE(0, [1, 2, 3, 4], LAMBDA(a, x, a + x))",
    "MAKEARRAY(2, 2, LAMBDA(r, c, r + c))",
    // bare variable (unbound at a cell's top level — an error, both paths)
    "x",
    // v6 — hardening: numeric, coercion, error-propagation, recursion edges
    "((((1 + 1) + 1) + 1) + 1)",
    "2 ^ 3 ^ 2",
    "2 ^ 2000",
    "2 ^ -10",
    "-0",
    "0 / 0",
    "100000 * 100000 * 100000",
    "TRUE + TRUE",
    "FALSE * 100",
    r#""5" + 3"#,
    r#""abc" + 1"#,
    r#"1 = "1""#,
    r#""" & "" & "x""#,
    "IF(1 / 0, 1, 2)",
    "SUM(1 / 0, 2, 3)",
    "1 + BOGUS(2)",
    r#"IFERROR(SQRT(-1), "caught")"#,
    "LETREC(fib, LAMBDA(n, IF(n <= 1, n, fib(n - 1) + fib(n - 2))), fib(12))",
    "LETREC(down, LAMBDA(n, IF(n <= 0, 0, n + down(n - 1))), down(50))",
    "REDUCE(1, [1, 2, 3, 4, 5, 6, 7, 8], LAMBDA(a, x, a * x))",
    "MAKEARRAY(4, 4, LAMBDA(r, c, IF(r = c, 1, 0)))",
    "MAP([1, 2, 3, 4, 5], LAMBDA(x, x * x))",
    "SUM(SUM(A1:A3), MAX(B1, B2), MIN(A1, A2))",
    "LET(xs, [1, 2, 3, 4], SUM(MAP(xs, LAMBDA(v, v * v))))",
    "LET(adder, LAMBDA(x, LAMBDA(y, x + y)), (adder(10))(5))",
];
