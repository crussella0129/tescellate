//! The pure selected-range model for the square sheet.
//!
//! A [`Selection`] is an `anchor` cell and a `cursor` (the active cell).
//! The selected range is the inclusive rectangle spanning the two — a
//! single cell when they coincide. No egui and no engine here, so the
//! whole model is exercised by ordinary `cargo test`.

/// A zero-indexed `(column, row)` cell.
pub type Cell = (u32, u32);

/// A rectangular cell selection — an `anchor` and a `cursor`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    /// Where the selection was anchored (a plain move, or the start of a
    /// shift-extend / drag).
    pub anchor: Cell,
    /// The active cell — where editing happens and arrows move from.
    pub cursor: Cell,
}

impl Selection {
    /// A one-cell selection.
    pub fn single(cell: Cell) -> Self {
        Self {
            anchor: cell,
            cursor: cell,
        }
    }

    /// Move the whole selection to one cell — a plain arrow or click.
    pub fn collapse_to(&mut self, cell: Cell) {
        self.anchor = cell;
        self.cursor = cell;
    }

    /// Move the cursor while keeping the anchor — a shift-extend or drag.
    pub fn extend_to(&mut self, cell: Cell) {
        self.cursor = cell;
    }

    /// Whether the selection covers more than one cell.
    pub fn is_range(&self) -> bool {
        self.anchor != self.cursor
    }

    /// The inclusive `(min, max)` corners of the selected rectangle —
    /// normalised, so it holds whichever way the anchor and cursor lie.
    pub fn bounds(&self) -> (Cell, Cell) {
        let (ac, ar) = self.anchor;
        let (cc, cr) = self.cursor;
        ((ac.min(cc), ar.min(cr)), (ac.max(cc), ar.max(cr)))
    }

    /// Whether `cell` falls inside the selected rectangle.
    pub fn contains(&self, cell: Cell) -> bool {
        let ((min_c, min_r), (max_c, max_r)) = self.bounds();
        let (c, r) = cell;
        c >= min_c && c <= max_c && r >= min_r && r <= max_r
    }

    /// `(columns, rows)` spanned by the selection — at least `(1, 1)`.
    pub fn dimensions(&self) -> (u32, u32) {
        let ((min_c, min_r), (max_c, max_r)) = self.bounds();
        (max_c - min_c + 1, max_r - min_r + 1)
    }

    /// Every cell in the selection, row-major.
    pub fn cells(&self) -> impl Iterator<Item = Cell> {
        let ((min_c, min_r), (max_c, max_r)) = self.bounds();
        (min_r..=max_r).flat_map(move |r| (min_c..=max_c).map(move |c| (c, r)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_selection_is_a_single_cell() {
        let s = Selection::single((3, 4));
        assert!(!s.is_range());
        assert_eq!(s.dimensions(), (1, 1));
        assert!(s.contains((3, 4)));
        assert!(!s.contains((3, 5)));
    }

    #[test]
    fn extend_grows_a_range_but_collapse_resets_it() {
        let mut s = Selection::single((1, 1));
        s.extend_to((3, 2));
        assert!(s.is_range());
        assert_eq!(s.anchor, (1, 1));
        assert_eq!(s.cursor, (3, 2));
        assert_eq!(s.dimensions(), (3, 2));

        s.collapse_to((5, 5));
        assert!(!s.is_range());
        assert_eq!(s.dimensions(), (1, 1));
    }

    #[test]
    fn bounds_are_normalised_when_the_cursor_is_above_the_anchor() {
        // Anchor bottom-right, cursor top-left — bounds still min/max.
        let s = Selection {
            anchor: (4, 6),
            cursor: (2, 1),
        };
        assert_eq!(s.bounds(), ((2, 1), (4, 6)));
        assert_eq!(s.dimensions(), (3, 6));
        assert!(s.contains((3, 3)));
        assert!(!s.contains((5, 3)));
    }

    #[test]
    fn cells_enumerates_the_whole_rectangle() {
        let s = Selection {
            anchor: (1, 1),
            cursor: (2, 3),
        };
        let cells: Vec<Cell> = s.cells().collect();
        assert_eq!(cells.len(), 6);
        assert!(cells.contains(&(1, 1)));
        assert!(cells.contains(&(2, 3)));
        assert!(cells.contains(&(2, 2)));
        // Every enumerated cell is inside the selection.
        assert!(cells.iter().all(|&c| s.contains(c)));
    }

    #[test]
    fn cells_of_a_single_selection_is_just_that_cell() {
        let s = Selection::single((7, 2));
        let cells: Vec<Cell> = s.cells().collect();
        assert_eq!(cells, vec![(7, 2)]);
    }
}
