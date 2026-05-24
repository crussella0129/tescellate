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

use crate::format::{BorderMode, CellFormat, FontSize, HAlign, NumberFormat, VAlign};

/// A formatting change the user triggered on the ribbon.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RibbonAction {
    ToggleBold,
    ToggleItalic,
    ToggleStrikethrough,
    ToggleUnderline,
    SetAlign(HAlign),
    SetVAlign(VAlign),
    SetFontSize(FontSize),
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
    /// Turn the selection into (or out of) slider cells with the
    /// default 0–100 range.
    ToggleSlider,
    /// Turn the selection into (or out of) clickable button cells —
    /// clicking re-fires the cell's source.
    ToggleButton,
    /// Turn the selection into (or out of) progress-bar cells with
    /// the default max.
    ToggleProgressBar,
    /// Aggregate the selection (`SUM`, `AVERAGE`, …) into the cell below it.
    Aggregate(&'static str),
    /// Apply a border mode across the selection.
    SetBorders(BorderMode),
    /// Toggle the accounting "negative numbers in red" rendering on the
    /// selected cells.
    ToggleNegativeRed,
    /// Toggle whether long cell text wraps onto multiple lines within the
    /// cell instead of clipping at the right edge.
    ToggleWrapText,
    /// Arm the format painter — capture the active cell's format so the
    /// next square-cell click applies it. A second click on the button
    /// (or pressing Escape) disarms it.
    ToggleFormatPainter,
    /// Open the keyboard-shortcuts help overlay.
    OpenHelp,
    /// Switch between the light and dark colour themes.
    ToggleTheme,
    /// Open the Find / Replace dialog.
    OpenFind,
    /// Step to the next Find match.
    FindNext,
    /// Step to the previous Find match.
    FindPrev,
    /// Select every cell on the active sheet.
    SelectAll,
    /// Expand the selection to the contiguous data region around the
    /// cursor.
    SelectRegion,
    /// Open the note editor for the cell under the cursor.
    OpenNote,
    /// Close the application window.
    Quit,
    /// Sort the current column / range ascending.
    SortAscending,
    /// Sort the current column / range descending.
    SortDescending,
    /// Step the egui zoom factor up.
    ZoomIn,
    /// Step the egui zoom factor down.
    ZoomOut,
    /// Reset the egui zoom factor to 1.0.
    ResetZoom,
    /// Open the About / app-info window.
    OpenAbout,
    /// Save the workbook (Ctrl+S).
    Save,
    /// Open a workbook (Ctrl+O).
    Open,
    /// Toggle the Voronoi sheet's frozen-seeds flag (ADR-014). Locks /
    /// unlocks the seed-handle drag + proximity-fade.
    ToggleVoronoiFreeze,
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

/// Draw a single bordered ribbon group titled `title`, with `content`
/// laid out horizontally inside the frame and the title shown as a
/// small caption underneath. Returns whichever [`RibbonAction`] the
/// inner controls produced this frame.
///
/// `content` runs in a NON-wrapping horizontal layout — when the
/// window is narrow, whole groups overflow into the ribbon's "More
/// ⋮" menu (see [`ribbon`]) instead of being squished into single
/// columns.
fn ribbon_group(
    ui: &mut egui::Ui,
    title: &str,
    content: impl FnOnce(&mut egui::Ui) -> Option<RibbonAction>,
) -> Option<RibbonAction> {
    let mut action = None;
    egui::Frame::group(ui.style())
        .inner_margin(egui::Margin::symmetric(6.0, 3.0))
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    action = content(ui);
                });
                ui.add_space(1.0);
                ui.small(egui::RichText::new(title).weak());
            });
        });
    action
}

/// Estimated widths (points) of each ribbon group in the order they
/// render, used to decide which trailing groups collapse into the
/// More menu. v119 trims another ~6% off each estimate — the v117
/// numbers were still leaving roughly a quarter-group of empty space
/// to the right of the last inline group at most window widths.
const GROUP_WIDTHS: &[f32] = &[
    108.0, // File (Save, Open)
    108.0, // History (Undo, Redo)
    244.0, // Clipboard (Copy, Cut, Paste, Paste values, Painter)
    410.0, // Font (B, I, S, U, Wrap, Size combo, Text colour, Fill colour)
    232.0, // Alignment (L, C, R, V Top, Mid, Btm)
    213.0, // Number (combo, +.0, -.0, (-))
    138.0, // Borders (All, Outer, None)
    184.0, // Cells (Clear, Conditional…, Checkbox)
    78.0,  // Data (AutoSum)
    92.0,  // View (?, Theme)
    138.0, // Voronoi (contextual — hidden when active sheet isn't Voronoi)
];

/// Width reserved at the right edge for the "More ⋮" overflow menu.
/// `⋮` is a small symbol — a bare `menu_button` around it is about
/// 24 px wide. 26 leaves a couple of pixels of breathing room without
/// reserving space the button doesn't actually use.
const MORE_WIDTH: f32 = 26.0;

/// Right-edge buffer kept between the last group and the window's
/// right edge / the More button. egui's item_spacing already gives
/// the inline frames a visual gap, so RIGHT_MARGIN can be zero — the
/// buffer was just absorbing slop from the old estimates.
const RIGHT_MARGIN: f32 = 0.0;

/// Inner buttons of the File group — save / open. Sits leftmost on the
/// ribbon to match the document-toolbar convention users carry over
/// from every other spreadsheet / word processor they've used.
fn group_file(ui: &mut egui::Ui) -> Option<RibbonAction> {
    let mut action = None;
    if ui.button("Save").on_hover_text("Save (Ctrl+S)").clicked() {
        action = Some(RibbonAction::Save);
    }
    if ui.button("Open").on_hover_text("Open (Ctrl+O)").clicked() {
        action = Some(RibbonAction::Open);
    }
    action
}

/// Inner buttons of the History group — undo/redo.
fn group_history(ui: &mut egui::Ui, can_undo: bool, can_redo: bool) -> Option<RibbonAction> {
    let mut action = None;
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
    action
}

/// Inner buttons of the Clipboard group — copy / cut / paste / paste
/// values / format painter.
fn group_clipboard(ui: &mut egui::Ui, painter_armed: bool) -> Option<RibbonAction> {
    let mut action = None;
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
    if ui
        .selectable_label(painter_armed, "Painter")
        .on_hover_text("Format painter — capture this cell's format, then click another")
        .clicked()
    {
        action = Some(RibbonAction::ToggleFormatPainter);
    }
    action
}

/// Inner controls of the Font group — bold/italic/strike/underline,
/// wrap, size, text/fill colours.
fn group_font(ui: &mut egui::Ui, current: &CellFormat) -> Option<RibbonAction> {
    let mut action = None;
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
    if ui
        .selectable_label(current.wrap_text, "Wrap")
        .on_hover_text("Wrap long text onto multiple lines within the cell")
        .clicked()
    {
        action = Some(RibbonAction::ToggleWrapText);
    }
    egui::ComboBox::from_id_salt("ribbon_font_size")
        .selected_text(match current.font_size {
            FontSize::Small => "Small",
            FontSize::Normal => "Normal",
            FontSize::Large => "Large",
        })
        .show_ui(ui, |ui| {
            for (size, label) in [
                (FontSize::Small, "Small"),
                (FontSize::Normal, "Normal"),
                (FontSize::Large, "Large"),
            ] {
                if ui
                    .selectable_label(current.font_size == size, label)
                    .clicked()
                {
                    action = Some(RibbonAction::SetFontSize(size));
                }
            }
        });
    let mut text_color = current.text_color.unwrap_or(Color32::BLACK);
    if ui
        .color_edit_button_srgba(&mut text_color)
        .on_hover_text("Text colour")
        .changed()
    {
        action = Some(RibbonAction::SetTextColor(Some(text_color)));
    }
    let mut fill = current.fill.unwrap_or(Color32::WHITE);
    if ui
        .color_edit_button_srgba(&mut fill)
        .on_hover_text("Fill colour")
        .changed()
    {
        action = Some(RibbonAction::SetFill(Some(fill)));
    }
    action
}

/// Inner controls of the Alignment group — H Left/Center/Right and V
/// Top/Mid/Btm.
fn group_alignment(ui: &mut egui::Ui, current: &CellFormat) -> Option<RibbonAction> {
    let mut action = None;
    for (align, label) in [
        (HAlign::Left, "Left"),
        (HAlign::Center, "Center"),
        (HAlign::Right, "Right"),
    ] {
        if ui.selectable_label(current.align == align, label).clicked() {
            action = Some(RibbonAction::SetAlign(align));
        }
    }
    ui.separator();
    for (valign, label) in [
        (VAlign::Top, "Top"),
        (VAlign::Middle, "Mid"),
        (VAlign::Bottom, "Btm"),
    ] {
        if ui
            .selectable_label(current.valign == valign, label)
            .clicked()
        {
            action = Some(RibbonAction::SetVAlign(valign));
        }
    }
    action
}

/// Inner controls of the Number group — format combo, decimal
/// steppers, negative-red toggle.
fn group_number(ui: &mut egui::Ui, current: &CellFormat) -> Option<RibbonAction> {
    let mut action = None;
    egui::ComboBox::from_id_salt("ribbon_number_format")
        .selected_text(number_format_label(current.number))
        .show_ui(ui, |ui| {
            let current_label = number_format_label(current.number);
            for &(format, label) in NUMBER_FORMATS {
                if ui.selectable_label(current_label == label, label).clicked() {
                    action = Some(RibbonAction::SetNumber(format));
                }
            }
        });
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
    if ui
        .selectable_label(
            current.negative_red,
            egui::RichText::new("(-)").color(Color32::from_rgb(220, 50, 50)),
        )
        .on_hover_text("Show negative numbers in red")
        .clicked()
    {
        action = Some(RibbonAction::ToggleNegativeRed);
    }
    action
}

/// Inner controls of the Borders group — all / outer / none.
fn group_borders(ui: &mut egui::Ui) -> Option<RibbonAction> {
    let mut action = None;
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
    action
}

/// Inner controls of the Cells group — clear format, conditional
/// formatting editor, checkbox-widget toggle.
fn group_cells(ui: &mut egui::Ui) -> Option<RibbonAction> {
    let mut action = None;
    if ui
        .button("Clear")
        .on_hover_text("Reset this cell to the default format")
        .clicked()
    {
        action = Some(RibbonAction::ClearFormat);
    }
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
    if ui
        .button("Slider")
        .on_hover_text("Turn the selected cells into 0–100 sliders")
        .clicked()
    {
        action = Some(RibbonAction::ToggleSlider);
    }
    if ui
        .button("Button")
        .on_hover_text("Turn the selected cells into clickable buttons (re-fires the formula)")
        .clicked()
    {
        action = Some(RibbonAction::ToggleButton);
    }
    if ui
        .button("Progress")
        .on_hover_text("Turn the selected cells into read-only 0–100 progress bars")
        .clicked()
    {
        action = Some(RibbonAction::ToggleProgressBar);
    }
    action
}

/// Inner controls of the Data group — AutoSum submenu.
fn group_data(ui: &mut egui::Ui) -> Option<RibbonAction> {
    let mut action = None;
    ui.menu_button("AutoSum", |ui| {
        for func in ["SUM", "AVERAGE", "COUNT", "MIN", "MAX"] {
            if ui.button(func).clicked() {
                action = Some(RibbonAction::Aggregate(func));
                ui.close_menu();
            }
        }
    });
    action
}

/// Inner controls of the View group — keyboard-shortcuts overlay and
/// theme toggle.
fn group_view(ui: &mut egui::Ui) -> Option<RibbonAction> {
    let mut action = None;
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
    action
}

/// Contextual Voronoi group — only rendered when the active sheet is the
/// Voronoi tab. Holds the "Freeze Seeds" toggle (ADR-014). The button's
/// visual state reflects the current freeze flag so the user can see at a
/// glance whether the tessellation is locked.
fn group_voronoi(ui: &mut egui::Ui, frozen: bool) -> Option<RibbonAction> {
    let mut action = None;
    let label = if frozen {
        "Unfreeze Seeds"
    } else {
        "Freeze Seeds"
    };
    let mut btn = egui::Button::new(label);
    if frozen {
        // Highlight when active so the user can see the lock.
        btn = btn.fill(egui::Color32::from_rgb(70, 120, 220));
    }
    if ui
        .add(btn)
        .on_hover_text("Lock the Voronoi tessellation (hides seed handles, suppresses drag)")
        .clicked()
    {
        action = Some(RibbonAction::ToggleVoronoiFreeze);
    }
    action
}

/// How many of the leading groups fit on a single ribbon row of width
/// `available`. Trailing groups beyond the count overflow into the
/// "More ⋮" menu. Always returns at least 1 — if even the first group
/// is too wide, draw it anyway rather than show an empty ribbon.
pub fn fit_count(group_widths: &[f32], available: f32) -> usize {
    let total: f32 = group_widths.iter().sum();
    if total + RIGHT_MARGIN <= available {
        // Everything fits — no overflow menu needed.
        return group_widths.len();
    }
    // Reserve space for the More menu and a tight right-edge margin.
    let budget = available - MORE_WIDTH - RIGHT_MARGIN;
    let mut acc = 0.0;
    for (i, w) in group_widths.iter().enumerate() {
        if acc + w > budget {
            return i.max(1);
        }
        acc += w;
    }
    group_widths.len()
}

/// Draw the toolbar for the selected cell's `current` format. Returns
/// the action the user triggered this frame, if any. `can_undo` /
/// `can_redo` gate the undo/redo buttons. `painter_armed` lights the
/// format-painter toggle when it is currently armed.
///
/// Groups render as bordered, non-wrapping blocks. When the window is
/// too narrow to fit them all, the rightmost groups collapse into a
/// "More ⋮" menu_button at the right edge — Excel/Sheets style. The
/// ribbon's vertical height stays one row.
pub fn ribbon(
    ui: &mut egui::Ui,
    current: &CellFormat,
    can_undo: bool,
    can_redo: bool,
    painter_armed: bool,
    voronoi_active: bool,
    voronoi_frozen: bool,
) -> Option<RibbonAction> {
    let mut action: Option<RibbonAction> = None;
    let avail = ui.available_width();
    // Hide the Voronoi group's width contribution when it's not visible —
    // otherwise non-Voronoi sheets reserve space for an invisible group.
    let widths: Vec<f32> = GROUP_WIDTHS
        .iter()
        .enumerate()
        .map(|(i, &w)| if i == 10 && !voronoi_active { 0.0 } else { w })
        .collect();
    let visible = fit_count(&widths, avail);

    ui.horizontal(|ui| {
        let mut emit = |a: Option<RibbonAction>| {
            if let Some(a) = a {
                action = Some(a);
            }
        };
        for i in 0..visible {
            match i {
                0 => emit(ribbon_group(ui, "File", group_file)),
                1 => emit(ribbon_group(ui, "History", |ui| {
                    group_history(ui, can_undo, can_redo)
                })),
                2 => emit(ribbon_group(ui, "Clipboard", |ui| {
                    group_clipboard(ui, painter_armed)
                })),
                3 => emit(ribbon_group(ui, "Font", |ui| group_font(ui, current))),
                4 => emit(ribbon_group(ui, "Alignment", |ui| {
                    group_alignment(ui, current)
                })),
                5 => emit(ribbon_group(ui, "Number", |ui| group_number(ui, current))),
                6 => emit(ribbon_group(ui, "Borders", group_borders)),
                7 => emit(ribbon_group(ui, "Cells", group_cells)),
                8 => emit(ribbon_group(ui, "Data", group_data)),
                9 => emit(ribbon_group(ui, "View", group_view)),
                10 if voronoi_active => emit(ribbon_group(ui, "Voronoi", |ui| {
                    group_voronoi(ui, voronoi_frozen)
                })),
                _ => {}
            }
        }
        if visible < GROUP_WIDTHS.len() {
            ui.menu_button("⋮", |ui| {
                ui.set_min_width(220.0);
                for i in visible..GROUP_WIDTHS.len() {
                    let title = match i {
                        0 => "File",
                        1 => "History",
                        2 => "Clipboard",
                        3 => "Font",
                        4 => "Alignment",
                        5 => "Number",
                        6 => "Borders",
                        7 => "Cells",
                        8 => "Data",
                        9 => "View",
                        10 if voronoi_active => "Voronoi",
                        _ => "",
                    };
                    if title.is_empty() {
                        // Hidden group (e.g. Voronoi on a non-Voronoi sheet).
                        continue;
                    }
                    ui.label(egui::RichText::new(title).strong());
                    ui.horizontal_wrapped(|ui| {
                        let a = match i {
                            0 => group_file(ui),
                            1 => group_history(ui, can_undo, can_redo),
                            2 => group_clipboard(ui, painter_armed),
                            3 => group_font(ui, current),
                            4 => group_alignment(ui, current),
                            5 => group_number(ui, current),
                            6 => group_borders(ui),
                            7 => group_cells(ui),
                            8 => group_data(ui),
                            9 => group_view(ui),
                            10 if voronoi_active => group_voronoi(ui, voronoi_frozen),
                            _ => None,
                        };
                        if let Some(a) = a {
                            action = Some(a);
                        }
                    });
                    if i + 1 < GROUP_WIDTHS.len() {
                        ui.separator();
                    }
                }
            });
        }
    });
    action
}

/// Draw the application's menu bar — File / Edit / Insert / Format /
/// Data / View / Help — above the ribbon. Each menu item dispatches an
/// existing [`RibbonAction`] so the menu and ribbon share handlers in
/// `app.rs`. `File` entries that need engine support (Save/Open/Export)
/// are shown disabled so users can see where they'll live.
pub fn menu_bar(ui: &mut egui::Ui, can_undo: bool, can_redo: bool) -> Option<RibbonAction> {
    let mut action = None;
    egui::menu::bar(ui, |ui| {
        ui.label(egui::RichText::new("Tescellate").strong());
        ui.separator();
        ui.menu_button("File", |ui| {
            ui.add_enabled(false, egui::Button::new("New"));
            ui.add_enabled(false, egui::Button::new("Open…"));
            ui.add_enabled(false, egui::Button::new("Save"));
            ui.add_enabled(false, egui::Button::new("Save As…"));
            ui.separator();
            ui.add_enabled(false, egui::Button::new("Export…"));
            ui.label(egui::RichText::new("(Save/Open need engine support)").weak());
            ui.separator();
            if ui.button("Quit").clicked() {
                action = Some(RibbonAction::Quit);
                ui.close_menu();
            }
        });
        ui.menu_button("Edit", |ui| {
            if ui
                .add_enabled(can_undo, egui::Button::new("Undo  Ctrl+Z"))
                .clicked()
            {
                action = Some(RibbonAction::Undo);
                ui.close_menu();
            }
            if ui
                .add_enabled(can_redo, egui::Button::new("Redo  Ctrl+Y"))
                .clicked()
            {
                action = Some(RibbonAction::Redo);
                ui.close_menu();
            }
            ui.separator();
            if ui.button("Cut  Ctrl+X").clicked() {
                action = Some(RibbonAction::Cut);
                ui.close_menu();
            }
            if ui.button("Copy  Ctrl+C").clicked() {
                action = Some(RibbonAction::Copy);
                ui.close_menu();
            }
            if ui.button("Paste  Ctrl+V").clicked() {
                action = Some(RibbonAction::Paste);
                ui.close_menu();
            }
            if ui.button("Paste values  Ctrl+Shift+V").clicked() {
                action = Some(RibbonAction::PasteValues);
                ui.close_menu();
            }
            ui.separator();
            if ui.button("Find / Replace…  Ctrl+F").clicked() {
                action = Some(RibbonAction::OpenFind);
                ui.close_menu();
            }
            if ui.button("Find next  F3").clicked() {
                action = Some(RibbonAction::FindNext);
                ui.close_menu();
            }
            if ui.button("Find previous  Shift+F3").clicked() {
                action = Some(RibbonAction::FindPrev);
                ui.close_menu();
            }
            ui.separator();
            if ui.button("Select all  Ctrl+A").clicked() {
                action = Some(RibbonAction::SelectAll);
                ui.close_menu();
            }
            if ui.button("Select region  Ctrl+Shift+8").clicked() {
                action = Some(RibbonAction::SelectRegion);
                ui.close_menu();
            }
        });
        ui.menu_button("Insert", |ui| {
            ui.menu_button("AutoSum", |ui| {
                for func in ["SUM", "AVERAGE", "COUNT", "MIN", "MAX"] {
                    if ui.button(func).clicked() {
                        action = Some(RibbonAction::Aggregate(func));
                        ui.close_menu();
                    }
                }
            });
            ui.menu_button("Widget", |ui| {
                if ui.button("Checkbox").clicked() {
                    action = Some(RibbonAction::ToggleWidget);
                    ui.close_menu();
                }
                ui.add_enabled(false, egui::Button::new("Button (coming soon)"));
                ui.add_enabled(false, egui::Button::new("Switch (coming soon)"));
                ui.add_enabled(false, egui::Button::new("Slider (coming soon)"));
            });
            ui.separator();
            if ui.button("Note for active cell…").clicked() {
                action = Some(RibbonAction::OpenNote);
                ui.close_menu();
            }
            ui.separator();
            ui.add_enabled(false, egui::Button::new("Function… (coming soon)"));
            ui.add_enabled(false, egui::Button::new("Hyperlink… (coming soon)"));
            ui.add_enabled(false, egui::Button::new("Image… (coming soon)"));
            ui.add_enabled(false, egui::Button::new("Chart… (coming soon)"));
            ui.add_enabled(false, egui::Button::new("Row above"));
            ui.add_enabled(false, egui::Button::new("Row below"));
            ui.add_enabled(false, egui::Button::new("Column left"));
            ui.add_enabled(false, egui::Button::new("Column right"));
            ui.label(egui::RichText::new("(Row/column insert needs engine support)").weak());
        });
        ui.menu_button("Format", |ui| {
            if ui.button("Bold  Ctrl+B").clicked() {
                action = Some(RibbonAction::ToggleBold);
                ui.close_menu();
            }
            if ui.button("Italic  Ctrl+I").clicked() {
                action = Some(RibbonAction::ToggleItalic);
                ui.close_menu();
            }
            if ui.button("Underline  Ctrl+U").clicked() {
                action = Some(RibbonAction::ToggleUnderline);
                ui.close_menu();
            }
            if ui.button("Strikethrough").clicked() {
                action = Some(RibbonAction::ToggleStrikethrough);
                ui.close_menu();
            }
            ui.separator();
            ui.menu_button("Align", |ui| {
                if ui.button("Left").clicked() {
                    action = Some(RibbonAction::SetAlign(HAlign::Left));
                    ui.close_menu();
                }
                if ui.button("Center").clicked() {
                    action = Some(RibbonAction::SetAlign(HAlign::Center));
                    ui.close_menu();
                }
                if ui.button("Right").clicked() {
                    action = Some(RibbonAction::SetAlign(HAlign::Right));
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("Top").clicked() {
                    action = Some(RibbonAction::SetVAlign(VAlign::Top));
                    ui.close_menu();
                }
                if ui.button("Middle").clicked() {
                    action = Some(RibbonAction::SetVAlign(VAlign::Middle));
                    ui.close_menu();
                }
                if ui.button("Bottom").clicked() {
                    action = Some(RibbonAction::SetVAlign(VAlign::Bottom));
                    ui.close_menu();
                }
            });
            ui.menu_button("Borders", |ui| {
                if ui.button("All").clicked() {
                    action = Some(RibbonAction::SetBorders(BorderMode::All));
                    ui.close_menu();
                }
                if ui.button("Outer").clicked() {
                    action = Some(RibbonAction::SetBorders(BorderMode::Outer));
                    ui.close_menu();
                }
                if ui.button("None").clicked() {
                    action = Some(RibbonAction::SetBorders(BorderMode::None));
                    ui.close_menu();
                }
            });
            ui.menu_button("Number format", |ui| {
                for &(format, label) in NUMBER_FORMATS {
                    if ui.button(label).clicked() {
                        action = Some(RibbonAction::SetNumber(format));
                        ui.close_menu();
                    }
                }
            });
            ui.separator();
            if ui.button("Wrap text").clicked() {
                action = Some(RibbonAction::ToggleWrapText);
                ui.close_menu();
            }
            if ui.button("Negative red").clicked() {
                action = Some(RibbonAction::ToggleNegativeRed);
                ui.close_menu();
            }
            if ui.button("Format painter").clicked() {
                action = Some(RibbonAction::ToggleFormatPainter);
                ui.close_menu();
            }
            ui.separator();
            if ui.button("Conditional formatting…").clicked() {
                action = Some(RibbonAction::OpenConditional);
                ui.close_menu();
            }
            if ui.button("Toggle checkbox").clicked() {
                action = Some(RibbonAction::ToggleWidget);
                ui.close_menu();
            }
            if ui.button("Clear format").clicked() {
                action = Some(RibbonAction::ClearFormat);
                ui.close_menu();
            }
        });
        ui.menu_button("Data", |ui| {
            if ui.button("Sort ascending").clicked() {
                action = Some(RibbonAction::SortAscending);
                ui.close_menu();
            }
            if ui.button("Sort descending").clicked() {
                action = Some(RibbonAction::SortDescending);
                ui.close_menu();
            }
            ui.separator();
            ui.menu_button("AutoSum", |ui| {
                for func in ["SUM", "AVERAGE", "COUNT", "MIN", "MAX"] {
                    if ui.button(func).clicked() {
                        action = Some(RibbonAction::Aggregate(func));
                        ui.close_menu();
                    }
                }
            });
            ui.separator();
            ui.add_enabled(false, egui::Button::new("Filter… (coming soon)"));
            ui.add_enabled(false, egui::Button::new("Remove duplicates… (coming soon)"));
            ui.add_enabled(false, egui::Button::new("Data validation… (coming soon)"));
            ui.add_enabled(false, egui::Button::new("Split text to columns…"));
        });
        ui.menu_button("View", |ui| {
            if ui.button("Toggle theme").clicked() {
                action = Some(RibbonAction::ToggleTheme);
                ui.close_menu();
            }
            ui.separator();
            if ui.button("Zoom in").clicked() {
                action = Some(RibbonAction::ZoomIn);
                ui.close_menu();
            }
            if ui.button("Zoom out").clicked() {
                action = Some(RibbonAction::ZoomOut);
                ui.close_menu();
            }
            if ui.button("Reset zoom").clicked() {
                action = Some(RibbonAction::ResetZoom);
                ui.close_menu();
            }
            ui.separator();
            ui.add_enabled(false, egui::Button::new("Show formula bar (always on)"));
            ui.add_enabled(false, egui::Button::new("Show gridlines (always on)"));
            ui.add_enabled(false, egui::Button::new("Freeze panes (coming soon)"));
            ui.add_enabled(false, egui::Button::new("Full screen (coming soon)"));
        });
        ui.menu_button("Help", |ui| {
            if ui.button("Keyboard shortcuts  F1").clicked() {
                action = Some(RibbonAction::OpenHelp);
                ui.close_menu();
            }
            if ui.button("About Tescellate…").clicked() {
                action = Some(RibbonAction::OpenAbout);
                ui.close_menu();
            }
        });
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

    #[test]
    fn fit_count_returns_all_when_everything_fits() {
        // 200 + 200 + 200 = 600. Available 1000 — all three fit.
        assert_eq!(fit_count(&[200.0, 200.0, 200.0], 1000.0), 3);
    }

    #[test]
    fn fit_count_overflows_trailing_groups_when_window_is_narrow() {
        // 200+200+200=600 > 300 budget. With MORE_WIDTH=26 and
        // RIGHT_MARGIN=0: budget = 300 - 26 - 0 = 274. Only the
        // first group (200) fits — the second would push acc to 400.
        assert_eq!(fit_count(&[200.0, 200.0, 200.0], 300.0), 1);
    }

    #[test]
    fn fit_count_shows_at_least_one_group_even_when_too_narrow() {
        // Even a 50-px window keeps the first group inline so the
        // ribbon never collapses entirely into a single hamburger.
        assert_eq!(fit_count(&[200.0, 200.0], 50.0), 1);
    }

    #[test]
    fn fit_count_keeps_all_groups_at_the_exact_breakpoint() {
        // total=200, RIGHT_MARGIN=0 -> everything fits at avail=200.
        assert_eq!(fit_count(&[100.0, 100.0], 200.0), 2);
        // Just below the breakpoint -> overflow path engages and at
        // avail=199 budget = 199-26-0 = 173; first group (100) fits,
        // second (acc=200) overflows.
        assert_eq!(fit_count(&[100.0, 100.0], 199.0), 1);
    }

    #[test]
    fn fit_count_tightens_overflow_at_common_window_widths() {
        // Regression guard: v117's GROUP_WIDTHS fit 4 inline groups
        // at 1280, 5 at 1366 and 1440. v119's tighter estimates fit
        // one more group at each common width below 4K — closing the
        // ~0.25-block trailing gap the user reported.
        assert!(
            fit_count(GROUP_WIDTHS, 1280.0) >= 5,
            "v119 should fit ≥5 groups at 1280px"
        );
        assert!(
            fit_count(GROUP_WIDTHS, 1366.0) >= 5,
            "v119 should fit ≥5 groups at 1366px"
        );
        assert!(
            fit_count(GROUP_WIDTHS, 1440.0) >= 6,
            "v119 should fit ≥6 groups at 1440px"
        );
    }
}
