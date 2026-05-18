//! Find — substring search (optionally case-sensitive) over a sheet.
//!
//! [`FindState`] holds the query, the cells that currently match it, and
//! which match is "current"; `app.rs` populates the matches by feeding
//! it each cell's source text. The match predicate, the replacement, and
//! the wrap-around stepping are pure, so `cargo test` covers them.

/// Whether `text` contains `query` — case-insensitively unless
/// `case_sensitive`. An empty query matches nothing.
pub fn cell_matches(query: &str, text: &str, case_sensitive: bool) -> bool {
    if query.is_empty() {
        return false;
    }
    if case_sensitive {
        text.contains(query)
    } else {
        text.to_lowercase().contains(&query.to_lowercase())
    }
}

/// Replace every occurrence of `query` in `source` with `replacement` —
/// case-insensitively unless `case_sensitive`. Returns the rewritten
/// string; equal to `source` when `query` is empty or absent. The
/// case-insensitive path scans char-by-char, so it is correct for any
/// Unicode rather than byte-slicing a lowercased copy.
pub fn replace_all(source: &str, query: &str, replacement: &str, case_sensitive: bool) -> String {
    if query.is_empty() {
        return source.to_string();
    }
    if case_sensitive {
        return source.replace(query, replacement);
    }
    let lower_query = query.to_lowercase();
    let query_len = query.chars().count();
    let chars: Vec<char> = source.chars().collect();
    let mut result = String::new();
    let mut i = 0;
    while i < chars.len() {
        let window: String = chars[i..].iter().take(query_len).collect();
        if window.chars().count() == query_len && window.to_lowercase() == lower_query {
            result.push_str(replacement);
            i += query_len;
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }
    result
}

/// The Find panel's state: the query, the matching cells (any lattice's
/// coordinate `K`), and the index of the current match within them.
#[derive(Debug, Clone)]
pub struct FindState<K> {
    pub query: String,
    /// The replacement text for Find & Replace.
    pub replace: String,
    /// Whether matching is case-sensitive.
    pub case_sensitive: bool,
    matches: Vec<K>,
    current: usize,
}

impl<K> Default for FindState<K> {
    fn default() -> Self {
        Self {
            query: String::new(),
            replace: String::new(),
            case_sensitive: false,
            matches: Vec::new(),
            current: 0,
        }
    }
}

impl<K: Copy + PartialEq> FindState<K> {
    /// Rebuild the match list from `(cell, text)` pairs, resetting the
    /// current match to the first. Call whenever the query or the sheet
    /// contents change.
    pub fn refresh(&mut self, cells: impl Iterator<Item = (K, String)>) {
        let query = self.query.clone();
        let case_sensitive = self.case_sensitive;
        self.matches = cells
            .filter(|(_, text)| cell_matches(&query, text, case_sensitive))
            .map(|(cell, _)| cell)
            .collect();
        self.current = 0;
    }

    /// Forget the query, the replacement, and every match.
    pub fn clear(&mut self) {
        self.query.clear();
        self.replace.clear();
        self.matches.clear();
        self.current = 0;
    }

    /// How many cells match.
    pub fn match_count(&self) -> usize {
        self.matches.len()
    }

    /// The current match, or `None` when nothing matches.
    pub fn current_match(&self) -> Option<K> {
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
    pub fn step(&mut self, forward: bool) -> Option<K> {
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
    pub fn is_match(&self, cell: K) -> bool {
        self.matches.contains(&cell)
    }

    /// Every matching cell, in scan order — what Replace All iterates.
    pub fn matches(&self) -> &[K] {
        &self.matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_matches_substring_case_insensitive_by_default() {
        assert!(cell_matches("ab", "xxABzz", false));
        assert!(cell_matches("HELLO", "hello world", false));
        assert!(cell_matches("7", "1700", false));
        assert!(!cell_matches("qq", "hello", false));
        // A blank query matches nothing.
        assert!(!cell_matches("", "anything", false));
    }

    #[test]
    fn cell_matches_respects_case_sensitivity() {
        // Case-insensitive: "AB" finds "ab".
        assert!(cell_matches("AB", "xabz", false));
        // Case-sensitive: it does not; the exact case still does.
        assert!(!cell_matches("AB", "xabz", true));
        assert!(cell_matches("ab", "xabz", true));
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

    #[test]
    fn replace_all_is_case_insensitive_and_unicode_safe() {
        assert_eq!(replace_all("hello world", "o", "0", false), "hell0 w0rld");
        // Case-insensitive find; the replacement is written verbatim.
        assert_eq!(replace_all("Banana", "A", "_", false), "B_n_n_");
        // No occurrence — unchanged.
        assert_eq!(replace_all("hello", "zzz", "x", false), "hello");
        // An empty query is a no-op.
        assert_eq!(replace_all("hello", "", "x", false), "hello");
        // Replacing with empty deletes the matches.
        assert_eq!(replace_all("a-b-c", "-", "", false), "abc");
        // Multi-char, case-insensitive, non-overlapping.
        assert_eq!(replace_all("xXxX", "xx", "y", false), "yy");
        // Non-ASCII content is handled per char.
        assert_eq!(replace_all("café CAFÉ", "café", "tea", false), "tea tea");
    }

    #[test]
    fn replace_all_case_sensitive_only_hits_exact_case() {
        // Case-sensitive replace touches only the matching case.
        assert_eq!(replace_all("aAaA", "a", "x", true), "xAxA");
        // Case-insensitive replace hits every case.
        assert_eq!(replace_all("aAaA", "a", "x", false), "xxxx");
    }
}
