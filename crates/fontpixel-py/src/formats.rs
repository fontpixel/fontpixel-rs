//! Format registry, detection, reading and writing (`fontpixel::formats`).
//!
//! Python API (re-exported by the `fontpixel` package):
//!
//! - `Registry`: the format registry handle. `Registry()` is the built-in
//!   registry (`fontpixel::registry()`); `Registry(builtin=False)` is the planned
//!   catalogue without codecs (`Registry::new`). Per-format typed options are
//!   set with `set_read_options(format, dict)` / `set_write_options(format, dict)`
//!   or the per-format `set_<module>_read_options(dict)` methods.
//! - `read`, `write`, `detect`, `registry`, `list_formats` module functions.
//! - `Loaded` (read result); writes return the `Written` / `OutputBundle`
//!   classes of [`crate::resources`].
//! - `fontpixel.formats`: per-module namespaces with the direct codec functions
//!   (see [`crate::formats_direct`]) plus `normalize`/`option_types` for every
//!   serde type of the format layer (see [`crate::formats_options`]).
//!
//! Inputs are bytes-like, an `Input`, or an `OutputBundle`/`Written` (read
//! back with its own name and sidecars). Sidecar resources are anything
//! [`crate::resources::resolver_arg`] accepts: a `ResourceSet`, a
//! `dict[str, bytes]`, a resolver object or a callable. Nothing reads the
//! filesystem implicitly.
pub use crate::bitmap::bytes_arg;
use crate::conv;
use crate::errors::{IntoPy, InvalidInputError};
use crate::formats_custom::py_result;
use crate::handles::{PyFont, PyFontSet};
pub use crate::resources::path_arg;
use crate::resources::{InputParts, PyInput, PyOutputBundle, PyWritten, input_arg, resolver_arg};
use fontpixel::formats::{Loaded, ReadOptions, WriteOptions};
use fontpixel::resources::{
    Input, OutputBundle, ResourceLimits, ResourcePath, ResourceResolver, TransportLimits,
};
use fontpixel::{Error, FontRegistry, FontSet, Report};
use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use std::collections::BTreeMap;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

/// Optional `ResourceLimits` dict (`None` = defaults).
pub fn limits_arg(obj: Option<&Bound<'_, PyAny>>) -> PyResult<ResourceLimits> {
    conv::from_py(obj)
}

/// Optional `TransportLimits` dict (`None` = the writer's own limits).
pub fn transport_arg(obj: Option<&Bound<'_, PyAny>>) -> PyResult<Option<TransportLimits>> {
    match obj {
        None => Ok(None),
        Some(o) if o.is_none() => Ok(None),
        Some(o) => conv::from_py_required(o).map(Some),
    }
}

/// Optional report dict carried into a write (`None` = empty report).
pub fn report_arg(obj: Option<&Bound<'_, PyAny>>) -> PyResult<Report> {
    conv::from_py(obj)
}

/// A font-set argument: a `FontSet` handle (shared, read in place) or a `Font`
/// handle (copied into a one-strike set).
pub enum SetArg<'py> {
    /// A `FontSet` handle.
    Set(Bound<'py, PyFontSet>),
    /// A one-strike set built from a `Font` handle.
    Owned(FontSet),
}

impl<'py> SetArg<'py> {
    /// Extract from a `FontSet` or `Font` handle.
    pub fn extract(obj: &Bound<'py, PyAny>) -> PyResult<Self> {
        if let Ok(set) = obj.cast::<PyFontSet>() {
            return Ok(Self::Set(set.clone()));
        }
        if let Ok(font) = obj.cast::<PyFont>() {
            let font = font.get().snapshot()?;
            return Ok(Self::Owned(FontSet {
                fonts: vec![font],
                ..FontSet::default()
            }));
        }
        Err(PyTypeError::new_err(format!(
            "expected a FontSet or Font, got {}",
            obj.get_type().name()?
        )))
    }

    /// Run `f` with the set.
    pub fn with<R>(&self, f: impl FnOnce(&FontSet) -> PyResult<R>) -> PyResult<R> {
        match self {
            Self::Set(set) => set.get().read(f),
            Self::Owned(set) => f(set),
        }
    }

    /// An owned copy (or the owned one-strike set).
    pub fn into_owned(self) -> PyResult<FontSet> {
        match self {
            Self::Set(set) => set.get().snapshot(),
            Self::Owned(set) => Ok(set),
        }
    }
}

/// A font argument for direct single-strike codecs: a `Font` handle, or a
/// `FontSet` handle holding exactly one strike.
pub fn with_font<R>(
    obj: &Bound<'_, PyAny>,
    f: impl FnOnce(&fontpixel::Font) -> PyResult<R>,
) -> PyResult<R> {
    if let Ok(font) = obj.cast::<PyFont>() {
        return font.get().read(f);
    }
    if let Ok(set) = obj.cast::<PyFontSet>() {
        return set.get().read(|s| f(s.single().py()?));
    }
    Err(PyTypeError::new_err(format!(
        "expected a Font or a one-strike FontSet, got {}",
        obj.get_type().name()?
    )))
}

// ---------------------------------------------------------------------------
// Inputs
// ---------------------------------------------------------------------------

/// An `OutputBundle` (or a `Written`'s bundle) passed as read input.
fn bundle_arg(obj: &Bound<'_, PyAny>) -> PyResult<Option<OutputBundle>> {
    if let Ok(b) = obj.cast::<PyOutputBundle>() {
        return Ok(Some(b.get().bundle.clone()));
    }
    if obj.cast::<PyWritten>().is_ok() {
        let output = obj.getattr("output")?;
        return Ok(Some(output.cast::<PyOutputBundle>()?.get().bundle.clone()));
    }
    Ok(None)
}

/// The input a reader sees for a written bundle: a file bundle's primary
/// bytes with every other asset as a sidecar (paths relative to the entry
/// point's parent), or a directory bundle's entry point with its assets
/// relative to the directory.
pub fn bundle_input(bundle: &OutputBundle) -> InputParts {
    let primary = bundle.primary().as_str();
    let mut files = BTreeMap::new();
    let mut data = Vec::new();
    if bundle.is_directory() {
        let prefix = format!("{primary}/");
        for a in bundle.assets() {
            if let Some(rest) = a.path.as_str().strip_prefix(&prefix) {
                files.insert(rest.to_owned(), a.bytes.clone());
            }
        }
    } else {
        for a in bundle.assets() {
            if a.path.as_str() == primary {
                data = a.bytes.clone();
            } else {
                files.insert(a.path.as_str().to_owned(), a.bytes.clone());
            }
        }
    }
    InputParts {
        data: Arc::new(data),
        name: Some(primary.to_owned()),
        resolver: Arc::new(MapResolver(files)),
        directory: bundle.is_directory(),
    }
}

/// In-memory files of a bundle input.
struct MapResolver(BTreeMap<String, Vec<u8>>);

impl ResourceResolver for MapResolver {
    fn resolve(&self, path: &ResourcePath, max_bytes: usize) -> fontpixel::Result<Vec<u8>> {
        let bytes = self
            .0
            .get(path.as_str())
            .ok_or_else(|| Error::MissingResource(path.as_str().into()))?;
        if bytes.len() > max_bytes {
            return Err(Error::ResourceLimit {
                resource: "resource bytes",
                limit: max_bytes,
            });
        }
        Ok(bytes.to_vec())
    }

    fn list(
        &self,
        directory: Option<&ResourcePath>,
        max_entries: usize,
        _max_bytes: usize,
    ) -> fontpixel::Result<Vec<ResourcePath>> {
        let prefix = directory.map(|d| format!("{}/", d.as_str()));
        let mut out = Vec::new();
        for key in self.0.keys() {
            let rest = match &prefix {
                Some(p) => match key.strip_prefix(p.as_str()) {
                    Some(rest) => rest,
                    None => continue,
                },
                None => key.as_str(),
            };
            if rest.is_empty() || rest.contains('/') {
                continue;
            }
            if out.len() >= max_entries {
                return Err(Error::ResourceLimit {
                    resource: "directory entries",
                    limit: max_entries,
                });
            }
            out.push(ResourcePath::new(key.clone())?);
        }
        Ok(out)
    }
}

/// Owned input built from Python arguments.
pub struct InputArgs {
    parts: InputParts,
}

impl InputArgs {
    /// Collect the `data`, `name`, `resources` and `directory` arguments.
    ///
    /// `data` is bytes-like, an `Input`, or an `OutputBundle`/`Written` (read
    /// back with its own name and sidecars). `resources` is anything
    /// `resources::resolver_arg` accepts. `directory` names a directory entry
    /// point (`Input::directory`); it excludes primary bytes and `name`.
    pub fn new(
        data: Option<&Bound<'_, PyAny>>,
        name: Option<String>,
        resources: Option<&Bound<'_, PyAny>>,
        directory: Option<String>,
    ) -> PyResult<Self> {
        let data = data.filter(|d| !d.is_none());
        let has_resources = resources.is_some_and(|r| !r.is_none());
        if let Some(d) = data {
            let carried = if let Some(bundle) = bundle_arg(d)? {
                Some(bundle_input(&bundle))
            } else if d.cast::<PyInput>().is_ok() {
                Some(input_arg(d)?)
            } else {
                None
            };
            if let Some(parts) = carried {
                if name.is_some() || has_resources || directory.is_some() {
                    return Err(InvalidInputError::new_err(
                        "an Input or OutputBundle already carries its name and resources",
                    ));
                }
                return Ok(Self { parts });
            }
        }
        let resolver = resolver_arg(resources)?;
        if let Some(dir) = directory {
            if data.is_some() || name.is_some() {
                return Err(InvalidInputError::new_err(
                    "a directory input takes no primary bytes or name; pass directory and resources",
                ));
            }
            return Ok(Self {
                parts: InputParts {
                    data: Arc::new(Vec::new()),
                    name: Some(dir),
                    resolver,
                    directory: true,
                },
            });
        }
        let data = match data {
            Some(d) => bytes_arg(d)?,
            None => Vec::new(),
        };
        Ok(Self {
            parts: InputParts {
                data: Arc::new(data),
                name,
                resolver,
                directory: false,
            },
        })
    }

    /// Build the borrowed `Input` and run `f` with it.
    pub fn with<R>(&self, f: impl FnOnce(&Input<'_>) -> R) -> R {
        crate::formats_custom::with_resources(&self.parts.resolver, || self.parts.with(f))
    }
}

// ---------------------------------------------------------------------------
// Result classes
// ---------------------------------------------------------------------------

/// A decoded model with its read report and the format actually used
/// (`fontpixel::formats::Loaded`).
#[pyclass(name = "Loaded", module = "fontpixel", frozen)]
pub struct PyLoaded {
    /// `FontSet` handle (or `Font` handle after `into_single`).
    #[pyo3(get)]
    pub fonts: Py<PyAny>,
    /// The read report as a dict (`{"entries": [...]}`).
    #[pyo3(get)]
    pub report: Py<PyAny>,
    /// Format ID used to decode the input.
    #[pyo3(get)]
    pub format: String,
    report_value: Report,
}

impl PyLoaded {
    /// Wrap a `Loaded<FontSet>`.
    pub fn new(py: Python<'_>, loaded: Loaded<FontSet>) -> PyResult<Self> {
        Ok(Self {
            fonts: Py::new(py, PyFontSet::new(loaded.fonts))?.into_any(),
            report: conv::to_py(py, &loaded.report)?.unbind(),
            format: loaded.format.to_owned(),
            report_value: loaded.report,
        })
    }
}

#[pymethods]
impl PyLoaded {
    /// The sole strike as a `Loaded` whose `fonts` is a `Font` handle
    /// (`Loaded::into_single`). Raises `InvalidInputError` for several strikes.
    #[pyo3(name = "into_single")]
    fn single_loaded(&self, py: Python<'_>) -> PyResult<Self> {
        let set = self
            .fonts
            .bind(py)
            .cast::<PyFontSet>()
            .map_err(|_| InvalidInputError::new_err("this Loaded already holds one Font"))?
            .get()
            .snapshot()?;
        let format: &'static str = static_format_id(&self.format)?;
        let loaded = Loaded {
            fonts: set,
            report: self.report_value.clone(),
            format,
        }
        .into_single()
        .py()?;
        Ok(Self {
            fonts: Py::new(py, PyFont::new(loaded.fonts))?.into_any(),
            report: conv::to_py(py, &loaded.report)?.unbind(),
            format: loaded.format.to_owned(),
            report_value: loaded.report,
        })
    }

    fn __repr__(&self) -> String {
        format!(
            "Loaded(format={:?}, report_entries={})",
            self.format,
            self.report_value.entries.len()
        )
    }
}

/// Map a format ID back to its `'static` catalogue string.
pub fn static_format_id(id: &str) -> PyResult<&'static str> {
    fontpixel::formats::catalogue::CATALOGUE
        .iter()
        .find(|f| f.id == id)
        .map(|f| f.id)
        .ok_or_else(|| crate::errors::to_py(Error::UnknownFormat(id.into())))
}

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

/// The format registry (`fontpixel::Registry<FontSet>`).
#[pyclass(name = "Registry", module = "fontpixel")]
pub struct PyRegistry {
    /// The Rust registry.
    pub registry: FontRegistry,
}

/// Default primary output name for a format: `font.<first extension>`.
pub fn default_primary(id: &str) -> PyResult<ResourcePath> {
    let info = fontpixel::formats::catalogue::CATALOGUE
        .iter()
        .find(|f| f.id == id)
        .ok_or_else(|| crate::errors::to_py(Error::UnknownFormat(id.into())))?;
    let name = match info.extensions.first() {
        Some(ext) => format!("font.{ext}"),
        None => "font".to_owned(),
    };
    path_arg(&name)
}

/// Build `WriteOptions` from keyword arguments.
pub fn write_options(
    id: &str,
    primary_name: Option<&str>,
    strict: bool,
    transport_limits: Option<&Bound<'_, PyAny>>,
) -> PyResult<WriteOptions> {
    let primary = match primary_name {
        Some(p) => path_arg(p)?,
        None => default_primary(id)?,
    };
    let mut options = WriteOptions::new(primary);
    options.strict = strict;
    options.transport_limits = transport_arg(transport_limits)?;
    Ok(options)
}

/// How a registry write treats the model.
#[derive(Clone, Copy)]
pub enum WriteMode {
    /// `write_with_report`.
    Plain,
    /// `write_reserved_with_report`.
    Reserved,
    /// `write_owned` (moves a `FontSet` handle's contents).
    Owned,
}

/// Common write keyword arguments.
pub struct WriteArgs<'a, 'py> {
    /// Earlier diagnostics (dict) or `None`.
    pub report: Option<&'a Bound<'py, PyAny>>,
    /// Primary output name, or the format default.
    pub primary_name: Option<&'a str>,
    /// Strict mode.
    pub strict: bool,
    /// `TransportLimits` dict or `None`.
    pub transport_limits: Option<&'a Bound<'py, PyAny>>,
}

impl PyRegistry {
    /// Shared read implementation.
    pub fn read_impl(
        &self,
        py: Python<'_>,
        input: &InputArgs,
        format: Option<String>,
        limits: ResourceLimits,
    ) -> PyResult<PyLoaded> {
        let options = ReadOptions { format, limits };
        let loaded = py_result(py.detach(|| input.with(|i| self.registry.read(i, &options))))?;
        PyLoaded::new(py, loaded)
    }

    /// Shared detect implementation.
    pub fn detect_impl<'py>(
        &self,
        py: Python<'py>,
        input: &InputArgs,
        limits: ResourceLimits,
    ) -> PyResult<Bound<'py, PyAny>> {
        let detected = py_result(py.detach(|| input.with(|i| self.registry.detect(i, &limits))))?;
        conv::to_py(py, &detected)
    }

    /// Shared write implementation.
    pub fn write_impl(
        &self,
        fonts: &Bound<'_, PyAny>,
        format: &str,
        args: WriteArgs<'_, '_>,
        mode: WriteMode,
    ) -> PyResult<PyWritten> {
        let py = fonts.py();
        // Default primary from this registry's metadata (Python codecs too).
        let default_name = match args.primary_name {
            Some(_) => None,
            None => {
                let info = py_result(self.registry.status(format))?.format;
                Some(match info.extensions.first() {
                    Some(ext) => format!("font.{ext}"),
                    None => "font".to_owned(),
                })
            }
        };
        let options = write_options(
            format,
            args.primary_name.or(default_name.as_deref()),
            args.strict,
            args.transport_limits,
        )?;
        let report = report_arg(args.report)?;
        let written = py_result(match mode {
            WriteMode::Owned => {
                let set = if let Ok(handle) = fonts.cast::<PyFontSet>() {
                    handle.get().write(|s| Ok(std::mem::take(s)))?
                } else {
                    SetArg::extract(fonts)?.into_owned()?
                };
                py.detach(|| self.registry.write_owned(set, format, &options, report))
            }
            WriteMode::Plain => SetArg::extract(fonts)?.with(|set| {
                Ok(py.detach(|| {
                    self.registry
                        .write_with_report(set, format, &options, report)
                }))
            })?,
            WriteMode::Reserved => SetArg::extract(fonts)?.with(|set| {
                Ok(py.detach(|| {
                    self.registry
                        .write_reserved_with_report(set, format, &options, report)
                }))
            })?,
        })?;
        PyWritten::new(py, written)
    }
}

#[pymethods]
impl PyRegistry {
    /// The built-in registry with every compiled codec (`fontpixel::registry()`),
    /// or with `builtin=False` the planned catalogue without codecs
    /// (`Registry::new`).
    #[new]
    #[pyo3(signature = (builtin = true))]
    fn new(builtin: bool) -> Self {
        Self {
            registry: if builtin {
                fontpixel::registry()
            } else {
                FontRegistry::new()
            },
        }
    }

    /// Every catalogue format with its capabilities, as `FormatStatus` dicts
    /// (`{"format": {"id", "name", "extensions", "feature_enabled"}, "detect",
    /// "read", "write"}`).
    fn formats<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let all: Vec<_> = self.registry.formats().collect();
        conv::to_py(py, &all)
    }

    /// The `FormatStatus` dict of one format ID; raises `UnknownFormatError`.
    fn status<'py>(&self, py: Python<'py>, format: &str) -> PyResult<Bound<'py, PyAny>> {
        conv::to_py(py, &self.registry.status(format).py()?)
    }

    /// Formats whose extension is the longest matching suffix of `name`
    /// (`FormatInfo` dicts; ties are all returned).
    fn extension_candidates<'py>(
        &self,
        py: Python<'py>,
        name: &str,
    ) -> PyResult<Bound<'py, PyAny>> {
        conv::to_py(py, &self.registry.extension_candidates(name))
    }

    /// The unique `FormatInfo` for a filename; raises `UnrecognizedFormatError`
    /// or `AmbiguousFormatError` (with `candidates`).
    fn format_from_name<'py>(&self, py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
        conv::to_py(py, &self.registry.format_from_name(name).py()?)
    }

    /// Detect the format of an input: content detectors first, the filename
    /// hint only when no detector matches. Returns
    /// `{"format": FormatInfo, "confidence": "certain"|"possible"|"extension"}`.
    /// `directory` names a directory entry point (an error: directory inputs
    /// need an explicit format).
    #[pyo3(signature = (data = None, *, name = None, resources = None, limits = None, directory = None))]
    fn detect<'py>(
        &self,
        py: Python<'py>,
        data: Option<&Bound<'py, PyAny>>,
        name: Option<String>,
        resources: Option<&Bound<'py, PyAny>>,
        limits: Option<&Bound<'py, PyAny>>,
        directory: Option<String>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let input = InputArgs::new(data, name, resources, directory)?;
        self.detect_impl(py, &input, limits_arg(limits)?)
    }

    /// Raise `NotImplementedFormatError` unless `format` has a writer.
    fn ensure_writable(&self, format: &str) -> PyResult<()> {
        self.registry.ensure_writable(format).py()
    }

    /// Decode an input (`Registry::read`). `format` bypasses detection;
    /// `limits` is a `ResourceLimits` dict; `resources` supplies sidecars;
    /// `directory="name"` reads a native directory entry point whose files come
    /// from `resources` (an explicit format is then required).
    #[pyo3(signature = (data = None, format = None, *, name = None, resources = None, limits = None, directory = None))]
    fn read(
        &self,
        data: Option<&Bound<'_, PyAny>>,
        format: Option<String>,
        name: Option<String>,
        resources: Option<&Bound<'_, PyAny>>,
        limits: Option<&Bound<'_, PyAny>>,
        directory: Option<String>,
    ) -> PyResult<PyLoaded> {
        let input = InputArgs::new(data, name, resources, directory)?;
        let limits = limits_arg(limits)?;
        Python::attach(|py| self.read_impl(py, &input, format, limits))
    }

    /// Encode a `FontSet` (or one `Font`) as `format` (`Registry::write`, or
    /// `write_with_report` when `report` carries earlier read diagnostics).
    /// `primary_name` defaults to `font.<first extension>`; a `.gz` suffix
    /// selects the gzip envelope. `strict=True` raises `StrictLossError` on
    /// any read or write loss. `transport_limits` is a `TransportLimits` dict.
    #[pyo3(signature = (fonts, format, *, report = None, primary_name = None, strict = false, transport_limits = None))]
    fn write(
        &self,
        fonts: &Bound<'_, PyAny>,
        format: &str,
        report: Option<&Bound<'_, PyAny>>,
        primary_name: Option<&str>,
        strict: bool,
        transport_limits: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<PyWritten> {
        let args = WriteArgs {
            report,
            primary_name,
            strict,
            transport_limits,
        };
        self.write_impl(fonts, format, args, WriteMode::Plain)
    }

    /// `Registry::write_with_report`: combine `report` (earlier read
    /// diagnostics, a dict) with the write diagnostics before strict checks.
    #[pyo3(signature = (fonts, format, report, *, primary_name = None, strict = false, transport_limits = None))]
    fn write_with_report(
        &self,
        fonts: &Bound<'_, PyAny>,
        format: &str,
        report: &Bound<'_, PyAny>,
        primary_name: Option<&str>,
        strict: bool,
        transport_limits: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<PyWritten> {
        let args = WriteArgs {
            report: Some(report),
            primary_name,
            strict,
            transport_limits,
        };
        self.write_impl(fonts, format, args, WriteMode::Plain)
    }

    /// `Registry::write_reserved_with_report`: as `write_with_report` while the
    /// caller keeps the model's retained usage reserved outside
    /// `transport_limits`. Output and report are identical.
    #[pyo3(signature = (fonts, format, report = None, *, primary_name = None, strict = false, transport_limits = None))]
    fn write_reserved_with_report(
        &self,
        fonts: &Bound<'_, PyAny>,
        format: &str,
        report: Option<&Bound<'_, PyAny>>,
        primary_name: Option<&str>,
        strict: bool,
        transport_limits: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<PyWritten> {
        let args = WriteArgs {
            report,
            primary_name,
            strict,
            transport_limits,
        };
        self.write_impl(fonts, format, args, WriteMode::Reserved)
    }

    /// `Registry::write_owned`: hand the model over so it can be released
    /// before later encoder stages. A `FontSet` argument is *moved*: the
    /// handle is left empty (zero strikes) afterwards. Output and report equal
    /// `write_with_report`; `transport_limits` include the model.
    #[pyo3(signature = (fonts, format, report = None, *, primary_name = None, strict = false, transport_limits = None))]
    fn write_owned(
        &self,
        fonts: &Bound<'_, PyAny>,
        format: &str,
        report: Option<&Bound<'_, PyAny>>,
        primary_name: Option<&str>,
        strict: bool,
        transport_limits: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<PyWritten> {
        let args = WriteArgs {
            report,
            primary_name,
            strict,
            transport_limits,
        };
        self.write_impl(fonts, format, args, WriteMode::Owned)
    }

    /// Configure the typed reader options of `format` from a dict (the
    /// format's `*ReadOptions` fields; unknown keys raise `ValueError`).
    fn set_read_options(&mut self, format: &str, options: &Bound<'_, PyAny>) -> PyResult<()> {
        crate::formats_options::set_read(&mut self.registry, format, options)
    }

    /// Configure the typed writer options of `format` from a dict (the
    /// format's `*WriteOptions` fields; unknown keys raise `ValueError`).
    fn set_write_options(&mut self, format: &str, options: &Bound<'_, PyAny>) -> PyResult<()> {
        crate::formats_options::set_write(&mut self.registry, format, options)
    }

    /// Configure both sides of `format`; `None` leaves a side unchanged.
    #[pyo3(signature = (format, read = None, write = None))]
    fn set_options(
        &mut self,
        format: &str,
        read: Option<&Bound<'_, PyAny>>,
        write: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        if let Some(r) = read.filter(|r| !r.is_none()) {
            crate::formats_options::set_read(&mut self.registry, format, r)?;
        }
        if let Some(w) = write.filter(|w| !w.is_none()) {
            crate::formats_options::set_write(&mut self.registry, format, w)?;
        }
        Ok(())
    }

    /// Install a codec from its dict form (`{"read_options": …,
    /// "write_options": …}`, the format's `*Codec` struct); missing sides use
    /// their defaults.
    fn set_codec(&mut self, format: &str, codec: &Bound<'_, PyAny>) -> PyResult<()> {
        crate::formats_options::set_codec(&mut self.registry, format, codec)
    }

    fn __repr__(&self) -> String {
        let live = self
            .registry
            .formats()
            .filter(|s| s.read || s.write || s.detect)
            .count();
        format!(
            "Registry({} formats, {live} with codecs)",
            self.registry.formats().len()
        )
    }
}

// ---------------------------------------------------------------------------
// Module functions
// ---------------------------------------------------------------------------

/// The built-in registry (`fontpixel::registry()`).
#[pyfunction]
fn registry() -> PyRegistry {
    PyRegistry::new(true)
}

/// Every catalogue format with its capabilities (`FormatStatus` dicts).
#[pyfunction]
fn list_formats<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
    let all: Vec<_> = fontpixel::registry().formats().collect();
    conv::to_py(py, &all)
}

/// Decode font bytes with the built-in registry (`fontpixel::read`).
///
/// `format` selects the reader (otherwise content detection, then `name`
/// as a filename hint). `options` is the format's typed read-options dict.
/// `resources` supplies sidecars (`dict[str, bytes]`, a callable or a
/// resolver object). `limits` is a `ResourceLimits` dict. `directory="name"`
/// reads a native directory entry point through `resources`.
#[pyfunction]
#[pyo3(signature = (data = None, format = None, options = None, resources = None, *, name = None, limits = None, directory = None))]
fn read(
    data: Option<&Bound<'_, PyAny>>,
    format: Option<String>,
    options: Option<&Bound<'_, PyAny>>,
    resources: Option<&Bound<'_, PyAny>>,
    name: Option<String>,
    limits: Option<&Bound<'_, PyAny>>,
    directory: Option<String>,
) -> PyResult<PyLoaded> {
    let mut reg = PyRegistry::new(true);
    let input = InputArgs::new(data, name, resources, directory)?;
    let limits = limits_arg(limits)?;
    Python::attach(|py| {
        let format = match (format, options.filter(|o| !o.is_none())) {
            (Some(f), Some(o)) => {
                crate::formats_options::set_read(&mut reg.registry, &f, o)?;
                Some(f)
            }
            (None, Some(o)) => {
                let detected =
                    py_result(py.detach(|| input.with(|i| reg.registry.detect(i, &limits))))?;
                let id = detected.format.id.to_owned();
                crate::formats_options::set_read(&mut reg.registry, &id, o)?;
                Some(id)
            }
            (f, None) => f,
        };
        reg.read_impl(py, &input, format, limits)
    })
}

/// Encode a `FontSet` (or one `Font`) with the built-in registry
/// (`fontpixel::write`, or `write_with_report` when `report` is given).
///
/// `options` is the format's typed write-options dict. `primary_name`
/// defaults to `font.<first extension>`; a `.gz` suffix selects the gzip
/// envelope. `strict=True` raises `StrictLossError` on any loss.
#[pyfunction]
#[pyo3(signature = (fonts, format, options = None, *, report = None, primary_name = None, strict = false, transport_limits = None))]
fn write(
    fonts: &Bound<'_, PyAny>,
    format: &str,
    options: Option<&Bound<'_, PyAny>>,
    report: Option<&Bound<'_, PyAny>>,
    primary_name: Option<&str>,
    strict: bool,
    transport_limits: Option<&Bound<'_, PyAny>>,
) -> PyResult<PyWritten> {
    let mut reg = PyRegistry::new(true);
    if let Some(o) = options.filter(|o| !o.is_none()) {
        crate::formats_options::set_write(&mut reg.registry, format, o)?;
    }
    let args = WriteArgs {
        report,
        primary_name,
        strict,
        transport_limits,
    };
    reg.write_impl(fonts, format, args, WriteMode::Plain)
}

/// Detect the format of font bytes with the built-in registry. Returns
/// `{"format": FormatInfo, "confidence": …}`.
#[pyfunction]
#[pyo3(signature = (data = None, name = None, *, resources = None, limits = None))]
fn detect<'py>(
    py: Python<'py>,
    data: Option<&Bound<'py, PyAny>>,
    name: Option<String>,
    resources: Option<&Bound<'py, PyAny>>,
    limits: Option<&Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyAny>> {
    let input = InputArgs::new(data, name, resources, None)?;
    PyRegistry::new(true).detect_impl(py, &input, limits_arg(limits)?)
}

/// Register the format classes and functions, and the `fontpixel.formats`
/// submodule with its per-codec namespaces.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyRegistry>()?;
    m.add_class::<PyLoaded>()?;
    m.add_function(wrap_pyfunction!(registry, m)?)?;
    m.add_function(wrap_pyfunction!(list_formats, m)?)?;
    m.add_function(wrap_pyfunction!(read, m)?)?;
    m.add_function(wrap_pyfunction!(write, m)?)?;
    m.add_function(wrap_pyfunction!(detect, m)?)?;
    let py = m.py();
    let formats = PyModule::new(py, "formats")?;
    formats.add(
        "__doc__",
        "Per-codec namespaces (`fontpixel.formats.<module>`) and typed option helpers.",
    )?;
    crate::formats_options::register(&formats)?;
    crate::formats_direct::register(&formats)?;
    crate::formats_extra::register(&formats)?;
    m.add_submodule(&formats)?;
    let modules = py.import("sys")?.getattr("modules")?;
    modules.set_item("fontpixel.formats", &formats)?;
    let names: Vec<String> = formats.getattr("MODULES")?.extract()?;
    for name in names {
        let sub = formats.getattr(name.as_str())?;
        modules.set_item(format!("fontpixel.formats.{name}"), sub)?;
    }
    Ok(())
}
