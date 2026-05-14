//! `CellValue` — the lingua franca across formula engines. See PLAN.md §4.

use serde::{Deserialize, Serialize};
use std::any::Any;
use std::fmt::Debug;
use std::sync::Arc;

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

/// Opaque callable value carried by `CellValue::Function`. The concrete
/// implementation lives in the formula engine crate; core only needs to
/// hold and display labels for the value. Use `as_any()` to downcast at
/// the call site (e.g. `MAP`, `REDUCE`, `Apply` evaluator).
pub trait CarbideFn: Send + Sync + Any + Debug {
    fn as_any(&self) -> &dyn Any;
    fn debug_label(&self) -> String;
}

/// Result of evaluating a cell. Engine-agnostic so values can cross
/// engine boundaries within a workbook.
///
/// `Function` is `Arc<dyn CarbideFn>` because lambdas are not `Clone` and
/// have engine-specific bodies. Equality uses `Arc::ptr_eq` — two textually
/// identical `LAMBDA(x, x+1)` expressions in different cells compare
/// unequal. The DAG keys on cell address, not value, so this is fine.
///
/// `Clone`, `Default`, and `Debug` are derived; `PartialEq`, `Serialize`,
/// and `Deserialize` are hand-implemented (below) because `Arc<dyn>`
/// doesn't fit the derive macros.
#[derive(Debug, Clone, Default)]
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
    /// First-class function value: `LAMBDA(...)` literals, LET-bound
    /// closures, etc. Cannot fully round-trip through `.tscl` save/load
    /// — deserializes to `CellError::StaleFunction` and is restored by
    /// recompute-on-load from the cell's stored `source`.
    Function(Arc<dyn CarbideFn>),
}

impl PartialEq for CellValue {
    fn eq(&self, other: &Self) -> bool {
        use CellValue::*;
        match (self, other) {
            (Empty, Empty) => true,
            (Number(a), Number(b)) => a == b,
            (Integer(a), Integer(b)) => a == b,
            (Bool(a), Bool(b)) => a == b,
            (Text(a), Text(b)) => a == b,
            (Array(a), Array(b)) => a == b,
            (Error(a), Error(b)) => a == b,
            (Pending, Pending) => true,
            // Function identity: two lambdas are equal iff they're the
            // same allocation. Cheap, deterministic, and good enough for
            // the DAG's needs (which keys on CellRef anyway).
            (Function(a), Function(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }
}

// ---------------------------------------------------------------------------
// Serde — preserves the existing `{"kind": ..., "value": ...}` shape for the
// data variants and emits `{"kind":"function","value":{"label":...}}` for
// Function. On deserialize, a `function` variant becomes
// `Error(CellError::StaleFunction)`: lambdas don't round-trip; recompute-
// on-load restores them from the cell's stored source.
// ---------------------------------------------------------------------------

impl Serialize for CellValue {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("CellValue", 2)?;
        match self {
            CellValue::Empty => {
                s.serialize_field("kind", "empty")?;
                s.serialize_field::<()>("value", &())?;
            }
            CellValue::Number(n) => {
                s.serialize_field("kind", "number")?;
                s.serialize_field("value", n)?;
            }
            CellValue::Integer(i) => {
                s.serialize_field("kind", "integer")?;
                s.serialize_field("value", i)?;
            }
            CellValue::Bool(b) => {
                s.serialize_field("kind", "bool")?;
                s.serialize_field("value", b)?;
            }
            CellValue::Text(t) => {
                s.serialize_field("kind", "text")?;
                s.serialize_field("value", t)?;
            }
            CellValue::Array(arr) => {
                s.serialize_field("kind", "array")?;
                s.serialize_field("value", arr.as_ref())?;
            }
            CellValue::Error(e) => {
                s.serialize_field("kind", "error")?;
                s.serialize_field("value", e)?;
            }
            CellValue::Pending => {
                s.serialize_field("kind", "pending")?;
                s.serialize_field::<()>("value", &())?;
            }
            CellValue::Function(f) => {
                s.serialize_field("kind", "function")?;
                #[derive(Serialize)]
                struct FnLabel<'a> {
                    label: &'a str,
                }
                let label = f.debug_label();
                s.serialize_field("value", &FnLabel { label: &label })?;
            }
        }
        s.end()
    }
}

impl<'de> Deserialize<'de> for CellValue {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // We deserialize through an intermediate tagged enum that covers
        // every Serialize variant *except* Function (which decodes to a
        // StaleFunction error since lambdas don't round-trip).
        #[derive(Deserialize)]
        #[serde(tag = "kind", content = "value", rename_all = "snake_case")]
        enum Wire {
            Empty,
            Number(f64),
            Integer(i64),
            Bool(bool),
            Text(String),
            Array(Box<Array>),
            Error(super::cell::CellError),
            Pending,
            Function(serde::de::IgnoredAny),
        }
        let wire = Wire::deserialize(deserializer)?;
        Ok(match wire {
            Wire::Empty => CellValue::Empty,
            Wire::Number(n) => CellValue::Number(n),
            Wire::Integer(i) => CellValue::Integer(i),
            Wire::Bool(b) => CellValue::Bool(b),
            Wire::Text(t) => CellValue::Text(t),
            Wire::Array(a) => CellValue::Array(a),
            Wire::Error(e) => CellValue::Error(e),
            Wire::Pending => CellValue::Pending,
            Wire::Function(_) => CellValue::Error(super::cell::CellError::StaleFunction),
        })
    }
}
