//! `CellValue` — the lingua franca across formula engines. See PLAN.md §4.

use serde::{Deserialize, Serialize};

/// A 2D array value. Row-major: element at `(r, c)` lives at `data[r * cols + c]`.
/// See PLAN.md §6.2.1. Constructed via `Array::row`, `Array::col`, `Array::from_2d`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Array {
    pub rows: usize,
    pub cols: usize,
    pub data: Vec<CellValue>,
}

impl Array {
    pub fn new(rows: usize, cols: usize, data: Vec<CellValue>) -> Self {
        debug_assert_eq!(rows * cols, data.len(), "array data length mismatch");
        Self { rows, cols, data }
    }

    pub fn row(values: Vec<CellValue>) -> Self {
        let cols = values.len();
        Self::new(1, cols, values)
    }

    pub fn col(values: Vec<CellValue>) -> Self {
        let rows = values.len();
        Self::new(rows, 1, values)
    }

    pub fn from_2d(rows_data: Vec<Vec<CellValue>>) -> Result<Self, ShapeError> {
        let rows = rows_data.len();
        let cols = rows_data.first().map(|r| r.len()).unwrap_or(0);
        for (i, r) in rows_data.iter().enumerate() {
            if r.len() != cols {
                return Err(ShapeError::Ragged {
                    row: i,
                    got: r.len(),
                    expected: cols,
                });
            }
        }
        let data: Vec<_> = rows_data.into_iter().flatten().collect();
        Ok(Self::new(rows, cols, data))
    }

    pub fn get(&self, row: usize, col: usize) -> Option<&CellValue> {
        if row >= self.rows || col >= self.cols {
            return None;
        }
        self.data.get(row * self.cols + col)
    }

    pub fn shape(&self) -> (usize, usize) {
        (self.rows, self.cols)
    }

    pub fn is_scalar(&self) -> bool {
        self.rows == 1 && self.cols == 1
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn iter(&self) -> std::slice::Iter<'_, CellValue> {
        self.data.iter()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ShapeError {
    #[error("ragged array: row {row} has {got} elements, expected {expected}")]
    Ragged {
        row: usize,
        got: usize,
        expected: usize,
    },
}

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
    /// Array of values. When this is a cell's *result* and the array is
    /// larger than 1×1, it spills into adjacent cells (PLAN.md §6.2.2).
    /// Boxed to keep the enum size small for the common scalar case.
    Array(Box<Array>),
    Error(super::cell::CellError),
    /// Async eval / compile in flight. Renderer should show a spinner.
    Pending,
}
