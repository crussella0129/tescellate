//! The formatting toolbar — a single Google-Sheets-style strip that
//! surfaces the [`crate::format`] model with buttons, a combo, and colour
//! pickers. (Excel's tabbed ribbon is the eventual shape; one strip is
//! right for the current feature set.)
//!
//! [`ribbon`] is egui rendering, verified by the build and a physical
//! run. The pure helpers — [`number_format_label`] and [`NUMBER_FORMATS`]
//! — are ordinary `cargo test` material. The strip never mutates state
//! directly: it returns a [`RibbonAction`] for `app.rs` to apply, so the
//! interaction's *result* stays a plain, inspectable value.

use egui::Color32;

use crate::format::{BorderMode, CellFormat, HAlign, NumberFormat, VAlign};

/// A formatting change the user triggered on the ribbon.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RibbonAction {
    ToggleBold,
    ToggleItalic,
    ToggleStrikethrough,
    ToggleUnderline,
    SetAlign(HAlign),
    SetVAlign(VAlign),
    SetNumber(NumberFormat),
    /// Increase (`+1`) or decrease (`-1`) the selected cells' decimal places.
    AdjustDecimals(i32),
    SetTextColor(Option<Color32>),
    SetFill(Option<Color32>),
    /// Reset the selected cell to the default (unstyled) format.
    ClearFormat,
    /// Undo the most recent action.
    Undo,
    /// Redo the most recently undone action.
    Redo,
    /// Copy the selected range.
    Copy,
    /// Cut the selected range.
    Cut,
    /// Paste the clipboard at the active cell.
    Paste,
    /// Paste only the clipboard's values, dropping formulas.
    PasteValues,
    /// Open the conditional-formatting rule editor.
    OpenConditional,
    /// Turn the selection into (or out of) boolean checkbox cells.
    ToggleWidget,
    /// Aggregate the selection (`SUM`, `AVERAGE`, …) into the cell below it.
    Aggregate(&'static str),
    /// Apply a border mode across the selection.
    SetBorders(BorderMode),
    /// Open the keyboard-shortcuts help overlay.
    OpenHelp,
    /// Switch between the light and dark colour themes.
    ToggleTheme,
}

/// The number formats the ribbon's combo offers, with display labels. The
/// `decimals` here are the combo's starting point — General first, as
/// spreadsheets list it.
pub const NUMBER_FORMATS: &[(NumberFormat, &str)] = &[
    (NumberFormat::General, "General"),
    (NumberFormat::Number { decimals: 2 }, "Number"),
    (NumberFormat::Thousands { decimals: 2 }, "Thousands"),
    (NumberFormat::Percent { decimals: 0 }, "Percent"),
    (NumberFormat::Currency, "Currency"),
    (NumberFormat::Scientific { decimals: 2 }, "Scientific"),
    (NumberFormat::Date, "Date"),
    (NumberFormat::Time, "Time"),
    (NumberFormat::DateTime, "Date & time"),
];

/// A short label for a number format — used for the combo's selected
/// text. Decimal-place differences collapse to the same label.
pub fn number_format_label(format: NumberFormat) -> &'static str {
    match format {
        NumberFormat::General => "General",
        NumberFormat::Number { .. } => "Number",
        NumberFormat::Percent { .. } => "Percent",
        NumberFormat::Currency => "Currency",
        NumberFormat::Thousands { .. } => "Thousands",
        NumberFormat::Scientific { .. } => "Scientific",
        NumberFormat::Date => "Date",
        NumberFormat::Time => "Time",
        NumberFormat::DateTime => "Date & time",
    }
}

/// Draw the toolbar for the selected cell's `current` format. Returns
/// the action the user triggered this frame, if any. `can_undo` /
/// `can_redo` gate the undo/redo buttons.
pub fn ribbon(
    ui: &mut egui::Ui,
    current: &CellFormat,
    can_undo: bool,
    can_redo: bool,
) -> Option<RibbonAction> {
    let mut action = None;
    // Wrapped, so the strip flows onto a second line on a narrow window
    // rather than running its last controls off the edge.
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new("Tescellate").strong());
        ui.separator();

        // Undo / redo — disabled when there is nothing to do.
        if ui
            .add_enabled(can_undo, egui::Button::new("Undo"))
            .on_hover_text("Undo (Ctrl+Z)")
            .clicked()
        {
            action = Some(RibbonAction::Undo);
        }
        if ui
            .add_enabled(can_redo, egui::Button::new("Redo"))
            .on_hover_text("Redo (Ctrl+Y)")
            .clicked()
        {
            action = Some(RibbonAction::Redo);
        }
        ui.separator();

        // Clipboard.
        if ui.button("Copy").on_hover_text("Copy (Ctrl+C)").clicked() {
            action = Some(RibbonAction::Copy);
        }
        if ui.button("Cut").on_hover_text("Cut (Ctrl+X)").clicked() {
            action = Some(RibbonAction::Cut);
        }
        if ui.button("Paste").on_hover_text("Paste (Ctrl+V)").clicked() {
            action = Some(RibbonAction::Paste);
        }
        if ui
            .button("Paste values")
            .on_hover_text("Paste values only — drops formulas (Ctrl+Shift+V)")
            .clicked()
        {
            action = Some(RibbonAction::PasteValues);
        }
        ui.separator();

        // Bold / italic — `selectable_label` shows the button pressed-in
        // when the selected cell already has that style.
        if ui
            .selectable_label(current.bold, egui::RichText::new("B").strong())
            .on_hover_text("Bold (Ctrl+B)")
            .clicked()
        {
            action = Some(RibbonAction::ToggleBold);
        }
        if ui
            .selectable_label(current.italic, egui::RichText::new("I").italics())
            .on_hover_text("Italic (Ctrl+I)")
            .clicked()
        {
            action = Some(RibbonAction::ToggleItalic);
        }
        if ui
            .selectable_label(
                current.strikethrough,
                egui::RichText::new("S").strikethrough(),
            )
            .on_hover_text("Strikethrough")
            .clicked()
        {
            action = Some(RibbonAction::ToggleStrikethrough);
        }
        if ui
            .selectable_label(current.underline, egui::RichText::new("U").underline())
            .on_hover_text("Underline (Ctrl+U)")
            .clicked()
        {
            action = Some(RibbonAction::ToggleUnderline);
        }
        ui.separator();

        // Alignment — a three-way segmented control.
        for (align, label) in [
            (HAlign::Left, "Left"),
            (HAlign::Center, "Center"),
            (HAlign::Right, "Right"),
        ] {
            if ui.selectable_label(current.align == align, label).clicked() {
                action = Some(RibbonAction::SetAlign(align));
            }
        }
        for (valign, label) in [
            (VAlign::Top, "Top"),
            (VAlign::Middle, "Middle"),
            (VAlign::Bottom, "Bottom"),
        ] {
            if ui
                .selectable_label(current.valign == valign, label)
                .clicked()
            {
                action = Some(RibbonAction::SetVAlign(valign));
            }
        }
        ui.separator();

        // Number format.
        egui::ComboBox::from_label("Number")
            .selected_text(number_format_label(current.number))
            .show_ui(ui, |ui| {
                let current_label = number_format_label(current.number);
                for &(format, label) in NUMBER_FORMATS {
                    if ui.selectable_label(current_label == label, label).clicked() {
                        action = Some(RibbonAction::SetNumber(format));
                    }
                }
            });
        // Decimal-place steppers, the way a spreadsheet toolbar pairs them.
        if ui
            .button("+.0")
            .on_hover_text("Increase decimal places")
            .clicked()
        {
            action = Some(RibbonAction::AdjustDecimals(1));
        }
        if ui
            .button("-.0")
            .on_hover_text("Decrease decimal places")
            .clicked()
        {
            action = Some(RibbonAction::AdjustDecimals(-1));
        }
        ui.separator();

        // Colours. The picker yields a concrete colour; the action wraps
        // it in `Some`. `Clear` (below) is how a colour is removed.
        ui.label("Text");
        let mut text_color = current.text_color.unwrap_or(Color32::BLACK);
        if ui.color_edit_button_srgba(&mut text_color).changed() {
            action = Some(RibbonAction::SetTextColor(Some(text_color)));
        }
        ui.label("Fill");
        let mut fill = current.fill.unwrap_or(Color32::WHITE);
        if ui.color_edit_button_srgba(&mut fill).changed() {
            action = Some(RibbonAction::SetFill(Some(fill)));
        }
        ui.separator();

        if ui
            .button("Clear")
            .on_hover_text("Reset this cell to the default format")
            .clicked()
        {
            action = Some(RibbonAction::ClearFormat);
        }
        ui.separator();

        if ui
            .button("Conditional…")
            .on_hover_text("Conditional-formatting rules")
            .clicked()
        {
            action = Some(RibbonAction::OpenConditional);
        }
        if ui
            .button("Checkbox")
            .on_hover_text("Turn the selected cells into boolean checkboxes")
            .clicked()
        {
            action = Some(RibbonAction::ToggleWidget);
        }
        ui.menu_button("AutoSum", |ui| {
            for func in ["SUM", "AVERAGE", "COUNT", "MIN", "MAX"] {
                if ui.button(func).clicked() {
                    action = Some(RibbonAction::Aggregate(func));
                    ui.close_menu();
                }
            }
        });
        ui.separator();

        // Borders — one-shot commands across the selection.
        ui.label("Borders");
        if ui
            .button("All")
            .on_hover_text("Border every selected cell")
            .clicked()
        {
            action = Some(RibbonAction::SetBorders(BorderMode::All));
        }
        if ui
            .button("Outer")
            .on_hover_text("Border the selection's outer edge")
            .clicked()
        {
            action = Some(RibbonAction::SetBorders(BorderMode::Outer));
        }
        if ui
            .button("None")
            .on_hover_text("Remove borders from the selection")
            .clicked()
        {
            action = Some(RibbonAction::SetBorders(BorderMode::None));
        }
        ui.separator();

        if ui
            .button("?")
            .on_hover_text("Keyboard shortcuts (F1)")
            .clicked()
        {
            action = Some(RibbonAction::OpenHelp);
        }
        if ui
            .button("Theme")
            .on_hover_text("Toggle the light / dark theme")
            .clicked()
        {
            action = Some(RibbonAction::ToggleTheme);
        }
    });
    action
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn number_format_label_ignores_decimal_places() {
        assert_eq!(number_format_label(NumberFormat::General), "General");
        assert_eq!(
            number_format_label(NumberFormat::Number { decimals: 2 }),
            "Number",
        );
        assert_eq!(
            number_format_label(NumberFormat::Number { decimals: 5 }),
            "Number",
        );
        assert_eq!(
            number_format_label(NumberFormat::Percent { decimals: 0 }),
            "Percent",
        );
        assert_eq!(number_format_label(NumberFormat::Currency), "Currency");
    }

    #[test]
    fn number_formats_list_starts_with_general() {
        assert_eq!(NUMBER_FORMATS.len(), 9);
        assert_eq!(NUMBER_FORMATS[0].0, NumberFormat::General);
    }

    #[test]
    fn every_listed_format_matches_its_label() {
        for &(format, label) in NUMBER_FORMATS {
            assert_eq!(number_format_label(format), label);
        }
    }
}
