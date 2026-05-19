//! The Tescellate application — an `eframe::App` that owns a
//! `WorkbookEngine` and draws the spreadsheet with egui.
//!
//! The square sheet has keyboard navigation, in-cell editing, row/column
//! sizing, cell formatting, a formatting ribbon, multi-cell range
//! selection, copy/paste and (v10) undo/redo. The hex sheet renders
//! `tescellate-tess`'s `HexLattice` as a real tessellation and is
//! interactive too — both sheets share the pure `keymap` command layer.

use eframe::egui;
use tescellate_core::{CellError, CellValue, SheetId};
use tescellate_formula::WorkbookEngine;
use tescellate_tess::hex::{self, HexCoord, HexLattice, HexOrientation};
use tescellate_tess::{Lattice, LatticeKind, Point2};

use crate::clipboard::{Clipboard, CopiedCell, PasteMode};
use crate::conditional::{self, Condition, Rule};
use crate::find::{self, FindState};
use crate::format::{self, BorderMode, Borders, CellFormat, FormatMap, HAlign, HexBorders, VAlign};
use crate::grid::{self, GridMetrics};
use crate::history::History;
use crate::keymap::{self, Command, Dir, Mode};
use crate::note::NoteMap;
use crate::ribbon::{self, RibbonAction};
use crate::selection::{FillDir, HexSelection, Selection};
use crate::sort;
use crate::stats;
use crate::widget::{self, Widgets};

const COLS: u32 = 16;
const ROWS: u32 = 32;
/// How many rows a Page Up / Page Down moves the cursor.
const PAGE_ROWS: u32 = 16;

/// Circumradius of a rendered hex cell, in points.
const HEX_SIZE: f32 = 36.0;
/// How many rings of hexes the hex view shows around the origin.
const HEX_VIEW_RADIUS: i32 = 3;

/// How close in time (seconds) two format edits must be to coalesce into
/// one undo step — long enough to catch a colour-picker drag, short
/// enough that two deliberate edits stay separate.
const COALESCE_WINDOW: f64 = 0.6;

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
    (egui::Modifiers::NONE, egui::Key::End),
    (egui::Modifiers::CTRL, egui::Key::End),
    (egui::Modifiers::NONE, egui::Key::PageUp),
    (egui::Modifiers::NONE, egui::Key::PageDown),
    (egui::Modifiers::NONE, egui::Key::Escape),
    (egui::Modifiers::NONE, egui::Key::F2),
    (egui::Modifiers::NONE, egui::Key::F3),
    (egui::Modifiers::SHIFT, egui::Key::F3),
    (egui::Modifiers::NONE, egui::Key::F1),
    (egui::Modifiers::NONE, egui::Key::Delete),
    (egui::Modifiers::NONE, egui::Key::Backspace),
    (egui::Modifiers::CTRL, egui::Key::B),
    (egui::Modifiers::CTRL, egui::Key::I),
    (egui::Modifiers::CTRL, egui::Key::C),
    (egui::Modifiers::CTRL, egui::Key::V),
    (egui::Modifiers::CTRL, egui::Key::X),
    (egui::Modifiers::CTRL, egui::Key::Z),
    (CTRL_SHIFT, egui::Key::Z),
    (egui::Modifiers::CTRL, egui::Key::Y),
    (egui::Modifiers::CTRL, egui::Key::D),
    (egui::Modifiers::CTRL, egui::Key::R),
    (egui::Modifiers::CTRL, egui::Key::F),
    (CTRL_SHIFT, egui::Key::L),
    (CTRL_SHIFT, egui::Key::E),
    (CTRL_SHIFT, egui::Key::R),
    (CTRL_SHIFT, egui::Key::Num8),
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

/// A cell on either sheet — Find tracks matches across both lattices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CellId {
    Square((u32, u32)),
    Hex(HexCoord),
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

/// A column or row header being dragged to sweep a range selection. The
/// index is the column/row where the drag began.
#[derive(Debug, Clone, Copy)]
enum HeaderDrag {
    Column(u32),
    Row(u32),
}

/// The kind of condition picked in the conditional-formatting editor — a
/// [`Condition`] without its threshold, which the editor's text field
/// supplies separately.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CondKind {
    Greater,
    GreaterEqual,
    Less,
    LessEqual,
    Equal,
    NotEqual,
    Between,
    Contains,
    IsTrue,
    IsFalse,
    NonEmpty,
    IsEmpty,
}

impl CondKind {
    const ALL: [CondKind; 12] = [
        CondKind::Greater,
        CondKind::GreaterEqual,
        CondKind::Less,
        CondKind::LessEqual,
        CondKind::Equal,
        CondKind::NotEqual,
        CondKind::Between,
        CondKind::Contains,
        CondKind::IsTrue,
        CondKind::IsFalse,
        CondKind::NonEmpty,
        CondKind::IsEmpty,
    ];

    fn label(self) -> &'static str {
        match self {
            CondKind::Greater => "greater than",
            CondKind::GreaterEqual => "greater or equal",
            CondKind::Less => "less than",
            CondKind::LessEqual => "less or equal",
            CondKind::Equal => "equal to",
            CondKind::NotEqual => "not equal to",
            CondKind::Between => "between",
            CondKind::Contains => "contains text",
            CondKind::IsTrue => "is TRUE",
            CondKind::IsFalse => "is FALSE",
            CondKind::NonEmpty => "non-empty",
            CondKind::IsEmpty => "empty",
        }
    }

    /// Whether this kind needs the first numeric threshold field.
    fn needs_threshold(self) -> bool {
        matches!(
            self,
            CondKind::Greater
                | CondKind::GreaterEqual
                | CondKind::Less
                | CondKind::LessEqual
                | CondKind::Equal
                | CondKind::NotEqual
                | CondKind::Between
                | CondKind::Contains
        )
    }

    /// Whether this kind needs the second numeric threshold field — only
    /// `Between`, whose range carries an upper bound.
    fn needs_threshold2(self) -> bool {
        matches!(self, CondKind::Between)
    }

    /// The kind matching an existing condition — the inverse of the
    /// `kind` arm of [`CondDraft::build`].
    fn from_condition(condition: &Condition) -> CondKind {
        match condition {
            Condition::GreaterThan(_) => CondKind::Greater,
            Condition::GreaterOrEqual(_) => CondKind::GreaterEqual,
            Condition::LessThan(_) => CondKind::Less,
            Condition::LessOrEqual(_) => CondKind::LessEqual,
            Condition::EqualTo(_) => CondKind::Equal,
            Condition::NotEqualTo(_) => CondKind::NotEqual,
            Condition::Between(..) => CondKind::Between,
            Condition::Contains(_) => CondKind::Contains,
            Condition::IsTrue => CondKind::IsTrue,
            Condition::IsFalse => CondKind::IsFalse,
            Condition::NonEmpty => CondKind::NonEmpty,
            Condition::IsEmpty => CondKind::IsEmpty,
        }
    }
}

/// The in-progress new rule in the conditional-formatting editor.
struct CondDraft {
    kind: CondKind,
    threshold: String,
    /// The range's upper bound for `Between`; unused by other kinds.
    threshold2: String,
    fill: egui::Color32,
    /// Whether a matching cell is also rendered bold.
    bold: bool,
    /// Whether a matching cell is also rendered italic.
    italic: bool,
    /// Whether a matching cell is also struck through.
    strikethrough: bool,
    /// Whether a matching cell is also underlined.
    underline: bool,
    /// Whether a matching cell's text is recoloured, and to what.
    text_color_on: bool,
    text_color: egui::Color32,
}

impl Default for CondDraft {
    fn default() -> Self {
        Self {
            kind: CondKind::Greater,
            threshold: "100".to_string(),
            threshold2: "200".to_string(),
            fill: egui::Color32::from_rgb(255, 235, 156),
            bold: false,
            italic: false,
            strikethrough: false,
            underline: false,
            text_color_on: false,
            text_color: egui::Color32::from_rgb(190, 40, 40),
        }
    }
}

impl CondDraft {
    /// Build a [`Rule`] from the draft, or `None` when a numeric
    /// threshold is required but the field doesn't parse.
    fn build(&self) -> Option<Rule> {
        let condition = match self.kind {
            CondKind::Greater => Condition::GreaterThan(self.threshold.trim().parse().ok()?),
            CondKind::GreaterEqual => {
                Condition::GreaterOrEqual(self.threshold.trim().parse().ok()?)
            }
            CondKind::Less => Condition::LessThan(self.threshold.trim().parse().ok()?),
            CondKind::LessEqual => Condition::LessOrEqual(self.threshold.trim().parse().ok()?),
            CondKind::Equal => Condition::EqualTo(self.threshold.trim().parse().ok()?),
            CondKind::NotEqual => Condition::NotEqualTo(self.threshold.trim().parse().ok()?),
            CondKind::Between => Condition::Between(
                self.threshold.trim().parse().ok()?,
                self.threshold2.trim().parse().ok()?,
            ),
            CondKind::Contains => {
                let needle = self.threshold.trim();
                if needle.is_empty() {
                    return None;
                }
                Condition::Contains(needle.to_string())
            }
            CondKind::IsTrue => Condition::IsTrue,
            CondKind::IsFalse => Condition::IsFalse,
            CondKind::NonEmpty => Condition::NonEmpty,
            CondKind::IsEmpty => Condition::IsEmpty,
        };
        Some(Rule {
            condition,
            format: CellFormat {
                fill: Some(self.fill),
                bold: self.bold,
                italic: self.italic,
                strikethrough: self.strikethrough,
                underline: self.underline,
                text_color: self.text_color_on.then_some(self.text_color),
                ..CellFormat::default()
            },
        })
    }

    /// Load an existing [`Rule`] back into a draft — the inverse of
    /// [`CondDraft::build`], for the editor's "Edit" button.
    fn from_rule(rule: &Rule) -> Self {
        let defaults = CondDraft::default();
        let threshold = match &rule.condition {
            Condition::GreaterThan(t)
            | Condition::LessThan(t)
            | Condition::EqualTo(t)
            | Condition::NotEqualTo(t)
            | Condition::GreaterOrEqual(t)
            | Condition::LessOrEqual(t)
            | Condition::Between(t, _) => t.to_string(),
            Condition::Contains(s) => s.clone(),
            _ => defaults.threshold,
        };
        let threshold2 = match &rule.condition {
            Condition::Between(_, t) => t.to_string(),
            _ => defaults.threshold2,
        };
        CondDraft {
            kind: CondKind::from_condition(&rule.condition),
            threshold,
            threshold2,
            fill: rule.format.fill.unwrap_or(defaults.fill),
            bold: rule.format.bold,
            italic: rule.format.italic,
            strikethrough: rule.format.strikethrough,
            underline: rule.format.underline,
            text_color_on: rule.format.text_color.is_some(),
            text_color: rule.format.text_color.unwrap_or(defaults.text_color),
        }
    }
}

/// One cell's source before and after a change.
#[derive(Debug, Clone)]
struct CellEdit {
    sheet: SheetId,
    addr: String,
    before: Option<String>,
    after: Option<String>,
}

/// One square cell's format before and after a change.
#[derive(Debug, Clone)]
struct FormatEdit {
    cell: (u32, u32),
    before: CellFormat,
    after: CellFormat,
}

/// One hex cell's format before and after a change.
#[derive(Debug, Clone)]
struct HexFormatEdit {
    cell: HexCoord,
    before: CellFormat,
    after: CellFormat,
}

/// An undoable action — a batch of cell-content edits, or a batch of
/// square or hex formatting edits. One `Action` is one undo step.
#[derive(Debug, Clone)]
enum Action {
    Cells(Vec<CellEdit>),
    Formats(Vec<FormatEdit>),
    HexFormats(Vec<HexFormatEdit>),
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
    /// The square cursor as of the last frame — drives scroll-to-cursor.
    prev_cursor: (u32, u32),
    /// The selected hex range on the hex sheet.
    hex_selection: HexSelection,
    /// `Some` while a cell is being edited (on whichever sheet is active).
    edit: Option<EditState>,
    /// Per-column widths and per-row heights of the square sheet.
    metrics: GridMetrics,
    /// `Some` while a header border is being dragged.
    resizing: Option<Resize>,
    /// The pointer position when the user pressed the button — `Some`
    /// while a press is active. Captured BEFORE egui's drag threshold
    /// is crossed, so the resize hit-test reflects where the user
    /// clicked rather than the position the pointer has moved to by
    /// the time `drag_started` fires (which can be 5+ pixels off the
    /// border).
    press_pos: Option<egui::Pos2>,
    /// `Some` while a column or row header is being dragged to sweep a
    /// range selection.
    header_drag: Option<HeaderDrag>,
    /// `Some` while the format-painter is armed: the captured format
    /// that the next square-cell click will apply.
    format_painter: Option<CellFormat>,
    /// Per-cell visual formatting of the square sheet.
    formats: FormatMap<(u32, u32)>,
    /// Per-cell visual formatting of the hex sheet.
    hex_formats: FormatMap<HexCoord>,
    /// The copy / cut / paste object — the captured block, and its cut
    /// origin when it was cut rather than copied.
    clipboard: Clipboard,
    /// Undo/redo history — one entry per action.
    history: History<Action>,
    /// `ctx.input().time` for the current frame — the clock used to
    /// coalesce rapid format edits.
    frame_time: f64,
    /// When the last format action was recorded, and the cells it
    /// touched. A fresh format edit over the same cells within
    /// `COALESCE_WINDOW` merges into it — so a colour-picker drag is one
    /// undo step, not one per frame.
    last_format_time: f64,
    last_format_cells: Vec<(u32, u32)>,
    /// The hex-sheet counterparts, for hex format coalescing.
    last_hex_format_time: f64,
    last_hex_format_cells: Vec<HexCoord>,
    /// Conditional-formatting rules for the square sheet, evaluated each
    /// frame and layered over a cell's manual format.
    cond_rules: Vec<Rule>,
    /// Whether the conditional-formatting rule editor window is open.
    cond_window_open: bool,
    /// The in-progress new rule in that editor.
    cond_draft: CondDraft,
    /// Square-sheet cells that render as interactive boolean toggles.
    widgets: Widgets,
    /// The formula bar's edit buffer — mirrors the active cell's source
    /// except while the bar itself is being edited.
    formula_bar: String,
    /// Find-panel state — the query and its matching cells.
    find: FindState<CellId>,
    /// Whether the Find window is open.
    find_open: bool,
    /// Set when Find is opened by Ctrl+F so the query field grabs focus.
    find_just_opened: bool,
    /// Whether the keyboard-shortcuts help overlay is open.
    help_open: bool,
    /// Free-text notes on square-sheet cells.
    notes: NoteMap<(u32, u32)>,
    /// Free-text notes on hex-sheet cells.
    hex_notes: NoteMap<HexCoord>,
    /// Whether the cell-note editor window is open.
    note_open: bool,
    /// The cell the note editor is editing — on either sheet.
    note_cell: CellId,
    /// The note editor's text buffer.
    note_draft: String,
    /// The Name box's edit buffer — the active cell's address, editable
    /// to jump the selection (square sheet).
    name_box: String,
    /// Whether the dark colour theme is active (the default).
    dark_mode: bool,
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
            prev_cursor: (0, 0),
            hex_selection: HexSelection::single(HexCoord::new(0, 0)),
            edit: None,
            metrics: GridMetrics::new(),
            resizing: None,
            press_pos: None,
            header_drag: None,
            format_painter: None,
            formats: FormatMap::new(),
            hex_formats: {
                let mut m = FormatMap::new();
                // A seeded demo so hex formatting is visible at launch.
                m.update(HexCoord::new(1, 0), |f| {
                    f.fill = Some(egui::Color32::from_rgb(201, 237, 203));
                });
                m
            },
            clipboard: Clipboard::default(),
            history: History::new(),
            frame_time: 0.0,
            last_format_time: 0.0,
            last_format_cells: Vec::new(),
            last_hex_format_time: 0.0,
            last_hex_format_cells: Vec::new(),
            cond_rules: vec![Rule {
                condition: Condition::GreaterThan(1000.0),
                format: CellFormat {
                    fill: Some(egui::Color32::from_rgb(201, 237, 203)),
                    ..CellFormat::default()
                },
            }],
            cond_window_open: false,
            cond_draft: CondDraft::default(),
            widgets: {
                let mut w = Widgets::default();
                w.set_toggle((3, 1), true);
                w
            },
            formula_bar: String::new(),
            find: FindState::default(),
            find_open: false,
            find_just_opened: false,
            help_open: false,
            notes: NoteMap::new(),
            hex_notes: NoteMap::new(),
            note_open: false,
            note_cell: CellId::Square((0, 0)),
            note_draft: String::new(),
            name_box: String::new(),
            dark_mode: true,
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

    /// The raw evaluated value of a square-sheet cell — used by
    /// conditional formatting. `Empty` when the cell holds nothing.
    fn cell_value(&self, col: u32, row: u32) -> CellValue {
        self.engine
            .get_cell(self.square_sheet, &grid::cell_address(col, row))
            .map(|snapshot| snapshot.value)
            .unwrap_or_default()
    }

    /// The raw evaluated value of a hex-sheet cell, for conditional
    /// formatting. `Empty` when the cell holds nothing.
    fn hex_cell_value(&self, coord: HexCoord) -> CellValue {
        self.engine
            .get_cell(self.hex_sheet, &hex_address(coord))
            .map(|snapshot| snapshot.value)
            .unwrap_or_default()
    }

    /// The effective format for a hex cell — its stored format layered
    /// with the first matching conditional-formatting rule.
    fn hex_effective_format(&self, coord: HexCoord) -> CellFormat {
        let base = self.hex_formats.get(coord);
        if self.cond_rules.is_empty() {
            base
        } else {
            conditional::effective_format(&base, &self.hex_cell_value(coord), &self.cond_rules)
        }
    }

    /// The raw source text of a square-sheet cell — empty when the cell
    /// holds nothing. Find and Replace both work on this.
    fn cell_source(&self, col: u32, row: u32) -> String {
        self.engine
            .get_cell(self.square_sheet, &grid::cell_address(col, row))
            .and_then(|snapshot| snapshot.source)
            .unwrap_or_default()
    }

    /// The raw source text of a hex-sheet cell — empty when it holds
    /// nothing. Find and Replace use it on the hex sheet.
    fn hex_cell_source(&self, coord: HexCoord) -> String {
        self.engine
            .get_cell(self.hex_sheet, &hex_address(coord))
            .and_then(|snapshot| snapshot.source)
            .unwrap_or_default()
    }

    /// The display text for a hex-sheet cell. (The number format is not
    /// yet applied on the hex sheet — that is a follow-on.)
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
                .get_cell(self.hex_sheet, &hex_address(self.hex_selection.cursor))
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
            ActiveSheet::Hex => (self.hex_sheet, hex_address(self.hex_selection.cursor)),
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
            ActiveSheet::Hex => hex_address(self.hex_selection.cursor),
        };
        let source = match &self.edit {
            Some(edit) => edit.buffer.clone(),
            None => self.active_cell_source(),
        };
        (addr, source)
    }

    /// The engine values of every cell in the active selection — what the
    /// status bar aggregates.
    fn selection_values(&self) -> Vec<CellValue> {
        match self.active {
            ActiveSheet::Square => self
                .selection
                .cells()
                .map(|(c, r)| {
                    self.engine
                        .get_cell(self.square_sheet, &grid::cell_address(c, r))
                        .map(|s| s.value)
                        .unwrap_or(CellValue::Empty)
                })
                .collect(),
            ActiveSheet::Hex => self
                .hex_selection
                .cells()
                .into_iter()
                .map(|c| {
                    self.engine
                        .get_cell(self.hex_sheet, &hex_address(c))
                        .map(|s| s.value)
                        .unwrap_or(CellValue::Empty)
                })
                .collect(),
        }
    }

    /// Read key events and turn them into commands through `keymap`. Keys
    /// that map to a command are consumed so no other widget reacts too.
    fn collect_commands(&self, ctx: &egui::Context) -> Vec<Command> {
        // A focused text input that is not the in-cell editor — the
        // formula bar, or a dialog field — owns the keyboard; don't also
        // read its keystrokes as cell commands.
        if self.edit.is_none() && ctx.wants_keyboard_input() {
            return Vec::new();
        }
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
            Command::SelectAll => self.select_all(),
            Command::SelectRegion => self.select_region(),
            Command::Jump(dir) => self.jump_active(dir),
            Command::JumpExtend(dir) => self.jump_extend_active(dir),
            Command::MoveToRowStart => match self.active {
                ActiveSheet::Square => {
                    let row = self.selection.cursor.1;
                    self.selection.collapse_to((0, row));
                }
                ActiveSheet::Hex => self.hex_selection.collapse_to(HexCoord::new(0, 0)),
            },
            Command::MoveToOrigin => match self.active {
                ActiveSheet::Square => self.selection.collapse_to((0, 0)),
                ActiveSheet::Hex => self.hex_selection.collapse_to(HexCoord::new(0, 0)),
            },
            Command::MoveToRowEnd => match self.active {
                ActiveSheet::Square => {
                    let row = self.selection.cursor.1;
                    self.selection.collapse_to((COLS - 1, row));
                }
                ActiveSheet::Hex => self.hex_selection.collapse_to(HexCoord::new(0, 0)),
            },
            Command::MoveToSheetEnd => match self.active {
                ActiveSheet::Square => self.selection.collapse_to((COLS - 1, ROWS - 1)),
                ActiveSheet::Hex => self.hex_selection.collapse_to(HexCoord::new(0, 0)),
            },
            Command::PageUp => self.page(true),
            Command::PageDown => self.page(false),
            Command::BeginEdit { replace_with } => self.begin_edit(replace_with),
            Command::Commit(dir) => {
                self.commit_edit();
                self.move_active(dir);
            }
            Command::Cancel => self.edit = None,
            Command::ClearMarquee => {
                if self.clipboard.cut_origin().is_some() {
                    self.clipboard.consume_cut();
                }
                self.format_painter = None;
            }
            Command::Clear => self.clear_active(),
            Command::ToggleBold => self.toggle_range(|f| f.bold, |f, v| f.bold = v),
            Command::ToggleItalic => self.toggle_range(|f| f.italic, |f, v| f.italic = v),
            Command::ToggleUnderline => self.toggle_range(|f| f.underline, |f, v| f.underline = v),
            Command::SetAlign(align) => self.format_range(|f| f.align = align),
            Command::Copy => self.copy_selection(ctx),
            Command::Cut => self.cut_selection(ctx),
            Command::Paste => self.paste(PasteMode::Normal),
            Command::PasteValues => self.paste(PasteMode::ValuesOnly),
            Command::Undo => self.undo(),
            Command::Redo => self.redo(),
            Command::FillDown => self.fill(FillDir::Down),
            Command::FillRight => self.fill(FillDir::Right),
            Command::OpenFind => {
                self.find_open = true;
                self.find_just_opened = true;
            }
            Command::FindNext => self.find_step(true),
            Command::FindPrev => self.find_step(false),
            Command::OpenHelp => self.help_open = true,
        }
    }

    /// Select every cell of the active sheet — Ctrl+A, or a click on the
    /// header corner. The hex sheet has no fixed bounds, so it selects
    /// the parallelogram bounding the visible disc.
    fn select_all(&mut self) {
        match self.active {
            ActiveSheet::Square => self.selection = Selection::all(COLS, ROWS),
            ActiveSheet::Hex => {
                let r = HEX_VIEW_RADIUS;
                self.hex_selection = HexSelection {
                    anchor: HexCoord::new(-r, -r),
                    cursor: HexCoord::new(r, r),
                };
            }
        }
    }

    /// Expand the selection to the current data region around the cursor
    /// — the contiguous block of non-empty cells, Excel's "current
    /// region". The active cell lands at the region's top-left.
    fn select_region(&mut self) {
        match self.active {
            ActiveSheet::Square => {
                let ((min_c, min_r), (max_c, max_r)) =
                    grid::current_region(self.selection.cursor, COLS, ROWS, |c, r| {
                        self.square_occupied(c, r)
                    });
                self.selection = Selection {
                    anchor: (max_c, max_r),
                    cursor: (min_c, min_r),
                };
            }
            ActiveSheet::Hex => {
                let cursor = self.hex_selection.cursor;
                let ((min_q, min_r), (max_q, max_r)) =
                    hex_current_region((cursor.q, cursor.r), HEX_VIEW_RADIUS, |q, r| {
                        let c = HexCoord::new(q, r);
                        hex_in_view(c) && self.hex_occupied(c)
                    });
                self.hex_selection = HexSelection {
                    anchor: HexCoord::new(max_q, max_r),
                    cursor: HexCoord::new(min_q, min_r),
                };
            }
        }
    }

    /// Apply a formatting action from the ribbon across the selection.
    fn apply_ribbon(&mut self, action: RibbonAction, ctx: &egui::Context) {
        match action {
            RibbonAction::ToggleBold => self.toggle_range(|f| f.bold, |f, v| f.bold = v),
            RibbonAction::ToggleItalic => self.toggle_range(|f| f.italic, |f, v| f.italic = v),
            RibbonAction::ToggleStrikethrough => {
                self.toggle_range(|f| f.strikethrough, |f, v| f.strikethrough = v)
            }
            RibbonAction::ToggleUnderline => {
                self.toggle_range(|f| f.underline, |f, v| f.underline = v)
            }
            RibbonAction::SetAlign(align) => self.format_range(|f| f.align = align),
            RibbonAction::SetVAlign(valign) => self.format_range(|f| f.valign = valign),
            RibbonAction::SetFontSize(size) => self.format_range(|f| f.font_size = size),
            RibbonAction::SetNumber(number) => self.format_range(|f| f.number = number),
            RibbonAction::AdjustDecimals(delta) => {
                self.format_range(|f| f.number = format::adjust_decimals(f.number, delta))
            }
            RibbonAction::SetTextColor(color) => self.format_range(|f| f.text_color = color),
            RibbonAction::SetFill(fill) => self.format_range(|f| f.fill = fill),
            RibbonAction::ClearFormat => self.format_range(|f| *f = CellFormat::default()),
            RibbonAction::Undo => self.undo(),
            RibbonAction::Redo => self.redo(),
            RibbonAction::Copy => self.copy_selection(ctx),
            RibbonAction::Cut => self.cut_selection(ctx),
            RibbonAction::Paste => self.paste(PasteMode::Normal),
            RibbonAction::PasteValues => self.paste(PasteMode::ValuesOnly),
            RibbonAction::OpenConditional => self.cond_window_open = true,
            RibbonAction::ToggleWidget => self.toggle_widget_cells(),
            RibbonAction::Aggregate(func) => self.autosum(func),
            RibbonAction::SetBorders(mode) => self.apply_border(mode),
            RibbonAction::ToggleNegativeRed => {
                self.toggle_range(|f| f.negative_red, |f, v| f.negative_red = v);
            }
            RibbonAction::ToggleWrapText => {
                self.toggle_range(|f| f.wrap_text, |f, v| f.wrap_text = v);
            }
            RibbonAction::ToggleFormatPainter => {
                if self.format_painter.is_some() {
                    self.format_painter = None;
                } else {
                    self.format_painter = Some(self.formats.get(self.selection.cursor));
                }
            }
            RibbonAction::OpenHelp => self.help_open = true,
            RibbonAction::ToggleTheme => self.dark_mode = !self.dark_mode,
        }
    }

    /// Turn the selected square-sheet cells into boolean checkbox cells,
    /// or back into ordinary cells if they all already are. A no-op on
    /// the hex sheet, which has no widgets yet.
    fn toggle_widget_cells(&mut self) {
        if self.active != ActiveSheet::Square {
            return;
        }
        let cells: Vec<(u32, u32)> = self.selection.cells().collect();
        let all_on = cells.iter().all(|&c| self.widgets.is_toggle(c));
        for cell in cells {
            self.widgets.set_toggle(cell, !all_on);
        }
    }

    /// AutoSum — write `=FUNC(...)` of the selection (`func` is an
    /// aggregate like `"SUM"` or `"AVERAGE"`) into the cell directly
    /// below it, then move there. A no-op when that cell would fall off
    /// the sheet — past the last row, or outside the hex disc.
    fn autosum(&mut self, func: &str) {
        match self.active {
            ActiveSheet::Square => {
                let Some((target, formula)) = grid::autosum(self.selection.bounds(), ROWS, func)
                else {
                    return;
                };
                self.commit_edit();
                let addr = grid::cell_address(target.0, target.1);
                self.apply_edits(self.square_sheet, vec![(addr, Some(formula))]);
                self.selection.collapse_to(target);
            }
            ActiveSheet::Hex => {
                let Some((target, formula)) = hex_autosum(self.hex_selection.bounds(), func) else {
                    return;
                };
                self.commit_edit();
                self.apply_edits(self.hex_sheet, vec![(hex_address(target), Some(formula))]);
                self.hex_selection.collapse_to(target);
            }
        }
    }

    /// Fill series — fill each selected column with an arithmetic
    /// progression extrapolated from the numbers already at the top of
    /// that column (a q-line, down the r-axis, on the hex sheet). A
    /// column with no numeric seed is left alone. One undo step.
    fn fill_series(&mut self) {
        match self.active {
            ActiveSheet::Square => {
                let ((min_c, min_r), (max_c, max_r)) = self.selection.bounds();
                let total = (max_r - min_r + 1) as usize;
                let mut targets = Vec::new();
                for c in min_c..=max_c {
                    let mut seed = Vec::new();
                    for r in min_r..=max_r {
                        match self.cell_value(c, r) {
                            CellValue::Number(n) => seed.push(n),
                            CellValue::Integer(i) => seed.push(i as f64),
                            _ => break,
                        }
                    }
                    if seed.is_empty() {
                        continue;
                    }
                    for (i, v) in grid::series_fill(&seed, total).into_iter().enumerate() {
                        let r = min_r + i as u32;
                        targets.push((grid::cell_address(c, r), Some(v.to_string())));
                    }
                }
                if !targets.is_empty() {
                    self.commit_edit();
                    self.apply_edits(self.square_sheet, targets);
                }
            }
            ActiveSheet::Hex => {
                let ((min_q, min_r), (max_q, max_r)) = self.hex_selection.bounds();
                let total = (max_r - min_r + 1) as usize;
                let mut targets = Vec::new();
                for q in min_q..=max_q {
                    let mut seed = Vec::new();
                    for r in min_r..=max_r {
                        match self.hex_cell_value(HexCoord::new(q, r)) {
                            CellValue::Number(n) => seed.push(n),
                            CellValue::Integer(i) => seed.push(i as f64),
                            _ => break,
                        }
                    }
                    if seed.is_empty() {
                        continue;
                    }
                    for (i, v) in grid::series_fill(&seed, total).into_iter().enumerate() {
                        let r = min_r + i as i32;
                        targets.push((hex_address(HexCoord::new(q, r)), Some(v.to_string())));
                    }
                }
                if !targets.is_empty() {
                    self.commit_edit();
                    self.apply_edits(self.hex_sheet, targets);
                }
            }
        }
    }

    /// Sort the selected block by the cursor's column, whole rows moving
    /// together — ascending, or descending when `ascending` is false. A
    /// stable sort: rows with equal keys keep their order. Cell sources
    /// move with their values. One undo step. On the hex sheet the
    /// cursor's q-line is the key, the rows running down the r-axis.
    fn sort_selection(&mut self, ascending: bool) {
        match self.active {
            ActiveSheet::Square => {
                let ((min_c, min_r), (max_c, max_r)) = self.selection.bounds();
                let key_col = self.selection.cursor.0;
                let keys: Vec<CellValue> = (min_r..=max_r)
                    .map(|r| self.cell_value(key_col, r))
                    .collect();
                let order = sort::row_order(&keys, ascending);
                let mut targets: Vec<(String, Option<String>)> = Vec::new();
                for c in min_c..=max_c {
                    let sources: Vec<String> =
                        (min_r..=max_r).map(|r| self.cell_source(c, r)).collect();
                    for (i, &src_row) in order.iter().enumerate() {
                        let r = min_r + i as u32;
                        let source = sources[src_row].clone();
                        let cell = (!source.is_empty()).then_some(source);
                        targets.push((grid::cell_address(c, r), cell));
                    }
                }
                self.commit_edit();
                self.apply_edits(self.square_sheet, targets);
            }
            ActiveSheet::Hex => {
                let ((min_q, min_r), (max_q, max_r)) = self.hex_selection.bounds();
                let key_q = self.hex_selection.cursor.q;
                let keys: Vec<CellValue> = (min_r..=max_r)
                    .map(|r| self.hex_cell_value(HexCoord::new(key_q, r)))
                    .collect();
                let order = sort::row_order(&keys, ascending);
                let mut targets: Vec<(String, Option<String>)> = Vec::new();
                for q in min_q..=max_q {
                    let sources: Vec<String> = (min_r..=max_r)
                        .map(|r| self.hex_cell_source(HexCoord::new(q, r)))
                        .collect();
                    for (i, &src_row) in order.iter().enumerate() {
                        let r = min_r + i as i32;
                        let source = sources[src_row].clone();
                        let cell = (!source.is_empty()).then_some(source);
                        targets.push((hex_address(HexCoord::new(q, r)), cell));
                    }
                }
                self.commit_edit();
                self.apply_edits(self.hex_sheet, targets);
            }
        }
    }

    /// Apply a border mode across the selection — one undo step. Both
    /// sheets resolve borders per cell: the square sheet via
    /// `border_sides`, the hex sheet via `apply_hex_border`.
    fn apply_border(&mut self, mode: BorderMode) {
        match self.active {
            ActiveSheet::Square => self.apply_square_border(mode),
            ActiveSheet::Hex => self.apply_hex_border(mode),
        }
    }

    /// The square-sheet border apply — `border_sides` per cell.
    fn apply_square_border(&mut self, mode: BorderMode) {
        let bounds = self.selection.bounds();
        let cells: Vec<(u32, u32)> = self.selection.cells().collect();
        let mut edits = Vec::new();
        for cell in cells {
            let before = self.formats.get(cell);
            let mut after = before.clone();
            after.borders = format::border_sides(cell, bounds, mode);
            if before == after {
                continue;
            }
            self.formats.update(cell, |f| *f = after.clone());
            edits.push(FormatEdit {
                cell,
                before,
                after,
            });
        }
        if !edits.is_empty() {
            self.history.record(Action::Formats(edits));
        }
    }

    /// The hex-sheet border apply. `All` and `None` set every cell's
    /// edges uniformly; `Outer` resolves each cell's perimeter via
    /// `hex_outer_borders` — an edge is bordered only when the hex
    /// across it is not itself selected.
    fn apply_hex_border(&mut self, mode: BorderMode) {
        let selection = self.hex_selection;
        let mut edits = Vec::new();
        for cell in selection.cells() {
            let before = self.hex_formats.get(cell);
            let mut after = before.clone();
            after.hex_borders = match mode {
                BorderMode::None => HexBorders::default(),
                BorderMode::All => HexBorders::all(),
                BorderMode::Outer => format::hex_outer_borders(cell, |c| selection.contains(c)),
            };
            if before == after {
                continue;
            }
            self.hex_formats.update(cell, |f| *f = after.clone());
            edits.push(HexFormatEdit {
                cell,
                before,
                after,
            });
        }
        if !edits.is_empty() {
            self.history.record(Action::HexFormats(edits));
        }
    }

    /// The conditional-formatting rule editor — a floating window listing
    /// the square sheet's rules, with a row to add another.
    fn conditional_window(&mut self, ctx: &egui::Context) {
        let mut open = self.cond_window_open;
        egui::Window::new("Conditional formatting")
            .open(&mut open)
            .resizable(false)
            .show(ctx, |ui| {
                if self.cond_rules.is_empty() {
                    ui.label(egui::RichText::new("No rules yet — add one below.").weak());
                }
                let count = self.cond_rules.len();
                let mut remove = None;
                let mut edit = None;
                let mut raise = None;
                let mut lower = None;
                for (i, rule) in self.cond_rules.iter().enumerate() {
                    ui.horizontal(|ui| {
                        if let Some(fill) = rule.format.fill {
                            let (rect, _) = ui
                                .allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
                            ui.painter().rect_filled(rect, 2.0, fill);
                        }
                        // Render the description in the rule's own style,
                        // so the row previews what a match looks like.
                        let mut desc = egui::RichText::new(describe_condition(&rule.condition));
                        if rule.format.bold {
                            desc = desc.strong();
                        }
                        if rule.format.italic {
                            desc = desc.italics();
                        }
                        if rule.format.strikethrough {
                            desc = desc.strikethrough();
                        }
                        if rule.format.underline {
                            desc = desc.underline();
                        }
                        if let Some(color) = rule.format.text_color {
                            desc = desc.color(color);
                        }
                        ui.label(desc);
                        if ui
                            .add_enabled(i > 0, egui::Button::new("↑").small())
                            .clicked()
                        {
                            raise = Some(i);
                        }
                        if ui
                            .add_enabled(i + 1 < count, egui::Button::new("↓").small())
                            .clicked()
                        {
                            lower = Some(i);
                        }
                        if ui.small_button("Edit").clicked() {
                            edit = Some(i);
                        }
                        if ui.small_button("Remove").clicked() {
                            remove = Some(i);
                        }
                    });
                }
                // Editing a rule loads it back into the draft and drops it,
                // so the "Add rule" button re-commits the edited form.
                if let Some(i) = edit {
                    self.cond_draft = CondDraft::from_rule(&self.cond_rules[i]);
                    self.cond_rules.remove(i);
                } else if let Some(i) = remove {
                    self.cond_rules.remove(i);
                } else if let Some(i) = raise {
                    if let Some((a, b)) = conditional::swap_for_move(count, i, true) {
                        self.cond_rules.swap(a, b);
                    }
                } else if let Some(i) = lower {
                    if let Some((a, b)) = conditional::swap_for_move(count, i, false) {
                        self.cond_rules.swap(a, b);
                    }
                }
                ui.separator();
                ui.horizontal(|ui| {
                    egui::ComboBox::from_label("when value")
                        .selected_text(self.cond_draft.kind.label())
                        .show_ui(ui, |ui| {
                            for kind in CondKind::ALL {
                                ui.selectable_value(&mut self.cond_draft.kind, kind, kind.label());
                            }
                        });
                    if self.cond_draft.kind.needs_threshold() {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.cond_draft.threshold)
                                .desired_width(56.0),
                        );
                    }
                    if self.cond_draft.kind.needs_threshold2() {
                        ui.label("and");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.cond_draft.threshold2)
                                .desired_width(56.0),
                        );
                    }
                    ui.label("fill");
                    ui.color_edit_button_srgba(&mut self.cond_draft.fill);
                    ui.checkbox(&mut self.cond_draft.bold, "bold");
                    ui.checkbox(&mut self.cond_draft.italic, "italic");
                    ui.checkbox(&mut self.cond_draft.strikethrough, "strike");
                    ui.checkbox(&mut self.cond_draft.underline, "underline");
                    ui.checkbox(&mut self.cond_draft.text_color_on, "text");
                    ui.color_edit_button_srgba(&mut self.cond_draft.text_color);
                    if ui.button("Add rule").clicked() {
                        if let Some(rule) = self.cond_draft.build() {
                            self.cond_rules.push(rule);
                        }
                    }
                });
            });
        self.cond_window_open = open;
    }

    /// The Find panel — a floating window with a query box, prev/next,
    /// and a match count. Ctrl+F opens it.
    fn find_window(&mut self, ctx: &egui::Context) {
        if !self.find_open {
            return;
        }
        let mut open = self.find_open;
        egui::Window::new("Find")
            .open(&mut open)
            .resizable(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut self.find.query)
                            .desired_width(180.0)
                            .hint_text("find in cells"),
                    );
                    if self.find_just_opened {
                        response.request_focus();
                        self.find_just_opened = false;
                    }
                    if response.changed() {
                        self.refresh_find();
                    }
                    if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        response.request_focus();
                        self.find_step(true);
                    }
                    if ui
                        .checkbox(&mut self.find.case_sensitive, "Match case")
                        .changed()
                    {
                        self.refresh_find();
                    }
                    if ui
                        .checkbox(&mut self.find.whole_cell, "Whole cell")
                        .changed()
                    {
                        self.refresh_find();
                    }
                    if ui
                        .checkbox(&mut self.find.in_selection, "Within selection")
                        .changed()
                    {
                        self.refresh_find();
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Replace");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.find.replace)
                            .desired_width(180.0)
                            .hint_text("replace with"),
                    );
                });
                ui.horizontal(|ui| {
                    if ui.button("◀ Prev").clicked() {
                        self.find_step(false);
                    }
                    if ui.button("Next ▶").clicked() {
                        self.find_step(true);
                    }
                    let count = self.find.match_count();
                    let label = if count > 0 {
                        format!("{} of {}", self.find.current_index(), count)
                    } else if self.find.query.is_empty() {
                        String::new()
                    } else {
                        "no matches".to_string()
                    };
                    ui.label(egui::RichText::new(label).weak());
                });
                ui.horizontal(|ui| {
                    let has_match = self.find.match_count() > 0;
                    if ui
                        .add_enabled(has_match, egui::Button::new("Replace"))
                        .clicked()
                    {
                        self.replace_current();
                    }
                    if ui
                        .add_enabled(has_match, egui::Button::new("Replace All"))
                        .clicked()
                    {
                        self.replace_all_matches();
                    }
                });
            });
        let was_open = self.find_open;
        self.find_open = open;
        // Closing Find drops the query so its grid highlights clear.
        if was_open && !self.find_open {
            self.find.clear();
        }
    }

    /// The sheet, address and current source of a Find match.
    fn cell_id_target(&self, id: CellId) -> (SheetId, String, String) {
        match id {
            CellId::Square((c, r)) => (
                self.square_sheet,
                grid::cell_address(c, r),
                self.cell_source(c, r),
            ),
            CellId::Hex(h) => (self.hex_sheet, hex_address(h), self.hex_cell_source(h)),
        }
    }

    /// Move the active sheet's selection onto a Find match.
    fn jump_to(&mut self, id: CellId) {
        match id {
            CellId::Square(cell) => self.selection.collapse_to(cell),
            CellId::Hex(coord) => self.hex_selection.collapse_to(coord),
        }
    }

    /// Rebuild the Find matches from the active sheet's current contents
    /// and jump the selection to the first match.
    fn refresh_find(&mut self) {
        let in_selection = self.find.in_selection;
        let cells: Vec<(CellId, String)> = match self.active {
            ActiveSheet::Square => (0..ROWS)
                .flat_map(|r| (0..COLS).map(move |c| (c, r)))
                .filter(|&(c, r)| !in_selection || self.selection.contains((c, r)))
                .map(|(c, r)| (CellId::Square((c, r)), self.cell_source(c, r)))
                .collect(),
            ActiveSheet::Hex => hex::hex_disc(HexCoord::new(0, 0), HEX_VIEW_RADIUS)
                .into_iter()
                .filter(|&coord| !in_selection || self.hex_selection.contains(coord))
                .map(|coord| (CellId::Hex(coord), self.hex_cell_source(coord)))
                .collect(),
        };
        self.find.refresh(cells.into_iter());
        if let Some(id) = self.find.current_match() {
            self.jump_to(id);
        }
    }

    /// Step Find to the next/previous match and move the selection to it.
    fn find_step(&mut self, forward: bool) {
        if let Some(id) = self.find.step(forward) {
            self.jump_to(id);
        }
    }

    /// The keyboard-shortcuts help overlay — a floating window listing
    /// `keymap::SHORTCUTS`. Opened by F1 or the ribbon's "?" button.
    fn help_window(&mut self, ctx: &egui::Context) {
        if !self.help_open {
            return;
        }
        let mut open = self.help_open;
        egui::Window::new("Help")
            .open(&mut open)
            .resizable(false)
            .show(ctx, |ui| {
                ui.heading("Keyboard");
                egui::Grid::new("shortcuts_grid")
                    .striped(true)
                    .show(ui, |ui| {
                        for &(keys, desc) in keymap::SHORTCUTS {
                            ui.monospace(keys);
                            ui.label(desc);
                            ui.end_row();
                        }
                    });
                ui.add_space(8.0);
                ui.heading("Mouse");
                egui::Grid::new("gestures_grid")
                    .striped(true)
                    .show(ui, |ui| {
                        for &(gesture, desc) in keymap::GESTURES {
                            ui.monospace(gesture);
                            ui.label(desc);
                            ui.end_row();
                        }
                    });
            });
        self.help_open = open;
    }

    /// The cell-note editor — a small window with a multiline field for
    /// the note on `note_cell`, opened from the right-click menu.
    fn note_window(&mut self, ctx: &egui::Context) {
        if !self.note_open {
            return;
        }
        let mut open = self.note_open;
        let cell = self.note_cell;
        let address = match cell {
            CellId::Square((c, r)) => grid::cell_address(c, r),
            CellId::Hex(h) => hex_address(h),
        };
        egui::Window::new("Cell note")
            .open(&mut open)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(format!("Note for {address}"));
                ui.add(
                    egui::TextEdit::multiline(&mut self.note_draft)
                        .desired_rows(4)
                        .desired_width(260.0),
                );
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        self.set_note(cell, self.note_draft.clone());
                        self.note_open = false;
                    }
                    if ui.button("Clear").clicked() {
                        self.set_note(cell, String::new());
                        self.note_open = false;
                    }
                });
            });
        // The window's [x] clears `open`; a Save/Clear button may have
        // already closed it.
        self.note_open &= open;
    }

    /// Write a note to whichever sheet `cell` belongs to. Blank text
    /// clears the note (see [`NoteMap::set`]).
    fn set_note(&mut self, cell: CellId, text: String) {
        match cell {
            CellId::Square(c) => self.notes.set(c, text),
            CellId::Hex(h) => self.hex_notes.set(h, text),
        }
    }

    /// Replace the query with the replacement in the current match's
    /// source, write it back, then rebuild the match list.
    fn replace_current(&mut self) {
        if self.find.query.is_empty() {
            return;
        }
        if let Some(id) = self.find.current_match() {
            let (sheet, addr, source) = self.cell_id_target(id);
            let new = find::replace_all(
                &source,
                &self.find.query,
                &self.find.replace,
                self.find.case_sensitive,
            );
            if new != source {
                self.apply_edits(sheet, vec![(addr, commit_source(&new))]);
            }
            self.refresh_find();
        }
    }

    /// Replace the query with the replacement in every matching cell's
    /// source — one undo step — then rebuild the match list.
    fn replace_all_matches(&mut self) {
        if self.find.query.is_empty() {
            return;
        }
        let query = self.find.query.clone();
        let replacement = self.find.replace.clone();
        let case_sensitive = self.find.case_sensitive;
        let matches: Vec<CellId> = self.find.matches().to_vec();
        let mut sheet = self.square_sheet;
        let mut targets = Vec::new();
        for id in matches {
            let (s, addr, source) = self.cell_id_target(id);
            sheet = s;
            let new = find::replace_all(&source, &query, &replacement, case_sensitive);
            if new != source {
                targets.push((addr, commit_source(&new)));
            }
        }
        self.apply_edits(sheet, targets);
        self.refresh_find();
    }

    /// Apply a formatting change to every cell of the active sheet's
    /// selection, recorded as one undo step. Cells the change leaves
    /// untouched are dropped.
    fn format_range(&mut self, edit: impl Fn(&mut CellFormat)) {
        match self.active {
            ActiveSheet::Square => self.format_square_range(edit),
            ActiveSheet::Hex => self.format_hex_range(edit),
        }
    }

    /// The square-sheet format apply, with edit-coalescing — a rapid run
    /// of same-cell format edits (a colour-picker drag) collapses into
    /// one undo step.
    fn format_square_range(&mut self, edit: impl Fn(&mut CellFormat)) {
        let cells: Vec<(u32, u32)> = self.selection.cells().collect();
        let mut edits = Vec::new();
        for cell in cells {
            let before = self.formats.get(cell);
            let mut after = before.clone();
            edit(&mut after);
            if before == after {
                continue;
            }
            self.formats.update(cell, |f| *f = after.clone());
            edits.push(FormatEdit {
                cell,
                before,
                after,
            });
        }
        if edits.is_empty() {
            return;
        }
        let touched: Vec<(u32, u32)> = edits.iter().map(|e| e.cell).collect();
        // Coalesce with the previous format action when it touched the
        // same cells within COALESCE_WINDOW — a colour-picker drag fires
        // a change every frame, and the whole drag should undo at once.
        let recent = self.frame_time - self.last_format_time < COALESCE_WINDOW;
        if recent && self.last_format_cells == touched {
            match self.history.pop_undo() {
                Some(Action::Formats(prev)) => {
                    let merged = merge_format_edits(prev, edits);
                    self.history.record(Action::Formats(merged));
                }
                Some(other) => {
                    // The top wasn't a format action — restore it.
                    self.history.record(other);
                    self.history.record(Action::Formats(edits));
                }
                None => self.history.record(Action::Formats(edits)),
            }
        } else {
            self.history.record(Action::Formats(edits));
        }
        self.last_format_time = self.frame_time;
        self.last_format_cells = touched;
    }

    /// The hex-sheet format apply, recorded as one undo step. A rapid
    /// run of same-cell edits (a colour drag) coalesces into one step.
    fn format_hex_range(&mut self, edit: impl Fn(&mut CellFormat)) {
        let mut edits = Vec::new();
        for cell in self.hex_selection.cells() {
            let before = self.hex_formats.get(cell);
            let mut after = before.clone();
            edit(&mut after);
            if before == after {
                continue;
            }
            self.hex_formats.update(cell, |f| *f = after.clone());
            edits.push(HexFormatEdit {
                cell,
                before,
                after,
            });
        }
        if edits.is_empty() {
            return;
        }
        let touched: Vec<HexCoord> = edits.iter().map(|e| e.cell).collect();
        // Coalesce a rapid run of same-cell hex format edits (a colour
        // drag) into one undo step, mirroring the square sheet.
        let recent = self.frame_time - self.last_hex_format_time < COALESCE_WINDOW;
        if recent && self.last_hex_format_cells == touched {
            match self.history.pop_undo() {
                Some(Action::HexFormats(prev)) => {
                    let merged = merge_hex_format_edits(prev, edits);
                    self.history.record(Action::HexFormats(merged));
                }
                Some(other) => {
                    // The top wasn't a hex format action — restore it.
                    self.history.record(other);
                    self.history.record(Action::HexFormats(edits));
                }
                None => self.history.record(Action::HexFormats(edits)),
            }
        } else {
            self.history.record(Action::HexFormats(edits));
        }
        self.last_hex_format_time = self.frame_time;
        self.last_hex_format_cells = touched;
    }

    /// Toggle a boolean format flag across the selection — Excel's rule:
    /// if every cell already has the flag, clear it for all; otherwise
    /// set it for all. Works on whichever sheet is active.
    fn toggle_range(
        &mut self,
        get: impl Fn(&CellFormat) -> bool,
        set: impl Fn(&mut CellFormat, bool),
    ) {
        let target = match self.active {
            ActiveSheet::Square => {
                let cells: Vec<(u32, u32)> = self.selection.cells().collect();
                toggle_target(cells.iter().map(|&c| get(&self.formats.get(c))))
            }
            ActiveSheet::Hex => {
                let cells = self.hex_selection.cells();
                toggle_target(cells.iter().map(|&c| get(&self.hex_formats.get(c))))
            }
        };
        self.format_range(|f| set(f, target));
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

    /// Page Up / Page Down — move the cursor a page. The square sheet
    /// shifts by `PAGE_ROWS` rows, clamped; the hex sheet walks to the
    /// r-extreme of the view along the cursor's q-line.
    fn page(&mut self, up: bool) {
        match self.active {
            ActiveSheet::Square => {
                let (c, r) = self.selection.cursor;
                let row = grid::page_step(r, up, PAGE_ROWS, ROWS - 1);
                self.selection.collapse_to((c, row));
            }
            ActiveSheet::Hex => {
                let dir = if up { Dir::Up } else { Dir::Down };
                let next = hex_jump(self.hex_selection.cursor, dir, hex_in_view, |_| false);
                self.hex_selection.collapse_to(next);
            }
        }
    }

    /// The square-sheet cell a Ctrl+arrow jump lands on from the cursor —
    /// the block edge along `dir`'s row or column, via `grid::jump_target`.
    fn square_jump(&self, dir: Dir) -> (u32, u32) {
        let (c, r) = self.selection.cursor;
        match dir {
            Dir::Left => (
                grid::jump_target(c, COLS - 1, false, |i| self.square_occupied(i, r)),
                r,
            ),
            Dir::Right => (
                grid::jump_target(c, COLS - 1, true, |i| self.square_occupied(i, r)),
                r,
            ),
            Dir::Up => (
                c,
                grid::jump_target(r, ROWS - 1, false, |i| self.square_occupied(c, i)),
            ),
            Dir::Down => (
                c,
                grid::jump_target(r, ROWS - 1, true, |i| self.square_occupied(c, i)),
            ),
        }
    }

    /// Jump the cursor to the data edge — Ctrl+arrow, on either sheet.
    fn jump_active(&mut self, dir: Dir) {
        match self.active {
            ActiveSheet::Square => {
                let next = self.square_jump(dir);
                self.selection.collapse_to(next);
            }
            ActiveSheet::Hex => {
                let next = hex_jump(self.hex_selection.cursor, dir, hex_in_view, |c| {
                    self.hex_occupied(c)
                });
                self.hex_selection.collapse_to(next);
            }
        }
    }

    /// Extend the selection to the data edge — Ctrl+Shift+arrow, on
    /// either sheet. Keeps the anchor and jumps the cursor.
    fn jump_extend_active(&mut self, dir: Dir) {
        match self.active {
            ActiveSheet::Square => {
                let next = self.square_jump(dir);
                self.selection.extend_to(next);
            }
            ActiveSheet::Hex => {
                let next = hex_jump(self.hex_selection.cursor, dir, hex_in_view, |c| {
                    self.hex_occupied(c)
                });
                self.hex_selection.extend_to(next);
            }
        }
    }

    /// Whether a square-sheet cell holds content — the predicate
    /// `grid::jump_target` scans for Ctrl+arrow.
    fn square_occupied(&self, col: u32, row: u32) -> bool {
        !matches!(self.cell_value(col, row), CellValue::Empty)
    }

    /// Whether a hex-sheet cell holds content — the predicate the hex
    /// Ctrl+arrow jump scans.
    fn hex_occupied(&self, coord: HexCoord) -> bool {
        !matches!(self.hex_cell_value(coord), CellValue::Empty)
    }

    /// Extend the selection one cell — Shift+arrow, on either sheet.
    fn extend_active(&mut self, dir: Dir) {
        match self.active {
            ActiveSheet::Square => {
                let next = step_square(self.selection.cursor, dir);
                self.selection.extend_to(next);
            }
            ActiveSheet::Hex => self.extend_hex_selection(dir),
        }
    }

    /// Step the hex selection one axial cell — collapsing any range and
    /// ignoring a move that would leave the visible disc.
    fn move_hex_selection(&mut self, dir: Dir) {
        let next = hex_step(self.hex_selection.cursor, dir);
        if hex_in_view(next) {
            self.hex_selection.collapse_to(next);
        }
    }

    /// Extend the hex selection one axial cell — Shift+arrow.
    fn extend_hex_selection(&mut self, dir: Dir) {
        let next = hex_step(self.hex_selection.cursor, dir);
        if hex_in_view(next) {
            self.hex_selection.extend_to(next);
        }
    }

    fn begin_edit(&mut self, replace_with: Option<char>) {
        // An edit acts on a single cell — collapse any range to its cursor.
        match self.active {
            ActiveSheet::Square => {
                let cursor = self.selection.cursor;
                self.selection.collapse_to(cursor);
            }
            ActiveSheet::Hex => {
                let cursor = self.hex_selection.cursor;
                self.hex_selection.collapse_to(cursor);
            }
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
        for (addr, after) in dedup_targets(targets) {
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
            self.history.record(Action::Cells(edits));
        }
    }

    /// Write the in-progress edit to the active sheet and leave editing
    /// mode. An all-whitespace buffer clears the cell.
    fn commit_edit(&mut self) {
        let Some(edit) = self.edit.take() else {
            return;
        };
        let source = commit_source(&edit.buffer);
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
            ActiveSheet::Hex => {
                let targets = self
                    .hex_selection
                    .cells()
                    .into_iter()
                    .map(|c| (hex_address(c), None))
                    .collect();
                (self.hex_sheet, targets)
            }
        };
        self.apply_edits(sheet, targets);
    }

    /// Read a cell into a `CopiedCell` — its source plus its value as a
    /// re-typeable literal (the fallback a cross-lattice paste writes).
    fn copied_cell(&self, sheet: SheetId, addr: &str) -> CopiedCell {
        match self.engine.get_cell(sheet, addr) {
            Some(snapshot) => {
                let value = natural_text(snapshot.value);
                CopiedCell {
                    source: snapshot.source,
                    value: (!value.is_empty()).then_some(value),
                }
            }
            None => CopiedCell::default(),
        }
    }

    /// Copy the selected range into the in-app clipboard and push a
    /// tab-separated grid of displayed values to the system clipboard.
    /// Works on either sheet — a hex range is a `q × r` block, so it
    /// captures into the same rectangular clipboard.
    fn copy_selection(&mut self, ctx: &egui::Context) {
        match self.active {
            ActiveSheet::Square => {
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
                        cells.push(self.copied_cell(self.square_sheet, &grid::cell_address(c, r)));
                    }
                    tsv.push('\n');
                }
                self.clipboard = Clipboard::capture(width, height, cells, false);
                ctx.copy_text(tsv);
            }
            ActiveSheet::Hex => {
                let ((min_q, min_r), (max_q, max_r)) = self.hex_selection.bounds();
                let (width, height) = self.hex_selection.dimensions();
                let mut cells = Vec::new();
                let mut tsv = String::new();
                for r in min_r..=max_r {
                    for q in min_q..=max_q {
                        if q > min_q {
                            tsv.push('\t');
                        }
                        let coord = HexCoord::new(q, r);
                        tsv.push_str(&self.hex_cell_text(coord));
                        cells.push(self.copied_cell(self.hex_sheet, &hex_address(coord)));
                    }
                    tsv.push('\n');
                }
                self.clipboard = Clipboard::capture(width as u32, height as u32, cells, true);
                ctx.copy_text(tsv);
            }
        }
    }

    /// Cut the selected range — captures it like a copy, then arms the
    /// range so the next paste clears it. Works on either sheet.
    fn cut_selection(&mut self, ctx: &egui::Context) {
        self.copy_selection(ctx);
        // copy_selection just rebuilt the clipboard as a copy — mark it a
        // cut, recording the leading corner of the selection's origin.
        let origin = match self.active {
            ActiveSheet::Square => {
                let ((c, r), _) = self.selection.bounds();
                (c as i32, r as i32)
            }
            ActiveSheet::Hex => {
                let ((q, r), _) = self.hex_selection.bounds();
                (q, r)
            }
        };
        self.clipboard.mark_as_cut(origin);
    }

    /// Paste the clipboard block with its top-left at the active cell,
    /// recorded as one undo step. Pasting onto the same kind of sheet
    /// the copy came from writes the cell sources; pasting onto a
    /// different lattice writes each cell's value instead.
    fn paste(&mut self, mode: PasteMode) {
        if self.clipboard.is_empty() {
            return;
        }
        let (width, height) = self.clipboard.dimensions();
        match self.active {
            ActiveSheet::Square => {
                let (target_c, target_r) = self.selection.cursor;
                let mut targets = Vec::new();
                // A cut from this sheet clears its origin cells — queued
                // first, so a paste on the same cell overrides the clear.
                if let Some((oc, or)) = self.clipboard.cut_origin() {
                    if !self.clipboard.from_hex() {
                        for j in 0..height {
                            for i in 0..width {
                                let addr = grid::cell_address(oc as u32 + i, or as u32 + j);
                                targets.push((addr, None));
                            }
                        }
                    }
                }
                for (rel_c, rel_r, cell) in self.clipboard.entries() {
                    let c = target_c + rel_c;
                    let r = target_r + rel_r;
                    if c >= COLS || r >= ROWS {
                        continue;
                    }
                    let source = self.clipboard.paste_text(cell, false, mode);
                    targets.push((grid::cell_address(c, r), source));
                }
                self.apply_edits(self.square_sheet, targets);
                // Select the pasted block, the active cell at its top-left.
                let end_c = (target_c + width - 1).min(COLS - 1);
                let end_r = (target_r + height - 1).min(ROWS - 1);
                self.selection = Selection {
                    anchor: (end_c, end_r),
                    cursor: (target_c, target_r),
                };
            }
            ActiveSheet::Hex => {
                let cursor = self.hex_selection.cursor;
                let mut targets = Vec::new();
                // A cut from this sheet clears its origin cells — queued
                // first, so a paste on the same cell overrides the clear.
                if let Some((oq, or)) = self.clipboard.cut_origin() {
                    if self.clipboard.from_hex() {
                        for j in 0..height {
                            for i in 0..width {
                                let coord = HexCoord::new(oq + i as i32, or + j as i32);
                                targets.push((hex_address(coord), None));
                            }
                        }
                    }
                }
                for (rel_c, rel_r, cell) in self.clipboard.entries() {
                    let coord = HexCoord::new(cursor.q + rel_c as i32, cursor.r + rel_r as i32);
                    if !hex_in_view(coord) {
                        continue;
                    }
                    let source = self.clipboard.paste_text(cell, true, mode);
                    targets.push((hex_address(coord), source));
                }
                self.apply_edits(self.hex_sheet, targets);
                // Select the pasted block, the cursor at its origin.
                let far = HexCoord::new(cursor.q + width as i32 - 1, cursor.r + height as i32 - 1);
                self.hex_selection = HexSelection {
                    anchor: far,
                    cursor,
                };
            }
        }
        // A paste consumes the cut — the clipboard reverts to a copy.
        self.clipboard.consume_cut();
    }

    /// Revert the most recent action — cell content or formatting. The
    /// engine recomputes any dependents.
    fn undo(&mut self) {
        let Some(action) = self.history.undo() else {
            return;
        };
        match action {
            Action::Cells(edits) => {
                for edit in &edits {
                    let _ = self
                        .engine
                        .set_cell(edit.sheet, &edit.addr, edit.before.as_deref());
                }
            }
            Action::Formats(edits) => {
                for edit in &edits {
                    self.formats.update(edit.cell, |f| *f = edit.before.clone());
                }
            }
            Action::HexFormats(edits) => {
                for edit in &edits {
                    self.hex_formats
                        .update(edit.cell, |f| *f = edit.before.clone());
                }
            }
        }
    }

    /// Re-apply the most recently undone action.
    fn redo(&mut self) {
        let Some(action) = self.history.redo() else {
            return;
        };
        match action {
            Action::Cells(edits) => {
                for edit in &edits {
                    let _ = self
                        .engine
                        .set_cell(edit.sheet, &edit.addr, edit.after.as_deref());
                }
            }
            Action::Formats(edits) => {
                for edit in &edits {
                    self.formats.update(edit.cell, |f| *f = edit.after.clone());
                }
            }
            Action::HexFormats(edits) => {
                for edit in &edits {
                    self.hex_formats
                        .update(edit.cell, |f| *f = edit.after.clone());
                }
            }
        }
    }

    /// Fill the selection's leading edge across the rest of it — Ctrl+D
    /// (down) / Ctrl+R (right), on either sheet. Sources copy verbatim
    /// and the whole fill is a single undo step.
    fn fill(&mut self, dir: FillDir) {
        let (sheet, targets) = match self.active {
            ActiveSheet::Square => {
                let targets = self
                    .selection
                    .fill_targets(dir)
                    .into_iter()
                    .map(|(target, from)| {
                        let source = self
                            .engine
                            .get_cell(self.square_sheet, &grid::cell_address(from.0, from.1))
                            .and_then(|s| s.source);
                        (grid::cell_address(target.0, target.1), source)
                    })
                    .collect();
                (self.square_sheet, targets)
            }
            ActiveSheet::Hex => {
                let targets = self
                    .hex_selection
                    .fill_targets(dir)
                    .into_iter()
                    .map(|(target, from)| {
                        let source = self
                            .engine
                            .get_cell(self.hex_sheet, &hex_address(from))
                            .and_then(|s| s.source);
                        (hex_address(target), source)
                    })
                    .collect();
                (self.hex_sheet, targets)
            }
        };
        self.apply_edits(sheet, targets);
    }

    /// Resize the header border under an in-progress drag. The headers
    /// are both frozen, so the hit-tests use their floating origins —
    /// `col_hdr_origin` for column borders, `row_hdr_origin` for row.
    ///
    /// The hit-test runs on the **press position** captured in
    /// [`Self::press_pos`] rather than the pointer's position at the
    /// moment `drag_started` fires. egui's drag threshold lets the
    /// pointer travel 5+ pixels before that event arrives — far enough
    /// to leave the border's grab zone and have the click be misread
    /// as a header-select drag.
    fn handle_resize(
        &mut self,
        response: &egui::Response,
        col_hdr_origin: egui::Pos2,
        row_hdr_origin: egui::Pos2,
    ) {
        if response.drag_started() {
            if let Some(p) = self.press_pos.or_else(|| response.interact_pointer_pos()) {
                self.resizing = self
                    .metrics
                    .col_border_at(col_hdr_origin, p, COLS)
                    .map(Resize::Column)
                    .or_else(|| {
                        self.metrics
                            .row_border_at(row_hdr_origin, p, ROWS)
                            .map(Resize::Row)
                    });
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

    /// Set a column's width to fit its widest cell's rendered text —
    /// triggered by a double-click on the column's resize border.
    fn autofit_column(&mut self, col: u32, painter: &egui::Painter) {
        let widths = (0..ROWS).map(|row| {
            let text = self.cell_text(col, row);
            if text.is_empty() {
                0.0
            } else {
                let pts = self.formats.get((col, row)).font_size.points();
                painter
                    .layout_no_wrap(text, egui::FontId::proportional(pts), egui::Color32::BLACK)
                    .size()
                    .x
            }
        });
        // 5.0 is the cell text's left/right inset in `draw_cell_text`.
        let width = grid::fit_extent(widths, 2.0 * 5.0, grid::MIN_COL_W);
        self.metrics.set_col_width(col, width);
    }

    /// Set a row's height to fit its tallest cell's rendered text —
    /// triggered by a double-click on the row's resize border. Each
    /// cell is measured at its own font size.
    fn autofit_row(&mut self, row: u32, painter: &egui::Painter) {
        let heights = (0..COLS).map(|col| {
            let text = self.cell_text(col, row);
            if text.is_empty() {
                0.0
            } else {
                let pts = self.formats.get((col, row)).font_size.points();
                painter
                    .layout_no_wrap(text, egui::FontId::proportional(pts), egui::Color32::BLACK)
                    .size()
                    .y
            }
        });
        // 5.0 is the cell text's top/bottom inset in `draw_cell_text`.
        let height = grid::fit_extent(heights, 2.0 * 5.0, grid::MIN_ROW_H);
        self.metrics.set_row_height(row, height);
    }

    /// The square cell under a pointer position, if any.
    fn cell_under(
        &self,
        response: &egui::Response,
        origin: egui::Pos2,
        header_x: f32,
        header_y: f32,
    ) -> Option<(u32, u32)> {
        response.interact_pointer_pos().and_then(|p| {
            self.metrics
                .cell_at_frozen(origin, header_x, header_y, p, COLS, ROWS)
        })
    }

    fn draw_grid(&mut self, ui: &mut egui::Ui) {
        let size = egui::vec2(
            self.metrics.total_width(COLS),
            self.metrics.total_height(ROWS),
        );
        let (response, painter) = ui.allocate_painter(size, egui::Sense::click_and_drag());
        let origin = response.rect.min;
        // Both headers are frozen: each floats to its viewport edge so
        // the A/B/C strip and the 1/2/3 column stay visible while the
        // cells scroll. The corner box rides them both.
        let header_x = ui.clip_rect().left();
        let header_y = ui.clip_rect().top();
        let col_hdr_origin = egui::pos2(origin.x, header_y);
        let row_hdr_origin = egui::pos2(header_x, origin.y);
        let corner_origin = egui::pos2(header_x, header_y);

        // Capture the press position the moment the button goes down,
        // before egui's drag threshold has had a chance to fire — its
        // ~5-pixel slop is enough to leave a border's grab zone and
        // make a resize attempt read as a header-select drag instead.
        // `interact_pointer_pos` is `Some` while a press is active, so
        // the None → Some transition is the press frame.
        let interact_pos = response.interact_pointer_pos();
        match (self.press_pos, interact_pos) {
            (None, Some(p)) => self.press_pos = Some(p),
            (Some(_), None) => self.press_pos = None,
            _ => {}
        }

        // Keep the active cell in view when the keyboard moves it past a
        // scroll edge. `None` alignment scrolls the minimum amount, so an
        // already-visible cell (e.g. one just clicked) is left untouched.
        if self.selection.cursor != self.prev_cursor {
            let (c, r) = self.selection.cursor;
            let cursor_rect = egui::Rect::from_min_size(
                origin + egui::vec2(self.metrics.col_left(c), self.metrics.row_top(r)),
                egui::vec2(self.metrics.col_width(c), self.metrics.row_height(r)),
            );
            ui.scroll_to_rect(cursor_rect, None);
            self.prev_cursor = self.selection.cursor;
        }

        if let Some(p) = response.hover_pos() {
            if self
                .metrics
                .col_border_at(col_hdr_origin, p, COLS)
                .is_some()
            {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeColumn);
            } else if self
                .metrics
                .row_border_at(row_hdr_origin, p, ROWS)
                .is_some()
            {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeRow);
            }
        }
        self.handle_resize(&response, col_hdr_origin, row_hdr_origin);

        // A hovered cell shows its error detail, or failing that its note.
        if let Some(p) = response.hover_pos() {
            if let Some(cell) = self
                .metrics
                .cell_at_frozen(origin, header_x, header_y, p, COLS, ROWS)
            {
                if let CellValue::Error(e) = self.cell_value(cell.0, cell.1) {
                    let detail = error_detail(&e);
                    egui::show_tooltip_at_pointer(
                        ui.ctx(),
                        ui.layer_id(),
                        egui::Id::new("cell_error_tooltip"),
                        |ui| ui.label(detail),
                    );
                } else if self.notes.has(cell) {
                    let note = self.notes.get(cell).to_string();
                    egui::show_tooltip_at_pointer(
                        ui.ctx(),
                        ui.layer_id(),
                        egui::Id::new("cell_note_tooltip"),
                        |ui| ui.label(note),
                    );
                }
            }
        }

        // Double-clicking a column or row border autofits it to content.
        if response.double_clicked() {
            if let Some(p) = response.interact_pointer_pos() {
                if let Some(col) = self.metrics.col_border_at(col_hdr_origin, p, COLS) {
                    self.autofit_column(col, &painter);
                } else if let Some(row) = self.metrics.row_border_at(row_hdr_origin, p, ROWS) {
                    self.autofit_row(row, &painter);
                }
            }
        }

        // A drag that didn't start on a header border sweeps a selection:
        // a cell drag a cell range, a header drag a column/row range.
        // The drag_started arm hit-tests against `press_pos` for the
        // same reason `handle_resize` does — egui's threshold can leave
        // the pointer a band-width away from where the user actually
        // clicked.
        if self.resizing.is_none() {
            if response.drag_started() {
                if let Some(p) = self.press_pos.or_else(|| response.interact_pointer_pos()) {
                    if let Some(col) = self.metrics.col_header_at(col_hdr_origin, p, COLS) {
                        self.commit_edit();
                        self.header_drag = Some(HeaderDrag::Column(col));
                        self.selection = Selection::column(col, ROWS);
                    } else if let Some(row) = self.metrics.row_header_at(row_hdr_origin, p, ROWS) {
                        self.commit_edit();
                        self.header_drag = Some(HeaderDrag::Row(row));
                        self.selection = Selection::row(row, COLS);
                    } else if let Some(cell) = self
                        .metrics
                        .cell_at_frozen(origin, header_x, header_y, p, COLS, ROWS)
                    {
                        self.commit_edit();
                        self.selection.collapse_to(cell);
                    }
                }
            } else if response.dragged() {
                if let Some(p) = response.interact_pointer_pos() {
                    match self.header_drag {
                        Some(HeaderDrag::Column(c0)) => {
                            let c1 = self.metrics.col_at_x(origin, p, COLS);
                            self.selection = Selection::column_range(c0, c1, ROWS);
                        }
                        Some(HeaderDrag::Row(r0)) => {
                            let r1 = self.metrics.row_at_y(origin, p, ROWS);
                            self.selection = Selection::row_range(r0, r1, COLS);
                        }
                        None => {
                            if let Some(cell) = self
                                .metrics
                                .cell_at_frozen(origin, header_x, header_y, p, COLS, ROWS)
                            {
                                self.selection.extend_to(cell);
                            }
                        }
                    }
                }
            }
        }
        if response.drag_stopped() {
            self.header_drag = None;
        }
        if response.clicked() {
            if let Some(cell) = self.cell_under(&response, origin, header_x, header_y) {
                // Formula-mode click — while the in-cell edit is a formula
                // (`=…`), a click on another cell inserts that cell's A1
                // address into the formula at the end of the buffer
                // instead of committing the edit and moving the selection.
                // The TextEdit re-focuses next frame via `fresh = true`.
                let in_formula = self
                    .edit
                    .as_ref()
                    .is_some_and(|e| e.buffer.trim_start().starts_with('='));
                if in_formula {
                    let addr = grid::cell_address(cell.0, cell.1);
                    if let Some(edit) = self.edit.as_mut() {
                        edit.buffer.push_str(&addr);
                        edit.fresh = true;
                    }
                } else if let Some(fmt) = self.format_painter.take() {
                    // Format painter — paint the captured format onto the
                    // target cell and disarm; selection doesn't move.
                    self.commit_edit();
                    self.formats.update(cell, |f| *f = fmt);
                } else {
                    self.commit_edit();
                    if ui.input(|i| i.modifiers.shift) {
                        self.selection.extend_to(cell);
                    } else {
                        self.selection.collapse_to(cell);
                    }
                }
            } else if let Some(p) = response.interact_pointer_pos() {
                // A click on a header selects a whole column or row — with
                // Shift, the range from the current anchor. The header
                // corner selects the sheet.
                let shift = ui.input(|i| i.modifiers.shift);
                if let Some(col) = self.metrics.col_header_at(col_hdr_origin, p, COLS) {
                    self.commit_edit();
                    self.selection = if shift {
                        Selection::column_range(self.selection.anchor.0, col, ROWS)
                    } else {
                        Selection::column(col, ROWS)
                    };
                } else if let Some(row) = self.metrics.row_header_at(row_hdr_origin, p, ROWS) {
                    self.commit_edit();
                    self.selection = if shift {
                        Selection::row_range(self.selection.anchor.1, row, COLS)
                    } else {
                        Selection::row(row, COLS)
                    };
                } else if grid::in_header_corner(corner_origin, p) {
                    self.commit_edit();
                    self.selection = Selection::all(COLS, ROWS);
                }
            }
        }
        // A double-click on a cell begins editing it in place.
        if response.double_clicked() {
            if let Some(cell) = self.cell_under(&response, origin, header_x, header_y) {
                self.selection.collapse_to(cell);
                self.begin_edit(None);
            }
        }
        // Right-click selects the cell under the cursor — unless it is
        // already inside the selection, which a right-click keeps — then
        // opens a context menu of the common cell actions.
        if response.secondary_clicked() {
            if let Some(cell) = self.cell_under(&response, origin, header_x, header_y) {
                if !self.selection.contains(cell) {
                    self.commit_edit();
                    self.selection.collapse_to(cell);
                }
            }
        }
        response.context_menu(|ui| {
            if ui.button("Copy").clicked() {
                self.copy_selection(ui.ctx());
                ui.close_menu();
            }
            if ui.button("Cut").clicked() {
                self.cut_selection(ui.ctx());
                ui.close_menu();
            }
            if ui.button("Paste").clicked() {
                self.paste(PasteMode::Normal);
                ui.close_menu();
            }
            if ui.button("Paste values").clicked() {
                self.paste(PasteMode::ValuesOnly);
                ui.close_menu();
            }
            if ui.button("Clear").clicked() {
                self.clear_active();
                ui.close_menu();
            }
            ui.separator();
            if ui.button("Select region").clicked() {
                self.select_region();
                ui.close_menu();
            }
            if ui.button("Toggle checkbox").clicked() {
                self.toggle_widget_cells();
                ui.close_menu();
            }
            if ui.button("Fill series").clicked() {
                self.fill_series();
                ui.close_menu();
            }
            ui.menu_button("Sort", |ui| {
                if ui.button("Ascending").clicked() {
                    self.sort_selection(true);
                    ui.close_menu();
                }
                if ui.button("Descending").clicked() {
                    self.sort_selection(false);
                    ui.close_menu();
                }
            });
            if ui.button("Edit note…").clicked() {
                let cell = self.selection.cursor;
                self.note_cell = CellId::Square(cell);
                self.note_draft = self.notes.get(cell).to_string();
                self.note_open = true;
                ui.close_menu();
            }
        });

        let visuals = ui.visuals();
        let grid_line = egui::Stroke::new(1.0, visuals.weak_text_color());
        let header_bg = visuals.faint_bg_color;
        let cell_bg = visuals.extreme_bg_color;
        let text_color = visuals.text_color();
        let sel_color = visuals.selection.stroke.color;
        let sel = visuals.selection.bg_fill;
        let sel_tint = egui::Color32::from_rgba_unmultiplied(sel.r(), sel.g(), sel.b(), 64);
        let find_tint = egui::Color32::from_rgba_unmultiplied(255, 200, 0, 80);
        let border_stroke = egui::Stroke::new(2.0, text_color);
        let font = egui::FontId::proportional(13.0);

        // Cells: fill, the selection tint, border, and the formatted
        // value. The cell being edited is left blank for the overlay.
        let cursor = self.selection.cursor;
        let editing_cell = self.edit.as_ref().map(|_| cursor);
        for r in 0..ROWS {
            for c in 0..COLS {
                let rect = self.metrics.cell_rect(origin, c, r);
                let base = self.formats.get((c, r));
                let fmt = if self.cond_rules.is_empty() {
                    base
                } else {
                    let value = self.cell_value(c, r);
                    conditional::effective_format(&base, &value, &self.cond_rules)
                };
                painter.rect_filled(rect, 0.0, fmt.fill.unwrap_or(cell_bg));
                // The active cell stays untinted; the rest of the range
                // gets a translucent wash, the way Excel/Sheets show it.
                if (c, r) != cursor && self.selection.contains((c, r)) {
                    painter.rect_filled(rect, 0.0, sel_tint);
                }
                if self.find.is_match(CellId::Square((c, r))) {
                    painter.rect_filled(rect, 0.0, find_tint);
                }
                painter.rect_stroke(rect, 0.0, grid_line);
                if editing_cell == Some((c, r)) {
                    continue;
                }
                // Toggle cells are drawn as a checkbox in a later pass.
                if self.widgets.is_toggle((c, r)) {
                    continue;
                }
                let text = self.cell_text(c, r);
                if !text.is_empty() {
                    let value = self.cell_value(c, r);
                    let numeric = matches!(value, CellValue::Number(_) | CellValue::Integer(_));
                    let is_negative = match value {
                        CellValue::Number(n) => n < 0.0,
                        CellValue::Integer(i) => i < 0,
                        _ => false,
                    };
                    draw_cell_text(
                        &painter,
                        &text,
                        rect,
                        &fmt,
                        numeric,
                        is_negative,
                        text_color,
                    );
                }
                if self.notes.has((c, r)) {
                    draw_note_marker(&painter, rect);
                }
            }
        }

        // A second pass for the heavy format borders, so a neighbouring
        // cell's grid line does not overpaint a bordered range's right
        // or bottom perimeter.
        for r in 0..ROWS {
            for c in 0..COLS {
                let base = self.formats.get((c, r));
                let fmt = if self.cond_rules.is_empty() {
                    base
                } else {
                    conditional::effective_format(&base, &self.cell_value(c, r), &self.cond_rules)
                };
                if fmt.borders == Borders::default() {
                    continue;
                }
                let rect = self.metrics.cell_rect(origin, c, r);
                draw_borders(&painter, rect, &fmt.borders, border_stroke);
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

        // The cut marquee — a dashed border around the armed range, when
        // the clipboard's cut belongs to this (square) sheet.
        if let Some((oc, or)) = self.clipboard.cut_origin() {
            if !self.clipboard.from_hex() {
                let (cw, ch) = self.clipboard.dimensions();
                let tl = self.metrics.cell_rect(origin, oc as u32, or as u32);
                let br = self
                    .metrics
                    .cell_rect(origin, oc as u32 + cw - 1, or as u32 + ch - 1);
                let rect = egui::Rect::from_min_max(tl.min, br.max);
                let corners = [
                    rect.left_top(),
                    rect.right_top(),
                    rect.right_bottom(),
                    rect.left_bottom(),
                    rect.left_top(),
                ];
                painter.extend(egui::Shape::dashed_line(
                    &corners,
                    egui::Stroke::new(1.5, sel_color),
                    4.0,
                    3.0,
                ));
            }
        }

        // Boolean toggle cells render as a clickable checkbox.
        if !self.widgets.is_empty() {
            let mut flipped = None;
            for r in 0..ROWS {
                for c in 0..COLS {
                    if !self.widgets.is_toggle((c, r)) || editing_cell == Some((c, r)) {
                        continue;
                    }
                    let rect = self.metrics.cell_rect(origin, c, r);
                    let mut checked = widget::bool_state(&self.cell_value(c, r));
                    if ui
                        .put(rect, egui::Checkbox::new(&mut checked, ""))
                        .changed()
                    {
                        flipped = Some((grid::cell_address(c, r), checked));
                    }
                }
            }
            if let Some((addr, checked)) = flipped {
                let source = widget::bool_source(checked).to_string();
                self.apply_edits(self.square_sheet, vec![(addr, Some(source))]);
            }
        }

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

        // The frozen row header — floated to the viewport's left edge
        // so the 1/2/3 column stays visible as the cells scroll right.
        // Drawn after the cells so it overpaints any column that scrolls
        // beneath it.
        for r in 0..ROWS {
            let rect = egui::Rect::from_min_size(
                egui::pos2(header_x, origin.y + self.metrics.row_top(r)),
                egui::vec2(grid::HEADER_W, self.metrics.row_height(r)),
            );
            painter.rect_filled(rect, 0.0, header_bg);
            // Tint the active cell's row header.
            if r == self.selection.cursor.1 {
                painter.rect_filled(rect, 0.0, sel_tint);
            }
            painter.rect_stroke(rect, 0.0, grid_line);
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                (r + 1).to_string(),
                font.clone(),
                text_color,
            );
        }
        // The frozen column header — floated to the viewport's top edge
        // so the A/B/C strip stays visible as the cells scroll down.
        for c in 0..COLS {
            let rect = egui::Rect::from_min_size(
                egui::pos2(origin.x + self.metrics.col_left(c), header_y),
                egui::vec2(self.metrics.col_width(c), grid::HEADER_H),
            );
            painter.rect_filled(rect, 0.0, header_bg);
            // Tint the active cell's column header.
            if c == self.selection.cursor.0 {
                painter.rect_filled(rect, 0.0, sel_tint);
            }
            painter.rect_stroke(rect, 0.0, grid_line);
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                grid::column_label(c),
                font.clone(),
                text_color,
            );
        }
        // The header corner — drawn last so it always sits above both
        // frozen header bands, even when one of them scrolls into the
        // other's track.
        let corner_rect =
            egui::Rect::from_min_size(corner_origin, egui::vec2(grid::HEADER_W, grid::HEADER_H));
        painter.rect_filled(corner_rect, 0.0, header_bg);
        painter.rect_stroke(corner_rect, 0.0, grid_line);
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
        let fmt = self.hex_effective_format(coord);
        painter.add(egui::Shape::convex_polygon(vertices.clone(), fill, stroke));
        // Per-edge borders, stroked heavier over the polygon outline.
        if fmt.hex_borders.edges.iter().any(|&on| on) {
            let border = egui::Stroke::new(2.0, text_color);
            for (i, &on) in fmt.hex_borders.edges.iter().enumerate() {
                if on {
                    painter.line_segment([vertices[i], vertices[(i + 1) % 6]], border);
                }
            }
        }

        let text = self.hex_cell_text(coord);
        if !text.is_empty() {
            let value = self.hex_cell_value(coord);
            let numeric = matches!(value, CellValue::Number(_) | CellValue::Integer(_));
            let is_negative = match value {
                CellValue::Number(n) => n < 0.0,
                CellValue::Integer(i) => i < 0,
                _ => false,
            };
            let c = self.hex_lattice.centroid(coord);
            let vshift = match fmt.valign {
                VAlign::Top => -HEX_SIZE * 0.5,
                VAlign::Middle => 0.0,
                VAlign::Bottom => HEX_SIZE * 0.5,
            };
            let centroid = egui::pos2(origin.x + c.x, origin.y + c.y + vshift);
            let align = format::effective_align(fmt.align, numeric);
            let (anchor, pos) = hex_text_layout(align, centroid, HEX_SIZE * 0.6);
            let color = format::effective_text_color(&fmt, is_negative, text_color);
            let font = egui::FontId::proportional(fmt.font_size.points());
            let text_rect = painter.text(pos, anchor, &text, font.clone(), color);
            if fmt.bold {
                // Faux-bold: a second pass nudged half a pixel across.
                painter.text(egui::pos2(pos.x + 0.5, pos.y), anchor, &text, font, color);
            }
            draw_text_decorations(painter, text_rect, &fmt, color);
        }
        // A note marker — a small dot near the hexagon's top.
        if self.hex_notes.has(coord) {
            let centre = self.hex_lattice.centroid(coord);
            let dot = egui::pos2(origin.x + centre.x, origin.y + centre.y - HEX_SIZE * 0.52);
            painter.circle_filled(dot, 3.0, egui::Color32::from_rgb(220, 90, 70));
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
                    if hex_in_view(coord) {
                        self.commit_edit();
                        // Shift-click extends the range; a plain click resets.
                        if ui.input(|i| i.modifiers.shift) {
                            self.hex_selection.extend_to(coord);
                        } else {
                            self.hex_selection.collapse_to(coord);
                        }
                    }
                }
            }
        }

        // A double-click on a hex begins editing it in place.
        if response.double_clicked() {
            if let Some(p) = response.interact_pointer_pos() {
                let local = Point2::new(p.x - origin.x, p.y - origin.y);
                if let Some(coord) = self.hex_lattice.cell_at(local) {
                    if hex_in_view(coord) {
                        self.hex_selection.collapse_to(coord);
                        self.begin_edit(None);
                    }
                }
            }
        }

        // Right-click selects the hex under the pointer (unless it is
        // already selected), then opens the cell-actions menu.
        if response.secondary_clicked() {
            if let Some(p) = response.interact_pointer_pos() {
                let local = Point2::new(p.x - origin.x, p.y - origin.y);
                if let Some(coord) = self.hex_lattice.cell_at(local) {
                    if hex_in_view(coord) && !self.hex_selection.contains(coord) {
                        self.commit_edit();
                        self.hex_selection.collapse_to(coord);
                    }
                }
            }
        }
        response.context_menu(|ui| {
            if ui.button("Copy").clicked() {
                self.copy_selection(ui.ctx());
                ui.close_menu();
            }
            if ui.button("Cut").clicked() {
                self.cut_selection(ui.ctx());
                ui.close_menu();
            }
            if ui.button("Paste").clicked() {
                self.paste(PasteMode::Normal);
                ui.close_menu();
            }
            if ui.button("Paste values").clicked() {
                self.paste(PasteMode::ValuesOnly);
                ui.close_menu();
            }
            if ui.button("Clear").clicked() {
                self.clear_active();
                ui.close_menu();
            }
            ui.separator();
            if ui.button("Select region").clicked() {
                self.select_region();
                ui.close_menu();
            }
            if ui.button("Fill series").clicked() {
                self.fill_series();
                ui.close_menu();
            }
            ui.menu_button("Sort", |ui| {
                if ui.button("Ascending").clicked() {
                    self.sort_selection(true);
                    ui.close_menu();
                }
                if ui.button("Descending").clicked() {
                    self.sort_selection(false);
                    ui.close_menu();
                }
            });
            if ui.button("Edit note…").clicked() {
                let coord = self.hex_selection.cursor;
                self.note_cell = CellId::Hex(coord);
                self.note_draft = self.hex_notes.get(coord).to_string();
                self.note_open = true;
                ui.close_menu();
            }
        });
        // A hovered hex shows its error detail, or failing that its note.
        if let Some(p) = response.hover_pos() {
            let local = Point2::new(p.x - origin.x, p.y - origin.y);
            if let Some(coord) = self.hex_lattice.cell_at(local) {
                if hex_in_view(coord) {
                    if let CellValue::Error(e) = self.hex_cell_value(coord) {
                        let detail = error_detail(&e);
                        egui::show_tooltip_at_pointer(
                            ui.ctx(),
                            ui.layer_id(),
                            egui::Id::new("hex_error_tooltip"),
                            |ui| ui.label(detail),
                        );
                    } else if self.hex_notes.has(coord) {
                        let note = self.hex_notes.get(coord).to_string();
                        egui::show_tooltip_at_pointer(
                            ui.ctx(),
                            ui.layer_id(),
                            egui::Id::new("hex_note_tooltip"),
                            |ui| ui.label(note),
                        );
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

        let cursor = self.hex_selection.cursor;
        for coord in hex::hex_disc(HexCoord::new(0, 0), HEX_VIEW_RADIUS) {
            if coord == cursor {
                continue;
            }
            // Cells inside the selected range take the selection fill.
            let fill = if self.hex_selection.contains(coord) {
                sel_bg
            } else if self.find.is_match(CellId::Hex(coord)) {
                egui::Color32::from_rgb(255, 236, 170)
            } else {
                self.hex_effective_format(coord).fill.unwrap_or(cell_bg)
            };
            self.paint_hex(&painter, origin, coord, fill, line, text_color);
        }
        // The cursor hex is painted last so its ring sits above every
        // neighbour's shared border. While editing, the text editor
        // overlay (drawn below) covers the cell's painted value.
        self.paint_hex(&painter, origin, cursor, sel_bg, sel_stroke, text_color);

        // The cut marquee — a dashed outline around each armed hex, when
        // the clipboard's cut belongs to this (hex) sheet.
        if let Some((oq, or)) = self.clipboard.cut_origin() {
            if self.clipboard.from_hex() {
                let (cw, ch) = self.clipboard.dimensions();
                let dash = egui::Stroke::new(1.5, sel_stroke.color);
                for j in 0..ch {
                    for i in 0..cw {
                        let coord = HexCoord::new(oq + i as i32, or + j as i32);
                        if !hex_in_view(coord) {
                            continue;
                        }
                        let mut loop_pts: Vec<egui::Pos2> = self
                            .hex_lattice
                            .vertices(coord)
                            .iter()
                            .map(|v| egui::pos2(origin.x + v.x, origin.y + v.y))
                            .collect();
                        if let Some(&first) = loop_pts.first() {
                            loop_pts.push(first);
                        }
                        painter.extend(egui::Shape::dashed_line(&loop_pts, dash, 4.0, 3.0));
                    }
                }
            }
        }

        // The in-cell editor overlay, sized to sit within the hexagon.
        if let Some(edit) = &mut self.edit {
            let centroid = self.hex_lattice.centroid(self.hex_selection.cursor);
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
        ctx.set_visuals(if self.dark_mode {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        });
        self.frame_time = ctx.input(|i| i.time);
        for command in self.collect_commands(ctx) {
            self.apply(command, ctx);
        }

        egui::TopBottomPanel::top("tescellate_ribbon").show(ctx, |ui| match self.active {
            ActiveSheet::Square => {
                let current = self.formats.get(self.selection.cursor);
                let can_undo = self.history.can_undo();
                let can_redo = self.history.can_redo();
                if let Some(action) = ribbon::ribbon(
                    ui,
                    &current,
                    can_undo,
                    can_redo,
                    self.format_painter.is_some(),
                ) {
                    self.apply_ribbon(action, ctx);
                }
            }
            ActiveSheet::Hex => {
                // The hex-only orientation toggle, then the shared format
                // ribbon — formatting now applies to hex cells too.
                ui.horizontal(|ui| {
                    // Switching orientation changes geometry only — axial
                    // (q,r) coords are orientation-independent, so cell
                    // data, formatting and selection carry over.
                    let pointy = matches!(self.hex_lattice.orientation, HexOrientation::Pointy);
                    if ui.selectable_label(pointy, "Pointy-top").clicked() {
                        self.hex_lattice = HexLattice::pointy(HEX_SIZE);
                    }
                    if ui.selectable_label(!pointy, "Flat-top").clicked() {
                        self.hex_lattice = HexLattice::flat(HEX_SIZE);
                    }
                });
                let current = self.hex_formats.get(self.hex_selection.cursor);
                let can_undo = self.history.can_undo();
                let can_redo = self.history.can_redo();
                if let Some(action) = ribbon::ribbon(
                    ui,
                    &current,
                    can_undo,
                    can_redo,
                    self.format_painter.is_some(),
                ) {
                    self.apply_ribbon(action, ctx);
                }
            }
        });

        egui::TopBottomPanel::top("tescellate_formula_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let (addr, source) = self.active_address_and_source();
                match self.active {
                    ActiveSheet::Square => {
                        let response = ui.add(
                            egui::TextEdit::singleline(&mut self.name_box)
                                .desired_width(64.0)
                                .font(egui::TextStyle::Monospace),
                        );
                        if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            // Enter jumps the selection to the typed address.
                            if let Some((c, r)) = grid::parse_address(&self.name_box) {
                                if c < COLS && r < ROWS {
                                    self.selection.collapse_to((c, r));
                                }
                            }
                        } else if !response.has_focus() {
                            self.name_box = addr.clone();
                        }
                    }
                    ActiveSheet::Hex => {
                        ui.monospace(addr.clone());
                    }
                }
                match self.active {
                    ActiveSheet::Square if self.selection.is_range() => {
                        let (cols, rows) = self.selection.dimensions();
                        ui.label(egui::RichText::new(format!("{cols}C × {rows}R")).weak());
                    }
                    ActiveSheet::Hex if self.hex_selection.is_range() => {
                        let (q, r) = self.hex_selection.dimensions();
                        ui.label(egui::RichText::new(format!("{q}q × {r}r")).weak());
                    }
                    _ => {}
                }
                ui.separator();
                let width = ui.available_width();
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.formula_bar)
                        .desired_width(width)
                        .font(egui::TextStyle::Monospace)
                        .hint_text("value or =formula"),
                );
                if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    // Enter commits the bar to the active cell.
                    self.edit = None;
                    let (sheet, addr) = self.active_target();
                    let new_source = commit_source(&self.formula_bar);
                    self.apply_edits(sheet, vec![(addr, new_source)]);
                } else if !response.has_focus() {
                    // Not being edited — mirror the active cell's source.
                    self.formula_bar = source;
                }
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
                    if self.find_open {
                        self.refresh_find();
                    }
                }
                if ui
                    .selectable_label(self.active == ActiveSheet::Hex, "Hex demo")
                    .clicked()
                {
                    self.commit_edit();
                    self.active = ActiveSheet::Hex;
                    if self.find_open {
                        self.refresh_find();
                    }
                }
                // Selection statistics, pushed to the right edge.
                let stats = stats::selection_stats(&self.selection_values());
                if stats.nonempty > 0 {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let text = if stats.count > 0 {
                            let avg = stats.average.map(format_number).unwrap_or_default();
                            let min = stats.min.map(format_number).unwrap_or_default();
                            let max = stats.max.map(format_number).unwrap_or_default();
                            format!(
                                "Sum {}     Avg {}     Min {}     Max {}     Count {}",
                                format_number(stats.sum),
                                avg,
                                min,
                                max,
                                stats.nonempty,
                            )
                        } else {
                            format!("Count {}", stats.nonempty)
                        };
                        ui.label(egui::RichText::new(text).weak());
                    });
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

        self.conditional_window(ctx);
        self.find_window(ctx);
        self.help_window(ctx);
        self.note_window(ctx);
    }
}

/// Draw a square cell's value with its formatting — colour, alignment,
/// italic, and faux-bold (a second offset pass, since egui's default font
/// has no bold weight).
/// A small triangle in a cell's top-right corner, marking that the cell
/// carries a note.
fn draw_note_marker(painter: &egui::Painter, rect: egui::Rect) {
    let size = 7.0;
    let tr = rect.right_top();
    let marker = vec![
        egui::pos2(tr.x - size, tr.y),
        egui::pos2(tr.x, tr.y),
        egui::pos2(tr.x, tr.y + size),
    ];
    painter.add(egui::Shape::convex_polygon(
        marker,
        egui::Color32::from_rgb(220, 90, 70),
        egui::Stroke::NONE,
    ));
}

fn draw_cell_text(
    painter: &egui::Painter,
    text: &str,
    rect: egui::Rect,
    fmt: &CellFormat,
    numeric: bool,
    is_negative: bool,
    default_color: egui::Color32,
) {
    let color = format::effective_text_color(fmt, is_negative, default_color);
    let pad = 5.0;
    let mut job = egui::text::LayoutJob::default();
    if fmt.wrap_text {
        job.wrap.max_width = (rect.width() - 2.0 * pad).max(0.0);
    }
    job.append(
        text,
        0.0,
        egui::TextFormat {
            font_id: egui::FontId::proportional(fmt.font_size.points()),
            color,
            italics: fmt.italic,
            ..Default::default()
        },
    );
    let galley = painter.layout_job(job);
    let size = galley.size();
    let y = rect.top() + format::vertical_offset(fmt.valign, rect.height(), size.y, pad);
    let x = match format::effective_align(fmt.align, numeric) {
        HAlign::Left | HAlign::Auto => rect.left() + pad,
        HAlign::Center => rect.center().x - size.x / 2.0,
        HAlign::Right => rect.right() - pad - size.x,
    };
    let pos = egui::pos2(x, y);
    painter.galley(pos, galley.clone(), color);
    if fmt.bold {
        painter.galley(pos + egui::vec2(0.55, 0.0), galley, color);
    }
    draw_text_decorations(painter, egui::Rect::from_min_size(pos, size), fmt, color);
}

/// Draw the strikethrough / underline lines for `fmt` across a text's
/// bounding `rect`, in `color`. A no-op when neither decoration is set —
/// shared by the square and hex cell-text drawing.
fn draw_text_decorations(
    painter: &egui::Painter,
    rect: egui::Rect,
    fmt: &CellFormat,
    color: egui::Color32,
) {
    let stroke = egui::Stroke::new(1.0, color);
    if fmt.strikethrough {
        painter.hline(rect.x_range(), rect.center().y, stroke);
    }
    if fmt.underline {
        painter.hline(rect.x_range(), rect.bottom() - 1.0, stroke);
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

/// Whether a hex coord lies within the rendered radius-3 view disc.
fn hex_in_view(coord: HexCoord) -> bool {
    HexCoord::new(0, 0).distance(coord) <= HEX_VIEW_RADIUS
}

/// The hex cell a Ctrl+arrow jump lands on — [`grid::block_jump`] walking
/// `dir` from `start` over axial coordinates, bounded by `in_view`.
fn hex_jump(
    start: HexCoord,
    dir: Dir,
    in_view: impl Fn(HexCoord) -> bool,
    occupied: impl Fn(HexCoord) -> bool,
) -> HexCoord {
    let step = |c: HexCoord| {
        let n = hex_step(c, dir);
        in_view(n).then_some(n)
    };
    grid::block_jump(start, step, occupied)
}

/// The hex AutoSum action for a selection whose inclusive axial corners
/// are `bounds`: the hex directly below the selection's bottom row that
/// should hold the result, and the `=FUNC(...)` formula (`func` is an
/// aggregate name). `None` when that hex falls outside the visible disc.
fn hex_autosum(bounds: ((i32, i32), (i32, i32)), func: &str) -> Option<(HexCoord, String)> {
    let ((min_q, min_r), (max_q, max_r)) = bounds;
    let target = HexCoord::new(min_q, max_r + 1);
    if !hex_in_view(target) {
        return None;
    }
    let range = if (min_q, min_r) == (max_q, max_r) {
        hex_address(HexCoord::new(min_q, min_r))
    } else {
        format!(
            "{}:{}",
            hex_address(HexCoord::new(min_q, min_r)),
            hex_address(HexCoord::new(max_q, max_r)),
        )
    };
    Some((target, format!("={func}({range})")))
}

/// The current hex data region around `cursor` — the axial analogue of
/// [`grid::current_region`]. The `(q, r)` box grows outward to a
/// fixpoint while the row or column just past an edge holds an
/// `occupied` cell within the box's span; `occupied` reports `false`
/// for out-of-view cells, so the grow halts at the disc. `radius`
/// bounds the walk.
fn hex_current_region(
    cursor: (i32, i32),
    radius: i32,
    occupied: impl Fn(i32, i32) -> bool,
) -> ((i32, i32), (i32, i32)) {
    let (mut min_q, mut min_r) = cursor;
    let (mut max_q, mut max_r) = cursor;
    loop {
        let mut grew = false;
        if min_r > -radius && (min_q..=max_q).any(|q| occupied(q, min_r - 1)) {
            min_r -= 1;
            grew = true;
        }
        if max_r < radius && (min_q..=max_q).any(|q| occupied(q, max_r + 1)) {
            max_r += 1;
            grew = true;
        }
        if min_q > -radius && (min_r..=max_r).any(|r| occupied(min_q - 1, r)) {
            min_q -= 1;
            grew = true;
        }
        if max_q < radius && (min_r..=max_r).any(|r| occupied(max_q + 1, r)) {
            max_q += 1;
            grew = true;
        }
        if !grew {
            return ((min_q, min_r), (max_q, max_r));
        }
    }
}

/// Render a cell value with the engine's natural formatting — no number
/// format applied. Used for the hex sheet and as the square sheet's
/// fallback for non-numeric values.
/// A friendly description of a formula error — shown as a tooltip when
/// an errored cell is hovered. `Lang` / `Compile` carry the engine's
/// own message.
fn error_detail(err: &CellError) -> String {
    match err {
        CellError::Ref => "Reference to a missing cell".to_string(),
        CellError::Cycle => "Part of a dependency cycle".to_string(),
        CellError::DivZero => "Division by zero".to_string(),
        CellError::Num => "Numeric overflow or invalid number".to_string(),
        CellError::Value => "Wrong type of value".to_string(),
        CellError::Lang(msg) => format!("Formula error: {msg}"),
        CellError::Compile(msg) => format!("Compile error: {msg}"),
        CellError::Timeout => "Formula timed out".to_string(),
        CellError::Spill => "Spill range is blocked".to_string(),
        CellError::StaleFunction => "Stored function needs recalculation".to_string(),
    }
}

/// The Excel-style short code for a formula error — what an errored
/// cell displays.
fn error_code(err: &CellError) -> &'static str {
    match err {
        CellError::Ref => "#REF!",
        CellError::Cycle => "#CYCLE!",
        CellError::DivZero => "#DIV/0!",
        CellError::Num => "#NUM!",
        CellError::Value => "#VALUE!",
        CellError::Lang(_) => "#PARSE!",
        CellError::Compile(_) => "#COMPILE!",
        CellError::Timeout => "#TIMEOUT!",
        CellError::Spill => "#SPILL!",
        CellError::StaleFunction => "#STALE!",
    }
}

fn natural_text(value: CellValue) -> String {
    match value {
        CellValue::Empty => String::new(),
        CellValue::Number(n) => format_number(n),
        CellValue::Integer(i) => i.to_string(),
        CellValue::Bool(b) => if b { "TRUE" } else { "FALSE" }.to_string(),
        CellValue::Text(t) => t,
        CellValue::Error(e) => error_code(&e).to_string(),
        CellValue::Array(_) => "{array}".to_string(),
        _ => String::new(),
    }
}

/// A short human description of a condition, for the rule editor's list.
fn describe_condition(condition: &Condition) -> String {
    match condition {
        Condition::GreaterThan(t) => format!("value > {}", format_number(*t)),
        Condition::GreaterOrEqual(t) => format!("value >= {}", format_number(*t)),
        Condition::LessThan(t) => format!("value < {}", format_number(*t)),
        Condition::LessOrEqual(t) => format!("value <= {}", format_number(*t)),
        Condition::EqualTo(t) => format!("value = {}", format_number(*t)),
        Condition::NotEqualTo(t) => format!("value != {}", format_number(*t)),
        Condition::Between(a, b) => format!(
            "value in {}..{}",
            format_number(a.min(*b)),
            format_number(a.max(*b)),
        ),
        Condition::Contains(s) => format!("text contains \"{s}\""),
        Condition::IsTrue => "value is TRUE".to_string(),
        Condition::IsFalse => "value is FALSE".to_string(),
        Condition::NonEmpty => "cell is non-empty".to_string(),
        Condition::IsEmpty => "cell is empty".to_string(),
    }
}

/// The source to write for an edit buffer — `None` (a cleared cell) when
/// the buffer is empty or all whitespace, otherwise the buffer verbatim.
/// Shared by the in-cell editor and the formula bar.
fn commit_source(buffer: &str) -> Option<String> {
    if buffer.trim().is_empty() {
        None
    } else {
        Some(buffer.to_string())
    }
}

/// Draw a cell's set border sides as line segments over the grid line.
fn draw_borders(
    painter: &egui::Painter,
    rect: egui::Rect,
    borders: &Borders,
    stroke: egui::Stroke,
) {
    if borders.top {
        painter.line_segment([rect.left_top(), rect.right_top()], stroke);
    }
    if borders.bottom {
        painter.line_segment([rect.left_bottom(), rect.right_bottom()], stroke);
    }
    if borders.left {
        painter.line_segment([rect.left_top(), rect.left_bottom()], stroke);
    }
    if borders.right {
        painter.line_segment([rect.right_top(), rect.right_bottom()], stroke);
    }
}

/// The anchor and position to draw a hex cell's text at, given its
/// horizontal alignment. Left/Right anchor `inset` points either side of
/// the `centroid`; Center sits on it.
fn hex_text_layout(align: HAlign, centroid: egui::Pos2, inset: f32) -> (egui::Align2, egui::Pos2) {
    match align {
        HAlign::Left | HAlign::Auto => (
            egui::Align2::LEFT_CENTER,
            egui::pos2(centroid.x - inset, centroid.y),
        ),
        HAlign::Center => (egui::Align2::CENTER_CENTER, centroid),
        HAlign::Right => (
            egui::Align2::RIGHT_CENTER,
            egui::pos2(centroid.x + inset, centroid.y),
        ),
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

/// Collapse duplicate addresses in an edit batch so one undo step never
/// holds two edits for the same cell — the last write for an address
/// wins (e.g. a paste landing on a just-cleared cut cell).
fn dedup_targets(targets: Vec<(String, Option<String>)>) -> Vec<(String, Option<String>)> {
    let mut out: Vec<(String, Option<String>)> = Vec::new();
    for (addr, after) in targets {
        if let Some(slot) = out.iter_mut().find(|(a, _)| *a == addr) {
            slot.1 = after;
        } else {
            out.push((addr, after));
        }
    }
    out
}

/// The new state for a range toggle: off when every cell already has the
/// flag, on otherwise — Excel's bold/italic-over-a-range rule.
fn toggle_target(mut currently_on: impl Iterator<Item = bool>) -> bool {
    !currently_on.all(|on| on)
}

/// Merge two consecutive format-edit batches over the same cells into
/// one — the earliest `before` and the latest `after` per cell, so a
/// coalesced gesture undoes to its true starting state.
fn merge_format_edits(prev: Vec<FormatEdit>, new: Vec<FormatEdit>) -> Vec<FormatEdit> {
    new.into_iter()
        .map(|n| {
            let before = prev
                .iter()
                .find(|p| p.cell == n.cell)
                .map(|p| p.before.clone())
                .unwrap_or_else(|| n.before.clone());
            FormatEdit {
                cell: n.cell,
                before,
                after: n.after,
            }
        })
        .collect()
}

/// Merge two consecutive hex format-edit batches over the same cells —
/// earliest `before`, latest `after` per cell. The hex twin of
/// [`merge_format_edits`].
fn merge_hex_format_edits(prev: Vec<HexFormatEdit>, new: Vec<HexFormatEdit>) -> Vec<HexFormatEdit> {
    new.into_iter()
        .map(|n| {
            let before = prev
                .iter()
                .find(|p| p.cell == n.cell)
                .map(|p| p.before.clone())
                .unwrap_or_else(|| n.before.clone());
            HexFormatEdit {
                cell: n.cell,
                before,
                after: n.after,
            }
        })
        .collect()
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
    fn error_code_maps_to_excel_style_codes() {
        assert_eq!(error_code(&CellError::DivZero), "#DIV/0!");
        assert_eq!(error_code(&CellError::Ref), "#REF!");
        assert_eq!(error_code(&CellError::Value), "#VALUE!");
        assert_eq!(error_code(&CellError::Num), "#NUM!");
        assert_eq!(error_code(&CellError::Lang("bad token".into())), "#PARSE!");
        // The mapping flows through natural_text.
        assert_eq!(
            natural_text(CellValue::Error(CellError::DivZero)),
            "#DIV/0!",
        );
    }

    #[test]
    fn error_detail_describes_each_error() {
        assert_eq!(error_detail(&CellError::DivZero), "Division by zero");
        assert!(error_detail(&CellError::Ref).contains("missing cell"));
        // Lang / Compile carry the engine's own message.
        assert!(error_detail(&CellError::Lang("unexpected }".into())).contains("unexpected }"));
        assert!(error_detail(&CellError::Compile("E0382".into())).contains("E0382"));
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

    #[test]
    fn hex_jump_walks_to_a_run_far_end() {
        // A run along the q-axis at r = 0.
        let run = [
            HexCoord::new(0, 0),
            HexCoord::new(1, 0),
            HexCoord::new(2, 0),
        ];
        let target = hex_jump(HexCoord::new(0, 0), Dir::Right, hex_in_view, |c| {
            run.contains(&c)
        });
        // From the run's start, jump to its far end.
        assert_eq!(target, HexCoord::new(2, 0));
    }

    #[test]
    fn hex_jump_skips_a_gap_to_the_next_content() {
        // H(0,0) occupied, a gap, then H(3,0) occupied.
        let content = [HexCoord::new(0, 0), HexCoord::new(3, 0)];
        let target = hex_jump(HexCoord::new(0, 0), Dir::Right, hex_in_view, |c| {
            content.contains(&c)
        });
        assert_eq!(target, HexCoord::new(3, 0));
    }

    #[test]
    fn hex_jump_runs_to_the_view_edge_when_empty() {
        // No content — jump to the last in-view cell along the axis.
        let target = hex_jump(HexCoord::new(0, 0), Dir::Right, hex_in_view, |_| false);
        assert_eq!(target, HexCoord::new(3, 0));
        assert!(hex_in_view(target));
        assert!(!hex_in_view(hex_step(target, Dir::Right)));
    }

    #[test]
    fn hex_jump_at_the_view_edge_stays_put() {
        // H(3,0) is the last in-view cell going right; jumping right stays.
        let target = hex_jump(HexCoord::new(3, 0), Dir::Right, hex_in_view, |_| true);
        assert_eq!(target, HexCoord::new(3, 0));
    }

    #[test]
    fn hex_current_region_grows_to_the_block() {
        // A 2x2 axial block at (0,0)..(1,1).
        let filled = |q, r| (0..=1).contains(&q) && (0..=1).contains(&r);
        assert_eq!(hex_current_region((0, 0), 3, filled), ((0, 0), (1, 1)));
        assert_eq!(hex_current_region((1, 1), 3, filled), ((0, 0), (1, 1)));
    }

    #[test]
    fn hex_current_region_isolated_cell() {
        assert_eq!(
            hex_current_region((0, 0), 3, |_, _| false),
            ((0, 0), (0, 0)),
        );
    }

    #[test]
    fn hex_current_region_clamps_to_the_radius() {
        // Everything occupied — the grow stops at the radius bound.
        assert_eq!(
            hex_current_region((0, 0), 3, |_, _| true),
            ((-3, -3), (3, 3)),
        );
    }

    #[test]
    fn cond_draft_round_trips_through_a_rule() {
        // A threshold rule recovers its kind, threshold, and effects.
        let draft = CondDraft {
            kind: CondKind::Less,
            threshold: "42.5".to_string(),
            bold: true,
            italic: true,
            strikethrough: true,
            underline: true,
            text_color_on: true,
            ..CondDraft::default()
        };
        let back = CondDraft::from_rule(&draft.build().unwrap());
        assert_eq!(back.kind, CondKind::Less);
        assert_eq!(back.threshold, "42.5");
        assert!(back.bold);
        assert!(back.italic && back.strikethrough && back.underline);
        assert!(back.text_color_on);
        // A no-threshold condition recovers its kind.
        let rule = CondDraft {
            kind: CondKind::IsEmpty,
            ..CondDraft::default()
        }
        .build()
        .unwrap();
        assert_eq!(CondDraft::from_rule(&rule).kind, CondKind::IsEmpty);
        // Between round-trips both of its bounds.
        let between = CondDraft {
            kind: CondKind::Between,
            threshold: "10".to_string(),
            threshold2: "20".to_string(),
            ..CondDraft::default()
        };
        let back = CondDraft::from_rule(&between.build().unwrap());
        assert_eq!(back.kind, CondKind::Between);
        assert_eq!(back.threshold, "10");
        assert_eq!(back.threshold2, "20");
    }

    #[test]
    fn hex_autosum_sums_into_the_hex_below() {
        // The column H(0,-1):H(0,1) totals into H(0,2).
        assert_eq!(
            hex_autosum(((0, -1), (0, 1)), "SUM"),
            Some((HexCoord::new(0, 2), "=SUM(H(0,-1):H(0,1))".to_string())),
        );
    }

    #[test]
    fn hex_autosum_single_cell_and_chosen_function() {
        assert_eq!(
            hex_autosum(((1, 0), (1, 0)), "SUM"),
            Some((HexCoord::new(1, 1), "=SUM(H(1,0))".to_string())),
        );
        // The chosen aggregate name is used verbatim.
        assert_eq!(
            hex_autosum(((0, -1), (0, 1)), "AVERAGE"),
            Some((HexCoord::new(0, 2), "=AVERAGE(H(0,-1):H(0,1))".to_string())),
        );
    }

    #[test]
    fn hex_autosum_is_none_outside_the_view() {
        // The total hex would land past the radius-3 view disc.
        assert_eq!(hex_autosum(((0, 2), (0, 3)), "SUM"), None);
    }

    #[test]
    fn dedup_targets_keeps_the_last_write_per_address() {
        let targets = vec![
            ("A1".to_string(), None),
            ("B1".to_string(), Some("x".to_string())),
            ("A1".to_string(), Some("final".to_string())),
        ];
        let out = dedup_targets(targets);
        assert_eq!(out.len(), 2);
        let a1 = out.iter().find(|(a, _)| a == "A1").unwrap();
        assert_eq!(a1.1, Some("final".to_string()));
    }

    #[test]
    fn dedup_targets_leaves_distinct_addresses_alone() {
        let targets = vec![
            ("A1".to_string(), Some("a".to_string())),
            ("B2".to_string(), None),
        ];
        assert_eq!(dedup_targets(targets).len(), 2);
    }

    #[test]
    fn toggle_target_turns_off_only_when_every_cell_is_on() {
        // Every cell already has the flag — the toggle clears it.
        assert!(!toggle_target([true, true, true].into_iter()));
        // A mix, or all-off — the toggle sets it for the whole range.
        assert!(toggle_target([true, false, true].into_iter()));
        assert!(toggle_target([false, false].into_iter()));
    }

    #[test]
    fn merge_format_edits_keeps_oldest_before_and_newest_after() {
        let cell = (1, 1);
        let red = CellFormat {
            text_color: Some(egui::Color32::RED),
            ..CellFormat::default()
        };
        let blue = CellFormat {
            text_color: Some(egui::Color32::BLUE),
            ..CellFormat::default()
        };
        // Frame 1 changed default -> red; frame 2 changed red -> blue.
        let prev = vec![FormatEdit {
            cell,
            before: CellFormat::default(),
            after: red.clone(),
        }];
        let new = vec![FormatEdit {
            cell,
            before: red,
            after: blue.clone(),
        }];
        let merged = merge_format_edits(prev, new);
        assert_eq!(merged.len(), 1);
        // The merged edit spans the whole gesture: default -> blue.
        assert_eq!(merged[0].before, CellFormat::default());
        assert_eq!(merged[0].after, blue);
    }

    #[test]
    fn merge_hex_format_edits_keeps_oldest_before_and_newest_after() {
        let cell = HexCoord::new(2, -1);
        let red = CellFormat {
            text_color: Some(egui::Color32::RED),
            ..CellFormat::default()
        };
        let blue = CellFormat {
            text_color: Some(egui::Color32::BLUE),
            ..CellFormat::default()
        };
        let prev = vec![HexFormatEdit {
            cell,
            before: CellFormat::default(),
            after: red.clone(),
        }];
        let new = vec![HexFormatEdit {
            cell,
            before: red,
            after: blue.clone(),
        }];
        let merged = merge_hex_format_edits(prev, new);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].before, CellFormat::default());
        assert_eq!(merged[0].after, blue);
    }

    #[test]
    fn hex_text_layout_anchors_by_alignment() {
        let c = egui::pos2(100.0, 50.0);
        let (anchor, pos) = hex_text_layout(HAlign::Center, c, 10.0);
        assert_eq!(anchor, egui::Align2::CENTER_CENTER);
        assert_eq!(pos, c);
        let (anchor, pos) = hex_text_layout(HAlign::Left, c, 10.0);
        assert_eq!(anchor, egui::Align2::LEFT_CENTER);
        assert_eq!(pos, egui::pos2(90.0, 50.0));
        let (anchor, pos) = hex_text_layout(HAlign::Right, c, 10.0);
        assert_eq!(anchor, egui::Align2::RIGHT_CENTER);
        assert_eq!(pos, egui::pos2(110.0, 50.0));
    }

    #[test]
    fn commit_source_clears_on_blank_keeps_content() {
        assert_eq!(
            commit_source("=SUM(A1:A3)"),
            Some("=SUM(A1:A3)".to_string())
        );
        assert_eq!(commit_source("42"), Some("42".to_string()));
        // Empty or all-whitespace clears the cell.
        assert_eq!(commit_source(""), None);
        assert_eq!(commit_source("   "), None);
        // Content with surrounding spaces is kept verbatim.
        assert_eq!(commit_source("  hi  "), Some("  hi  ".to_string()));
    }
}
