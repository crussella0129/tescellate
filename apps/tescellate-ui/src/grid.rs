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
pub const BORDER_GRAB: f32 = 6.0;

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

    /// All non-default column widths, as `(col, width)` pairs. Used by
    /// the state-IO snapshot to round-trip user-resized columns.
    pub fn col_widths_iter(&self) -> impl Iterator<Item = (u32, f32)> + '_ {
        self.col_w.iter().map(|(c, w)| (*c, *w))
    }

    /// All non-default row heights, as `(row, height)` pairs.
    pub fn row_heights_iter(&self) -> impl Iterator<Item = (u32, f32)> + '_ {
        self.row_h.iter().map(|(r, h)| (*r, *h))
    }

    /// Replace stored col widths / row heights from `(idx, size)` pairs.
    /// Sizes below the per-axis minimum are clamped on the way in. Used
    /// by state-IO snapshots when restoring a saved workbook.
    pub fn replace_with(
        &mut self,
        col_widths: impl IntoIterator<Item = (u32, f32)>,
        row_heights: impl IntoIterator<Item = (u32, f32)>,
    ) {
        self.col_w = col_widths
            .into_iter()
            .map(|(c, w)| (c, w.max(MIN_COL_W)))
            .collect();
        self.row_h = row_heights
            .into_iter()
            .map(|(r, h)| (r, h.max(MIN_ROW_H)))
            .collect();
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

    /// The inclusive `(start, end)` column index span whose cells overlap
    /// the horizontal clip window `[clip_left, clip_right]` (screen
    /// coords), given the grid content's left edge at `origin_x`. Used to
    /// cull the render to on-screen columns instead of painting all of
    /// them. One incremental pass over the axis; a cell is included when
    /// its span overlaps the window inclusively (so an edge-straddling
    /// cell still paints). An empty axis returns `(0, 0)`.
    pub fn visible_col_range(
        &self,
        origin_x: f32,
        clip_left: f32,
        clip_right: f32,
        cols: u32,
    ) -> (u32, u32) {
        if cols == 0 {
            return (0, 0);
        }
        let rel_left = clip_left - origin_x;
        let rel_right = clip_right - origin_x;
        let mut x = HEADER_W;
        let mut start = 0u32;
        let mut end = cols - 1;
        let mut found_start = false;
        for c in 0..cols {
            let w = self.col_width(c);
            let cell_left = x;
            let cell_right = x + w;
            if !found_start && cell_right >= rel_left {
                start = c;
                found_start = true;
            }
            if cell_left <= rel_right {
                end = c;
            } else {
                break;
            }
            x += w;
        }
        (start, end)
    }

    /// Row analogue of [`GridMetrics::visible_col_range`].
    pub fn visible_row_range(
        &self,
        origin_y: f32,
        clip_top: f32,
        clip_bottom: f32,
        rows: u32,
    ) -> (u32, u32) {
        if rows == 0 {
            return (0, 0);
        }
        let rel_top = clip_top - origin_y;
        let rel_bottom = clip_bottom - origin_y;
        let mut y = HEADER_H;
        let mut start = 0u32;
        let mut end = rows - 1;
        let mut found_start = false;
        for r in 0..rows {
            let h = self.row_height(r);
            let cell_top = y;
            let cell_bottom = y + h;
            if !found_start && cell_bottom >= rel_top {
                start = r;
                found_start = true;
            }
            if cell_top <= rel_bottom {
                end = r;
            } else {
                break;
            }
            y += h;
        }
        (start, end)
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

    /// Like [`GridMetrics::cell_at`], but with both the row header
    /// frozen at viewport-x `header_x` and the column header frozen at
    /// viewport-y `header_y`: a point inside either floating header
    /// band hits no cell, so a click on a frozen header is not mistaken
    /// for the cell scrolled beneath it.
    pub fn cell_at_frozen(
        &self,
        origin: Pos2,
        header_x: f32,
        header_y: f32,
        p: Pos2,
        cols: u32,
        rows: u32,
    ) -> Option<(u32, u32)> {
        if p.x < header_x + HEADER_W || p.y < header_y + HEADER_H {
            return None;
        }
        self.cell_at(origin, p, cols, rows)
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

    /// Snapshot the current column/row geometry into a [`GridLayout`] with
    /// precomputed prefix sums, so the render loop can position cells in
    /// O(1) instead of `cell_rect`'s O(index) `col_left`/`row_top` walk.
    /// Built once per frame in `draw_grid`; one O(cols + rows) pass.
    pub fn layout(&self, cols: u32, rows: u32) -> GridLayout {
        let mut col_lefts = Vec::with_capacity(cols as usize + 1);
        let mut x = HEADER_W;
        col_lefts.push(x);
        for c in 0..cols {
            x += self.col_width(c);
            col_lefts.push(x);
        }
        let mut row_tops = Vec::with_capacity(rows as usize + 1);
        let mut y = HEADER_H;
        row_tops.push(y);
        for r in 0..rows {
            y += self.row_height(r);
            row_tops.push(y);
        }
        GridLayout {
            col_lefts,
            row_tops,
        }
    }
}

/// Precomputed per-frame grid geometry: prefix sums of column lefts and
/// row tops, so cell positioning is O(1). Produced by
/// [`GridMetrics::layout`]; valid for the frame it was built in (a
/// column resize mutates the metrics and is reflected next frame).
pub struct GridLayout {
    /// `col_lefts[c]` is the left edge of column `c`; length `cols + 1`,
    /// so `col_lefts[cols]` is the grid's right edge.
    col_lefts: Vec<f32>,
    /// `row_tops[r]` is the top edge of row `r`; length `rows + 1`.
    row_tops: Vec<f32>,
}

impl GridLayout {
    /// Left edge of column `c` relative to the grid origin. O(1).
    pub fn col_left(&self, col: u32) -> f32 {
        self.col_lefts[(col as usize).min(self.col_lefts.len() - 1)]
    }

    /// Top edge of row `r` relative to the grid origin. O(1).
    pub fn row_top(&self, row: u32) -> f32 {
        self.row_tops[(row as usize).min(self.row_tops.len() - 1)]
    }

    /// Width of column `c` (difference of adjacent prefix sums). O(1).
    pub fn col_width(&self, col: u32) -> f32 {
        let i = col as usize;
        if i + 1 < self.col_lefts.len() {
            self.col_lefts[i + 1] - self.col_lefts[i]
        } else {
            DEFAULT_COL_W
        }
    }

    /// Height of row `r`. O(1).
    pub fn row_height(&self, row: u32) -> f32 {
        let i = row as usize;
        if i + 1 < self.row_tops.len() {
            self.row_tops[i + 1] - self.row_tops[i]
        } else {
            DEFAULT_ROW_H
        }
    }

    /// The screen rect of cell `(col, row)` given the grid `origin`.
    /// Matches [`GridMetrics::cell_rect`] exactly. O(1).
    pub fn cell_rect(&self, origin: Pos2, col: u32, row: u32) -> Rect {
        let min = pos2(origin.x + self.col_left(col), origin.y + self.row_top(row));
        Rect::from_min_size(min, Vec2::new(self.col_width(col), self.row_height(row)))
    }
}

/// Whether `p` lies in the header corner — the box above the row-header
/// band and left of the column-header band. Clicking it selects the
/// whole sheet, as spreadsheets do.
pub fn in_header_corner(origin: Pos2, p: Pos2) -> bool {
    let local = p - origin;
    local.x >= 0.0 && local.x < HEADER_W && local.y >= 0.0 && local.y < HEADER_H
}

/// A block-jump along a 1-D walk — the spreadsheet Ctrl+arrow rule.
/// From `start`, follow `step` (which yields the next coordinate, or
/// `None` at the boundary): from inside a run of content (`occupied`),
/// stop at the run's far end; from a run's trailing edge or an empty
/// cell, stop at the next content cell; failing that, stop at the last
/// cell before the boundary. Lattice-agnostic — the square sheet walks
/// an index, the hex sheet walks axial coordinates.
pub fn block_jump<C: Copy>(
    start: C,
    step: impl Fn(C) -> Option<C>,
    occupied: impl Fn(C) -> bool,
) -> C {
    let Some(next) = step(start) else {
        return start;
    };
    if occupied(start) && occupied(next) {
        // Inside a run of content — advance to its far end.
        let mut cur = next;
        while let Some(ahead) = step(cur) {
            if !occupied(ahead) {
                break;
            }
            cur = ahead;
        }
        cur
    } else {
        // Skip the gap to the next content cell, or stop at the boundary.
        let mut cur = next;
        loop {
            if occupied(cur) {
                return cur;
            }
            match step(cur) {
                Some(ahead) => cur = ahead,
                None => return cur,
            }
        }
    }
}

/// The Ctrl+Arrow block-jump target along one grid axis — [`block_jump`]
/// over an index in `0..=max`. `forward` jumps toward `max`, else 0.
pub fn jump_target(start: u32, max: u32, forward: bool, occupied: impl Fn(u32) -> bool) -> u32 {
    let step = |i: u32| {
        if forward {
            (i < max).then_some(i + 1)
        } else {
            i.checked_sub(1)
        }
    };
    block_jump(start, step, occupied)
}

/// The extent — a column width or row height — that fits a set of
/// measured content sizes: the largest plus `padding`, never below
/// `min`. An empty iterator yields `min`, so a blank column autofits to
/// its minimum width.
pub fn fit_extent(measured: impl Iterator<Item = f32>, padding: f32, min: f32) -> f32 {
    let widest = measured.fold(0.0_f32, f32::max);
    (widest + padding).max(min)
}

/// The AutoSum-style action for a selection whose inclusive corners are
/// `bounds`: the cell that should hold the result, and the
/// `=FUNC(range)` formula to put there — `func` is an aggregate name
/// such as `"SUM"` or `"AVERAGE"`. The result lands directly below the
/// selection's bottom-left corner; `None` when that row would fall past
/// `rows`.
pub fn autosum(
    bounds: ((u32, u32), (u32, u32)),
    rows: u32,
    func: &str,
) -> Option<((u32, u32), String)> {
    let ((min_c, min_r), (max_c, max_r)) = bounds;
    let target_row = max_r + 1;
    if target_row >= rows {
        return None;
    }
    let range = if (min_c, min_r) == (max_c, max_r) {
        cell_address(min_c, min_r)
    } else {
        format!(
            "{}:{}",
            cell_address(min_c, min_r),
            cell_address(max_c, max_r),
        )
    };
    Some(((min_c, target_row), format!("={func}({range})")))
}

/// The values for a "fill series" over `total` cells, extrapolating an
/// arithmetic progression from `seed` — the cells that already hold
/// numbers. The step is the gap between the first two seed values, or
/// `1.0` when there are fewer than two; the run starts at the first
/// seed value (or `0.0` when the seed is empty). The result restates
/// the seed cells and continues the progression through the rest.
pub fn series_fill(seed: &[f64], total: usize) -> Vec<f64> {
    let start = seed.first().copied().unwrap_or(0.0);
    let step = match seed {
        [a, b, ..] => b - a,
        _ => 1.0,
    };
    (0..total).map(|i| start + step * i as f64).collect()
}

/// Extend a fill-handle lane by `count` cells past the seed.
///
/// `seed` is the original column/row's cell sources (each `None` for an
/// empty cell, `Some(text)` for a value-or-formula). The result has
/// exactly `count` items, the values to write into the cells past the
/// seed:
///
/// * If `seed.len() >= 2` AND every `Some` parses as `f64` (empties are
///   ignored), continue an arithmetic progression — the step between
///   the first two numeric seeds — just like [`series_fill`] does.
///   Empty seeds become empty extensions too (a `None` slot).
/// * Otherwise the seed pattern repeats: cell `i` of the extension
///   copies from `seed[i % seed.len()]`.
///
/// `format_number` lets callers control how the synthetic numeric
/// values come back as strings (Tescellate uses Excel-style trimming
/// of trailing zeros).
pub fn fill_lane(
    seed: &[Option<String>],
    count: usize,
    format_number: impl Fn(f64) -> String,
) -> Vec<Option<String>> {
    if seed.is_empty() || count == 0 {
        return vec![None; count];
    }
    // Try to read every non-empty seed as a number; if any non-empty
    // seed fails, we fall back to pattern repeat.
    let parsed: Vec<Option<f64>> = seed
        .iter()
        .map(|s| s.as_deref().map(str::trim).map(str::parse::<f64>))
        .map(|opt| match opt {
            Some(Ok(n)) => Some(n),
            Some(Err(_)) => Some(f64::NAN),
            None => None,
        })
        .collect();
    let any_nan = parsed.iter().any(|p| p.is_some_and(f64::is_nan));
    let numerics_seen = parsed.iter().filter(|p| p.is_some()).count();
    if !any_nan && numerics_seen >= 2 {
        // Compute the step from the first two numeric seeds; preserve
        // the empty-cell slots in the extension at the same lane
        // positions they occupy in the seed pattern.
        let numerics: Vec<f64> = parsed.iter().filter_map(|p| *p).collect();
        let step = numerics[1] - numerics[0];
        let last = *numerics.last().unwrap();
        // Count how many numeric slots elapse between the last seed
        // cell and the "extended position" each output cell sits at.
        let mut out = Vec::with_capacity(count);
        let mut produced_numeric = 0usize; // how many numeric cells we've emitted so far
        for i in 0..count {
            // The position in the cyclic seed pattern.
            let pos = (seed.len() + i) % seed.len();
            if parsed[pos].is_none() {
                out.push(None);
            } else {
                produced_numeric += 1;
                // Numbers continue from `last` with `step` between
                // each numeric slot.
                let value = last + step * produced_numeric as f64;
                out.push(Some(format_number(value)));
            }
        }
        return out;
    }
    // Pattern repeat — `seed[i % seed.len()]` for each extension cell.
    (0..count).map(|i| seed[i % seed.len()].clone()).collect()
}

/// The row a Page Up / Page Down lands on: `start` shifted by `page`
/// rows toward `0` (`up`) or toward `max` (down), clamped to `0..=max`.
pub fn page_step(start: u32, up: bool, page: u32, max: u32) -> u32 {
    if up {
        start.saturating_sub(page)
    } else {
        (start + page).min(max)
    }
}

/// The current data region around `cursor` — its inclusive `(min, max)`
/// corners. Starting from the cursor's 1x1 box, the box grows outward
/// while the row or column just beyond an edge holds any non-empty cell
/// (`occupied`) within the box's span, to a fixpoint. An isolated cell
/// yields just itself. `cols` / `rows` bound the grow.
pub fn current_region(
    cursor: (u32, u32),
    cols: u32,
    rows: u32,
    occupied: impl Fn(u32, u32) -> bool,
) -> ((u32, u32), (u32, u32)) {
    let (mut min_c, mut min_r) = cursor;
    let (mut max_c, mut max_r) = cursor;
    loop {
        let mut grew = false;
        if min_r > 0 && (min_c..=max_c).any(|c| occupied(c, min_r - 1)) {
            min_r -= 1;
            grew = true;
        }
        if max_r + 1 < rows && (min_c..=max_c).any(|c| occupied(c, max_r + 1)) {
            max_r += 1;
            grew = true;
        }
        if min_c > 0 && (min_r..=max_r).any(|r| occupied(min_c - 1, r)) {
            min_c -= 1;
            grew = true;
        }
        if max_c + 1 < cols && (min_r..=max_r).any(|r| occupied(max_c + 1, r)) {
            max_c += 1;
            grew = true;
        }
        if !grew {
            return ((min_c, min_r), (max_c, max_r));
        }
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
    fn layout_matches_cell_rect() {
        let m = GridMetrics::new();
        let layout = m.layout(52, 200);
        let origin = pos2(7.0, 11.0);
        for &(c, r) in &[(0u32, 0u32), (5, 3), (51, 199)] {
            assert_eq!(
                layout.cell_rect(origin, c, r),
                m.cell_rect(origin, c, r),
                "layout rect must match cell_rect at ({c}, {r})"
            );
        }
    }

    #[test]
    fn layout_matches_cell_rect_with_overrides() {
        let mut m = GridMetrics::new();
        m.set_col_width(2, 200.0);
        m.set_row_height(4, 40.0);
        let layout = m.layout(52, 200);
        let origin = pos2(0.0, 0.0);
        // Before, at, and after each override.
        for &(c, r) in &[(1u32, 3u32), (2, 4), (5, 6)] {
            assert_eq!(
                layout.cell_rect(origin, c, r),
                m.cell_rect(origin, c, r),
                "layout rect must match cell_rect at ({c}, {r}) with overrides"
            );
        }
    }

    #[test]
    fn layout_col_width_row_height_accessors() {
        let mut m = GridMetrics::new();
        m.set_col_width(3, 150.0);
        m.set_row_height(7, 33.0);
        let layout = m.layout(52, 200);
        for c in [0u32, 3, 10] {
            assert_eq!(layout.col_width(c), m.col_width(c));
        }
        for r in [0u32, 7, 50] {
            assert_eq!(layout.row_height(r), m.row_height(r));
        }
    }

    #[test]
    fn visible_range_full_when_everything_fits() {
        let m = GridMetrics::new();
        // A clip window wider/taller than the whole grid → full range.
        let wide = HEADER_W + 100.0 * DEFAULT_COL_W;
        assert_eq!(m.visible_col_range(0.0, 0.0, wide, 52), (0, 51));
        let tall = HEADER_H + 1000.0 * DEFAULT_ROW_H;
        assert_eq!(m.visible_row_range(0.0, 0.0, tall, 200), (0, 199));
    }

    #[test]
    fn visible_range_windows_when_scrolled() {
        let m = GridMetrics::new();
        // Scroll right: origin_x is negative (content shifted left under
        // a fixed viewport). Viewport is [0, 400] in screen coords.
        let origin_x = -(HEADER_W + 10.0 * DEFAULT_COL_W); // 10 cols scrolled off
        let (c0, c1) = m.visible_col_range(origin_x, 0.0, 400.0, 52);
        assert!(
            c0 >= 9,
            "start should skip the scrolled-off columns, got {c0}"
        );
        assert!(c1 < 51, "end should not reach the last column, got {c1}");
        assert!(c1 >= c0);
    }

    #[test]
    fn visible_range_includes_boundary_straddle() {
        let m = GridMetrics::new();
        // Place the clip-left exactly partway through column 5's span.
        // Column 5 straddles the boundary and must be included.
        let col5_mid = HEADER_W + 5.0 * DEFAULT_COL_W + DEFAULT_COL_W / 2.0;
        let (c0, _) = m.visible_col_range(0.0, col5_mid, col5_mid + 10.0, 52);
        assert_eq!(c0, 5, "the straddling column must be the start");
    }

    #[test]
    fn visible_range_empty_axis_is_zero_zero() {
        let m = GridMetrics::new();
        assert_eq!(m.visible_col_range(0.0, 0.0, 500.0, 0), (0, 0));
        assert_eq!(m.visible_row_range(0.0, 0.0, 500.0, 0), (0, 0));
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

    #[test]
    fn fit_extent_takes_the_widest_plus_padding() {
        // 55 (the widest) + 10 padding = 65.
        assert_eq!(fit_extent([20.0, 55.0, 30.0].into_iter(), 10.0, 32.0), 65.0);
    }

    #[test]
    fn fit_extent_clamps_to_the_minimum() {
        // Narrow content still leaves the column at least `min` wide.
        assert_eq!(fit_extent([4.0, 2.0].into_iter(), 10.0, 32.0), 32.0);
        // An empty column (nothing measured) fits to the minimum.
        assert_eq!(fit_extent(std::iter::empty::<f32>(), 10.0, 32.0), 32.0);
    }

    #[test]
    fn autosum_sums_a_column_into_the_cell_below() {
        // A1:A5 selected -> =SUM(A1:A5) lands in A6.
        assert_eq!(
            autosum(((0, 0), (0, 4)), 32, "SUM"),
            Some(((0, 5), "=SUM(A1:A5)".to_string())),
        );
    }

    #[test]
    fn autosum_handles_a_block_and_a_single_cell() {
        // A 2D block aggregates the whole rectangle below its bottom-left.
        assert_eq!(
            autosum(((1, 1), (3, 4)), 32, "SUM"),
            Some(((1, 5), "=SUM(B2:D5)".to_string())),
        );
        // A single cell aggregates just itself.
        assert_eq!(
            autosum(((2, 2), (2, 2)), 32, "SUM"),
            Some(((2, 3), "=SUM(C3)".to_string())),
        );
    }

    #[test]
    fn autosum_is_none_with_no_room_below() {
        // The selection's bottom row is the last row — nowhere to put it.
        assert_eq!(autosum(((0, 30), (0, 31)), 32, "SUM"), None);
    }

    #[test]
    fn autosum_uses_the_chosen_function() {
        assert_eq!(
            autosum(((0, 0), (0, 4)), 32, "AVERAGE"),
            Some(((0, 5), "=AVERAGE(A1:A5)".to_string())),
        );
    }

    #[test]
    fn series_fill_continues_an_arithmetic_progression() {
        // Seed 1,2 over 5 cells -> 1..5.
        assert_eq!(series_fill(&[1.0, 2.0], 5), vec![1.0, 2.0, 3.0, 4.0, 5.0]);
        // The step is the first gap.
        assert_eq!(series_fill(&[10.0, 20.0], 4), vec![10.0, 20.0, 30.0, 40.0]);
        // A descending step.
        assert_eq!(series_fill(&[9.0, 6.0], 4), vec![9.0, 6.0, 3.0, 0.0]);
    }

    #[test]
    fn series_fill_from_one_seed_steps_by_one() {
        assert_eq!(series_fill(&[5.0], 4), vec![5.0, 6.0, 7.0, 8.0]);
    }

    #[test]
    fn series_fill_with_no_seed_counts_from_zero() {
        assert_eq!(series_fill(&[], 3), vec![0.0, 1.0, 2.0]);
    }

    fn nf(n: f64) -> String {
        // Test format: integers without decimal, others with.
        if n.fract() == 0.0 {
            (n as i64).to_string()
        } else {
            n.to_string()
        }
    }

    fn s(text: &str) -> Option<String> {
        Some(text.to_string())
    }

    #[test]
    fn fill_lane_extends_a_numeric_progression() {
        // Seed 1, 2 → extend by 3 → 3, 4, 5
        let out = fill_lane(&[s("1"), s("2")], 3, nf);
        assert_eq!(out, vec![s("3"), s("4"), s("5")]);
    }

    #[test]
    fn fill_lane_uses_the_first_step() {
        // Seed 10, 20 → step 10 → 30, 40, 50, 60
        let out = fill_lane(&[s("10"), s("20")], 4, nf);
        assert_eq!(out, vec![s("30"), s("40"), s("50"), s("60")]);
    }

    #[test]
    fn fill_lane_extends_descending_progressions() {
        let out = fill_lane(&[s("9"), s("6")], 3, nf);
        assert_eq!(out, vec![s("3"), s("0"), s("-3")]);
    }

    #[test]
    fn fill_lane_repeats_a_text_pattern() {
        // Non-numeric → repeat cyclically.
        let out = fill_lane(&[s("foo"), s("bar")], 5, nf);
        assert_eq!(out, vec![s("foo"), s("bar"), s("foo"), s("bar"), s("foo")]);
    }

    #[test]
    fn fill_lane_repeats_a_mixed_pattern() {
        // Any non-numeric in the seed falls back to repeat.
        let out = fill_lane(&[s("1"), s("apple")], 4, nf);
        assert_eq!(out, vec![s("1"), s("apple"), s("1"), s("apple")]);
    }

    #[test]
    fn fill_lane_with_a_single_seed_copies_that_value() {
        // One value → cannot infer a step → repeat (which copies).
        let out = fill_lane(&[s("Apple")], 3, nf);
        assert_eq!(out, vec![s("Apple"), s("Apple"), s("Apple")]);
    }

    #[test]
    fn fill_lane_with_a_single_numeric_seed_repeats_it() {
        // One numeric — needs two to detect a step; repeat the value.
        let out = fill_lane(&[s("7")], 3, nf);
        assert_eq!(out, vec![s("7"), s("7"), s("7")]);
    }

    #[test]
    fn fill_lane_with_empty_count_returns_an_empty_extension() {
        assert!(fill_lane(&[s("1"), s("2")], 0, nf).is_empty());
    }

    #[test]
    fn fill_lane_with_empty_seed_returns_empties() {
        // Defensive: no seed → no pattern to repeat; the caller's empty
        // slots stay empty.
        assert_eq!(fill_lane(&[], 3, nf), vec![None, None, None]);
    }

    #[test]
    fn page_step_moves_a_page_and_clamps() {
        // Down a page from row 2.
        assert_eq!(page_step(2, false, 16, 31), 18);
        // Down clamps at the last row.
        assert_eq!(page_step(20, false, 16, 31), 31);
        // Up a page.
        assert_eq!(page_step(20, true, 16, 31), 4);
        // Up clamps at row 0.
        assert_eq!(page_step(5, true, 16, 31), 0);
    }

    #[test]
    fn current_region_grows_to_the_data_block() {
        // A 2x2 block of content spanning (1,1)..(2,2).
        let filled = |c, r| (1..=2).contains(&c) && (1..=2).contains(&r);
        assert_eq!(current_region((1, 1), 16, 32, filled), ((1, 1), (2, 2)));
        // The same region from any cell inside it.
        assert_eq!(current_region((2, 2), 16, 32, filled), ((1, 1), (2, 2)));
    }

    #[test]
    fn current_region_of_an_isolated_cell_is_itself() {
        assert_eq!(
            current_region((3, 4), 16, 32, |_, _| false),
            ((3, 4), (3, 4)),
        );
    }

    #[test]
    fn current_region_stops_at_empty_rows_and_columns() {
        // Content fills columns 0..1 and rows 0..1; beyond is empty.
        let filled = |c, r| c <= 1 && r <= 1;
        assert_eq!(current_region((0, 0), 16, 32, filled), ((0, 0), (1, 1)));
    }

    #[test]
    fn cell_at_frozen_excludes_both_floating_header_bands() {
        let m = GridMetrics::new();
        let origin = pos2(0.0, 0.0);
        // Unscrolled (header_x/y == origin.x/y) it matches cell_at exactly.
        let deep = pos2(HEADER_W + 10.0, HEADER_H + 10.0);
        assert_eq!(
            m.cell_at_frozen(origin, origin.x, origin.y, deep, 100, 100),
            m.cell_at(origin, deep, 100, 100),
        );
        assert!(m
            .cell_at_frozen(origin, origin.x, origin.y, deep, 100, 100)
            .is_some());
        // Scrolled down, the column header floats to y = 300: a point
        // inside that band hits no cell, though cell_at alone would.
        let in_col_band = pos2(HEADER_W + 10.0, 300.0 + HEADER_H / 2.0);
        assert!(m.cell_at(origin, in_col_band, 100, 100).is_some());
        assert_eq!(
            m.cell_at_frozen(origin, origin.x, 300.0, in_col_band, 100, 100),
            None,
        );
        // Just past the column band a cell is found again.
        let below = pos2(HEADER_W + 10.0, 300.0 + HEADER_H + 5.0);
        assert!(m
            .cell_at_frozen(origin, origin.x, 300.0, below, 100, 100)
            .is_some());
        // Scrolled right, the row header floats to x = 400: a point
        // inside that band hits no cell, though cell_at alone would.
        let in_row_band = pos2(400.0 + HEADER_W / 2.0, HEADER_H + 10.0);
        assert!(m.cell_at(origin, in_row_band, 100, 100).is_some());
        assert_eq!(
            m.cell_at_frozen(origin, 400.0, origin.y, in_row_band, 100, 100),
            None,
        );
        // Just past the row band a cell is found again.
        let right = pos2(400.0 + HEADER_W + 5.0, HEADER_H + 10.0);
        assert!(m
            .cell_at_frozen(origin, 400.0, origin.y, right, 100, 100)
            .is_some());
    }
}
