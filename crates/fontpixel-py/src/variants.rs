//! `fontpixel.variants`: regional/script bitmap alternatives, deterministic
//! deduplication and locl-enabled OpenType output (`fontpixel::variants`).
//!
//! Fonts are passed as `fontpixel.Font` handles (copied into the model) or as
//! serde font dicts. `Variants` is a handle that owns its faces; the consuming
//! Rust methods `encode_owned` and `into_prepared` consume the handle's model
//! (later use raises `InvalidInputError`), so faces are dropped during planning
//! exactly as in Rust. `prepare` (which borrows in Rust) plans a copy and owns
//! the result, so the returned `PreparedVariants` never dangles. Exports return
//! `{bytes, report}`; options are `variants.Options` dicts (see `defaults()`).
//! The GIL is released while planning and encoding.
use crate::errors::{IntoPy, InvalidInputError};
use crate::handles::PyFont;
use crate::sfnt_support::{dict, opt, out, pybytes, req, submodule, tag_arg, tag_str};
use fontpixel::Font;
use fontpixel::outline::Shape;
use fontpixel::sfnt::gsub::LayoutLimits;
use fontpixel::variants::{
    self, Encoded, LanguageVariant, MetadataOptions, NotdefPolicy, Options, OutlineFlavor,
    PreparedVariants, ScriptVariant, Variants,
};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use std::sync::{Arc, Mutex};

/// A model font from a `Font` handle (copied) or a serde font dict.
pub fn font_arg(obj: &Bound<'_, PyAny>) -> PyResult<Font> {
    if let Ok(handle) = obj.cast::<PyFont>() {
        return handle.get().snapshot();
    }
    req::<Font>(obj)
}

fn encoded<'py>(py: Python<'py>, e: &Encoded) -> PyResult<Bound<'py, PyAny>> {
    dict(
        py,
        vec![
            ("bytes", pybytes(py, &e.bytes)),
            ("report", out(py, &e.report)?),
        ],
    )
}

fn consumed() -> PyErr {
    InvalidInputError::new_err("this Variants was consumed by encode_owned() or into_prepared()")
}

fn tags(obj: &Bound<'_, PyAny>) -> PyResult<Vec<[u8; 4]>> {
    obj.try_iter()?
        .map(|t| tag_arg(&t?))
        .collect::<PyResult<Vec<_>>>()
}

fn tag_list(t: &[[u8; 4]]) -> Vec<String> {
    t.iter().map(|&t| tag_str(t)).collect()
}

/// One default face plus ordered language and script alternatives
/// (`fontpixel::variants::Variants`).
///
/// Holds the model behind a lock; calls copy a reference to the current
/// model and release the lock (and the GIL) while working.
#[pyclass(name = "Variants", module = "fontpixel.variants", frozen)]
pub struct PyVariants {
    inner: Mutex<Option<Arc<Variants>>>,
}

impl PyVariants {
    fn wrap(v: Variants) -> Self {
        Self {
            inner: Mutex::new(Some(Arc::new(v))),
        }
    }
    fn lock(&self) -> PyResult<std::sync::MutexGuard<'_, Option<Arc<Variants>>>> {
        self.inner
            .lock()
            .map_err(|_| InvalidInputError::new_err("Variants state was poisoned"))
    }
    /// The current model (shared, read-only).
    pub fn current(&self) -> PyResult<Arc<Variants>> {
        self.lock()?.clone().ok_or_else(consumed)
    }
    fn edit<R>(&self, f: impl FnOnce(&mut Variants) -> PyResult<R>) -> PyResult<R> {
        let mut guard = self.lock()?;
        let arc = guard.as_mut().ok_or_else(consumed)?;
        f(Arc::make_mut(arc))
    }
    /// Take the model out of the handle (cloning only if a call still shares it).
    fn take(&self) -> PyResult<Variants> {
        let arc = self.lock()?.take().ok_or_else(consumed)?;
        Ok(Arc::try_unwrap(arc).unwrap_or_else(|shared| (*shared).clone()))
    }
}

#[pymethods]
impl PyVariants {
    /// `Variants::new`: start with a default face (`Font` handle or font dict).
    #[new]
    fn py_new(base: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self::wrap(Variants::new(font_arg(base)?)))
    }
    /// Build from a serde Variants dict `{base, languages, scripts}`.
    #[staticmethod]
    fn from_dict(value: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self::wrap(req(value)?))
    }
    /// The serde Variants dict (fonts as font dicts).
    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        out(py, &*self.current()?)
    }
    /// Whether `encode_owned()`/`into_prepared()` consumed this handle.
    #[getter]
    fn consumed(&self) -> PyResult<bool> {
        Ok(self.lock()?.is_none())
    }
    /// A copy of the default face as a `Font` handle.
    #[getter]
    fn base(&self) -> PyResult<PyFont> {
        Ok(PyFont::new(self.current()?.base.clone()))
    }
    /// Replace the default face.
    #[setter]
    fn set_base(&self, font: &Bound<'_, PyAny>) -> PyResult<()> {
        let f = font_arg(font)?;
        self.edit(|v| {
            v.base = f;
            Ok(())
        })
    }
    /// Language alternatives as `[{language, scripts, font}]` (OpenType tag
    /// strings, `Font` handle copies), in application order.
    #[getter]
    fn languages<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyList>> {
        let v = self.current()?;
        let list = PyList::empty(py);
        for l in &v.languages {
            let d = PyDict::new(py);
            d.set_item("language", tag_str(l.language))?;
            d.set_item("scripts", tag_list(&l.scripts))?;
            d.set_item("font", PyFont::new(l.font.clone()))?;
            list.append(d)?;
        }
        Ok(list)
    }
    /// Replace the language alternatives with `[{language, scripts, font}]`
    /// (tags as 4-character strings; fonts as handles or dicts).
    #[setter]
    fn set_languages(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let mut items = Vec::new();
        for item in value.try_iter()? {
            let item = item?;
            items.push(LanguageVariant {
                language: tag_arg(&item.get_item("language")?)?,
                scripts: tags(&item.get_item("scripts")?)?,
                font: font_arg(&item.get_item("font")?)?,
            });
        }
        self.edit(|v| {
            v.languages = items;
            Ok(())
        })
    }
    /// Script alternatives as `[{scripts, font}]`.
    #[getter]
    fn scripts<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyList>> {
        let v = self.current()?;
        let list = PyList::empty(py);
        for s in &v.scripts {
            let d = PyDict::new(py);
            d.set_item("scripts", tag_list(&s.scripts))?;
            d.set_item("font", PyFont::new(s.font.clone()))?;
            list.append(d)?;
        }
        Ok(list)
    }
    /// Replace the script alternatives with `[{scripts, font}]`.
    #[setter]
    fn set_scripts(&self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let mut items = Vec::new();
        for item in value.try_iter()? {
            let item = item?;
            items.push(ScriptVariant {
                scripts: tags(&item.get_item("scripts")?)?,
                font: font_arg(&item.get_item("font")?)?,
            });
        }
        self.edit(|v| {
            v.scripts = items;
            Ok(())
        })
    }
    /// `Variants::add_language`: add a BCP-47/OpenType language with the
    /// default CJK script scope (DFLT, hani, kana, hang, bopo).
    fn add_language(&self, language: &str, font: &Bound<'_, PyAny>) -> PyResult<()> {
        let f = font_arg(font)?;
        self.edit(|v| v.add_language(language, f).py())
    }
    /// Append a language alternative with explicit OpenType `language` tag and
    /// `scripts` tags (the `languages` field).
    fn add_language_variant(
        &self,
        language: &Bound<'_, PyAny>,
        scripts: &Bound<'_, PyAny>,
        font: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let item = LanguageVariant {
            language: tag_arg(language)?,
            scripts: tags(scripts)?,
            font: font_arg(font)?,
        };
        self.edit(|v| {
            v.languages.push(item);
            Ok(())
        })
    }
    /// Append a script alternative for `scripts` tags (the `scripts` field).
    fn add_script(&self, scripts: &Bound<'_, PyAny>, font: &Bound<'_, PyAny>) -> PyResult<()> {
        let item = ScriptVariant {
            scripts: tags(scripts)?,
            font: font_arg(font)?,
        };
        self.edit(|v| {
            v.scripts.push(item);
            Ok(())
        })
    }
    /// `Variants::plan`: glyph order, deduplication and locl routing as a Plan
    /// dict `{glyphs: [(name, glyph)], cmap, gsub}`.
    #[pyo3(signature = (limits = None))]
    fn plan<'py>(
        &self,
        py: Python<'py>,
        limits: Option<&Bound<'py, PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let l: LayoutLimits = opt(limits)?;
        let v = self.current()?;
        out(py, &py.detach(|| v.plan(&l)).py()?)
    }
    /// `Variants::prepare`: validate and plan once for repeated exports.
    ///
    /// Plans an owned copy of the model (Rust borrows it instead), so the
    /// returned handle is independent of later edits to this one.
    #[pyo3(signature = (limits = None))]
    fn prepare(
        &self,
        py: Python<'_>,
        limits: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<PyPreparedVariants> {
        let l: LayoutLimits = opt(limits)?;
        let v = self.current()?;
        let p = py.detach(|| (*v).clone().into_prepared(&l)).py()?;
        Ok(PyPreparedVariants { prepared: p })
    }
    /// `Variants::into_prepared`: as `prepare`, consuming this handle's model.
    #[pyo3(name = "into_prepared", signature = (limits = None))]
    fn consume_prepared(
        &self,
        py: Python<'_>,
        limits: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<PyPreparedVariants> {
        let l: LayoutLimits = opt(limits)?;
        let v = self.take()?;
        let p = py.detach(|| v.into_prepared(&l)).py()?;
        Ok(PyPreparedVariants { prepared: p })
    }
    /// `Variants::retained_usage`: retained bytes as an owning caller charges
    /// them; raises `ResourceLimitError` above `max_bytes`.
    fn retained_usage(&self, max_bytes: usize) -> PyResult<usize> {
        self.current()?.retained_usage(max_bytes).py()
    }
    /// `variants::encode`: plan and export (`{bytes, report}`).
    #[pyo3(signature = (options = None))]
    fn encode<'py>(
        &self,
        py: Python<'py>,
        options: Option<&Bound<'py, PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let o: Options = opt(options)?;
        let v = self.current()?;
        encoded(py, &py.detach(|| variants::encode(&v, &o)).py()?)
    }
    /// `Variants::encode_owned`: plan and export, consuming this handle's
    /// model and dropping faces as soon as they are planned.
    #[pyo3(signature = (options = None))]
    fn encode_owned<'py>(
        &self,
        py: Python<'py>,
        options: Option<&Bound<'py, PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let o: Options = opt(options)?;
        let v = self.take()?;
        encoded(py, &py.detach(|| v.encode_owned(&o)).py()?)
    }
}

/// An immutable plan ready for repeated exports
/// (`fontpixel::variants::PreparedVariants`). Safe to share between threads.
#[pyclass(name = "PreparedVariants", module = "fontpixel.variants", frozen)]
pub struct PyPreparedVariants {
    prepared: PreparedVariants<'static>,
}

#[pymethods]
impl PyPreparedVariants {
    /// `PreparedVariants::plan`: the Plan dict.
    fn plan<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        out(py, self.prepared.plan())
    }
    /// `PreparedVariants::retained_bytes`: admitted bytes of the shared plan.
    fn retained_bytes(&self) -> usize {
        self.prepared.retained_bytes()
    }
    /// `PreparedVariants::encode_allowance`: bytes one `encode` may admit
    /// besides the plan.
    fn encode_allowance(&self) -> usize {
        self.prepared.encode_allowance()
    }
    /// `PreparedVariants::encode`: export without replanning; `options.limits`
    /// must equal the limits given to `prepare`.
    #[pyo3(signature = (options = None))]
    fn encode<'py>(
        &self,
        py: Python<'py>,
        options: Option<&Bound<'py, PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let o: Options = opt(options)?;
        let p = &self.prepared;
        encoded(py, &py.detach(|| p.encode(&o)).py()?)
    }
}

/// `variants::encode`: plan and export a `Variants` handle.
#[pyfunction]
#[pyo3(name = "encode", signature = (variants, options = None))]
fn variants_encode<'py>(
    py: Python<'py>,
    variants: &PyVariants,
    options: Option<&Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyAny>> {
    variants.encode(py, options)
}

/// `variants::encode_single`: export one face without alternatives. Unlike
/// regional planning (`Variants`), this keeps physical glyph identity and
/// order, so its bytes differ from `Variants.encode_owned` by design.
#[pyfunction]
#[pyo3(signature = (font, options = None))]
fn encode_single<'py>(
    py: Python<'py>,
    font: &Bound<'py, PyAny>,
    options: Option<&Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyAny>> {
    let f = font_arg(font)?;
    let o: Options = opt(options)?;
    encoded(py, &py.detach(|| variants::encode_single(&f, &o)).py()?)
}

/// `variants::encode_single_owned`: as `encode_single`, letting the export
/// own (and drop early) a copy of the face; output is identical.
#[pyfunction]
#[pyo3(signature = (font, options = None))]
fn encode_single_owned<'py>(
    py: Python<'py>,
    font: &Bound<'py, PyAny>,
    options: Option<&Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyAny>> {
    let f = font_arg(font)?;
    let o: Options = opt(options)?;
    encoded(
        py,
        &py.detach(|| variants::encode_single_owned(f, &o)).py()?,
    )
}

/// `variants::script_tag`: validate an explicit four-letter script tag
/// (e.g. `"latn"`); other lengths or characters raise `InvalidInputError`.
#[pyfunction]
fn script_tag(value: &str) -> PyResult<String> {
    Ok(tag_str(variants::script_tag(value).py()?))
}

/// `variants::language_tag`: the OpenType language-system tag of a BCP-47 or
/// OpenType tag via the OpenType language registry (e.g. `"zh-HK"` → `"ZHH "`);
/// unknown tags raise `InvalidInputError`.
#[pyfunction]
fn language_tag(value: &str) -> PyResult<String> {
    Ok(tag_str(variants::language_tag(value).py()?))
}

/// `MetadataOptions::validate`: check metadata controls against an outline
/// `flavor` (`"true_type"`/`"cff"`) and `shape`.
#[pyfunction]
fn validate_metadata(
    metadata: &Bound<'_, PyAny>,
    flavor: &Bound<'_, PyAny>,
    shape: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let m: MetadataOptions = req(metadata)?;
    let f: OutlineFlavor = req(flavor)?;
    let s: Shape = req(shape)?;
    m.validate(f, s).py()
}

/// `NotdefPolicy::empty_metrics`: `(advance, left_side_bearing)` of an empty
/// `.notdef` policy, or `None` for `"preserve"`.
#[pyfunction]
fn notdef_empty_metrics(policy: &Bound<'_, PyAny>) -> PyResult<Option<(u16, i16)>> {
    Ok(req::<NotdefPolicy>(policy)?.empty_metrics())
}

/// Default option dicts of `fontpixel.variants`.
#[pyfunction]
#[pyo3(name = "defaults")]
fn variants_defaults<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
    dict(
        py,
        vec![
            ("Options", out(py, &Options::default())?),
            ("MetadataOptions", out(py, &MetadataOptions::default())?),
            ("NotdefPolicy", out(py, &NotdefPolicy::default())?),
            ("LayoutLimits", out(py, &LayoutLimits::default())?),
        ],
    )
}

/// Register `fontpixel.variants`.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let v = submodule(
        m,
        "variants",
        "fontpixel.variants",
        "Regional bitmap alternatives, deduplication and locl-enabled OpenType output.",
    )?;
    v.add_class::<PyVariants>()?;
    v.add_class::<PyPreparedVariants>()?;
    v.add_function(wrap_pyfunction!(variants_encode, &v)?)?;
    v.add_function(wrap_pyfunction!(encode_single, &v)?)?;
    v.add_function(wrap_pyfunction!(encode_single_owned, &v)?)?;
    v.add_function(wrap_pyfunction!(script_tag, &v)?)?;
    v.add_function(wrap_pyfunction!(language_tag, &v)?)?;
    v.add_function(wrap_pyfunction!(validate_metadata, &v)?)?;
    v.add_function(wrap_pyfunction!(notdef_empty_metrics, &v)?)?;
    v.add_function(wrap_pyfunction!(variants_defaults, &v)?)?;
    Ok(())
}
