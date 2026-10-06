//! Shared helpers for the OpenType, WOFF, variants, outline and merge bindings.
//!
//! - Byte inputs accept `bytes` (borrowed without copying), `bytearray` (copied)
//!   and any other object `bytes(obj)` accepts, such as `memoryview`.
//! - Four-byte OpenType tags accept a four-character Latin-1 `str` (the serde
//!   form, so `"OS/2"`) or a four-byte `bytes` object, and come back as `str`.
//! - Result structs whose Rust fields are `Vec<u8>` are returned as dicts whose
//!   byte fields are Python `bytes`; nested model dicts keep the serde shape
//!   (lists of integers for `Vec<u8>`), which input accepts as `bytes` too.
//! - Native submodules are registered in `sys.modules` under their dotted names,
//!   so `import fontpixel.sfnt.cmap` works without Python shim files.
use crate::conv;
use crate::errors::InvalidInputError;
use crate::handles::PyBitmap;
use fontpixel::model::GlyphBitmap;
use pyo3::prelude::*;
use pyo3::pybacked::PyBackedBytes;
use pyo3::types::{PyBytes, PyDict};
use serde::Serialize;
use serde::de::DeserializeOwned;

/// Read a byte buffer argument. A `bytes` object is shared without copying
/// (handles keep it alive); anything else `crate::bitmap::bytes_arg` accepts
/// (`bytearray`, `memoryview`, int sequences) is copied once.
pub fn bytes_arg(obj: &Bound<'_, PyAny>) -> PyResult<PyBackedBytes> {
    if let Ok(b) = obj.cast::<PyBytes>() {
        return Ok(b.clone().into());
    }
    let copied = crate::bitmap::bytes_arg(obj)?;
    Ok(PyBytes::new(obj.py(), &copied).into())
}

/// Parse a four-byte tag from `str` (Latin-1 characters) or `bytes`.
pub fn tag_arg(obj: &Bound<'_, PyAny>) -> PyResult<[u8; 4]> {
    if let Ok(text) = obj.extract::<String>() {
        let mut out = [0u8; 4];
        let mut count = 0;
        for c in text.chars() {
            let byte = u8::try_from(u32::from(c)).map_err(|_| {
                InvalidInputError::new_err(format!("tag {text:?} has a character above U+00FF"))
            })?;
            if count == 4 {
                return Err(InvalidInputError::new_err(format!(
                    "tag {text:?} must be four characters"
                )));
            }
            out[count] = byte;
            count += 1;
        }
        if count != 4 {
            return Err(InvalidInputError::new_err(format!(
                "tag {text:?} must be four characters"
            )));
        }
        return Ok(out);
    }
    let bytes = bytes_arg(obj)?;
    <[u8; 4]>::try_from(&*bytes)
        .map_err(|_| InvalidInputError::new_err("tag bytes must have length four"))
}

/// The serde string form of a tag (each byte as one Latin-1 character).
pub fn tag_str(tag: [u8; 4]) -> String {
    tag.iter().map(|&b| char::from(b)).collect()
}

/// Deserialize an optional options/limits dict; `None` gives the default.
pub fn opt<T: DeserializeOwned + Default>(obj: Option<&Bound<'_, PyAny>>) -> PyResult<T> {
    conv::from_py(obj)
}

/// Deserialize a required dict or value.
pub fn req<T: DeserializeOwned>(obj: &Bound<'_, PyAny>) -> PyResult<T> {
    conv::from_py_required(obj)
}

/// Serialize a value with serde.
pub fn out<'py, T: Serialize + ?Sized>(py: Python<'py>, value: &T) -> PyResult<Bound<'py, PyAny>> {
    conv::to_py(py, value)
}

/// Python `bytes` from a slice.
pub fn pybytes<'py>(py: Python<'py>, value: &[u8]) -> Bound<'py, PyAny> {
    PyBytes::new(py, value).into_any()
}

/// Build a result dict from `(key, value)` pairs.
pub fn dict<'py>(
    py: Python<'py>,
    items: Vec<(&str, Bound<'py, PyAny>)>,
) -> PyResult<Bound<'py, PyAny>> {
    let d = PyDict::new(py);
    for (k, v) in items {
        d.set_item(k, v)?;
    }
    Ok(d.into_any())
}

/// The serde dict of `value` with selected byte fields replaced by `bytes`.
pub fn with_bytes<'py, T: Serialize>(
    py: Python<'py>,
    value: &T,
    fields: &[(&str, &[u8])],
) -> PyResult<Bound<'py, PyAny>> {
    let d = out(py, value)?;
    let dict = d.cast::<PyDict>()?;
    for (k, v) in fields {
        dict.set_item(*k, PyBytes::new(py, v))?;
    }
    Ok(d)
}

/// The Mono raster of a `Bitmap` handle or bitmap dict (copied); Gray8 needs
/// explicit thresholding first.
pub fn mono(bitmap: &Bound<'_, PyAny>) -> PyResult<fontpixel::Bitmap> {
    match crate::bitmap::bitmap_arg(bitmap)? {
        GlyphBitmap::Mono(m) => Ok(m),
        GlyphBitmap::Gray8(_) => Err(InvalidInputError::new_err(
            "this operation needs a Mono bitmap; threshold Gray8 explicitly first",
        )),
    }
}

/// A new Mono bitmap handle.
pub fn mono_handle(bitmap: fontpixel::Bitmap) -> PyBitmap {
    PyBitmap::new(GlyphBitmap::Mono(bitmap))
}

/// Create a native submodule `parent.name`, registered in `sys.modules` as
/// `qualified`, so `import fontpixel.sfnt.cmap` works.
pub fn submodule<'py>(
    parent: &Bound<'py, PyModule>,
    name: &str,
    qualified: &str,
    doc: &str,
) -> PyResult<Bound<'py, PyModule>> {
    let py = parent.py();
    let m = PyModule::new(py, name)?;
    m.setattr("__doc__", doc)?;
    parent.add(name, &m)?;
    m.setattr("__name__", qualified)?;
    py.import("sys")?
        .getattr("modules")?
        .set_item(qualified, &m)?;
    Ok(m)
}
