//! UI-side persistence snapshot — the typed mirror of the opaque
//! `tescellate_store::UiState` blob that rides inside a `.tscl` file
//! alongside the workbook.
//!
//! [`UiSnapshot`] is a deliberately small, JSON-friendly struct that
//! captures only the UI fields the user expects to survive a save / open
//! cycle: which sheet was on screen, Stage Mode flag, per-sheet
//! formatting, widgets, notes, and conditional rules. Everything else
//! (selection drag, find state, history, dialog visibility, etc.) is
//! intentionally ephemeral.
//!
//! [`capture`] is called when writing a `.tscl`; [`restore`] is called
//! when reading one back. Both are pure functions over the app struct:
//! no rendering, no engine calls.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use tescellate_tess::hex::HexCoord;
use tescellate_tess::triangle::TriCoord;

use crate::conditional::Rule;
use crate::format::FormatMap;
use crate::widget::Widgets;

/// JSON-stable name of the sheet on screen when the workbook was saved.
/// Defaults to `Square` when an older snapshot omits the field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ActiveSheetTag {
    #[default]
    Square,
    Hex,
    Triangle,
}

/// The persistent slice of a `TescellateApp`. New fields land here with a
/// `#[serde(default)]` so older snapshots tolerate the addition; removed
/// fields are kept until the next major format bump.
///
/// HashMap fields with non-string keys serialize as Vec-of-pair arrays
/// (JSON object keys must be strings, and tuple/struct keys would silently
/// fail otherwise). The on-disk form is `[[[c, r], "note"], …]`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct UiSnapshot {
    pub active_sheet: ActiveSheetTag,
    pub stage_mode: bool,
    pub dark_mode: bool,
    pub square_formats: FormatMap<(u32, u32)>,
    pub hex_formats: FormatMap<HexCoord>,
    pub triangle_formats: FormatMap<TriCoord>,
    pub widgets: Widgets,
    #[serde(with = "vec_pair::square_notes")]
    pub square_notes: HashMap<(u32, u32), String>,
    #[serde(with = "vec_pair::hex_notes")]
    pub hex_notes: HashMap<HexCoord, String>,
    #[serde(with = "vec_pair::tri_notes")]
    pub triangle_notes: HashMap<TriCoord, String>,
    pub conditional_rules: Vec<Rule>,
    pub column_widths: HashMap<u32, f32>,
    pub row_heights: HashMap<u32, f32>,
    /// True only for the fresh seed workbook produced by `TescellateApp::new`
    /// before the user touched anything. Used by the autosave path to skip
    /// persisting the seed itself.
    pub is_fresh_seed: bool,
}

/// HashMap-with-non-string-key serde adapters. One module per concrete
/// key type because `serde(with = ...)` needs a path, not a generic.
mod vec_pair {
    macro_rules! vec_pair_for {
        ($mod:ident, $key:ty) => {
            pub mod $mod {
                use serde::{Deserialize, Deserializer, Serialize, Serializer};
                use std::collections::HashMap;

                pub fn serialize<S: Serializer>(
                    map: &HashMap<$key, String>,
                    s: S,
                ) -> Result<S::Ok, S::Error> {
                    let pairs: Vec<(&$key, &String)> = map.iter().collect();
                    pairs.serialize(s)
                }

                pub fn deserialize<'de, D: Deserializer<'de>>(
                    d: D,
                ) -> Result<HashMap<$key, String>, D::Error> {
                    let pairs: Vec<($key, String)> = Vec::deserialize(d)?;
                    Ok(pairs.into_iter().collect())
                }
            }
        };
    }
    vec_pair_for!(square_notes, (u32, u32));
    vec_pair_for!(hex_notes, tescellate_tess::hex::HexCoord);
    vec_pair_for!(tri_notes, tescellate_tess::triangle::TriCoord);
}

/// JSON-encode a snapshot into the opaque store-side blob. Falls back to
/// `Value::Null` only on a `serde_json::to_value` panic (which the typed
/// adapters above prevent), keeping the round-trip lossless in practice.
pub fn snapshot_to_ui_state(s: &UiSnapshot) -> tescellate_store::UiState {
    tescellate_store::UiState(serde_json::to_value(s).unwrap_or(serde_json::Value::Null))
}

/// JSON-decode an opaque store-side blob into a typed snapshot. Returns
/// the default snapshot on a parse failure — losing UI state is annoying
/// but not corrupting; the workbook itself still loads.
pub fn ui_state_to_snapshot(ui: &tescellate_store::UiState) -> UiSnapshot {
    serde_json::from_value(ui.0.clone()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conditional::Condition;
    use crate::format::CellFormat;
    use egui::Color32;

    #[test]
    fn snapshot_round_trips_through_ui_state() {
        let mut widgets = Widgets::default();
        widgets.set_slider((1, 5), 0.0, 100.0);
        widgets.set_toggle((2, 5), true);

        let mut square_formats: FormatMap<(u32, u32)> = FormatMap::new();
        square_formats.update((0, 0), |f| {
            f.bold = true;
            f.text_color = Some(Color32::from_rgba_unmultiplied(10, 20, 30, 255));
        });

        let snap = UiSnapshot {
            active_sheet: ActiveSheetTag::Hex,
            stage_mode: true,
            dark_mode: false,
            square_formats,
            hex_formats: FormatMap::default(),
            triangle_formats: FormatMap::default(),
            widgets,
            square_notes: [((3, 3), "remember this".to_string())]
                .into_iter()
                .collect(),
            hex_notes: HashMap::new(),
            triangle_notes: HashMap::new(),
            conditional_rules: vec![Rule {
                condition: Condition::GreaterThan(50.0),
                format: CellFormat {
                    fill: Some(Color32::from_rgba_unmultiplied(200, 100, 50, 255)),
                    ..CellFormat::default()
                },
            }],
            column_widths: [(0, 90.0), (1, 120.0)].into_iter().collect(),
            row_heights: [(2, 30.0)].into_iter().collect(),
            is_fresh_seed: false,
        };

        let ui = snapshot_to_ui_state(&snap);
        let back = ui_state_to_snapshot(&ui);

        assert_eq!(back.active_sheet, ActiveSheetTag::Hex);
        assert!(back.stage_mode);
        assert!(back.widgets.is_slider((1, 5)));
        assert!(back.widgets.is_toggle((2, 5)));
        assert!(back.square_formats.get((0, 0)).bold);
        assert_eq!(
            back.square_notes.get(&(3, 3)).map(String::as_str),
            Some("remember this")
        );
        assert_eq!(back.conditional_rules.len(), 1);
        assert_eq!(back.column_widths.get(&0), Some(&90.0));
    }

    #[test]
    fn empty_ui_state_yields_default_snapshot() {
        let ui = tescellate_store::UiState::default();
        let snap = ui_state_to_snapshot(&ui);
        assert_eq!(snap.active_sheet, ActiveSheetTag::Square);
        assert!(!snap.stage_mode);
        assert_eq!(snap.conditional_rules.len(), 0);
    }
}
