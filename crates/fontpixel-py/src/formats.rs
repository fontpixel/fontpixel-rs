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
//! - `Loaded`, `Written`, `OutputBundle`, `OutputAsset`, `ResourcePath` classes.
//! - `fontpixel.formats`: per-module namespaces with the direct codec functions
//!   (see [`crate::formats_direct`]) plus `normalize`/`option_types` for every
//!   serde type of the format layer (see [`crate::formats_options`]).
//!
//! Inputs are `bytes`, `bytearray` or `memoryview`. Sidecar resources are an
//! explicit `dict[str, bytes]`, a callable `resolve(path, max_bytes)`, or an
//! object with `resolve(path, max_bytes)` and optionally
//! `list(directory, max_entries, max_bytes)` methods. Nothing reads the
//! filesystem implicitly.
use crate::conv;
use crate::errors::{IntoPy, InvalidInputError};
use crate::handles::{PyFont, PyFontSet};
use fontpixel::formats::{Loaded, ReadOptions, WriteOptions};
use fontpixel::resources::{
    Input, OutputAsset, OutputBundle, ResourceLimits, ResourcePath, ResourceResolver,
    TransportLimits, Written,
};
use fontpixel::{Error, FontRegistry, FontSet, Report};
use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use pyo3::types::{PyByteArray, PyBytes, PyDict, PyMemoryView};
use std::borrow::Cow;
use std::collections::BTreeMap;

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

/// Borrow or copy a bytes-like argument (`bytes`, `bytearray`, `memoryview`).
pub fn bytes_arg<'a>(obj: &'a Bound<'_, PyAny>) -> PyResult<Cow<'a, [u8]>> {
    if let Ok(b) = obj.cast::<PyBytes>() {
        return Ok(Cow::Borrowed(b.as_bytes()));
    }
    if let Ok(b) = obj.cast::<PyByteArray>() {
        return Ok(Cow::Owned(b.to_vec()));
    }
    if obj.cast::<PyMemoryView>().is_ok() {
        let copied = obj.call_method0("tobytes")?;
        let copied = copied.cast::<PyBytes>()?;
        return Ok(Cow::Owned(copied.as_bytes().to_vec()));
    }
    Err(PyTypeError::new_err(format!(
        "expected bytes, bytearray or memoryview, got {}",
        obj.get_type().name()?
    )))
}

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

/// Validated relative output/resource path.
pub fn path_arg(path: &str) -> PyResult<ResourcePath> {
    ResourcePath::new(path).py()
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
// Resources
// ---------------------------------------------------------------------------

enum ResolverKind {
    /// Explicit named resources, copied at call time.
    Map(BTreeMap<String, Vec<u8>>),
    /// `resolve(path, max_bytes)` callable.
    Callable(Py<PyAny>),
    /// Object with `resolve` and optionally `list` methods.
    Object(Py<PyAny>),
}

/// A caller-supplied resource source implementing `ResourceResolver`.
pub struct PyResolver {
    kind: ResolverKind,
}

fn resolver_error(py: Python<'_>, path: &str, err: PyErr) -> Error {
    if err.is_instance_of::<pyo3::exceptions::PyKeyError>(py)
        || err.is_instance_of::<pyo3::exceptions::PyFileNotFoundError>(py)
        || err.is_instance_of::<crate::errors::MissingResourceError>(py)
    {
        return Error::MissingResource(path.into());
    }
    if err.is_instance_of::<crate::errors::ResourceLimitError>(py) {
        let limit = err
            .value(py)
            .getattr("limit")
            .and_then(|l| l.extract::<usize>())
            .unwrap_or(0);
        return Error::ResourceLimit {
            resource: "resource bytes",
            limit,
        };
    }
    Error::Io(std::io::Error::other(format!(
        "resource resolver failed for '{path}': {err}"
    )))
}

fn checked_resource(path: &str, bytes: &[u8], max_bytes: usize) -> fontpixel::Result<Vec<u8>> {
    if bytes.len() > max_bytes {
        return Err(Error::ResourceLimit {
            resource: "resource bytes",
            limit: max_bytes,
        });
    }
    let _ = path;
    // `to_vec` allocates exactly `len` bytes, so capacity stays within budget.
    Ok(bytes.to_vec())
}

fn resolved_bytes(
    py: Python<'_>,
    path: &str,
    value: PyResult<Bound<'_, PyAny>>,
    max_bytes: usize,
) -> fontpixel::Result<Vec<u8>> {
    let value = value.map_err(|e| resolver_error(py, path, e))?;
    if value.is_none() {
        return Err(Error::MissingResource(path.into()));
    }
    let bytes = bytes_arg(&value).map_err(|e| resolver_error(py, path, e))?;
    checked_resource(path, &bytes, max_bytes)
}

impl ResourceResolver for PyResolver {
    fn resolve(&self, path: &ResourcePath, max_bytes: usize) -> fontpixel::Result<Vec<u8>> {
        let name = path.as_str();
        match &self.kind {
            ResolverKind::Map(map) => match map.get(name) {
                Some(bytes) => checked_resource(name, bytes, max_bytes),
                None => Err(Error::MissingResource(name.into())),
            },
            ResolverKind::Callable(f) => Python::attach(|py| {
                resolved_bytes(py, name, f.bind(py).call1((name, max_bytes)), max_bytes)
            }),
            ResolverKind::Object(o) => Python::attach(|py| {
                resolved_bytes(
                    py,
                    name,
                    o.bind(py).call_method1("resolve", (name, max_bytes)),
                    max_bytes,
                )
            }),
        }
    }

    fn list(
        &self,
        directory: Option<&ResourcePath>,
        max_entries: usize,
        max_bytes: usize,
    ) -> fontpixel::Result<Vec<ResourcePath>> {
        match &self.kind {
            ResolverKind::Map(map) => {
                let prefix = directory.map(|d| format!("{}/", d.as_str()));
                let mut out = Vec::new();
                let mut used = 0usize;
                for key in map.keys() {
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
                    used = used.saturating_add(
                        std::mem::size_of::<ResourcePath>()
                            .saturating_add(key.len())
                            .saturating_mul(2),
                    );
                    if used > max_bytes {
                        return Err(Error::ResourceLimit {
                            resource: "directory listing bytes",
                            limit: max_bytes,
                        });
                    }
                    out.push(ResourcePath::new(key.clone())?);
                }
                Ok(out)
            }
            ResolverKind::Object(o) => Python::attach(|py| {
                let o = o.bind(py);
                let dir = directory.map(|d| d.as_str().to_owned());
                if !o.hasattr("list").unwrap_or(false) {
                    return Err(Error::InvalidInput(
                        "resource enumeration is unavailable".into(),
                    ));
                }
                let listed = o
                    .call_method1("list", (dir.clone(), max_entries, max_bytes))
                    .and_then(|v| v.extract::<Vec<String>>())
                    .map_err(|e| resolver_error(py, dir.as_deref().unwrap_or(""), e))?;
                listed.into_iter().map(ResourcePath::new).collect()
            }),
            ResolverKind::Callable(_) => Err(Error::InvalidInput(
                "resource enumeration is unavailable".into(),
            )),
        }
    }
}

impl PyResolver {
    /// Build from `None`, a `dict[str, bytes]`, a resolver object or a callable.
    pub fn extract(obj: Option<&Bound<'_, PyAny>>) -> PyResult<Option<Self>> {
        let Some(obj) = obj else { return Ok(None) };
        if obj.is_none() {
            return Ok(None);
        }
        if let Ok(dict) = obj.cast::<PyDict>() {
            let mut map = BTreeMap::new();
            for (k, v) in dict.iter() {
                let key: String = k.extract()?;
                ResourcePath::new(key.clone()).py()?;
                map.insert(key, bytes_arg(&v)?.into_owned());
            }
            return Ok(Some(Self {
                kind: ResolverKind::Map(map),
            }));
        }
        if obj.hasattr("resolve")? {
            return Ok(Some(Self {
                kind: ResolverKind::Object(obj.clone().unbind()),
            }));
        }
        if obj.is_callable() {
            return Ok(Some(Self {
                kind: ResolverKind::Callable(obj.clone().unbind()),
            }));
        }
        Err(PyTypeError::new_err(
            "resources must be a dict[str, bytes], a resolve(path, max_bytes) callable \
             or an object with a resolve method",
        ))
    }
}

/// An `OutputBundle` (or the bundle of a `Written`) passed as read input.
fn bundle_arg(obj: &Bound<'_, PyAny>) -> Option<Py<PyOutputBundle>> {
    if let Ok(b) = obj.cast::<PyOutputBundle>() {
        return Some(b.clone().unbind());
    }
    obj.cast::<PyWritten>()
        .ok()
        .map(|w| w.get().output.clone_ref(obj.py()))
}

/// Owned pieces of an `Input`, built from Python arguments.
pub struct InputArgs<'a> {
    data: Cow<'a, [u8]>,
    name: Option<String>,
    resolver: Option<PyResolver>,
    directory: bool,
}

impl<'a> InputArgs<'a> {
    /// Collect `data`, `name`, `resources` and `directory` arguments.
    /// `directory` is a directory entry-point name (`Input::directory`); it
    /// excludes primary bytes and replaces `name`.
    pub fn new(
        data: Option<&'a Bound<'_, PyAny>>,
        name: Option<String>,
        resources: Option<&Bound<'_, PyAny>>,
        directory: Option<String>,
    ) -> PyResult<Self> {
        if let Some(bundle) = data.and_then(|d| bundle_arg(d)) {
            if name.is_some() || resources.is_some_and(|r| !r.is_none()) || directory.is_some()
            {
                return Err(InvalidInputError::new_err(
                    "an OutputBundle input already carries its name and resources",
                ));
            }
            return Ok(Self::from_bundle(&bundle.get().bundle));
        }
        let resolver = PyResolver::extract(resources)?;
        let data = match data {
            Some(d) if !d.is_none() => bytes_arg(d)?,
            _ => Cow::Borrowed(&[][..]),
        };
        if let Some(dir) = directory {
            if !data.is_empty() || name.is_some() {
                return Err(InvalidInputError::new_err(
                    "a directory input takes no primary bytes or name; pass directory and resources",
                ));
            }
            return Ok(Self {
                data,
                name: Some(dir),
                resolver,
                directory: true,
            });
        }
        Ok(Self {
            data,
            name,
            resolver,
            directory: false,
        })
    }

    /// The input a reader sees for a written bundle: a file bundle's primary
    /// bytes with every asset as a sidecar (paths relative to the entry
    /// point's parent), or a directory bundle's entry point with its assets
    /// relative to the directory.
    pub fn from_bundle(bundle: &OutputBundle) -> Self {
        let primary = bundle.primary().as_str();
        let mut map = BTreeMap::new();
        let mut data = Vec::new();
        if bundle.is_directory() {
            let prefix = format!("{primary}/");
            for a in bundle.assets() {
                if let Some(rest) = a.path.as_str().strip_prefix(&prefix) {
                    map.insert(rest.to_owned(), a.bytes.clone());
                }
            }
        } else {
            for a in bundle.assets() {
                if a.path.as_str() == primary {
                    data = a.bytes.clone();
                } else {
                    map.insert(a.path.as_str().to_owned(), a.bytes.clone());
                }
            }
        }
        Self {
            data: Cow::Owned(data),
            name: Some(primary.to_owned()),
            resolver: Some(PyResolver {
                kind: ResolverKind::Map(map),
            }),
            directory: bundle.is_directory(),
        }
    }

    /// Build the borrowed `Input` and run `f` with it.
    pub fn with<R>(&self, f: impl FnOnce(&Input<'_>) -> R) -> R {
        let none = fontpixel::resources::NoResources;
        let resolver: &dyn ResourceResolver = match &self.resolver {
            Some(r) => r,
            None => &none,
        };
        let input = if self.directory {
            Input::directory(self.name.as_deref().unwrap_or_default(), resolver)
        } else {
            let mut input = Input::new(&self.data).with_resources(resolver);
            if let Some(name) = &self.name {
                input = input.with_name(name);
            }
            input
        };
        f(&input)
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
    fn into_single(&self, py: Python<'_>) -> PyResult<Self> {
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

/// One output asset: a validated relative path and its exact bytes
/// (`fontpixel::resources::OutputAsset`).
#[pyclass(name = "OutputAsset", module = "fontpixel", frozen)]
pub struct PyOutputAsset {
    asset: OutputAsset,
}

#[pymethods]
impl PyOutputAsset {
    /// Create an asset; the path is validated like `ResourcePath`.
    #[new]
    #[pyo3(signature = (path, data))]
    fn new(path: &str, data: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            asset: OutputAsset {
                path: path_arg(path)?,
                bytes: bytes_arg(data)?.into_owned(),
            },
        })
    }
    /// Relative path, including any subdirectory.
    #[getter]
    fn path(&self) -> &str {
        self.asset.path.as_str()
    }
    /// The asset bytes (a copy).
    #[getter]
    fn bytes<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.asset.bytes)
    }
    fn __repr__(&self) -> String {
        format!(
            "OutputAsset({:?}, {} bytes)",
            self.asset.path.as_str(),
            self.asset.bytes.len()
        )
    }
    fn __eq__(&self, other: &Bound<'_, PyAny>) -> bool {
        other
            .cast::<PyOutputAsset>()
            .is_ok_and(|o| o.get().asset == self.asset)
    }
}

/// Complete validated output: one entry point plus sidecars
/// (`fontpixel::resources::OutputBundle`).
#[pyclass(name = "OutputBundle", module = "fontpixel", frozen)]
pub struct PyOutputBundle {
    /// The Rust bundle.
    pub bundle: OutputBundle,
}

fn assets_arg(assets: &Bound<'_, PyAny>) -> PyResult<Vec<OutputAsset>> {
    let mut out = Vec::new();
    if let Ok(dict) = assets.cast::<PyDict>() {
        for (k, v) in dict.iter() {
            out.push(OutputAsset {
                path: path_arg(&k.extract::<String>()?)?,
                bytes: bytes_arg(&v)?.into_owned(),
            });
        }
        return Ok(out);
    }
    for item in assets.try_iter()? {
        let item = item?;
        if let Ok(a) = item.cast::<PyOutputAsset>() {
            out.push(a.get().asset.clone());
        } else {
            let (path, data): (String, Bound<'_, PyAny>) = item.extract()?;
            out.push(OutputAsset {
                path: path_arg(&path)?,
                bytes: bytes_arg(&data)?.into_owned(),
            });
        }
    }
    Ok(out)
}

#[pymethods]
impl PyOutputBundle {
    /// Validate a bundle (`OutputBundle::new`, or `OutputBundle::directory`
    /// when `directory=True`). `assets` is a `dict[str, bytes]`, or an
    /// iterable of `OutputAsset` / `(path, bytes)` pairs.
    #[new]
    #[pyo3(signature = (primary, assets, directory = false))]
    fn new(primary: &str, assets: &Bound<'_, PyAny>, directory: bool) -> PyResult<Self> {
        let primary = path_arg(primary)?;
        let assets = assets_arg(assets)?;
        let bundle = if directory {
            OutputBundle::directory(primary, assets)
        } else {
            OutputBundle::new(primary, assets)
        }
        .py()?;
        Ok(Self { bundle })
    }
    /// A one-file bundle (`OutputBundle::single`).
    #[staticmethod]
    #[pyo3(signature = (path, data))]
    fn single(path: &str, data: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            bundle: OutputBundle::single(path_arg(path)?, bytes_arg(data)?.into_owned()),
        })
    }
    /// A native directory bundle (`OutputBundle::directory`).
    #[staticmethod]
    #[pyo3(signature = (primary, assets))]
    fn directory(primary: &str, assets: &Bound<'_, PyAny>) -> PyResult<Self> {
        Self::new(primary, assets, true)
    }
    /// Entry-point path.
    #[getter]
    fn primary(&self) -> &str {
        self.bundle.primary().as_str()
    }
    /// Whether the entry point is a directory.
    #[getter]
    fn is_directory(&self) -> bool {
        self.bundle.is_directory()
    }
    /// `{path: bytes}` for every asset, sorted by path (copies).
    #[getter]
    fn assets<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let d = PyDict::new(py);
        for a in self.bundle.assets() {
            d.set_item(a.path.as_str(), PyBytes::new(py, &a.bytes))?;
        }
        Ok(d)
    }
    /// Every asset as `OutputAsset` objects, sorted by path (copies;
    /// `OutputBundle::into_assets`).
    fn into_assets(&self) -> Vec<PyOutputAsset> {
        self.bundle
            .assets()
            .iter()
            .map(|a| PyOutputAsset { asset: a.clone() })
            .collect()
    }
    /// The bytes of a one-file bundle; raises `MultipleAssetsError` when the
    /// bundle has sidecars (`OutputBundle::into_single`).
    fn into_single<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        let bytes = self.bundle.clone().into_single().py()?;
        Ok(PyBytes::new(py, &bytes))
    }
    /// Owned storage of this bundle, bounded by `max_bytes`
    /// (`OutputBundle::retained_bytes`).
    #[pyo3(signature = (max_bytes = usize::MAX))]
    fn retained_bytes(&self, max_bytes: usize) -> PyResult<usize> {
        self.bundle.retained_bytes(max_bytes).py()
    }
    fn __len__(&self) -> usize {
        self.bundle.assets().len()
    }
    fn __repr__(&self) -> String {
        format!(
            "OutputBundle(primary={:?}, assets={}, directory={})",
            self.bundle.primary().as_str(),
            self.bundle.assets().len(),
            self.bundle.is_directory()
        )
    }
    fn __eq__(&self, other: &Bound<'_, PyAny>) -> bool {
        other
            .cast::<PyOutputBundle>()
            .is_ok_and(|o| o.get().bundle == self.bundle)
    }
}

/// Encoded assets and the combined read/write report
/// (`fontpixel::resources::Written`).
#[pyclass(name = "Written", module = "fontpixel", frozen)]
pub struct PyWritten {
    /// All encoded assets.
    #[pyo3(get)]
    pub output: Py<PyOutputBundle>,
    /// Conversion diagnostics as a dict.
    #[pyo3(get)]
    pub report: Py<PyAny>,
}

impl PyWritten {
    /// Wrap a `Written`.
    pub fn new(py: Python<'_>, written: Written) -> PyResult<Self> {
        Ok(Self {
            output: Py::new(
                py,
                PyOutputBundle {
                    bundle: written.output,
                },
            )?,
            report: conv::to_py(py, &written.report)?.unbind(),
        })
    }
}

#[pymethods]
impl PyWritten {
    /// `{path: bytes}` for every asset (shortcut for `output.assets`).
    #[getter]
    fn assets<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        self.output.get().assets(py)
    }
    /// The bytes of a one-file result; raises `MultipleAssetsError` otherwise.
    fn into_single<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        self.output.get().into_single(py)
    }
    fn __repr__(&self) -> String {
        format!("Written({})", self.output.get().__repr__())
    }
}

/// A validated relative resource path (`fontpixel::resources::ResourcePath`).
#[pyclass(name = "ResourcePath", module = "fontpixel", frozen, eq, hash)]
#[derive(PartialEq, Eq, Hash)]
pub struct PyResourcePath {
    path: ResourcePath,
}

#[pymethods]
impl PyResourcePath {
    /// Validate a relative path; empty, `.`/`..` components, backslashes,
    /// colons and control characters raise `InvalidInputError`.
    #[new]
    fn new(path: String) -> PyResult<Self> {
        Ok(Self {
            path: path_arg(&path)?,
        })
    }
    /// The path string.
    fn as_str(&self) -> &str {
        self.path.as_str()
    }
    fn __str__(&self) -> &str {
        self.path.as_str()
    }
    fn __repr__(&self) -> String {
        format!("ResourcePath({:?})", self.path.as_str())
    }
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
        input: &InputArgs<'_>,
        format: Option<String>,
        limits: ResourceLimits,
    ) -> PyResult<PyLoaded> {
        let options = ReadOptions { format, limits };
        let loaded = py
            .detach(|| input.with(|i| self.registry.read(i, &options)))
            .py()?;
        PyLoaded::new(py, loaded)
    }

    /// Shared detect implementation.
    pub fn detect_impl<'py>(
        &self,
        py: Python<'py>,
        input: &InputArgs<'_>,
        limits: ResourceLimits,
    ) -> PyResult<Bound<'py, PyAny>> {
        let detected = py
            .detach(|| input.with(|i| self.registry.detect(i, &limits)))
            .py()?;
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
        let options = write_options(
            format,
            args.primary_name,
            args.strict,
            args.transport_limits,
        )?;
        let report = report_arg(args.report)?;
        let written = match mode {
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
        }
        .py()?;
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
                let detected = py
                    .detach(|| input.with(|i| reg.registry.detect(i, &limits)))
                    .py()?;
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
    m.add_class::<PyWritten>()?;
    m.add_class::<PyOutputBundle>()?;
    m.add_class::<PyOutputAsset>()?;
    m.add_class::<PyResourcePath>()?;
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
