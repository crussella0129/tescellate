//! The pure keyboard-command layer.
//!
//! It maps a key press — `(key, modifiers, mode)` — to a spreadsheet
//! [`Command`]. There is no egui rendering and no engine here, only
//! `egui::Key` as a plain data enum, so the whole keyboard *model* is
//! exhaustively unit-testable with ordinary `cargo test`. `app.rs`
//! consumes the matching key events and routes them through this module,
//! so these tests genuinely pin the app's behaviour.

use crate::format::HAlign;
use egui::Key;

/// A movement direction on the grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    Up,
    Down,
    Left,
    Right,
}

/// Whether the grid is navigating between cells or editing a cell's text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Navigating,
    Editing,
}

/// A spreadsheet command — the interpreted result of a key press.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Move the selection one cell, collapsing any range to a single cell.
    Move(Dir),
    /// Extend the selection one cell — Shift+arrow. Moves the cursor and
    /// keeps the anchor, growing or shrinking the selected range.
    Extend(Dir),
    /// Select the whole sheet (Ctrl+A).
    SelectAll,
    /// Move to the first column of the current row (Home).
    MoveToRowStart,
    /// Move to the top-left cell (Ctrl+Home).
    MoveToOrigin,
    /// Begin editing the selected cell. `replace_with` is `Some(c)` when a
    /// character was typed — the cell's content is replaced, starting with
    /// `c` — or `None` for an F2-style edit of the existing content.
    BeginEdit { replace_with: Option<char> },
    /// Commit the in-progress edit, then move one cell.
    Commit(Dir),
    /// Discard the in-progress edit (Escape).
    Cancel,
    /// Clear the selected cell (Delete / Backspace while navigating).
    Clear,
    /// Toggle bold on the selected cell (Ctrl+B).
    ToggleBold,
    /// Toggle italic on the selected cell (Ctrl+I).
    ToggleItalic,
    /// Set the selected cell's horizontal alignment (Ctrl+Shift+L/E/R).
    SetAlign(HAlign),
    /// Copy the selected range to the clipboard (Ctrl+C).
    Copy,
    /// Cut the selected range — copies it, and the next paste clears it
    /// (Ctrl+X).
    Cut,
    /// Paste the clipboard at the active cell (Ctrl+V).
    Paste,
    /// Undo the most recent action (Ctrl+Z).
    Undo,
    /// Redo the most recently undone action (Ctrl+Y / Ctrl+Shift+Z).
    Redo,
    /// Fill the selection's top row down into it (Ctrl+D).
    FillDown,
    /// Fill the selection's left column rightward into it (Ctrl+R).
    FillRight,
    /// Open the Find panel (Ctrl+F).
    OpenFind,
    /// Open the keyboard-shortcuts help overlay (F1).
    OpenHelp,
}

/// The keyboard shortcuts, as `(keys, description)` — the data behind
/// the F1 help overlay.
pub const SHORTCUTS: &[(&str, &str)] = &[
    ("Arrow keys", "Move the selection"),
    ("Shift + arrows", "Extend the selection"),
    ("Ctrl+A", "Select the whole sheet"),
    ("Tab / Shift+Tab", "Move right / left"),
    ("Enter / Shift+Enter", "Move down / up"),
    ("Home", "Jump to the row start"),
    ("Ctrl+Home", "Jump to the top-left cell"),
    ("F2", "Edit the selected cell"),
    ("Delete / Backspace", "Clear the selection"),
    ("Ctrl+B / Ctrl+I", "Bold / italic"),
    ("Ctrl+Shift+L / E / R", "Align left / centre / right"),
    ("Ctrl+C / X / V", "Copy / cut / paste"),
    ("Ctrl+Z / Ctrl+Y", "Undo / redo"),
    ("Ctrl+D / Ctrl+R", "Fill down / right"),
    ("Ctrl+F", "Find & replace"),
    ("F1", "This shortcuts list"),
];

/// Interpret a non-text key press.
pub fn command_for_key(key: Key, shift: bool, ctrl: bool, mode: Mode) -> Option<Command> {
    match mode {
        Mode::Navigating => navigating(key, shift, ctrl),
        Mode::Editing => editing(key, shift),
    }
}

fn navigating(key: Key, shift: bool, ctrl: bool) -> Option<Command> {
    Some(match key {
        // Shift turns an arrow into a range extension rather than a move.
        Key::ArrowUp => move_or_extend(Dir::Up, shift),
        Key::ArrowDown => move_or_extend(Dir::Down, shift),
        Key::ArrowLeft => move_or_extend(Dir::Left, shift),
        Key::ArrowRight => move_or_extend(Dir::Right, shift),
        // Tab/Enter move the selection; Shift reverses the axis direction.
        Key::Tab => Command::Move(if shift { Dir::Left } else { Dir::Right }),
        Key::Enter => Command::Move(if shift { Dir::Up } else { Dir::Down }),
        Key::Home if ctrl => Command::MoveToOrigin,
        Key::Home => Command::MoveToRowStart,
        Key::F2 => Command::BeginEdit { replace_with: None },
        Key::Delete | Key::Backspace => Command::Clear,
        Key::A if ctrl => Command::SelectAll,
        // Formatting shortcuts. Plain B/I/L/E/R are not commands — typed
        // text begins an edit instead — so each is guarded on Ctrl.
        Key::B if ctrl => Command::ToggleBold,
        Key::I if ctrl => Command::ToggleItalic,
        Key::L if ctrl && shift => Command::SetAlign(HAlign::Left),
        Key::E if ctrl && shift => Command::SetAlign(HAlign::Center),
        Key::R if ctrl && shift => Command::SetAlign(HAlign::Right),
        Key::C if ctrl => Command::Copy,
        Key::X if ctrl => Command::Cut,
        Key::V if ctrl => Command::Paste,
        Key::Z if ctrl && shift => Command::Redo,
        Key::Z if ctrl => Command::Undo,
        Key::Y if ctrl => Command::Redo,
        Key::D if ctrl => Command::FillDown,
        // Ctrl+Shift+R is alignment (matched above); plain Ctrl+R fills.
        Key::R if ctrl => Command::FillRight,
        Key::F if ctrl => Command::OpenFind,
        Key::F1 => Command::OpenHelp,
        _ => return None,
    })
}

/// Shift turns an arrow-key move into a range extension.
fn move_or_extend(dir: Dir, shift: bool) -> Command {
    if shift {
        Command::Extend(dir)
    } else {
        Command::Move(dir)
    }
}

fn editing(key: Key, shift: bool) -> Option<Command> {
    Some(match key {
        Key::Enter => Command::Commit(if shift { Dir::Up } else { Dir::Down }),
        Key::Tab => Command::Commit(if shift { Dir::Left } else { Dir::Right }),
        Key::Escape => Command::Cancel,
        // Arrow keys (and everything else) belong to the text editor while
        // editing — they move the caret, not the selection.
        _ => return None,
    })
}

/// Interpret typed text. While navigating, the first printable character
/// begins a replace-edit; while editing, the text editor owns its input,
/// so this returns `None`.
pub fn command_for_text(text: &str, mode: Mode) -> Option<Command> {
    if mode != Mode::Navigating {
        return None;
    }
    let ch = text.chars().next()?;
    if ch.is_control() {
        return None;
    }
    Some(Command::BeginEdit {
        replace_with: Some(ch),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nav(key: Key, shift: bool, ctrl: bool) -> Option<Command> {
        command_for_key(key, shift, ctrl, Mode::Navigating)
    }

    fn edit(key: Key, shift: bool) -> Option<Command> {
        command_for_key(key, shift, false, Mode::Editing)
    }

    #[test]
    fn arrows_move_the_selection() {
        assert_eq!(
            nav(Key::ArrowUp, false, false),
            Some(Command::Move(Dir::Up))
        );
        assert_eq!(
            nav(Key::ArrowDown, false, false),
            Some(Command::Move(Dir::Down))
        );
        assert_eq!(
            nav(Key::ArrowLeft, false, false),
            Some(Command::Move(Dir::Left))
        );
        assert_eq!(
            nav(Key::ArrowRight, false, false),
            Some(Command::Move(Dir::Right)),
        );
    }

    #[test]
    fn tab_and_enter_move_with_shift_reversing() {
        assert_eq!(nav(Key::Tab, false, false), Some(Command::Move(Dir::Right)));
        assert_eq!(nav(Key::Tab, true, false), Some(Command::Move(Dir::Left)));
        assert_eq!(
            nav(Key::Enter, false, false),
            Some(Command::Move(Dir::Down))
        );
        assert_eq!(nav(Key::Enter, true, false), Some(Command::Move(Dir::Up)));
    }

    #[test]
    fn home_jumps_to_row_start_or_origin() {
        assert_eq!(nav(Key::Home, false, false), Some(Command::MoveToRowStart));
        assert_eq!(nav(Key::Home, false, true), Some(Command::MoveToOrigin));
    }

    #[test]
    fn f2_begins_an_edit_in_place() {
        assert_eq!(
            nav(Key::F2, false, false),
            Some(Command::BeginEdit { replace_with: None }),
        );
    }

    #[test]
    fn delete_and_backspace_clear_while_navigating() {
        assert_eq!(nav(Key::Delete, false, false), Some(Command::Clear));
        assert_eq!(nav(Key::Backspace, false, false), Some(Command::Clear));
    }

    #[test]
    fn unhandled_navigation_keys_yield_no_command() {
        assert_eq!(nav(Key::A, false, false), None);
        assert_eq!(nav(Key::Escape, false, false), None);
    }

    #[test]
    fn editing_commits_on_enter_and_tab() {
        assert_eq!(edit(Key::Enter, false), Some(Command::Commit(Dir::Down)));
        assert_eq!(edit(Key::Enter, true), Some(Command::Commit(Dir::Up)));
        assert_eq!(edit(Key::Tab, false), Some(Command::Commit(Dir::Right)));
        assert_eq!(edit(Key::Tab, true), Some(Command::Commit(Dir::Left)));
    }

    #[test]
    fn editing_cancels_on_escape() {
        assert_eq!(edit(Key::Escape, false), Some(Command::Cancel));
    }

    #[test]
    fn arrows_belong_to_the_text_editor_while_editing() {
        assert_eq!(edit(Key::ArrowLeft, false), None);
        assert_eq!(edit(Key::ArrowRight, false), None);
        assert_eq!(edit(Key::Home, false), None);
    }

    #[test]
    fn typed_text_begins_a_replace_edit_only_while_navigating() {
        assert_eq!(
            command_for_text("a", Mode::Navigating),
            Some(Command::BeginEdit {
                replace_with: Some('a')
            }),
        );
        assert_eq!(
            command_for_text("7", Mode::Navigating),
            Some(Command::BeginEdit {
                replace_with: Some('7')
            }),
        );
        assert_eq!(command_for_text("a", Mode::Editing), None);
        assert_eq!(command_for_text("", Mode::Navigating), None);
        assert_eq!(command_for_text("\t", Mode::Navigating), None);
    }

    #[test]
    fn ctrl_shortcuts_toggle_bold_and_italic() {
        assert_eq!(nav(Key::B, false, true), Some(Command::ToggleBold));
        assert_eq!(nav(Key::I, false, true), Some(Command::ToggleItalic));
        // Without Ctrl they are ordinary typed characters, not commands.
        assert_eq!(nav(Key::B, false, false), None);
        assert_eq!(nav(Key::I, false, false), None);
    }

    #[test]
    fn ctrl_shift_letters_set_alignment() {
        assert_eq!(
            nav(Key::L, true, true),
            Some(Command::SetAlign(HAlign::Left)),
        );
        assert_eq!(
            nav(Key::E, true, true),
            Some(Command::SetAlign(HAlign::Center)),
        );
        assert_eq!(
            nav(Key::R, true, true),
            Some(Command::SetAlign(HAlign::Right)),
        );
        // Ctrl alone (no Shift) is not an alignment command.
        assert_eq!(nav(Key::L, false, true), None);
    }

    #[test]
    fn shift_arrows_extend_the_selection() {
        assert_eq!(
            nav(Key::ArrowUp, true, false),
            Some(Command::Extend(Dir::Up))
        );
        assert_eq!(
            nav(Key::ArrowRight, true, false),
            Some(Command::Extend(Dir::Right)),
        );
        // Without Shift the same keys move (and collapse) the selection.
        assert_eq!(
            nav(Key::ArrowUp, false, false),
            Some(Command::Move(Dir::Up))
        );
        assert_eq!(
            nav(Key::ArrowRight, false, false),
            Some(Command::Move(Dir::Right)),
        );
    }

    #[test]
    fn ctrl_c_v_x_copy_paste_cut() {
        assert_eq!(nav(Key::C, false, true), Some(Command::Copy));
        assert_eq!(nav(Key::V, false, true), Some(Command::Paste));
        assert_eq!(nav(Key::X, false, true), Some(Command::Cut));
        // Plain C/V/X are ordinary typed characters, not commands.
        assert_eq!(nav(Key::C, false, false), None);
        assert_eq!(nav(Key::X, false, false), None);
    }

    #[test]
    fn ctrl_z_and_y_undo_and_redo() {
        assert_eq!(nav(Key::Z, false, true), Some(Command::Undo));
        assert_eq!(nav(Key::Y, false, true), Some(Command::Redo));
        // Ctrl+Shift+Z is the alternate redo.
        assert_eq!(nav(Key::Z, true, true), Some(Command::Redo));
        // Plain Z/Y are typed characters, not commands.
        assert_eq!(nav(Key::Z, false, false), None);
    }

    #[test]
    fn ctrl_d_and_ctrl_r_fill() {
        assert_eq!(nav(Key::D, false, true), Some(Command::FillDown));
        assert_eq!(nav(Key::R, false, true), Some(Command::FillRight));
        // Ctrl+Shift+R stays alignment, not a fill.
        assert_eq!(
            nav(Key::R, true, true),
            Some(Command::SetAlign(HAlign::Right)),
        );
        // Plain D/R are typed characters, not commands.
        assert_eq!(nav(Key::D, false, false), None);
    }

    #[test]
    fn ctrl_f_opens_find() {
        assert_eq!(nav(Key::F, false, true), Some(Command::OpenFind));
        // Plain F is an ordinary typed character, not a command.
        assert_eq!(nav(Key::F, false, false), None);
    }

    #[test]
    fn ctrl_a_selects_all() {
        assert_eq!(nav(Key::A, false, true), Some(Command::SelectAll));
        // Plain A is an ordinary typed character, not a command.
        assert_eq!(nav(Key::A, false, false), None);
    }

    #[test]
    fn f1_opens_the_help_overlay() {
        assert_eq!(nav(Key::F1, false, false), Some(Command::OpenHelp));
        // The shortcuts list is populated and every entry is well-formed.
        assert!(!SHORTCUTS.is_empty());
        assert!(SHORTCUTS
            .iter()
            .all(|(k, d)| !k.is_empty() && !d.is_empty()));
    }
}
