//! The Tescellate application — an `eframe::App` that owns a
//! `WorkbookEngine` and draws the spreadsheet grid with egui.
//!
//! v2: Excel/Sheets-grade keyboard navigation and in-cell editing. Key
//! presses are routed through the pure [`crate::keymap`] layer, so the
//! keyboard *model* is unit-tested independently of egui.

use eframe::egui;
use tescellate_core::{CellValue, SheetId};
use tescellate_formula::WorkbookEngine;
use tescellate_tess::LatticeKind;

use crate::grid;
use crate::keymap::{self, Command, Dir, Mode};

const COLS: u32 = 16;
const ROWS: u32 = 32;

/// Key combos handled while navigating. Each is checked with
/// `consume_key`, so egui's own widgets never also see them (Tab would
/// otherwise move widget focus).
const NAV_KEYS: &[(egui::Modifiers, egui::Key)] = &[
    (egui::Modifiers::NONE, egui::Key::ArrowUp),
    (egui::Modifiers::NONE, egui::Key::ArrowDown),
    (egui::Modifiers::NONE, egui::Key::ArrowLeft),
    (egui::Modifiers::NONE, egui::Key::ArrowRight),
    (egui::Modifiers::NONE, egui::Key::Tab),
    (egui::Modifiers::SHIFT, egui::Key::Tab),
    (egui::Modifiers::NONE, egui::Key::Enter),
    (egui::Modifiers::SHIFT, egui::Key::Enter),
    (egui::Modifiers::NONE, egui::Key::Home),
    (egui::Modifiers::CTRL, egui::Key::Home),
    (egui::Modifiers::NONE, egui::Key::F2),
    (egui::Modifiers::NONE, egui::Key::Delete),
    (egui::Modifiers::NONE, egui::Key::Backspace),
];

/// Key combos handled while editing. Arrows and text are deliberately
/// absent — they belong to the `TextEdit` (caret movement, typing).
const EDIT_KEYS: &[(egui::Modifiers, egui::Key)] = &[
    (egui::Modifiers::NONE, egui::Key::Enter),
    (egui::Modifiers::SHIFT, egui::Key::Enter),
    (egui::Modifiers::NONE, egui::Key::Tab),
    (egui::Modifiers::SHIFT, egui::Key::Tab),
    (egui::Modifiers::NONE, egui::Key::Escape),
];

/// An in-progress cell edit.
struct EditState {
    buffer: String,
    /// Set on the first frame so the `TextEdit` can grab focus and put the
    /// caret at the end; cleared once that has happened.
    fresh: bool,
}

/// The Tescellate front-end application.
pub struct TescellateApp {
    engine: WorkbookEngine,
    sheet: SheetId,
    /// Zero-indexed `(column, row)` of the selected cell.
    selected: (u32, u32),
    /// `Some` while a cell is being edited.
    edit: Option<EditState>,
}

impl TescellateApp {
    /// Build the app, seeding a small demo workbook.
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let mut engine = WorkbookEngine::new();
        engine.new_workbook();
        let sheet = engine.add_sheet("Sheet1", LatticeKind::Square);
        for (addr, src) in [
            ("A1", "Monthly budget"),
            ("A2", "Rent"),
            ("B2", "=1200"),
            ("A3", "Food"),
            ("B3", "=450"),
            ("A4", "Transport"),
            ("B4", "=180"),
            ("A5", "Total"),
            ("B5", "=SUM(B2:B4)"),
        ] {
            let _ = engine.set_cell(sheet, addr, Some(src));
        }
        Self {
            engine,
            sheet,
            selected: (0, 0),
            edit: None,
        }
    }

    fn mode(&self) -> Mode {
        if self.edit.is_some() {
            Mode::Editing
        } else {
            Mode::Navigating
        }
    }

    /// The display text for a cell, read from the engine.
    fn cell_text(&self, col: u32, row: u32) -> String {
        let addr = grid::cell_address(col, row);
        let Some(snapshot) = self.engine.get_cell(self.sheet, &addr) else {
            return String::new();
        };
        match snapshot.value {
            CellValue::Empty => String::new(),
            CellValue::Number(n) => format_number(n),
            CellValue::Integer(i) => i.to_string(),
            CellValue::Bool(b) => if b { "TRUE" } else { "FALSE" }.to_string(),
            CellValue::Text(t) => t,
            CellValue::Error(_) => "#ERROR".to_string(),
            CellValue::Array(_) => "{array}".to_string(),
            _ => String::new(),
        }
    }

    /// The raw source of the selected cell.
    fn selected_source(&self) -> String {
        let (col, row) = self.selected;
        self.engine
            .get_cell(self.sheet, &grid::cell_address(col, row))
            .and_then(|s| s.source)
            .unwrap_or_default()
    }

    /// Read key events and turn them into commands through `keymap`. Keys
    /// that map to a command are consumed so no other widget reacts too.
    fn collect_commands(&self, ctx: &egui::Context) -> Vec<Command> {
        let mode = self.mode();
        let keys = match mode {
            Mode::Editing => EDIT_KEYS,
            Mode::Navigating => NAV_KEYS,
        };
        let mut commands = Vec::new();
        ctx.input_mut(|input| {
            for &(modifiers, key) in keys {
                if input.consume_key(modifiers, key) {
                    if let Some(cmd) =
                        keymap::command_for_key(key, modifiers.shift, modifiers.ctrl, mode)
                    {
                        commands.push(cmd);
                    }
                }
            }
            // While navigating, the first typed character begins an edit;
            // remove that text event so the new `TextEdit` doesn't re-type
            // it.
            if mode == Mode::Navigating {
                let mut typed: Option<String> = None;
                input.events.retain(|event| match event {
                    egui::Event::Text(text) if typed.is_none() => {
                        typed = Some(text.clone());
                        false
                    }
                    _ => true,
                });
                if let Some(text) = typed {
                    if let Some(cmd) = keymap::command_for_text(&text, mode) {
                        commands.push(cmd);
                    }
                }
            }
        });
        commands
    }

    fn apply(&mut self, command: Command) {
        match command {
            Command::Move(dir) => self.move_selection(dir),
            Command::MoveToRowStart => self.selected.0 = 0,
            Command::MoveToOrigin => self.selected = (0, 0),
            Command::BeginEdit { replace_with } => self.begin_edit(replace_with),
            Command::Commit(dir) => {
                self.commit_edit();
                self.move_selection(dir);
            }
            Command::Cancel => self.edit = None,
            Command::Clear => self.clear_selected(),
        }
    }

    fn move_selection(&mut self, dir: Dir) {
        let (col, row) = self.selected;
        self.selected = match dir {
            Dir::Up => (col, row.saturating_sub(1)),
            Dir::Down => (col, (row + 1).min(ROWS - 1)),
            Dir::Left => (col.saturating_sub(1), row),
            Dir::Right => ((col + 1).min(COLS - 1), row),
        };
    }

    fn begin_edit(&mut self, replace_with: Option<char>) {
        let buffer = match replace_with {
            Some(ch) => ch.to_string(),
            None => self.selected_source(),
        };
        self.edit = Some(EditState {
            buffer,
            fresh: true,
        });
    }

    /// Write the in-progress edit to the engine and leave editing mode. An
    /// all-whitespace buffer clears the cell.
    fn commit_edit(&mut self) {
        let Some(edit) = self.edit.take() else {
            return;
        };
        let (col, row) = self.selected;
        let addr = grid::cell_address(col, row);
        let source = if edit.buffer.trim().is_empty() {
            None
        } else {
            Some(edit.buffer.as_str())
        };
        let _ = self.engine.set_cell(self.sheet, &addr, source);
    }

    fn clear_selected(&mut self) {
        let (col, row) = self.selected;
        let _ = self
            .engine
            .set_cell(self.sheet, &grid::cell_address(col, row), None);
    }

    fn draw_grid(&mut self, ui: &mut egui::Ui) {
        let size = egui::vec2(
            grid::HEADER_W + COLS as f32 * grid::CELL_W,
            grid::HEADER_H + ROWS as f32 * grid::CELL_H,
        );
        let (response, painter) = ui.allocate_painter(size, egui::Sense::click());
        let origin = response.rect.min;

        // A click commits any in-progress edit, then moves the selection.
        if response.clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                if let Some((c, r)) = grid::cell_at(origin.x, origin.y, pos.x, pos.y) {
                    if c < COLS && r < ROWS {
                        self.commit_edit();
                        self.selected = (c, r);
                    }
                }
            }
        }

        let visuals = ui.visuals();
        let grid_line = egui::Stroke::new(1.0, visuals.weak_text_color());
        let header_bg = visuals.faint_bg_color;
        let cell_bg = visuals.extreme_bg_color;
        let text_color = visuals.text_color();
        let font = egui::FontId::proportional(13.0);

        // Column-letter headers.
        for c in 0..COLS {
            let rect = egui::Rect::from_min_size(
                egui::pos2(
                    origin.x + grid::HEADER_W + c as f32 * grid::CELL_W,
                    origin.y,
                ),
                egui::vec2(grid::CELL_W, grid::HEADER_H),
            );
            painter.rect_filled(rect, 0.0, header_bg);
            painter.rect_stroke(rect, 0.0, grid_line);
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                grid::column_label(c),
                font.clone(),
                text_color,
            );
        }
        // Row-number headers.
        for r in 0..ROWS {
            let rect = egui::Rect::from_min_size(
                egui::pos2(
                    origin.x,
                    origin.y + grid::HEADER_H + r as f32 * grid::CELL_H,
                ),
                egui::vec2(grid::HEADER_W, grid::CELL_H),
            );
            painter.rect_filled(rect, 0.0, header_bg);
            painter.rect_stroke(rect, 0.0, grid_line);
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                (r + 1).to_string(),
                font.clone(),
                text_color,
            );
        }
        // Cells and their values. The cell being edited is left blank —
        // the `TextEdit` overlay below stands in for it.
        let editing_cell = self.edit.as_ref().map(|_| self.selected);
        for r in 0..ROWS {
            for c in 0..COLS {
                let rect = grid::cell_rect(origin.x, origin.y, c, r);
                painter.rect_filled(rect, 0.0, cell_bg);
                painter.rect_stroke(rect, 0.0, grid_line);
                if editing_cell == Some((c, r)) {
                    continue;
                }
                let text = self.cell_text(c, r);
                if !text.is_empty() {
                    painter.text(
                        rect.left_center() + egui::vec2(5.0, 0.0),
                        egui::Align2::LEFT_CENTER,
                        text,
                        font.clone(),
                        text_color,
                    );
                }
            }
        }
        // The selection ring.
        let (sc, sr) = self.selected;
        painter.rect_stroke(
            grid::cell_rect(origin.x, origin.y, sc, sr),
            0.0,
            egui::Stroke::new(2.0, visuals.selection.stroke.color),
        );

        // The in-cell editor overlay.
        if let Some(edit) = &mut self.edit {
            let rect = grid::cell_rect(origin.x, origin.y, sc, sr);
            let response = ui.put(
                rect,
                egui::TextEdit::singleline(&mut edit.buffer)
                    .frame(true)
                    .font(egui::TextStyle::Monospace),
            );
            if std::mem::take(&mut edit.fresh) {
                response.request_focus();
                // Put the caret at the end so a type-to-edit appends.
                if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), response.id) {
                    let end = egui::text::CCursor::new(edit.buffer.chars().count());
                    state
                        .cursor
                        .set_char_range(Some(egui::text::CCursorRange::one(end)));
                    egui::TextEdit::store_state(ui.ctx(), response.id, state);
                }
            }
        }
    }
}

impl eframe::App for TescellateApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        for command in self.collect_commands(ctx) {
            self.apply(command);
        }

        egui::TopBottomPanel::top("tescellate_toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Tescellate");
                ui.label(egui::RichText::new("pure-Rust front-end · v2").weak());
            });
        });
        egui::TopBottomPanel::top("tescellate_formula_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let (c, r) = self.selected;
                ui.monospace(grid::cell_address(c, r));
                ui.separator();
                let shown = match &self.edit {
                    Some(edit) => edit.buffer.clone(),
                    None => self.selected_source(),
                };
                ui.label(if shown.is_empty() {
                    egui::RichText::new("(empty)").weak()
                } else {
                    egui::RichText::new(shown).monospace()
                });
            });
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::both()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    self.draw_grid(ui);
                });
        });
    }
}

/// Format a numeric cell value: integers without a fractional part, other
/// finite numbers with Rust's default float formatting.
fn format_number(n: f64) -> String {
    if n.is_finite() && n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}
