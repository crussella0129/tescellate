//! Sorting cells by value.
//!
//! [`compare_values`] is the pure total order a column sort uses. No
//! egui and no engine mutation here, so `cargo test` covers it.

use carbide_core::CellValue;
use std::cmp::Ordering;

/// A coarse rank grouping [`CellValue`] variants for sorting: numbers
/// first, then text, then booleans, then everything else (blank,
/// errors, …) — mirroring how spreadsheets order a mixed column.
fn rank(v: &CellValue) -> u8 {
    match v {
        CellValue::Number(_) | CellValue::Integer(_) => 0,
        CellValue::Text(_) => 1,
        CellValue::Bool(_) => 2,
        _ => 3,
    }
}

/// The numeric value of `v` — `0.0` for anything that is not a number.
fn as_number(v: &CellValue) -> f64 {
    match v {
        CellValue::Number(n) => *n,
        CellValue::Integer(i) => *i as f64,
        _ => 0.0,
    }
}

/// The total order a column sort uses: numbers (compared numerically)
/// before text (lexical) before booleans (`false` < `true`) before
/// everything else. Same-rank "other" values compare equal, so a
/// stable sort leaves them where they were.
pub fn compare_values(a: &CellValue, b: &CellValue) -> Ordering {
    rank(a).cmp(&rank(b)).then_with(|| match rank(a) {
        0 => as_number(a).total_cmp(&as_number(b)),
        1 => match (a, b) {
            (CellValue::Text(x), CellValue::Text(y)) => x.cmp(y),
            _ => Ordering::Equal,
        },
        2 => match (a, b) {
            (CellValue::Bool(x), CellValue::Bool(y)) => x.cmp(y),
            _ => Ordering::Equal,
        },
        _ => Ordering::Equal,
    })
}

/// The row order that sorts `keys`. `row_order(keys, ascending)[i]` is
/// the index, in the original `keys`, of the row that belongs at sorted
/// position `i` — so reordering a table's other columns through this
/// permutation moves whole rows together. The sort is stable: rows
/// whose keys compare equal keep their original relative order, in
/// either direction.
pub fn row_order(keys: &[CellValue], ascending: bool) -> Vec<usize> {
    let mut order: Vec<usize> = (0..keys.len()).collect();
    order.sort_by(|&a, &b| {
        let ord = compare_values(&keys[a], &keys[b]);
        if ascending {
            ord
        } else {
            ord.reverse()
        }
    });
    order
}

#[cfg(test)]
mod tests {
    use super::*;

    fn num(n: f64) -> CellValue {
        CellValue::Number(n)
    }
    fn int(i: i64) -> CellValue {
        CellValue::Integer(i)
    }
    fn text(s: &str) -> CellValue {
        CellValue::Text(s.to_string())
    }

    #[test]
    fn numbers_compare_numerically() {
        assert_eq!(compare_values(&num(2.0), &num(10.0)), Ordering::Less);
        assert_eq!(compare_values(&num(10.0), &num(2.0)), Ordering::Greater);
        // An Integer and a Number of equal value compare equal.
        assert_eq!(compare_values(&int(5), &num(5.0)), Ordering::Equal);
    }

    #[test]
    fn numbers_sort_before_text() {
        assert_eq!(compare_values(&num(999.0), &text("a")), Ordering::Less);
        assert_eq!(compare_values(&text("a"), &num(999.0)), Ordering::Greater);
    }

    #[test]
    fn text_compares_lexically() {
        assert_eq!(
            compare_values(&text("apple"), &text("banana")),
            Ordering::Less,
        );
        assert_eq!(
            compare_values(&text("zed"), &text("apple")),
            Ordering::Greater,
        );
    }

    #[test]
    fn bools_then_blanks_rank_last() {
        // false sorts before true.
        assert_eq!(
            compare_values(&CellValue::Bool(false), &CellValue::Bool(true)),
            Ordering::Less,
        );
        // Text outranks a boolean; a boolean outranks a blank cell.
        assert_eq!(
            compare_values(&text("z"), &CellValue::Bool(false)),
            Ordering::Less,
        );
        assert_eq!(
            compare_values(&CellValue::Bool(true), &CellValue::Empty),
            Ordering::Less,
        );
    }

    #[test]
    fn row_order_sorts_indices_by_their_keys() {
        let keys = [num(30.0), num(10.0), num(20.0)];
        // Ascending 10, 20, 30 — original indices 1, 2, 0.
        assert_eq!(row_order(&keys, true), vec![1, 2, 0]);
        // Descending reverses the order.
        assert_eq!(row_order(&keys, false), vec![0, 2, 1]);
    }

    #[test]
    fn row_order_is_stable_for_equal_keys() {
        // Indices 0 and 2 share the key 5; a stable sort keeps 0 before 2.
        let keys = [num(5.0), text("a"), num(5.0), text("b")];
        let asc = row_order(&keys, true);
        let p0 = asc.iter().position(|&i| i == 0).unwrap();
        let p2 = asc.iter().position(|&i| i == 2).unwrap();
        assert!(p0 < p2);
        // Equal keys do not flip when the sort is reversed either.
        let desc = row_order(&keys, false);
        let d0 = desc.iter().position(|&i| i == 0).unwrap();
        let d2 = desc.iter().position(|&i| i == 2).unwrap();
        assert!(d0 < d2);
    }

    #[test]
    fn row_order_of_no_keys_is_empty() {
        assert!(row_order(&[], true).is_empty());
    }
}
