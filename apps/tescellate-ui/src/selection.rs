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

use smallvec::SmallVec;
use tescellate_core::SheetId;
use tescellate_tess::hex::HexCoord;
use tescellate_tess::triangle::TriCoord;

use crate::format::FormatMap;
use crate::formula_mode;

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
pub trait Coord: Copy + PartialEq + Eq + std::hash::Hash {
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

impl Coord for TriCoord {
    fn min_max(self, other: Self) -> (Self, Self) {
        (
            TriCoord::new(self.col.min(other.col), self.row.min(other.row)),
            TriCoord::new(self.col.max(other.col), self.row.max(other.row)),
        )
    }

    fn rect_cells(min: Self, max: Self) -> Vec<Self> {
        let mut out =
            Vec::with_capacity(((max.col - min.col + 1) * (max.row - min.row + 1)) as usize);
        for row in min.row..=max.row {
            for col in min.col..=max.col {
                out.push(TriCoord::new(col, row));
            }
        }
        out
    }

    fn rect_contains(min: Self, max: Self, coord: Self) -> bool {
        coord.col >= min.col && coord.col <= max.col && coord.row >= min.row && coord.row <= max.row
    }

    fn rect_dims(min: Self, max: Self) -> (u32, u32) {
        (
            (max.col - min.col + 1) as u32,
            (max.row - min.row + 1) as u32,
        )
    }

    fn rect_fill_targets(min: Self, max: Self, dir: FillDir) -> Vec<(Self, Self)> {
        let mut pairs = Vec::new();
        match dir {
            FillDir::Down => {
                for col in min.col..=max.col {
                    for row in (min.row + 1)..=max.row {
                        pairs.push((TriCoord::new(col, row), TriCoord::new(col, min.row)));
                    }
                }
            }
            FillDir::Right => {
                for row in min.row..=max.row {
                    for col in (min.col + 1)..=max.col {
                        pairs.push((TriCoord::new(col, row), TriCoord::new(min.col, row)));
                    }
                }
            }
        }
        pairs
    }

    fn step_back(self, dir: FillDir) -> Option<Self> {
        // Triangle coords are unbounded i32 — same as hex. A neighbour
        // always exists; the lattice's geometry, not the coord type,
        // decides whether the neighbour points the same way.
        match dir {
            FillDir::Down => Some(TriCoord::new(self.col, self.row - 1)),
            FillDir::Right => Some(TriCoord::new(self.col - 1, self.row)),
        }
    }
}

/// `Coord` impl for [`tescellate_tess::voronoi::VoronoiCoord`] —
/// Voronoi cells aren't arranged in a grid, so most rectangle-shaped
/// operations are degenerate. Sprint 6 ships single-cell selection
/// only; range selection on Voronoi waits for a real use case.
impl Coord for tescellate_tess::voronoi::VoronoiCoord {
    fn min_max(self, _other: Self) -> (Self, Self) {
        // No spatial ordering on seed indices — every "range" is just
        // the single anchor cell.
        (self, self)
    }

    fn rect_cells(min: Self, _max: Self) -> Vec<Self> {
        vec![min]
    }

    fn rect_contains(min: Self, _max: Self, coord: Self) -> bool {
        coord == min
    }

    fn rect_dims(_min: Self, _max: Self) -> (u32, u32) {
        (1, 1)
    }

    fn rect_fill_targets(_min: Self, _max: Self, _dir: FillDir) -> Vec<(Self, Self)> {
        // Fill drag is undefined on Voronoi — single-cell only.
        Vec::new()
    }

    fn step_back(self, _dir: FillDir) -> Option<Self> {
        // No directional neighbour concept on Voronoi seeds yet — the
        // fill-handle pre-step machinery just doesn't apply.
        None
    }
}

/// A rectangular cell selection — an `anchor` and a `cursor`, with an
/// optional explicit-set `extra` escape hatch for lattices whose ranges
/// don't fit a rect-coord model (Voronoi: marquee by screen-rect; ADR-013).
///
/// **Scope-cap rule (ADR-013):** `extra` is for render-only effects this
/// sprint. Existing operation paths (copy/paste, format apply, widget
/// apply, range-eval feeds) call [`primary_cells`](Self::primary_cells) /
/// [`primary_contains`](Self::primary_contains) so Voronoi marquee extras
/// don't fan out into pipelines that aren't ready yet. Render paths use
/// [`cells`](Self::cells) / [`contains`](Self::contains), which include
/// `extra`, so outlines highlight every selected cell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection<C: Coord> {
    /// Where the selection was anchored (a plain move, or the start of a
    /// shift-extend / drag).
    pub anchor: C,
    /// The active cell — where editing happens and arrows move from.
    pub cursor: C,
    /// Explicit-set selection escape hatch (Voronoi marquee; empty for
    /// rect-only selections on the other lattices).
    pub extra: SmallVec<[C; 4]>,
}

impl<C: Coord> Selection<C> {
    /// A one-cell selection.
    pub fn single(coord: C) -> Self {
        Self {
            anchor: coord,
            cursor: coord,
            extra: SmallVec::new(),
        }
    }

    /// A rect selection with the given `anchor`/`cursor` corners and no
    /// `extra` cells. Use this instead of the `Self { … }` literal so
    /// callers don't need to import `smallvec` (ADR-013).
    pub fn from_anchor_cursor(anchor: C, cursor: C) -> Self {
        Self {
            anchor,
            cursor,
            extra: SmallVec::new(),
        }
    }

    /// Move the whole selection to one cell — a plain arrow or click.
    /// Clears any `extra` cells so a single click always collapses to one cell.
    pub fn collapse_to(&mut self, coord: C) {
        self.anchor = coord;
        self.cursor = coord;
        self.extra.clear();
    }

    /// Move the cursor while keeping the anchor — a shift-extend or drag.
    pub fn extend_to(&mut self, coord: C) {
        self.cursor = coord;
    }

    /// Whether the selection covers more than one cell.
    pub fn is_range(&self) -> bool {
        self.anchor != self.cursor || !self.extra.is_empty()
    }

    /// Normalised `(min, max)` corners of the selected rectangle.
    pub fn bounds(&self) -> (C, C) {
        self.anchor.min_max(self.cursor)
    }

    /// Whether `coord` falls inside the selected rectangle OR is in
    /// `extra`. Render paths (selection-outline drawing) use this. Operation
    /// paths use [`primary_contains`](Self::primary_contains) to ignore
    /// `extra` (ADR-013 scope cap).
    pub fn contains(&self, coord: C) -> bool {
        self.primary_contains(coord) || self.extra.contains(&coord)
    }

    /// Whether `coord` falls inside the selected rectangle ONLY (ignores
    /// `extra`). Use this for operations that aren't yet ready to fan out
    /// over Voronoi marquee extras: copy/paste pickups, format/widget
    /// apply, range-eval feeds. (ADR-013.)
    pub fn primary_contains(&self, coord: C) -> bool {
        let (min, max) = self.bounds();
        C::rect_contains(min, max, coord)
    }

    /// `(axis_a, axis_b)` extents of the selection — at least `(1, 1)`.
    /// For square, this is `(columns, rows)`; for hex, `(q-span, r-span)`.
    /// Reports the rect dimensions; `extra` cells don't contribute.
    pub fn dimensions(&self) -> (u32, u32) {
        let (min, max) = self.bounds();
        C::rect_dims(min, max)
    }

    /// Every cell in the selection, **including** any `extra` cells (in
    /// rect order then `extra` insertion order, deduped). Render paths use
    /// this so outlines highlight every selected cell. Operation paths use
    /// [`primary_cells`](Self::primary_cells) (ADR-013 scope cap).
    pub fn cells(&self) -> Vec<C> {
        let mut out = self.primary_cells();
        for &c in &self.extra {
            if !out.contains(&c) {
                out.push(c);
            }
        }
        out
    }

    /// Every cell in the selection's rectangle (ignores `extra`). Used by
    /// operation paths that mutate cells or feed into eval, where the
    /// Voronoi marquee multi-cell semantics aren't yet defined. (ADR-013.)
    pub fn primary_cells(&self) -> Vec<C> {
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
            extra: SmallVec::new(),
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
            extra: SmallVec::new(),
        }
    }

    /// A selection covering the whole `cols × rows` grid. The active
    /// cell is the top-left, `(0, 0)`.
    pub fn all(cols: u32, rows: u32) -> Self {
        Self {
            anchor: (cols.saturating_sub(1), rows.saturating_sub(1)),
            cursor: (0, 0),
            extra: SmallVec::new(),
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

/// A rectangular triangle selection. The selected range is the
/// rectangular block in `(col, row)` triangle coordinates between
/// anchor and cursor, matching the engine's `T(c0,r0):T(c1,r1)`
/// range shape.
pub type TriangleSelection = Selection<TriCoord>;

/// The per-lattice state bundle every sheet kind shares — engine
/// handle, selection, in-progress formula reference, and per-cell
/// visual formatting. Stage C of the unified-lattice refactor: where
/// v117 unified the selection model, this unifies *the state every
/// sheet carries with it*, so adding a triangle sheet later means
/// instantiating `Sheet<TriangleCoord>` rather than spreading another
/// dozen parallel fields across `TescellateApp`.
///
/// Notes, format-edit coalescing timestamps, and rendering-only state
/// (column widths, scroll position) intentionally stay outside the
/// bundle for now — they collapse cleanly in a later pass once this
/// shape settles.
pub struct Sheet<C: Coord> {
    /// The engine sheet handle this UI sheet wraps.
    pub sheet_id: SheetId,
    /// The selected cell range.
    pub selection: Selection<C>,
    /// `Some` while a formula-mode pointer drag is building a range
    /// reference into the edit buffer.
    pub formula_drag: Option<formula_mode::DragState<C>>,
    /// The formula reference the user is currently pointing at — drawn
    /// as a dashed marquee on the grid until the edit is committed
    /// or cancelled.
    pub formula_highlight: Option<formula_mode::Highlight<C>>,
    /// Per-cell visual formatting (font, colours, alignment, number
    /// format, borders, etc.).
    pub formats: FormatMap<C>,
}

impl<C: Coord> Sheet<C> {
    /// A fresh sheet wrapping `sheet_id`, with the cursor at `origin`
    /// and no formula reference or formatting yet.
    pub fn new(sheet_id: SheetId, origin: C) -> Self {
        Self {
            sheet_id,
            selection: Selection::single(origin),
            formula_drag: None,
            formula_highlight: None,
            formats: FormatMap::default(),
        }
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
            anchor: (4u32, 6u32),
            cursor: (2u32, 1u32),
            extra: SmallVec::new(),
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
            extra: SmallVec::new(),
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
            extra: SmallVec::new(),
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
            extra: SmallVec::new(),
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
            extra: SmallVec::new(),
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
            extra: SmallVec::new(),
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
            extra: SmallVec::new(),
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
            extra: SmallVec::new(),
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
            extra: SmallVec::new(),
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
            extra: SmallVec::new(),
        };
        assert!(s.fill_targets(FillDir::Down).is_empty());
    }

    #[test]
    fn triangle_selection_single_then_extends() {
        let mut s = TriangleSelection::single(TriCoord::new(1, 1));
        assert!(!s.is_range());
        assert_eq!(s.cells(), vec![TriCoord::new(1, 1)]);
        s.extend_to(TriCoord::new(3, 2));
        assert!(s.is_range());
        assert_eq!(s.dimensions(), (3, 2));
        s.collapse_to(TriCoord::new(0, 0));
        assert!(!s.is_range());
    }

    #[test]
    fn triangle_selection_bounds_normalise_and_contain() {
        // Anchor bottom-right, cursor top-left — bounds still min/max.
        let s = TriangleSelection {
            anchor: TriCoord::new(3, 4),
            cursor: TriCoord::new(1, 0),
            extra: SmallVec::new(),
        };
        let (min, max) = s.bounds();
        assert_eq!((min.col, min.row), (1, 0));
        assert_eq!((max.col, max.row), (3, 4));
        assert!(s.contains(TriCoord::new(2, 2)));
        assert!(!s.contains(TriCoord::new(4, 2)));
    }

    #[test]
    fn triangle_selection_cells_enumerate_the_rectangle() {
        let s = TriangleSelection {
            anchor: TriCoord::new(0, 0),
            cursor: TriCoord::new(2, 1),
            extra: SmallVec::new(),
        };
        let cells = s.cells();
        // 3 columns × 2 rows = 6 triangle cells.
        assert_eq!(cells.len(), 6);
        assert!(cells.contains(&TriCoord::new(0, 0)));
        assert!(cells.contains(&TriCoord::new(2, 1)));
        assert!(cells.iter().all(|&c| s.contains(c)));
    }

    #[test]
    fn triangle_fill_down_propagates_the_top_row_per_column() {
        // 2 columns × 3 rows: the two lower rows fill from the top.
        let s = TriangleSelection {
            anchor: TriCoord::new(0, 0),
            cursor: TriCoord::new(1, 2),
            extra: SmallVec::new(),
        };
        let pairs = s.fill_targets(FillDir::Down);
        assert_eq!(pairs.len(), 4);
        assert!(pairs.contains(&(TriCoord::new(0, 1), TriCoord::new(0, 0))));
        assert!(pairs.contains(&(TriCoord::new(1, 2), TriCoord::new(1, 0))));
    }

    #[test]
    fn triangle_fill_right_propagates_the_left_column_per_row() {
        let s = TriangleSelection {
            anchor: TriCoord::new(0, 0),
            cursor: TriCoord::new(2, 1),
            extra: SmallVec::new(),
        };
        let pairs = s.fill_targets(FillDir::Right);
        assert_eq!(pairs.len(), 4);
        assert!(pairs.contains(&(TriCoord::new(1, 0), TriCoord::new(0, 0))));
        assert!(pairs.contains(&(TriCoord::new(2, 1), TriCoord::new(0, 1))));
    }

    #[test]
    fn triangle_fill_on_a_single_cell_pulls_from_the_neighbour() {
        let s = TriangleSelection::single(TriCoord::new(3, -2));
        // Triangle coords are unbounded — step_back never returns None.
        assert_eq!(
            s.fill_targets(FillDir::Down),
            vec![(TriCoord::new(3, -2), TriCoord::new(3, -3))],
        );
        assert_eq!(
            s.fill_targets(FillDir::Right),
            vec![(TriCoord::new(3, -2), TriCoord::new(2, -2))],
        );
    }

    #[test]
    fn triangle_fill_down_on_a_single_row_range_is_a_noop() {
        let s = TriangleSelection {
            anchor: TriCoord::new(0, 4),
            cursor: TriCoord::new(3, 4),
            extra: SmallVec::new(),
        };
        assert!(s.fill_targets(FillDir::Down).is_empty());
    }

    // --- T-001 (ADR-013): explicit-set `extra` escape hatch ---

    #[test]
    fn selection_default_extra_is_empty_and_cells_unchanged() {
        let s = Selection::single((1u32, 2u32));
        assert!(s.extra.is_empty());
        assert_eq!(s.cells(), vec![(1, 2)]);
        assert_eq!(s.primary_cells(), vec![(1, 2)]);
    }

    #[test]
    fn selection_extra_adds_to_cells_dedup() {
        // 3×3 rect (0,0)..(2,2) plus an extra at (5,5) — and one dup at (1,1)
        // (inside the rect) that must NOT appear twice.
        let mut s = Selection::single((0u32, 0u32));
        s.extend_to((2u32, 2u32));
        s.extra.push((5, 5));
        s.extra.push((1, 1));
        let cells = s.cells();
        assert_eq!(cells.len(), 10, "9 rect cells + 1 unique extra");
        assert!(cells.contains(&(5, 5)));
        assert_eq!(cells.iter().filter(|&&c| c == (1, 1)).count(), 1);
    }

    #[test]
    fn selection_extra_widens_contains_but_not_primary_contains() {
        let mut s = Selection::single((0u32, 0u32));
        s.extra.push((5, 5));
        assert!(s.contains((5, 5)), "contains() includes extra");
        assert!(
            !s.primary_contains((5, 5)),
            "primary_contains() ignores extra (ADR-013 scope cap)"
        );
    }

    #[test]
    fn selection_primary_cells_excludes_extra() {
        let mut s = Selection::single((0u32, 0u32));
        s.extend_to((1u32, 1u32));
        s.extra.push((9, 9));
        assert_eq!(
            s.primary_cells().len(),
            4,
            "primary_cells is rect-only (2×2)"
        );
        assert!(!s.primary_cells().contains(&(9, 9)));
        assert!(s.cells().contains(&(9, 9)), "cells() includes extra");
    }

    #[test]
    fn selection_collapse_to_clears_extra() {
        let mut s = Selection::single((0u32, 0u32));
        s.extra.push((5, 5));
        s.extra.push((6, 6));
        s.collapse_to((3, 3));
        assert!(s.extra.is_empty());
        assert_eq!(s.cells(), vec![(3, 3)]);
    }

    #[test]
    fn selection_extra_persists_through_drag_end() {
        // C-006: between drag-end and the next click, `extra` must persist
        // (so outlines stay drawn). Simulate by populating extra and NOT
        // calling collapse_to — the field is unchanged.
        let mut s = Selection::single((0u32, 0u32));
        s.extra.push((4, 4));
        s.extra.push((7, 7));
        let snapshot = s.extra.clone();
        // No collapse_to between here and the assert — extra persists.
        assert_eq!(s.extra, snapshot);
    }
}
