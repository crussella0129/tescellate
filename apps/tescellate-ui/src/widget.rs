//! Boolean toggle cells — a square-sheet cell can be turned into a
//! clickable checkbox whose value is `TRUE` or `FALSE`.
//!
//! Excel and Sheets have checkbox cells; Tescellate's wider plan is for
//! cells to become first-class UI controls (switches, buttons). v24 is
//! the boolean core: the set of toggle cells, plus the two pure
//! conversions between a cell's value and its checkbox state.
//!
//! No egui and no engine here, so `cargo test` covers the module.

use std::collections::HashSet;

use tescellate_core::CellValue;

/// The square-sheet cells that render as boolean toggles, by `(col, row)`.
#[derive(Debug, Clone, Default)]
pub struct Widgets {
    toggles: HashSet<(u32, u32)>,
}

impl Widgets {
    /// Whether `cell` is a toggle.
    pub fn is_toggle(&self, cell: (u32, u32)) -> bool {
        self.toggles.contains(&cell)
    }

    /// Make `cell` a toggle (`on`) or revert it to an ordinary cell.
    pub fn set_toggle(&mut self, cell: (u32, u32), on: bool) {
        if on {
            self.toggles.insert(cell);
        } else {
            self.toggles.remove(&cell);
        }
    }

    /// Whether no cell is a toggle — lets the renderer skip its pass.
    pub fn is_empty(&self) -> bool {
        self.toggles.is_empty()
    }

    /// How many cells are toggles.
    pub fn count(&self) -> usize {
        self.toggles.len()
    }
}

/// A toggle cell's checkbox state — checked only when the cell currently
/// holds boolean `TRUE`. Every other value (a number, text, empty, …)
/// reads as unchecked.
pub fn bool_state(value: &CellValue) -> bool {
    matches!(value, CellValue::Bool(true))
}

/// The source to write for a checkbox state — a literal the formula
/// engine parses straight back to a boolean.
pub fn bool_source(checked: bool) -> &'static str {
    if checked {
        "TRUE"
    } else {
        "FALSE"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_and_query_toggle_cells() {
        let mut w = Widgets::default();
        assert!(w.is_empty());
        w.set_toggle((2, 3), true);
        w.set_toggle((5, 1), true);
        assert!(w.is_toggle((2, 3)));
        assert!(!w.is_toggle((0, 0)));
        assert_eq!(w.count(), 2);
        assert!(!w.is_empty());
        // Turning a cell off removes it.
        w.set_toggle((2, 3), false);
        assert!(!w.is_toggle((2, 3)));
        assert_eq!(w.count(), 1);
    }

    #[test]
    fn setting_the_same_cell_twice_is_idempotent() {
        let mut w = Widgets::default();
        w.set_toggle((1, 1), true);
        w.set_toggle((1, 1), true);
        assert_eq!(w.count(), 1);
    }

    #[test]
    fn bool_state_is_true_only_for_boolean_true() {
        assert!(bool_state(&CellValue::Bool(true)));
        assert!(!bool_state(&CellValue::Bool(false)));
        assert!(!bool_state(&CellValue::Empty));
        assert!(!bool_state(&CellValue::Number(1.0)));
        assert!(!bool_state(&CellValue::Integer(1)));
        assert!(!bool_state(&CellValue::Text("TRUE".into())));
    }

    #[test]
    fn bool_source_round_trips_the_checkbox_state() {
        assert_eq!(bool_source(true), "TRUE");
        assert_eq!(bool_source(false), "FALSE");
    }
}
