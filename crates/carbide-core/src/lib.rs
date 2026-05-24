//! Workbook / sheet / cell types and the DAG evaluation engine for Carbide.
//!
//! See `PLAN.md` §4 (data model) and §5 (DAG engine). Phase 0 ships only the
//! type skeleton; the recompute engine lands in Phase 1.

use serde::{Deserialize, Serialize};
use carbide_tess::{LatticeConfig, LatticeKind};

pub mod cell;
pub mod dag;
pub mod env;
pub mod extent;
pub mod reference;
pub mod value;

pub use cell::{Cell, CellError, EngineKind};
pub use dag::{Dag, DagError};
pub use env::Env;
pub use extent::{BoundedExtent, SheetExtent};
pub use reference::CellRef;
pub use value::{Array, CarbideFn, CellValue, RefShape, ShapeError};

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
    /// Bounded vs unbounded. Unbounded is the default — sparse storage means
    /// "infinite" costs nothing extra in RAM. Bounded sheets reject
    /// out-of-bounds addresses at write time.
    #[serde(default)]
    pub extent: SheetExtent,
    /// Per-lattice persisted configuration. `None` for the uniform tilings
    /// (square/hex/triangle/parallelogram — fully described by `lattice`)
    /// and for legacy files predating this field. `Some(Voronoi(..))`
    /// carries the seed set so dragged Voronoi seeds persist and the
    /// engine's eval-time lattice matches the UI (ADR-011 / ADR-012).
    #[serde(default)]
    pub lattice_config: Option<LatticeConfig>,
    // Cells indexed by canonical address. Lattice-typed coord enum is a
    // Phase 2+ refactor.
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
                        extent: SheetExtent::Unbounded,
                        lattice_config: None,
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

    #[test]
    fn sheet_with_lattice_config_round_trips() {
        use carbide_tess::{LatticeConfig, VoronoiConfig};
        let cfg = LatticeConfig::Voronoi(VoronoiConfig {
            seeds: vec![[0.0, 0.0], [10.0, 0.0], [0.0, 10.0]],
            bounds: [-20.0, -20.0, 20.0, 20.0],
            frozen: false,
        });
        let sheet = Sheet {
            id: SheetId(7),
            name: "Vor".into(),
            lattice: LatticeKind::Voronoi,
            extent: SheetExtent::Unbounded,
            lattice_config: Some(cfg.clone()),
            cells: hashbrown::HashMap::new(),
        };
        let json = serde_json::to_string(&sheet).unwrap();
        let back: Sheet = serde_json::from_str(&json).unwrap();
        assert_eq!(back.lattice_config, Some(cfg));
    }

    #[test]
    fn sheet_missing_lattice_config_defaults_none() {
        // A legacy `workbook.json` sheet object with no `lattice_config` key.
        let legacy = r#"{
            "id": 1,
            "name": "Sheet1",
            "lattice": "square",
            "cells": {}
        }"#;
        let sheet: Sheet = serde_json::from_str(legacy).unwrap();
        assert_eq!(sheet.lattice_config, None);
    }
}
