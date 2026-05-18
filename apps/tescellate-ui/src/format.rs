//! The pure cell-formatting model — visual style and number formats.
//!
//! No egui rendering and no engine: just the data and the number-format
//! logic, so it is unit-tested with ordinary `cargo test`. `app.rs` reads
//! a [`CellFormat`] when painting a cell and through [`render_number`]
//! when turning a value into display text.

use std::collections::HashMap;

use egui::Color32;

/// Horizontal text alignment within a cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HAlign {
    #[default]
    Left,
    Center,
    Right,
}

/// How a numeric cell value is rendered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NumberFormat {
    /// The engine's natural rendering.
    #[default]
    General,
    /// A fixed number of decimal places.
    Number { decimals: u8 },
    /// A percentage — the value times 100, with a `%` suffix.
    Percent { decimals: u8 },
    /// A `$` prefix and two decimal places.
    Currency,
}

/// Which sides of a cell carry a border line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Borders {
    pub top: bool,
    pub bottom: bool,
    pub left: bool,
    pub right: bool,
}

/// A border command from the ribbon, applied across the selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorderMode {
    /// Border all four sides of every selected cell.
    All,
    /// Border only the sides on the selection's outer edge.
    Outer,
    /// Clear every border.
    None,
}

/// Which sides of `cell` get a border under `mode`, given the selection's
/// `bounds` — its min and max `(col, row)` corner. `All` borders every
/// side; `None` clears them; `Outer` borders only the sides lying on the
/// selection's edge (so a one-cell selection gets all four).
pub fn border_sides(
    cell: (u32, u32),
    bounds: ((u32, u32), (u32, u32)),
    mode: BorderMode,
) -> Borders {
    match mode {
        BorderMode::None => Borders::default(),
        BorderMode::All => Borders {
            top: true,
            bottom: true,
            left: true,
            right: true,
        },
        BorderMode::Outer => {
            let ((min_c, min_r), (max_c, max_r)) = bounds;
            let (c, r) = cell;
            Borders {
                top: r == min_r,
                bottom: r == max_r,
                left: c == min_c,
                right: c == max_c,
            }
        }
    }
}

/// The full visual format of one cell. The `Default` is "no formatting".
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CellFormat {
    pub bold: bool,
    pub italic: bool,
    pub align: HAlign,
    pub text_color: Option<Color32>,
    pub fill: Option<Color32>,
    pub number: NumberFormat,
    pub borders: Borders,
}

impl CellFormat {
    /// Whether this format carries no styling at all.
    pub fn is_default(&self) -> bool {
        *self == CellFormat::default()
    }
}

/// Render a numeric value under a number format. `None` means "use the
/// engine's natural rendering" — returned for [`NumberFormat::General`].
pub fn render_number(value: f64, format: NumberFormat) -> Option<String> {
    match format {
        NumberFormat::General => None,
        NumberFormat::Number { decimals } => Some(format!("{:.*}", decimals as usize, value)),
        NumberFormat::Percent { decimals } => {
            Some(format!("{:.*}%", decimals as usize, value * 100.0))
        }
        NumberFormat::Currency => Some(format!("${:.2}", value)),
    }
}

/// Per-cell formatting overrides. A cell absent from the map is unstyled,
/// so an empty `FormatMap` is a plain sheet.
#[derive(Debug, Clone, Default)]
pub struct FormatMap {
    formats: HashMap<(u32, u32), CellFormat>,
}

impl FormatMap {
    pub fn new() -> Self {
        Self::default()
    }

    /// The format of a cell — its default if never styled.
    pub fn get(&self, cell: (u32, u32)) -> CellFormat {
        self.formats.get(&cell).cloned().unwrap_or_default()
    }

    /// Mutate a cell's format in place. An entry that ends up back at the
    /// default is dropped, so the map only ever holds real styling.
    pub fn update(&mut self, cell: (u32, u32), edit: impl FnOnce(&mut CellFormat)) {
        let mut format = self.get(cell);
        edit(&mut format);
        if format.is_default() {
            self.formats.remove(&cell);
        } else {
            self.formats.insert(cell, format);
        }
    }

    /// How many cells carry non-default formatting.
    pub fn styled_count(&self) -> usize {
        self.formats.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn general_defers_to_natural_rendering() {
        assert_eq!(render_number(3.5, NumberFormat::General), None);
    }

    #[test]
    fn number_format_fixes_the_decimal_places() {
        assert_eq!(
            render_number(3.5, NumberFormat::Number { decimals: 2 }),
            Some("3.50".to_string()),
        );
        assert_eq!(
            render_number(3.7, NumberFormat::Number { decimals: 0 }),
            Some("4".to_string()),
        );
    }

    #[test]
    fn percent_scales_by_one_hundred() {
        assert_eq!(
            render_number(0.5, NumberFormat::Percent { decimals: 0 }),
            Some("50%".to_string()),
        );
        assert_eq!(
            render_number(0.125, NumberFormat::Percent { decimals: 1 }),
            Some("12.5%".to_string()),
        );
    }

    #[test]
    fn currency_prefixes_a_dollar_sign() {
        assert_eq!(
            render_number(1200.0, NumberFormat::Currency),
            Some("$1200.00".to_string()),
        );
    }

    #[test]
    fn cell_format_default_is_unstyled() {
        assert!(CellFormat::default().is_default());
        let f = CellFormat {
            bold: true,
            ..Default::default()
        };
        assert!(!f.is_default());
    }

    #[test]
    fn format_map_stores_and_evicts() {
        let mut map = FormatMap::new();
        assert_eq!(map.get((0, 0)), CellFormat::default());

        map.update((2, 3), |f| f.bold = true);
        assert!(map.get((2, 3)).bold);
        assert_eq!(map.styled_count(), 1);

        // Toggling back to the default drops the entry.
        map.update((2, 3), |f| f.bold = false);
        assert!(map.get((2, 3)).is_default());
        assert_eq!(map.styled_count(), 0);
    }

    #[test]
    fn format_map_update_composes() {
        let mut map = FormatMap::new();
        map.update((1, 1), |f| f.italic = true);
        map.update((1, 1), |f| f.align = HAlign::Right);
        let f = map.get((1, 1));
        assert!(f.italic);
        assert_eq!(f.align, HAlign::Right);
    }

    #[test]
    fn border_sides_all_and_none() {
        let bounds = ((1, 1), (3, 3));
        let all = border_sides((2, 2), bounds, BorderMode::All);
        assert!(all.top && all.bottom && all.left && all.right);
        assert_eq!(
            border_sides((2, 2), bounds, BorderMode::None),
            Borders::default(),
        );
    }

    #[test]
    fn outer_borders_trace_the_selection_edge() {
        let bounds = ((1, 1), (3, 3));
        // Top-left corner — top and left only.
        let tl = border_sides((1, 1), bounds, BorderMode::Outer);
        assert!(tl.top && tl.left && !tl.bottom && !tl.right);
        // Bottom-right corner — bottom and right only.
        let br = border_sides((3, 3), bounds, BorderMode::Outer);
        assert!(br.bottom && br.right && !br.top && !br.left);
        // A top-edge, non-corner cell — top only.
        let top = border_sides((2, 1), bounds, BorderMode::Outer);
        assert!(top.top && !top.bottom && !top.left && !top.right);
        // An interior cell — no borders.
        assert_eq!(
            border_sides((2, 2), bounds, BorderMode::Outer),
            Borders::default(),
        );
    }

    #[test]
    fn outer_borders_on_a_single_cell_are_all_four() {
        let only = border_sides((5, 5), ((5, 5), (5, 5)), BorderMode::Outer);
        assert!(only.top && only.bottom && only.left && only.right);
    }
}
