//! Aggregate statistics over a selection — what the status bar shows.
//!
//! Pure: it takes a slice of `CellValue` and returns a [`Stats`], with no
//! egui and no engine, so it is exercised by ordinary `cargo test`.

use tescellate_core::CellValue;

/// Summary stats for a set of cell values — Excel's status-bar set.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stats {
    /// Sum of the numeric cells.
    pub sum: f64,
    /// How many of the cells held a number.
    pub count: usize,
    /// How many of the cells were non-empty — numeric or not.
    pub nonempty: usize,
    /// Mean of the numeric cells — `None` when none were numeric.
    pub average: Option<f64>,
    /// Smallest numeric value — `None` when none were numeric.
    pub min: Option<f64>,
    /// Largest numeric value — `None` when none were numeric.
    pub max: Option<f64>,
}

/// Aggregate the numeric cells among `values`. Non-numeric cells — text,
/// blanks, booleans, errors — are ignored, as Excel's status bar does.
pub fn selection_stats(values: &[CellValue]) -> Stats {
    let mut sum = 0.0;
    let mut count = 0usize;
    let mut nonempty = 0usize;
    let mut min: Option<f64> = None;
    let mut max: Option<f64> = None;
    for value in values {
        if !matches!(value, CellValue::Empty) {
            nonempty += 1;
        }
        let number = match value {
            CellValue::Number(n) => Some(*n),
            CellValue::Integer(i) => Some(*i as f64),
            _ => None,
        };
        if let Some(n) = number {
            sum += n;
            count += 1;
            min = Some(min.map_or(n, |m| m.min(n)));
            max = Some(max.map_or(n, |m| m.max(n)));
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
        nonempty,
        average,
        min,
        max,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_selection_has_no_numbers() {
        let s = selection_stats(&[]);
        assert_eq!(s.count, 0);
        assert_eq!(s.nonempty, 0);
        assert_eq!(s.sum, 0.0);
        assert_eq!(s.average, None);
        assert_eq!(s.min, None);
        assert_eq!(s.max, None);
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
        // Text and Bool are non-empty even though they are not numeric.
        assert_eq!(s.nonempty, 2);
        assert_eq!(s.average, None);
        assert_eq!(s.min, None);
        assert_eq!(s.max, None);
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

    #[test]
    fn min_and_max_track_the_numeric_extremes() {
        let values = [
            CellValue::Number(7.0),
            CellValue::Integer(-3),
            CellValue::Text("skip".to_string()),
            CellValue::Number(12.5),
        ];
        let s = selection_stats(&values);
        assert_eq!(s.min, Some(-3.0));
        assert_eq!(s.max, Some(12.5));
        // A single numeric cell is both the min and the max.
        let one = selection_stats(&[CellValue::Number(4.0)]);
        assert_eq!(one.min, Some(4.0));
        assert_eq!(one.max, Some(4.0));
    }

    #[test]
    fn nonempty_counts_filled_cells_numeric_or_not() {
        let values = [
            CellValue::Number(1.0),
            CellValue::Empty,
            CellValue::Text("x".to_string()),
            CellValue::Empty,
            CellValue::Bool(false),
        ];
        let s = selection_stats(&values);
        // Three cells are filled; only one of them is numeric.
        assert_eq!(s.nonempty, 3);
        assert_eq!(s.count, 1);
    }
}
