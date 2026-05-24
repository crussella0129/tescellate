//! `Cell` and the engine-kind tag. See PLAN.md §4 and §6.

use crate::CellValue;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineKind {
    ExcelLite,
    Python,
    Rhai,
    RustNative,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "code", content = "detail")]
pub enum CellError {
    /// Reference to a non-existent cell.
    Ref,
    /// Participant in a dependency cycle.
    Cycle,
    /// Division by zero.
    DivZero,
    /// Numeric out-of-range / NaN propagated.
    Num,
    /// Type mismatch (string where number expected, etc.).
    Value,
    /// Parse error in the active formula engine. Detail = engine message.
    Lang(String),
    /// Engine-level compile error (e.g., rustc diagnostics). Detail = stderr.
    Compile(String),
    /// Formula exceeded its execution budget.
    Timeout,
    /// Spill region collides with a non-empty cell. See PLAN.md §6.2.2.
    Spill,
    /// A previously-stored `Function` value (a lambda) was loaded from
    /// disk. Lambdas don't fully round-trip through `.tscl`; this is the
    /// placeholder until `engine::rebuild_dag` re-evaluates the cell's
    /// source and produces a live `CellValue::Function` again.
    StaleFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cell {
    /// Source as the user typed it. `None` means a blank cell.
    pub source: Option<String>,
    /// Override engine for this cell. `None` → inherit sheet / workbook default.
    pub engine: Option<EngineKind>,
    /// Last evaluated result.
    #[serde(default)]
    pub value: CellValue,
}

impl Cell {
    pub fn blank() -> Self {
        Self {
            source: None,
            engine: None,
            value: CellValue::Empty,
        }
    }
}
