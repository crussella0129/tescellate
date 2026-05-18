//! Find — case-insensitive substring search over the square sheet.
//!
//! [`FindState`] holds the query, the cells that currently match it, and
//! which match is "current"; `app.rs` populates the matches by feeding
//! it each cell's display text. The match predicate and the wrap-around
//! stepping are pure, so `cargo test` covers them.

/// Whether `text` contains `query`, case-insensitively. An empty query
/// matches nothing — Find with a blank box highlights no cells.
pub fn cell_matches(query: &str, text: &str) -> bool {
    !query.is_empty() && text.to_lowercase().contains(&query.to_lowercase())
}

/// The Find panel's state: the query, the matching cells in row-major
/// order, and the index of the current match within them.
#[derive(Debug, Clone, Default)]
pub struct FindState {
    pub query: String,
    matches: Vec<(u32, u32)>,
    current: usize,
}

impl FindState {
    /// Rebuild the match list from `(cell, text)` pairs, resetting the
    /// current match to the first. Call whenever the query or the sheet
    /// contents change.
    pub fn refresh(&mut self, cells: impl Iterator<Item = ((u32, u32), String)>) {
        let query = self.query.clone();
        self.matches = cells
            .filter(|(_, text)| cell_matches(&query, text))
            .map(|(cell, _)| cell)
            .collect();
        self.current = 0;
    }

    /// Forget the query and every match.
    pub fn clear(&mut self) {
        self.query.clear();
        self.matches.clear();
        self.current = 0;
    }

    /// How many cells match.
    pub fn match_count(&self) -> usize {
        self.matches.len()
    }

    /// The current match, or `None` when nothing matches.
    pub fn current_match(&self) -> Option<(u32, u32)> {
        self.matches.get(self.current).copied()
    }

    /// One-based index of the current match, for an "n of m" label; `0`
    /// when there are no matches.
    pub fn current_index(&self) -> usize {
        if self.matches.is_empty() {
            0
        } else {
            self.current + 1
        }
    }

    /// Step to the next (`forward`) or previous match, wrapping around,
    /// and return it. `None` when there are no matches.
    pub fn step(&mut self, forward: bool) -> Option<(u32, u32)> {
        let len = self.matches.len();
        if len == 0 {
            return None;
        }
        self.current = if forward {
            (self.current + 1) % len
        } else {
            (self.current + len - 1) % len
        };
        self.current_match()
    }

    /// Whether `cell` is one of the matches — used to tint it in the grid.
    pub fn is_match(&self, cell: (u32, u32)) -> bool {
        self.matches.contains(&cell)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_matches_is_a_case_insensitive_substring() {
        assert!(cell_matches("ab", "xxABzz"));
        assert!(cell_matches("HELLO", "hello world"));
        assert!(cell_matches("7", "1700"));
        assert!(!cell_matches("qq", "hello"));
        // A blank query matches nothing.
        assert!(!cell_matches("", "anything"));
    }

    fn sheet() -> Vec<((u32, u32), String)> {
        vec![
            ((0, 0), "apple".to_string()),
            ((1, 0), "banana".to_string()),
            ((0, 1), "Apricot".to_string()),
            ((1, 1), "cherry".to_string()),
        ]
    }

    #[test]
    fn refresh_collects_matching_cells_in_order() {
        let mut f = FindState {
            query: "ap".to_string(),
            ..Default::default()
        };
        f.refresh(sheet().into_iter());
        // "apple" and "Apricot" both contain "ap".
        assert_eq!(f.match_count(), 2);
        assert_eq!(f.current_match(), Some((0, 0)));
        assert_eq!(f.current_index(), 1);
        assert!(f.is_match((0, 1)));
        assert!(!f.is_match((1, 1)));
    }

    #[test]
    fn step_wraps_in_both_directions() {
        let mut f = FindState {
            query: "a".to_string(),
            ..Default::default()
        };
        f.refresh(sheet().into_iter());
        // "apple", "banana", "Apricot" each contain an 'a'.
        assert_eq!(f.match_count(), 3);
        assert_eq!(f.current_match(), Some((0, 0)));
        assert_eq!(f.step(true), Some((1, 0)));
        assert_eq!(f.step(true), Some((0, 1)));
        // Forward off the end wraps to the first.
        assert_eq!(f.step(true), Some((0, 0)));
        // Backward off the start wraps to the last.
        assert_eq!(f.step(false), Some((0, 1)));
    }

    #[test]
    fn no_matches_yields_nothing() {
        let mut f = FindState {
            query: "zzz".to_string(),
            ..Default::default()
        };
        f.refresh(sheet().into_iter());
        assert_eq!(f.match_count(), 0);
        assert_eq!(f.current_match(), None);
        assert_eq!(f.current_index(), 0);
        assert_eq!(f.step(true), None);
        assert!(!f.is_match((0, 0)));
    }
}
