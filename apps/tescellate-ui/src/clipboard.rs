//! The in-app clipboard — a rectangular block of copied cells.
//!
//! Each copied cell keeps **both** its raw source and its evaluated
//! value. A paste onto the *same* kind of sheet writes the source, so
//! formulas carry; a paste onto a *different* kind writes the value as a
//! plain literal, since the source's cell references (`B2`, `H(1,0)`, …)
//! don't translate across lattices. That makes copy/paste first-class:
//! data moves between any cell type, formulas degrade gracefully to
//! their result when they cross a lattice boundary.
//!
//! No egui and no engine here, so the whole model is `cargo test`-able.

/// One copied cell — its source, and its value rendered as a literal.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CopiedCell {
    /// The raw source as the user typed it. `None` is a blank cell.
    pub source: Option<String>,
    /// The cell's evaluated value as a plain, re-typeable literal — what
    /// a cross-lattice paste writes in place of the (untranslatable)
    /// source. `None` when the cell evaluates to nothing.
    pub value: Option<String>,
}

/// A rectangular block of copied cells, stored row-major.
#[derive(Debug, Clone, Default)]
pub struct Clipboard {
    width: u32,
    height: u32,
    cells: Vec<CopiedCell>,
    /// Whether the block was copied from the hex sheet — paste compares
    /// this against the destination to choose source vs. value.
    from_hex: bool,
}

impl Clipboard {
    /// Capture a `width × height` block, row-major. `from_hex` records
    /// which kind of sheet it came from.
    pub fn capture(width: u32, height: u32, cells: Vec<CopiedCell>, from_hex: bool) -> Self {
        debug_assert_eq!(cells.len(), (width * height) as usize);
        Self {
            width,
            height,
            cells,
            from_hex,
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

    /// Every captured cell as `(rel_col, rel_row, &cell)`, row-major.
    pub fn entries(&self) -> impl Iterator<Item = (u32, u32, &CopiedCell)> {
        let width = self.width;
        self.cells.iter().enumerate().map(move |(i, cell)| {
            let i = i as u32;
            (i % width, i / width, cell)
        })
    }

    /// What to write when pasting `cell` onto a sheet that is (or isn't)
    /// the hex sheet. Same kind as the copy → the source, so formulas
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
        assert_eq!(got[0].0, 0);
        assert_eq!(got[0].1, 0);
        assert_eq!(got[1], (1, 0, &CopiedCell::default()));
        assert_eq!(got[3].2.source.as_deref(), Some("=3"));
    }

    #[test]
    fn same_lattice_paste_keeps_the_source() {
        // Copied from the square sheet, pasted onto the square sheet.
        let c = Clipboard::capture(1, 1, vec![formula("=SUM(B2:B4)", "60")], false);
        let (.., cell) = c.entries().next().unwrap();
        assert_eq!(c.source_for(cell, false), Some("=SUM(B2:B4)".to_string()),);
    }

    #[test]
    fn cross_lattice_paste_degrades_to_the_value() {
        // Copied from the square sheet, pasted onto the hex sheet — the
        // formula's references can't translate, so the value is written.
        let c = Clipboard::capture(1, 1, vec![formula("=SUM(B2:B4)", "60")], false);
        let (.., cell) = c.entries().next().unwrap();
        assert_eq!(c.source_for(cell, true), Some("60".to_string()));

        // And the other direction: hex copy pasted onto the square sheet.
        let h = Clipboard::capture(1, 1, vec![formula("=H(1,0)", "12")], true);
        let (.., hcell) = h.entries().next().unwrap();
        assert_eq!(h.source_for(hcell, false), Some("12".to_string()));
        assert_eq!(h.source_for(hcell, true), Some("=H(1,0)".to_string()));
    }
}
