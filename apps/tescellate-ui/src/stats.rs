//! Aggregate statistics over a selection — what the status bar shows.
//!
//! Pure: it takes a slice of `CellValue` and returns a [`Stats`], with no
//! egui and no engine, so it is exercised by ordinary `cargo test`.

use tescellate_core::CellValue;

/// Summary stats for a set of cell values — Excel's status-bar trio.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stats {
    /// Sum of the numeric cells.
    pub sum: f64,
    /// How many of the cells held a number.
    pub count: usize,
    /// Mean of the numeric cells — `None` when none were numeric.
    pub average: Option<f64>,
}

/// Aggregate the numeric cells among `values`. Non-numeric cells — text,
/// blanks, booleans, errors — are ignored, as Excel's status bar does.
pub fn selection_stats(values: &[CellValue]) -> Stats {
    let mut sum = 0.0;
    let mut count = 0usize;
    for value in values {
        let number = match value {
            CellValue::Number(n) => Some(*n),
            CellValue::Integer(i) => Some(*i as f64),
            _ => None,
        };
        if let Some(n) = number {
            sum += n;
            count += 1;
        }
    }
    let average = if count > 0 {
        Some(sum / count as f64)
    } else {
        None
    };
    Stats {
        sum,
        count,
        average,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_selection_has_no_numbers() {
        let s = selection_stats(&[]);
        assert_eq!(s.count, 0);
        assert_eq!(s.sum, 0.0);
        assert_eq!(s.average, None);
    }

    #[test]
    fn non_numeric_cells_are_ignored() {
        let values = [
            CellValue::Text("hi".to_string()),
            CellValue::Empty,
            CellValue::Bool(true),
        ];
        let s = selection_stats(&values);
        assert_eq!(s.count, 0);
        assert_eq!(s.average, None);
    }

    #[test]
    fn sum_count_and_average_over_numbers() {
        let values = [
            CellValue::Number(10.0),
            CellValue::Number(20.0),
            CellValue::Number(30.0),
        ];
        let s = selection_stats(&values);
        assert_eq!(s.sum, 60.0);
        assert_eq!(s.count, 3);
        assert_eq!(s.average, Some(20.0));
    }

    #[test]
    fn integers_count_and_text_among_them_is_skipped() {
        let values = [
            CellValue::Integer(4),
            CellValue::Text("label".to_string()),
            CellValue::Number(6.0),
        ];
        let s = selection_stats(&values);
        assert_eq!(s.sum, 10.0);
        assert_eq!(s.count, 2);
        assert_eq!(s.average, Some(5.0));
    }
}
