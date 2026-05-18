//! The Tescellate application — an `eframe::App` that owns a
//! `WorkbookEngine` and draws the spreadsheet with egui.
//!
//! The square sheet has keyboard navigation, in-cell editing, row/column
//! sizing, cell formatting, a formatting ribbon, multi-cell range
//! selection, copy/paste and (v10) undo/redo. The hex sheet renders
//! `tescellate-tess`'s `HexLattice` as a real tessellation and is
//! interactive too — both sheets share the pure `keymap` command layer.

use eframe::egui;
use tescellate_core::{CellValue, SheetId};
use tescellate_formula::WorkbookEngine;
use tescellate_tess::hex::{self, HexCoord, HexLattice};
use tescellate_tess::{Lattice, LatticeKind, Point2};

use crate::clipboard::Clipboard;
use crate::format::{self, CellFormat, FormatMap, HAlign};
use crate::grid::{self, GridMetrics};
use crate::history::History;
use crate::keymap::{self, Command, Dir, Mode};
use crate::ribbon::{self, RibbonAction};
use crate::selection::{FillDir, Selection};

const COLS: u32 = 16;
const ROWS: u32 = 32;

/// Circumradius of a rendered hex cell, in points.
const HEX_SIZE: f32 = 36.0;
/// How many rings of hexes the hex view shows around the origin.
const HEX_VIEW_RADIUS: i32 = 3;

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
    (egui::Modifiers::SHIFT, egui::Key::ArrowUp),
    (egui::Modifiers::SHIFT, egui::Key::ArrowDown),
    (egui::Modifiers::SHIFT, egui::Key::ArrowLeft),
    (egui::Modifiers::SHIFT, egui::Key::ArrowRight),
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
    (egui::Modifiers::CTRL, egui::Key::C),
    (egui::Modifiers::CTRL, egui::Key::V),
    (egui::Modifiers::CTRL, egui::Key::Z),
    (CTRL_SHIFT, egui::Key::Z),
    (egui::Modifiers::CTRL, egui::Key::Y),
    (egui::Modifiers::CTRL, egui::Key::D),
    (egui::Modifiers::CTRL, egui::Key::R),
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

/// Which of the workbook's sheets is currently on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActiveSheet {
    Square,
    Hex,
}

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

/// One cell's source before and after a change — the unit of undo/redo.
#[derive(Debug, Clone)]
struct CellEdit {
    sheet: SheetId,
    addr: String,
    before: Option<String>,
    after: Option<String>,
}

/// The Tescellate front-end application.
pub struct TescellateApp {
    engine: WorkbookEngine,
    /// The square-lattice sheet.
    square_sheet: SheetId,
    /// The hex-lattice sheet.
    hex_sheet: SheetId,
    /// Geometry for the hex sheet — owned by `tescellate-tess`.
    hex_lattice: HexLattice,
    /// Which sheet is on screen.
    active: ActiveSheet,
    /// The selected cell range on the square sheet.
    selection: Selection,
    /// Axial coord selected on the hex sheet.
    hex_selected: HexCoord,
    /// `Some` while a cell is being edited (on whichever sheet is active).
    edit: Option<EditState>,
    /// Per-column widths and per-row heights of the square sheet.
    metrics: GridMetrics,
    /// `Some` while a header border is being dragged.
    resizing: Option<Resize>,
    /// Per-cell visual formatting of the square sheet.
    formats: FormatMap,
    /// The last copied block of cell sources.
    clipboard: Clipboard,
    /// Undo/redo history — each entry is one action's cell edits.
    history: History<Vec<CellEdit>>,
}

impl TescellateApp {
    /// Build the app, seeding a square demo sheet and a hex demo sheet.
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let mut engine = WorkbookEngine::new();
        engine.new_workbook();

        let square_sheet = engine.add_sheet("Budget", LatticeKind::Square);
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
            let _ = engine.set_cell(square_sheet, addr, Some(src));
        }

        // A pointy-top hex sheet: a labelled core, its six edge-neighbors,
        // and a cell summing an axial range — the hex address form is
        // `H(q,r)`, and `SUM` over a hex range is an axial parallelogram.
        let hex_sheet = engine.add_sheet("Hex demo", LatticeKind::HexPointy);
        for (addr, src) in [
            ("H(0,0)", "core"),
            ("H(1,0)", "=12"),
            ("H(-1,0)", "=20"),
            ("H(0,1)", "=10"),
            ("H(0,-1)", "=15"),
            ("H(1,-1)", "=8"),
            ("H(-1,1)", "=5"),
            ("H(0,2)", "=SUM(H(-1,-1):H(1,1))"),
        ] {
            let _ = engine.set_cell(hex_sheet, addr, Some(src));
        }

        Self {
            engine,
            square_sheet,
            hex_sheet,
            hex_lattice: HexLattice::pointy(HEX_SIZE),
            active: ActiveSheet::Square,
            selection: Selection::single((0, 0)),
            hex_selected: HexCoord::new(0, 0),
            edit: None,
            metrics: GridMetrics::new(),
            resizing: None,
            formats: FormatMap::new(),
            clipboard: Clipboard::default(),
            history: History::new(),
        }
    }

    fn mode(&self) -> Mode {
        if self.edit.is_some() {
            Mode::Editing
        } else {
            Mode::Navigating
        }
    }

    /// The display text for a square-sheet cell, read from the engine and
    /// rendered under the cell's number format.
    fn cell_text(&self, col: u32, row: u32) -> String {
        let addr = grid::cell_address(col, row);
        let Some(snapshot) = self.engine.get_cell(self.square_sheet, &addr) else {
            return String::new();
        };
        let number = self.formats.get((col, row)).number;
        match snapshot.value {
            CellValue::Number(n) => {
                format::render_number(n, number).unwrap_or_else(|| format_number(n))
            }
            CellValue::Integer(i) => {
                format::render_number(i as f64, number).unwrap_or_else(|| i.to_string())
            }
            other => natural_text(other),
        }
    }

    /// The display text for a hex-sheet cell — no number format, since
    /// formatting is square-only for now.
    fn hex_cell_text(&self, coord: HexCoord) -> String {
        self.engine
            .get_cell(self.hex_sheet, &hex_address(coord))
            .map(|snapshot| natural_text(snapshot.value))
            .unwrap_or_default()
    }

    /// The raw source of the active square cell (the selection cursor).
    fn selected_source(&self) -> String {
        let (col, row) = self.selection.cursor;
        self.engine
            .get_cell(self.square_sheet, &grid::cell_address(col, row))
            .and_then(|s| s.source)
            .unwrap_or_default()
    }

    /// The raw source of the active cell on whichever sheet is active.
    fn active_cell_source(&self) -> String {
        match self.active {
            ActiveSheet::Square => self.selected_source(),
            ActiveSheet::Hex => self
                .engine
                .get_cell(self.hex_sheet, &hex_address(self.hex_selected))
                .and_then(|s| s.source)
                .unwrap_or_default(),
        }
    }

    /// The `(sheet, address)` of the active cell on the active sheet.
    fn active_target(&self) -> (SheetId, String) {
        match self.active {
            ActiveSheet::Square => {
                let (col, row) = self.selection.cursor;
                (self.square_sheet, grid::cell_address(col, row))
            }
            ActiveSheet::Hex => (self.hex_sheet, hex_address(self.hex_selected)),
        }
    }

    /// The active cell's address and source text on the active sheet,
    /// for the formula bar.
    fn active_address_and_source(&self) -> (String, String) {
        let addr = match self.active {
            ActiveSheet::Square => {
                let (col, row) = self.selection.cursor;
                grid::cell_address(col, row)
            }
            ActiveSheet::Hex => hex_address(self.hex_selected),
        };
        let source = match &self.edit {
            Some(edit) => edit.buffer.clone(),
            None => self.active_cell_source(),
        };
        (addr, source)
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

    fn apply(&mut self, command: Command, ctx: &egui::Context) {
        match command {
            Command::Move(dir) => self.move_active(dir),
            Command::Extend(dir) => self.extend_active(dir),
            Command::MoveToRowStart => match self.active {
                ActiveSheet::Square => {
                    let row = self.selection.cursor.1;
                    self.selection.collapse_to((0, row));
                }
                ActiveSheet::Hex => self.hex_selected = HexCoord::new(0, 0),
            },
            Command::MoveToOrigin => match self.active {
                ActiveSheet::Square => self.selection.collapse_to((0, 0)),
                ActiveSheet::Hex => self.hex_selected = HexCoord::new(0, 0),
            },
            Command::BeginEdit { replace_with } => self.begin_edit(replace_with),
            Command::Commit(dir) => {
                self.commit_edit();
                self.move_active(dir);
            }
            Command::Cancel => self.edit = None,
            Command::Clear => self.clear_active(),
            Command::ToggleBold => self.format_selected(|f| f.bold = !f.bold),
            Command::ToggleItalic => self.format_selected(|f| f.italic = !f.italic),
            Command::SetAlign(align) => self.format_selected(|f| f.align = align),
            Command::Copy => self.copy_selection(ctx),
            Command::Paste => self.paste(),
            Command::Undo => self.undo(),
            Command::Redo => self.redo(),
            Command::FillDown => self.fill(FillDir::Down),
            Command::FillRight => self.fill(FillDir::Right),
        }
    }

    /// Apply a formatting action from the ribbon to the active cell.
    fn apply_ribbon(&mut self, action: RibbonAction) {
        let sel = self.selection.cursor;
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

    /// Mutate the active square cell's format. A no-op on the hex sheet,
    /// which carries no formatting yet.
    fn format_selected(&mut self, edit: impl FnOnce(&mut CellFormat)) {
        if self.active == ActiveSheet::Square {
            self.formats.update(self.selection.cursor, edit);
        }
    }

    /// Move the selection on whichever sheet is active, collapsing any
    /// square-sheet range to a single cell.
    fn move_active(&mut self, dir: Dir) {
        match self.active {
            ActiveSheet::Square => {
                let next = step_square(self.selection.cursor, dir);
                self.selection.collapse_to(next);
            }
            ActiveSheet::Hex => self.move_hex_selection(dir),
        }
    }

    /// Extend the selection one cell — Shift+arrow. On the hex sheet,
    /// which has no range model yet, this is a plain move.
    fn extend_active(&mut self, dir: Dir) {
        match self.active {
            ActiveSheet::Square => {
                let next = step_square(self.selection.cursor, dir);
                self.selection.extend_to(next);
            }
            ActiveSheet::Hex => self.move_hex_selection(dir),
        }
    }

    /// Step the hex selection one axial cell, ignoring a move that would
    /// leave the visible disc.
    fn move_hex_selection(&mut self, dir: Dir) {
        let next = hex_step(self.hex_selected, dir);
        if HexCoord::new(0, 0).distance(next) <= HEX_VIEW_RADIUS {
            self.hex_selected = next;
        }
    }

    fn begin_edit(&mut self, replace_with: Option<char>) {
        // An edit acts on a single cell — collapse any range to its cursor.
        if self.active == ActiveSheet::Square {
            let cursor = self.selection.cursor;
            self.selection.collapse_to(cursor);
        }
        let buffer = match replace_with {
            Some(ch) => ch.to_string(),
            None => self.active_cell_source(),
        };
        self.edit = Some(EditState {
            buffer,
            fresh: true,
        });
    }

    /// Apply a batch of cell writes as one undoable step. Each target is
    /// an `(address, new source)`; a target that doesn't actually change
    /// the cell is dropped, so an undo step never holds a no-op.
    fn apply_edits(&mut self, sheet: SheetId, targets: Vec<(String, Option<String>)>) {
        let mut edits = Vec::new();
        for (addr, after) in targets {
            let before = self.engine.get_cell(sheet, &addr).and_then(|s| s.source);
            if before == after {
                continue;
            }
            let _ = self.engine.set_cell(sheet, &addr, after.as_deref());
            edits.push(CellEdit {
                sheet,
                addr,
                before,
                after,
            });
        }
        if !edits.is_empty() {
            self.history.record(edits);
        }
    }

    /// Write the in-progress edit to the active sheet and leave editing
    /// mode. An all-whitespace buffer clears the cell.
    fn commit_edit(&mut self) {
        let Some(edit) = self.edit.take() else {
            return;
        };
        let source = if edit.buffer.trim().is_empty() {
            None
        } else {
            Some(edit.buffer)
        };
        let (sheet, addr) = self.active_target();
        self.apply_edits(sheet, vec![(addr, source)]);
    }

    /// Clear the selection on whichever sheet is active — the whole range
    /// on the square sheet, the single selected cell on the hex sheet.
    fn clear_active(&mut self) {
        let (sheet, targets) = match self.active {
            ActiveSheet::Square => {
                let targets = self
                    .selection
                    .cells()
                    .map(|(c, r)| (grid::cell_address(c, r), None))
                    .collect();
                (self.square_sheet, targets)
            }
            ActiveSheet::Hex => (self.hex_sheet, vec![(hex_address(self.hex_selected), None)]),
        };
        self.apply_edits(sheet, targets);
    }

    /// Copy the selected range — square sheet only. Captures each cell's
    /// source into the in-app clipboard, and pushes a tab-separated grid
    /// of *displayed values* to the system clipboard for interop.
    fn copy_selection(&mut self, ctx: &egui::Context) {
        if self.active != ActiveSheet::Square {
            return;
        }
        let ((min_c, min_r), (max_c, max_r)) = self.selection.bounds();
        let (width, height) = self.selection.dimensions();
        let mut cells = Vec::with_capacity((width * height) as usize);
        let mut tsv = String::new();
        for r in min_r..=max_r {
            for c in min_c..=max_c {
                if c > min_c {
                    tsv.push('\t');
                }
                tsv.push_str(&self.cell_text(c, r));
                let source = self
                    .engine
                    .get_cell(self.square_sheet, &grid::cell_address(c, r))
                    .and_then(|s| s.source);
                cells.push(source);
            }
            tsv.push('\n');
        }
        self.clipboard = Clipboard::capture(width, height, cells);
        ctx.copy_text(tsv);
    }

    /// Paste the clipboard block with its top-left at the active cell —
    /// square sheet only, recorded as one undo step. Sources are written
    /// verbatim (no relative-reference rewriting yet).
    fn paste(&mut self) {
        if self.active != ActiveSheet::Square || self.clipboard.is_empty() {
            return;
        }
        let (target_c, target_r) = self.selection.cursor;
        let (width, height) = self.clipboard.dimensions();
        let mut targets = Vec::new();
        for (rel_c, rel_r, src) in self.clipboard.entries() {
            let c = target_c + rel_c;
            let r = target_r + rel_r;
            if c >= COLS || r >= ROWS {
                continue;
            }
            targets.push((grid::cell_address(c, r), src.map(str::to_string)));
        }
        self.apply_edits(self.square_sheet, targets);
        // Select the pasted block, with the active cell at its top-left.
        let end_c = (target_c + width - 1).min(COLS - 1);
        let end_r = (target_r + height - 1).min(ROWS - 1);
        self.selection = Selection {
            anchor: (end_c, end_r),
            cursor: (target_c, target_r),
        };
    }

    /// Revert the most recent action — restores each changed cell's prior
    /// source. The engine recomputes any dependents.
    fn undo(&mut self) {
        let Some(edits) = self.history.undo() else {
            return;
        };
        for edit in &edits {
            let _ = self
                .engine
                .set_cell(edit.sheet, &edit.addr, edit.before.as_deref());
        }
    }

    /// Re-apply the most recently undone action.
    fn redo(&mut self) {
        let Some(edits) = self.history.redo() else {
            return;
        };
        for edit in &edits {
            let _ = self
                .engine
                .set_cell(edit.sheet, &edit.addr, edit.after.as_deref());
        }
    }

    /// Fill the selection's leading edge across the rest of it — Ctrl+D
    /// (down) / Ctrl+R (right), square sheet only. Sources copy verbatim
    /// and the whole fill is a single undo step.
    fn fill(&mut self, dir: FillDir) {
        if self.active != ActiveSheet::Square {
            return;
        }
        let mut targets = Vec::new();
        for (target, from) in self.selection.fill_targets(dir) {
            let source = self
                .engine
                .get_cell(self.square_sheet, &grid::cell_address(from.0, from.1))
                .and_then(|s| s.source);
            targets.push((grid::cell_address(target.0, target.1), source));
        }
        self.apply_edits(self.square_sheet, targets);
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

    /// The square cell under a pointer position, if any.
    fn cell_under(&self, response: &egui::Response, origin: egui::Pos2) -> Option<(u32, u32)> {
        response
            .interact_pointer_pos()
            .and_then(|p| self.metrics.cell_at(origin, p, COLS, ROWS))
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

        // A drag that didn't start on a header border sweeps a selection.
        if self.resizing.is_none() {
            if response.drag_started() {
                if let Some(cell) = self.cell_under(&response, origin) {
                    self.commit_edit();
                    self.selection.collapse_to(cell);
                }
            } else if response.dragged() {
                if let Some(cell) = self.cell_under(&response, origin) {
                    self.selection.extend_to(cell);
                }
            }
        }
        if response.clicked() {
            if let Some(cell) = self.cell_under(&response, origin) {
                self.commit_edit();
                // Shift-click extends the range; a plain click resets it.
                if ui.input(|i| i.modifiers.shift) {
                    self.selection.extend_to(cell);
                } else {
                    self.selection.collapse_to(cell);
                }
            }
        }

        let visuals = ui.visuals();
        let grid_line = egui::Stroke::new(1.0, visuals.weak_text_color());
        let header_bg = visuals.faint_bg_color;
        let cell_bg = visuals.extreme_bg_color;
        let text_color = visuals.text_color();
        let sel_color = visuals.selection.stroke.color;
        let sel = visuals.selection.bg_fill;
        let sel_tint = egui::Color32::from_rgba_unmultiplied(sel.r(), sel.g(), sel.b(), 64);
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
        // Cells: fill, the selection tint, border, and the formatted
        // value. The cell being edited is left blank for the overlay.
        let cursor = self.selection.cursor;
        let editing_cell = self.edit.as_ref().map(|_| cursor);
        for r in 0..ROWS {
            for c in 0..COLS {
                let rect = self.metrics.cell_rect(origin, c, r);
                let fmt = self.formats.get((c, r));
                painter.rect_filled(rect, 0.0, fmt.fill.unwrap_or(cell_bg));
                // The active cell stays untinted; the rest of the range
                // gets a translucent wash, the way Excel/Sheets show it.
                if (c, r) != cursor && self.selection.contains((c, r)) {
                    painter.rect_filled(rect, 0.0, sel_tint);
                }
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
        // The range border, when the selection spans more than one cell.
        if self.selection.is_range() {
            let ((min_c, min_r), (max_c, max_r)) = self.selection.bounds();
            let tl = self.metrics.cell_rect(origin, min_c, min_r);
            let br = self.metrics.cell_rect(origin, max_c, max_r);
            painter.rect_stroke(
                egui::Rect::from_min_max(tl.min, br.max),
                0.0,
                egui::Stroke::new(1.5, sel_color),
            );
        }
        // The active-cell ring.
        let (sc, sr) = cursor;
        painter.rect_stroke(
            self.metrics.cell_rect(origin, sc, sr),
            0.0,
            egui::Stroke::new(2.0, sel_color),
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

    /// Paint one hexagon — its polygon and cell value — using vertices
    /// and a centroid straight from the engine's lattice.
    fn paint_hex(
        &self,
        painter: &egui::Painter,
        origin: egui::Pos2,
        coord: HexCoord,
        fill: egui::Color32,
        stroke: egui::Stroke,
        text_color: egui::Color32,
    ) {
        let vertices: Vec<egui::Pos2> = self
            .hex_lattice
            .vertices(coord)
            .iter()
            .map(|v| egui::pos2(origin.x + v.x, origin.y + v.y))
            .collect();
        painter.add(egui::Shape::convex_polygon(vertices, fill, stroke));

        let text = self.hex_cell_text(coord);
        if !text.is_empty() {
            let centroid = self.hex_lattice.centroid(coord);
            painter.text(
                egui::pos2(origin.x + centroid.x, origin.y + centroid.y),
                egui::Align2::CENTER_CENTER,
                text,
                egui::FontId::proportional(13.0),
                text_color,
            );
        }
    }

    fn draw_hex_grid(&mut self, ui: &mut egui::Ui) {
        let size = ui.available_size();
        let (response, painter) = ui.allocate_painter(size, egui::Sense::click());
        // Lattice-space (0,0) is drawn at the panel's centre.
        let origin = response.rect.center();

        if response.clicked() {
            if let Some(p) = response.interact_pointer_pos() {
                let local = Point2::new(p.x - origin.x, p.y - origin.y);
                if let Some(coord) = self.hex_lattice.cell_at(local) {
                    if HexCoord::new(0, 0).distance(coord) <= HEX_VIEW_RADIUS {
                        self.commit_edit();
                        self.hex_selected = coord;
                    }
                }
            }
        }

        let visuals = ui.visuals();
        let line = egui::Stroke::new(1.0, visuals.weak_text_color());
        let sel_stroke = egui::Stroke::new(2.5, visuals.selection.stroke.color);
        let cell_bg = visuals.extreme_bg_color;
        let sel_bg = visuals.selection.bg_fill;
        let text_color = visuals.text_color();

        for coord in hex::hex_disc(HexCoord::new(0, 0), HEX_VIEW_RADIUS) {
            if coord == self.hex_selected {
                continue;
            }
            self.paint_hex(&painter, origin, coord, cell_bg, line, text_color);
        }
        // The selected hex is painted last so its ring sits above every
        // neighbour's shared border. While editing, the text editor
        // overlay (drawn below) covers the cell's painted value.
        self.paint_hex(
            &painter,
            origin,
            self.hex_selected,
            sel_bg,
            sel_stroke,
            text_color,
        );

        // The in-cell editor overlay, sized to sit within the hexagon.
        if let Some(edit) = &mut self.edit {
            let centroid = self.hex_lattice.centroid(self.hex_selected);
            let center = egui::pos2(origin.x + centroid.x, origin.y + centroid.y);
            let rect = egui::Rect::from_center_size(center, egui::vec2(1.6 * HEX_SIZE, 24.0));
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
            self.apply(command, ctx);
        }

        egui::TopBottomPanel::top("tescellate_ribbon").show(ctx, |ui| match self.active {
            ActiveSheet::Square => {
                let current = self.formats.get(self.selection.cursor);
                if let Some(action) = ribbon::ribbon(ui, &current) {
                    self.apply_ribbon(action);
                }
            }
            ActiveSheet::Hex => {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Tescellate").strong());
                    ui.separator();
                    ui.label("Hex demo — arrows move along the q/r axes; type or F2 to edit.");
                });
            }
        });

        egui::TopBottomPanel::top("tescellate_formula_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let (addr, source) = self.active_address_and_source();
                ui.monospace(addr);
                if self.active == ActiveSheet::Square && self.selection.is_range() {
                    let (cols, rows) = self.selection.dimensions();
                    ui.label(egui::RichText::new(format!("{cols}C × {rows}R")).weak());
                }
                ui.separator();
                ui.label(if source.is_empty() {
                    egui::RichText::new("(empty)").weak()
                } else {
                    egui::RichText::new(source).monospace()
                });
            });
        });

        egui::TopBottomPanel::bottom("tescellate_sheet_tabs").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui
                    .selectable_label(self.active == ActiveSheet::Square, "Budget")
                    .clicked()
                {
                    self.commit_edit();
                    self.active = ActiveSheet::Square;
                }
                if ui
                    .selectable_label(self.active == ActiveSheet::Hex, "Hex demo")
                    .clicked()
                {
                    self.commit_edit();
                    self.active = ActiveSheet::Hex;
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| match self.active {
            ActiveSheet::Square => {
                egui::ScrollArea::both()
                    .auto_shrink([false, false])
                    .show(ui, |ui| self.draw_grid(ui));
            }
            ActiveSheet::Hex => self.draw_hex_grid(ui),
        });
    }
}

/// Draw a square cell's value with its formatting — colour, alignment,
/// italic, and faux-bold (a second offset pass, since egui's default font
/// has no bold weight).
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

/// One step from a square cell, clamped to the grid.
fn step_square(cell: (u32, u32), dir: Dir) -> (u32, u32) {
    let (col, row) = cell;
    match dir {
        Dir::Up => (col, row.saturating_sub(1)),
        Dir::Down => (col, (row + 1).min(ROWS - 1)),
        Dir::Left => (col.saturating_sub(1), row),
        Dir::Right => ((col + 1).min(COLS - 1), row),
    }
}

/// One axial step from a hex coord. On the pointy-top lattice, left/right
/// run along the q-axis (pure horizontal on screen) and up/down along the
/// r-axis. Any combination reaches every hex.
fn hex_step(coord: HexCoord, dir: Dir) -> HexCoord {
    match dir {
        Dir::Left => HexCoord::new(coord.q - 1, coord.r),
        Dir::Right => HexCoord::new(coord.q + 1, coord.r),
        Dir::Up => HexCoord::new(coord.q, coord.r - 1),
        Dir::Down => HexCoord::new(coord.q, coord.r + 1),
    }
}

/// Render a cell value with the engine's natural formatting — no number
/// format applied. Used for the hex sheet and as the square sheet's
/// fallback for non-numeric values.
fn natural_text(value: CellValue) -> String {
    match value {
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

/// Format a numeric cell value: integers without a fractional part, other
/// finite numbers with Rust's default float formatting.
fn format_number(n: f64) -> String {
    if n.is_finite() && n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

/// The `H(q,r)` address string for a hex coord — the form the engine's
/// hex lattice canonicalizes to.
fn hex_address(c: HexCoord) -> String {
    format!("H({},{})", c.q, c.r)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_number_drops_the_point_for_integers() {
        assert_eq!(format_number(3.0), "3");
        assert_eq!(format_number(-7.0), "-7");
        assert_eq!(format_number(0.0), "0");
    }

    #[test]
    fn format_number_keeps_a_fractional_part() {
        assert_eq!(format_number(3.5), "3.5");
        assert_eq!(format_number(-0.25), "-0.25");
    }

    #[test]
    fn natural_text_renders_each_value_kind() {
        assert_eq!(natural_text(CellValue::Empty), "");
        assert_eq!(natural_text(CellValue::Number(42.0)), "42");
        assert_eq!(natural_text(CellValue::Bool(true)), "TRUE");
        assert_eq!(natural_text(CellValue::Bool(false)), "FALSE");
        assert_eq!(natural_text(CellValue::Text("hi".to_string())), "hi");
    }

    #[test]
    fn hex_address_formats_axial_coords() {
        assert_eq!(hex_address(HexCoord::new(0, 0)), "H(0,0)");
        assert_eq!(hex_address(HexCoord::new(2, -3)), "H(2,-3)");
        assert_eq!(hex_address(HexCoord::new(-1, 4)), "H(-1,4)");
    }

    #[test]
    fn hex_step_moves_along_the_axial_axes() {
        let origin = HexCoord::new(0, 0);
        assert_eq!(hex_step(origin, Dir::Right), HexCoord::new(1, 0));
        assert_eq!(hex_step(origin, Dir::Left), HexCoord::new(-1, 0));
        assert_eq!(hex_step(origin, Dir::Down), HexCoord::new(0, 1));
        assert_eq!(hex_step(origin, Dir::Up), HexCoord::new(0, -1));
    }

    #[test]
    fn hex_step_opposite_directions_cancel() {
        let c = HexCoord::new(2, -3);
        assert_eq!(hex_step(hex_step(c, Dir::Right), Dir::Left), c);
        assert_eq!(hex_step(hex_step(c, Dir::Down), Dir::Up), c);
    }

    #[test]
    fn hex_step_is_always_a_unit_move() {
        let c = HexCoord::new(1, 1);
        for dir in [Dir::Up, Dir::Down, Dir::Left, Dir::Right] {
            assert_eq!(c.distance(hex_step(c, dir)), 1);
        }
    }

    #[test]
    fn step_square_clamps_at_the_grid_edges() {
        // The top-left corner can't move further up or left.
        assert_eq!(step_square((0, 0), Dir::Up), (0, 0));
        assert_eq!(step_square((0, 0), Dir::Left), (0, 0));
        // The bottom-right corner can't move further down or right.
        assert_eq!(
            step_square((COLS - 1, ROWS - 1), Dir::Right),
            (COLS - 1, ROWS - 1)
        );
        assert_eq!(
            step_square((COLS - 1, ROWS - 1), Dir::Down),
            (COLS - 1, ROWS - 1)
        );
    }

    #[test]
    fn step_square_moves_within_the_grid() {
        assert_eq!(step_square((3, 4), Dir::Right), (4, 4));
        assert_eq!(step_square((3, 4), Dir::Down), (3, 5));
        assert_eq!(step_square((3, 4), Dir::Left), (2, 4));
        assert_eq!(step_square((3, 4), Dir::Up), (3, 3));
    }
}
