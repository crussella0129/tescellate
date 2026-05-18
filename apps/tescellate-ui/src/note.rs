//! Cell notes — a free-text comment attached to a cell.
//!
//! [`NoteMap`] is a plain map from a lattice coordinate `K` to a note
//! string. A cell absent from the map has no note, and setting a
//! blank note removes the entry — so an empty map is a sheet with no
//! notes. No egui and no engine here, so `cargo test` covers it.

use std::collections::HashMap;

/// Free-text notes attached to cells, keyed by a lattice's coordinate
/// type `K` — `(u32, u32)` for the square sheet, `HexCoord` for the hex
/// sheet.
#[derive(Debug, Clone)]
pub struct NoteMap<K> {
    notes: HashMap<K, String>,
}

impl<K> Default for NoteMap<K> {
    fn default() -> Self {
        Self {
            notes: HashMap::new(),
        }
    }
}

impl<K: Eq + std::hash::Hash + Copy> NoteMap<K> {
    pub fn new() -> Self {
        Self::default()
    }

    /// The note on a cell — `""` when it has none.
    pub fn get(&self, cell: K) -> &str {
        self.notes.get(&cell).map(String::as_str).unwrap_or("")
    }

    /// Whether a cell carries a note.
    pub fn has(&self, cell: K) -> bool {
        self.notes.contains_key(&cell)
    }

    /// Set a cell's note. Text that is empty or all whitespace removes
    /// the note instead, so the map only ever holds real content.
    pub fn set(&mut self, cell: K, text: impl Into<String>) {
        let text = text.into();
        if text.trim().is_empty() {
            self.notes.remove(&cell);
        } else {
            self.notes.insert(cell, text);
        }
    }

    /// How many cells carry a note.
    pub fn count(&self) -> usize {
        self.notes.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_get_and_has() {
        let mut m: NoteMap<(u32, u32)> = NoteMap::new();
        assert!(!m.has((1, 1)));
        assert_eq!(m.get((1, 1)), "");
        m.set((1, 1), "check this");
        assert!(m.has((1, 1)));
        assert_eq!(m.get((1, 1)), "check this");
        assert_eq!(m.count(), 1);
    }

    #[test]
    fn blank_text_removes_the_note() {
        let mut m: NoteMap<(u32, u32)> = NoteMap::new();
        m.set((2, 3), "temporary");
        assert!(m.has((2, 3)));
        // Whitespace-only is treated as empty and clears the note.
        m.set((2, 3), "   ");
        assert!(!m.has((2, 3)));
        assert_eq!(m.count(), 0);
    }

    #[test]
    fn set_overwrites_an_existing_note() {
        let mut m: NoteMap<(u32, u32)> = NoteMap::new();
        m.set((0, 0), "first");
        m.set((0, 0), "second");
        assert_eq!(m.get((0, 0)), "second");
        assert_eq!(m.count(), 1);
    }

    #[test]
    fn works_with_a_hex_key() {
        use tescellate_tess::hex::HexCoord;
        let mut m: NoteMap<HexCoord> = NoteMap::new();
        m.set(HexCoord::new(1, -2), "hex note");
        assert!(m.has(HexCoord::new(1, -2)));
        assert_eq!(m.get(HexCoord::new(0, 0)), "");
    }
}
