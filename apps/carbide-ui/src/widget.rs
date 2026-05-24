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

use serde::{Deserialize, Serialize};
use carbide_core::CellValue;

/// The kind of widget a cell renders as. Cells with no entry in
/// [`Widgets`] render as ordinary text/number/formula cells.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
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
    /// A read-only horizontal progress bar clamped to `[0, max]`.
    /// Reads the cell's numeric value; never writes back. Pairs
    /// with a Slider on a related cell when "edit input here,
    /// display progress there" is the gesture the user wants.
    ProgressBar { max: f64 },
}

impl WidgetKind {
    /// The default slider range, used by `Widgets::set_slider_default`
    /// and the ribbon's "Make slider" action.
    pub const DEFAULT_SLIDER: WidgetKind = WidgetKind::Slider {
        min: 0.0,
        max: 100.0,
    };
}

/// Map of cells to the widget they render as. Generic over the lattice
/// coordinate `K` — the square sheet uses `Widgets<(u32, u32)>` and the
/// hex sheet uses `Widgets<HexCoord>`. Mirrors the same lattice-generic
/// pattern as [`crate::format::FormatMap`] and [`crate::note::NoteMap`].
///
/// Serialized as a `Vec<(K, WidgetKind)>` because hash-map keys must be
/// strings in JSON — the on-disk form is `[[key, kind], …]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    from = "WidgetsRepr<K>",
    into = "WidgetsRepr<K>",
    bound(
        serialize = "K: Serialize + Eq + std::hash::Hash + Copy",
        deserialize = "K: Deserialize<'de> + Eq + std::hash::Hash + Copy"
    )
)]
pub struct Widgets<K: Eq + std::hash::Hash + Copy> {
    cells: HashMap<K, WidgetKind>,
}

impl<K: Eq + std::hash::Hash + Copy> Default for Widgets<K> {
    fn default() -> Self {
        Self {
            cells: HashMap::new(),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(bound(
    serialize = "K: Serialize",
    deserialize = "K: Deserialize<'de> + Eq + std::hash::Hash"
))]
struct WidgetsRepr<K> {
    cells: Vec<(K, WidgetKind)>,
}

impl<K: Eq + std::hash::Hash + Copy> Default for WidgetsRepr<K> {
    fn default() -> Self {
        Self { cells: Vec::new() }
    }
}

impl<K: Eq + std::hash::Hash + Copy> From<WidgetsRepr<K>> for Widgets<K> {
    fn from(repr: WidgetsRepr<K>) -> Self {
        Widgets {
            cells: repr.cells.into_iter().collect(),
        }
    }
}

impl<K: Eq + std::hash::Hash + Copy> From<Widgets<K>> for WidgetsRepr<K> {
    fn from(w: Widgets<K>) -> Self {
        WidgetsRepr {
            cells: w.cells.into_iter().collect(),
        }
    }
}

impl<K: Eq + std::hash::Hash + Copy> Widgets<K> {
    /// The widget kind for `cell`, if any.
    pub fn kind(&self, cell: K) -> Option<WidgetKind> {
        self.cells.get(&cell).copied()
    }

    /// Whether `cell` is any widget (checkbox, slider, …).
    pub fn is_widget(&self, cell: K) -> bool {
        self.cells.contains_key(&cell)
    }

    /// Whether `cell` is a checkbox toggle.
    pub fn is_toggle(&self, cell: K) -> bool {
        matches!(self.kind(cell), Some(WidgetKind::Toggle))
    }

    /// Whether `cell` is a slider.
    pub fn is_slider(&self, cell: K) -> bool {
        matches!(self.kind(cell), Some(WidgetKind::Slider { .. }))
    }

    /// Whether `cell` is a clickable button.
    pub fn is_button(&self, cell: K) -> bool {
        matches!(self.kind(cell), Some(WidgetKind::Button))
    }

    /// Whether `cell` is a progress bar.
    pub fn is_progress_bar(&self, cell: K) -> bool {
        matches!(self.kind(cell), Some(WidgetKind::ProgressBar { .. }))
    }

    /// Set (or clear, with `kind = None`) the widget on `cell`.
    pub fn set(&mut self, cell: K, kind: Option<WidgetKind>) {
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
    pub fn set_toggle(&mut self, cell: K, on: bool) {
        if on {
            self.set(cell, Some(WidgetKind::Toggle));
        } else {
            self.set(cell, None);
        }
    }

    /// Convenience: turn `cell` into a slider with the default range,
    /// or clear it (`!on`).
    pub fn set_slider_default(&mut self, cell: K, on: bool) {
        if on {
            self.set(cell, Some(WidgetKind::DEFAULT_SLIDER));
        } else {
            self.set(cell, None);
        }
    }

    /// Set a slider on `cell` with an explicit `[min, max]` range.
    /// Replaces any prior widget kind on the cell.
    pub fn set_slider(&mut self, cell: K, min: f64, max: f64) {
        self.set(cell, Some(WidgetKind::Slider { min, max }));
    }

    /// Convenience: turn `cell` into a clickable button (`on`) or
    /// clear its widget treatment (`!on`).
    pub fn set_button(&mut self, cell: K, on: bool) {
        if on {
            self.set(cell, Some(WidgetKind::Button));
        } else {
            self.set(cell, None);
        }
    }

    /// Turn `cell` into a progress bar with the given `max`. Replaces
    /// any prior widget kind.
    pub fn set_progress_bar(&mut self, cell: K, max: f64) {
        self.set(cell, Some(WidgetKind::ProgressBar { max }));
    }

    /// Default progress-bar max for the ribbon's "Progress" action
    /// — 100 mirrors the default slider range so a Slider–ProgressBar
    /// pair reads naturally without per-cell tuning.
    pub const DEFAULT_PROGRESS_MAX: f64 = 100.0;

    /// Whether no cell carries a widget — lets the renderer skip its
    /// widget pass entirely.
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    /// How many cells carry a widget.
    pub fn count(&self) -> usize {
        self.cells.len()
    }

    /// All `(cell, kind)` pairs. Used by state-IO snapshots.
    pub fn iter(&self) -> impl Iterator<Item = (&K, &WidgetKind)> {
        self.cells.iter()
    }

    /// Replace the map with `(cell, kind)` pairs. Used by state-IO
    /// snapshots when restoring a saved workbook.
    pub fn replace_with(&mut self, entries: impl IntoIterator<Item = (K, WidgetKind)>) {
        self.cells = entries.into_iter().collect();
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

/// Map a cell's numeric value to a progress-bar fraction in `[0, 1]`.
/// Non-numeric values read as 0; a `max` of zero (or non-positive)
/// collapses to a full bar (so misconfigured ranges fail visibly).
pub fn progress_fraction(value: &CellValue, max: f64) -> f32 {
    let raw = match value {
        CellValue::Number(n) => *n,
        CellValue::Integer(i) => *i as f64,
        _ => 0.0,
    };
    if max <= 0.0 {
        return 1.0;
    }
    (raw / max).clamp(0.0, 1.0) as f32
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
    fn set_and_query_progress_bar() {
        let mut w = Widgets::default();
        w.set_progress_bar((2, 2), 500.0);
        assert!(w.is_progress_bar((2, 2)));
        assert!(!w.is_slider((2, 2)));
        assert_eq!(w.kind((2, 2)), Some(WidgetKind::ProgressBar { max: 500.0 }),);
    }

    #[test]
    fn progress_fraction_clamps_and_handles_edge_cases() {
        // Mid-range maps to the expected fraction.
        assert_eq!(progress_fraction(&CellValue::Number(50.0), 100.0), 0.5);
        // Values above `max` clamp to 1.0; below 0 clamp to 0.
        assert_eq!(progress_fraction(&CellValue::Number(150.0), 100.0), 1.0);
        assert_eq!(progress_fraction(&CellValue::Number(-10.0), 100.0), 0.0);
        // Integers behave the same as their numeric value.
        assert_eq!(progress_fraction(&CellValue::Integer(25), 100.0), 0.25);
        // Non-numeric reads as 0 — the bar starts empty for a brand-
        // new cell.
        assert_eq!(progress_fraction(&CellValue::Empty, 100.0), 0.0);
        // A non-positive max collapses to a full bar (misconfigured
        // range fails visibly rather than dividing by zero).
        assert_eq!(progress_fraction(&CellValue::Number(50.0), 0.0), 1.0);
    }

    #[test]
    fn widgets_generic_with_voronoi_coord_round_trip() {
        use carbide_tess::voronoi::VoronoiCoord;
        let mut w: Widgets<VoronoiCoord> = Widgets::default();
        w.set_toggle(VoronoiCoord(5), true);
        w.set_button(VoronoiCoord(0), true);

        let json = serde_json::to_string(&w).unwrap();
        let back: Widgets<VoronoiCoord> = serde_json::from_str(&json).unwrap();

        assert_eq!(back.count(), 2);
        assert!(back.is_toggle(VoronoiCoord(5)));
        assert!(back.is_button(VoronoiCoord(0)));
    }

    #[test]
    fn widgets_generic_with_tri_coord_round_trip() {
        use carbide_tess::triangle::TriCoord;
        let mut w: Widgets<TriCoord> = Widgets::default();
        w.set_toggle(TriCoord::new(2, -1), true);
        w.set_button(TriCoord::new(0, 0), true);

        let json = serde_json::to_string(&w).unwrap();
        let back: Widgets<TriCoord> = serde_json::from_str(&json).unwrap();

        assert_eq!(back.count(), 2);
        assert!(back.is_toggle(TriCoord::new(2, -1)));
        assert!(back.is_button(TriCoord::new(0, 0)));
    }

    #[test]
    fn widgets_generic_with_hex_coord_round_trip() {
        use carbide_tess::hex::HexCoord;
        let mut w: Widgets<HexCoord> = Widgets::default();
        w.set_button(HexCoord::new(2, 2), true);
        w.set_toggle(HexCoord::new(-1, 3), true);

        let json = serde_json::to_string(&w).unwrap();
        let back: Widgets<HexCoord> = serde_json::from_str(&json).unwrap();

        assert_eq!(back.count(), 2);
        assert!(back.is_button(HexCoord::new(2, 2)));
        assert!(back.is_toggle(HexCoord::new(-1, 3)));
    }

    #[test]
    fn widgets_round_trip_with_every_kind() {
        let mut w: Widgets<(u32, u32)> = Widgets::default();
        w.set_toggle((0, 0), true);
        w.set_slider((1, 1), -5.0, 25.0);
        w.set_button((2, 2), true);
        w.set_progress_bar((3, 3), 500.0);

        let json = serde_json::to_string(&w).unwrap();
        let back: Widgets<(u32, u32)> = serde_json::from_str(&json).unwrap();

        assert_eq!(back.count(), 4);
        assert!(back.is_toggle((0, 0)));
        assert_eq!(
            back.kind((1, 1)),
            Some(WidgetKind::Slider {
                min: -5.0,
                max: 25.0
            }),
        );
        assert!(back.is_button((2, 2)));
        assert_eq!(
            back.kind((3, 3)),
            Some(WidgetKind::ProgressBar { max: 500.0 })
        );
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
