//! Grid geometry — A1 addressing and cell ↔ pixel mapping.
//!
//! [`GridMetrics`] holds per-column widths and per-row heights (each
//! defaulting until overridden), so columns and rows can be resized
//! independently. It is framework-light — only egui's plain geometry
//! value types — so every function here is exercised by ordinary
//! `cargo test`, no browser required.

use std::collections::HashMap;

use egui::{pos2, Pos2, Rect, Vec2};

/// Default column width, in points.
pub const DEFAULT_COL_W: f32 = 84.0;
/// Default row height, in points.
pub const DEFAULT_ROW_H: f32 = 22.0;
/// Smallest a column may be dragged.
pub const MIN_COL_W: f32 = 32.0;
/// Smallest a row may be dragged.
pub const MIN_ROW_H: f32 = 16.0;
/// Width of the row-number header column.
pub const HEADER_W: f32 = 44.0;
/// Height of the column-letter header row.
pub const HEADER_H: f32 = 22.0;
/// Half-width of the border zone that begins a resize drag.
pub const BORDER_GRAB: f32 = 4.0;

/// The A1-style label of a zero-indexed column: `0 -> "A"`, `25 -> "Z"`,
/// `26 -> "AA"`. Bijective base-26, as spreadsheets use.
pub fn column_label(mut col: u32) -> String {
    let mut bytes = Vec::new();
    loop {
        bytes.push(b'A' + (col % 26) as u8);
        if col < 26 {
            break;
        }
        col = col / 26 - 1;
    }
    bytes.reverse();
    String::from_utf8(bytes).expect("column label is ASCII")
}

/// The A1 address of a zero-indexed cell: `(0, 0) -> "A1"`.
pub fn cell_address(col: u32, row: u32) -> String {
    format!("{}{}", column_label(col), row + 1)
}

/// Parse an A1-style address — column letters then a 1-based row number,
/// e.g. `"B7"` — into a zero-indexed `(col, row)`. The inverse of
/// [`cell_address`]. `None` for anything that is not `[A-Za-z]+[0-9]+`
/// with a non-zero row; surrounding whitespace and letter case are
/// tolerated.
pub fn parse_address(text: &str) -> Option<(u32, u32)> {
    let text = text.trim();
    let split = text.find(|c: char| c.is_ascii_digit())?;
    let (letters, digits) = text.split_at(split);
    if letters.is_empty() || !letters.bytes().all(|b| b.is_ascii_alphabetic()) {
        return None;
    }
    // Bijective base-26: A=1, Z=26, AA=27, ...
    let mut col1 = 0u32;
    for b in letters.bytes() {
        let digit = (b.to_ascii_uppercase() - b'A' + 1) as u32;
        col1 = col1.checked_mul(26)?.checked_add(digit)?;
    }
    let row1: u32 = digits.parse().ok()?;
    if col1 == 0 || row1 == 0 {
        return None;
    }
    Some((col1 - 1, row1 - 1))
}

/// Per-column and per-row sizes. A column/row absent from the maps uses
/// the default size, so an empty `GridMetrics` is a uniform grid.
#[derive(Debug, Clone, Default)]
pub struct GridMetrics {
    col_w: HashMap<u32, f32>,
    row_h: HashMap<u32, f32>,
}

impl GridMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn col_width(&self, col: u32) -> f32 {
        self.col_w.get(&col).copied().unwrap_or(DEFAULT_COL_W)
    }

    pub fn row_height(&self, row: u32) -> f32 {
        self.row_h.get(&row).copied().unwrap_or(DEFAULT_ROW_H)
    }

    /// Set a column's width, clamped to at least [`MIN_COL_W`].
    pub fn set_col_width(&mut self, col: u32, width: f32) {
        self.col_w.insert(col, width.max(MIN_COL_W));
    }

    /// Set a row's height, clamped to at least [`MIN_ROW_H`].
    pub fn set_row_height(&mut self, row: u32, height: f32) {
        self.row_h.insert(row, height.max(MIN_ROW_H));
    }

    /// X of a column's left edge, relative to the grid origin. Column 0
    /// starts just past the row-header band.
    pub fn col_left(&self, col: u32) -> f32 {
        HEADER_W + (0..col).map(|c| self.col_width(c)).sum::<f32>()
    }

    /// Y of a row's top edge, relative to the grid origin.
    pub fn row_top(&self, row: u32) -> f32 {
        HEADER_H + (0..row).map(|r| self.row_height(r)).sum::<f32>()
    }

    /// Total grid width including the row-header band.
    pub fn total_width(&self, cols: u32) -> f32 {
        self.col_left(cols)
    }

    /// Total grid height including the column-header band.
    pub fn total_height(&self, rows: u32) -> f32 {
        self.row_top(rows)
    }

    /// The on-screen rectangle of a cell, given the grid's top-left origin.
    pub fn cell_rect(&self, origin: Pos2, col: u32, row: u32) -> Rect {
        let min = pos2(origin.x + self.col_left(col), origin.y + self.row_top(row));
        Rect::from_min_size(min, Vec2::new(self.col_width(col), self.row_height(row)))
    }

    /// The cell a point falls in. `None` if the point is in a header band,
    /// above/left of the grid, or past the last `cols`/`rows` cell.
    pub fn cell_at(&self, origin: Pos2, p: Pos2, cols: u32, rows: u32) -> Option<(u32, u32)> {
        let local = p - origin;
        if local.x < HEADER_W || local.y < HEADER_H {
            return None;
        }
        let col = self.axis_index(local.x, cols, |c| self.col_width(c), HEADER_W)?;
        let row = self.axis_index(local.y, rows, |r| self.row_height(r), HEADER_H)?;
        Some((col, row))
    }

    /// Walk an axis, accumulating sizes, to find which index `local`
    /// (a grid-relative coordinate) lands in. `header` is the leading band.
    fn axis_index(
        &self,
        local: f32,
        count: u32,
        size: impl Fn(u32) -> f32,
        header: f32,
    ) -> Option<u32> {
        let mut edge = header;
        for i in 0..count {
            edge += size(i);
            if local < edge {
                return Some(i);
            }
        }
        None
    }

    /// If `p` is in the column-header band and near a column's right
    /// border, the column that border resizes.
    pub fn col_border_at(&self, origin: Pos2, p: Pos2, cols: u32) -> Option<u32> {
        let local = p - origin;
        if local.y < 0.0 || local.y >= HEADER_H {
            return None;
        }
        let mut edge = HEADER_W;
        for c in 0..cols {
            edge += self.col_width(c);
            if (local.x - edge).abs() <= BORDER_GRAB {
                return Some(c);
            }
        }
        None
    }

    /// If `p` is in the row-header band and near a row's bottom border,
    /// the row that border resizes.
    pub fn row_border_at(&self, origin: Pos2, p: Pos2, rows: u32) -> Option<u32> {
        let local = p - origin;
        if local.x < 0.0 || local.x >= HEADER_W {
            return None;
        }
        let mut edge = HEADER_H;
        for r in 0..rows {
            edge += self.row_height(r);
            if (local.y - edge).abs() <= BORDER_GRAB {
                return Some(r);
            }
        }
        None
    }

    /// If `p` lies in the column-header band — but not within a resize
    /// grab zone, which [`GridMetrics::col_border_at`] claims first — the
    /// column whose header it hits. `None` outside the band or past the
    /// last column.
    pub fn col_header_at(&self, origin: Pos2, p: Pos2, cols: u32) -> Option<u32> {
        if self.col_border_at(origin, p, cols).is_some() {
            return None;
        }
        let local = p - origin;
        if local.x < HEADER_W || local.y < 0.0 || local.y >= HEADER_H {
            return None;
        }
        self.axis_index(local.x, cols, |c| self.col_width(c), HEADER_W)
    }

    /// If `p` lies in the row-header band — but not within a resize grab
    /// zone, which [`GridMetrics::row_border_at`] claims first — the row
    /// whose header it hits. `None` outside the band or past the last row.
    pub fn row_header_at(&self, origin: Pos2, p: Pos2, rows: u32) -> Option<u32> {
        if self.row_border_at(origin, p, rows).is_some() {
            return None;
        }
        let local = p - origin;
        if local.y < HEADER_H || local.x < 0.0 || local.x >= HEADER_W {
            return None;
        }
        self.axis_index(local.y, rows, |r| self.row_height(r), HEADER_H)
    }

    /// The column whose band contains `p`'s x-coordinate, clamped into
    /// `0..cols` — for sweeping a column selection by dragging, where
    /// the pointer ranges freely past the grid and `y` is irrelevant.
    /// `0` when `cols` is `0`.
    pub fn col_at_x(&self, origin: Pos2, p: Pos2, cols: u32) -> u32 {
        if cols == 0 {
            return 0;
        }
        let x = (p - origin).x;
        if x < HEADER_W {
            return 0;
        }
        self.axis_index(x, cols, |c| self.col_width(c), HEADER_W)
            .unwrap_or(cols - 1)
    }

    /// The row whose band contains `p`'s y-coordinate, clamped into
    /// `0..rows` — the row analogue of [`GridMetrics::col_at_x`].
    pub fn row_at_y(&self, origin: Pos2, p: Pos2, rows: u32) -> u32 {
        if rows == 0 {
            return 0;
        }
        let y = (p - origin).y;
        if y < HEADER_H {
            return 0;
        }
        self.axis_index(y, rows, |r| self.row_height(r), HEADER_H)
            .unwrap_or(rows - 1)
    }
}

/// Whether `p` lies in the header corner — the box above the row-header
/// band and left of the column-header band. Clicking it selects the
/// whole sheet, as spreadsheets do.
pub fn in_header_corner(origin: Pos2, p: Pos2) -> bool {
    let local = p - origin;
    local.x >= 0.0 && local.x < HEADER_W && local.y >= 0.0 && local.y < HEADER_H
}

/// The Ctrl+Arrow block-jump target along one axis. `start` is the
/// current index, `max` the last valid index, `occupied(i)` whether
/// cell `i` holds content; `forward` jumps toward `max`, else toward 0.
///
/// Mirrors a spreadsheet's block-jump: from inside a run of content,
/// jump to that run's far end; from a run's trailing edge (the next
/// cell is empty), or from an empty cell, skip to the next content
/// cell — or to the grid edge when there is no further content.
pub fn jump_target(start: u32, max: u32, forward: bool, occupied: impl Fn(u32) -> bool) -> u32 {
    let edge = if forward { max } else { 0 };
    if start == edge {
        return start;
    }
    let step = |i: u32| if forward { i + 1 } else { i - 1 };
    let next = step(start);
    if occupied(start) && occupied(next) {
        // Inside a run of content — go to its far end.
        let mut i = next;
        while i != edge && occupied(step(i)) {
            i = step(i);
        }
        i
    } else {
        // Skip the gap to the next content cell, or land on the edge.
        let mut i = next;
        while i != edge && !occupied(i) {
            i = step(i);
        }
        i
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn column_labels_are_bijective_base_26() {
        assert_eq!(column_label(0), "A");
        assert_eq!(column_label(25), "Z");
        assert_eq!(column_label(26), "AA");
        assert_eq!(column_label(701), "ZZ");
    }

    #[test]
    fn cell_addresses() {
        assert_eq!(cell_address(0, 0), "A1");
        assert_eq!(cell_address(27, 99), "AB100");
    }

    #[test]
    fn parse_address_inverts_cell_address() {
        assert_eq!(parse_address("A1"), Some((0, 0)));
        assert_eq!(parse_address("B7"), Some((1, 6)));
        assert_eq!(parse_address("Z9"), Some((25, 8)));
        assert_eq!(parse_address("AA1"), Some((26, 0)));
        assert_eq!(parse_address("AB100"), Some((27, 99)));
        // Letter case and surrounding whitespace are tolerated.
        assert_eq!(parse_address("  c3  "), Some((2, 2)));
        // Round-trips with cell_address.
        assert_eq!(parse_address(&cell_address(27, 99)), Some((27, 99)));
    }

    #[test]
    fn parse_address_rejects_malformed_input() {
        assert_eq!(parse_address(""), None);
        assert_eq!(parse_address("B"), None); // no row number
        assert_eq!(parse_address("7"), None); // no column letters
        assert_eq!(parse_address("B0"), None); // row 0 does not exist
        assert_eq!(parse_address("7B"), None); // digits before letters
        assert_eq!(parse_address("B7B"), None); // trailing letters
        assert_eq!(parse_address("B 7"), None); // embedded space
    }

    #[test]
    fn default_metrics_are_uniform() {
        let m = GridMetrics::new();
        assert_eq!(m.col_width(0), DEFAULT_COL_W);
        assert_eq!(m.col_width(99), DEFAULT_COL_W);
        assert_eq!(m.row_height(5), DEFAULT_ROW_H);
        assert_eq!(m.col_left(0), HEADER_W);
        assert_eq!(m.col_left(2), HEADER_W + 2.0 * DEFAULT_COL_W);
    }

    #[test]
    fn resizing_clamps_to_a_minimum_and_shifts_later_columns() {
        let mut m = GridMetrics::new();
        m.set_col_width(0, 200.0);
        assert_eq!(m.col_width(0), 200.0);
        assert_eq!(m.col_left(1), HEADER_W + 200.0);
        // Below the minimum is clamped up.
        m.set_col_width(1, 1.0);
        assert_eq!(m.col_width(1), MIN_COL_W);
        m.set_row_height(3, 0.0);
        assert_eq!(m.row_height(3), MIN_ROW_H);
    }

    #[test]
    fn cell_at_inverts_cell_rect_after_a_resize() {
        let mut m = GridMetrics::new();
        m.set_col_width(0, 150.0);
        m.set_row_height(0, 40.0);
        let origin = pos2(10.0, 20.0);
        for (c, r) in [(0, 0), (1, 1), (3, 5)] {
            let mid = m.cell_rect(origin, c, r).center();
            assert_eq!(m.cell_at(origin, mid, 8, 8), Some((c, r)));
        }
        // Header band and out-of-range yield None.
        assert_eq!(m.cell_at(origin, pos2(12.0, 22.0), 8, 8), None);
        let far = m.cell_rect(origin, 7, 7).center();
        assert_eq!(m.cell_at(origin, far, 4, 4), None);
    }

    #[test]
    fn col_border_hit_test() {
        let m = GridMetrics::new();
        let origin = pos2(0.0, 0.0);
        // The right border of column 0 is at HEADER_W + DEFAULT_COL_W.
        let border_x = HEADER_W + DEFAULT_COL_W;
        assert_eq!(
            m.col_border_at(origin, pos2(border_x, HEADER_H / 2.0), 8),
            Some(0),
        );
        // Mid-column is not a border.
        assert_eq!(
            m.col_border_at(origin, pos2(HEADER_W + DEFAULT_COL_W / 2.0, 5.0), 8),
            None,
        );
        // Below the header band is not a column-resize zone.
        assert_eq!(
            m.col_border_at(origin, pos2(border_x, HEADER_H + 50.0), 8),
            None,
        );
    }

    #[test]
    fn row_border_hit_test() {
        let m = GridMetrics::new();
        let origin = pos2(0.0, 0.0);
        let border_y = HEADER_H + DEFAULT_ROW_H;
        assert_eq!(
            m.row_border_at(origin, pos2(HEADER_W / 2.0, border_y), 8),
            Some(0),
        );
        assert_eq!(
            m.row_border_at(origin, pos2(HEADER_W + 50.0, border_y), 8),
            None,
        );
    }

    #[test]
    fn col_header_hit_test() {
        let m = GridMetrics::new();
        let origin = pos2(0.0, 0.0);
        // Mid-header of column 0 — inside the band, clear of any border.
        let x0 = HEADER_W + DEFAULT_COL_W / 2.0;
        assert_eq!(
            m.col_header_at(origin, pos2(x0, HEADER_H / 2.0), 8),
            Some(0)
        );
        // Column 2's header.
        let x2 = HEADER_W + 2.5 * DEFAULT_COL_W;
        assert_eq!(m.col_header_at(origin, pos2(x2, 4.0), 8), Some(2));
        // Below the header band is not a header click.
        assert_eq!(m.col_header_at(origin, pos2(x0, HEADER_H + 30.0), 8), None);
        // The row-header band (x < HEADER_W) is not a column header.
        assert_eq!(m.col_header_at(origin, pos2(10.0, 4.0), 8), None);
        // A resize-border zone yields None — col_border_at claims it.
        let border_x = HEADER_W + DEFAULT_COL_W;
        assert_eq!(m.col_header_at(origin, pos2(border_x, 4.0), 8), None);
        // Past the last column is None.
        assert_eq!(
            m.col_header_at(origin, pos2(HEADER_W + 20.0 * DEFAULT_COL_W, 4.0), 8),
            None,
        );
    }

    #[test]
    fn row_header_hit_test() {
        let m = GridMetrics::new();
        let origin = pos2(0.0, 0.0);
        let y0 = HEADER_H + DEFAULT_ROW_H / 2.0;
        assert_eq!(
            m.row_header_at(origin, pos2(HEADER_W / 2.0, y0), 8),
            Some(0)
        );
        // Right of the row-header band is not a row header.
        assert_eq!(m.row_header_at(origin, pos2(HEADER_W + 30.0, y0), 8), None);
        // The column-header band (y < HEADER_H) is not a row header.
        assert_eq!(m.row_header_at(origin, pos2(10.0, 4.0), 8), None);
        // A resize-border zone yields None.
        let border_y = HEADER_H + DEFAULT_ROW_H;
        assert_eq!(
            m.row_header_at(origin, pos2(HEADER_W / 2.0, border_y), 8),
            None,
        );
    }

    #[test]
    fn header_corner_hit_test() {
        let origin = pos2(0.0, 0.0);
        // Inside the corner box.
        assert!(in_header_corner(
            origin,
            pos2(HEADER_W / 2.0, HEADER_H / 2.0)
        ));
        // The column-header band is not the corner.
        assert!(!in_header_corner(
            origin,
            pos2(HEADER_W + 10.0, HEADER_H / 2.0)
        ));
        // The row-header band is not the corner.
        assert!(!in_header_corner(
            origin,
            pos2(HEADER_W / 2.0, HEADER_H + 10.0)
        ));
        // Above or left of the grid is not the corner.
        assert!(!in_header_corner(origin, pos2(-5.0, -5.0)));
    }

    #[test]
    fn col_at_x_clamps_into_range() {
        let m = GridMetrics::new();
        let origin = pos2(0.0, 0.0);
        // Left of the grid clamps to column 0.
        assert_eq!(m.col_at_x(origin, pos2(-50.0, 100.0), 8), 0);
        assert_eq!(m.col_at_x(origin, pos2(HEADER_W / 2.0, 100.0), 8), 0);
        // Mid-grid resolves the column, ignoring y.
        let x3 = HEADER_W + 3.5 * DEFAULT_COL_W;
        assert_eq!(m.col_at_x(origin, pos2(x3, 999.0), 8), 3);
        // Past the last column clamps to the last.
        let far = HEADER_W + 50.0 * DEFAULT_COL_W;
        assert_eq!(m.col_at_x(origin, pos2(far, 5.0), 8), 7);
        // No columns is a safe zero.
        assert_eq!(m.col_at_x(origin, pos2(x3, 5.0), 0), 0);
    }

    #[test]
    fn row_at_y_clamps_into_range() {
        let m = GridMetrics::new();
        let origin = pos2(0.0, 0.0);
        // Above the grid clamps to row 0.
        assert_eq!(m.row_at_y(origin, pos2(100.0, -50.0), 8), 0);
        // Mid-grid resolves the row, ignoring x.
        let y4 = HEADER_H + 4.5 * DEFAULT_ROW_H;
        assert_eq!(m.row_at_y(origin, pos2(999.0, y4), 8), 4);
        // Past the last row clamps to the last.
        let far = HEADER_H + 50.0 * DEFAULT_ROW_H;
        assert_eq!(m.row_at_y(origin, pos2(5.0, far), 8), 7);
    }

    #[test]
    fn jump_target_from_inside_a_run_goes_to_its_far_end() {
        // [X X X . . X], indices 0..=5.
        let cells = [true, true, true, false, false, true];
        let occ = |i: u32| cells[i as usize];
        // Forward from the run's start lands on its last cell.
        assert_eq!(jump_target(0, 5, true, occ), 2);
        // Backward from the run's end lands on its first cell.
        assert_eq!(jump_target(2, 5, false, occ), 0);
    }

    #[test]
    fn jump_target_skips_a_gap_to_the_next_content() {
        let cells = [true, true, true, false, false, true];
        let occ = |i: u32| cells[i as usize];
        // Forward from the run's trailing edge skips the gap to index 5.
        assert_eq!(jump_target(2, 5, true, occ), 5);
        // Backward from index 5 skips back to the run's end at index 2.
        assert_eq!(jump_target(5, 5, false, occ), 2);
    }

    #[test]
    fn jump_target_from_an_empty_cell_finds_the_next_content() {
        // [. . X . .].
        let cells = [false, false, true, false, false];
        let occ = |i: u32| cells[i as usize];
        assert_eq!(jump_target(0, 4, true, occ), 2);
        assert_eq!(jump_target(4, 4, false, occ), 2);
    }

    #[test]
    fn jump_target_runs_to_the_edge_when_no_content_remains() {
        let empty = |_: u32| false;
        // An all-empty axis jumps straight to the far edge.
        assert_eq!(jump_target(0, 9, true, empty), 9);
        assert_eq!(jump_target(9, 9, false, empty), 0);
    }

    #[test]
    fn jump_target_at_the_edge_stays_put() {
        let occ = |_: u32| true;
        assert_eq!(jump_target(9, 9, true, occ), 9);
        assert_eq!(jump_target(0, 9, false, occ), 0);
    }
}
