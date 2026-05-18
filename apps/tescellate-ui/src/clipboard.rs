//! The clipboard — the one first-class copy / cut / paste object.
//!
//! Each captured cell keeps **both** its raw source and its evaluated
//! value. A paste onto the *same* kind of sheet writes the source, so
//! formulas carry; a paste onto a *different* kind writes the value as a
//! plain literal, since the source's cell references (`B2`, `H(1,0)`, …)
//! don't translate across lattices. That makes copy/paste first-class:
//! data moves between any cell type, formulas degrade gracefully to
//! their result when they cross a lattice boundary.
//!
//! **Cut is part of this same object** — not a separate per-sheet field.
//! A cut is just a captured block that also remembers its origin; the
//! next paste clears that origin (when it lands on the cut's own
//! lattice) and the cut is consumed.
//!
//! No egui and no engine here, so the whole model is `cargo test`-able.

/// One captured cell — its source, and its value rendered as a literal.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CopiedCell {
    /// The raw source as the user typed it. `None` is a blank cell.
    pub source: Option<String>,
    /// The cell's evaluated value as a plain, re-typeable literal — what
    /// a cross-lattice paste writes in place of the (untranslatable)
    /// source. `None` when the cell evaluates to nothing.
    pub value: Option<String>,
}

/// A rectangular block of captured cells, stored row-major — and, when
/// the block was cut rather than copied, the origin to clear on paste.
#[derive(Debug, Clone, Default)]
pub struct Clipboard {
    width: u32,
    height: u32,
    cells: Vec<CopiedCell>,
    /// Whether the block was captured from the hex sheet — paste compares
    /// this against the destination to choose source vs. value.
    from_hex: bool,
    /// `Some(leading_corner)` when this block was **cut** — the smallest
    /// `(col,row)` / `(q,r)` of the origin region, in the source
    /// lattice's own integer coords. With `dimensions()` and `from_hex`
    /// it fully describes the region the next paste clears. `None` for a
    /// plain copy.
    cut_origin: Option<(i32, i32)>,
}

impl Clipboard {
    /// Capture a `width × height` block, row-major, as a **copy**.
    /// `from_hex` records which kind of sheet it came from.
    pub fn capture(width: u32, height: u32, cells: Vec<CopiedCell>, from_hex: bool) -> Self {
        debug_assert_eq!(cells.len(), (width * height) as usize);
        Self {
            width,
            height,
            cells,
            from_hex,
            cut_origin: None,
        }
    }

    /// Turn a freshly-captured block into a **cut**: record the leading
    /// corner of the origin region so the next paste can clear it.
    pub fn mark_as_cut(&mut self, origin: (i32, i32)) {
        self.cut_origin = Some(origin);
    }

    /// Whether the clipboard holds nothing.
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    /// `(width, height)` of the captured block.
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Whether the block was captured from the hex sheet.
    pub fn from_hex(&self) -> bool {
        self.from_hex
    }

    /// The leading corner of the cut origin region — `Some` only when
    /// this block was cut. With `dimensions()` it gives the whole region.
    pub fn cut_origin(&self) -> Option<(i32, i32)> {
        self.cut_origin
    }

    /// Drop the cut origin — a paste has consumed the cut, so further
    /// pastes of this block behave as a plain copy.
    pub fn consume_cut(&mut self) {
        self.cut_origin = None;
    }

    /// Every captured cell as `(rel_col, rel_row, &cell)`, row-major.
    pub fn entries(&self) -> impl Iterator<Item = (u32, u32, &CopiedCell)> {
        let width = self.width;
        self.cells.iter().enumerate().map(move |(i, cell)| {
            let i = i as u32;
            (i % width, i / width, cell)
        })
    }

    /// What to write when pasting `cell` onto a sheet that is (or isn't)
    /// the hex sheet. Same kind as the capture → the source, so formulas
    /// carry; different kind → the value, since the source can't
    /// translate across lattices.
    pub fn source_for(&self, cell: &CopiedCell, target_is_hex: bool) -> Option<String> {
        if self.from_hex == target_is_hex {
            cell.source.clone()
        } else {
            cell.value.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A formula cell: a source plus the value it evaluates to.
    fn formula(source: &str, value: &str) -> CopiedCell {
        CopiedCell {
            source: Some(source.to_string()),
            value: Some(value.to_string()),
        }
    }

    #[test]
    fn a_default_clipboard_is_empty() {
        let c = Clipboard::default();
        assert!(c.is_empty());
        assert_eq!(c.dimensions(), (0, 0));
        assert_eq!(c.entries().count(), 0);
        assert_eq!(c.cut_origin(), None);
    }

    #[test]
    fn capture_records_dimensions_and_entries() {
        let cells = vec![
            formula("=1", "1"),
            CopiedCell::default(),
            formula("=2", "2"),
            formula("=3", "3"),
        ];
        let c = Clipboard::capture(2, 2, cells, false);
        assert!(!c.is_empty());
        assert_eq!(c.dimensions(), (2, 2));
        let got: Vec<_> = c.entries().collect();
        assert_eq!(got.len(), 4);
        assert_eq!(got[1], (1, 0, &CopiedCell::default()));
        assert_eq!(got[3].2.source.as_deref(), Some("=3"));
    }

    #[test]
    fn a_plain_capture_is_a_copy_not_a_cut() {
        let c = Clipboard::capture(1, 1, vec![formula("=1", "1")], false);
        assert_eq!(c.cut_origin(), None);
    }

    #[test]
    fn mark_as_cut_then_consume() {
        let mut c = Clipboard::capture(1, 1, vec![formula("=1", "1")], true);
        c.mark_as_cut((2, -3));
        assert_eq!(c.cut_origin(), Some((2, -3)));
        assert!(c.from_hex());
        // A paste consumes the cut — afterwards it is a plain copy.
        c.consume_cut();
        assert_eq!(c.cut_origin(), None);
    }

    #[test]
    fn same_lattice_paste_keeps_the_source() {
        let c = Clipboard::capture(1, 1, vec![formula("=SUM(B2:B4)", "60")], false);
        let (.., cell) = c.entries().next().unwrap();
        assert_eq!(c.source_for(cell, false), Some("=SUM(B2:B4)".to_string()));
    }

    #[test]
    fn cross_lattice_paste_degrades_to_the_value() {
        // Copied from the square sheet, pasted onto the hex sheet — the
        // formula's references can't translate, so the value is written.
        let c = Clipboard::capture(1, 1, vec![formula("=SUM(B2:B4)", "60")], false);
        let (.., cell) = c.entries().next().unwrap();
        assert_eq!(c.source_for(cell, true), Some("60".to_string()));

        // And the other direction: hex capture pasted onto the square sheet.
        let h = Clipboard::capture(1, 1, vec![formula("=H(1,0)", "12")], true);
        let (.., hcell) = h.entries().next().unwrap();
        assert_eq!(h.source_for(hcell, false), Some("12".to_string()));
        assert_eq!(h.source_for(hcell, true), Some("=H(1,0)".to_string()));
    }
}
