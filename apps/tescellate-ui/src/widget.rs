//! Interactive cell widgets — a square-sheet cell can render as
//! something other than text/numbers/formula. Today: a clickable
//! checkbox or a draggable horizontal slider. Future kinds (button,
//! switch, progress bar) plug in as new [`WidgetKind`] variants.
//!
//! Each widget reads its current value from the cell's evaluated
//! [`CellValue`] and writes back a re-typeable literal source string
//! when the user interacts. That keeps widgets first-class with the
//! formula engine — a slider whose underlying cell holds `=42 + 8`
//! still renders at 50, and dragging the thumb replaces the formula
//! with the new literal.
//!
//! No egui and no engine here, so `cargo test` covers the module.

use std::collections::HashMap;

use tescellate_core::CellValue;

/// The kind of widget a cell renders as. Cells with no entry in
/// [`Widgets`] render as ordinary text/number/formula cells.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WidgetKind {
    /// A boolean checkbox; checked iff the cell's value is `TRUE`.
    Toggle,
    /// A horizontal slider clamped to `[min, max]`; the thumb reads
    /// the cell's numeric value and a drag rewrites it.
    Slider { min: f64, max: f64 },
    /// A clickable action button. Clicking re-fires the cell's
    /// current source through the engine, so a cell holding a
    /// non-deterministic expression (`RAND`, `NOW`, …) recomputes
    /// to a fresh value on each press. Cells with literal sources
    /// re-eval to the same value — effectively a no-op.
    Button,
}

impl WidgetKind {
    /// The default slider range, used by `Widgets::set_slider_default`
    /// and the ribbon's "Make slider" action.
    pub const DEFAULT_SLIDER: WidgetKind = WidgetKind::Slider {
        min: 0.0,
        max: 100.0,
    };
}

/// Map of square-sheet cells to the widget they render as.
#[derive(Debug, Clone, Default)]
pub struct Widgets {
    cells: HashMap<(u32, u32), WidgetKind>,
}

impl Widgets {
    /// The widget kind for `cell`, if any.
    pub fn kind(&self, cell: (u32, u32)) -> Option<WidgetKind> {
        self.cells.get(&cell).copied()
    }

    /// Whether `cell` is any widget (checkbox, slider, …).
    pub fn is_widget(&self, cell: (u32, u32)) -> bool {
        self.cells.contains_key(&cell)
    }

    /// Whether `cell` is a checkbox toggle.
    pub fn is_toggle(&self, cell: (u32, u32)) -> bool {
        matches!(self.kind(cell), Some(WidgetKind::Toggle))
    }

    /// Whether `cell` is a slider.
    pub fn is_slider(&self, cell: (u32, u32)) -> bool {
        matches!(self.kind(cell), Some(WidgetKind::Slider { .. }))
    }

    /// Whether `cell` is a clickable button.
    pub fn is_button(&self, cell: (u32, u32)) -> bool {
        matches!(self.kind(cell), Some(WidgetKind::Button))
    }

    /// Set (or clear, with `kind = None`) the widget on `cell`.
    pub fn set(&mut self, cell: (u32, u32), kind: Option<WidgetKind>) {
        match kind {
            Some(k) => {
                self.cells.insert(cell, k);
            }
            None => {
                self.cells.remove(&cell);
            }
        }
    }

    /// Convenience: turn `cell` into a checkbox (`on`) or remove its
    /// widget treatment entirely (`!on`).
    pub fn set_toggle(&mut self, cell: (u32, u32), on: bool) {
        if on {
            self.set(cell, Some(WidgetKind::Toggle));
        } else {
            self.set(cell, None);
        }
    }

    /// Convenience: turn `cell` into a slider with the default range,
    /// or clear it (`!on`).
    pub fn set_slider_default(&mut self, cell: (u32, u32), on: bool) {
        if on {
            self.set(cell, Some(WidgetKind::DEFAULT_SLIDER));
        } else {
            self.set(cell, None);
        }
    }

    /// Set a slider on `cell` with an explicit `[min, max]` range.
    /// Replaces any prior widget kind on the cell.
    pub fn set_slider(&mut self, cell: (u32, u32), min: f64, max: f64) {
        self.set(cell, Some(WidgetKind::Slider { min, max }));
    }

    /// Convenience: turn `cell` into a clickable button (`on`) or
    /// clear its widget treatment (`!on`).
    pub fn set_button(&mut self, cell: (u32, u32), on: bool) {
        if on {
            self.set(cell, Some(WidgetKind::Button));
        } else {
            self.set(cell, None);
        }
    }

    /// Whether no cell carries a widget — lets the renderer skip its
    /// widget pass entirely.
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    /// How many cells carry a widget.
    pub fn count(&self) -> usize {
        self.cells.len()
    }
}

/// A toggle cell's checkbox state — checked only when the cell currently
/// holds boolean `TRUE`. Every other value (a number, text, empty, …)
/// reads as unchecked.
pub fn bool_state(value: &CellValue) -> bool {
    matches!(value, CellValue::Bool(true))
}

/// The source to write for a checkbox state — a literal the formula
/// engine parses straight back to a boolean.
pub fn bool_source(checked: bool) -> &'static str {
    if checked {
        "TRUE"
    } else {
        "FALSE"
    }
}

/// Read a slider cell's current numeric position from its evaluated
/// [`CellValue`]. Non-numeric values default to `min` so a brand-new
/// slider cell starts at the bottom of its range.
pub fn slider_value(value: &CellValue, min: f64, max: f64) -> f64 {
    let raw = match value {
        CellValue::Number(n) => *n,
        CellValue::Integer(i) => *i as f64,
        _ => min,
    };
    raw.clamp(min, max)
}

/// The source to write for a slider position. Integer-valued positions
/// land as integer literals (`5`), so the cell's number format reads
/// cleanly; otherwise the float is written with `{}` precision.
pub fn slider_source(value: f64) -> String {
    if value.is_finite() && value.fract().abs() < 1e-9 {
        format!("{}", value.round() as i64)
    } else {
        format!("{value}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_and_query_widget_cells() {
        let mut w = Widgets::default();
        assert!(w.is_empty());
        w.set_toggle((2, 3), true);
        w.set_slider_default((5, 1), true);
        assert!(w.is_toggle((2, 3)));
        assert!(w.is_slider((5, 1)));
        assert!(!w.is_toggle((5, 1)));
        assert!(!w.is_slider((2, 3)));
        assert_eq!(w.count(), 2);
        assert!(!w.is_empty());
        // Clearing a cell drops both kinds.
        w.set((2, 3), None);
        assert!(!w.is_widget((2, 3)));
        assert_eq!(w.count(), 1);
    }

    #[test]
    fn set_replaces_the_existing_kind() {
        let mut w = Widgets::default();
        w.set_toggle((0, 0), true);
        // Converting a checkbox to a slider doesn't double-count it.
        w.set_slider_default((0, 0), true);
        assert!(!w.is_toggle((0, 0)));
        assert!(w.is_slider((0, 0)));
        assert_eq!(w.count(), 1);
    }

    #[test]
    fn bool_state_is_true_only_for_boolean_true() {
        assert!(bool_state(&CellValue::Bool(true)));
        assert!(!bool_state(&CellValue::Bool(false)));
        assert!(!bool_state(&CellValue::Empty));
        assert!(!bool_state(&CellValue::Number(1.0)));
        assert!(!bool_state(&CellValue::Integer(1)));
        assert!(!bool_state(&CellValue::Text("TRUE".into())));
    }

    #[test]
    fn bool_source_round_trips_the_checkbox_state() {
        assert_eq!(bool_source(true), "TRUE");
        assert_eq!(bool_source(false), "FALSE");
    }

    #[test]
    fn slider_value_clamps_to_range() {
        assert_eq!(slider_value(&CellValue::Number(50.0), 0.0, 100.0), 50.0);
        // Below the bottom clamps up.
        assert_eq!(slider_value(&CellValue::Number(-10.0), 0.0, 100.0), 0.0);
        // Above the top clamps down.
        assert_eq!(slider_value(&CellValue::Number(150.0), 0.0, 100.0), 100.0);
        // An integer reads as its numeric value.
        assert_eq!(slider_value(&CellValue::Integer(42), 0.0, 100.0), 42.0);
        // A non-numeric value falls back to `min`.
        assert_eq!(slider_value(&CellValue::Empty, 10.0, 90.0), 10.0);
        assert_eq!(
            slider_value(&CellValue::Text("hi".into()), 10.0, 90.0),
            10.0
        );
    }

    #[test]
    fn set_and_query_button() {
        let mut w = Widgets::default();
        w.set_button((1, 1), true);
        assert!(w.is_button((1, 1)));
        assert!(!w.is_toggle((1, 1)));
        assert!(!w.is_slider((1, 1)));
        assert!(w.is_widget((1, 1)));
        w.set_button((1, 1), false);
        assert!(!w.is_widget((1, 1)));
    }

    #[test]
    fn slider_source_writes_an_integer_literal_when_possible() {
        assert_eq!(slider_source(0.0), "0");
        assert_eq!(slider_source(50.0), "50");
        assert_eq!(slider_source(-3.0), "-3");
        // Fractional values round-trip via the float representation.
        let mixed = slider_source(2.5);
        // Either "2.5" or with platform float formatting — accept the
        // round-trip rather than a brittle byte-equal.
        assert_eq!(mixed.parse::<f64>().unwrap(), 2.5);
    }
}
