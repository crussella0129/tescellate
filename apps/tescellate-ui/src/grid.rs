//! Pure grid geometry — A1 addressing and cell ↔ pixel mapping.
//!
//! Deliberately framework-light: it touches only egui's plain geometry
//! value types (`Rect`, `Pos2`), so every function here is exercised by
//! ordinary `cargo test`, no browser or GUI required.

use egui::{pos2, Rect, Vec2};

/// Width of a cell, in points.
pub const CELL_W: f32 = 84.0;
/// Height of a cell, in points.
pub const CELL_H: f32 = 22.0;
/// Width of the row-number header column.
pub const HEADER_W: f32 = 44.0;
/// Height of the column-letter header row.
pub const HEADER_H: f32 = 22.0;

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

/// The on-screen rectangle of a zero-indexed cell, given the grid's
/// top-left origin (the corner of the header band).
pub fn cell_rect(origin_x: f32, origin_y: f32, col: u32, row: u32) -> Rect {
    let x = origin_x + HEADER_W + col as f32 * CELL_W;
    let y = origin_y + HEADER_H + row as f32 * CELL_H;
    Rect::from_min_size(pos2(x, y), Vec2::new(CELL_W, CELL_H))
}

/// Which cell a point falls in, given the grid's top-left origin. Returns
/// `None` when the point lands in the header band or above/left of it.
pub fn cell_at(origin_x: f32, origin_y: f32, px: f32, py: f32) -> Option<(u32, u32)> {
    let local_x = px - origin_x - HEADER_W;
    let local_y = py - origin_y - HEADER_H;
    if local_x < 0.0 || local_y < 0.0 {
        return None;
    }
    Some(((local_x / CELL_W) as u32, (local_y / CELL_H) as u32))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn column_labels_are_bijective_base_26() {
        assert_eq!(column_label(0), "A");
        assert_eq!(column_label(25), "Z");
        assert_eq!(column_label(26), "AA");
        assert_eq!(column_label(27), "AB");
        assert_eq!(column_label(51), "AZ");
        assert_eq!(column_label(52), "BA");
        assert_eq!(column_label(701), "ZZ");
    }

    #[test]
    fn cell_addresses() {
        assert_eq!(cell_address(0, 0), "A1");
        assert_eq!(cell_address(2, 4), "C5");
        assert_eq!(cell_address(27, 99), "AB100");
    }

    #[test]
    fn cell_rect_places_the_first_cell_after_the_header() {
        let r = cell_rect(0.0, 0.0, 0, 0);
        assert_eq!(r.min, pos2(HEADER_W, HEADER_H));
        assert_eq!(r.width(), CELL_W);
        assert_eq!(r.height(), CELL_H);
    }

    #[test]
    fn cell_at_is_the_inverse_of_cell_rect() {
        for (c, r) in [(0, 0), (1, 2), (7, 13)] {
            let rect = cell_rect(10.0, 20.0, c, r);
            let mid = rect.center();
            assert_eq!(cell_at(10.0, 20.0, mid.x, mid.y), Some((c, r)));
        }
    }

    #[test]
    fn cell_at_rejects_the_header_band() {
        assert_eq!(cell_at(0.0, 0.0, 5.0, 5.0), None);
        assert_eq!(cell_at(0.0, 0.0, HEADER_W + 5.0, 5.0), None);
        assert_eq!(cell_at(0.0, 0.0, 5.0, HEADER_H + 5.0), None);
    }
}
