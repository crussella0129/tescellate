//! `CellValue` — the lingua franca across formula engines. See PLAN.md §4.

use serde::{Deserialize, Serialize};

/// Result of evaluating a cell. Engine-agnostic so values can cross
/// engine boundaries within a workbook.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum CellValue {
    #[default]
    Empty,
    Number(f64),
    Integer(i64),
    Bool(bool),
    Text(String),
    /// Spilled array result (Excel "dynamic array" style). Phase 2+.
    Array(Vec<Vec<CellValue>>),
    Error(super::cell::CellError),
    /// Async eval / compile in flight. Renderer should show a spinner.
    Pending,
}
