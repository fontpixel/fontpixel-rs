//! Python methods of the model handles (`FontSet`, `Font`, `Glyph`), plus the
//! `Codepoint` value class and the `Properties` list handle.
//!
//! Plain model data (`FontMetrics`, `FontNames`, `FontCharset`,
//! `GlyphMetrics`, `Bounds`, `Vector`, `Rational`, …) crosses the boundary as
//! dicts with the Rust field names. Reading such a field returns a copy;
//! assign the edited dict back to change the model. Glyph IDs are `int`s.
//!
//! Locking rule used throughout: every handle argument is copied (`snapshot`)
//! before the receiver is locked, so passing a view of the same font or set
//! never deadlocks.
use crate::bitmap::bitmap_arg;
use crate::conv::{from_py, from_py_required, to_py};
use crate::errors::{IntoPy, to_py as error_to_py};
use crate::handles::{
    FontLoc, GlyphLoc, PyBitmap, PyFont, PyFontSet, PyGlyph, Shared, read_font, write_font,
};
use fontpixel::ResourceLimits;
use fontpixel::bitmap::Bounds;
use fontpixel::model::{
    Codepoint, Font, FontCharset, FontMetrics, FontNames, FontSet, Glyph, GlyphBitmap, GlyphId,
    GlyphMetrics, Properties, Property, PropertyValue,
};
use pyo3::exceptions::{PyIndexError, PyKeyError, PyTypeError};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyByteArray, PyBytes, PyInt, PyIterator, PyList, PyString};
use std::sync::{Arc, RwLock};

/// An `InvalidInputError` built through `errors::to_py`, so it carries `kind` and
/// `exit_code` like errors raised by the core.
pub fn invalid(message: impl Into<String>) -> PyErr {
    error_to_py(fontpixel::Error::InvalidInput(message.into()))
}

fn poisoned() -> PyErr {
    invalid("handle state was poisoned by an earlier panic")
}

/// Python-style repr of an optional string.
fn opt_repr(value: Option<&str>) -> String {
    value.map_or_else(
        || "None".to_owned(),
        |v| format!("'{}'", v.replace('\\', "\\\\").replace('\'', "\\'")),
    )
}

fn new_shared<T>(value: T) -> Shared<T> {
    Arc::new(RwLock::new(value))
}

/// A Python iterator over a list of already built items.
fn iter_list<'py, T>(py: Python<'py>, items: Vec<T>) -> PyResult<Bound<'py, PyIterator>>
where
    T: IntoPyObject<'py>,
{
    PyList::new(py, items)?.as_any().try_iter()
}

// ---------------------------------------------------------------------------
// Codepoint
// ---------------------------------------------------------------------------

/// A tagged character code (`fontpixel::model::Codepoint`): either a Unicode
/// scalar value or a legacy code in the font's own charset.
///
/// Functions that take a codepoint also accept an `int` (Unicode), a
/// one-character `str` (Unicode) or the plain-data dict `{"Unicode": n}` /
/// `{"Legacy": n}`.
#[pyclass(
    name = "Codepoint",
    module = "fontpixel",
    frozen,
    eq,
    hash,
    ord,
    from_py_object
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PyCodepoint(pub Codepoint);

#[pymethods]
impl PyCodepoint {
    /// Codepoint(value, kind="unicode")
    ///
    /// `kind` is `"unicode"` or `"legacy"`.
    #[new]
    #[pyo3(signature = (value, kind = "unicode"))]
    fn py_new(value: u32, kind: &str) -> PyResult<Self> {
        match kind {
            "unicode" => Ok(Self(Codepoint::Unicode(value))),
            "legacy" => Ok(Self(Codepoint::Legacy(value))),
            _ => Err(invalid(format!(
                "codepoint kind must be 'unicode' or 'legacy', not '{kind}'"
            ))),
        }
    }
    /// A Unicode codepoint (`Codepoint::Unicode`).
    #[staticmethod]
    #[pyo3(signature = (value))]
    fn unicode(value: u32) -> Self {
        Self(Codepoint::Unicode(value))
    }
    /// A legacy charset code (`Codepoint::Legacy`).
    #[staticmethod]
    #[pyo3(signature = (value))]
    fn legacy(value: u32) -> Self {
        Self(Codepoint::Legacy(value))
    }
    /// The numeric value regardless of kind (`Codepoint::value`).
    #[getter]
    fn value(&self) -> u32 {
        self.0.value()
    }
    /// `"unicode"` or `"legacy"`.
    #[getter]
    fn kind(&self) -> &'static str {
        match self.0 {
            Codepoint::Unicode(_) => "unicode",
            Codepoint::Legacy(_) => "legacy",
        }
    }
    /// Whether this is a Unicode codepoint.
    #[getter]
    fn is_unicode(&self) -> bool {
        matches!(self.0, Codepoint::Unicode(_))
    }
    /// The character for a Unicode scalar value, or `None` for legacy codes
    /// and surrogates (`Codepoint::scalar`).
    fn scalar(&self) -> Option<char> {
        self.0.scalar()
    }
    /// Plain-data form: `{"Unicode": n}` or `{"Legacy": n}`.
    #[pyo3(name = "to_dict")]
    fn plain<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        to_py(py, &self.0)
    }
    /// Build from the plain-data form (or any accepted codepoint value).
    #[staticmethod]
    #[pyo3(signature = (value))]
    fn from_dict(value: &Bound<'_, PyAny>) -> PyResult<Self> {
        codepoint_arg(value).map(Self)
    }
    fn __int__(&self) -> u32 {
        self.0.value()
    }
    fn __repr__(&self) -> String {
        format!("Codepoint.{}(0x{:04X})", self.kind(), self.0.value())
    }
}

/// Extract a codepoint from a `Codepoint`, `int`, one-character `str` or dict.
pub fn codepoint_arg(obj: &Bound<'_, PyAny>) -> PyResult<Codepoint> {
    if let Ok(cp) = obj.cast::<PyCodepoint>() {
        return Ok(cp.get().0);
    }
    if obj.is_instance_of::<PyBool>() {
        return Err(PyTypeError::new_err("a bool is not a codepoint"));
    }
    if obj.is_instance_of::<PyInt>() {
        return Ok(Codepoint::Unicode(obj.extract()?));
    }
    if let Ok(s) = obj.cast::<PyString>() {
        let s = s.to_str()?;
        let mut chars = s.chars();
        return match (chars.next(), chars.next()) {
            (Some(c), None) => Ok(Codepoint::Unicode(u32::from(c))),
            _ => Err(PyTypeError::new_err(
                "a codepoint string must be exactly one character",
            )),
        };
    }
    from_py_required(obj)
}

fn codepoints_arg(obj: &Bound<'_, PyAny>) -> PyResult<Vec<Codepoint>> {
    obj.try_iter()?.map(|item| codepoint_arg(&item?)).collect()
}

// ---------------------------------------------------------------------------
// Properties
// ---------------------------------------------------------------------------

/// Where a `Properties` handle's list lives.
#[derive(Clone)]
pub enum PropsLoc {
    /// Standalone list.
    Owned(Shared<Properties>),
    /// `FontSet.properties`.
    Set(Shared<FontSet>),
    /// `Font.properties`.
    Font(FontLoc),
    /// `Font.headers`.
    Headers(FontLoc),
    /// `Font.names["localized"]`.
    Localized(FontLoc),
    /// `Glyph.properties`.
    Glyph(GlyphLoc),
}

/// An ordered list of named properties (`fontpixel::model::Properties`).
///
/// Obtained from `FontSet.properties`, `Font.properties`, `Font.headers`,
/// `Font.localized_names` or `Glyph.properties`, it is a live view: `set`,
/// `push` and `remove` edit the owning model in place. `Properties(items)`
/// builds a standalone list; `copy()` detaches a view.
///
/// Values are given as Python values (`int` → Integer, or Unsigned above
/// the `i64` range; `str` → Text; `bytes` → Bytes; `list` → List;
/// `None` → Flag) or as the tagged plain-data form (`{"Decimal": "1.5"}`).
/// Values are returned in the tagged form, which keeps every variant.
#[pyclass(name = "Properties", module = "fontpixel", frozen)]
pub struct PyProperties {
    /// Storage location.
    pub loc: PropsLoc,
}

impl PyProperties {
    /// Wrap an owned list.
    pub fn owned(props: Properties) -> Self {
        Self {
            loc: PropsLoc::Owned(new_shared(props)),
        }
    }
    /// Run `f` with shared access.
    pub fn read<R>(&self, f: impl FnOnce(&Properties) -> PyResult<R>) -> PyResult<R> {
        match &self.loc {
            PropsLoc::Owned(p) => f(&*p.read().map_err(|_| poisoned())?),
            PropsLoc::Set(s) => f(&s.read().map_err(|_| poisoned())?.properties),
            PropsLoc::Font(l) => read_font(l, |font| f(&font.properties)),
            PropsLoc::Headers(l) => read_font(l, |font| f(&font.headers)),
            PropsLoc::Localized(l) => read_font(l, |font| f(&font.names.localized)),
            PropsLoc::Glyph(l) => PyGlyph { loc: l.clone() }.read(|g| f(&g.properties)),
        }
    }
    /// Run `f` with exclusive access.
    pub fn write<R>(&self, f: impl FnOnce(&mut Properties) -> PyResult<R>) -> PyResult<R> {
        match &self.loc {
            PropsLoc::Owned(p) => f(&mut *p.write().map_err(|_| poisoned())?),
            PropsLoc::Set(s) => f(&mut s.write().map_err(|_| poisoned())?.properties),
            PropsLoc::Font(l) => write_font(l, |font| f(&mut font.properties)),
            PropsLoc::Headers(l) => write_font(l, |font| f(&mut font.headers)),
            PropsLoc::Localized(l) => write_font(l, |font| f(&mut font.names.localized)),
            PropsLoc::Glyph(l) => PyGlyph { loc: l.clone() }.write(|g| f(&mut g.properties)),
        }
    }
}

/// Convert a Python value to a `PropertyValue` (see `Properties`).
pub fn property_value_arg(obj: &Bound<'_, PyAny>) -> PyResult<PropertyValue> {
    if obj.is_none() {
        return Ok(PropertyValue::Flag);
    }
    if obj.is_instance_of::<PyBool>() {
        return Err(PyTypeError::new_err(
            "a bool is not a property value; use None for a flag",
        ));
    }
    if obj.is_instance_of::<PyInt>() {
        if let Ok(v) = obj.extract::<i64>() {
            return Ok(PropertyValue::Integer(v));
        }
        return Ok(PropertyValue::Unsigned(obj.extract()?));
    }
    if let Ok(s) = obj.cast::<PyString>() {
        return Ok(PropertyValue::Text(s.to_str()?.to_owned()));
    }
    if let Ok(b) = obj.cast::<PyBytes>() {
        return Ok(PropertyValue::Bytes(b.as_bytes().to_vec()));
    }
    if let Ok(b) = obj.cast::<PyByteArray>() {
        return Ok(PropertyValue::Bytes(b.to_vec()));
    }
    if let Ok(list) = obj.cast::<PyList>() {
        return list
            .iter()
            .map(|v| property_value_arg(&v))
            .collect::<PyResult<Vec<_>>>()
            .map(PropertyValue::List);
    }
    from_py_required(obj)
}

/// Extract a property list from a `Properties` handle or a list of dicts.
pub fn properties_arg(obj: &Bound<'_, PyAny>) -> PyResult<Properties> {
    if let Ok(p) = obj.cast::<PyProperties>() {
        return p.get().read(|p| Ok(p.clone()));
    }
    from_py_required(obj)
}

fn opt_properties(obj: Option<&Bound<'_, PyAny>>) -> PyResult<Properties> {
    match obj {
        Some(o) if !o.is_none() => properties_arg(o),
        _ => Ok(Properties::default()),
    }
}

#[pymethods]
impl PyProperties {
    /// Properties(items=None)
    ///
    /// A standalone list from another `Properties` or a list of
    /// `{"name": str, "value": <tagged value>, "raw"?: bytes}` dicts.
    #[new]
    #[pyo3(signature = (items = None))]
    fn py_new(items: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        Ok(Self::owned(opt_properties(items)?))
    }
    fn __len__(&self) -> PyResult<usize> {
        self.read(|p| Ok(p.len()))
    }
    /// Whether the list is empty (`Properties::is_empty`).
    fn is_empty(&self) -> PyResult<bool> {
        self.read(|p| Ok(p.is_empty()))
    }
    fn __iter__<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyIterator>> {
        let list = self.to_list(py)?;
        list.as_any().try_iter()
    }
    fn __getitem__<'py>(&self, py: Python<'py>, index: isize) -> PyResult<Bound<'py, PyAny>> {
        self.read(|p| {
            let len = p.len() as isize;
            let i = if index < 0 { index + len } else { index };
            let prop = p
                .iter()
                .nth(usize::try_from(i).map_err(|_| PyIndexError::new_err("property index"))?)
                .ok_or_else(|| PyIndexError::new_err("property index out of range"))?;
            to_py(py, prop)
        })
    }
    /// The value of the last property named exactly `name`, in tagged form,
    /// or `None` (`Properties::get`).
    #[pyo3(signature = (name))]
    fn get<'py>(&self, py: Python<'py>, name: &str) -> PyResult<Option<Bound<'py, PyAny>>> {
        self.read(|p| p.get(name).map(|v| to_py(py, v)).transpose())
    }
    /// Like `get`, comparing names ASCII-case-insensitively
    /// (`Properties::get_case_insensitive`).
    #[pyo3(signature = (name))]
    fn get_case_insensitive<'py>(
        &self,
        py: Python<'py>,
        name: &str,
    ) -> PyResult<Option<Bound<'py, PyAny>>> {
        self.read(|p| {
            p.get_case_insensitive(name)
                .map(|v| to_py(py, v))
                .transpose()
        })
    }
    /// Replace the last property whose name matches case-insensitively, or
    /// append a new one (`Properties::set`).
    #[pyo3(signature = (name, value))]
    fn set(&self, name: String, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let value = property_value_arg(value)?;
        self.write(|p| {
            p.set(name, value);
            Ok(())
        })
    }
    /// Append a property, keeping duplicates (`Properties::push` with
    /// `Property::new`; `raw` is the optional source spelling).
    #[pyo3(signature = (name, value, raw = None))]
    fn push(&self, name: String, value: &Bound<'_, PyAny>, raw: Option<Vec<u8>>) -> PyResult<()> {
        let mut property = Property::new(name, property_value_arg(value)?);
        property.raw = raw;
        self.write(|p| {
            p.push(property);
            Ok(())
        })
    }
    /// Remove every property whose name matches case-insensitively; returns
    /// the number removed (`Properties::remove`).
    #[pyo3(signature = (name))]
    fn remove(&self, name: &str) -> PyResult<usize> {
        self.write(|p| Ok(p.remove(name)))
    }
    /// A list of property dicts (`Properties::iter` / `into_vec`).
    fn to_list<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyList>> {
        self.read(|p| {
            let items = p
                .iter()
                .map(|prop| to_py(py, prop))
                .collect::<PyResult<Vec<_>>>()?;
            PyList::new(py, items)
        })
    }
    /// A standalone copy.
    fn copy(&self) -> PyResult<Self> {
        self.read(|p| Ok(Self::owned(p.clone())))
    }
    fn __copy__(&self) -> PyResult<Self> {
        self.copy()
    }
    fn __deepcopy__(&self, _memo: &Bound<'_, PyAny>) -> PyResult<Self> {
        self.copy()
    }
    fn __eq__(&self, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        let Ok(other) = properties_arg(other) else {
            return Ok(false);
        };
        self.read(|p| Ok(*p == other))
    }
    fn __repr__(&self) -> PyResult<String> {
        self.read(|p| {
            let names: Vec<_> = p.iter().map(|q| q.name.as_str()).collect();
            Ok(format!("Properties({names:?})"))
        })
    }
}

// ---------------------------------------------------------------------------
// FontSet
// ---------------------------------------------------------------------------

fn font_arg(obj: &Bound<'_, PyAny>) -> PyResult<Font> {
    if let Ok(f) = obj.cast::<PyFont>() {
        return f.get().snapshot();
    }
    from_py_required(obj)
}

fn fonts_arg(obj: &Bound<'_, PyAny>) -> PyResult<Vec<Font>> {
    obj.try_iter()?.map(|f| font_arg(&f?)).collect()
}

fn usage<'py>(
    py: Python<'py>,
    max_bytes: Option<usize>,
    f: impl FnOnce(usize) -> fontpixel::Result<fontpixel::model::ModelUsage>,
) -> PyResult<Bound<'py, PyAny>> {
    let usage = f(max_bytes.unwrap_or(usize::MAX)).py()?;
    to_py(py, &usage)
}

#[pymethods]
impl PyFontSet {
    /// FontSet(fonts=None, properties=None, source_format=None)
    ///
    /// A set of strikes. `fonts` are copied into the set.
    #[new]
    #[pyo3(signature = (fonts = None, properties = None, source_format = None))]
    fn py_new(
        fonts: Option<&Bound<'_, PyAny>>,
        properties: Option<&Bound<'_, PyAny>>,
        source_format: Option<String>,
    ) -> PyResult<Self> {
        let fonts = match fonts {
            Some(f) if !f.is_none() => fonts_arg(f)?,
            _ => Vec::new(),
        };
        Ok(Self::new(FontSet {
            fonts,
            properties: opt_properties(properties)?,
            source_format,
        }))
    }
    /// Build from the plain-data form produced by `to_dict` (serde shape).
    #[staticmethod]
    #[pyo3(signature = (data))]
    fn from_dict(data: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self::new(from_py_required(data)?))
    }
    /// The complete plain-data form (`{"fonts": [...], "properties": [...],
    /// "source_format"?: str}`); copies every pixel.
    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.read(|s| to_py(py, s))
    }
    /// Live views of the strikes, in order. Assigning a list of fonts
    /// replaces them (copies). Views address fonts by index.
    #[getter]
    fn fonts(&self) -> PyResult<Vec<PyFont>> {
        let n = self.read(|s| Ok(s.fonts.len()))?;
        Ok((0..n).map(|i| PyFont::view(&self.set, i)).collect())
    }
    #[setter]
    fn set_fonts(&self, fonts: &Bound<'_, PyAny>) -> PyResult<()> {
        let fonts = fonts_arg(fonts)?;
        self.write(|s| {
            s.fonts = fonts;
            Ok(())
        })
    }
    /// Set-level properties (live `Properties` view); assignable.
    #[getter]
    fn properties(&self) -> PyProperties {
        PyProperties {
            loc: PropsLoc::Set(self.set.clone()),
        }
    }
    #[setter]
    fn set_properties(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let props = properties_arg(value)?;
        self.write(|s| {
            s.properties = props;
            Ok(())
        })
    }
    /// The format ID the set was read from, if any.
    #[getter]
    fn source_format(&self) -> PyResult<Option<String>> {
        self.read(|s| Ok(s.source_format.clone()))
    }
    #[setter]
    fn set_source_format(&self, value: Option<String>) -> PyResult<()> {
        self.write(|s| {
            s.source_format = value;
            Ok(())
        })
    }
    fn __len__(&self) -> PyResult<usize> {
        self.read(|s| Ok(s.fonts.len()))
    }
    fn __getitem__(&self, index: isize) -> PyResult<PyFont> {
        let n = self.read(|s| Ok(s.fonts.len()))? as isize;
        let i = if index < 0 { index + n } else { index };
        if i < 0 || i >= n {
            return Err(PyIndexError::new_err("font index out of range"));
        }
        Ok(PyFont::view(&self.set, i as usize))
    }
    fn __iter__<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyIterator>> {
        iter_list(py, self.fonts()?)
    }
    /// Append a copy of `font`; returns its index.
    #[pyo3(signature = (font))]
    fn append(&self, font: &Bound<'_, PyAny>) -> PyResult<usize> {
        let font = font_arg(font)?;
        self.write(|s| {
            s.fonts.push(font);
            Ok(s.fonts.len() - 1)
        })
    }
    /// Remove and return strike `index` (default: the last) as an owned font.
    /// Views of later strikes now address the following font.
    #[pyo3(signature = (index = -1))]
    fn pop(&self, index: isize) -> PyResult<PyFont> {
        self.write(|s| {
            let n = s.fonts.len() as isize;
            let i = if index < 0 { index + n } else { index };
            if i < 0 || i >= n {
                return Err(PyIndexError::new_err("font index out of range"));
            }
            Ok(PyFont::new(s.fonts.remove(i as usize)))
        })
    }
    /// A view of the only strike; `InvalidInputError` unless there is
    /// exactly one (`FontSet::single`).
    fn single(&self) -> PyResult<PyFont> {
        self.read(|s| s.single().map(|_| ()).py())?;
        Ok(PyFont::view(&self.set, 0))
    }
    /// An owned copy of the only strike (`FontSet::into_single`; Python keeps
    /// the set intact).
    #[pyo3(name = "into_single")]
    fn owned_single(&self) -> PyResult<PyFont> {
        let set = self.snapshot()?;
        Ok(PyFont::new(set.into_single().py()?))
    }
    /// Check glyph counts, pixel and byte budgets for every strike
    /// (`FontSet::validate`); raises `ResourceLimitError`/`InvalidInputError`.
    #[pyo3(signature = (limits = None))]
    fn validate(&self, limits: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        let limits: ResourceLimits = from_py(limits)?;
        self.read(|s| s.validate(&limits).py())
    }
    /// Conservative retained storage of the set (`FontSet::retained_usage`):
    /// `{"bytes", "fonts", "glyphs", "glyph_slots"}`. Raises
    /// `ResourceLimitError` above `max_bytes` (default: unbounded).
    #[pyo3(signature = (max_bytes = None))]
    fn retained_usage<'py>(
        &self,
        py: Python<'py>,
        max_bytes: Option<usize>,
    ) -> PyResult<Bound<'py, PyAny>> {
        self.read(|s| usage(py, max_bytes, |m| s.retained_usage(m)))
    }
    /// An independent copy of the set.
    fn copy(&self) -> PyResult<Self> {
        Ok(Self::new(self.snapshot()?))
    }
    fn __copy__(&self) -> PyResult<Self> {
        self.copy()
    }
    fn __deepcopy__(&self, _memo: &Bound<'_, PyAny>) -> PyResult<Self> {
        self.copy()
    }
    fn __eq__(&self, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        let Ok(other) = other.cast::<PyFontSet>() else {
            return Ok(false);
        };
        let other = other.get().snapshot()?;
        self.read(|s| Ok(*s == other))
    }
    fn __repr__(&self) -> PyResult<String> {
        self.read(|s| {
            Ok(format!(
                "FontSet(fonts={}, source_format={})",
                s.fonts.len(),
                opt_repr(s.source_format.as_deref())
            ))
        })
    }
}

// ---------------------------------------------------------------------------
// Font
// ---------------------------------------------------------------------------

/// Copy a glyph argument: a `Glyph` handle or its plain-data dict.
pub fn glyph_arg(obj: &Bound<'_, PyAny>) -> PyResult<Glyph> {
    if let Ok(g) = obj.cast::<PyGlyph>() {
        return g.get().snapshot();
    }
    from_py_required(obj)
}

fn unknown_glyph() -> PyErr {
    invalid("unknown or removed glyph ID")
}

/// The `GlyphId` with index `id`. `GlyphId` has no public constructor; its
/// serde form is the bare index, which is what plain data uses too.
pub fn glyph_id(id: usize) -> GlyphId {
    serde_json::from_value(serde_json::Value::from(id)).expect("GlyphId deserializes from an index")
}

/// `Some(id)` when `id` names a live glyph of `font`.
fn gid(font: &Font, id: usize) -> Option<GlyphId> {
    let id = glyph_id(id);
    font.glyph(id).map(|_| id)
}

/// Look up the `GlyphId` for a raw index (live slots only).
fn live_gid(font: &Font, id: usize) -> PyResult<GlyphId> {
    gid(font, id).ok_or_else(unknown_glyph)
}

impl PyFont {
    fn glyph_view(&self, id: GlyphId) -> PyGlyph {
        PyGlyph::view(&self.loc, id)
    }
}

#[pymethods]
impl PyFont {
    /// Font(glyphs=None)
    ///
    /// A strike built from copies of `glyphs` (`Font::from_glyphs`); glyph
    /// IDs are assigned in order from 0.
    #[new]
    #[pyo3(signature = (glyphs = None))]
    fn py_new(glyphs: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        let glyphs = match glyphs {
            Some(g) if !g.is_none() => g
                .try_iter()?
                .map(|g| glyph_arg(&g?))
                .collect::<PyResult<Vec<_>>>()?,
            _ => Vec::new(),
        };
        Ok(Self::new(Font::from_glyphs(glyphs).py()?))
    }
    /// Build from the plain-data form produced by `to_dict` (serde shape;
    /// `glyphs` keeps `None` for removed slots so IDs survive).
    #[staticmethod]
    #[pyo3(signature = (data))]
    fn from_dict(data: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self::new(from_py_required(data)?))
    }
    /// The complete plain-data form; copies every pixel.
    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.read(|f| to_py(py, f))
    }
    /// Whether this handle is a view into a `FontSet` (otherwise it owns its font).
    #[getter]
    fn is_view(&self) -> bool {
        matches!(self.loc, FontLoc::InSet(..))
    }
    /// Number of live glyphs (`Font::len`).
    fn __len__(&self) -> PyResult<usize> {
        self.read(|f| Ok(f.len()))
    }
    /// Number of assigned glyph IDs, including removed slots (`Font::slot_count`).
    #[getter]
    fn slot_count(&self) -> PyResult<usize> {
        self.read(|f| Ok(f.slot_count()))
    }
    /// Whether the font has no live glyphs (`Font::is_empty`).
    fn is_empty(&self) -> PyResult<bool> {
        self.read(|f| Ok(f.is_empty()))
    }
    /// Live views of every glyph, in ID order.
    fn __iter__<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyIterator>> {
        let ids = self.read(|f| Ok(f.glyphs().map(|(id, _)| id).collect::<Vec<_>>()))?;
        iter_list(py, ids.into_iter().map(|id| self.glyph_view(id)).collect())
    }
    /// The glyph view for ID `id`; `KeyError` if it does not exist.
    fn __getitem__(&self, id: usize) -> PyResult<PyGlyph> {
        self.glyph(id)?
            .ok_or_else(|| PyKeyError::new_err(format!("no glyph with ID {id}")))
    }
    /// `(id, glyph view)` pairs for every live glyph in ID order (`Font::glyphs`).
    fn glyphs(&self) -> PyResult<Vec<(usize, PyGlyph)>> {
        let ids = self.read(|f| Ok(f.glyphs().map(|(id, _)| id).collect::<Vec<_>>()))?;
        Ok(ids
            .into_iter()
            .map(|id| (id.index(), self.glyph_view(id)))
            .collect())
    }
    /// IDs of every live glyph in order.
    fn glyph_ids(&self) -> PyResult<Vec<usize>> {
        self.read(|f| Ok(f.glyphs().map(|(id, _)| id.index()).collect()))
    }
    /// A live view of glyph `id`, or `None` (`Font::glyph`).
    #[pyo3(signature = (id))]
    fn glyph(&self, id: usize) -> PyResult<Option<PyGlyph>> {
        let id = self.read(|f| Ok(gid(f, id)))?;
        Ok(id.map(|id| self.glyph_view(id)))
    }
    /// The effective glyph ID for a codepoint, or `None` (`Font::glyph_id`).
    #[pyo3(signature = (codepoint))]
    fn glyph_id(&self, codepoint: &Bound<'_, PyAny>) -> PyResult<Option<usize>> {
        let cp = codepoint_arg(codepoint)?;
        self.read(|f| Ok(f.glyph_id(cp).map(GlyphId::index)))
    }
    /// The effective glyph for a codepoint, as a view, or `None`
    /// (`Font::glyph_for_codepoint`).
    #[pyo3(signature = (codepoint))]
    fn glyph_for_codepoint(&self, codepoint: &Bound<'_, PyAny>) -> PyResult<Option<PyGlyph>> {
        let cp = codepoint_arg(codepoint)?;
        let id = self.read(|f| Ok(f.glyph_for_codepoint(cp).and_then(|_| f.glyph_id(cp))))?;
        Ok(id.map(|id| self.glyph_view(id)))
    }
    /// Every glyph ID mapped to a codepoint, ascending; the last is effective
    /// (`Font::glyph_ids_for_codepoint`).
    #[pyo3(signature = (codepoint))]
    fn glyph_ids_for_codepoint(&self, codepoint: &Bound<'_, PyAny>) -> PyResult<Vec<usize>> {
        let cp = codepoint_arg(codepoint)?;
        self.read(|f| {
            Ok(f.glyph_ids_for_codepoint(cp)
                .iter()
                .map(|id| id.index())
                .collect())
        })
    }
    /// The effective glyph for a Unicode sequence (list of ints or a `str`),
    /// as a view, or `None` (`Font::glyph_for_sequence`).
    #[pyo3(signature = (sequence))]
    fn glyph_for_sequence(&self, sequence: &Bound<'_, PyAny>) -> PyResult<Option<PyGlyph>> {
        let seq: Vec<u32> = if let Ok(s) = sequence.cast::<PyString>() {
            s.to_str()?.chars().map(u32::from).collect()
        } else {
            sequence.extract()?
        };
        let id = self.read(|f| {
            let Some(found) = f.glyph_for_sequence(&seq) else {
                return Ok(None);
            };
            Ok(f.glyphs()
                .find(|(_, g)| std::ptr::eq(*g, found))
                .map(|(id, _)| id))
        })?;
        Ok(id.map(|id| self.glyph_view(id)))
    }
    /// `(Codepoint, id)` pairs of every effective mapping, in codepoint order
    /// (`Font::codepoints`).
    fn codepoints(&self) -> PyResult<Vec<(PyCodepoint, usize)>> {
        self.read(|f| {
            Ok(f.codepoints()
                .map(|(cp, id)| (PyCodepoint(cp), id.index()))
                .collect())
        })
    }
    /// Effective mappings with `start <= codepoint <= end` (inclusive, in the
    /// `Codepoint` order where every Unicode code precedes every legacy code;
    /// empty when `start > end`) (`Font::codepoint_range`).
    #[pyo3(signature = (start, end))]
    fn codepoint_range(
        &self,
        start: &Bound<'_, PyAny>,
        end: &Bound<'_, PyAny>,
    ) -> PyResult<Vec<(PyCodepoint, usize)>> {
        let (start, end) = (codepoint_arg(start)?, codepoint_arg(end)?);
        self.read(|f| {
            Ok(f.codepoint_range(start, end)
                .map(|(cp, id)| (PyCodepoint(cp), id.index()))
                .collect())
        })
    }
    /// Add a copy of `glyph` (a `Glyph` or glyph dict) after validating it;
    /// returns its new ID (`Font::add_glyph`).
    #[pyo3(signature = (glyph))]
    fn add_glyph(&self, glyph: &Bound<'_, PyAny>) -> PyResult<usize> {
        let glyph = glyph_arg(glyph)?;
        self.write(|f| f.add_glyph(glyph).py().map(GlyphId::index))
    }
    /// Replace glyph `id` with a copy of `glyph`, re-indexing its codes;
    /// returns the old glyph as an owned handle (`Font::replace_glyph`).
    #[pyo3(signature = (id, glyph))]
    fn replace_glyph(&self, id: usize, glyph: &Bound<'_, PyAny>) -> PyResult<PyGlyph> {
        let glyph = glyph_arg(glyph)?;
        self.write(|f| {
            let id = live_gid(f, id)?;
            f.replace_glyph(id, glyph).py().map(PyGlyph::new)
        })
    }
    /// Remove glyph `id`; returns it as an owned handle, or `None` if it does
    /// not exist. Its ID is never reused (`Font::remove_glyph`).
    #[pyo3(signature = (id))]
    fn remove_glyph(&self, id: usize) -> PyResult<Option<PyGlyph>> {
        self.write(|f| {
            let Some(id) = gid(f, id) else {
                return Ok(None);
            };
            Ok(f.remove_glyph(id).map(PyGlyph::new))
        })
    }
    /// Edit glyph `id` through `func(glyph)`, which receives an owned copy;
    /// the edited copy replaces the glyph when `func` returns, and nothing
    /// changes if it raises (`Font::edit_glyph`).
    #[pyo3(signature = (id, func))]
    fn edit_glyph(&self, py: Python<'_>, id: usize, func: &Bound<'_, PyAny>) -> PyResult<()> {
        let original = self.read(|f| Ok(f.glyph(live_gid(f, id)?).cloned()))?;
        let original = original.ok_or_else(unknown_glyph)?;
        let handle = Bound::new(py, PyGlyph::new(original))?;
        func.call1((handle.clone(),))?;
        let edited = handle.get().snapshot()?;
        self.write(|f| {
            let id = live_gid(f, id)?;
            f.edit_glyph(id, |g| {
                *g = edited;
                Ok(())
            })
            .py()
        })
    }
    /// Glyph `id` cropped or extended to the font's cell bounds
    /// (`Font::glyph_in_cell`).
    #[pyo3(signature = (id))]
    fn glyph_in_cell(&self, id: usize) -> PyResult<PyBitmap> {
        self.read(|f| f.glyph_in_cell(glyph_id(id)).py().map(PyBitmap::new))
    }
    /// Set `metrics["bounds"]` to the union of all inked glyph boxes
    /// (`Font::update_bounds`).
    fn update_bounds(&self) -> PyResult<()> {
        self.write(|f| f.update_bounds().py())
    }
    /// Validate glyphs, the default glyph and resource budgets
    /// (`Font::validate`).
    #[pyo3(signature = (limits = None))]
    fn validate(&self, limits: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        let limits: ResourceLimits = from_py(limits)?;
        self.read(|f| f.validate(&limits).py())
    }
    /// Conservative retained storage of this strike (`Font::retained_usage`).
    #[pyo3(signature = (max_bytes = None))]
    fn retained_usage<'py>(
        &self,
        py: Python<'py>,
        max_bytes: Option<usize>,
    ) -> PyResult<Bound<'py, PyAny>> {
        self.read(|f| usage(py, max_bytes, |m| f.retained_usage(m)))
    }
    /// Storage a new owned copy would need, without copying
    /// (`Font::cloned_usage`).
    #[pyo3(signature = (max_bytes = None))]
    fn cloned_usage<'py>(
        &self,
        py: Python<'py>,
        max_bytes: Option<usize>,
    ) -> PyResult<Bound<'py, PyAny>> {
        self.read(|f| usage(py, max_bytes, |m| f.cloned_usage(m)))
    }
    /// Font-wide metrics (`FontMetrics`) as a dict copy; assign a dict to replace.
    #[getter]
    fn metrics<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.read(|f| to_py(py, &f.metrics))
    }
    #[setter]
    fn set_metrics(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let metrics: FontMetrics = from_py_required(value)?;
        self.write(|f| {
            if metrics
                .default_glyph
                .is_some_and(|id| f.glyph(id).is_none())
            {
                return Err(invalid(
                    "default glyph refers to a removed or nonexistent glyph",
                ));
            }
            f.metrics = metrics;
            Ok(())
        })
    }
    /// Font names (`FontNames`) as a dict copy; assign a dict to replace.
    #[getter]
    fn names<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.read(|f| to_py(py, &f.names))
    }
    #[setter]
    fn set_names(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let names: FontNames = from_py_required(value)?;
        self.write(|f| {
            f.names = names;
            Ok(())
        })
    }
    /// Live view of `names["localized"]`.
    #[getter]
    fn localized_names(&self) -> PyProperties {
        PyProperties {
            loc: PropsLoc::Localized(self.loc.clone()),
        }
    }
    /// Charset identification (`FontCharset`) as a dict copy; assignable.
    #[getter]
    fn charset<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.read(|f| to_py(py, &f.charset))
    }
    #[setter]
    fn set_charset(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let charset: FontCharset = from_py_required(value)?;
        self.write(|f| {
            f.charset = charset;
            Ok(())
        })
    }
    /// The format ID this strike was read from, if any.
    #[getter]
    fn source_format(&self) -> PyResult<Option<String>> {
        self.read(|f| Ok(f.source_format.clone()))
    }
    #[setter]
    fn set_source_format(&self, value: Option<String>) -> PyResult<()> {
        self.write(|f| {
            f.source_format = value;
            Ok(())
        })
    }
    /// Format header records (live `Properties` view); assignable.
    #[getter]
    fn headers(&self) -> PyProperties {
        PyProperties {
            loc: PropsLoc::Headers(self.loc.clone()),
        }
    }
    #[setter]
    fn set_headers(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let props = properties_arg(value)?;
        self.write(|f| {
            f.headers = props;
            Ok(())
        })
    }
    /// Font properties (live `Properties` view); assignable.
    #[getter]
    fn properties(&self) -> PyProperties {
        PyProperties {
            loc: PropsLoc::Font(self.loc.clone()),
        }
    }
    #[setter]
    fn set_properties(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let props = properties_arg(value)?;
        self.write(|f| {
            f.properties = props;
            Ok(())
        })
    }
    /// The canonical charset name of `charset` (`FontCharset::canonical_name`).
    fn charset_canonical_name(&self) -> PyResult<String> {
        self.read(|f| Ok(f.charset.canonical_name()))
    }
    /// An owned copy of this font.
    fn copy(&self) -> PyResult<Self> {
        Ok(Self::new(self.snapshot()?))
    }
    fn __copy__(&self) -> PyResult<Self> {
        self.copy()
    }
    fn __deepcopy__(&self, _memo: &Bound<'_, PyAny>) -> PyResult<Self> {
        self.copy()
    }
    fn __eq__(&self, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        let Ok(other) = other.cast::<PyFont>() else {
            return Ok(false);
        };
        let other = other.get().snapshot()?;
        self.read(|f| Ok(*f == other))
    }
    fn __repr__(&self) -> PyResult<String> {
        self.read(|f| {
            Ok(format!(
                "Font(glyphs={}, family={}, view={})",
                f.len(),
                opt_repr(f.names.family.as_deref()),
                if matches!(self.loc, FontLoc::InSet(..)) {
                    "True"
                } else {
                    "False"
                }
            ))
        })
    }
}

// ---------------------------------------------------------------------------
// Glyph
// ---------------------------------------------------------------------------

fn bounds_arg(obj: &Bound<'_, PyAny>) -> PyResult<Bounds> {
    from_py_required(obj)
}

#[pymethods]
impl PyGlyph {
    /// Glyph(bitmap=None, codepoints=None, name=None)
    ///
    /// A glyph whose bounds match `bitmap` at offset (0, 0) (`Glyph::new`).
    #[new]
    #[pyo3(signature = (bitmap = None, codepoints = None, name = None))]
    fn py_new(
        bitmap: Option<&Bound<'_, PyAny>>,
        codepoints: Option<&Bound<'_, PyAny>>,
        name: Option<String>,
    ) -> PyResult<Self> {
        let bitmap = match bitmap {
            Some(b) if !b.is_none() => bitmap_arg(b)?,
            _ => GlyphBitmap::default(),
        };
        let mut glyph = Glyph::new(bitmap);
        if let Some(c) = codepoints.filter(|c| !c.is_none()) {
            glyph.codepoints = codepoints_arg(c)?;
        }
        glyph.name = name;
        Ok(Self::new(glyph))
    }
    /// Build from the plain-data form produced by `to_dict` (serde shape).
    #[staticmethod]
    #[pyo3(signature = (data))]
    fn from_dict(data: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self::new(from_py_required(data)?))
    }
    /// The complete plain-data form; copies the pixels.
    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.read(|g| to_py(py, g))
    }
    /// Whether this handle is a view into a font.
    #[getter]
    fn is_view(&self) -> bool {
        matches!(self.loc, GlyphLoc::InFont(..))
    }
    /// The glyph ID of a view, or `None` for an owned glyph.
    #[getter]
    fn id(&self) -> Option<usize> {
        match &self.loc {
            GlyphLoc::InFont(_, id) => Some(id.index()),
            GlyphLoc::Owned(_) => None,
        }
    }
    /// Encoded codepoints (list of `Codepoint`); assignable from any
    /// accepted codepoint values. Edits of a view re-index the font.
    #[getter]
    fn codepoints(&self) -> PyResult<Vec<PyCodepoint>> {
        self.read(|g| Ok(g.codepoints.iter().copied().map(PyCodepoint).collect()))
    }
    #[setter]
    fn set_codepoints(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let cps = codepoints_arg(value)?;
        self.write(|g| {
            g.codepoints = cps;
            Ok(())
        })
    }
    /// Unicode sequences (ligatures/clusters) mapped to this glyph.
    #[getter]
    fn unicode_sequences(&self) -> PyResult<Vec<Vec<u32>>> {
        self.read(|g| Ok(g.unicode_sequences.clone()))
    }
    #[setter]
    fn set_unicode_sequences(&self, value: Vec<Vec<u32>>) -> PyResult<()> {
        self.write(|g| {
            g.unicode_sequences = value;
            Ok(())
        })
    }
    /// The glyph name, if any.
    #[getter]
    fn name(&self) -> PyResult<Option<String>> {
        self.read(|g| Ok(g.name.clone()))
    }
    #[setter]
    fn set_name(&self, value: Option<String>) -> PyResult<()> {
        self.write(|g| {
            g.name = value;
            Ok(())
        })
    }
    /// Glyph metrics (`GlyphMetrics`) as a dict copy; assign to replace
    /// (validated: bounds must match the bitmap).
    #[getter]
    fn metrics<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.read(|g| to_py(py, &g.metrics))
    }
    #[setter]
    fn set_metrics(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let metrics: GlyphMetrics = from_py_required(value)?;
        self.write(|g| {
            g.metrics = metrics;
            g.validate().py()
        })
    }
    /// A copy of the bitmap. Assigning a `Bitmap` replaces the pixels and
    /// resizes `metrics["bounds"]`, keeping its offset.
    #[getter]
    fn bitmap(&self) -> PyResult<PyBitmap> {
        self.read(|g| Ok(PyBitmap::new(g.bitmap.clone())))
    }
    #[setter(bitmap)]
    fn set_bitmap_attr(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let bitmap = bitmap_arg(value)?;
        self.write(|g| {
            let (x, y) = (g.metrics.bounds.x, g.metrics.bounds.y);
            g.set_bitmap(bitmap, x, y, false).py()
        })
    }
    /// Glyph properties (live `Properties` view); assignable.
    #[getter]
    fn properties(&self) -> PyProperties {
        PyProperties {
            loc: PropsLoc::Glyph(self.loc.clone()),
        }
    }
    #[setter]
    fn set_properties(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let props = properties_arg(value)?;
        self.write(|g| {
            g.properties = props;
            Ok(())
        })
    }
    /// Replace the bitmap and its bounds, placing its bottom-left corner at
    /// `(x, y)`; with `trim`, crop to the ink first (`Glyph::set_bitmap`).
    #[pyo3(signature = (bitmap, x = 0, y = 0, trim = false))]
    fn set_bitmap(&self, bitmap: &Bound<'_, PyAny>, x: i64, y: i64, trim: bool) -> PyResult<()> {
        let bitmap = bitmap_arg(bitmap)?;
        self.write(|g| g.set_bitmap(bitmap, x, y, trim).py())
    }
    /// Move the glyph box by `(dx, dy)` with overflow checks (`Glyph::shift`).
    #[pyo3(signature = (dx, dy))]
    fn shift(&self, dx: i64, dy: i64) -> PyResult<()> {
        self.write(|g| g.shift(dx, dy).py())
    }
    /// Check bounds/bitmap agreement and Unicode sequences (`Glyph::validate`).
    fn validate(&self) -> PyResult<()> {
        self.read(|g| g.validate().py())
    }
    /// The glyph cropped or extended to `cell` (a bounds dict), in cell
    /// coordinates (`Glyph::bitmap_in_cell`).
    #[pyo3(signature = (cell))]
    fn bitmap_in_cell(&self, cell: &Bound<'_, PyAny>) -> PyResult<PyBitmap> {
        let cell = bounds_arg(cell)?;
        self.read(|g| g.bitmap_in_cell(cell).py().map(PyBitmap::new))
    }
    /// An owned copy.
    fn copy(&self) -> PyResult<Self> {
        Ok(Self::new(self.snapshot()?))
    }
    fn __copy__(&self) -> PyResult<Self> {
        self.copy()
    }
    fn __deepcopy__(&self, _memo: &Bound<'_, PyAny>) -> PyResult<Self> {
        self.copy()
    }
    fn __eq__(&self, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        let Ok(other) = other.cast::<PyGlyph>() else {
            return Ok(false);
        };
        let other = other.get().snapshot()?;
        self.read(|g| Ok(*g == other))
    }
    fn __repr__(&self) -> PyResult<String> {
        let id = self.id();
        self.read(|g| {
            let cps: Vec<String> = g
                .codepoints
                .iter()
                .map(|c| PyCodepoint(*c).__repr__())
                .collect();
            Ok(format!(
                "Glyph(id={}, codepoints=[{}], name={}, size={}x{})",
                id.map_or_else(|| "None".to_owned(), |i| i.to_string()),
                cps.join(", "),
                opt_repr(g.name.as_deref()),
                g.bitmap.width(),
                g.bitmap.height()
            ))
        })
    }
}

/// Register the model classes and functions.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyCodepoint>()?;
    m.add_class::<PyProperties>()?;
    Ok(())
}
