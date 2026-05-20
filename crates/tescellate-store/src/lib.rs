//! `.tscl` file format I/O. See PLAN.md §9.
//!
//! A `.tscl` file is a zip archive with this layout:
//!
//! ```text
//! manifest.json   { "format_version": 1, "engines": [...] }
//! workbook.json   serialized `Workbook`
//! ui.json         opaque [`UiState`] JSON (added in v1)
//! ```
//!
//! v0 files (manifest.format_version == 0) predate `ui.json` and load with
//! [`UiState::default()`]. Future phases add `sheets/<id>.json` for large
//! workbooks, `formulas/native/<hash>.rs` for cached Rust compilations, and
//! `trust.json` for the native-formula trust manifest.

use serde::{Deserialize, Serialize};
use std::io::{Cursor, Read, Seek, Write};
use tescellate_core::{EngineKind, Workbook};
use thiserror::Error;

/// Highest format version this build can write and read at full fidelity.
/// v0 files are still readable for migration; reads of v0 fill in
/// [`UiState::default()`].
pub const FORMAT_VERSION: u32 = 1;

/// Opaque UI-side state ridden alongside the workbook inside a `.tscl`.
///
/// The store treats this as a transparent JSON blob — it does not interpret
/// or validate fields. The egui front-end owns the schema (per-sheet formats,
/// widget catalogues, conditional rules, stage flags, etc.) and serializes /
/// deserializes its own typed snapshot against this value. Anything else that
/// later wants to ride along inside a workbook file uses the same envelope.
///
/// On disk this lands in `ui.json` inside the zip; v0 files predate the field
/// and load as `UiState::default()`, which is `{}`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UiState(pub serde_json::Value);

impl UiState {
    pub fn empty() -> Self {
        UiState(serde_json::Value::Object(serde_json::Map::new()))
    }
    pub fn is_empty(&self) -> bool {
        matches!(&self.0, serde_json::Value::Null)
            || matches!(&self.0, serde_json::Value::Object(m) if m.is_empty())
    }
    pub fn as_value(&self) -> &serde_json::Value {
        &self.0
    }
}

impl From<serde_json::Value> for UiState {
    fn from(v: serde_json::Value) -> Self {
        UiState(v)
    }
}

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("zip: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("format: {0}")]
    Format(String),
    #[error("unsupported format version {0}, this build supports {FORMAT_VERSION}")]
    Version(u32),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub format_version: u32,
    /// Formula engines the workbook depends on. Used to refuse to open a
    /// file when a needed engine isn't compiled into this build.
    pub engines: Vec<EngineKind>,
}

/// Persist a workbook together with an opaque UI state. Writes v1 of the
/// format. `ui` is serialized verbatim into `ui.json`; pass
/// [`UiState::default()`] when no UI state is being tracked.
pub fn save_full<W: Write + Seek>(
    workbook: &Workbook,
    ui: &UiState,
    writer: W,
) -> Result<(), StoreError> {
    let mut zip = zip::ZipWriter::new(writer);
    let opts: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    let manifest = Manifest {
        format_version: FORMAT_VERSION,
        engines: vec![workbook.default_engine],
    };
    zip.start_file("manifest.json", opts)?;
    zip.write_all(&serde_json::to_vec_pretty(&manifest)?)?;

    zip.start_file("workbook.json", opts)?;
    zip.write_all(&serde_json::to_vec(workbook)?)?;

    zip.start_file("ui.json", opts)?;
    zip.write_all(&serde_json::to_vec(ui)?)?;

    zip.finish()?;
    Ok(())
}

/// Load a workbook + opaque UI state. v0 files (no `ui.json`) load with
/// `(workbook, UiState::default())`. Unknown future versions error with
/// [`StoreError::Version`].
pub fn load_full<R: Read + Seek>(reader: R) -> Result<(Workbook, UiState), StoreError> {
    let mut zip = zip::ZipArchive::new(reader)?;

    let manifest: Manifest = {
        let mut f = zip.by_name("manifest.json")?;
        let mut buf = String::new();
        f.read_to_string(&mut buf)?;
        serde_json::from_str(&buf)?
    };
    if manifest.format_version > FORMAT_VERSION {
        return Err(StoreError::Version(manifest.format_version));
    }

    let workbook: Workbook = {
        let mut f = zip.by_name("workbook.json")?;
        let mut buf = String::new();
        f.read_to_string(&mut buf)?;
        serde_json::from_str(&buf)?
    };

    // ui.json is optional: v0 doesn't have it; tolerate FileNotFound and
    // surface real read/parse errors otherwise.
    let ui = match zip.by_name("ui.json") {
        Ok(mut f) => {
            let mut buf = String::new();
            f.read_to_string(&mut buf)?;
            serde_json::from_str(&buf)?
        }
        Err(zip::result::ZipError::FileNotFound) => UiState::default(),
        Err(e) => return Err(StoreError::Zip(e)),
    };

    Ok((workbook, ui))
}

/// Persist a workbook with no UI state. Thin wrapper over [`save_full`].
pub fn save<W: Write + Seek>(workbook: &Workbook, writer: W) -> Result<(), StoreError> {
    save_full(workbook, &UiState::default(), writer)
}

/// Load a workbook discarding any UI state. Thin wrapper over [`load_full`].
pub fn load<R: Read + Seek>(reader: R) -> Result<Workbook, StoreError> {
    load_full(reader).map(|(wb, _ui)| wb)
}

/// Helper for in-memory round-trips, used by tests and the IPC layer.
pub fn save_to_bytes(workbook: &Workbook) -> Result<Vec<u8>, StoreError> {
    let mut buf: Vec<u8> = Vec::new();
    save(workbook, Cursor::new(&mut buf))?;
    Ok(buf)
}

pub fn load_from_bytes(bytes: &[u8]) -> Result<Workbook, StoreError> {
    load(Cursor::new(bytes))
}

/// In-memory `save_full` for the wasm UI and IPC layer.
pub fn save_full_to_bytes(workbook: &Workbook, ui: &UiState) -> Result<Vec<u8>, StoreError> {
    let mut buf: Vec<u8> = Vec::new();
    save_full(workbook, ui, Cursor::new(&mut buf))?;
    Ok(buf)
}

/// In-memory `load_full` for the wasm UI and IPC layer.
pub fn load_full_from_bytes(bytes: &[u8]) -> Result<(Workbook, UiState), StoreError> {
    load_full(Cursor::new(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use hashbrown::HashMap;
    use tescellate_core::{Cell, CellValue, Sheet, SheetId, WorkbookId, WorkbookMeta};
    use tescellate_tess::LatticeKind;

    fn sample() -> Workbook {
        let mut cells = HashMap::new();
        cells.insert(
            "A1".to_string(),
            Cell {
                source: Some("=42".into()),
                engine: None,
                value: CellValue::Number(42.0),
            },
        );
        cells.insert(
            "B1".to_string(),
            Cell {
                source: Some("=A1*2".into()),
                engine: None,
                value: CellValue::Number(84.0),
            },
        );
        let mut sheets = HashMap::new();
        let sid = SheetId(1);
        sheets.insert(
            sid,
            Sheet {
                id: sid,
                name: "Sheet1".into(),
                lattice: LatticeKind::Square,
                extent: tescellate_core::SheetExtent::Unbounded,
                cells,
            },
        );
        Workbook {
            id: WorkbookId(1),
            meta: WorkbookMeta {
                title: "test".into(),
                created_at: "2026-05-14T00:00:00Z".into(),
                format_version: 0,
            },
            default_engine: EngineKind::ExcelLite,
            sheet_order: vec![sid],
            sheets,
        }
    }

    #[test]
    fn round_trip_preserves_values_and_sources() {
        let wb = sample();
        let bytes = save_to_bytes(&wb).unwrap();
        let back = load_from_bytes(&bytes).unwrap();
        assert_eq!(back.meta.title, "test");
        assert_eq!(back.sheets.len(), 1);
        let sheet = back.sheets.values().next().unwrap();
        assert_eq!(sheet.cells.len(), 2);
        let a1 = sheet.cells.get("A1").unwrap();
        assert_eq!(a1.source.as_deref(), Some("=42"));
        assert_eq!(a1.value, CellValue::Number(42.0));
    }

    #[test]
    fn refuses_unknown_format_version() {
        let wb = sample();
        let mut bytes = save_to_bytes(&wb).unwrap();
        // Hand-edit the manifest inside the zip: extract, mutate, re-pack.
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes.clone())).unwrap();
        let mut wb_bytes = Vec::new();
        archive
            .by_name("workbook.json")
            .unwrap()
            .read_to_end(&mut wb_bytes)
            .unwrap();
        drop(archive);

        let mut new_zip = Vec::new();
        {
            let mut w = zip::ZipWriter::new(Cursor::new(&mut new_zip));
            let opts = zip::write::SimpleFileOptions::default();
            w.start_file("manifest.json", opts).unwrap();
            w.write_all(br#"{"format_version": 99, "engines": ["excel_lite"]}"#)
                .unwrap();
            w.start_file("workbook.json", opts).unwrap();
            w.write_all(&wb_bytes).unwrap();
            w.finish().unwrap();
        }
        bytes = new_zip;

        let err = load_from_bytes(&bytes).unwrap_err();
        assert!(matches!(err, StoreError::Version(99)));
    }
}
