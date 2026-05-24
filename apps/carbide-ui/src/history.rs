//! A generic undo/redo history — two stacks of entries.
//!
//! `History<E>` is pure: it knows nothing about cells or the engine, it
//! just shuffles opaque entries between an undo stack and a redo stack.
//! `app.rs` instantiates it with a batch of cell edits as the entry type.
//! The whole model is exercised by ordinary `cargo test`.

/// Two stacks — done actions and undone actions — over an opaque entry.
pub struct History<E> {
    /// Applied actions, most recent on top.
    undo_stack: Vec<E>,
    /// Undone actions available to redo, most recent on top.
    redo_stack: Vec<E>,
}

impl<E: Clone> History<E> {
    pub fn new() -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    /// Record a freshly-applied action. This invalidates the redo
    /// future — once you act, the actions you'd undone can't come back.
    pub fn record(&mut self, entry: E) {
        self.undo_stack.push(entry);
        self.redo_stack.clear();
    }

    /// Remove and return the most recent undo entry, leaving the redo
    /// stack untouched — used to coalesce a fresh action into the
    /// previous one (the caller re-`record`s the merged result).
    pub fn pop_undo(&mut self) -> Option<E> {
        self.undo_stack.pop()
    }

    /// Take the most recent action to undo. It moves to the redo stack,
    /// so a later [`redo`](Self::redo) can replay it.
    pub fn undo(&mut self) -> Option<E> {
        let entry = self.undo_stack.pop()?;
        self.redo_stack.push(entry.clone());
        Some(entry)
    }

    /// Take the most recently undone action to redo. It moves back onto
    /// the undo stack.
    pub fn redo(&mut self) -> Option<E> {
        let entry = self.redo_stack.pop()?;
        self.undo_stack.push(entry.clone());
        Some(entry)
    }

    /// Whether there is anything to undo.
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// Whether there is anything to redo.
    /// Drop both stacks. Used after loading a workbook so undo doesn't
    /// reach back into actions from the prior workbook.
    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }
}

impl<E: Clone> Default for History<E> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_history_has_nothing_to_undo_or_redo() {
        let h: History<i32> = History::new();
        assert!(!h.can_undo());
        assert!(!h.can_redo());
    }

    #[test]
    fn record_then_walk_the_undo_and_redo_stacks() {
        let mut h: History<i32> = History::new();
        h.record(1);
        h.record(2);
        assert!(h.can_undo());

        // Undo pops most-recent-first.
        assert_eq!(h.undo(), Some(2));
        assert_eq!(h.undo(), Some(1));
        assert_eq!(h.undo(), None);
        assert!(!h.can_undo());
        assert!(h.can_redo());

        // Redo replays them in reverse.
        assert_eq!(h.redo(), Some(1));
        assert_eq!(h.redo(), Some(2));
        assert_eq!(h.redo(), None);
        assert!(h.can_undo());
    }

    #[test]
    fn recording_a_new_action_clears_the_redo_future() {
        let mut h: History<i32> = History::new();
        h.record(1);
        h.record(2);
        h.undo(); // redo stack now holds [2]
        assert!(h.can_redo());

        h.record(99); // a fresh action invalidates the redo future
        assert!(!h.can_redo());
        // The new action is still undoable, on top of the kept one.
        assert_eq!(h.undo(), Some(99));
        assert_eq!(h.undo(), Some(1));
    }

    #[test]
    fn an_undo_can_be_redone_then_undone_again() {
        let mut h: History<&str> = History::new();
        h.record("edit");
        assert_eq!(h.undo(), Some("edit"));
        assert_eq!(h.redo(), Some("edit"));
        assert_eq!(h.undo(), Some("edit"));
        assert!(h.can_redo());
    }
}
