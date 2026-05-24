//! `CellRef` — sheet-qualified cell identity used by the DAG and IPC layer.

use crate::SheetId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CellRef {
    pub sheet: SheetId,
    /// Canonical address as the sheet's lattice formats it. Always the
    /// canonical form, never user-typed casing (the caller normalizes
    /// before constructing a `CellRef`).
    pub address: String,
}

impl CellRef {
    pub fn new(sheet: SheetId, address: impl Into<String>) -> Self {
        Self {
            sheet,
            address: address.into(),
        }
    }
}
