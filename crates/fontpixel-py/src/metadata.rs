//! Name overrides (`fontpixel::metadata`), bitmap projection
//! (`fontpixel::projection`) and report helpers (`fontpixel::report`).
//!
//! Reports are plain dicts `{"entries": [{"level", "impact", "code",
//! "location": {"font_index", "glyph_index", "codepoint", "asset"},
//! "message"}]}` with every entry; nothing is truncated.
use crate::conv::{from_py, from_py_required, to_py};
use crate::errors::IntoPy;
use crate::handles::PyFontSet;
use crate::model::invalid;
use fontpixel::ResourceLimits;
use fontpixel::metadata::{NameOverrides, apply_names as core_apply_names};
use fontpixel::projection::{
    BitmapProjection, EditValue, MAX_FONT_NAME_BYTES, MAX_PROPERTY_EDITS, MAX_PROPERTY_NAME_BYTES,
    MAX_PROPERTY_TEXT_BYTES, PropertyEdit, ScalableAdvance, SizeProjection,
    apply_bitmap_projection as core_apply_projection,
};
use fontpixel::report::{Report, ReportEntry};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyInt, PyString};

/// Apply family/style/name/notice overrides to every strike as one
/// transaction (`metadata::apply_names`). `overrides` is a `NameOverrides`
/// dict (`family`, `style`, `full_name`, `postscript_name`, `version`,
/// `copyright`, `license`); an empty dict changes nothing. Returns the report.
#[pyfunction]
#[pyo3(signature = (font_set, overrides = None, limits = None))]
fn apply_names<'py>(
    py: Python<'py>,
    font_set: &Bound<'_, PyFontSet>,
    overrides: Option<&Bound<'_, PyAny>>,
    limits: Option<&Bound<'_, PyAny>>,
) -> PyResult<Bound<'py, PyAny>> {
    let overrides: NameOverrides = from_py(overrides)?;
    let limits: ResourceLimits = from_py(limits)?;
    let report = font_set
        .get()
        .write(|set| core_apply_names(set, &overrides, &limits).py())?;
    to_py(py, &report)
}

/// Whether a `NameOverrides` dict requests no change (`NameOverrides::is_empty`).
#[pyfunction]
#[pyo3(signature = (overrides))]
fn name_overrides_is_empty(overrides: &Bound<'_, PyAny>) -> PyResult<bool> {
    let overrides: NameOverrides = from_py_required(overrides)?;
    Ok(overrides.is_empty())
}

// projection ------------------------------------------------------------------

#[derive(Clone, Debug)]
enum OwnedValue {
    Integer(i64),
    Text(String),
}

#[derive(Clone, Debug)]
enum OwnedEdit {
    Set(String, OwnedValue),
    Remove(String),
}

impl OwnedEdit {
    fn name(&self) -> &str {
        match self {
            Self::Set(name, _) | Self::Remove(name) => name,
        }
    }
    fn borrow(&self) -> PropertyEdit<'_> {
        match self {
            Self::Set(name, value) => PropertyEdit::Set {
                name,
                value: match value {
                    OwnedValue::Integer(v) => EditValue::Integer(*v),
                    OwnedValue::Text(t) => EditValue::Text(t),
                },
            },
            Self::Remove(name) => PropertyEdit::Remove { name },
        }
    }
    fn to_py<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let inner = PyDict::new(py);
        let outer = PyDict::new(py);
        match self {
            Self::Set(name, value) => {
                inner.set_item("name", name)?;
                match value {
                    OwnedValue::Integer(v) => inner.set_item("value", v)?,
                    OwnedValue::Text(t) => inner.set_item("value", t)?,
                }
                outer.set_item("set", inner)?;
            }
            Self::Remove(name) => {
                inner.set_item("name", name)?;
                outer.set_item("remove", inner)?;
            }
        }
        Ok(outer.into_any())
    }
}

fn edit_arg(obj: &Bound<'_, PyAny>) -> PyResult<OwnedEdit> {
    let bad = || {
        invalid(
            "a property edit is {'set': {'name': str, 'value': int | str}} or {'remove': {'name': str}}",
        )
    };
    let dict = obj.cast::<PyDict>().map_err(|_| bad())?;
    if dict.len() != 1 {
        return Err(bad());
    }
    let (key, body) = dict.iter().next().ok_or_else(bad)?;
    let body = body.cast_into::<PyDict>().map_err(|_| bad())?;
    let name: String = body.get_item("name")?.ok_or_else(bad)?.extract()?;
    match key.extract::<String>()?.as_str() {
        "set" => {
            if body.len() != 2 {
                return Err(bad());
            }
            let value = body.get_item("value")?.ok_or_else(bad)?;
            let value = if value.is_instance_of::<PyInt>() {
                OwnedValue::Integer(value.extract()?)
            } else if let Ok(s) = value.cast::<PyString>() {
                OwnedValue::Text(s.to_str()?.to_owned())
            } else {
                return Err(bad());
            };
            Ok(OwnedEdit::Set(name, value))
        }
        "remove" if body.len() == 1 => Ok(OwnedEdit::Remove(name)),
        _ => Err(bad()),
    }
}

/// Typed bitmap-projection options (`fontpixel::projection::BitmapProjection`
/// with owned `PropertyEdit`s). The default changes nothing.
///
/// - `scalable_advance`: `"preserve"` or
///   `{"from_device": {"units_per_em": int, "reference_ppem": int}}`
/// - `font_name`: complete `FONT`/XLFD name
/// - `size`: `{"point_size": {"numerator", "denominator"}, "resolution": {"x", "y"}}`
/// - `properties`: edits `{"set": {"name", "value": int | str}}` or
///   `{"remove": {"name"}}`, applied in order
#[pyclass(name = "BitmapProjection", module = "fontpixel", frozen)]
pub struct PyBitmapProjection {
    scalable_advance: ScalableAdvance,
    font_name: Option<String>,
    size: Option<SizeProjection>,
    properties: Vec<OwnedEdit>,
}

impl PyBitmapProjection {
    fn with<R>(&self, f: impl FnOnce(&BitmapProjection<'_>) -> R) -> R {
        let edits: Vec<PropertyEdit<'_>> = self.properties.iter().map(OwnedEdit::borrow).collect();
        let mut projection = BitmapProjection::default();
        projection.scalable_advance = self.scalable_advance;
        projection.font_name = self.font_name.as_deref();
        projection.size = self.size;
        projection.properties = &edits;
        f(&projection)
    }
    fn apply_to<'py>(
        &self,
        py: Python<'py>,
        font_set: &PyFontSet,
        max_working_bytes: Option<usize>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let limit = max_working_bytes.unwrap_or(usize::MAX);
        let outcome = font_set.write(|set| {
            self.with(|projection| core_apply_projection(set, projection, limit))
                .py()
        })?;
        let value = to_py(py, &outcome)?;
        value
            .cast::<PyDict>()?
            .set_item("retained_bytes", outcome.retained_bytes())?;
        Ok(value)
    }
}

#[pymethods]
impl PyBitmapProjection {
    /// BitmapProjection(scalable_advance=None, font_name=None, size=None, properties=None)
    #[new]
    #[pyo3(signature = (scalable_advance = None, font_name = None, size = None, properties = None))]
    fn py_new(
        scalable_advance: Option<&Bound<'_, PyAny>>,
        font_name: Option<String>,
        size: Option<&Bound<'_, PyAny>>,
        properties: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let properties = match properties {
            Some(p) if !p.is_none() => p
                .try_iter()?
                .map(|e| edit_arg(&e?))
                .collect::<PyResult<Vec<_>>>()?,
            _ => Vec::new(),
        };
        Ok(Self {
            scalable_advance: from_py(scalable_advance)?,
            font_name,
            size: size
                .filter(|s| !s.is_none())
                .map(from_py_required)
                .transpose()?,
            properties,
        })
    }
    /// Options deriving scalable advances from device advances
    /// (`BitmapProjection::scalable_from_device`).
    #[staticmethod]
    #[pyo3(signature = (units_per_em, reference_ppem))]
    fn scalable_from_device(units_per_em: u32, reference_ppem: u32) -> Self {
        let p = BitmapProjection::scalable_from_device(units_per_em, reference_ppem);
        Self {
            scalable_advance: p.scalable_advance,
            font_name: None,
            size: None,
            properties: Vec::new(),
        }
    }
    /// Whether these options change nothing (`BitmapProjection::is_identity`).
    fn is_identity(&self) -> bool {
        self.with(|p| p.is_identity())
    }
    /// The scalable-advance policy (serde form).
    #[getter]
    fn scalable_advance<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        to_py(py, &self.scalable_advance)
    }
    /// The projected font name.
    #[getter]
    fn font_name(&self) -> Option<String> {
        self.font_name.clone()
    }
    /// The projected size, as a dict.
    #[getter]
    fn size<'py>(&self, py: Python<'py>) -> PyResult<Option<Bound<'py, PyAny>>> {
        self.size.as_ref().map(|s| to_py(py, s)).transpose()
    }
    /// The property edits, as dicts.
    #[getter]
    fn properties<'py>(&self, py: Python<'py>) -> PyResult<Vec<Bound<'py, PyAny>>> {
        self.properties.iter().map(|e| e.to_py(py)).collect()
    }
    /// The edited property names in order (`PropertyEdit::name`).
    fn property_names(&self) -> Vec<String> {
        self.with(|p| p.properties.iter().map(|e| e.name().to_owned()).collect())
    }
    /// Apply to every strike of `font_set` (`apply_bitmap_projection`).
    #[pyo3(signature = (font_set, max_working_bytes = None))]
    fn apply<'py>(
        &self,
        py: Python<'py>,
        font_set: &Bound<'_, PyFontSet>,
        max_working_bytes: Option<usize>,
    ) -> PyResult<Bound<'py, PyAny>> {
        self.apply_to(py, font_set.get(), max_working_bytes)
    }
    fn __repr__(&self) -> String {
        let names: Vec<_> = self.properties.iter().map(OwnedEdit::name).collect();
        format!(
            "BitmapProjection(scalable_advance={:?}, font_name={:?}, size={:?}, properties={names:?})",
            self.scalable_advance, self.font_name, self.size
        )
    }
}

/// Apply `projection` (a `BitmapProjection` or a dict of its constructor
/// arguments) to every strike as one transaction; on error the set is
/// unchanged (`projection::apply_bitmap_projection`). Returns the
/// `ProjectionOutcome` dict (`report`, `scalable_advance_changes`,
/// `metadata_changes`) plus `retained_bytes`
/// (`ProjectionOutcome::retained_bytes`). `max_working_bytes` defaults to
/// unbounded.
#[pyfunction]
#[pyo3(signature = (font_set, projection, max_working_bytes = None))]
fn apply_bitmap_projection<'py>(
    py: Python<'py>,
    font_set: &Bound<'_, PyFontSet>,
    projection: &Bound<'_, PyAny>,
    max_working_bytes: Option<usize>,
) -> PyResult<Bound<'py, PyAny>> {
    if let Ok(p) = projection.cast::<PyBitmapProjection>() {
        return p.get().apply_to(py, font_set.get(), max_working_bytes);
    }
    let kwargs = projection.cast::<PyDict>()?;
    let get = |k: &str| kwargs.get_item(k);
    for key in kwargs.keys() {
        let key: String = key.extract()?;
        if !["scalable_advance", "font_name", "size", "properties"].contains(&key.as_str()) {
            return Err(invalid(format!("unknown projection field '{key}'")));
        }
    }
    let p = PyBitmapProjection::py_new(
        get("scalable_advance")?.as_ref(),
        get("font_name")?.map(|v| v.extract()).transpose()?,
        get("size")?.as_ref(),
        get("properties")?.as_ref(),
    )?;
    p.apply_to(py, font_set.get(), max_working_bytes)
}

// report ----------------------------------------------------------------------

/// Whether a report dict has any loss entry (`Report::has_loss`).
#[pyfunction]
#[pyo3(signature = (report))]
fn report_has_loss(report: &Bound<'_, PyAny>) -> PyResult<bool> {
    let report: Report = from_py_required(report)?;
    Ok(report.has_loss())
}

/// Whether a report dict has any error entry (`Report::has_errors`).
#[pyfunction]
#[pyo3(signature = (report))]
fn report_has_errors(report: &Bound<'_, PyAny>) -> PyResult<bool> {
    let report: Report = from_py_required(report)?;
    Ok(report.has_errors())
}

/// A new report dict with `entry` appended, after validating both
/// (`Report::push`).
#[pyfunction]
#[pyo3(signature = (report, entry))]
fn report_push<'py>(
    py: Python<'py>,
    report: &Bound<'_, PyAny>,
    entry: &Bound<'_, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let mut report: Report = from_py_required(report)?;
    let entry: ReportEntry = from_py_required(entry)?;
    report.push(entry);
    to_py(py, &report)
}

/// A new report dict with every entry of `other` appended (`Report::extend`).
#[pyfunction]
#[pyo3(signature = (report, other))]
fn report_extend<'py>(
    py: Python<'py>,
    report: &Bound<'_, PyAny>,
    other: &Bound<'_, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let mut report: Report = from_py_required(report)?;
    let other: Report = from_py_required(other)?;
    report.extend(other);
    to_py(py, &report)
}

/// The report in the CLI's text form, one line per entry (`Report: Display`).
#[pyfunction]
#[pyo3(signature = (report))]
fn report_to_text(report: &Bound<'_, PyAny>) -> PyResult<String> {
    let report: Report = from_py_required(report)?;
    Ok(report.to_string())
}

/// Register the metadata, projection and report API.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyBitmapProjection>()?;
    for f in [
        wrap_pyfunction!(apply_names, m)?,
        wrap_pyfunction!(name_overrides_is_empty, m)?,
        wrap_pyfunction!(apply_bitmap_projection, m)?,
        wrap_pyfunction!(report_has_loss, m)?,
        wrap_pyfunction!(report_has_errors, m)?,
        wrap_pyfunction!(report_push, m)?,
        wrap_pyfunction!(report_extend, m)?,
        wrap_pyfunction!(report_to_text, m)?,
    ] {
        m.add_function(f)?;
    }
    m.add("MAX_PROPERTY_EDITS", MAX_PROPERTY_EDITS)?;
    m.add("MAX_PROPERTY_NAME_BYTES", MAX_PROPERTY_NAME_BYTES)?;
    m.add("MAX_PROPERTY_TEXT_BYTES", MAX_PROPERTY_TEXT_BYTES)?;
    m.add("MAX_FONT_NAME_BYTES", MAX_FONT_NAME_BYTES)?;
    Ok(())
}
