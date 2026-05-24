//! Generates the v160-shaped sample workbook used by sprint 19's visual
//! checkpoint to verify the dual-extension `.crbd`/`.tscl` Open filter.
//!
//! Writes bytes to stdout — invoke as:
//!
//!     cargo run -p carbide-store --example gen_back_compat > examples/sprint19-back-compat.tscl
//!
//! The byte layout is identical to what pre-v161 code would have produced
//! (the rename was suffix-only; the zip schema is unchanged at v2).

use carbide_core::{
    Cell, CellValue, EngineKind, Sheet, SheetExtent, SheetId, Workbook, WorkbookId, WorkbookMeta,
};
use carbide_store::{save_full_to_bytes, UiState};
use carbide_tess::LatticeKind;
use hashbrown::HashMap;
use std::io::Write;

fn main() {
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
            source: Some("=A1 * 2".into()),
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
            extent: SheetExtent::Unbounded,
            lattice_config: None,
            cells,
        },
    );
    let wb = Workbook {
        id: WorkbookId(1),
        meta: WorkbookMeta {
            title: "sprint19 back-compat sample".into(),
            created_at: "2026-05-24T00:00:00Z".into(),
            format_version: 0,
        },
        default_engine: EngineKind::ExcelLite,
        sheet_order: vec![sid],
        sheets,
    };
    let bytes = save_full_to_bytes(&wb, &UiState::default()).expect("save_full_to_bytes");
    std::io::stdout()
        .write_all(&bytes)
        .expect("write bytes to stdout");
}
