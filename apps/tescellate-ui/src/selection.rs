//! The pure selected-range model.
//!
//! A [`Selection<C>`] is an `anchor` cell and a `cursor` (the active
//! cell). The selected range is the inclusive rectangle spanning the
//! two — a single cell when they coincide. The lattice provides a
//! [`Coord`] implementation; the selection logic itself is lattice-
//! agnostic. Stage B of the unified-lattice refactor: square cells
//! `(u32,u32)` and `HexCoord` flow through the same `Selection<C>`
//! type, so triangle / voronoi grids inherit the model for free.
//!
//! No egui and no engine here, so every method is exercised by
//! ordinary `cargo test`.

use tescellate_tess::hex::HexCoord;

/// A zero-indexed `(column, row)` cell on the square grid.
pub type Cell = (u32, u32);

/// Which way a fill propagates across the selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FillDir {
    Down,
    Right,
}

/// Lattice-agnostic coordinate operations [`Selection<C>`] needs. Each
/// concrete coord type (`(u32,u32)` for the square grid, `HexCoord` for
/// the hex grid) implements this; new lattices add an impl rather than
/// a parallel `Selection` struct.
///
/// The methods are deliberately framed in terms of *normalised*
/// `(min, max)` rectangles — selection takes care of normalising the
/// anchor/cursor pair via [`Coord::min_max`] before calling the rest.
pub trait Coord: Copy + PartialEq + Eq {
    /// Normalised `(min, max)` corners of the rectangle spanning `self`
    /// and `other`. Order-independent: callers can pass anchor and
    /// cursor in either order and get the same answer.
    fn min_max(self, other: Self) -> (Self, Self);

    /// Every coord in the inclusive rectangle `[min, max]`. Order is
    /// deterministic (row-major for square, q-then-r for hex).
    fn rect_cells(min: Self, max: Self) -> Vec<Self>;

    /// Whether `coord` falls inside the inclusive rectangle.
    fn rect_contains(min: Self, max: Self, coord: Self) -> bool;

    /// Per-axis spans of the rectangle, each ≥ 1.
    fn rect_dims(min: Self, max: Self) -> (u32, u32);

    /// `(target, source)` pairs to fill a multi-cell rect from its
    /// leading edge along `dir`. Empty if the rect is already single
    /// along that axis (a one-row range filled Down, etc.).
    fn rect_fill_targets(min: Self, max: Self, dir: FillDir) -> Vec<(Self, Self)>;

    /// Step back one cell along `dir`. `None` if the neighbour is out
    /// of bounds (e.g. row 0 filling Down for the square grid). Hex's
    /// axial coords are unbounded, so its impl never returns `None`.
    fn step_back(self, dir: FillDir) -> Option<Self>;
}

impl Coord for Cell {
    fn min_max(self, other: Self) -> (Self, Self) {
        let (a, b) = (self, other);
        ((a.0.min(b.0), a.1.min(b.1)), (a.0.max(b.0), a.1.max(b.1)))
    }

    fn rect_cells((mc, mr): Self, (xc, xr): Self) -> Vec<Self> {
        let mut out = Vec::with_capacity(((xc - mc + 1) * (xr - mr + 1)) as usize);
        for r in mr..=xr {
            for c in mc..=xc {
                out.push((c, r));
            }
        }
        out
    }

    fn rect_contains((mc, mr): Self, (xc, xr): Self, (c, r): Self) -> bool {
        c >= mc && c <= xc && r >= mr && r <= xr
    }

    fn rect_dims((mc, mr): Self, (xc, xr): Self) -> (u32, u32) {
        (xc - mc + 1, xr - mr + 1)
    }

    fn rect_fill_targets((mc, mr): Self, (xc, xr): Self, dir: FillDir) -> Vec<(Self, Self)> {
        let mut pairs = Vec::new();
        match dir {
            FillDir::Down => {
                for c in mc..=xc {
                    for r in (mr + 1)..=xr {
                        pairs.push(((c, r), (c, mr)));
                    }
                }
            }
            FillDir::Right => {
                for r in mr..=xr {
                    for c in (mc + 1)..=xc {
                        pairs.push(((c, r), (mc, r)));
                    }
                }
            }
        }
        pairs
    }

    fn step_back(self, dir: FillDir) -> Option<Self> {
        let (c, r) = self;
        match dir {
            // `then` is lazy — `r - 1` would underflow `u32` if eagerly
            // evaluated when `r == 0`, so the closure form is required
            // even though `then_some` reads slightly cleaner.
            FillDir::Down => (r > 0).then(|| (c, r - 1)),
            FillDir::Right => (c > 0).then(|| (c - 1, r)),
        }
    }
}

impl Coord for HexCoord {
    fn min_max(self, other: Self) -> (Self, Self) {
        (
            HexCoord::new(self.q.min(other.q), self.r.min(other.r)),
            HexCoord::new(self.q.max(other.q), self.r.max(other.r)),
        )
    }

    fn rect_cells(min: Self, max: Self) -> Vec<Self> {
        let mut out = Vec::with_capacity(((max.q - min.q + 1) * (max.r - min.r + 1)) as usize);
        for r in min.r..=max.r {
            for q in min.q..=max.q {
                out.push(HexCoord::new(q, r));
            }
        }
        out
    }

    fn rect_contains(min: Self, max: Self, coord: Self) -> bool {
        coord.q >= min.q && coord.q <= max.q && coord.r >= min.r && coord.r <= max.r
    }

    fn rect_dims(min: Self, max: Self) -> (u32, u32) {
        ((max.q - min.q + 1) as u32, (max.r - min.r + 1) as u32)
    }

    fn rect_fill_targets(min: Self, max: Self, dir: FillDir) -> Vec<(Self, Self)> {
        let mut pairs = Vec::new();
        match dir {
            FillDir::Down => {
                for q in min.q..=max.q {
                    for r in (min.r + 1)..=max.r {
                        pairs.push((HexCoord::new(q, r), HexCoord::new(q, min.r)));
                    }
                }
            }
            FillDir::Right => {
                for r in min.r..=max.r {
                    for q in (min.q + 1)..=max.q {
                        pairs.push((HexCoord::new(q, r), HexCoord::new(min.q, r)));
                    }
                }
            }
        }
        pairs
    }

    fn step_back(self, dir: FillDir) -> Option<Self> {
        // Axial coords are unbounded i32 — the neighbour always exists.
        match dir {
            FillDir::Down => Some(HexCoord::new(self.q, self.r - 1)),
            FillDir::Right => Some(HexCoord::new(self.q - 1, self.r)),
        }
    }
}

/// A rectangular cell selection — an `anchor` and a `cursor`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection<C: Coord> {
    /// Where the selection was anchored (a plain move, or the start of a
    /// shift-extend / drag).
    pub anchor: C,
    /// The active cell — where editing happens and arrows move from.
    pub cursor: C,
}

impl<C: Coord> Selection<C> {
    /// A one-cell selection.
    pub fn single(coord: C) -> Self {
        Self {
            anchor: coord,
            cursor: coord,
        }
    }

    /// Move the whole selection to one cell — a plain arrow or click.
    pub fn collapse_to(&mut self, coord: C) {
        self.anchor = coord;
        self.cursor = coord;
    }

    /// Move the cursor while keeping the anchor — a shift-extend or drag.
    pub fn extend_to(&mut self, coord: C) {
        self.cursor = coord;
    }

    /// Whether the selection covers more than one cell.
    pub fn is_range(&self) -> bool {
        self.anchor != self.cursor
    }

    /// Normalised `(min, max)` corners of the selected rectangle.
    pub fn bounds(&self) -> (C, C) {
        self.anchor.min_max(self.cursor)
    }

    /// Whether `coord` falls inside the selected rectangle.
    pub fn contains(&self, coord: C) -> bool {
        let (min, max) = self.bounds();
        C::rect_contains(min, max, coord)
    }

    /// `(axis_a, axis_b)` extents of the selection — at least `(1, 1)`.
    /// For square, this is `(columns, rows)`; for hex, `(q-span, r-span)`.
    pub fn dimensions(&self) -> (u32, u32) {
        let (min, max) = self.bounds();
        C::rect_dims(min, max)
    }

    /// Every cell in the selection. Row-major for square, q-then-r for hex.
    pub fn cells(&self) -> Vec<C> {
        let (min, max) = self.bounds();
        C::rect_cells(min, max)
    }

    /// The `(target, source)` cell pairs for a fill. A multi-cell range
    /// fills its leading edge across the rest of the selection. A
    /// single cell pulls from its neighbour one step back. The list is
    /// empty when there is nothing to fill (a single-axis range filled
    /// along that axis, or a single cell already at the grid edge).
    pub fn fill_targets(&self, dir: FillDir) -> Vec<(C, C)> {
        if self.is_range() {
            let (min, max) = self.bounds();
            C::rect_fill_targets(min, max, dir)
        } else {
            self.cursor
                .step_back(dir)
                .map(|n| vec![(self.cursor, n)])
                .unwrap_or_default()
        }
    }
}

/// Square-grid-specific constructors. These rely on row/column
/// semantics that hex doesn't have, so they live in their own impl
/// block on `Selection<Cell>` rather than on the generic `Selection<C>`.
impl Selection<Cell> {
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
}

/// A rectangular cell selection on the square sheet.
pub type SquareSelection = Selection<Cell>;

/// A rectangular hex selection. The selected range is the axial
/// parallelogram between anchor and cursor (every cell whose `q` and
/// `r` lie within the corners), the same shape the engine's
/// `H(a,b):H(c,d)` range describes.
pub type HexSelection = Selection<HexCoord>;

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
            anchor: (4u32, 6u32),
            cursor: (2u32, 1u32),
        };
        assert_eq!(s.bounds(), ((2, 1), (4, 6)));
        assert_eq!(s.dimensions(), (3, 6));
        assert!(s.contains((3, 3)));
        assert!(!s.contains((5, 3)));
    }

    #[test]
    fn cells_enumerates_the_whole_rectangle() {
        let s = Selection {
            anchor: (1u32, 1u32),
            cursor: (2u32, 3u32),
        };
        let cells: Vec<Cell> = s.cells();
        assert_eq!(cells.len(), 6);
        assert!(cells.contains(&(1, 1)));
        assert!(cells.contains(&(2, 3)));
        assert!(cells.contains(&(2, 2)));
        // Every enumerated cell is inside the selection.
        assert!(cells.iter().all(|&c| s.contains(c)));
    }

    #[test]
    fn cells_of_a_single_selection_is_just_that_cell() {
        let s = Selection::single((7u32, 2u32));
        assert_eq!(s.cells(), vec![(7, 2)]);
    }

    #[test]
    fn fill_down_propagates_the_top_row_per_column() {
        // A 2-column × 3-row range: rows 1 and 2 fill from row 0.
        let s = Selection {
            anchor: (0u32, 0u32),
            cursor: (1u32, 2u32),
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
            anchor: (0u32, 0u32),
            cursor: (2u32, 1u32),
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
            Selection::single((3u32, 5u32)).fill_targets(FillDir::Down),
            vec![((3, 5), (3, 4))],
        );
        assert_eq!(
            Selection::single((4u32, 2u32)).fill_targets(FillDir::Right),
            vec![((4, 2), (3, 2))],
        );
        // A cell at the grid edge has no neighbour to pull from.
        assert!(Selection::single((3u32, 0u32))
            .fill_targets(FillDir::Down)
            .is_empty());
        assert!(Selection::single((0u32, 2u32))
            .fill_targets(FillDir::Right)
            .is_empty());
    }

    #[test]
    fn fill_down_on_a_single_row_range_is_a_noop() {
        let s = Selection {
            anchor: (0u32, 4u32),
            cursor: (3u32, 4u32),
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
        let (min, max) = s.bounds();
        assert_eq!((min.q, min.r), (1, 0));
        assert_eq!((max.q, max.r), (3, 4));
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
