//! Python formula engine — embedded CPython via PyO3. See PLAN.md §6.2.
//!
//! Feature-gated (`python`): pulls in `pyo3` and links libpython.
//!
//! v8 slice 1 proved the embedding. Slice 2 adds the `ctx` bridge — a
//! Python object that calls back into the Rust `EvalCtx`, so a Python
//! formula can read cells (`ctx.cell("A1")`) and ranges. The
//! `FormulaEngine` integration lands in slice 3.

use std::cell::Cell;
use std::ffi::CString;

use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyDict, PyFloat, PyInt, PyList, PyString};

use crate::{CompiledFormula, EvalCtx, EvalError, FormulaEngine, FormulaRef, ParseError};
use carbide_core::{CellValue, EngineKind};

/// Failure evaluating a Python formula.
#[derive(Debug, thiserror::Error)]
pub enum PyEvalError {
    #[error("python: {0}")]
    Py(String),
    #[error("python: unsupported result type: {0}")]
    UnsupportedResult(String),
}

fn pyerr(e: PyErr) -> PyEvalError {
    PyEvalError::Py(e.to_string())
}

/// Evaluate a Python expression in an embedded interpreter and convert its
/// result to a `CellValue`. No workbook context — the formula cannot read
/// cells; use [`eval_python_with_ctx`] for that.
pub fn eval_python_expr(src: &str) -> Result<CellValue, PyEvalError> {
    let code = CString::new(src)
        .map_err(|_| PyEvalError::Py("source contains an interior NUL byte".into()))?;
    Python::with_gil(|py| {
        let obj = py.eval(code.as_c_str(), None, None).map_err(pyerr)?;
        py_to_cell_value(&obj)
    })
}

/// Evaluate a Python expression with a `ctx` object in scope, so the
/// formula can read cells (`ctx.cell("A1")`) and ranges
/// (`ctx.range("A1", "A3")`) from the workbook.
pub fn eval_python_with_ctx(src: &str, ctx: &dyn EvalCtx) -> Result<CellValue, PyEvalError> {
    let code = CString::new(src)
        .map_err(|_| PyEvalError::Py("source contains an interior NUL byte".into()))?;
    // SAFETY: the `'static` lifetime is a lie scoped to this call. The
    // `PyCtx` is poisoned (`set(None)`) before this function returns and
    // never escapes `with_gil`, so the borrowed `ctx` is always alive for
    // every `PyCtx` method call. `PyCtx` is `unsendable`, pinning it to
    // this thread.
    let ctx_static: &'static dyn EvalCtx = unsafe { std::mem::transmute(ctx) };
    Python::with_gil(|py| {
        let pyctx = Bound::new(
            py,
            PyCtx {
                ctx: Cell::new(Some(ctx_static)),
            },
        )
        .map_err(pyerr)?;
        let globals = PyDict::new(py);
        globals.set_item("ctx", &pyctx).map_err(pyerr)?;
        let result = py.eval(code.as_c_str(), Some(&globals), None);
        // Poison the bridge — a formula that stashed `ctx` cannot dangle.
        pyctx.borrow().ctx.set(None);
        let obj = result.map_err(pyerr)?;
        py_to_cell_value(&obj)
    })
}

/// The `ctx` object handed to a Python formula. Wraps the borrowed Rust
/// `EvalCtx` for the duration of one evaluation; `unsendable` so PyO3
/// pins it to the evaluating thread.
#[pyclass(unsendable)]
struct PyCtx {
    ctx: Cell<Option<&'static dyn EvalCtx>>,
}

#[pymethods]
impl PyCtx {
    /// Read a single cell's value.
    fn cell<'py>(&self, py: Python<'py>, addr: &str) -> PyResult<Bound<'py, PyAny>> {
        let ctx = self.live()?;
        let value = ctx
            .cell(addr)
            .map_err(|e| PyRuntimeError::new_err(format!("{e}")))?;
        cell_value_to_py(py, &value)
    }

    /// Read a rectangular range as a flat list of values.
    fn range<'py>(&self, py: Python<'py>, start: &str, end: &str) -> PyResult<Bound<'py, PyList>> {
        let ctx = self.live()?;
        let values = ctx
            .range(start, end)
            .map_err(|e| PyRuntimeError::new_err(format!("{e}")))?;
        let items: Vec<Bound<'py, PyAny>> = values
            .iter()
            .map(|v| cell_value_to_py(py, v))
            .collect::<PyResult<_>>()?;
        PyList::new(py, items)
    }
}

impl PyCtx {
    /// The live `EvalCtx`, or an error if the bridge has been poisoned
    /// (the formula tried to use `ctx` outside its own evaluation).
    fn live(&self) -> PyResult<&'static dyn EvalCtx> {
        self.ctx
            .get()
            .ok_or_else(|| PyRuntimeError::new_err("ctx is only valid during formula evaluation"))
    }
}

/// Convert a Python result object to a `CellValue`. `bool` is checked
/// before `int` because Python's `bool` is a subclass of `int`.
fn py_to_cell_value(obj: &Bound<'_, PyAny>) -> Result<CellValue, PyEvalError> {
    if obj.is_none() {
        Ok(CellValue::Empty)
    } else if obj.is_instance_of::<PyBool>() {
        Ok(CellValue::Bool(obj.extract::<bool>().map_err(pyerr)?))
    } else if obj.is_instance_of::<PyInt>() {
        // Python ints are unbounded — fall back to f64 past the i64 range.
        match obj.extract::<i64>() {
            Ok(i) => Ok(CellValue::Integer(i)),
            Err(_) => Ok(CellValue::Number(obj.extract::<f64>().map_err(pyerr)?)),
        }
    } else if obj.is_instance_of::<PyFloat>() {
        Ok(CellValue::Number(obj.extract::<f64>().map_err(pyerr)?))
    } else if obj.is_instance_of::<PyString>() {
        Ok(CellValue::Text(obj.extract::<String>().map_err(pyerr)?))
    } else {
        Err(PyEvalError::UnsupportedResult(format!("{obj:?}")))
    }
}

/// Convert a `CellValue` to a Python object for a formula to consume.
fn cell_value_to_py<'py>(py: Python<'py>, value: &CellValue) -> PyResult<Bound<'py, PyAny>> {
    match value {
        CellValue::Empty => Ok(py.None().into_bound(py)),
        CellValue::Number(n) => Ok(PyFloat::new(py, *n).into_any()),
        CellValue::Integer(i) => Ok(i
            .into_pyobject(py)
            .map_err(|e| PyRuntimeError::new_err(format!("{e}")))?
            .into_any()),
        CellValue::Bool(b) => Ok(PyBool::new(py, *b).to_owned().into_any()),
        CellValue::Text(s) => Ok(PyString::new(py, s).into_any()),
        CellValue::Array(arr) => {
            let items: Vec<Bound<'py, PyAny>> = arr
                .data
                .iter()
                .map(|v| cell_value_to_py(py, v))
                .collect::<PyResult<_>>()?;
            Ok(PyList::new(py, items)?.into_any())
        }
        CellValue::Error(e) => Err(PyRuntimeError::new_err(format!("cell error: {e:?}"))),
        CellValue::Pending => Err(PyRuntimeError::new_err("cell value is pending")),
        CellValue::Function(_) => Err(PyRuntimeError::new_err("cell holds a function value")),
        // A stored cell value is always already dereferenced; a reference
        // reaching here is anomalous. Surface its address text rather than
        // panic, matching `stringify`'s treatment.
        CellValue::Reference(r) => {
            let s = match r {
                carbide_core::RefShape::Cell(a) => a.clone(),
                carbide_core::RefShape::Range(a, b) => format!("{a}:{b}"),
            };
            Ok(PyString::new(py, &s).into_any())
        }
    }
}

/// The Python formula engine, registered in `WorkbookEngine` as
/// `EngineKind::Python`. A Python cell's `=`-prefixed source is run as a
/// Python expression with `ctx` in scope.
pub struct PythonEngine;

impl FormulaEngine for PythonEngine {
    fn kind(&self) -> EngineKind {
        EngineKind::Python
    }

    fn parse(&self, src: &str) -> Result<CompiledFormula, ParseError> {
        // Tolerate a leading `=`, like the Excel-lite engine.
        let body = src.strip_prefix('=').unwrap_or(src).to_string();
        Ok(CompiledFormula::Python(body))
    }

    fn refs(&self, _compiled: &CompiledFormula) -> Vec<FormulaRef> {
        // A Python formula reads cells through dynamic `ctx.cell(...)`
        // calls, which aren't statically analyzable — so Python cells
        // contribute no DAG edges and don't auto-recompute when a cell
        // they read changes. A documented limit of the Python engine.
        Vec::new()
    }

    fn eval(&self, compiled: &CompiledFormula, ctx: &dyn EvalCtx) -> Result<CellValue, EvalError> {
        match compiled {
            CompiledFormula::Python(src) => {
                eval_python_with_ctx(src, ctx).map_err(|e| EvalError::Value(e.to_string()))
            }
            CompiledFormula::ExcelLite(_) => Err(EvalError::Value(
                "internal: an Excel-lite formula reached the Python engine".into(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transpile::MapCtx;

    #[test]
    fn integer_arithmetic() {
        assert_eq!(eval_python_expr("2 + 2").unwrap(), CellValue::Integer(4));
        assert_eq!(
            eval_python_expr("2 ** 10").unwrap(),
            CellValue::Integer(1024)
        );
    }

    #[test]
    fn float_arithmetic() {
        assert_eq!(eval_python_expr("3.5 * 2").unwrap(), CellValue::Number(7.0));
        assert_eq!(eval_python_expr("10 / 4").unwrap(), CellValue::Number(2.5));
    }

    #[test]
    fn booleans_resolve_before_ints() {
        assert_eq!(eval_python_expr("7 > 3").unwrap(), CellValue::Bool(true));
        assert_eq!(eval_python_expr("1 == 2").unwrap(), CellValue::Bool(false));
    }

    #[test]
    fn strings_and_builtins() {
        assert_eq!(
            eval_python_expr("'a' + 'b'").unwrap(),
            CellValue::Text("ab".to_string())
        );
        assert_eq!(
            eval_python_expr("len('hello')").unwrap(),
            CellValue::Integer(5)
        );
        assert_eq!(
            eval_python_expr("sum([1, 2, 3, 4])").unwrap(),
            CellValue::Integer(10)
        );
    }

    #[test]
    fn python_error_surfaces_not_panics() {
        assert!(eval_python_expr("undefined_name").is_err());
    }

    #[test]
    fn python_formula_reads_cells() {
        let ctx = MapCtx::from_pairs(&[
            ("A1", CellValue::Number(10.0)),
            ("A2", CellValue::Number(2.5)),
        ]);
        assert_eq!(
            eval_python_with_ctx("ctx.cell('A1') + ctx.cell('A2')", &ctx).unwrap(),
            CellValue::Number(12.5)
        );
    }

    #[test]
    fn python_formula_reads_a_range() {
        let ctx = MapCtx::from_pairs(&[
            ("A1", CellValue::Number(1.0)),
            ("A2", CellValue::Number(2.0)),
            ("A3", CellValue::Number(3.0)),
        ]);
        assert_eq!(
            eval_python_with_ctx("sum(ctx.range('A1', 'A3'))", &ctx).unwrap(),
            CellValue::Number(6.0)
        );
    }

    #[test]
    fn python_formula_uses_real_logic() {
        let ctx = MapCtx::from_pairs(&[("A1", CellValue::Number(42.0))]);
        assert_eq!(
            eval_python_with_ctx("'big' if ctx.cell('A1') > 40 else 'small'", &ctx).unwrap(),
            CellValue::Text("big".to_string())
        );
    }

    #[test]
    fn missing_cell_reads_as_none() {
        let ctx = MapCtx::default();
        // An empty cell is Python `None`; `None or 0` is 0.
        assert_eq!(
            eval_python_with_ctx("ctx.cell('Z9') or 0", &ctx).unwrap(),
            CellValue::Integer(0)
        );
    }
}
