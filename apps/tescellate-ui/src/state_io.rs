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
    #[serde(alias = "widgets")]
    pub square_widgets: Widgets<(u32, u32)>,
    pub hex_widgets: Widgets<HexCoord>,
    /// Triangle-sheet widgets (sprint 4). New field — older v144/v145/v146
    /// snapshots default to empty here. Mirrors the square/hex shape.
    pub triangle_widgets: Widgets<TriCoord>,
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

/// localStorage key for the wasm autosave. Versioned so a future format
/// bump can ignore stale autosaves rather than crash on them.
pub const AUTOSAVE_KEY: &str = "tescellate.autosave.v1";

/// Maximum size of a base64-encoded autosave payload we'll try to write.
/// Browsers quota localStorage to ~5 MiB per origin; we cap at ~4 MiB
/// base64-encoded so a pathological workbook fails quietly rather than
/// throwing a quota exception that breaks the autosave path.
pub const AUTOSAVE_MAX_BYTES: usize = 4 * 1024 * 1024;

/// Persist `.tscl` bytes into browser localStorage (wasm32 only). On
/// native, a no-op — the dialog flow already covers explicit save. All
/// failures (no window, no storage, quota exceeded, oversize payload)
/// are swallowed; autosave is best-effort.
#[cfg(target_arch = "wasm32")]
pub fn autosave_to_local_storage(bytes: &[u8]) {
    use base64::Engine as _;
    if bytes.len() > AUTOSAVE_MAX_BYTES {
        return;
    }
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    if let Some(window) = web_sys::window() {
        if let Ok(Some(storage)) = window.local_storage() {
            let _ = storage.set_item(AUTOSAVE_KEY, &encoded);
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn autosave_to_local_storage(_bytes: &[u8]) {
    // No-op on native; explicit Save/Open is the persistence path.
}

/// Read back a previously-autosaved payload from localStorage (wasm32
/// only). Returns `None` on native, when no autosave exists, when the
/// stored value isn't valid base64, or on any underlying JS error —
/// all of which leave the caller free to fall back to the seed demos.
#[cfg(target_arch = "wasm32")]
pub fn load_from_local_storage() -> Option<Vec<u8>> {
    use base64::Engine as _;
    let window = web_sys::window()?;
    let storage = window.local_storage().ok().flatten()?;
    let encoded = storage.get_item(AUTOSAVE_KEY).ok().flatten()?;
    base64::engine::general_purpose::STANDARD
        .decode(encoded.as_bytes())
        .ok()
}

#[cfg(not(target_arch = "wasm32"))]
pub fn load_from_local_storage() -> Option<Vec<u8>> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conditional::Condition;
    use crate::format::CellFormat;
    use egui::Color32;

    #[test]
    fn snapshot_round_trips_through_ui_state() {
        let mut sq_widgets: Widgets<(u32, u32)> = Widgets::default();
        sq_widgets.set_slider((1, 5), 0.0, 100.0);
        sq_widgets.set_toggle((2, 5), true);
        let mut hex_widgets: Widgets<HexCoord> = Widgets::default();
        hex_widgets.set_button(HexCoord::new(2, 2), true);
        let mut triangle_widgets: Widgets<TriCoord> = Widgets::default();
        triangle_widgets.set_toggle(TriCoord::new(2, -1), true);

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
            square_widgets: sq_widgets,
            hex_widgets,
            triangle_widgets,
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
        assert!(back.square_widgets.is_slider((1, 5)));
        assert!(back.square_widgets.is_toggle((2, 5)));
        assert!(back.hex_widgets.is_button(HexCoord::new(2, 2)));
        assert!(back.triangle_widgets.is_toggle(TriCoord::new(2, -1)));
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

    #[test]
    fn v145_snapshot_loads_square_widgets_via_alias() {
        // A v145-era snapshot used the field name `widgets` for the
        // (square-only) widget map. The v146 schema renames that to
        // `square_widgets` and adds `hex_widgets` — the rename carries
        // `#[serde(alias = "widgets")]` so a v145 save still loads.
        let v145_json = serde_json::json!({
            "active_sheet": "Square",
            "stage_mode": false,
            "widgets": {
                "cells": [[[1, 1], "Toggle"]]
            }
        });
        let snap: UiSnapshot = serde_json::from_value(v145_json).unwrap();
        assert!(snap.square_widgets.is_toggle((1, 1)));
        assert!(snap.hex_widgets.is_empty());
    }

    #[test]
    fn autosave_to_local_storage_is_noop_on_native() {
        // No panics, no observable failure.
        autosave_to_local_storage(b"some bytes");
        autosave_to_local_storage(&[]);
        // Oversize payload still doesn't panic.
        autosave_to_local_storage(&vec![0u8; AUTOSAVE_MAX_BYTES + 1]);
    }

    #[test]
    fn load_from_local_storage_returns_none_on_native() {
        assert!(load_from_local_storage().is_none());
    }
}
