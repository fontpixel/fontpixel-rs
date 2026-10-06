//! Charset tables, codecs and Unicode merge categories (`fontpixel::charset`).
//!
//! `CharsetCodec` is a handle over the static mapping tables. Registry
//! records (`CharsetInfo`), Unicode ranges and Windows charsets are returned
//! as read-only dicts; enums use their serde strings (`Kind`:
//! `"unicode"`, `"single_byte"`, `"double_byte"`, `"gb18030"`;
//! `EastAsianWidth`: `"ambiguous"`, `"fullwidth"`, …).
use crate::conv::{from_py_required, to_py};
use crate::errors::FeatureDisabledError;
use crate::handles::PyFont;
use fontpixel::charset::merge_categories::{
    self as mc, CJK_RANGES, EastAsianWidth, GEOMETRIC_RANGES, UNICODE_BLOCKS, UNICODE_VERSION,
    UnicodeRange,
};
use fontpixel::charset::{self, CharsetCodec, CharsetError};
use fontpixel::model::FontCharset;
use pyo3::prelude::*;
use pyo3::types::PyDict;

/// Map a `CharsetError` to `FeatureDisabledError` with `kind`
/// `"CharsetFeatureDisabled"` and the `charset` attribute.
pub fn charset_error(err: CharsetError) -> PyErr {
    Python::attach(|py| {
        let message = err.to_string();
        let exc = FeatureDisabledError::new_err(message);
        let value = exc.value(py);
        match &err {
            CharsetError::FeatureDisabled(key) => {
                let _ = value.setattr("kind", "CharsetFeatureDisabled");
                let _ = value.setattr("charset", key);
                let _ = value.setattr("feature", "cjk-charsets");
            }
        }
        exc
    })
}

/// An exact charset mapping (`fontpixel::charset::CharsetCodec`).
#[pyclass(name = "CharsetCodec", module = "fontpixel", frozen)]
pub struct PyCharsetCodec {
    codec: CharsetCodec,
}

impl PyCharsetCodec {
    /// Wrap a codec.
    pub fn new(codec: CharsetCodec) -> Self {
        Self { codec }
    }
}

#[pymethods]
impl PyCharsetCodec {
    /// CharsetCodec(name, gl=None)
    ///
    /// Resolve a user-facing charset name (`CharsetCodec::new`). `gl=None`
    /// selects GL for `iso2022cn` only. Unknown names give an identity codec
    /// with `known == False`.
    #[new]
    #[pyo3(signature = (name, gl = None))]
    fn py_new(name: &str, gl: Option<bool>) -> PyResult<Self> {
        CharsetCodec::new(name, gl)
            .map(Self::new)
            .map_err(charset_error)
    }
    /// Resolve a canonical key or cleaned alias without re-sanitizing it
    /// (`CharsetCodec::from_key`).
    #[staticmethod]
    #[pyo3(signature = (key, gl = None))]
    fn from_key(key: &str, gl: Option<bool>) -> PyResult<Self> {
        CharsetCodec::from_key(key, gl)
            .map(Self::new)
            .map_err(charset_error)
    }
    /// The codec of a font's `charset` (`FontCharset::codec`); accepts a
    /// `Font` or a `FontCharset` dict.
    #[staticmethod]
    #[pyo3(signature = (font_or_charset))]
    fn for_font(font_or_charset: &Bound<'_, PyAny>) -> PyResult<Self> {
        let charset: FontCharset = if let Ok(font) = font_or_charset.cast::<PyFont>() {
            font.get().read(|f| Ok(f.charset.clone()))?
        } else {
            from_py_required(font_or_charset)?
        };
        charset.codec().map(Self::new).map_err(charset_error)
    }
    /// Canonical key (`CharsetCodec::name`).
    #[getter]
    fn name(&self) -> &str {
        self.codec.name()
    }
    /// Whether the name resolved to a known table (`CharsetCodec::known`).
    #[getter]
    fn known(&self) -> bool {
        self.codec.known()
    }
    /// Allowed code lengths (`CharsetCodec::kind`).
    #[getter]
    fn kind<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        to_py(py, &self.codec.kind())
    }
    /// Whether 94×94 GL codes are used (`CharsetCodec::gl`).
    #[getter]
    fn gl(&self) -> bool {
        self.codec.gl()
    }
    /// Unicode value for a glyph code, or `None` (`CharsetCodec::cp2unicode`).
    #[pyo3(signature = (code))]
    fn cp2unicode(&self, code: u32) -> Option<u32> {
        self.codec.cp2unicode(code)
    }
    /// Character for a glyph code, or `None` (`CharsetCodec::cp2char`).
    #[pyo3(signature = (code))]
    fn cp2char(&self, code: u32) -> Option<char> {
        self.codec.cp2char(code)
    }
    /// Glyph code for a Unicode value, or `None` (`CharsetCodec::unicode2cp`).
    #[pyo3(signature = (value))]
    fn unicode2cp(&self, value: u32) -> Option<u32> {
        self.codec.unicode2cp(value)
    }
    /// Glyph code for a character, or `None` (`CharsetCodec::char2cp`).
    #[pyo3(signature = (value))]
    fn char2cp(&self, value: char) -> Option<u32> {
        self.codec.char2cp(value)
    }
    fn __repr__(&self) -> String {
        format!(
            "CharsetCodec(name={:?}, known={}, gl={})",
            self.codec.name(),
            self.codec.known(),
            self.codec.gl()
        )
    }
}

/// Every canonical charset (`charset_table`), as dicts with `key`,
/// `python_codec`, `kind`, `gl_capable`, `force_gl`, `properties`,
/// `aliases` and `available` (`CharsetInfo::available`).
#[pyfunction]
fn charset_table(py: Python<'_>) -> PyResult<Vec<Bound<'_, PyAny>>> {
    charset::charset_table()
        .iter()
        .map(|info| {
            let d = to_py(py, info)?;
            d.cast::<PyDict>()?
                .set_item("available", info.available())?;
            Ok(d)
        })
        .collect()
}

/// Whether a canonical key denotes Unicode identity (`is_unicode_charset`).
#[pyfunction]
#[pyo3(signature = (key))]
fn is_unicode_charset(key: &str) -> bool {
    charset::is_unicode_charset(key)
}

/// Canonical key from XLFD registry/encoding (`sanitize_charset`).
#[pyfunction]
#[pyo3(signature = (registry = None, encoding = None))]
fn sanitize_charset(registry: Option<&str>, encoding: Option<&str>) -> String {
    charset::sanitize_charset(registry, encoding)
}

/// Canonical key for a user-facing name (`canonical_charset`).
#[pyfunction]
#[pyo3(signature = (name))]
fn canonical_charset(name: &str) -> String {
    charset::canonical_charset(name)
}

/// `(CHARSET_REGISTRY, CHARSET_ENCODING)` for a key (`charset_props`).
#[pyfunction]
#[pyo3(signature = (key))]
fn charset_props(key: &str) -> (String, String) {
    charset::charset_props(key)
}

/// Infer the GL form from glyph codes (`detect_gl`).
#[pyfunction]
#[pyo3(signature = (key, encoding, codes))]
fn detect_gl(key: &str, encoding: Option<&str>, codes: Vec<u32>) -> bool {
    charset::detect_gl(key, encoding, codes)
}

/// Resolve a Windows `dfCharSet` byte (`windows_charset`): `{"code_page":
/// "cp1252"}`, `"system_ansi"`, `"system_oem"`, `"system_mac"`, `"symbol"` or
/// `{"unknown": n}`.
#[pyfunction]
#[pyo3(signature = (value))]
fn windows_charset(py: Python<'_>, value: u8) -> PyResult<Bound<'_, PyAny>> {
    to_py(py, &charset::windows_charset(value))
}

/// One-based 94×94 row/cell to an EUC code, or `None` (`kuten_to_euc`).
#[pyfunction]
#[pyo3(signature = (row, cell))]
fn kuten_to_euc(row: u8, cell: u8) -> Option<u16> {
    charset::kuten_to_euc(row, cell)
}

/// EUC code to one-based `(row, cell)`, or `None` (`euc_to_kuten`).
#[pyfunction]
#[pyo3(signature = (code))]
fn euc_to_kuten(code: u16) -> Option<(u8, u8)> {
    charset::euc_to_kuten(code)
}

/// Canonical charset name of a `FontCharset` dict or `Font`
/// (`FontCharset::canonical_name`).
#[pyfunction]
#[pyo3(signature = (font_or_charset))]
fn charset_canonical_name(font_or_charset: &Bound<'_, PyAny>) -> PyResult<String> {
    if let Ok(font) = font_or_charset.cast::<PyFont>() {
        return font.get().read(|f| Ok(f.charset.canonical_name()));
    }
    let charset: FontCharset = from_py_required(font_or_charset)?;
    Ok(charset.canonical_name())
}

// merge_categories ----------------------------------------------------------

/// Whether a value is a Unicode scalar (`merge_categories::is_scalar`).
#[pyfunction]
#[pyo3(signature = (cp))]
fn is_scalar(cp: u32) -> bool {
    mc::is_scalar(cp)
}
/// Explicit CJK block policy (`merge_categories::is_cjk`).
#[pyfunction]
#[pyo3(signature = (cp))]
fn is_cjk(cp: u32) -> bool {
    mc::is_cjk(cp)
}
/// Scalar complement of CJK (`merge_categories::is_western`).
#[pyfunction]
#[pyo3(signature = (cp))]
fn is_western(cp: u32) -> bool {
    mc::is_western(cp)
}
/// Box/block/braille/Powerline ranges (`merge_categories::is_geometric`).
#[pyfunction]
#[pyo3(signature = (cp))]
fn is_geometric(cp: u32) -> bool {
    mc::is_geometric(cp)
}
/// Han ideograph blocks (`merge_categories::is_han`).
#[pyfunction]
#[pyo3(signature = (cp))]
fn is_han(cp: u32) -> bool {
    mc::is_han(cp)
}
/// The Unicode block `{start, end, name}`, or `None` (`merge_categories::unicode_block`).
#[pyfunction]
#[pyo3(signature = (cp))]
fn unicode_block(py: Python<'_>, cp: u32) -> PyResult<Option<Bound<'_, PyAny>>> {
    mc::unicode_block(cp).map(|r| to_py(py, r)).transpose()
}
/// East Asian Width property, or `None` for non-scalars
/// (`merge_categories::east_asian_width`).
#[pyfunction]
#[pyo3(signature = (cp))]
fn east_asian_width(py: Python<'_>, cp: u32) -> PyResult<Option<Bound<'_, PyAny>>> {
    mc::east_asian_width(cp)
        .map(|w: EastAsianWidth| to_py(py, &w))
        .transpose()
}

fn ranges<'py>(py: Python<'py>, ranges: &[UnicodeRange]) -> PyResult<Bound<'py, PyAny>> {
    let list = ranges
        .iter()
        .map(|r| to_py(py, r))
        .collect::<PyResult<Vec<_>>>()?;
    Ok(pyo3::types::PyTuple::new(py, list)?.into_any())
}

/// Register the charset class, functions and constants.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = m.py();
    m.add_class::<PyCharsetCodec>()?;
    for f in [
        wrap_pyfunction!(charset_table, m)?,
        wrap_pyfunction!(is_unicode_charset, m)?,
        wrap_pyfunction!(sanitize_charset, m)?,
        wrap_pyfunction!(canonical_charset, m)?,
        wrap_pyfunction!(charset_props, m)?,
        wrap_pyfunction!(detect_gl, m)?,
        wrap_pyfunction!(windows_charset, m)?,
        wrap_pyfunction!(kuten_to_euc, m)?,
        wrap_pyfunction!(euc_to_kuten, m)?,
        wrap_pyfunction!(charset_canonical_name, m)?,
        wrap_pyfunction!(is_scalar, m)?,
        wrap_pyfunction!(is_cjk, m)?,
        wrap_pyfunction!(is_western, m)?,
        wrap_pyfunction!(is_geometric, m)?,
        wrap_pyfunction!(is_han, m)?,
        wrap_pyfunction!(unicode_block, m)?,
        wrap_pyfunction!(east_asian_width, m)?,
    ] {
        m.add_function(f)?;
    }
    m.add("UNICODE_VERSION", UNICODE_VERSION)?;
    m.add("CJK_RANGES", ranges(py, CJK_RANGES)?)?;
    m.add("GEOMETRIC_RANGES", ranges(py, GEOMETRIC_RANGES)?)?;
    m.add("UNICODE_BLOCKS", ranges(py, UNICODE_BLOCKS)?)?;
    Ok(())
}
