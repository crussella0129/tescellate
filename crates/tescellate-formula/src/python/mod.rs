//! Python formula engine — embedded CPython via PyO3. See PLAN.md §6.2.
//!
//! Feature-gated (`python`): pulls in `pyo3` and links libpython.
//!
//! v8 slice 1 proves the embedding — evaluate a pure Python expression and
//! convert its result to a `CellValue`. The `ctx` bridge that lets a Python
//! formula read cells, and the `FormulaEngine` integration, land in slice 2.

use pyo3::prelude::*;
use pyo3::types::{PyBool, PyFloat, PyInt, PyString};
use tescellate_core::CellValue;

/// Failure evaluating a Python formula.
#[derive(Debug, thiserror::Error)]
pub enum PyEvalError {
    #[error("python: {0}")]
    Py(String),
    #[error("python: unsupported result type: {0}")]
    UnsupportedResult(String),
}

/// Evaluate a Python expression in an embedded interpreter and convert its
/// result to a `CellValue`.
pub fn eval_python_expr(src: &str) -> Result<CellValue, PyEvalError> {
    let code = std::ffi::CString::new(src)
        .map_err(|_| PyEvalError::Py("source contains an interior NUL byte".into()))?;
    Python::with_gil(|py| {
        let obj = py
            .eval(code.as_c_str(), None, None)
            .map_err(|e| PyEvalError::Py(e.to_string()))?;
        py_to_cell_value(&obj)
    })
}

/// Convert a Python result object to a `CellValue`. `bool` is checked
/// before `int` because Python's `bool` is a subclass of `int`.
fn py_to_cell_value(obj: &Bound<'_, PyAny>) -> Result<CellValue, PyEvalError> {
    let to_err = |e: PyErr| PyEvalError::Py(e.to_string());
    if obj.is_none() {
        Ok(CellValue::Empty)
    } else if obj.is_instance_of::<PyBool>() {
        Ok(CellValue::Bool(obj.extract::<bool>().map_err(to_err)?))
    } else if obj.is_instance_of::<PyInt>() {
        // Python ints are unbounded — fall back to f64 past the i64 range.
        match obj.extract::<i64>() {
            Ok(i) => Ok(CellValue::Integer(i)),
            Err(_) => Ok(CellValue::Number(obj.extract::<f64>().map_err(to_err)?)),
        }
    } else if obj.is_instance_of::<PyFloat>() {
        Ok(CellValue::Number(obj.extract::<f64>().map_err(to_err)?))
    } else if obj.is_instance_of::<PyString>() {
        Ok(CellValue::Text(obj.extract::<String>().map_err(to_err)?))
    } else {
        Err(PyEvalError::UnsupportedResult(format!("{obj:?}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        // Python `bool` subclasses `int`; the conversion must not collapse
        // `True` to `Integer(1)`.
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
        // A NameError must come back as an error, not a panic.
        assert!(eval_python_expr("undefined_name").is_err());
    }
}
