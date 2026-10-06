//! `fontpixel.ops`: every command-line operation as an in-process call.
//!
//! Each operation takes a request dict (the flat `fontpixel_ops` serde shape:
//! keys are the command-line argument ids in snake_case) and/or keyword
//! arguments, plus a storage choice:
//!
//! - `files=None` (default): filesystem mode. Request paths are real paths;
//!   sidecars are discovered, sources are protected from being overwritten and
//!   multi-output groups are written transactionally, exactly as the command
//!   line does. `outputs` of the result is empty.
//! - `files={name: bytes, ...}`: memory mode. Paths are keys into the given
//!   named buffers (sidecars resolve beside their primary key, a key prefix
//!   is a directory); nothing touches the filesystem and every prepared asset
//!   is returned in `outputs`.
//! - `files=<object>` with `entry(path)`, `read(path)` and `list(directory)`
//!   methods: memory mode over a caller-supplied resolver (see
//!   `PyProvider`).
//!
//! Results render the command-line documents byte for byte: `json()` is the
//! `--report json`/`--json` stdout and `text()` the text-mode stdout.
//! Failures raise the `fontpixel` exception classes with an `exit_code`
//! attribute equal to the command-line exit status; requests that cannot be
//! deserialized raise `InvalidInputError` with `exit_code` 2, as a rejected
//! command line does. The GIL is released while an operation runs.
use crate::conv;
use crate::errors;
use crate::handles::PyFontSet;
use crate::resources::{PyOutputBundle, PyResourceSet};
use fontpixel::Error;
use fontpixel_ops::batch::{self, BatchRequest, BatchResponse, BatchTask, TaskAction};
use fontpixel_ops::cli::{Cli, Command};
use fontpixel_ops::{
    CharsetRequest, CharsetResponse, ConvertRequest, ConvertResponse, DiffRequest, DiffResponse,
    EntryKind, FileProvider, FormatsRequest, FormatsResponse, InfoRequest, InfoResponse,
    MemoryFiles, MergeRequest, MergeResponse, PreparedOutput, PreviewRequest, PreviewResponse,
    Progress, RenderRequest, RenderResponse, ReportStyle, SfntResponse, Storage, SubsetRequest,
    TaskKind, TaskRequest, ValidateRequest, ValidateResponse, ValidationKind, VariantsRequest,
};
use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyBytes, PyDict, PyList, PyString, PyTuple};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::ops::ControlFlow;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

// ---------------------------------------------------------------- errors

/// The exception for `error`, with the command-line `exit_code` attached.
fn op_error(error: Error) -> PyErr {
    with_exit_code(error.exit_code(), errors::to_py(error))
}
fn with_exit_code(code: u8, error: PyErr) -> PyErr {
    Python::attach(|py| {
        let _ = error.value(py).setattr("exit_code", code);
    });
    error
}
/// A request that cannot be deserialized: `InvalidInputError`, exit code 2.
fn request_error(message: impl Into<String>) -> PyErr {
    with_exit_code(2, errors::to_py(Error::InvalidInput(message.into())))
}

// ---------------------------------------------------------------- requests

/// Recursively copy dicts and sequences, turning `os.PathLike` into `str`.
fn plain<'py>(object: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    let py = object.py();
    if let Ok(dict) = object.cast::<PyDict>() {
        let out = PyDict::new(py);
        for (key, value) in dict.iter() {
            out.set_item(key, plain(&value)?)?;
        }
        return Ok(out.into_any());
    }
    if object.is_instance_of::<PyList>() || object.is_instance_of::<PyTuple>() {
        let items = object
            .try_iter()?
            .map(|item| item.and_then(|value| plain(&value)))
            .collect::<PyResult<Vec<_>>>()?;
        return Ok(PyList::new(py, items)?.into_any());
    }
    if !object.is_instance_of::<PyString>() && object.hasattr("__fspath__")? {
        return py.import("os")?.call_method1("fspath", (object,));
    }
    Ok(object.clone())
}

/// The request dict merged with keyword options (a key may appear once).
fn request_object<'py>(
    py: Python<'py>,
    request: Option<&Bound<'py, PyAny>>,
    options: Option<&Bound<'py, PyDict>>,
) -> PyResult<Bound<'py, PyAny>> {
    let merged = PyDict::new(py);
    if let Some(request) = request.filter(|r| !r.is_none()) {
        let dict = request
            .cast::<PyDict>()
            .map_err(|_| PyTypeError::new_err("request must be a dict"))?;
        merged.update(dict.as_mapping())?;
    }
    if let Some(options) = options {
        for (key, value) in options.iter() {
            if merged.contains(&key)? {
                return Err(PyTypeError::new_err(format!(
                    "option {key} given both in the request and as a keyword"
                )));
            }
            merged.set_item(key, value)?;
        }
    }
    plain(merged.as_any())
}

/// Strictly deserialize a request (unknown keys and invalid values fail).
fn parse<T: DeserializeOwned>(object: &Bound<'_, PyAny>) -> PyResult<T> {
    pythonize::depythonize(object).map_err(|error| request_error(error.to_string()))
}

fn build<T: DeserializeOwned>(
    py: Python<'_>,
    request: Option<&Bound<'_, PyAny>>,
    options: Option<&Bound<'_, PyDict>>,
) -> PyResult<T> {
    parse(&request_object(py, request, options)?)
}

// ---------------------------------------------------------------- storage

/// A `FileProvider` backed by a Python object with `entry(path)` (returning
/// `None`, a file length `int`, or `"directory"`), `read(path) -> bytes` and
/// `list(directory) -> list[str]`. The first exception raised by a callback
/// is re-raised once the operation returns.
struct PyProvider {
    object: Py<PyAny>,
    error: Mutex<Option<PyErr>>,
}
impl PyProvider {
    fn record(&self, error: PyErr) -> Error {
        let message = error.to_string();
        if let Ok(mut slot) = self.error.lock() {
            slot.get_or_insert(error);
        }
        Error::Io(std::io::Error::other(format!(
            "file provider callback failed: {message}"
        )))
    }
    fn take_error(&self) -> Option<PyErr> {
        self.error.lock().ok().and_then(|mut slot| slot.take())
    }
}
impl FileProvider for PyProvider {
    fn entry(&self, path: &str) -> Option<EntryKind> {
        Python::attach(|py| {
            let result = self.object.bind(py).call_method1("entry", (path,));
            match result {
                Ok(value) if value.is_none() => None,
                Ok(value) => {
                    if let Ok(len) = value.extract::<u64>() {
                        Some(EntryKind::File { len })
                    } else if value.extract::<String>().ok().as_deref() == Some("directory") {
                        Some(EntryKind::Directory)
                    } else {
                        self.record(PyTypeError::new_err(
                            "entry() must return None, a file length or \"directory\"",
                        ));
                        None
                    }
                }
                Err(error) => {
                    self.record(error);
                    None
                }
            }
        })
    }
    fn read(&self, path: &str) -> fontpixel::Result<Vec<u8>> {
        Python::attach(|py| {
            self.object
                .bind(py)
                .call_method1("read", (path,))
                .and_then(|value| crate::bitmap::bytes_arg(&value))
                .map_err(|error| self.record(error))
        })
    }
    fn list(&self, directory: &str) -> fontpixel::Result<Vec<String>> {
        Python::attach(|py| {
            self.object
                .bind(py)
                .call_method1("list", (directory,))
                .and_then(|value| value.extract::<Vec<String>>())
                .map_err(|error| self.record(error))
        })
    }
}

/// The storage for a `files` argument, plus the resolver whose callback
/// errors must be re-raised.
fn storage(files: Option<&Bound<'_, PyAny>>) -> PyResult<(Storage, Option<Arc<PyProvider>>)> {
    let Some(files) = files.filter(|f| !f.is_none()) else {
        return Ok((Storage::Filesystem, None));
    };
    if let Ok(dict) = files.cast::<PyDict>() {
        let py = files.py();
        let os = py.import("os")?;
        let mut set = MemoryFiles::new();
        for (name, value) in dict.iter() {
            let name: String = os.call_method1("fspath", (name,))?.extract()?;
            set.insert(name, crate::bitmap::bytes_arg(&value)?);
        }
        return Ok((Storage::memory(set), None));
    }
    if files.cast::<PyResourceSet>().is_ok() {
        let mut set = MemoryFiles::new();
        for name in files.call_method0("paths")?.extract::<Vec<String>>()? {
            let value = files.call_method1("get", (name.as_str(),))?;
            set.insert(name, crate::bitmap::bytes_arg(&value)?);
        }
        return Ok((Storage::memory(set), None));
    }
    for method in ["entry", "read", "list"] {
        if !files.hasattr(method)? {
            return Err(PyTypeError::new_err(
                "files must be None, a dict of name to bytes, a ResourceSet, or an object with entry/read/list",
            ));
        }
    }
    let provider = Arc::new(PyProvider {
        object: files.clone().unbind(),
        error: Mutex::new(None),
    });
    Ok((Storage::Memory(provider.clone()), Some(provider)))
}

/// Run `f` without the GIL; a resolver exception takes precedence.
fn run<T: Send>(
    py: Python<'_>,
    provider: Option<Arc<PyProvider>>,
    f: impl FnOnce() -> fontpixel::Result<T> + Send,
) -> PyResult<T> {
    let result = py.detach(f);
    if let Some(error) = provider.and_then(|p| p.take_error()) {
        return Err(error);
    }
    result.map_err(op_error)
}

// ---------------------------------------------------------------- results

enum Response {
    Convert(ConvertResponse),
    Merge(Box<MergeResponse>),
    Sfnt(SfntResponse),
    Charset(CharsetResponse),
    Validate(ValidateResponse),
    Diff(DiffResponse),
    Info(InfoResponse),
    Preview(PreviewResponse),
    Render(RenderResponse),
    Formats(FormatsResponse),
}

/// The result of one operation.
///
/// `json()` and `text()` reproduce the command line's standard output for the
/// JSON and text report styles; `document` is the parsed JSON document,
/// `summary` the typed response fields, `report` the operation's diagnostics
/// (or `None` when it has several, see `summary`), and `outputs` maps every
/// prepared asset's storage key to its bytes (memory mode only).
#[pyclass(name = "OperationResult", module = "fontpixel.ops", frozen)]
pub struct PyOperationResult {
    operation: &'static str,
    response: Response,
    outputs: Vec<PreparedOutput>,
    fonts: Option<Py<PyFontSet>>,
}

fn utf8(bytes: Vec<u8>) -> PyResult<String> {
    String::from_utf8(bytes).map_err(|_| request_error("document is not UTF-8"))
}
fn capture(write: impl FnOnce(&mut Vec<u8>) -> fontpixel::Result<()>) -> PyResult<String> {
    let mut out = Vec::new();
    write(&mut out).map_err(op_error)?;
    utf8(out)
}
fn to_value<T: serde::Serialize>(value: &T) -> PyResult<Value> {
    serde_json::to_value(value).map_err(|error| request_error(error.to_string()))
}
fn outputs_dict<'py>(py: Python<'py>, outputs: &[PreparedOutput]) -> PyResult<Bound<'py, PyDict>> {
    let dict = PyDict::new(py);
    for output in outputs {
        for (key, bytes) in output.keyed_assets() {
            dict.set_item(key, PyBytes::new(py, bytes))?;
        }
    }
    Ok(dict)
}
fn bundles_dict<'py>(py: Python<'py>, outputs: &[PreparedOutput]) -> PyResult<Bound<'py, PyDict>> {
    let dict = PyDict::new(py);
    for output in outputs {
        dict.set_item(
            fontpixel_ops::memory::key(&output.destination),
            Py::new(py, PyOutputBundle::new(output.bundle.clone()))?,
        )?;
    }
    Ok(dict)
}
/// Plain data through JSON text (serde_json's `arbitrary_precision` makes a
/// direct `Value` conversion expose its private number representation).
fn value_to_py<'py>(py: Python<'py>, value: &Value) -> PyResult<Bound<'py, PyAny>> {
    loads(py, &value.to_string())
}
fn loads<'py>(py: Python<'py>, text: &str) -> PyResult<Bound<'py, PyAny>> {
    py.import("json")?.call_method1("loads", (text,))
}

impl PyOperationResult {
    fn new(
        py: Python<'_>,
        operation: &'static str,
        response: Response,
        outputs: Vec<PreparedOutput>,
    ) -> PyResult<Self> {
        let fonts = match &response {
            Response::Info(info) => Some(Py::new(py, PyFontSet::new(info.fonts.clone()))?),
            _ => None,
        };
        Ok(Self {
            operation,
            response,
            outputs,
            fonts,
        })
    }
    fn write_json(&self, pretty: bool) -> PyResult<String> {
        capture(|out| match &self.response {
            Response::Convert(r) => r.write_json(out, pretty),
            Response::Merge(r) => r.write_json(out, pretty),
            Response::Sfnt(r) => r.write(ReportStyle::Json, out, pretty),
            Response::Charset(r) => r.write_json(out, pretty),
            Response::Validate(r) => r.write_json(out, pretty),
            Response::Diff(r) => r.write_json(out, pretty),
            Response::Info(r) => r.write_json(out, pretty),
            Response::Preview(r) => r.write_report(ReportStyle::Json, out, pretty),
            Response::Render(r) => r.write_json(out, pretty),
            Response::Formats(r) => r.write_json(out, pretty),
        })
    }
    fn summary_value(&self) -> PyResult<Value> {
        Ok(match &self.response {
            Response::Convert(r) => json!({"each": r.each, "outputs": to_value(&r.outputs)?}),
            Response::Merge(r) => json!({
                "family": r.family,
                "glyphs": r.glyphs,
                "merge": to_value(&r.merge)?,
                "outputs": to_value(&r.outputs)?,
                "formats": r.formats,
                "summaries": r.summaries.iter().map(|s| Ok(json!({
                    "bytes": s.bytes,
                    "assets": to_value(&s.assets)?,
                    "report": to_value(&s.report)?,
                }))).collect::<PyResult<Vec<_>>>()?,
                "reads": to_value(&r.reads)?,
            }),
            Response::Sfnt(r) => json!({
                "output": r.output, "bytes": r.bytes, "report": to_value(&r.report)?,
            }),
            Response::Charset(r) => to_value(r)?,
            Response::Validate(r) => json!({
                "kind": match r.kind {
                    ValidationKind::Native => "native",
                    ValidationKind::Sfnt => "sfnt",
                },
                "format": r.format,
                "faces": to_value(&r.faces)?,
                "report": to_value(&r.report)?,
            }),
            Response::Diff(r) => json!({
                "equal": r.equal(),
                "difference_count": r.difference_count,
                "differences": to_value(&r.differences)?,
                "report_a": to_value(&r.report_a)?,
                "report_b": to_value(&r.report_b)?,
            }),
            Response::Info(r) => json!({
                "format": r.format,
                "font_count": r.fonts.fonts.len(),
                "report": to_value(&r.report)?,
            }),
            Response::Preview(r) => json!({"text": r.text, "report": to_value(&r.report)?}),
            Response::Render(r) => to_value(r)?,
            Response::Formats(r) => json!({"formats": to_value(&r.formats)?}),
        })
    }
}

#[pymethods]
impl PyOperationResult {
    /// Operation name (`"convert"`, `"merge"`, …).
    #[getter]
    fn operation(&self) -> &'static str {
        self.operation
    }
    /// The command-line JSON document (`--report json` / `--json` stdout):
    /// pretty for standalone commands, compact as in batch task results.
    #[pyo3(signature = (pretty = true))]
    fn json(&self, pretty: bool) -> PyResult<String> {
        self.write_json(pretty)
    }
    /// The command line's text-mode standard output.
    fn text(&self) -> PyResult<String> {
        capture(|out| match &self.response {
            Response::Convert(r) => r.write_text(out),
            Response::Merge(r) => r.write_text(out),
            Response::Sfnt(r) => r.write(ReportStyle::Text, out, true),
            Response::Charset(r) => r.write_text(out),
            Response::Validate(r) => r.write_text(out),
            Response::Diff(r) => r.write_text(out),
            Response::Info(r) => r.write_text(out, true),
            Response::Preview(r) => r.write_text(out),
            Response::Render(r) => r.write_text(out),
            Response::Formats(r) => r.write_text(out),
        })
    }
    /// The parsed JSON document (a dict), equal to `json.loads(self.json())`.
    #[getter]
    fn document<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        loads(py, &self.write_json(false)?)
    }
    /// Typed response fields as plain data (per operation; see the module docs).
    #[getter]
    fn summary<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        value_to_py(py, &self.summary_value()?)
    }
    /// The operation's diagnostics report, or `None` for operations with one
    /// report per output or input (convert, merge, diff, formats).
    #[getter]
    fn report<'py>(&self, py: Python<'py>) -> PyResult<Option<Bound<'py, PyAny>>> {
        let report = match &self.response {
            Response::Sfnt(r) => &r.report,
            Response::Charset(r) => &r.report,
            Response::Validate(r) => &r.report,
            Response::Info(r) => &r.report,
            Response::Preview(r) => &r.report,
            Response::Render(r) => &r.report,
            _ => return Ok(None),
        };
        conv::to_py(py, report).map(Some)
    }
    /// Every prepared asset as `{storage key: bytes}` (memory mode; empty
    /// after filesystem mode, where the outputs were already committed).
    #[getter]
    fn outputs<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        outputs_dict(py, &self.outputs)
    }
    /// Every prepared output as `{destination key: fontpixel.OutputBundle}`
    /// (memory mode; the bundle holds the primary asset and its sidecars or
    /// directory members, with paths relative to the destination's parent).
    #[getter]
    fn bundles<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        bundles_dict(py, &self.outputs)
    }
    /// The requested entry-point destination of each prepared output.
    #[getter]
    fn destinations(&self) -> Vec<String> {
        self.outputs
            .iter()
            .map(|o| o.destination.to_string_lossy().into_owned())
            .collect()
    }
    /// Command-line exit status of a completed operation: 0, or 1 for a diff
    /// that found differences.
    #[getter]
    fn exit_code(&self) -> u8 {
        match &self.response {
            Response::Diff(r) => r.status().err().map_or(0, |e| e.exit_code()),
            _ => 0,
        }
    }
    /// Decoded fonts of an `info` call as a `fontpixel.FontSet`, else `None`.
    #[getter]
    fn fonts(&self, py: Python<'_>) -> Option<Py<PyFontSet>> {
        self.fonts.as_ref().map(|f| f.clone_ref(py))
    }
    /// The JSON metadata of one decoded font (`info` only).
    #[pyo3(signature = (index, pretty = true))]
    fn font_json(&self, index: usize, pretty: bool) -> PyResult<String> {
        match &self.response {
            Response::Info(r) => r.font_json(index, pretty).map_err(op_error),
            _ => Err(PyTypeError::new_err("font_json() applies to info results")),
        }
    }
    /// The status dict of one format identifier (`formats` only), or `None`.
    fn format_status<'py>(&self, py: Python<'py>, id: &str) -> PyResult<Option<Bound<'py, PyAny>>> {
        match &self.response {
            Response::Formats(r) => r.get(id).map(|s| conv::to_py(py, s)).transpose(),
            _ => Err(PyTypeError::new_err(
                "format_status() applies to formats results",
            )),
        }
    }
    fn __repr__(&self) -> String {
        format!(
            "<fontpixel.ops.OperationResult {} ({} prepared outputs)>",
            self.operation,
            self.outputs.len()
        )
    }
}

/// The result of a batch: the complete JSON document plus typed summaries.
#[pyclass(name = "BatchResult", module = "fontpixel.ops", frozen)]
pub struct PyBatchResult {
    response: BatchResponse,
    document: String,
}

#[pymethods]
impl PyBatchResult {
    /// Aggregate exit code (precedence 2 > 1 > 3 > 0), as `fontpixel batch`.
    #[getter]
    fn exit_code(&self) -> u8 {
        self.response.exit_code
    }
    /// Requested worker ceiling, capped by the task count.
    #[getter]
    fn jobs(&self) -> usize {
        self.response.jobs
    }
    /// Worker threads actually used.
    #[getter]
    fn workers(&self) -> usize {
        self.response.workers
    }
    /// Per-task outcomes in request order (dicts with `index`, `id`, `kind`,
    /// `success`, `exit_code`, `committed`, `diagnostics_truncated`, `error`).
    #[getter]
    fn tasks<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let tasks: Vec<Value> = self
            .response
            .tasks
            .iter()
            .map(|t| {
                json!({
                    "index": t.index, "id": t.id, "kind": t.kind.name(), "success": t.success,
                    "exit_code": t.exit_code, "committed": t.committed,
                    "diagnostics_truncated": t.diagnostics_truncated, "error": t.error,
                })
            })
            .collect();
        value_to_py(py, &Value::Array(tasks))
    }
    /// The complete batch JSON document exactly as `fontpixel batch` prints it.
    fn json(&self) -> String {
        self.document.clone()
    }
    /// The parsed batch document.
    #[getter]
    fn document<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        loads(py, &self.document)
    }
    /// Committed outputs as `{storage key: bytes}` (memory mode only).
    #[getter]
    fn outputs<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        outputs_dict(py, &self.response.outputs)
    }
    /// Committed outputs as `{destination key: fontpixel.OutputBundle}`
    /// (memory mode only).
    #[getter]
    fn bundles<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        bundles_dict(py, &self.response.outputs)
    }
    fn __repr__(&self) -> String {
        format!(
            "<fontpixel.ops.BatchResult exit_code={} tasks={}>",
            self.response.exit_code,
            self.response.tasks.len()
        )
    }
}

// ---------------------------------------------------------------- operations

macro_rules! writer_op {
    ($(#[$doc:meta])* $name:ident, $request:ty, $call:path, $variant:ident) => {
        $(#[$doc])*
        #[pyfunction]
        #[pyo3(signature = (request = None, /, *, files = None, **options))]
        fn $name(
            py: Python<'_>,
            request: Option<&Bound<'_, PyAny>>,
            files: Option<&Bound<'_, PyAny>>,
            options: Option<&Bound<'_, PyDict>>,
        ) -> PyResult<PyOperationResult> {
            let request: $request = build(py, request, options)?;
            let (storage, provider) = storage(files)?;
            let completed = run(py, provider, move || $call(request, &storage))?;
            PyOperationResult::new(
                py,
                stringify!($name),
                Response::$variant(completed.response.into()),
                completed.outputs,
            )
        }
    };
    // An operation with a `progress=` callback (`$observed` is the
    // `*_with_progress` entry point; without a callback `$call` runs).
    ($(#[$doc:meta])* $name:ident, $request:ty, $call:path, $variant:ident, progress = $observed:path) => {
        $(#[$doc])*
        #[pyfunction]
        #[pyo3(signature = (request = None, /, *, files = None, progress = None, **options))]
        fn $name(
            py: Python<'_>,
            request: Option<&Bound<'_, PyAny>>,
            files: Option<&Bound<'_, PyAny>>,
            progress: Option<&Bound<'_, PyAny>>,
            options: Option<&Bound<'_, PyDict>>,
        ) -> PyResult<PyOperationResult> {
            let request: $request = build(py, request, options)?;
            let (storage, provider) = storage(files)?;
            let completed = match progress.filter(|callback| !callback.is_none()) {
                None => run(py, provider, move || $call(request, &storage))?,
                Some(callback) => observed(py, provider, callback, move |observer| {
                    $observed(request, &storage, observer)
                })?,
            };
            PyOperationResult::new(
                py,
                stringify!($name),
                Response::$variant(completed.response.into()),
                completed.outputs,
            )
        }
    };
}
/// Run `f` without the GIL, passing it an observer that calls the Python
/// `callback` with each `fontpixel_ops::Progress` as a dict
/// (`{"step", "done", "total", "item", "glyphs", "bytes"}`) at the
/// operation's step boundaries, re-acquiring the GIL for each call.
///
/// The callback cancels the operation by returning `False` (exactly; `None`,
/// `True` or any other value continues), which raises `CancelledError`; an
/// exception raised by the callback also cancels it and is re-raised as is.
/// Either way nothing is written or returned. A resolver exception still
/// takes precedence, as in [`run`].
fn observed<T: Send>(
    py: Python<'_>,
    provider: Option<Arc<PyProvider>>,
    callback: &Bound<'_, PyAny>,
    f: impl FnOnce(&mut dyn FnMut(&Progress) -> ControlFlow<()>) -> fontpixel::Result<T> + Send,
) -> PyResult<T> {
    if !callback.is_callable() {
        return Err(PyTypeError::new_err("progress must be a callable or None"));
    }
    let callback = callback.clone().unbind();
    let mut raised: Option<PyErr> = None;
    let result = py.detach(|| {
        let mut observer = |progress: &Progress| {
            Python::attach(|py| {
                let outcome =
                    conv::to_py(py, progress).and_then(|value| callback.bind(py).call1((value,)));
                match outcome {
                    Ok(value) if value.cast::<PyBool>().is_ok_and(|flag| !flag.is_true()) => {
                        ControlFlow::Break(())
                    }
                    Ok(_) => ControlFlow::Continue(()),
                    Err(error) => {
                        raised = Some(error);
                        ControlFlow::Break(())
                    }
                }
            })
        };
        f(&mut observer)
    });
    if let Some(error) = provider.and_then(|p| p.take_error()) {
        return Err(error);
    }
    if let Some(error) = raised {
        return Err(error);
    }
    result.map_err(op_error)
}
macro_rules! reader_op {
    ($(#[$doc:meta])* $name:ident, $request:ty, $call:path, $variant:ident) => {
        $(#[$doc])*
        #[pyfunction]
        #[pyo3(signature = (request = None, /, *, files = None, **options))]
        fn $name(
            py: Python<'_>,
            request: Option<&Bound<'_, PyAny>>,
            files: Option<&Bound<'_, PyAny>>,
            options: Option<&Bound<'_, PyDict>>,
        ) -> PyResult<PyOperationResult> {
            let request: $request = build(py, request, options)?;
            let (storage, provider) = storage(files)?;
            let response = run(py, provider, move || $call(request, &storage))?;
            PyOperationResult::new(py, stringify!($name), Response::$variant(response), Vec::new())
        }
    };
}

writer_op!(
    /// Convert inputs into one output, or each input into its own output
    /// (`fontpixel convert`). Request keys: `inputs`, `output`, `from`, `to`,
    /// `each`, `strict`, `font_index`, `report` and every reader/writer option.
    ///
    /// `progress`, if given, is called with a dict
    /// `{"step", "done", "total", "item", "glyphs", "bytes"}` after each input
    /// is read (`step` `"read"`) and each output is encoded (`"encode"`).
    /// Returning `False` cancels with `CancelledError`; raising cancels and
    /// re-raises; nothing is written either way. Results do not depend on it.
    convert, ConvertRequest, fontpixel_ops::convert, Convert,
    progress = fontpixel_ops::convert_with_progress
);
writer_op!(
    /// Merge selected glyphs from ordered parts and write every output as one
    /// group (`fontpixel merge`). Request keys: `parts` (list of
    /// `{input, take, dy, from, font_index}`), `outputs` (list of
    /// `{destination, format}`), `family` and the merge/writer options.
    ///
    /// `progress`, if given, is called with a dict
    /// `{"step", "done", "total", "item", "glyphs", "bytes"}` after each part
    /// is read (`"read"`), after the merge (`"merge"`) and after each output
    /// is encoded (`"encode"`), before anything is written. Returning `False`
    /// cancels with `CancelledError`; raising cancels and re-raises; nothing
    /// is written either way. Results do not depend on it.
    merge, MergeRequest, fontpixel_ops::merge, Merge,
    progress = fontpixel_ops::merge_with_progress
);
writer_op!(
    /// Build a locl-enabled OpenType font from regional and script faces
    /// (`fontpixel variants`).
    variants, VariantsRequest, fontpixel_ops::variants, Sfnt
);
writer_op!(
    /// Subset an sfnt, WOFF or WOFF2 font (`fontpixel subset`).
    subset, SubsetRequest, fontpixel_ops::subset, Sfnt
);
writer_op!(
    /// Remap glyph codes into another charset and write the result
    /// (`fontpixel charset`).
    charset, CharsetRequest, fontpixel_ops::charset, Charset
);
writer_op!(
    /// Lay out text (or every mapping with `all=True`) into a PNG or SVG image
    /// (`fontpixel render`).
    render, RenderRequest, fontpixel_ops::render, Render
);
reader_op!(
    /// Validate one input (`fontpixel validate`).
    validate, ValidateRequest, fontpixel_ops::validate, Validate
);
reader_op!(
    /// Compare two inputs (`fontpixel diff`). Differences are reported in the
    /// result (`summary["equal"]`, `exit_code` 1), not raised.
    diff, DiffRequest, fontpixel_ops::diff, Diff
);
reader_op!(
    /// Decode one input and describe its fonts (`fontpixel info`); the result's
    /// `fonts` is the decoded `fontpixel.FontSet`.
    info, InfoRequest, fontpixel_ops::info, Info
);
reader_op!(
    /// Lay out text (or every mapping) as character art (`fontpixel preview`):
    /// `text()` is the art, `json()` the diagnostics document.
    preview, PreviewRequest, fontpixel_ops::preview, Preview
);

/// List every catalogue format with the reader/writer compiled into this
/// build (`fontpixel formats`).
#[pyfunction]
#[pyo3(signature = (request = None, /, **options))]
fn formats(
    py: Python<'_>,
    request: Option<&Bound<'_, PyAny>>,
    options: Option<&Bound<'_, PyDict>>,
) -> PyResult<PyOperationResult> {
    let request: FormatsRequest = build(py, request, options)?;
    let response = fontpixel_ops::formats(&request);
    PyOperationResult::new(py, "formats", Response::Formats(response), Vec::new())
}

// ---------------------------------------------------------------- batch

fn working() -> usize {
    batch::DEFAULT_MAX_WORKING_BYTES
}
fn pending() -> usize {
    batch::DEFAULT_MAX_PENDING_BYTES
}
fn reports() -> usize {
    batch::DEFAULT_MAX_REPORT_BYTES
}
fn task_reports() -> usize {
    batch::DEFAULT_MAX_TASK_REPORT_BYTES
}
/// The typed batch request shape accepted by [`batch_run`].
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct BatchSpec {
    tasks: Vec<Value>,
    #[serde(default)]
    jobs: Option<usize>,
    #[serde(default = "working")]
    max_working_bytes: usize,
    #[serde(default = "pending")]
    max_pending_bytes: usize,
    #[serde(default = "reports")]
    max_report_bytes: usize,
    #[serde(default = "task_reports")]
    max_task_report_bytes: usize,
    #[serde(default)]
    protected_paths: Vec<PathBuf>,
}

/// Every path a (possibly invalid) task map names as an input.
fn declared_paths(task: &serde_json::Map<String, Value>) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let mut push = |value: Option<&Value>| {
        if let Some(Value::String(path)) = value {
            paths.push(PathBuf::from(path));
        }
    };
    if let Some(Value::Array(inputs)) = task.get("inputs") {
        inputs.iter().for_each(|v| push(Some(v)));
    }
    for key in ["input", "a", "b", "base", "text_file"] {
        push(task.get(key));
    }
    if let Some(Value::Array(parts)) = task.get("parts") {
        parts.iter().for_each(|p| push(p.get("input")));
    }
    paths
}

/// One typed task: a valid request becomes `BatchTask::new`; a request that
/// fails deserialization becomes an invalid task (exit code 2), as a task
/// whose command line is rejected in a TOML batch.
fn batch_task(value: Value) -> PyResult<BatchTask> {
    let Value::Object(mut map) = value else {
        return Err(request_error("every batch task must be a dict"));
    };
    let id = match map.remove("id") {
        Some(Value::String(id)) => id,
        _ => return Err(request_error("every batch task needs a string id")),
    };
    let kind: TaskKind = match map.get("kind") {
        Some(kind) => serde_json::from_value(kind.clone())
            .map_err(|error| request_error(format!("task {id}: {error}")))?,
        None => return Err(request_error(format!("task {id} needs a kind"))),
    };
    let declared_inputs = declared_paths(&map);
    Ok(
        match serde_json::from_value::<TaskRequest>(Value::Object(map)) {
            Ok(request) => BatchTask::new(id, request),
            Err(error) => BatchTask {
                id,
                kind,
                declared_inputs,
                action: TaskAction::Invalid(error.to_string()),
            },
        },
    )
}

/// A whole-batch failure: the exception carries `document`, the failure JSON
/// document `fontpixel batch` prints, and `exit_code`.
fn batch_failure(error: Error) -> PyErr {
    let mut document = Vec::new();
    let code = batch::write_failure(&mut document, &error);
    let exception = with_exit_code(code, errors::to_py(error));
    Python::attach(|py| {
        let _ = exception
            .value(py)
            .setattr("document", String::from_utf8_lossy(&document).into_owned());
    });
    exception
}

fn batch_result(
    py: Python<'_>,
    provider: Option<Arc<PyProvider>>,
    f: impl FnOnce(&mut Vec<u8>) -> fontpixel::Result<BatchResponse> + Send,
) -> PyResult<PyBatchResult> {
    let (result, document) = py.detach(|| {
        let mut document = Vec::new();
        let result = f(&mut document);
        (result, document)
    });
    if let Some(error) = provider.and_then(|p| p.take_error()) {
        return Err(error);
    }
    let response = result.map_err(batch_failure)?;
    Ok(PyBatchResult {
        response,
        document: utf8(document)?,
    })
}

/// Run typed tasks as one batch (`fontpixel batch` with typed tasks).
///
/// Request keys: `tasks` (list of request dicts, each with `id` and a `kind`
/// of `convert`, `vectorize`, `variants`, `subset`, `merge`, `charset`,
/// `validate` or `diff`), `jobs`, `max_working_bytes`, `max_pending_bytes`,
/// `max_report_bytes`, `max_task_report_bytes` and `protected_paths`. A task
/// whose request is invalid fails alone with exit code 2. Every successful
/// writer joins one output transaction (filesystem) or the returned outputs
/// (memory). A whole-batch failure raises with a `document` attribute.
#[pyfunction(name = "batch")]
#[pyo3(signature = (request = None, /, *, files = None, **options))]
fn batch_run(
    py: Python<'_>,
    request: Option<&Bound<'_, PyAny>>,
    files: Option<&Bound<'_, PyAny>>,
    options: Option<&Bound<'_, PyDict>>,
) -> PyResult<PyBatchResult> {
    let spec: BatchSpec = build(py, request, options)?;
    let tasks = spec
        .tasks
        .into_iter()
        .map(batch_task)
        .collect::<PyResult<Vec<_>>>()?;
    let request = BatchRequest {
        jobs: spec.jobs,
        max_working_bytes: spec.max_working_bytes,
        max_pending_bytes: spec.max_pending_bytes,
        max_report_bytes: spec.max_report_bytes,
        max_task_report_bytes: spec.max_task_report_bytes,
        protected_paths: spec.protected_paths,
        tasks,
    };
    let (storage, provider) = storage(files)?;
    batch_result(py, provider, move |out| batch::run(request, &storage, out))
}

/// Run a version-1 TOML job text (the `fontpixel batch` file format).
/// Relative paths resolve against `base`; `jobs` overrides the file's
/// worker ceiling.
#[pyfunction]
#[pyo3(signature = (text, /, *, base = None, jobs = None, files = None))]
fn batch_toml(
    py: Python<'_>,
    text: &str,
    base: Option<PathBuf>,
    jobs: Option<usize>,
    files: Option<&Bound<'_, PyAny>>,
) -> PyResult<PyBatchResult> {
    let (storage, provider) = storage(files)?;
    let base = base.unwrap_or_default();
    let text = text.to_owned();
    batch_result(py, provider, move |out| {
        batch::toml::run_text(&text, &base, jobs, &storage, out)
    })
}

/// Parse a TOML job text without running it: the tasks as dicts with `id`,
/// `kind`, `declared_inputs` and either `request` (the normalized request)
/// and `weight`, or `error` for a task that failed preparation.
#[pyfunction]
#[pyo3(signature = (text, /, *, base = None))]
fn parse_batch_toml<'py>(
    py: Python<'py>,
    text: &str,
    base: Option<PathBuf>,
) -> PyResult<Bound<'py, PyAny>> {
    let request = batch::toml::parse(text, &base.unwrap_or_default()).map_err(op_error)?;
    let tasks = request
        .tasks
        .iter()
        .map(|task| {
            let mut value = json!({
                "id": task.id,
                "kind": task.kind.name(),
                "declared_inputs": task.declared_inputs,
            });
            match &task.action {
                TaskAction::Run { request, weight } => {
                    value["request"] = to_value(request)?;
                    value["weight"] = json!(weight);
                }
                TaskAction::Invalid(error) => value["error"] = json!(error),
            }
            Ok(value)
        })
        .collect::<PyResult<Vec<_>>>()?;
    value_to_py(
        py,
        &json!({
            "jobs": request.jobs,
            "max_working_bytes": request.max_working_bytes,
            "max_pending_bytes": request.max_pending_bytes,
            "max_report_bytes": request.max_report_bytes,
            "max_task_report_bytes": request.max_task_report_bytes,
            "tasks": tasks,
        }),
    )
}

/// Run a TOML job file from the filesystem exactly as `fontpixel batch FILE
/// [--jobs N]` (the job file is protected from every output; `jobs` is
/// 1..=64).
#[pyfunction]
#[pyo3(signature = (path, /, *, jobs = None))]
fn batch_file(py: Python<'_>, path: PathBuf, jobs: Option<u32>) -> PyResult<PyBatchResult> {
    if let Some(jobs) = jobs
        && !(1..=64).contains(&jobs)
    {
        return Err(request_error(format!(
            "invalid value '{jobs}' for '--jobs <JOBS>': {jobs} is not in 1..=64"
        )));
    }
    batch_result(py, None, move |out| batch::toml::run_path(&path, jobs, out))
}

// ---------------------------------------------------------------- queries

/// Platform capabilities of this build: `filesystem`, `threads` and
/// `toml_batch` (bools).
#[pyfunction]
fn capabilities(py: Python<'_>) -> PyResult<Bound<'_, PyDict>> {
    let caps = fontpixel_ops::capabilities();
    let dict = PyDict::new(py);
    dict.set_item("filesystem", caps.filesystem)?;
    dict.set_item("threads", caps.threads)?;
    dict.set_item("toml_batch", caps.toml_batch)?;
    Ok(dict)
}

/// The memory-storage key of a path: `.` removed, `..` folded, `/` separators.
#[pyfunction]
fn memory_key(path: PathBuf) -> String {
    fontpixel_ops::memory::key(&path)
}

/// The output format `convert` infers for a destination path (OpenType
/// container extensions first, then the registry's filename hints); an
/// ambiguous or unknown extension raises like `convert` without `to`.
#[pyfunction]
fn output_format_for(destination: PathBuf) -> PyResult<String> {
    fontpixel_ops::convert::output_format_for(&destination).map_err(op_error)
}

fn command_name(command: &Command) -> &'static str {
    match command {
        Command::Batch(_) => "batch",
        Command::Merge(_) => "merge",
        Command::Diff(_) => "diff",
        Command::Validate(_) => "validate",
        Command::Charset(_) => "charset",
        Command::Variants(_) => "variants",
        Command::Subset(_) => "subset",
        Command::Formats(_) => "formats",
        Command::Info(_) => "info",
        Command::Preview(_) => "preview",
        Command::Render(_) => "render",
        Command::Convert(_) => "convert",
    }
}
fn command_request(command: &Command) -> PyResult<Value> {
    match command {
        Command::Batch(args) => Ok(json!({"file": args.file, "jobs": args.jobs})),
        Command::Merge(r) => to_value(r),
        Command::Diff(r) => to_value(r),
        Command::Validate(r) => to_value(r),
        Command::Charset(r) => to_value(r),
        Command::Variants(r) => to_value(r),
        Command::Subset(r) => to_value(r),
        Command::Formats(r) => to_value(r),
        Command::Info(r) => to_value(r),
        Command::Preview(r) => to_value(r),
        Command::Render(r) => to_value(r),
        Command::Convert(r) => to_value(r),
    }
}
fn style_name(style: ReportStyle) -> &'static str {
    match style {
        ReportStyle::Text => "text",
        ReportStyle::Json => "json",
    }
}
fn describe<'py>(
    py: Python<'py>,
    name: &str,
    command: &Command,
    task: Option<&TaskRequest>,
) -> PyResult<Bound<'py, PyAny>> {
    let mut value = json!({
        "command": name,
        "request": command_request(command)?,
        "report_style": style_name(command.report_style()),
        "source_identity_total_bytes": command.source_identity_total_bytes(),
    });
    if let Some(task) = task {
        value["declared_inputs"] = json!(task.declared_inputs());
        value["weight"] = json!(task.weight());
    }
    value_to_py(py, &value)
}

/// Parse a `fontpixel` command line (without the program name) with the
/// command-line definition and return `{"command", "request", "report_style",
/// "source_identity_total_bytes"}`; `request` can be passed to the matching
/// operation. A rejected command line raises `InvalidInputError` with the
/// parser's message and `exit_code` (0 for `--help`/`--version`).
#[pyfunction]
fn parse_args<'py>(py: Python<'py>, argv: Vec<String>) -> PyResult<Bound<'py, PyAny>> {
    let args = std::iter::once("fontpixel".to_owned()).chain(argv);
    let command = <Cli as clap::Parser>::try_parse_from(args)
        .map_err(|error| {
            with_exit_code(
                error.exit_code().clamp(0, 255) as u8,
                errors::to_py(Error::InvalidInput(error.to_string())),
            )
        })?
        .command;
    let name = command_name(&command);
    describe(py, name, &command, None)
}

/// Validate a request for `operation` and describe it: the normalized
/// `request` (every default filled in), `report_style`,
/// `source_identity_total_bytes` and, for batch task kinds, `declared_inputs`
/// and `weight`. Raises `InvalidInputError` (exit code 2) exactly when the
/// operation would reject the request before running.
#[pyfunction]
#[pyo3(signature = (operation, request = None, /, **options))]
fn inspect_request<'py>(
    py: Python<'py>,
    operation: &str,
    request: Option<&Bound<'py, PyAny>>,
    options: Option<&Bound<'py, PyDict>>,
) -> PyResult<Bound<'py, PyAny>> {
    let object = request_object(py, request, options)?;
    let (command, task) = match operation {
        "convert" | "vectorize" => {
            let r: ConvertRequest = parse(&object)?;
            let task = if operation == "convert" {
                TaskRequest::Convert(Box::new(r.clone()))
            } else {
                TaskRequest::Vectorize(Box::new(r.clone()))
            };
            (Command::Convert(Box::new(r)), Some(task))
        }
        "merge" => {
            let r: MergeRequest = parse(&object)?;
            (
                Command::Merge(Box::new(r.clone())),
                Some(TaskRequest::Merge(Box::new(r))),
            )
        }
        "variants" => {
            let r: VariantsRequest = parse(&object)?;
            (
                Command::Variants(Box::new(r.clone())),
                Some(TaskRequest::Variants(Box::new(r))),
            )
        }
        "subset" => {
            let r: SubsetRequest = parse(&object)?;
            (
                Command::Subset(Box::new(r.clone())),
                Some(TaskRequest::Subset(Box::new(r))),
            )
        }
        "charset" => {
            let r: CharsetRequest = parse(&object)?;
            (
                Command::Charset(Box::new(r.clone())),
                Some(TaskRequest::Charset(Box::new(r))),
            )
        }
        "validate" => {
            let r: ValidateRequest = parse(&object)?;
            (
                Command::Validate(Box::new(r.clone())),
                Some(TaskRequest::Validate(Box::new(r))),
            )
        }
        "diff" => {
            let r: DiffRequest = parse(&object)?;
            (
                Command::Diff(Box::new(r.clone())),
                Some(TaskRequest::Diff(Box::new(r))),
            )
        }
        "info" => (
            Command::Info(Box::new(parse::<InfoRequest>(&object)?)),
            None,
        ),
        "preview" => (
            Command::Preview(Box::new(parse::<PreviewRequest>(&object)?)),
            None,
        ),
        "render" => (
            Command::Render(Box::new(parse::<RenderRequest>(&object)?)),
            None,
        ),
        "formats" => (Command::Formats(parse::<FormatsRequest>(&object)?), None),
        other => return Err(request_error(format!("unknown operation {other:?}"))),
    };
    describe(py, operation, &command, task.as_ref())
}

/// Register `fontpixel._native._ops` (re-exported as `fontpixel.ops`).
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = m.py();
    let ops = PyModule::new(py, "_ops")?;
    ops.add(
        "__doc__",
        "Native backend of fontpixel.ops: every command-line operation in-process.",
    )?;
    for function in [
        wrap_pyfunction!(convert, &ops)?,
        wrap_pyfunction!(merge, &ops)?,
        wrap_pyfunction!(variants, &ops)?,
        wrap_pyfunction!(subset, &ops)?,
        wrap_pyfunction!(charset, &ops)?,
        wrap_pyfunction!(render, &ops)?,
        wrap_pyfunction!(validate, &ops)?,
        wrap_pyfunction!(diff, &ops)?,
        wrap_pyfunction!(info, &ops)?,
        wrap_pyfunction!(preview, &ops)?,
        wrap_pyfunction!(formats, &ops)?,
        wrap_pyfunction!(batch_run, &ops)?,
        wrap_pyfunction!(batch_toml, &ops)?,
        wrap_pyfunction!(batch_file, &ops)?,
        wrap_pyfunction!(parse_batch_toml, &ops)?,
        wrap_pyfunction!(capabilities, &ops)?,
        wrap_pyfunction!(memory_key, &ops)?,
        wrap_pyfunction!(output_format_for, &ops)?,
        wrap_pyfunction!(parse_args, &ops)?,
        wrap_pyfunction!(inspect_request, &ops)?,
    ] {
        ops.add_function(function)?;
    }
    ops.add_class::<PyOperationResult>()?;
    ops.add_class::<PyBatchResult>()?;
    ops.add(
        "DEFAULT_MAX_WORKING_BYTES",
        batch::DEFAULT_MAX_WORKING_BYTES,
    )?;
    ops.add(
        "DEFAULT_MAX_PENDING_BYTES",
        batch::DEFAULT_MAX_PENDING_BYTES,
    )?;
    ops.add("DEFAULT_MAX_REPORT_BYTES", batch::DEFAULT_MAX_REPORT_BYTES)?;
    ops.add(
        "DEFAULT_MAX_TASK_REPORT_BYTES",
        batch::DEFAULT_MAX_TASK_REPORT_BYTES,
    )?;
    m.add_submodule(&ops)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn declared_paths_cover_every_input_key() {
        let map = json!({
            "inputs": ["a", "b"], "input": "c", "a": "d", "b": "e", "base": "f",
            "text_file": "g", "parts": [{"input": "h"}]
        });
        let Value::Object(map) = map else { panic!() };
        let paths: Vec<String> = declared_paths(&map)
            .into_iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        assert_eq!(paths, ["a", "b", "c", "d", "e", "f", "g", "h"]);
        assert!(Path::new(&paths[0]).is_relative());
    }
}
