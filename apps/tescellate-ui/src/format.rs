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

/// The full visual format of one cell. The `Default` is "no formatting".
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CellFormat {
    pub bold: bool,
    pub italic: bool,
    pub align: HAlign,
    pub text_color: Option<Color32>,
    pub fill: Option<Color32>,
    pub number: NumberFormat,
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
}
