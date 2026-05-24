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
    /// Jump the cursor to the data edge in a direction — Ctrl+arrow.
    Jump(Dir),
    /// Extend the selection to the data edge — Ctrl+Shift+arrow.
    JumpExtend(Dir),
    /// Select the whole sheet (Ctrl+A).
    SelectAll,
    /// Select the contiguous data region around the cursor (Ctrl+Shift+8).
    SelectRegion,
    /// Move to the first column of the current row (Home).
    MoveToRowStart,
    /// Move to the top-left cell (Ctrl+Home).
    MoveToOrigin,
    /// Move to the last column of the current row (End).
    MoveToRowEnd,
    /// Move to the bottom-right cell (Ctrl+End).
    MoveToSheetEnd,
    /// Move the cursor up a page (Page Up).
    PageUp,
    /// Move the cursor down a page (Page Down).
    PageDown,
    /// Begin editing the selected cell. `replace_with` is `Some(c)` when a
    /// character was typed — the cell's content is replaced, starting with
    /// `c` — or `None` for an F2-style edit of the existing content.
    BeginEdit { replace_with: Option<char> },
    /// Commit the in-progress edit, then move one cell.
    Commit(Dir),
    /// Discard the in-progress edit (Escape).
    Cancel,
    /// Clear an armed cut/copy marquee — Escape while navigating.
    ClearMarquee,
    /// Clear the selected cell (Delete / Backspace while navigating).
    Clear,
    /// Toggle bold on the selected cell (Ctrl+B).
    ToggleBold,
    /// Toggle italic on the selected cell (Ctrl+I).
    ToggleItalic,
    /// Toggle underline on the selected cell (Ctrl+U).
    ToggleUnderline,
    /// Set the selected cell's horizontal alignment (Ctrl+Shift+L/E/R).
    SetAlign(HAlign),
    /// Copy the selected range to the clipboard (Ctrl+C).
    Copy,
    /// Cut the selected range — copies it, and the next paste clears it
    /// (Ctrl+X).
    Cut,
    /// Paste the clipboard at the active cell (Ctrl+V).
    Paste,
    /// Paste only the clipboard's values, dropping formulas
    /// (Ctrl+Shift+V).
    PasteValues,
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
    /// Jump to the next Find match (F3) — no-op when there is no query.
    FindNext,
    /// Jump to the previous Find match (Shift+F3).
    FindPrev,
    /// Open the keyboard-shortcuts help overlay (F1).
    OpenHelp,
    /// Toggle Stage Mode (Ctrl+Shift+P) — hide the editing chrome and
    /// lock cells against editing, leaving widgets interactive so the
    /// sheet reads as an app rather than a spreadsheet.
    ToggleStageMode,
    /// Exit Stage Mode (Escape while in Stage Mode).
    ExitStageMode,
    /// Save the workbook to a `.crbd` (Ctrl+S). Re-uses the last save
    /// path on native; on wasm always prompts for a download location.
    Save,
    /// Force a save dialog even when a path is known (Ctrl+Shift+S).
    SaveAs,
    /// Open a `.crbd` (Ctrl+O). Replaces the current workbook + UI state.
    Open,
}

/// The keyboard shortcuts, as `(keys, description)` — the data behind
/// the F1 help overlay.
pub const SHORTCUTS: &[(&str, &str)] = &[
    ("Arrow keys", "Move the selection"),
    ("Shift + arrows", "Extend the selection"),
    ("Ctrl + arrows", "Jump to the data edge"),
    ("Ctrl+Shift + arrows", "Extend to the data edge"),
    ("Ctrl+A", "Select the whole sheet"),
    ("Ctrl+Shift+8", "Select the data region"),
    ("Tab / Shift+Tab", "Move right / left"),
    ("Enter / Shift+Enter", "Move down / up"),
    ("Home", "Jump to the row start"),
    ("Ctrl+Home", "Jump to the top-left cell"),
    ("End", "Jump to the row end"),
    ("Ctrl+End", "Jump to the bottom-right cell"),
    ("Page Up / Down", "Move a page up or down"),
    ("F2", "Edit the selected cell"),
    ("Delete / Backspace", "Clear the selection"),
    ("Ctrl+B / Ctrl+I", "Bold / italic"),
    ("Ctrl+U", "Underline"),
    ("Ctrl+Shift+L / E / R", "Align left / centre / right"),
    ("Ctrl+C / X / V", "Copy / cut / paste"),
    ("Ctrl+Shift+V", "Paste values only"),
    ("Esc", "Clear the cut marquee"),
    ("Ctrl+Z / Ctrl+Y", "Undo / redo"),
    ("Ctrl+D / Ctrl+R", "Fill down / right"),
    ("Ctrl+F", "Find & replace"),
    ("F3 / Shift+F3", "Find next / previous match"),
    ("F1", "This shortcuts list"),
    ("Ctrl+Shift+P", "Toggle Stage Mode (present)"),
    ("Ctrl+S", "Save the workbook"),
    ("Ctrl+Shift+S", "Save the workbook as…"),
    ("Ctrl+O", "Open a workbook"),
];

/// The mouse gestures, as `(gesture, description)` — the data behind the
/// F1 help overlay's "Mouse" section, so feature discovery does not
/// depend on the user guessing.
pub const GESTURES: &[(&str, &str)] = &[
    ("Click", "Select a cell"),
    ("Drag", "Select a range"),
    ("Shift+click", "Extend the selection"),
    ("Double-click", "Edit the cell"),
    ("Click a header", "Select the column or row"),
    ("Drag headers", "Select a column or row range"),
    ("Click the corner", "Select the whole sheet"),
    ("Double-click a border", "Autofit the column or row"),
    ("Right-click", "Cell actions menu"),
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
        // Modifiers refine an arrow: Ctrl jumps, Shift extends, plain moves.
        Key::ArrowUp => arrow(Dir::Up, shift, ctrl),
        Key::ArrowDown => arrow(Dir::Down, shift, ctrl),
        Key::ArrowLeft => arrow(Dir::Left, shift, ctrl),
        Key::ArrowRight => arrow(Dir::Right, shift, ctrl),
        // Tab/Enter move the selection; Shift reverses the axis direction.
        Key::Tab => Command::Move(if shift { Dir::Left } else { Dir::Right }),
        Key::Enter => Command::Move(if shift { Dir::Up } else { Dir::Down }),
        Key::Home if ctrl => Command::MoveToOrigin,
        Key::Home => Command::MoveToRowStart,
        Key::End if ctrl => Command::MoveToSheetEnd,
        Key::End => Command::MoveToRowEnd,
        Key::PageUp => Command::PageUp,
        Key::PageDown => Command::PageDown,
        Key::F2 => Command::BeginEdit { replace_with: None },
        Key::Delete | Key::Backspace => Command::Clear,
        Key::A if ctrl => Command::SelectAll,
        Key::Num8 if ctrl && shift => Command::SelectRegion,
        // Formatting shortcuts. Plain B/I/L/E/R are not commands — typed
        // text begins an edit instead — so each is guarded on Ctrl.
        Key::B if ctrl => Command::ToggleBold,
        Key::I if ctrl => Command::ToggleItalic,
        Key::U if ctrl => Command::ToggleUnderline,
        Key::L if ctrl && shift => Command::SetAlign(HAlign::Left),
        Key::E if ctrl && shift => Command::SetAlign(HAlign::Center),
        Key::R if ctrl && shift => Command::SetAlign(HAlign::Right),
        Key::C if ctrl => Command::Copy,
        Key::X if ctrl => Command::Cut,
        Key::V if ctrl && shift => Command::PasteValues,
        Key::V if ctrl => Command::Paste,
        Key::Z if ctrl && shift => Command::Redo,
        Key::Z if ctrl => Command::Undo,
        Key::Y if ctrl => Command::Redo,
        Key::D if ctrl => Command::FillDown,
        // Ctrl+Shift+R is alignment (matched above); plain Ctrl+R fills.
        Key::R if ctrl => Command::FillRight,
        Key::F if ctrl => Command::OpenFind,
        Key::F3 if shift => Command::FindPrev,
        Key::F3 => Command::FindNext,
        Key::F1 => Command::OpenHelp,
        // Ctrl+Shift+P — Stage Mode toggle. P for Present/Play; mirrors
        // PowerPoint muscle memory.
        Key::P if ctrl && shift => Command::ToggleStageMode,
        // File operations. Ctrl+Shift+S must precede plain Ctrl+S.
        Key::S if ctrl && shift => Command::SaveAs,
        Key::S if ctrl => Command::Save,
        Key::O if ctrl => Command::Open,
        Key::Escape => Command::ClearMarquee,
        _ => return None,
    })
}

/// An arrow key, refined by its modifiers: Ctrl+Shift extends to the
/// data edge, Ctrl jumps to it, Shift extends one cell, plain moves.
fn arrow(dir: Dir, shift: bool, ctrl: bool) -> Command {
    match (ctrl, shift) {
        (true, true) => Command::JumpExtend(dir),
        (true, false) => Command::Jump(dir),
        (false, true) => Command::Extend(dir),
        (false, false) => Command::Move(dir),
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
    fn save_open_keymap_bindings() {
        assert_eq!(nav(Key::S, false, true), Some(Command::Save));
        assert_eq!(nav(Key::S, true, true), Some(Command::SaveAs));
        assert_eq!(nav(Key::O, false, true), Some(Command::Open));
        // Plain letters without Ctrl don't fire file commands.
        assert_eq!(nav(Key::S, false, false), None);
        assert_eq!(nav(Key::O, false, false), None);
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
    fn end_jumps_to_row_end_or_sheet_end() {
        assert_eq!(nav(Key::End, false, false), Some(Command::MoveToRowEnd));
        assert_eq!(nav(Key::End, false, true), Some(Command::MoveToSheetEnd));
    }

    #[test]
    fn page_keys_move_a_page() {
        assert_eq!(nav(Key::PageUp, false, false), Some(Command::PageUp));
        assert_eq!(nav(Key::PageDown, false, false), Some(Command::PageDown));
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
    }

    #[test]
    fn escape_clears_the_marquee_while_navigating() {
        assert_eq!(nav(Key::Escape, false, false), Some(Command::ClearMarquee));
        // While editing, Escape still cancels the in-progress edit.
        assert_eq!(edit(Key::Escape, false), Some(Command::Cancel));
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
    fn ctrl_u_toggles_underline() {
        assert_eq!(nav(Key::U, false, true), Some(Command::ToggleUnderline));
        // Plain U is an ordinary typed character, not a command.
        assert_eq!(nav(Key::U, false, false), None);
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
    fn ctrl_arrows_jump_to_the_data_edge() {
        assert_eq!(
            nav(Key::ArrowRight, false, true),
            Some(Command::Jump(Dir::Right)),
        );
        assert_eq!(nav(Key::ArrowUp, false, true), Some(Command::Jump(Dir::Up)));
    }

    #[test]
    fn ctrl_shift_arrows_extend_to_the_data_edge() {
        assert_eq!(
            nav(Key::ArrowDown, true, true),
            Some(Command::JumpExtend(Dir::Down)),
        );
        assert_eq!(
            nav(Key::ArrowLeft, true, true),
            Some(Command::JumpExtend(Dir::Left)),
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
    fn ctrl_shift_v_pastes_values_only() {
        assert_eq!(nav(Key::V, true, true), Some(Command::PasteValues));
        // Plain Ctrl+V (no Shift) stays an ordinary paste.
        assert_eq!(nav(Key::V, false, true), Some(Command::Paste));
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
    fn f3_steps_to_the_next_or_previous_find_match() {
        assert_eq!(nav(Key::F3, false, false), Some(Command::FindNext));
        assert_eq!(nav(Key::F3, true, false), Some(Command::FindPrev));
    }

    #[test]
    fn ctrl_a_selects_all() {
        assert_eq!(nav(Key::A, false, true), Some(Command::SelectAll));
        // Plain A is an ordinary typed character, not a command.
        assert_eq!(nav(Key::A, false, false), None);
    }

    #[test]
    fn ctrl_shift_8_selects_the_region() {
        assert_eq!(nav(Key::Num8, true, true), Some(Command::SelectRegion));
        // Ctrl alone (no Shift) is not the region command.
        assert_eq!(nav(Key::Num8, false, true), None);
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

    #[test]
    fn gestures_list_is_well_formed() {
        assert!(!GESTURES.is_empty());
        assert!(GESTURES.iter().all(|(g, d)| !g.is_empty() && !d.is_empty()));
    }

    #[test]
    fn ctrl_shift_p_toggles_stage_mode() {
        assert_eq!(nav(Key::P, true, true), Some(Command::ToggleStageMode));
        // Plain Ctrl+P is unmapped — Stage Mode wants both modifiers.
        assert_eq!(nav(Key::P, false, true), None);
        // Plain P is a typed character.
        assert_eq!(nav(Key::P, false, false), None);
    }
}
