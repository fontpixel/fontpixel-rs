//! Inputs, sidecar resolvers, read contexts and output bundles
//! (`fontpixel::resources`).
//!
//! Resolvers are explicit: `None` (standalone input, sidecar requests raise
//! `MissingResourceError`), a `ResourceSet` / `dict[str, bytes]` of named
//! in-memory files, or any object with `resolve(path, max_bytes) -> bytes`
//! (and optionally `list(directory, max_entries, max_bytes) -> list[str]`),
//! or a plain callable `f(path, max_bytes) -> bytes`. Nothing reads the
//! filesystem implicitly. Other binding modules turn such an argument into a
//! Rust resolver with [`resolver_arg`].
use crate::bitmap::bytes_arg;
use crate::conv::{from_py, from_py_required, to_py};
use crate::errors::{FontpixelError, IntoPy, MissingResourceError};
use fontpixel::resources::{
    Input, NoResources, OutputAsset, OutputBundle, ReadContext, ResourceLimits, ResourcePath,
    ResourceResolver, TransportLimits, Written,
};
use fontpixel::{Error, Result};
use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList};
use std::collections::BTreeMap;
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex, RwLock};

fn poisoned() -> PyErr {
    FontpixelError::new_err("resource state was poisoned by an earlier panic")
}

/// Validate a portable relative asset path (`ResourcePath::new`).
pub fn path_arg(path: &str) -> PyResult<ResourcePath> {
    ResourcePath::new(path).py()
}

// resolvers -----------------------------------------------------------------

type Files = Arc<RwLock<BTreeMap<ResourcePath, Vec<u8>>>>;

/// In-memory resolver over named files; `list` returns the immediate files
/// of a directory prefix.
struct MemoryResolver(Files);

fn limit(resource: &'static str, limit: usize) -> Error {
    Error::ResourceLimit { resource, limit }
}

impl ResourceResolver for MemoryResolver {
    fn resolve(&self, path: &ResourcePath, max_bytes: usize) -> Result<Vec<u8>> {
        let files = self
            .0
            .read()
            .map_err(|_| Error::InvalidInput("resource set poisoned".into()))?;
        let data = files
            .get(path)
            .ok_or_else(|| Error::MissingResource(path.to_string()))?;
        if data.len() > max_bytes {
            return Err(limit("resource bytes", max_bytes));
        }
        Ok(data.clone())
    }
    fn list(
        &self,
        directory: Option<&ResourcePath>,
        max_entries: usize,
        max_bytes: usize,
    ) -> Result<Vec<ResourcePath>> {
        let files = self
            .0
            .read()
            .map_err(|_| Error::InvalidInput("resource set poisoned".into()))?;
        let mut out = Vec::new();
        let mut used = 0usize;
        for path in files.keys() {
            let relative = match directory {
                Some(dir) => path
                    .as_str()
                    .strip_prefix(dir.as_str())
                    .and_then(|s| s.strip_prefix('/')),
                None => Some(path.as_str()),
            };
            if relative.is_none_or(|r| r.contains('/')) {
                continue;
            }
            if out.len() == max_entries {
                return Err(limit("directory entries", max_entries));
            }
            used = used.saturating_add(
                (std::mem::size_of::<ResourcePath>() + path.as_str().len()).saturating_mul(2),
            );
            if used > max_bytes {
                return Err(limit("directory listing bytes", max_bytes));
            }
            out.push(path.clone());
        }
        Ok(out)
    }
}

/// Resolver backed by a Python object or callable.
struct PyResolver(Py<PyAny>);

fn py_failure(py: Python<'_>, err: PyErr, path: &ResourcePath) -> Error {
    if err.is_instance_of::<MissingResourceError>(py)
        || err.is_instance_of::<pyo3::exceptions::PyKeyError>(py)
        || err.is_instance_of::<pyo3::exceptions::PyFileNotFoundError>(py)
    {
        Error::MissingResource(path.to_string())
    } else {
        Error::Io(std::io::Error::other(format!("resolver raised: {err}")))
    }
}

impl ResourceResolver for PyResolver {
    fn resolve(&self, path: &ResourcePath, max_bytes: usize) -> Result<Vec<u8>> {
        Python::attach(|py| {
            let obj = self.0.bind(py);
            let result = if obj.hasattr("resolve").unwrap_or(false) {
                obj.call_method1("resolve", (path.as_str(), max_bytes))
            } else {
                obj.call1((path.as_str(), max_bytes))
            };
            let value = result.map_err(|e| py_failure(py, e, path))?;
            if value.is_none() {
                return Err(Error::MissingResource(path.to_string()));
            }
            let bytes = bytes_arg(&value).map_err(|e| py_failure(py, e, path))?;
            if bytes.len() > max_bytes {
                return Err(limit("resource bytes", max_bytes));
            }
            Ok(bytes)
        })
    }
    fn list(
        &self,
        directory: Option<&ResourcePath>,
        max_entries: usize,
        max_bytes: usize,
    ) -> Result<Vec<ResourcePath>> {
        Python::attach(|py| {
            let obj = self.0.bind(py);
            if !obj.hasattr("list").unwrap_or(false) {
                return Err(Error::InvalidInput(
                    "resource enumeration is unavailable".into(),
                ));
            }
            let dir = directory.map(ResourcePath::as_str);
            let value = obj
                .call_method1("list", (dir, max_entries, max_bytes))
                .map_err(|e| Error::Io(std::io::Error::other(format!("resolver raised: {e}"))))?;
            let names: Vec<String> = value
                .extract()
                .map_err(|e| Error::InvalidInput(format!("resolver list: {e}")))?;
            if names.len() > max_entries {
                return Err(limit("directory entries", max_entries));
            }
            names.into_iter().map(ResourcePath::new).collect()
        })
    }
}

/// Named in-memory files offered to readers as sidecars (a
/// `ResourceResolver`). Paths are validated `ResourcePath`s (`/`-separated,
/// relative, no `.`/`..`).
#[pyclass(name = "ResourceSet", module = "fontpixel", frozen)]
pub struct PyResourceSet {
    files: Files,
}

#[pymethods]
impl PyResourceSet {
    /// ResourceSet(files=None)
    ///
    /// `files` maps paths to `bytes`/`bytearray`/`memoryview` (copied).
    #[new]
    #[pyo3(signature = (files = None))]
    fn py_new(files: Option<&Bound<'_, PyDict>>) -> PyResult<Self> {
        let mut map = BTreeMap::new();
        if let Some(files) = files {
            for (k, v) in files.iter() {
                map.insert(path_arg(&k.extract::<String>()?)?, bytes_arg(&v)?);
            }
        }
        Ok(Self {
            files: Arc::new(RwLock::new(map)),
        })
    }
    /// Add or replace a file.
    #[pyo3(signature = (path, data))]
    fn insert(&self, path: &str, data: &Bound<'_, PyAny>) -> PyResult<()> {
        let path = path_arg(path)?;
        let data = bytes_arg(data)?;
        self.files
            .write()
            .map_err(|_| poisoned())?
            .insert(path, data);
        Ok(())
    }
    /// Remove a file; returns whether it existed.
    #[pyo3(signature = (path))]
    fn remove(&self, path: &str) -> PyResult<bool> {
        let path = path_arg(path)?;
        Ok(self
            .files
            .write()
            .map_err(|_| poisoned())?
            .remove(&path)
            .is_some())
    }
    /// A copy of one file, or `None`.
    #[pyo3(signature = (path))]
    fn get<'py>(&self, py: Python<'py>, path: &str) -> PyResult<Option<Bound<'py, PyBytes>>> {
        let path = path_arg(path)?;
        let files = self.files.read().map_err(|_| poisoned())?;
        Ok(files.get(&path).map(|b| PyBytes::new(py, b)))
    }
    /// Every path in sorted order.
    fn paths(&self) -> PyResult<Vec<String>> {
        let files = self.files.read().map_err(|_| poisoned())?;
        Ok(files.keys().map(|p| p.as_str().to_owned()).collect())
    }
    /// Resolve like a reader would (`ResourceResolver::resolve`).
    #[pyo3(signature = (path, max_bytes = None))]
    fn resolve<'py>(
        &self,
        py: Python<'py>,
        path: &str,
        max_bytes: Option<usize>,
    ) -> PyResult<Bound<'py, PyBytes>> {
        let path = path_arg(path)?;
        let bytes = MemoryResolver(self.files.clone())
            .resolve(&path, max_bytes.unwrap_or(usize::MAX))
            .py()?;
        Ok(PyBytes::new(py, &bytes))
    }
    /// Immediate files of `directory` (`None`: the root)
    /// (`ResourceResolver::list`).
    #[pyo3(signature = (directory = None, max_entries = None, max_bytes = None))]
    fn list(
        &self,
        directory: Option<&str>,
        max_entries: Option<usize>,
        max_bytes: Option<usize>,
    ) -> PyResult<Vec<String>> {
        let directory = directory.map(path_arg).transpose()?;
        let paths = MemoryResolver(self.files.clone())
            .list(
                directory.as_ref(),
                max_entries.unwrap_or(usize::MAX),
                max_bytes.unwrap_or(usize::MAX),
            )
            .py()?;
        Ok(paths.into_iter().map(|p| p.as_str().to_owned()).collect())
    }
    fn __len__(&self) -> PyResult<usize> {
        Ok(self.files.read().map_err(|_| poisoned())?.len())
    }
    fn __contains__(&self, path: &str) -> PyResult<bool> {
        let Ok(path) = ResourcePath::new(path) else {
            return Ok(false);
        };
        Ok(self
            .files
            .read()
            .map_err(|_| poisoned())?
            .contains_key(&path))
    }
    fn __repr__(&self) -> PyResult<String> {
        Ok(format!("ResourceSet({:?})", self.paths()?))
    }
}

/// Turn a Python resolver argument into a Rust `ResourceResolver`: `None`
/// (`NoResources`), a `ResourceSet`, a `dict[str, bytes]`, an object with
/// `resolve`/`list` methods or a callable `f(path, max_bytes)`.
pub fn resolver_arg(obj: Option<&Bound<'_, PyAny>>) -> PyResult<Arc<dyn ResourceResolver>> {
    let Some(obj) = obj.filter(|o| !o.is_none()) else {
        return Ok(Arc::new(NoResources));
    };
    if let Ok(set) = obj.cast::<PyResourceSet>() {
        return Ok(Arc::new(MemoryResolver(set.get().files.clone())));
    }
    if let Ok(dict) = obj.cast::<PyDict>() {
        let set = PyResourceSet::py_new(Some(dict))?;
        return Ok(Arc::new(MemoryResolver(set.files)));
    }
    if obj.hasattr("resolve")? || obj.is_callable() {
        return Ok(Arc::new(PyResolver(obj.clone().unbind())));
    }
    Err(PyTypeError::new_err(
        "resources must be None, a ResourceSet, a dict of bytes, a resolver object or a callable",
    ))
}

// Input ---------------------------------------------------------------------

/// Owned parts of a `fontpixel::Input`.
#[derive(Clone)]
pub struct InputParts {
    /// Primary bytes.
    pub data: Arc<Vec<u8>>,
    /// Filename hint.
    pub name: Option<String>,
    /// Sidecar resolver.
    pub resolver: Arc<dyn ResourceResolver>,
    /// Directory entry point.
    pub directory: bool,
}

impl InputParts {
    /// Run `f` with the borrowed `Input` these parts describe.
    pub fn with<R>(&self, f: impl FnOnce(&Input<'_>) -> R) -> R {
        let input = if self.directory {
            Input::directory(self.name.as_deref().unwrap_or(""), &*self.resolver)
        } else {
            let mut input = Input::new(&self.data).with_resources(&*self.resolver);
            if let Some(name) = &self.name {
                input = input.with_name(name);
            }
            input
        };
        f(&input)
    }
}

/// Primary bytes, a filename hint and a sidecar resolver
/// (`fontpixel::resources::Input`). Immutable: `with_name` and
/// `with_resources` return new inputs.
#[pyclass(name = "Input", module = "fontpixel", frozen)]
pub struct PyInput {
    /// The owned input description.
    pub parts: InputParts,
}

#[pymethods]
impl PyInput {
    /// Input(data, name=None, resources=None)
    ///
    /// Standalone input (`Input::new`), optionally with a filename hint
    /// (`Input::with_name`) and sidecars (`Input::with_resources`).
    #[new]
    #[pyo3(signature = (data, name = None, resources = None))]
    fn py_new(
        data: &Bound<'_, PyAny>,
        name: Option<String>,
        resources: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        Ok(Self {
            parts: InputParts {
                data: Arc::new(bytes_arg(data)?),
                name,
                resolver: resolver_arg(resources)?,
                directory: false,
            },
        })
    }
    /// A directory entry point without primary bytes; `resources` is rooted
    /// at the directory (`Input::directory`).
    #[staticmethod]
    #[pyo3(signature = (name, resources))]
    fn directory(name: String, resources: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            parts: InputParts {
                data: Arc::new(Vec::new()),
                name: Some(name),
                resolver: resolver_arg(Some(resources))?,
                directory: true,
            },
        })
    }
    /// A copy with a filename hint (`Input::with_name`).
    #[pyo3(signature = (name))]
    fn with_name(&self, name: String) -> Self {
        let mut parts = self.parts.clone();
        parts.name = Some(name);
        Self { parts }
    }
    /// A copy with sidecar resources (`Input::with_resources`).
    #[pyo3(signature = (resources))]
    fn with_resources(&self, resources: &Bound<'_, PyAny>) -> PyResult<Self> {
        let mut parts = self.parts.clone();
        parts.resolver = resolver_arg(Some(resources))?;
        Ok(Self { parts })
    }
    /// The filename hint.
    #[getter]
    fn name(&self) -> Option<String> {
        self.parts.name.clone()
    }
    /// Whether this is a directory entry point.
    #[getter]
    fn is_directory(&self) -> bool {
        self.parts.directory
    }
    /// A copy of the primary bytes.
    #[getter]
    fn data<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.parts.data)
    }
    fn __len__(&self) -> usize {
        self.parts.data.len()
    }
    fn __repr__(&self) -> String {
        format!(
            "Input(bytes={}, name={:?}, directory={})",
            self.parts.data.len(),
            self.parts.name,
            self.parts.directory
        )
    }
}

/// Extract an `Input` handle or plain bytes (standalone input).
pub fn input_arg(obj: &Bound<'_, PyAny>) -> PyResult<InputParts> {
    if let Ok(input) = obj.cast::<PyInput>() {
        return Ok(input.get().parts.clone());
    }
    Ok(InputParts {
        data: Arc::new(bytes_arg(obj)?),
        name: None,
        resolver: Arc::new(NoResources),
        directory: false,
    })
}

// ReadContext ---------------------------------------------------------------

type Reply<T> = Sender<Result<T>>;

enum Request {
    Bytes(Sender<Vec<u8>>),
    Retained(Sender<usize>),
    Resource(ResourcePath, usize, Reply<Vec<u8>>),
    List(Option<ResourcePath>, usize, usize, Reply<Vec<ResourcePath>>),
}

/// Shared resource accounting for one input (`fontpixel::resources::ReadContext`).
///
/// Construction validates the primary input against `limits` and decodes a
/// gzip transport envelope. Each `resource*`/`list_resources_limited` call
/// counts against `max_resources` and the aggregate byte budget, exactly as
/// codecs see it. The context lives on a private worker thread (it borrows
/// its input); calls block until the worker answers, with the GIL released.
#[pyclass(name = "ReadContext", module = "fontpixel", frozen)]
pub struct PyReadContext {
    tx: Mutex<Sender<Request>>,
    name: Option<String>,
    directory: bool,
    limits: ResourceLimits,
}

fn worker_gone() -> PyErr {
    FontpixelError::new_err("read context worker stopped")
}

impl PyReadContext {
    fn ask<T: Send>(&self, py: Python<'_>, make: impl FnOnce(Sender<T>) -> Request) -> PyResult<T> {
        let (reply, rx) = channel();
        self.tx
            .lock()
            .map_err(|_| poisoned())?
            .send(make(reply))
            .map_err(|_| worker_gone())?;
        py.detach(move || rx.recv()).map_err(|_| worker_gone())
    }
}

#[pymethods]
impl PyReadContext {
    /// ReadContext(input, limits=None)
    ///
    /// `input` is an `Input` or plain bytes; `limits` a `ResourceLimits` dict
    /// (`ReadContext::new`).
    #[new]
    #[pyo3(signature = (input, limits = None))]
    fn py_new(
        py: Python<'_>,
        input: &Bound<'_, PyAny>,
        limits: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let parts = input_arg(input)?;
        let limits: ResourceLimits = from_py(limits)?;
        let (tx, rx) = channel::<Request>();
        let (ready, ready_rx) = channel::<Result<()>>();
        let worker_parts = parts.clone();
        let worker_limits = limits.clone();
        std::thread::Builder::new()
            .name("fontpixel-read-context".into())
            .spawn(move || {
                worker_parts.with(|input| {
                    let mut ctx = match ReadContext::new(input, &worker_limits) {
                        Ok(ctx) => {
                            let _ = ready.send(Ok(()));
                            ctx
                        }
                        Err(e) => {
                            let _ = ready.send(Err(e));
                            return;
                        }
                    };
                    while let Ok(request) = rx.recv() {
                        match request {
                            Request::Bytes(reply) => {
                                let _ = reply.send(ctx.bytes().to_vec());
                            }
                            Request::Retained(reply) => {
                                let _ = reply.send(ctx.retained_input_bytes());
                            }
                            Request::Resource(path, max, reply) => {
                                let _ = reply.send(ctx.resource_limited(&path, max));
                            }
                            Request::List(dir, entries, bytes, reply) => {
                                let _ = reply.send(ctx.list_resources_limited(
                                    dir.as_ref(),
                                    entries,
                                    bytes,
                                ));
                            }
                        }
                    }
                });
            })
            .map_err(|e| FontpixelError::new_err(format!("cannot start read context: {e}")))?;
        py.detach(move || ready_rx.recv())
            .map_err(|_| worker_gone())?
            .py()?;
        Ok(Self {
            tx: Mutex::new(tx),
            name: parts.name.clone(),
            directory: parts.directory,
            limits,
        })
    }
    /// Primary bytes after transport decoding (`ReadContext::bytes`).
    fn bytes<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        let data = self.ask(py, Request::Bytes)?;
        Ok(PyBytes::new(py, &data))
    }
    /// Bytes retained so far, including resolved resources
    /// (`ReadContext::retained_input_bytes`).
    fn retained_input_bytes(&self, py: Python<'_>) -> PyResult<usize> {
        self.ask(py, Request::Retained)
    }
    /// The filename hint (`ReadContext::name`).
    #[getter]
    fn name(&self) -> Option<String> {
        self.name.clone()
    }
    /// Whether the input is a directory entry point (`ReadContext::is_directory`).
    #[getter]
    fn is_directory(&self) -> bool {
        self.directory
    }
    /// The budgets in force (`ReadContext::limits`).
    #[getter]
    fn limits<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        to_py(py, &self.limits)
    }
    /// Resolve a sidecar under the remaining budget (`ReadContext::resource`).
    #[pyo3(signature = (path))]
    fn resource<'py>(&self, py: Python<'py>, path: &str) -> PyResult<Bound<'py, PyBytes>> {
        self.resource_limited(py, path, usize::MAX)
    }
    /// Resolve with an extra byte allowance (`ReadContext::resource_limited`).
    #[pyo3(signature = (path, max_bytes))]
    fn resource_limited<'py>(
        &self,
        py: Python<'py>,
        path: &str,
        max_bytes: usize,
    ) -> PyResult<Bound<'py, PyBytes>> {
        let path = path_arg(path)?;
        let data = self
            .ask(py, |reply| Request::Resource(path, max_bytes, reply))?
            .py()?;
        Ok(PyBytes::new(py, &data))
    }
    /// List immediate files of `directory` with shared accounting
    /// (`ReadContext::list_resources_limited`).
    #[pyo3(signature = (directory, max_entries, max_bytes))]
    fn list_resources_limited(
        &self,
        py: Python<'_>,
        directory: Option<&str>,
        max_entries: usize,
        max_bytes: usize,
    ) -> PyResult<Vec<String>> {
        let directory = directory.map(path_arg).transpose()?;
        let paths = self
            .ask(py, |reply| {
                Request::List(directory, max_entries, max_bytes, reply)
            })?
            .py()?;
        Ok(paths.into_iter().map(|p| p.as_str().to_owned()).collect())
    }
    fn __repr__(&self) -> String {
        format!(
            "ReadContext(name={:?}, directory={})",
            self.name, self.directory
        )
    }
}

// outputs -------------------------------------------------------------------

/// One in-memory output file (`fontpixel::resources::OutputAsset`).
#[pyclass(name = "OutputAsset", module = "fontpixel", frozen)]
pub struct PyOutputAsset {
    asset: OutputAsset,
}

#[pymethods]
impl PyOutputAsset {
    /// OutputAsset(path, data)
    #[new]
    #[pyo3(signature = (path, data))]
    fn py_new(path: &str, data: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            asset: OutputAsset {
                path: path_arg(path)?,
                bytes: bytes_arg(data)?,
            },
        })
    }
    /// Validated relative path.
    #[getter]
    fn path(&self) -> &str {
        self.asset.path.as_str()
    }
    /// A copy of the bytes.
    #[getter]
    fn data<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.asset.bytes)
    }
    fn __len__(&self) -> usize {
        self.asset.bytes.len()
    }
    fn __eq__(&self, other: &Bound<'_, PyAny>) -> bool {
        other
            .cast::<PyOutputAsset>()
            .is_ok_and(|o| o.get().asset == self.asset)
    }
    fn __repr__(&self) -> String {
        format!(
            "OutputAsset(path={:?}, bytes={})",
            self.asset.path.as_str(),
            self.asset.bytes.len()
        )
    }
}

fn asset_arg(obj: &Bound<'_, PyAny>) -> PyResult<OutputAsset> {
    if let Ok(a) = obj.cast::<PyOutputAsset>() {
        return Ok(a.get().asset.clone());
    }
    let (path, data): (String, Bound<'_, PyAny>) = obj.extract()?;
    Ok(OutputAsset {
        path: path_arg(&path)?,
        bytes: bytes_arg(&data)?,
    })
}

/// In-memory output files with a primary asset
/// (`fontpixel::resources::OutputBundle`).
#[pyclass(name = "OutputBundle", module = "fontpixel", frozen)]
pub struct PyOutputBundle {
    /// The bundle.
    pub bundle: OutputBundle,
}

impl PyOutputBundle {
    /// Wrap a bundle.
    pub fn new(bundle: OutputBundle) -> Self {
        Self { bundle }
    }
}

#[pymethods]
impl PyOutputBundle {
    /// OutputBundle(primary, assets, directory=False)
    ///
    /// `assets` holds `OutputAsset`s or `(path, bytes)` pairs. A file bundle
    /// must contain `primary` (`OutputBundle::new`); a directory bundle
    /// names its directory (`OutputBundle::directory`).
    #[new]
    #[pyo3(signature = (primary, assets, directory = false))]
    fn py_new(primary: &str, assets: &Bound<'_, PyAny>, directory: bool) -> PyResult<Self> {
        let primary = path_arg(primary)?;
        let assets = assets
            .try_iter()?
            .map(|a| asset_arg(&a?))
            .collect::<PyResult<Vec<_>>>()?;
        let bundle = if directory {
            OutputBundle::directory(primary, assets)
        } else {
            OutputBundle::new(primary, assets)
        };
        Ok(Self::new(bundle.py()?))
    }
    /// A one-file bundle (`OutputBundle::single`).
    #[staticmethod]
    #[pyo3(signature = (path, data))]
    fn single(path: &str, data: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self::new(OutputBundle::single(
            path_arg(path)?,
            bytes_arg(data)?,
        )))
    }
    /// From the plain-data form (`{"primary", "assets": [{"path", "bytes"}],
    /// "directory"}`), validated like the constructors.
    #[staticmethod]
    #[pyo3(signature = (data))]
    fn from_dict(data: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self::new(from_py_required(data)?))
    }
    /// Plain-data form (`bytes` as int lists, the serde shape).
    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        to_py(py, &self.bundle)
    }
    /// Whether the primary path names a directory (`OutputBundle::is_directory`).
    #[getter]
    fn is_directory(&self) -> bool {
        self.bundle.is_directory()
    }
    /// The primary path (`OutputBundle::primary`).
    #[getter]
    fn primary(&self) -> &str {
        self.bundle.primary().as_str()
    }
    /// Copies of every asset in order (`OutputBundle::assets`).
    #[getter]
    fn assets(&self) -> Vec<PyOutputAsset> {
        self.bundle
            .assets()
            .iter()
            .map(|a| PyOutputAsset { asset: a.clone() })
            .collect()
    }
    /// The assets as a list (`OutputBundle::into_assets`; the bundle stays usable).
    #[pyo3(name = "into_assets")]
    fn assets_list(&self) -> Vec<PyOutputAsset> {
        self.assets()
    }
    /// The only file's bytes; `MultipleAssetsError` for several
    /// (`OutputBundle::into_single`).
    #[pyo3(name = "into_single")]
    fn single_bytes<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        let bytes = self.bundle.clone().into_single().py()?;
        Ok(PyBytes::new(py, &bytes))
    }
    /// Retained bytes of the bundle, checked against `max_bytes`
    /// (`OutputBundle::retained_bytes`).
    #[pyo3(signature = (max_bytes = None))]
    fn retained_bytes(&self, max_bytes: Option<usize>) -> PyResult<usize> {
        self.bundle
            .retained_bytes(max_bytes.unwrap_or(usize::MAX))
            .py()
    }
    fn __len__(&self) -> usize {
        self.bundle.assets().len()
    }
    fn __iter__<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, pyo3::types::PyIterator>> {
        PyList::new(py, self.assets())?.as_any().try_iter()
    }
    fn __eq__(&self, other: &Bound<'_, PyAny>) -> bool {
        other
            .cast::<PyOutputBundle>()
            .is_ok_and(|o| o.get().bundle == self.bundle)
    }
    fn __repr__(&self) -> String {
        format!(
            "OutputBundle(primary={:?}, assets={}, directory={})",
            self.bundle.primary().as_str(),
            self.bundle.assets().len(),
            self.bundle.is_directory()
        )
    }
}

/// A writer result: the output bundle and its report
/// (`fontpixel::resources::Written`).
#[pyclass(name = "Written", module = "fontpixel", frozen)]
pub struct PyWritten {
    /// Output files.
    #[pyo3(get)]
    output: Py<PyOutputBundle>,
    /// Report dict.
    #[pyo3(get)]
    report: Py<PyAny>,
}

impl PyWritten {
    /// Wrap a Rust result.
    pub fn new(py: Python<'_>, written: Written) -> PyResult<Self> {
        Ok(Self {
            output: Py::new(py, PyOutputBundle::new(written.output))?,
            report: to_py(py, &written.report)?.unbind(),
        })
    }
}

#[pymethods]
impl PyWritten {
    /// Written(output, report=None)
    #[new]
    #[pyo3(signature = (output, report = None))]
    fn py_new(
        py: Python<'_>,
        output: &Bound<'_, PyOutputBundle>,
        report: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let report: fontpixel::Report = from_py(report)?;
        Self::new(
            py,
            Written {
                output: output.get().bundle.clone(),
                report,
            },
        )
    }
    /// Plain-data form (`{"output": {...}, "report": {...}}`).
    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let report: fontpixel::Report = from_py_required(self.report.bind(py))?;
        to_py(
            py,
            &Written {
                output: self.output.bind(py).get().bundle.clone(),
                report,
            },
        )
    }
    fn __repr__(&self, py: Python<'_>) -> String {
        format!("Written(output={})", self.output.bind(py).get().__repr__())
    }
}

/// Validate a portable relative asset path and return it (`ResourcePath::new`).
#[pyfunction]
#[pyo3(signature = (path))]
fn resource_path(path: &str) -> PyResult<String> {
    Ok(path_arg(path)?.as_str().to_owned())
}

/// Default `ResourceLimits` as a dict.
#[pyfunction]
fn default_resource_limits(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    to_py(py, &ResourceLimits::default())
}

/// Default `TransportLimits` as a dict.
#[pyfunction]
fn default_transport_limits(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    to_py(py, &TransportLimits::default())
}

/// Register the resource classes and functions.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyResourceSet>()?;
    m.add_class::<PyInput>()?;
    m.add_class::<PyReadContext>()?;
    m.add_class::<PyOutputAsset>()?;
    m.add_class::<PyOutputBundle>()?;
    m.add_class::<PyWritten>()?;
    m.add_function(wrap_pyfunction!(resource_path, m)?)?;
    m.add_function(wrap_pyfunction!(default_resource_limits, m)?)?;
    m.add_function(wrap_pyfunction!(default_transport_limits, m)?)?;
    Ok(())
}
