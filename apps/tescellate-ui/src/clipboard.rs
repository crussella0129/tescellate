//! The in-app clipboard — a rectangular block of copied cell sources.
//!
//! Copying a selection captures each cell's *source* (the formula or
//! literal as typed); pasting writes those sources back at a new origin.
//! No egui and no engine here, so the whole model is exercised by
//! ordinary `cargo test`.

/// A rectangular block of copied cell sources, stored row-major. A `None`
/// entry is a blank cell — pasting one clears its target.
#[derive(Debug, Clone, Default)]
pub struct Clipboard {
    width: u32,
    height: u32,
    cells: Vec<Option<String>>,
}

impl Clipboard {
    /// Capture a `width × height` block of sources, row-major.
    pub fn capture(width: u32, height: u32, cells: Vec<Option<String>>) -> Self {
        debug_assert_eq!(cells.len(), (width * height) as usize);
        Self {
            width,
            height,
            cells,
        }
    }

    /// Whether the clipboard holds nothing.
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    /// `(width, height)` of the captured block.
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// The source at a block-relative `(col, row)`, if it is in range.
    pub fn get(&self, col: u32, row: u32) -> Option<&str> {
        if col >= self.width || row >= self.height {
            return None;
        }
        self.cells[(row * self.width + col) as usize].as_deref()
    }

    /// Every captured cell as `(rel_col, rel_row, source)`, row-major.
    pub fn entries(&self) -> impl Iterator<Item = (u32, u32, Option<&str>)> {
        let width = self.width;
        self.cells.iter().enumerate().map(move |(i, source)| {
            let i = i as u32;
            (i % width, i / width, source.as_deref())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn src(s: &str) -> Option<String> {
        Some(s.to_string())
    }

    #[test]
    fn a_default_clipboard_is_empty() {
        let c = Clipboard::default();
        assert!(c.is_empty());
        assert_eq!(c.dimensions(), (0, 0));
        assert_eq!(c.entries().count(), 0);
    }

    #[test]
    fn capture_then_get_round_trips() {
        let c = Clipboard::capture(2, 2, vec![src("a"), src("b"), None, src("d")]);
        assert!(!c.is_empty());
        assert_eq!(c.dimensions(), (2, 2));
        assert_eq!(c.get(0, 0), Some("a"));
        assert_eq!(c.get(1, 0), Some("b"));
        assert_eq!(c.get(0, 1), None);
        assert_eq!(c.get(1, 1), Some("d"));
        // Out of the block's range.
        assert_eq!(c.get(2, 0), None);
        assert_eq!(c.get(0, 2), None);
    }

    #[test]
    fn entries_enumerate_row_major() {
        let c = Clipboard::capture(2, 2, vec![src("a"), src("b"), src("c"), src("d")]);
        let got: Vec<_> = c.entries().collect();
        assert_eq!(got.len(), 4);
        assert_eq!(got[0], (0, 0, Some("a")));
        assert_eq!(got[1], (1, 0, Some("b")));
        assert_eq!(got[2], (0, 1, Some("c")));
        assert_eq!(got[3], (1, 1, Some("d")));
    }

    #[test]
    fn a_single_cell_clipboard_holds_one_source() {
        let c = Clipboard::capture(1, 1, vec![src("=1+1")]);
        assert_eq!(c.dimensions(), (1, 1));
        assert_eq!(c.get(0, 0), Some("=1+1"));
        assert_eq!(c.entries().count(), 1);
    }
}
