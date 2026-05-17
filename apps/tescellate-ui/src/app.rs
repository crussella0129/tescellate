//! The Tescellate application — an `eframe::App` that owns a
//! `WorkbookEngine` and draws the spreadsheet grid with egui.
//!
//! v1 scaffold: it renders a square grid of the engine's cell values and
//! supports click-to-select. Keyboard navigation, in-cell editing, and the
//! rest of a real spreadsheet's UI arrive in later versions.

use eframe::egui;
use tescellate_core::{CellValue, SheetId};
use tescellate_formula::WorkbookEngine;
use tescellate_tess::LatticeKind;

use crate::grid;

const COLS: u32 = 16;
const ROWS: u32 = 32;

/// The Tescellate front-end application.
pub struct TescellateApp {
    engine: WorkbookEngine,
    sheet: SheetId,
    /// Zero-indexed `(column, row)` of the selected cell.
    selected: (u32, u32),
}

impl TescellateApp {
    /// Build the app, seeding a small demo workbook so the scaffolded grid
    /// shows real, engine-computed data rather than a blank sheet.
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

    /// The raw source of the selected cell, for the formula bar.
    fn selected_source(&self) -> String {
        let (col, row) = self.selected;
        self.engine
            .get_cell(self.sheet, &grid::cell_address(col, row))
            .and_then(|s| s.source)
            .unwrap_or_default()
    }

    fn draw_grid(&mut self, ui: &mut egui::Ui) {
        let size = egui::vec2(
            grid::HEADER_W + COLS as f32 * grid::CELL_W,
            grid::HEADER_H + ROWS as f32 * grid::CELL_H,
        );
        let (response, painter) = ui.allocate_painter(size, egui::Sense::click());
        let origin = response.rect.min;

        if response.clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                if let Some((c, r)) = grid::cell_at(origin.x, origin.y, pos.x, pos.y) {
                    if c < COLS && r < ROWS {
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
        // Cells and their values.
        for r in 0..ROWS {
            for c in 0..COLS {
                let rect = grid::cell_rect(origin.x, origin.y, c, r);
                painter.rect_filled(rect, 0.0, cell_bg);
                painter.rect_stroke(rect, 0.0, grid_line);
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
        // The selection ring, drawn last so it sits on top.
        let (sc, sr) = self.selected;
        painter.rect_stroke(
            grid::cell_rect(origin.x, origin.y, sc, sr),
            0.0,
            egui::Stroke::new(2.0, visuals.selection.stroke.color),
        );
    }
}

impl eframe::App for TescellateApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("tescellate_toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Tescellate");
                ui.label(egui::RichText::new("pure-Rust front-end · v1 scaffold").weak());
            });
        });
        egui::TopBottomPanel::top("tescellate_formula_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let (c, r) = self.selected;
                ui.monospace(grid::cell_address(c, r));
                ui.separator();
                let source = self.selected_source();
                ui.label(if source.is_empty() {
                    egui::RichText::new("(empty)").weak()
                } else {
                    egui::RichText::new(source)
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
