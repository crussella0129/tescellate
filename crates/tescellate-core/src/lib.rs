//! Workbook / sheet / cell types and the DAG evaluation engine for Tescellate.
//!
//! See `PLAN.md` §4 (data model) and §5 (DAG engine). Phase 0 ships only the
//! type skeleton; the recompute engine lands in Phase 1.

use serde::{Deserialize, Serialize};
use tescellate_tess::LatticeKind;

pub mod cell;
pub mod value;

pub use cell::{Cell, CellError, EngineKind};
pub use value::CellValue;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WorkbookId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SheetId(pub u32);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkbookMeta {
    pub title: String,
    pub created_at: String,
    pub format_version: u32,
}

/// Top-level container. See PLAN.md §4 for the model description.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workbook {
    pub id: WorkbookId,
    pub meta: WorkbookMeta,
    pub default_engine: EngineKind,
    pub sheet_order: Vec<SheetId>,
    pub sheets: hashbrown::HashMap<SheetId, Sheet>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sheet {
    pub id: SheetId,
    pub name: String,
    pub lattice: LatticeKind,
    // Cells indexed by serialized coord for now; Phase 1 will introduce a
    // lattice-typed coord enum so this can be strongly typed.
    pub cells: hashbrown::HashMap<String, Cell>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workbook_round_trips_through_json() {
        let wb = Workbook {
            id: WorkbookId(1),
            meta: WorkbookMeta {
                title: "scratch".into(),
                created_at: "2026-05-14T00:00:00Z".into(),
                format_version: 0,
            },
            default_engine: EngineKind::ExcelLite,
            sheet_order: vec![SheetId(1)],
            sheets: {
                let mut m = hashbrown::HashMap::new();
                m.insert(
                    SheetId(1),
                    Sheet {
                        id: SheetId(1),
                        name: "Sheet1".into(),
                        lattice: LatticeKind::Square,
                        cells: hashbrown::HashMap::new(),
                    },
                );
                m
            },
        };
        let s = serde_json::to_string(&wb).unwrap();
        let back: Workbook = serde_json::from_str(&s).unwrap();
        assert_eq!(back.meta.title, "scratch");
        assert_eq!(back.sheets.len(), 1);
    }
}
