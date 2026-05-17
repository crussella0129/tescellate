//! The Tescellate application — an `eframe::App` that owns a
//! `WorkbookEngine` and draws the spreadsheet grid with egui.
//!
//! v2: keyboard navigation and in-cell editing via the pure `keymap`
//! layer. v3: per-column / per-row sizing. v4: cell formatting from the
//! pure `format` layer. v5: the formatting ribbon — `format` set through
//! buttons and pickers, not only the keyboard.

use eframe::egui;
use tescellate_core::{CellValue, SheetId};
use tescellate_formula::WorkbookEngine;
use tescellate_tess::LatticeKind;

use crate::format::{self, CellFormat, FormatMap, HAlign};
use crate::grid::{self, GridMetrics};
use crate::keymap::{self, Command, Dir, Mode};
use crate::ribbon::{self, RibbonAction};

const COLS: u32 = 16;
const ROWS: u32 = 32;

/// Ctrl+Shift, for the alignment shortcuts.
const CTRL_SHIFT: egui::Modifiers = egui::Modifiers {
    alt: false,
    ctrl: true,
    shift: true,
    mac_cmd: false,
    command: false,
};

/// Key combos handled while navigating. Each is checked with
/// `consume_key`, so egui's own widgets never also see them.
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
    (egui::Modifiers::CTRL, egui::Key::B),
    (egui::Modifiers::CTRL, egui::Key::I),
    (CTRL_SHIFT, egui::Key::L),
    (CTRL_SHIFT, egui::Key::E),
    (CTRL_SHIFT, egui::Key::R),
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

/// A header border being dragged to resize.
#[derive(Debug, Clone, Copy)]
enum Resize {
    Column(u32),
    Row(u32),
}

/// The Tescellate front-end application.
pub struct TescellateApp {
    engine: WorkbookEngine,
    sheet: SheetId,
    /// Zero-indexed `(column, row)` of the selected cell.
    selected: (u32, u32),
    /// `Some` while a cell is being edited.
    edit: Option<EditState>,
    /// Per-column widths and per-row heights.
    metrics: GridMetrics,
    /// `Some` while a header border is being dragged.
    resizing: Option<Resize>,
    /// Per-cell visual formatting.
    formats: FormatMap,
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
            metrics: GridMetrics::new(),
            resizing: None,
            formats: FormatMap::new(),
        }
    }

    fn mode(&self) -> Mode {
        if self.edit.is_some() {
            Mode::Editing
        } else {
            Mode::Navigating
        }
    }

    /// The display text for a cell, read from the engine and rendered
    /// under the cell's number format.
    fn cell_text(&self, col: u32, row: u32) -> String {
        let addr = grid::cell_address(col, row);
        let Some(snapshot) = self.engine.get_cell(self.sheet, &addr) else {
            return String::new();
        };
        let number = self.formats.get((col, row)).number;
        match snapshot.value {
            CellValue::Empty => String::new(),
            CellValue::Number(n) => {
                format::render_number(n, number).unwrap_or_else(|| format_number(n))
            }
            CellValue::Integer(i) => {
                format::render_number(i as f64, number).unwrap_or_else(|| i.to_string())
            }
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
            if mode == Mode::Navigating {
                // The first typed character begins an edit; remove that
                // text event so the new `TextEdit` doesn't re-type it.
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
            Command::ToggleBold => {
                self.formats.update(self.selected, |f| f.bold = !f.bold);
            }
            Command::ToggleItalic => {
                self.formats.update(self.selected, |f| f.italic = !f.italic);
            }
            Command::SetAlign(align) => {
                self.formats.update(self.selected, |f| f.align = align);
            }
        }
    }

    /// Apply a formatting action from the ribbon to the selected cell.
    fn apply_ribbon(&mut self, action: RibbonAction) {
        let sel = self.selected;
        match action {
            RibbonAction::ToggleBold => self.formats.update(sel, |f| f.bold = !f.bold),
            RibbonAction::ToggleItalic => self.formats.update(sel, |f| f.italic = !f.italic),
            RibbonAction::SetAlign(align) => self.formats.update(sel, |f| f.align = align),
            RibbonAction::SetNumber(number) => self.formats.update(sel, |f| f.number = number),
            RibbonAction::SetTextColor(color) => self.formats.update(sel, |f| f.text_color = color),
            RibbonAction::SetFill(fill) => self.formats.update(sel, |f| f.fill = fill),
            RibbonAction::ClearFormat => self.formats.update(sel, |f| *f = CellFormat::default()),
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

    /// Resize the header border under an in-progress drag.
    fn handle_resize(&mut self, response: &egui::Response, origin: egui::Pos2) {
        if response.drag_started() {
            if let Some(p) = response.interact_pointer_pos() {
                self.resizing = self
                    .metrics
                    .col_border_at(origin, p, COLS)
                    .map(Resize::Column)
                    .or_else(|| self.metrics.row_border_at(origin, p, ROWS).map(Resize::Row));
            }
        }
        if response.dragged() {
            let delta = response.drag_delta();
            match self.resizing {
                Some(Resize::Column(c)) => {
                    let w = self.metrics.col_width(c) + delta.x;
                    self.metrics.set_col_width(c, w);
                }
                Some(Resize::Row(r)) => {
                    let h = self.metrics.row_height(r) + delta.y;
                    self.metrics.set_row_height(r, h);
                }
                None => {}
            }
        }
        if response.drag_stopped() {
            self.resizing = None;
        }
    }

    fn draw_grid(&mut self, ui: &mut egui::Ui) {
        let size = egui::vec2(
            self.metrics.total_width(COLS),
            self.metrics.total_height(ROWS),
        );
        let (response, painter) = ui.allocate_painter(size, egui::Sense::click_and_drag());
        let origin = response.rect.min;

        if let Some(p) = response.hover_pos() {
            if self.metrics.col_border_at(origin, p, COLS).is_some() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeColumn);
            } else if self.metrics.row_border_at(origin, p, ROWS).is_some() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeRow);
            }
        }
        self.handle_resize(&response, origin);

        if response.clicked() {
            if let Some(p) = response.interact_pointer_pos() {
                if let Some(cell) = self.metrics.cell_at(origin, p, COLS, ROWS) {
                    self.commit_edit();
                    self.selected = cell;
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
                egui::pos2(origin.x + self.metrics.col_left(c), origin.y),
                egui::vec2(self.metrics.col_width(c), grid::HEADER_H),
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
                egui::pos2(origin.x, origin.y + self.metrics.row_top(r)),
                egui::vec2(grid::HEADER_W, self.metrics.row_height(r)),
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
        // Cells: fill, border, and the formatted value. The cell being
        // edited is left blank for the `TextEdit` overlay below.
        let editing_cell = self.edit.as_ref().map(|_| self.selected);
        for r in 0..ROWS {
            for c in 0..COLS {
                let rect = self.metrics.cell_rect(origin, c, r);
                let fmt = self.formats.get((c, r));
                painter.rect_filled(rect, 0.0, fmt.fill.unwrap_or(cell_bg));
                painter.rect_stroke(rect, 0.0, grid_line);
                if editing_cell == Some((c, r)) {
                    continue;
                }
                let text = self.cell_text(c, r);
                if !text.is_empty() {
                    draw_cell_text(&painter, &text, rect, &fmt, text_color);
                }
            }
        }
        // The selection ring.
        let (sc, sr) = self.selected;
        painter.rect_stroke(
            self.metrics.cell_rect(origin, sc, sr),
            0.0,
            egui::Stroke::new(2.0, visuals.selection.stroke.color),
        );

        // The in-cell editor overlay.
        if let Some(edit) = &mut self.edit {
            let rect = self.metrics.cell_rect(origin, sc, sr);
            let response = ui.put(
                rect,
                egui::TextEdit::singleline(&mut edit.buffer)
                    .frame(true)
                    .font(egui::TextStyle::Monospace),
            );
            if std::mem::take(&mut edit.fresh) {
                response.request_focus();
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

        egui::TopBottomPanel::top("tescellate_ribbon").show(ctx, |ui| {
            let current = self.formats.get(self.selected);
            if let Some(action) = ribbon::ribbon(ui, &current) {
                self.apply_ribbon(action);
            }
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

/// Draw a cell's value with its formatting — colour, alignment, italic,
/// and faux-bold (a second offset pass, since egui's default font has no
/// bold weight).
fn draw_cell_text(
    painter: &egui::Painter,
    text: &str,
    rect: egui::Rect,
    fmt: &CellFormat,
    default_color: egui::Color32,
) {
    let color = fmt.text_color.unwrap_or(default_color);
    let mut job = egui::text::LayoutJob::default();
    job.append(
        text,
        0.0,
        egui::TextFormat {
            font_id: egui::FontId::proportional(13.0),
            color,
            italics: fmt.italic,
            ..Default::default()
        },
    );
    let galley = painter.layout_job(job);
    let pad = 5.0;
    let size = galley.size();
    let y = rect.center().y - size.y / 2.0;
    let x = match fmt.align {
        HAlign::Left => rect.left() + pad,
        HAlign::Center => rect.center().x - size.x / 2.0,
        HAlign::Right => rect.right() - pad - size.x,
    };
    let pos = egui::pos2(x, y);
    painter.galley(pos, galley.clone(), color);
    if fmt.bold {
        painter.galley(pos + egui::vec2(0.55, 0.0), galley, color);
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
