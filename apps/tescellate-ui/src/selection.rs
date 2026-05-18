//! The pure selected-range model for the square sheet.
//!
//! A [`Selection`] is an `anchor` cell and a `cursor` (the active cell).
//! The selected range is the inclusive rectangle spanning the two — a
//! single cell when they coincide. No egui and no engine here, so the
//! whole model is exercised by ordinary `cargo test`.

use tescellate_tess::hex::HexCoord;

/// A zero-indexed `(column, row)` cell.
pub type Cell = (u32, u32);

/// Which way a fill propagates across the selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FillDir {
    Down,
    Right,
}

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

    /// A selection spanning an entire column — every row `0..rows`. The
    /// active cell is the column's top, `(col, 0)`, as spreadsheets
    /// place it.
    pub fn column(col: u32, rows: u32) -> Self {
        Self::column_range(col, col, rows)
    }

    /// A selection spanning the full height of the columns between `c0`
    /// and `c1` inclusive, in either order — what a drag across column
    /// headers sweeps. The active cell is the top of column `c1`, the
    /// end the drag last reached.
    pub fn column_range(c0: u32, c1: u32, rows: u32) -> Self {
        Self {
            anchor: (c0, rows.saturating_sub(1)),
            cursor: (c1, 0),
        }
    }

    /// A selection spanning an entire row — every column `0..cols`. The
    /// active cell is the row's leftmost cell, `(0, row)`.
    pub fn row(row: u32, cols: u32) -> Self {
        Self::row_range(row, row, cols)
    }

    /// A selection spanning the full width of the rows between `r0` and
    /// `r1` inclusive, in either order. The active cell is the left of
    /// row `r1`.
    pub fn row_range(r0: u32, r1: u32, cols: u32) -> Self {
        Self {
            anchor: (cols.saturating_sub(1), r0),
            cursor: (0, r1),
        }
    }

    /// A selection covering the whole `cols × rows` grid. The active
    /// cell is the top-left, `(0, 0)`.
    pub fn all(cols: u32, rows: u32) -> Self {
        Self {
            anchor: (cols.saturating_sub(1), rows.saturating_sub(1)),
            cursor: (0, 0),
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

    /// The `(target, source)` cell pairs for a fill. A multi-cell range
    /// fills its leading edge — the top row for `Down`, the left column
    /// for `Right` — across the rest of the selection. A single cell
    /// pulls from its neighbour one step back. The list is empty when
    /// there is nothing to fill (a single-row range filled down, or a
    /// single cell already at the grid edge).
    pub fn fill_targets(&self, dir: FillDir) -> Vec<(Cell, Cell)> {
        let ((min_c, min_r), (max_c, max_r)) = self.bounds();
        let mut pairs = Vec::new();
        match dir {
            FillDir::Down => {
                if !self.is_range() {
                    if min_r > 0 {
                        pairs.push(((min_c, min_r), (min_c, min_r - 1)));
                    }
                } else {
                    for c in min_c..=max_c {
                        for r in (min_r + 1)..=max_r {
                            pairs.push(((c, r), (c, min_r)));
                        }
                    }
                }
            }
            FillDir::Right => {
                if !self.is_range() {
                    if min_c > 0 {
                        pairs.push(((min_c, min_r), (min_c - 1, min_r)));
                    }
                } else {
                    for r in min_r..=max_r {
                        for c in (min_c + 1)..=max_c {
                            pairs.push(((c, r), (min_c, r)));
                        }
                    }
                }
            }
        }
        pairs
    }
}

/// A rectangular hex selection — an `anchor` hex and a `cursor` hex.
/// The selected range is the axial parallelogram between them (every
/// cell whose `q` and `r` lie within the corners), the same shape the
/// engine's `H(a,b):H(c,d)` range describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HexSelection {
    pub anchor: HexCoord,
    pub cursor: HexCoord,
}

impl HexSelection {
    /// A one-cell hex selection.
    pub fn single(coord: HexCoord) -> Self {
        Self {
            anchor: coord,
            cursor: coord,
        }
    }

    /// Move the whole selection to one cell — a plain move or click.
    pub fn collapse_to(&mut self, coord: HexCoord) {
        self.anchor = coord;
        self.cursor = coord;
    }

    /// Move the cursor while keeping the anchor — a shift-extend.
    pub fn extend_to(&mut self, coord: HexCoord) {
        self.cursor = coord;
    }

    /// Whether the selection covers more than one cell.
    pub fn is_range(&self) -> bool {
        self.anchor != self.cursor
    }

    /// The inclusive `(q, r)` min/max corners — normalised, so it holds
    /// whichever way the anchor and cursor lie.
    pub fn bounds(&self) -> ((i32, i32), (i32, i32)) {
        (
            (
                self.anchor.q.min(self.cursor.q),
                self.anchor.r.min(self.cursor.r),
            ),
            (
                self.anchor.q.max(self.cursor.q),
                self.anchor.r.max(self.cursor.r),
            ),
        )
    }

    /// Whether `coord` falls inside the axial parallelogram.
    pub fn contains(&self, coord: HexCoord) -> bool {
        let ((min_q, min_r), (max_q, max_r)) = self.bounds();
        coord.q >= min_q && coord.q <= max_q && coord.r >= min_r && coord.r <= max_r
    }

    /// `(q-span, r-span)` of the selection — at least `(1, 1)`.
    pub fn dimensions(&self) -> (i32, i32) {
        let ((min_q, min_r), (max_q, max_r)) = self.bounds();
        (max_q - min_q + 1, max_r - min_r + 1)
    }

    /// Every cell of the axial parallelogram, in q-then-r order.
    pub fn cells(&self) -> Vec<HexCoord> {
        let ((min_q, min_r), (max_q, max_r)) = self.bounds();
        let mut out = Vec::new();
        for r in min_r..=max_r {
            for q in min_q..=max_q {
                out.push(HexCoord::new(q, r));
            }
        }
        out
    }

    /// The `(target, source)` cell pairs for a fill — the hex analog of
    /// [`Selection::fill_targets`]. A multi-cell range fills its leading
    /// edge (the `min_r` row down each `q`-column for `Down`, the
    /// `min_q` column right along each `r`-row for `Right`) across the
    /// rest of the selection. A single cell pulls from its neighbour one
    /// axial step back. Empty for a single-row range filled down.
    pub fn fill_targets(&self, dir: FillDir) -> Vec<(HexCoord, HexCoord)> {
        let ((min_q, min_r), (max_q, max_r)) = self.bounds();
        let mut pairs = Vec::new();
        match dir {
            FillDir::Down => {
                if !self.is_range() {
                    pairs.push((HexCoord::new(min_q, min_r), HexCoord::new(min_q, min_r - 1)));
                } else {
                    for q in min_q..=max_q {
                        for r in (min_r + 1)..=max_r {
                            pairs.push((HexCoord::new(q, r), HexCoord::new(q, min_r)));
                        }
                    }
                }
            }
            FillDir::Right => {
                if !self.is_range() {
                    pairs.push((HexCoord::new(min_q, min_r), HexCoord::new(min_q - 1, min_r)));
                } else {
                    for r in min_r..=max_r {
                        for q in (min_q + 1)..=max_q {
                            pairs.push((HexCoord::new(q, r), HexCoord::new(min_q, r)));
                        }
                    }
                }
            }
        }
        pairs
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
    fn column_selection_spans_every_row() {
        let s = Selection::column(3, 32);
        assert_eq!(s.bounds(), ((3, 0), (3, 31)));
        assert_eq!(s.dimensions(), (1, 32));
        // The active cell is the column's top.
        assert_eq!(s.cursor, (3, 0));
        assert!(s.contains((3, 15)));
        assert!(!s.contains((4, 15)));
    }

    #[test]
    fn row_selection_spans_every_column() {
        let s = Selection::row(7, 16);
        assert_eq!(s.bounds(), ((0, 7), (15, 7)));
        assert_eq!(s.dimensions(), (16, 1));
        assert_eq!(s.cursor, (0, 7));
        assert!(s.contains((9, 7)));
        assert!(!s.contains((9, 8)));
    }

    #[test]
    fn select_all_covers_the_whole_grid() {
        let s = Selection::all(16, 32);
        assert_eq!(s.bounds(), ((0, 0), (15, 31)));
        assert_eq!(s.dimensions(), (16, 32));
        assert_eq!(s.cursor, (0, 0));
    }

    #[test]
    fn region_constructors_survive_a_zero_extent() {
        // Degenerate counts must not underflow — they collapse to a cell.
        assert_eq!(Selection::column(2, 0).bounds(), ((2, 0), (2, 0)));
        assert_eq!(Selection::row(0, 0).bounds(), ((0, 0), (0, 0)));
        assert_eq!(Selection::all(0, 0).bounds(), ((0, 0), (0, 0)));
    }

    #[test]
    fn column_range_spans_the_columns_between_its_ends() {
        let s = Selection::column_range(1, 4, 32);
        assert_eq!(s.bounds(), ((1, 0), (4, 31)));
        assert_eq!(s.dimensions(), (4, 32));
        // The active cell is the top of the drag's end column.
        assert_eq!(s.cursor, (4, 0));
    }

    #[test]
    fn column_range_normalises_reversed_ends() {
        // Dragging right-to-left yields the same rectangle.
        let s = Selection::column_range(5, 2, 32);
        assert_eq!(s.bounds(), ((2, 0), (5, 31)));
        assert_eq!(s.cursor, (2, 0));
    }

    #[test]
    fn row_range_spans_the_rows_between_its_ends() {
        let s = Selection::row_range(3, 6, 16);
        assert_eq!(s.bounds(), ((0, 3), (15, 6)));
        assert_eq!(s.dimensions(), (16, 4));
        assert_eq!(s.cursor, (0, 6));
    }

    #[test]
    fn single_ended_ranges_match_the_whole_column_and_row() {
        // column / row are the degenerate one-index ranges.
        assert_eq!(Selection::column_range(7, 7, 32), Selection::column(7, 32));
        assert_eq!(Selection::row_range(2, 2, 16), Selection::row(2, 16));
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

    #[test]
    fn fill_down_propagates_the_top_row_per_column() {
        // A 2-column × 3-row range: rows 1 and 2 fill from row 0.
        let s = Selection {
            anchor: (0, 0),
            cursor: (1, 2),
        };
        let pairs = s.fill_targets(FillDir::Down);
        assert_eq!(pairs.len(), 4);
        assert!(pairs.contains(&((0, 1), (0, 0))));
        assert!(pairs.contains(&((0, 2), (0, 0))));
        assert!(pairs.contains(&((1, 1), (1, 0))));
        assert!(pairs.contains(&((1, 2), (1, 0))));
    }

    #[test]
    fn fill_right_propagates_the_left_column_per_row() {
        let s = Selection {
            anchor: (0, 0),
            cursor: (2, 1),
        };
        let pairs = s.fill_targets(FillDir::Right);
        assert_eq!(pairs.len(), 4);
        assert!(pairs.contains(&((1, 0), (0, 0))));
        assert!(pairs.contains(&((2, 0), (0, 0))));
        assert!(pairs.contains(&((1, 1), (0, 1))));
        assert!(pairs.contains(&((2, 1), (0, 1))));
    }

    #[test]
    fn fill_on_a_single_cell_pulls_from_the_neighbour() {
        assert_eq!(
            Selection::single((3, 5)).fill_targets(FillDir::Down),
            vec![((3, 5), (3, 4))],
        );
        assert_eq!(
            Selection::single((4, 2)).fill_targets(FillDir::Right),
            vec![((4, 2), (3, 2))],
        );
        // A cell at the grid edge has no neighbour to pull from.
        assert!(Selection::single((3, 0))
            .fill_targets(FillDir::Down)
            .is_empty());
        assert!(Selection::single((0, 2))
            .fill_targets(FillDir::Right)
            .is_empty());
    }

    #[test]
    fn fill_down_on_a_single_row_range_is_a_noop() {
        let s = Selection {
            anchor: (0, 4),
            cursor: (3, 4),
        };
        assert!(s.fill_targets(FillDir::Down).is_empty());
    }

    #[test]
    fn hex_selection_single_then_extends() {
        let mut s = HexSelection::single(HexCoord::new(1, 1));
        assert!(!s.is_range());
        assert_eq!(s.cells(), vec![HexCoord::new(1, 1)]);
        s.extend_to(HexCoord::new(2, 3));
        assert!(s.is_range());
        assert_eq!(s.dimensions(), (2, 3));
        s.collapse_to(HexCoord::new(5, 5));
        assert!(!s.is_range());
    }

    #[test]
    fn hex_selection_bounds_normalise_and_contain() {
        // Cursor below-left of the anchor — bounds are still min/max.
        let s = HexSelection {
            anchor: HexCoord::new(3, 4),
            cursor: HexCoord::new(1, 0),
        };
        assert_eq!(s.bounds(), ((1, 0), (3, 4)));
        assert!(s.contains(HexCoord::new(2, 2)));
        assert!(!s.contains(HexCoord::new(4, 2)));
    }

    #[test]
    fn hex_selection_cells_enumerate_the_parallelogram() {
        let s = HexSelection {
            anchor: HexCoord::new(0, 0),
            cursor: HexCoord::new(1, 2),
        };
        let cells = s.cells();
        assert_eq!(cells.len(), 6);
        assert!(cells.contains(&HexCoord::new(0, 0)));
        assert!(cells.contains(&HexCoord::new(1, 2)));
        assert!(cells.iter().all(|&c| s.contains(c)));
    }

    #[test]
    fn hex_fill_down_propagates_the_top_row_per_column() {
        // A 2-q × 3-r range: the two lower r-rows fill from min_r.
        let s = HexSelection {
            anchor: HexCoord::new(0, 0),
            cursor: HexCoord::new(1, 2),
        };
        let pairs = s.fill_targets(FillDir::Down);
        assert_eq!(pairs.len(), 4);
        assert!(pairs.contains(&(HexCoord::new(0, 1), HexCoord::new(0, 0))));
        assert!(pairs.contains(&(HexCoord::new(1, 2), HexCoord::new(1, 0))));
    }

    #[test]
    fn hex_fill_right_propagates_the_left_column_per_row() {
        let s = HexSelection {
            anchor: HexCoord::new(0, 0),
            cursor: HexCoord::new(2, 1),
        };
        let pairs = s.fill_targets(FillDir::Right);
        assert_eq!(pairs.len(), 4);
        assert!(pairs.contains(&(HexCoord::new(1, 0), HexCoord::new(0, 0))));
        assert!(pairs.contains(&(HexCoord::new(2, 1), HexCoord::new(0, 1))));
    }

    #[test]
    fn hex_fill_on_a_single_cell_pulls_from_the_neighbour() {
        let s = HexSelection::single(HexCoord::new(3, -2));
        assert_eq!(
            s.fill_targets(FillDir::Down),
            vec![(HexCoord::new(3, -2), HexCoord::new(3, -3))],
        );
        assert_eq!(
            s.fill_targets(FillDir::Right),
            vec![(HexCoord::new(3, -2), HexCoord::new(2, -2))],
        );
    }

    #[test]
    fn hex_fill_down_on_a_single_row_range_is_a_noop() {
        let s = HexSelection {
            anchor: HexCoord::new(0, 4),
            cursor: HexCoord::new(3, 4),
        };
        assert!(s.fill_targets(FillDir::Down).is_empty());
    }
}
