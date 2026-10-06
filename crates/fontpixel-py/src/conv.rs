//! serde conversions between Rust values and Python objects.
//!
//! Typed options and plain data use these helpers, so every option struct that
//! derives serde is available as a Python dict with the same field names. Unknown
//! keys are rejected by the types' own `deny_unknown_fields`.
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use serde::Serialize;
use serde::de::DeserializeOwned;

/// Serialize a Rust value to a Python object (dicts, lists, ints, strings, …).
pub fn to_py<'py, T: Serialize + ?Sized>(py: Python<'py>, value: &T) -> PyResult<Bound<'py, PyAny>> {
    pythonize::pythonize(py, value).map_err(|e| PyValueError::new_err(e.to_string()))
}

/// Deserialize a Python object into a Rust value; `None` gives the type's default.
pub fn from_py<T: DeserializeOwned + Default>(obj: Option<&Bound<'_, PyAny>>) -> PyResult<T> {
    match obj {
        None => Ok(T::default()),
        Some(o) if o.is_none() => Ok(T::default()),
        Some(o) => pythonize::depythonize(o).map_err(|e| PyValueError::new_err(e.to_string())),
    }
}

/// Deserialize a required Python object into a Rust value.
pub fn from_py_required<T: DeserializeOwned>(obj: &Bound<'_, PyAny>) -> PyResult<T> {
    pythonize::depythonize(obj).map_err(|e| PyValueError::new_err(e.to_string()))
}
