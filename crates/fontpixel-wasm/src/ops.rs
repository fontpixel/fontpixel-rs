//! `ops`: every command-line operation as an in-process call
//! (`fontpixel_ops`, the JS counterpart of Python's `fontpixel.ops`).
//!
//! The functions live in the JS namespace object `ops` (`ops.convert`,
//! `ops.merge`, …). Each operation takes a request object in the flat
//! `fontpixel_ops` plain-data shape (keys are the command-line argument ids
//! in snake_case: `inputs`, `output`, `to`, `max_total_bytes`, …) and an
//! options object `{files, progress}`:
//!
//! - `files` is required (memory storage): an object or `Map` of storage key
//!   → bytes (`Uint8Array`/`ArrayBuffer`/`DataView`), or a provider object
//!   `{entry(path), read(path), list(directory)}` called synchronously while
//!   the operation runs (`entry` returns `null`/`undefined`, a file length or
//!   `"directory"`). Request paths are keys; sidecars resolve beside their
//!   primary key and a key prefix acts as a directory. Every prepared asset
//!   is returned in `result.outputs`; nothing is written anywhere.
//!   Filesystem storage (real paths, sidecar discovery, source-alias
//!   protection, transactional writes) does not exist in WebAssembly: an
//!   omitted `files` throws `UnsupportedError` (capability `filesystem`).
//! - `progress` (convert, vectorize, merge): called with `Progress` plain
//!   data at each step; returning exactly `false` cancels with
//!   `CancelledError`, a throw cancels and is rethrown.
//!
//! Results render the command-line documents byte for byte: `json()` is the
//! `--report json`/`--json` stdout and `text()` the text stdout. Failures
//! throw the `fontpixel` error classes with `exitCode` equal to the
//! command-line exit status; requests that cannot be deserialized throw
//! `InvalidInputError` with `exitCode` 2, as a rejected command line does.
//! An exception thrown by a `files` provider takes precedence and is
//! rethrown unchanged, then one thrown by `progress`.
use crate::callbacks::{Callback, CallbackScope, ProgressCallback};
use crate::capabilities;
use crate::conv::{self, Options};
use crate::errors::{self, JsResult};
use crate::handles::{JsFontSet, Shared, shared};
use fontpixel::Error;
use fontpixel_ops::batch::{self, BatchRequest, BatchResponse, BatchTask, TaskAction};
use fontpixel_ops::{
    CharsetRequest, CharsetResponse, ConvertRequest, ConvertResponse, DiffRequest, DiffResponse,
    EntryKind, FileProvider, FormatsRequest, FormatsResponse, InfoRequest, InfoResponse,
    MemoryFiles, MergeRequest, MergeResponse, PreparedOutput, PreviewRequest, PreviewResponse,
    RenderRequest, RenderResponse, ReportStyle, Storage, TaskKind, TaskRequest, ValidateRequest,
    ValidateResponse, ValidationKind,
};
#[cfg(any(
    feature = "all-formats",
    feature = "merge",
    feature = "convert",
    feature = "web"
))]
use fontpixel_ops::{SfntResponse, SubsetRequest, VariantsRequest};
use js_sys::{Array, Function, Map, Object, Reflect};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::path::PathBuf;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

// ---------------------------------------------------------------- requests

/// Set `exitCode` 2 on a request-building failure (the command line's usage
/// error status), keeping the error's class and message.
pub(crate) fn as_request_error(error: JsValue) -> JsValue {
    if error.is_object() {
        let _ = Reflect::set(&error, &JsValue::from_str("exitCode"), &JsValue::from(2));
    }
    error
}

/// Strictly deserialize a request object (`undefined`/`null` is `{}`):
/// unknown keys, invalid values and violated command-line rules throw
/// `InvalidInputError` with `exitCode` 2 and the parser's message.
pub(crate) fn request<T: DeserializeOwned>(value: &JsValue) -> JsResult<T> {
    let value = if value.is_undefined() || value.is_null() {
        Object::new().into()
    } else {
        value.clone()
    };
    conv::from_js(&value).map_err(as_request_error)
}

/// A request as a `serde_json::Value` (exact numbers), for requests the
/// binding completes before deserializing them.
pub(crate) fn request_value(value: &JsValue) -> JsResult<Value> {
    request::<Value>(value)
}

/// `T` from a `serde_json::Value` (request rules apply; exit code 2).
pub(crate) fn request_from_value<T: DeserializeOwned>(value: Value) -> JsResult<T> {
    serde_json::from_value(value).map_err(|e| errors::request_error(conv::message(&e)))
}

// ---------------------------------------------------------------- storage

/// A `FileProvider` backed by a JS object with `entry(path)`, `read(path)`
/// and `list(directory)`. The first exception a method throws is recorded
/// and rethrown unchanged once the operation returns.
struct JsProvider(Callback);

fn provider_failed(what: &str) -> Error {
    Error::Io(std::io::Error::other(format!(
        "file provider callback failed: {what}"
    )))
}

impl JsProvider {
    fn call(&self, method: &str, arg: &str) -> Result<JsValue, ()> {
        let object = self.0.object().ok_or(())?;
        let function = Reflect::get(&object, &JsValue::from_str(method))
            .ok()
            .and_then(|f| f.dyn_into::<Function>().ok())
            .ok_or(())?;
        function
            .call1(&object, &JsValue::from_str(arg))
            .map_err(|error| self.0.record(error))
    }
}

impl FileProvider for JsProvider {
    fn entry(&self, path: &str) -> Option<EntryKind> {
        let value = self.call("entry", path).ok()?;
        if value.is_undefined() || value.is_null() {
            return None;
        }
        if value.as_string().as_deref() == Some("directory") {
            return Some(EntryKind::Directory);
        }
        match conv::int_from_js::<u64>(&value, "entry() result") {
            Ok(len) => Some(EntryKind::File { len }),
            Err(_) => {
                self.0.record(errors::invalid(
                    "entry() must return null, a file length or \"directory\"",
                ));
                None
            }
        }
    }
    fn read(&self, path: &str) -> fontpixel::Result<Vec<u8>> {
        let value = self
            .call("read", path)
            .map_err(|()| provider_failed("read"))?;
        conv::bytes_from_js(&value, "read() result").map_err(|error| {
            self.0.record(error);
            provider_failed("read")
        })
    }
    fn list(&self, directory: &str) -> fontpixel::Result<Vec<String>> {
        let value = self
            .call("list", directory)
            .map_err(|()| provider_failed("list"))?;
        let names = value.dyn_ref::<Array>().and_then(|names| {
            names
                .iter()
                .map(|name| name.as_string())
                .collect::<Option<Vec<_>>>()
        });
        names.ok_or_else(|| {
            self.0
                .record(errors::invalid("list() must return an array of strings"));
            provider_failed("list")
        })
    }
}

/// The storage of one call (`files`), plus the provider scope whose
/// exceptions are rethrown.
pub(crate) struct Files {
    pub(crate) storage: Storage,
    scope: Option<CallbackScope>,
}

impl Files {
    /// Memory storage over named buffers.
    pub(crate) fn memory(files: MemoryFiles) -> Self {
        Self {
            storage: Storage::memory(files),
            scope: None,
        }
    }

    /// Convert a `files` argument; `undefined`/`null` (filesystem mode)
    /// throws `UnsupportedError` naming `entry_point`.
    pub(crate) fn new(value: &JsValue, entry_point: &str) -> JsResult<Self> {
        if value.is_undefined() || value.is_null() {
            return Err(capabilities::unsupported(
                "filesystem",
                &format!("{entry_point} without `files` (filesystem storage)"),
            ));
        }
        let mut set = MemoryFiles::new();
        if let Some(map) = value.dyn_ref::<Map>() {
            let mut failure = None;
            map.for_each(&mut |bytes, key| {
                if failure.is_none()
                    && let Err(error) = insert(&mut set, &key, &bytes)
                {
                    failure = Some(error);
                }
            });
            return match failure {
                Some(error) => Err(error),
                None => Ok(Self::memory(set)),
            };
        }
        let Some(object) = value.dyn_ref::<Object>().filter(|_| !value.is_function()) else {
            return Err(files_type_error());
        };
        let methods = ["entry", "read", "list"]
            .iter()
            .map(|m| Reflect::get(object, &JsValue::from_str(m)).map(|f| f.is_function()))
            .collect::<Result<Vec<_>, _>>()?;
        if methods.iter().all(|&m| m) {
            let scope = CallbackScope::new(value.clone());
            let provider = JsProvider(scope.token());
            return Ok(Self {
                storage: Storage::memory(provider),
                scope: Some(scope),
            });
        }
        if methods.iter().any(|&m| m) {
            return Err(files_type_error());
        }
        for entry in Object::entries(object).iter() {
            let entry: Array = entry.unchecked_into();
            insert(&mut set, &entry.get(0), &entry.get(1))?;
        }
        Ok(Self::memory(set))
    }

    /// `result` mapped to JS, unless the provider threw: then its exception.
    pub(crate) fn finish<T>(&self, result: JsResult<T>) -> JsResult<T> {
        match &self.scope {
            Some(scope) => scope.rethrow(result),
            None => result,
        }
    }
}

fn files_type_error() -> JsValue {
    errors::invalid(
        "files must be an object or Map of name to bytes, or an object with entry/read/list methods",
    )
}

fn insert(set: &mut MemoryFiles, key: &JsValue, bytes: &JsValue) -> JsResult<()> {
    let name = key
        .as_string()
        .ok_or_else(|| errors::invalid("files: names must be strings"))?;
    let data = conv::bytes_from_js(bytes, &format!("files[{name:?}]"))?;
    set.insert(name, data);
    Ok(())
}

/// The trailing `{files, progress}` options of an operation.
struct Call {
    files: Files,
    progress: Option<ProgressCallback>,
}

impl Call {
    fn new(options: Option<JsValue>, name: &'static str, progress: bool) -> JsResult<Self> {
        let allowed: &[&str] = if progress {
            &["files", "progress"]
        } else {
            &["files"]
        };
        let options = Options::new(&options.unwrap_or(JsValue::UNDEFINED), allowed, name)?;
        let entry = name.trim_end_matches(" options");
        let files = Files::new(
            &options.get("files")?.unwrap_or(JsValue::UNDEFINED),
            &format!("ops.{entry}()"),
        )?;
        let progress = match options.get("progress")? {
            Some(callback) => ProgressCallback::new(&callback)?,
            None => None,
        };
        Ok(Self { files, progress })
    }

    /// Provider exception, then progress exception, then the operation's
    /// own error (mapped with its command-line exit code).
    fn finish<T>(&self, result: fontpixel::Result<T>) -> JsResult<T> {
        let result = result.map_err(errors::to_js);
        let result = match &self.progress {
            Some(progress) => progress.finish(result),
            None => result,
        };
        self.files.finish(result)
    }
}

// ---------------------------------------------------------------- results

enum Response {
    Convert(ConvertResponse),
    Merge(Box<MergeResponse>),
    #[cfg(any(
        feature = "all-formats",
        feature = "merge",
        feature = "convert",
        feature = "web"
    ))]
    Sfnt(SfntResponse),
    Charset(CharsetResponse),
    Validate(ValidateResponse),
    Diff(DiffResponse),
    Info(Box<InfoResponse>),
    Preview(PreviewResponse),
    Render(RenderResponse),
    Formats(FormatsResponse),
}

fn utf8(bytes: Vec<u8>) -> JsResult<String> {
    String::from_utf8(bytes).map_err(|_| errors::invalid("document is not UTF-8"))
}

fn capture(write: impl FnOnce(&mut Vec<u8>) -> fontpixel::Result<()>) -> JsResult<String> {
    let mut out = Vec::new();
    write(&mut out).map_err(errors::to_js)?;
    utf8(out)
}

fn to_value<T: serde::Serialize>(value: &T) -> JsResult<Value> {
    serde_json::to_value(value).map_err(|e| errors::invalid(conv::message(&e)))
}

/// Parse a JSON document written by the operations into a JS value.
pub(crate) fn document_to_js(text: &str) -> JsResult<JsValue> {
    let value: Value =
        serde_json::from_str(text).map_err(|e| errors::invalid(conv::message(&e)))?;
    conv::to_js(&value)
}

/// `{storage key: Uint8Array}` of every prepared asset.
pub(crate) fn outputs_object(outputs: &[PreparedOutput]) -> JsResult<Object> {
    let object = Object::new();
    for output in outputs {
        for (key, bytes) in output.keyed_assets() {
            Reflect::set(
                &object,
                &JsValue::from_str(&key),
                &conv::bytes_to_js(bytes).into(),
            )?;
        }
    }
    Ok(object)
}

fn bundles_object(outputs: &[PreparedOutput]) -> JsResult<Object> {
    let object = Object::new();
    for output in outputs {
        Reflect::set(
            &object,
            &JsValue::from_str(&fontpixel_ops::memory::key(&output.destination)),
            &conv::to_js(&output.bundle)?,
        )?;
    }
    Ok(object)
}

/// The result of one operation (`fontpixel_ops::Completed` plus the
/// response). `json()`/`text()` reproduce the command line's standard
/// output; `document` is the parsed JSON document, `summary` the typed
/// response fields, `report` the diagnostics (or `null` for operations with
/// one report per output or input), `outputs` every prepared asset as
/// `{storage key: Uint8Array}`.
#[wasm_bindgen(js_name = OperationResult)]
pub struct JsOperationResult {
    operation: &'static str,
    response: Response,
    outputs: Vec<PreparedOutput>,
    fonts: Option<Shared<fontpixel::FontSet>>,
}

impl JsOperationResult {
    fn new(operation: &'static str, response: Response, outputs: Vec<PreparedOutput>) -> Self {
        let fonts = match &response {
            Response::Info(info) => Some(shared(info.fonts.clone())),
            _ => None,
        };
        Self {
            operation,
            response,
            outputs,
            fonts,
        }
    }

    fn write_json(&self, pretty: bool) -> JsResult<String> {
        capture(|out| match &self.response {
            Response::Convert(r) => r.write_json(out, pretty),
            Response::Merge(r) => r.write_json(out, pretty),
            #[cfg(any(
                feature = "all-formats",
                feature = "merge",
                feature = "convert",
                feature = "web"
            ))]
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

    fn summary_value(&self) -> JsResult<Value> {
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
                }))).collect::<JsResult<Vec<_>>>()?,
                "reads": to_value(&r.reads)?,
            }),
            #[cfg(any(
                feature = "all-formats",
                feature = "merge",
                feature = "convert",
                feature = "web"
            ))]
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

#[wasm_bindgen(js_class = OperationResult)]
impl JsOperationResult {
    /// Operation name (`"convert"`, `"merge"`, …).
    #[wasm_bindgen(getter)]
    pub fn operation(&self) -> String {
        self.operation.to_owned()
    }
    /// The command-line JSON document (`--report json` / `--json` stdout):
    /// pretty (the default) for standalone commands, compact as in batch
    /// task results.
    pub fn json(&self, pretty: Option<bool>) -> JsResult<String> {
        self.write_json(pretty.unwrap_or(true))
    }
    /// The command line's text-mode standard output.
    pub fn text(&self) -> JsResult<String> {
        capture(|out| match &self.response {
            Response::Convert(r) => r.write_text(out),
            Response::Merge(r) => r.write_text(out),
            #[cfg(any(
                feature = "all-formats",
                feature = "merge",
                feature = "convert",
                feature = "web"
            ))]
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
    /// The parsed JSON document, equal to `JSON.parse(this.json())` (with
    /// integers beyond 2^53 − 1 as bigints).
    #[wasm_bindgen(getter, unchecked_return_type = "any")]
    pub fn document(&self) -> JsResult<JsValue> {
        document_to_js(&self.write_json(false)?)
    }
    /// Typed response fields as plain data (per operation; see the module
    /// documentation).
    #[wasm_bindgen(getter, unchecked_return_type = "any")]
    pub fn summary(&self) -> JsResult<JsValue> {
        conv::to_js(&self.summary_value()?)
    }
    /// The operation's diagnostics, or `null` for operations with one
    /// report per output or input (convert, merge, diff, formats).
    #[wasm_bindgen(getter, unchecked_return_type = "Report | null")]
    pub fn report(&self) -> JsResult<JsValue> {
        let report = match &self.response {
            #[cfg(any(
                feature = "all-formats",
                feature = "merge",
                feature = "convert",
                feature = "web"
            ))]
            Response::Sfnt(r) => &r.report,
            Response::Charset(r) => &r.report,
            Response::Validate(r) => &r.report,
            Response::Info(r) => &r.report,
            Response::Preview(r) => &r.report,
            Response::Render(r) => &r.report,
            _ => return Ok(JsValue::NULL),
        };
        conv::to_js(report)
    }
    /// Every prepared asset as `{storage key: Uint8Array}` (fresh copies).
    #[wasm_bindgen(getter, unchecked_return_type = "Record<string, Uint8Array>")]
    pub fn outputs(&self) -> JsResult<JsValue> {
        Ok(outputs_object(&self.outputs)?.into())
    }
    /// Every prepared output as `{destination key: OutputBundleDict}` (the
    /// primary asset and its sidecars or directory members, with paths
    /// relative to the destination's parent).
    #[wasm_bindgen(getter, unchecked_return_type = "Record<string, OutputBundleDict>")]
    pub fn bundles(&self) -> JsResult<JsValue> {
        Ok(bundles_object(&self.outputs)?.into())
    }
    /// The requested destination of each prepared output, in order.
    #[wasm_bindgen(getter)]
    pub fn destinations(&self) -> Vec<String> {
        self.outputs
            .iter()
            .map(|o| o.destination.to_string_lossy().into_owned())
            .collect()
    }
    /// Command-line exit status of the completed operation: 0, or 1 for a
    /// diff that found differences.
    #[wasm_bindgen(getter, js_name = exitCode)]
    pub fn exit_code(&self) -> u8 {
        match &self.response {
            Response::Diff(r) => r.status().err().map_or(0, |e| e.exit_code()),
            _ => 0,
        }
    }
    /// The decoded fonts of an `info` result (a `FontSet` sharing this
    /// result's storage), else `undefined`.
    #[wasm_bindgen(getter)]
    pub fn fonts(&self) -> Option<JsFontSet> {
        self.fonts
            .as_ref()
            .map(|set| JsFontSet { set: set.clone() })
    }
    /// The JSON metadata of one decoded font (`info` results only).
    #[wasm_bindgen(js_name = fontJson)]
    pub fn font_json(&self, index: f64, pretty: Option<bool>) -> JsResult<String> {
        let index: usize = conv::int(index, "index")?;
        match &self.response {
            Response::Info(r) => r
                .font_json(index, pretty.unwrap_or(true))
                .map_err(errors::to_js),
            _ => Err(js_sys::TypeError::new("fontJson() applies to info results").into()),
        }
    }
    /// The status record of one format identifier (`formats` results only),
    /// or `undefined`.
    #[wasm_bindgen(js_name = formatStatus, unchecked_return_type = "FormatStatus | undefined")]
    pub fn format_status(&self, id: &str) -> JsResult<JsValue> {
        match &self.response {
            Response::Formats(r) => match r.get(id) {
                Some(status) => conv::to_js(status),
                None => Ok(JsValue::UNDEFINED),
            },
            _ => Err(js_sys::TypeError::new("formatStatus() applies to formats results").into()),
        }
    }
}

/// The result of a batch: the complete JSON document plus typed summaries.
#[wasm_bindgen(js_name = BatchResult)]
pub struct JsBatchResult {
    response: BatchResponse,
    document: String,
}

#[wasm_bindgen(js_class = BatchResult)]
impl JsBatchResult {
    /// Aggregate exit code (precedence 2 > 1 > 3 > 0), as `fontpixel batch`.
    #[wasm_bindgen(getter, js_name = exitCode)]
    pub fn exit_code(&self) -> u8 {
        self.response.exit_code
    }
    /// Requested worker ceiling, capped by the task count.
    #[wasm_bindgen(getter)]
    pub fn jobs(&self) -> usize {
        self.response.jobs
    }
    /// Workers actually used (always 1 here: no threads).
    #[wasm_bindgen(getter)]
    pub fn workers(&self) -> usize {
        self.response.workers
    }
    /// Per-task outcomes in request order (`{index, id, kind, success,
    /// exit_code, committed, diagnostics_truncated, error}`).
    #[wasm_bindgen(getter, unchecked_return_type = "BatchTaskSummary[]")]
    pub fn tasks(&self) -> JsResult<JsValue> {
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
        conv::to_js(&tasks)
    }
    /// The complete batch JSON document exactly as `fontpixel batch` prints it.
    pub fn json(&self) -> String {
        self.document.clone()
    }
    /// The parsed batch document.
    #[wasm_bindgen(getter, unchecked_return_type = "any")]
    pub fn document(&self) -> JsResult<JsValue> {
        document_to_js(&self.document)
    }
    /// Committed outputs as `{storage key: Uint8Array}`.
    #[wasm_bindgen(getter, unchecked_return_type = "Record<string, Uint8Array>")]
    pub fn outputs(&self) -> JsResult<JsValue> {
        Ok(outputs_object(&self.response.outputs)?.into())
    }
    /// Committed outputs as `{destination key: OutputBundleDict}`.
    #[wasm_bindgen(getter, unchecked_return_type = "Record<string, OutputBundleDict>")]
    pub fn bundles(&self) -> JsResult<JsValue> {
        Ok(bundles_object(&self.response.outputs)?.into())
    }
}

// ---------------------------------------------------------------- operations

macro_rules! writer_op {
    ($(#[$doc:meta])* $cfg:meta, $name:ident, $js:literal, $request:ty, $ts:literal, $call:path, $variant:ident) => {
        $(#[$doc])*
        #[$cfg]
        #[wasm_bindgen(js_namespace = ops, js_name = $js)]
        pub fn $name(
            #[wasm_bindgen(unchecked_param_type = $ts)] request: JsValue,
            #[wasm_bindgen(unchecked_optional_param_type = "OpsCallOptions")] options: Option<JsValue>,
        ) -> JsResult<JsOperationResult> {
            let parsed: $request = self::request(&request)?;
            let call = Call::new(options, concat!($js, " options"), false)?;
            let completed = call.finish($call(parsed, &call.files.storage))?;
            Ok(JsOperationResult::new(
                $js,
                Response::$variant(completed.response.into()),
                completed.outputs,
            ))
        }
    };
}

macro_rules! reader_op {
    ($(#[$doc:meta])* $name:ident, $js:literal, $request:ty, $ts:literal, $call:path, $variant:ident) => {
        $(#[$doc])*
        #[wasm_bindgen(js_namespace = ops, js_name = $js)]
        pub fn $name(
            #[wasm_bindgen(unchecked_param_type = $ts)] request: JsValue,
            #[wasm_bindgen(unchecked_optional_param_type = "OpsCallOptions")] options: Option<JsValue>,
        ) -> JsResult<JsOperationResult> {
            let parsed: $request = self::request(&request)?;
            let call = Call::new(options, concat!($js, " options"), false)?;
            let response = call.finish($call(parsed, &call.files.storage))?;
            Ok(JsOperationResult::new($js, Response::$variant(response.into()), Vec::new()))
        }
    };
}

fn convert_impl(
    operation: &'static str,
    request: &JsValue,
    options: Option<JsValue>,
) -> JsResult<JsOperationResult> {
    let parsed: ConvertRequest = self::request(request)?;
    let name = if operation == "vectorize" {
        "vectorize options"
    } else {
        "convert options"
    };
    let call = Call::new(options, name, true)?;
    let result = match &call.progress {
        None => fontpixel_ops::convert(parsed, &call.files.storage),
        Some(progress) => fontpixel_ops::convert_with_progress(
            parsed,
            &call.files.storage,
            &mut progress.observer(),
        ),
    };
    let completed = call.finish(result)?;
    Ok(JsOperationResult::new(
        operation,
        Response::Convert(completed.response),
        completed.outputs,
    ))
}

/// Convert inputs into one output, or each input into its own output
/// (`fontpixel convert`). Request keys: `inputs`, `output`, `from`, `to`,
/// `each`, `strict`, `font_index`, `report` and every reader/writer option.
///
/// `options.progress`, if given, is called with `{step, done, total, item,
/// glyphs, bytes}` after each input is read (`"read"`) and each output is
/// encoded (`"encode"`). Returning `false` cancels with `CancelledError`;
/// throwing cancels and rethrows; nothing is returned either way. Results
/// do not depend on it.
#[wasm_bindgen(js_namespace = ops, js_name = convert)]
pub fn ops_convert(
    #[wasm_bindgen(unchecked_param_type = "ConvertRequest")] request: JsValue,
    #[wasm_bindgen(unchecked_optional_param_type = "OpsProgressOptions")] options: Option<JsValue>,
) -> JsResult<JsOperationResult> {
    convert_impl("convert", &request, options)
}

/// Vectorize (`fontpixel vectorize`): the command line's alias of
/// `convert`; the same operation, documents and progress contract.
#[wasm_bindgen(js_namespace = ops, js_name = vectorize)]
pub fn ops_vectorize(
    #[wasm_bindgen(unchecked_param_type = "ConvertRequest")] request: JsValue,
    #[wasm_bindgen(unchecked_optional_param_type = "OpsProgressOptions")] options: Option<JsValue>,
) -> JsResult<JsOperationResult> {
    convert_impl("vectorize", &request, options)
}

/// Merge selected glyphs from ordered parts and produce every output as one
/// group (`fontpixel merge`). Request keys: `parts` (`[{input, take, dy,
/// from, font_index}]`), `outputs` (`[{destination, format}]`), `family`
/// and the merge/writer options.
///
/// `options.progress`, if given, is called after each part is read
/// (`"read"`), after the merge (`"merge"`) and after each output is encoded
/// (`"encode"`), before anything is returned. Returning `false` cancels
/// with `CancelledError`; throwing cancels and rethrows. Results do not
/// depend on it.
#[wasm_bindgen(js_namespace = ops, js_name = merge)]
pub fn ops_merge(
    #[wasm_bindgen(unchecked_param_type = "MergeRequest")] request: JsValue,
    #[wasm_bindgen(unchecked_optional_param_type = "OpsProgressOptions")] options: Option<JsValue>,
) -> JsResult<JsOperationResult> {
    let parsed: MergeRequest = self::request(&request)?;
    let call = Call::new(options, "merge options", true)?;
    let result = match &call.progress {
        None => fontpixel_ops::merge(parsed, &call.files.storage),
        Some(progress) => fontpixel_ops::merge_with_progress(
            parsed,
            &call.files.storage,
            &mut progress.observer(),
        ),
    };
    let completed = call.finish(result)?;
    Ok(JsOperationResult::new(
        "merge",
        Response::Merge(Box::new(completed.response)),
        completed.outputs,
    ))
}

writer_op!(
    /// Build a locl-enabled OpenType font from regional and script faces
    /// (`fontpixel variants`). Needs the `sfnt_operations` capability.
    cfg(any(feature = "all-formats", feature = "merge", feature = "convert", feature = "web")),
    ops_variants, "variants", VariantsRequest, "VariantsRequest", fontpixel_ops::variants, Sfnt
);
writer_op!(
    /// Subset an sfnt, WOFF or WOFF2 font (`fontpixel subset`). Needs the
    /// `sfnt_operations` capability.
    cfg(any(feature = "all-formats", feature = "merge", feature = "convert", feature = "web")),
    ops_subset, "subset", SubsetRequest, "SubsetRequest", fontpixel_ops::subset, Sfnt
);
writer_op!(
    /// Remap glyph codes into another charset and write the result
    /// (`fontpixel charset`).
    cfg(all()),
    ops_charset, "charset", CharsetRequest, "CharsetRequest", fontpixel_ops::charset, Charset
);
writer_op!(
    /// Lay out text (or every mapping with `all: true`) into a PNG or SVG
    /// image (`fontpixel render`).
    cfg(all()),
    ops_render, "render", RenderRequest, "RenderRequest", fontpixel_ops::render, Render
);
reader_op!(
    /// Validate one input (`fontpixel validate`).
    ops_validate, "validate", ValidateRequest, "ValidateRequest", fontpixel_ops::validate, Validate
);
reader_op!(
    /// Compare two inputs (`fontpixel diff`). Differences are reported in
    /// the result (`summary.equal`, `exitCode` 1), not thrown.
    ops_diff, "diff", DiffRequest, "DiffRequest", fontpixel_ops::diff, Diff
);
reader_op!(
    /// Decode one input and describe its fonts (`fontpixel info`); the
    /// result's `fonts` is the decoded `FontSet`.
    ops_info, "info", InfoRequest, "InfoRequest", fontpixel_ops::info, Info
);
reader_op!(
    /// Lay out text (or every mapping) as character art (`fontpixel
    /// preview`): `text()` is the art, `json()` the diagnostics document.
    ops_preview, "preview", PreviewRequest, "PreviewRequest", fontpixel_ops::preview, Preview
);

#[cfg(not(any(
    feature = "all-formats",
    feature = "merge",
    feature = "convert",
    feature = "web"
)))]
mod sfnt_unavailable {
    use super::*;
    /// `fontpixel variants`: compiled out of this build profile; throws
    /// `UnsupportedError` (capability `sfnt_operations`).
    #[wasm_bindgen(js_namespace = ops, js_name = variants)]
    pub fn ops_variants(
        #[wasm_bindgen(unchecked_param_type = "VariantsRequest")] _request: JsValue,
        #[wasm_bindgen(unchecked_optional_param_type = "OpsCallOptions")] _options: Option<JsValue>,
    ) -> JsResult<JsOperationResult> {
        Err(capabilities::unsupported(
            "sfnt_operations",
            "ops.variants()",
        ))
    }
    /// `fontpixel subset`: compiled out of this build profile; throws
    /// `UnsupportedError` (capability `sfnt_operations`).
    #[wasm_bindgen(js_namespace = ops, js_name = subset)]
    pub fn ops_subset(
        #[wasm_bindgen(unchecked_param_type = "SubsetRequest")] _request: JsValue,
        #[wasm_bindgen(unchecked_optional_param_type = "OpsCallOptions")] _options: Option<JsValue>,
    ) -> JsResult<JsOperationResult> {
        Err(capabilities::unsupported("sfnt_operations", "ops.subset()"))
    }
}

/// List every catalogue format with the reader/writer compiled into this
/// build (`fontpixel formats`). Request keys: `json`.
#[wasm_bindgen(js_namespace = ops, js_name = formats)]
pub fn ops_formats(
    #[wasm_bindgen(unchecked_optional_param_type = "FormatsRequest")] request: Option<JsValue>,
) -> JsResult<JsOperationResult> {
    let parsed: FormatsRequest = self::request(&request.unwrap_or(JsValue::UNDEFINED))?;
    let response = fontpixel_ops::formats(&parsed);
    Ok(JsOperationResult::new(
        "formats",
        Response::Formats(response),
        Vec::new(),
    ))
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

/// The typed batch request shape accepted by `ops.batch`.
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

/// One typed task: a valid request becomes `BatchTask::new`; one that fails
/// deserialization becomes an invalid task (exit code 2), as a task whose
/// command line is rejected in a TOML batch.
fn batch_task(value: Value) -> JsResult<BatchTask> {
    let Value::Object(mut map) = value else {
        return Err(errors::request_error("every batch task must be an object"));
    };
    let id = match map.remove("id") {
        Some(Value::String(id)) => id,
        _ => return Err(errors::request_error("every batch task needs a string id")),
    };
    let kind: TaskKind = match map.get("kind") {
        Some(kind) => serde_json::from_value(kind.clone())
            .map_err(|error| errors::request_error(format!("task {id}: {error}")))?,
        None => return Err(errors::request_error(format!("task {id} needs a kind"))),
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

/// A whole-batch failure: the error carries `document`, the failure JSON
/// document `fontpixel batch` prints, and its `exitCode`.
fn batch_failure(error: Error) -> JsValue {
    let mut document = Vec::new();
    let code = batch::write_failure(&mut document, &error);
    let exception = errors::with_exit_code(error, code);
    let _ = Reflect::set(
        &exception,
        &JsValue::from_str("document"),
        &JsValue::from_str(&String::from_utf8_lossy(&document)),
    );
    exception
}

fn batch_result(
    files: &Files,
    run: impl FnOnce(&mut Vec<u8>) -> fontpixel::Result<BatchResponse>,
) -> JsResult<JsBatchResult> {
    let mut document = Vec::new();
    let result = run(&mut document).map_err(batch_failure);
    let response = files.finish(result)?;
    Ok(JsBatchResult {
        response,
        document: utf8(document)?,
    })
}

/// Run typed tasks as one batch (`fontpixel batch` with typed tasks).
///
/// Request keys: `tasks` (request objects, each with `id` and a `kind` of
/// `convert`, `vectorize`, `variants`, `subset`, `merge`, `charset`,
/// `validate` or `diff`), `jobs`, `max_working_bytes`, `max_pending_bytes`,
/// `max_report_bytes`, `max_task_report_bytes` and `protected_paths`. A task
/// whose request is invalid fails alone with exit code 2. Every successful
/// writer's outputs are returned. Without threads a batch runs one worker
/// inline; one whose admission computes more than one worker throws
/// `UnsupportedError` (capability `threads`). A whole-batch failure throws
/// with a `document` property.
#[wasm_bindgen(js_namespace = ops, js_name = batch)]
pub fn ops_batch(
    #[wasm_bindgen(unchecked_param_type = "BatchSpec")] request: JsValue,
    #[wasm_bindgen(unchecked_optional_param_type = "OpsCallOptions")] options: Option<JsValue>,
) -> JsResult<JsBatchResult> {
    let spec: BatchSpec = self::request(&request)?;
    let tasks = spec
        .tasks
        .into_iter()
        .map(batch_task)
        .collect::<JsResult<Vec<_>>>()?;
    let request = BatchRequest {
        jobs: spec.jobs,
        max_working_bytes: spec.max_working_bytes,
        max_pending_bytes: spec.max_pending_bytes,
        max_report_bytes: spec.max_report_bytes,
        max_task_report_bytes: spec.max_task_report_bytes,
        protected_paths: spec.protected_paths,
        tasks,
    };
    let call = Call::new(options, "batch options", false)?;
    batch_result(&call.files, |out| {
        batch::run(request, &call.files.storage, out)
    })
}

/// Run a version-1 TOML job text (the `fontpixel batch` file format) in
/// memory storage. Options: `files` (required), `base` (relative paths
/// resolve against it; default none) and `jobs` (overrides the file's
/// worker ceiling). Needs the `toml_batch` capability.
#[wasm_bindgen(js_namespace = ops, js_name = batchToml)]
pub fn ops_batch_toml(
    text: &str,
    #[wasm_bindgen(unchecked_optional_param_type = "BatchTomlOptions")] options: Option<JsValue>,
) -> JsResult<JsBatchResult> {
    let options = Options::new(
        &options.unwrap_or(JsValue::UNDEFINED),
        &["files", "base", "jobs"],
        "batchToml options",
    )?;
    let base = PathBuf::from(options.string("base")?.unwrap_or_default());
    let jobs = options
        .get("jobs")?
        .map(|j| conv::int_from_js::<usize>(&j, "jobs"))
        .transpose()?;
    let files = Files::new(
        &options.get("files")?.unwrap_or(JsValue::UNDEFINED),
        "ops.batchToml()",
    )?;
    #[cfg(feature = "toml")]
    {
        batch_result(&files, |out| {
            batch::toml::run_text(text, &base, jobs, &files.storage, out)
        })
    }
    #[cfg(not(feature = "toml"))]
    {
        let _ = (text, base, jobs, files);
        Err(capabilities::unsupported("toml_batch", "ops.batchToml()"))
    }
}

/// Parse a TOML job text without running it: `{jobs, max_*_bytes, tasks}`
/// with each task's `id`, `kind`, `declared_inputs` and either `request`
/// (the normalized request) and `weight`, or `error` for a task that failed
/// preparation. Option `base` as in `batchToml`. Needs the `toml_batch`
/// capability.
#[wasm_bindgen(js_namespace = ops, js_name = parseBatchToml, unchecked_return_type = "ParsedBatch")]
pub fn ops_parse_batch_toml(
    text: &str,
    #[wasm_bindgen(unchecked_optional_param_type = "{ base?: string }")] options: Option<JsValue>,
) -> JsResult<JsValue> {
    let options = Options::new(
        &options.unwrap_or(JsValue::UNDEFINED),
        &["base"],
        "parseBatchToml options",
    )?;
    let base = PathBuf::from(options.string("base")?.unwrap_or_default());
    #[cfg(feature = "toml")]
    {
        let request = batch::toml::parse(text, &base).map_err(errors::to_js)?;
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
            .collect::<JsResult<Vec<_>>>()?;
        conv::to_js(&json!({
            "jobs": request.jobs,
            "max_working_bytes": request.max_working_bytes,
            "max_pending_bytes": request.max_pending_bytes,
            "max_report_bytes": request.max_report_bytes,
            "max_task_report_bytes": request.max_task_report_bytes,
            "tasks": tasks,
        }))
    }
    #[cfg(not(feature = "toml"))]
    {
        let _ = (text, base);
        Err(capabilities::unsupported(
            "toml_batch",
            "ops.parseBatchToml()",
        ))
    }
}

/// `fontpixel batch FILE`: reads a job file and writes outputs through the
/// filesystem, which WebAssembly does not have. Always throws
/// `UnsupportedError` (capability `filesystem`); read the job file and its
/// inputs yourself and call `ops.batchToml(text, {files})`.
#[wasm_bindgen(js_namespace = ops, js_name = batchFile)]
pub fn ops_batch_file(
    _path: &str,
    #[wasm_bindgen(unchecked_optional_param_type = "{ jobs?: number }")] options: Option<JsValue>,
) -> JsResult<JsBatchResult> {
    Options::new(
        &options.unwrap_or(JsValue::UNDEFINED),
        &["jobs"],
        "batchFile options",
    )?;
    Err(capabilities::unsupported(
        "filesystem",
        "ops.batchFile() (fontpixel_ops::batch::toml::run_path/run_file)",
    ))
}

// ---------------------------------------------------------------- queries

/// The operations layer's platform answers: `{filesystem, threads,
/// toml_batch}` (booleans, `fontpixel_ops::capabilities()`); see the
/// top-level `capabilities()` for the records with reasons.
#[wasm_bindgen(js_namespace = ops, js_name = capabilities, unchecked_return_type = "OpsCapabilities")]
pub fn ops_capabilities() -> JsResult<JsValue> {
    let caps = fontpixel_ops::capabilities();
    conv::to_js(&json!({
        "filesystem": caps.filesystem,
        "threads": caps.threads,
        "toml_batch": caps.toml_batch,
    }))
}

/// The batch defaults (`fontpixel_ops::batch::DEFAULT_*`):
/// `{max_working_bytes, max_pending_bytes, max_report_bytes,
/// max_task_report_bytes}`.
#[wasm_bindgen(js_namespace = ops, js_name = defaults, unchecked_return_type = "BatchDefaults")]
pub fn ops_defaults() -> JsResult<JsValue> {
    conv::to_js(&json!({
        "max_working_bytes": batch::DEFAULT_MAX_WORKING_BYTES as u64,
        "max_pending_bytes": batch::DEFAULT_MAX_PENDING_BYTES as u64,
        "max_report_bytes": batch::DEFAULT_MAX_REPORT_BYTES as u64,
        "max_task_report_bytes": batch::DEFAULT_MAX_TASK_REPORT_BYTES as u64,
    }))
}

/// The memory-storage key of a path: `.` removed, `..` folded, `/`
/// separators.
#[wasm_bindgen(js_namespace = ops, js_name = memoryKey)]
pub fn ops_memory_key(path: &str) -> String {
    fontpixel_ops::memory::key(std::path::Path::new(path))
}

/// The output format `convert` infers for a destination path (OpenType
/// container extensions first, then the registry's filename hints); an
/// ambiguous or unknown extension throws like `convert` without `to`.
#[wasm_bindgen(js_namespace = ops, js_name = outputFormatFor)]
pub fn ops_output_format_for(destination: &str) -> JsResult<String> {
    fontpixel_ops::convert::output_format_for(std::path::Path::new(destination))
        .map_err(errors::to_js)
}

/// One request of a known operation, typed for `describe`.
enum Described {
    Convert(Box<ConvertRequest>),
    Merge(Box<MergeRequest>),
    #[cfg(any(
        feature = "all-formats",
        feature = "merge",
        feature = "convert",
        feature = "web"
    ))]
    Variants(Box<VariantsRequest>),
    #[cfg(any(
        feature = "all-formats",
        feature = "merge",
        feature = "convert",
        feature = "web"
    ))]
    Subset(Box<SubsetRequest>),
    Charset(Box<CharsetRequest>),
    Validate(Box<ValidateRequest>),
    Diff(Box<DiffRequest>),
    Info(Box<InfoRequest>),
    Preview(Box<PreviewRequest>),
    Render(Box<RenderRequest>),
    Formats(Box<FormatsRequest>),
}

fn style_name(style: ReportStyle) -> &'static str {
    match style {
        ReportStyle::Text => "text",
        ReportStyle::Json => "json",
    }
}

impl Described {
    /// `fontpixel_ops::cli::Command::report_style` for the same request.
    fn report_style(&self) -> ReportStyle {
        match self {
            Self::Merge(r) => r.report_style(),
            Self::Diff(r) => r.report_style(),
            Self::Validate(r) => r.report_style(),
            Self::Charset(r) => r.report,
            #[cfg(any(
                feature = "all-formats",
                feature = "merge",
                feature = "convert",
                feature = "web"
            ))]
            Self::Variants(r) => r.output.report,
            #[cfg(any(
                feature = "all-formats",
                feature = "merge",
                feature = "convert",
                feature = "web"
            ))]
            Self::Subset(r) => r.output.report,
            Self::Info(r) if r.json => ReportStyle::Json,
            Self::Formats(r) if r.json => ReportStyle::Json,
            Self::Convert(r) => r.report,
            Self::Preview(r) => r.layout.report,
            Self::Render(r) => r.layout.report,
            _ => ReportStyle::Text,
        }
    }
    /// `fontpixel_ops::cli::Command::source_identity_total_bytes`.
    fn source_identity_total_bytes(&self) -> usize {
        match self {
            Self::Merge(r) => r.source_identity_total_bytes(),
            Self::Diff(r) => r.source_identity_total_bytes(),
            Self::Validate(r) => r.source_identity_total_bytes(),
            Self::Charset(r) => r.source_identity_total_bytes(),
            #[cfg(any(
                feature = "all-formats",
                feature = "merge",
                feature = "convert",
                feature = "web"
            ))]
            Self::Variants(r) => r.source_identity_total_bytes(),
            #[cfg(any(
                feature = "all-formats",
                feature = "merge",
                feature = "convert",
                feature = "web"
            ))]
            Self::Subset(r) => r.source_identity_total_bytes(),
            Self::Info(r) => r.source_identity_total_bytes(),
            Self::Convert(r) => r.source_identity_total_bytes(),
            Self::Preview(r) => r.source_identity_total_bytes(),
            Self::Render(r) => r.source_identity_total_bytes(),
            Self::Formats(_) => fontpixel::ResourceLimits::default().max_total_bytes,
        }
    }
    fn request(&self) -> JsResult<Value> {
        match self {
            Self::Convert(r) => to_value(r),
            Self::Merge(r) => to_value(r),
            #[cfg(any(
                feature = "all-formats",
                feature = "merge",
                feature = "convert",
                feature = "web"
            ))]
            Self::Variants(r) => to_value(r),
            #[cfg(any(
                feature = "all-formats",
                feature = "merge",
                feature = "convert",
                feature = "web"
            ))]
            Self::Subset(r) => to_value(r),
            Self::Charset(r) => to_value(r),
            Self::Validate(r) => to_value(r),
            Self::Diff(r) => to_value(r),
            Self::Info(r) => to_value(r),
            Self::Preview(r) => to_value(r),
            Self::Render(r) => to_value(r),
            Self::Formats(r) => to_value(r),
        }
    }
}

fn describe(name: &str, described: &Described, task: Option<&TaskRequest>) -> JsResult<JsValue> {
    let mut value = json!({
        "command": name,
        "request": described.request()?,
        "report_style": style_name(described.report_style()),
        "source_identity_total_bytes": described.source_identity_total_bytes(),
    });
    if let Some(task) = task {
        value["declared_inputs"] = json!(task.declared_inputs());
        value["weight"] = json!(task.weight());
    }
    conv::to_js(&value)
}

/// Validate a request for `operation` and describe it: the normalized
/// `request` (every default filled in), `report_style`,
/// `source_identity_total_bytes` and, for batch task kinds,
/// `declared_inputs` and `weight`. Throws `InvalidInputError` (exit code 2)
/// exactly when the operation would reject the request before running.
#[wasm_bindgen(js_namespace = ops, js_name = inspectRequest, unchecked_return_type = "RequestDescription")]
pub fn ops_inspect_request(
    operation: &str,
    #[wasm_bindgen(unchecked_optional_param_type = "object")] request: Option<JsValue>,
) -> JsResult<JsValue> {
    let object = request.unwrap_or(JsValue::UNDEFINED);
    let (described, task) = match operation {
        "convert" | "vectorize" => {
            let r: ConvertRequest = self::request(&object)?;
            let task = if operation == "convert" {
                TaskRequest::Convert(Box::new(r.clone()))
            } else {
                TaskRequest::Vectorize(Box::new(r.clone()))
            };
            (Described::Convert(Box::new(r)), Some(task))
        }
        "merge" => {
            let r: MergeRequest = self::request(&object)?;
            (
                Described::Merge(Box::new(r.clone())),
                Some(TaskRequest::Merge(Box::new(r))),
            )
        }
        #[cfg(any(
            feature = "all-formats",
            feature = "merge",
            feature = "convert",
            feature = "web"
        ))]
        "variants" => {
            let r: VariantsRequest = self::request(&object)?;
            (
                Described::Variants(Box::new(r.clone())),
                Some(TaskRequest::Variants(Box::new(r))),
            )
        }
        #[cfg(any(
            feature = "all-formats",
            feature = "merge",
            feature = "convert",
            feature = "web"
        ))]
        "subset" => {
            let r: SubsetRequest = self::request(&object)?;
            (
                Described::Subset(Box::new(r.clone())),
                Some(TaskRequest::Subset(Box::new(r))),
            )
        }
        #[cfg(not(any(
            feature = "all-formats",
            feature = "merge",
            feature = "convert",
            feature = "web"
        )))]
        "variants" | "subset" => {
            return Err(capabilities::unsupported(
                "sfnt_operations",
                &format!("ops.inspectRequest({operation:?})"),
            ));
        }
        "charset" => {
            let r: CharsetRequest = self::request(&object)?;
            (
                Described::Charset(Box::new(r.clone())),
                Some(TaskRequest::Charset(Box::new(r))),
            )
        }
        "validate" => {
            let r: ValidateRequest = self::request(&object)?;
            (
                Described::Validate(Box::new(r.clone())),
                Some(TaskRequest::Validate(Box::new(r))),
            )
        }
        "diff" => {
            let r: DiffRequest = self::request(&object)?;
            (
                Described::Diff(Box::new(r.clone())),
                Some(TaskRequest::Diff(Box::new(r))),
            )
        }
        "info" => (Described::Info(Box::new(self::request(&object)?)), None),
        "preview" => (Described::Preview(Box::new(self::request(&object)?)), None),
        "render" => (Described::Render(Box::new(self::request(&object)?)), None),
        "formats" => (Described::Formats(Box::new(self::request(&object)?)), None),
        other => {
            return Err(errors::request_error(format!(
                "unknown operation {other:?}"
            )));
        }
    };
    describe(operation, &described, task.as_ref())
}

/// Parse a `fontpixel` command line (without the program name) with the
/// command-line definition and return `{command, request, report_style,
/// source_identity_total_bytes}`; `request` can be passed to the matching
/// `ops` function. A rejected command line throws `InvalidInputError` with
/// the parser's message and `exitCode` (0 for `--help`/`--version`). Needs
/// the `cli_parser` capability.
#[wasm_bindgen(js_namespace = ops, js_name = parseArgs, unchecked_return_type = "RequestDescription")]
pub fn ops_parse_args(argv: Vec<String>) -> JsResult<JsValue> {
    #[cfg(feature = "toml")]
    {
        use fontpixel_ops::cli::{Cli, Command};
        let args = std::iter::once("fontpixel".to_owned()).chain(argv);
        let command = <Cli as clap::Parser>::try_parse_from(args)
            .map_err(|error| {
                errors::with_exit_code(
                    Error::InvalidInput(error.to_string()),
                    error.exit_code().clamp(0, 255) as u8,
                )
            })?
            .command;
        let (name, described) = match command {
            Command::Batch(args) => {
                return conv::to_js(&json!({
                    "command": "batch",
                    "request": {"file": args.file, "jobs": args.jobs},
                    "report_style": "json",
                    "source_identity_total_bytes":
                        fontpixel::ResourceLimits::default().max_total_bytes,
                }));
            }
            Command::Merge(r) => ("merge", Described::Merge(r)),
            Command::Diff(r) => ("diff", Described::Diff(r)),
            Command::Validate(r) => ("validate", Described::Validate(r)),
            Command::Charset(r) => ("charset", Described::Charset(r)),
            #[cfg(any(
                feature = "all-formats",
                feature = "merge",
                feature = "convert",
                feature = "web"
            ))]
            Command::Variants(r) => ("variants", Described::Variants(r)),
            #[cfg(any(
                feature = "all-formats",
                feature = "merge",
                feature = "convert",
                feature = "web"
            ))]
            Command::Subset(r) => ("subset", Described::Subset(r)),
            Command::Formats(r) => ("formats", Described::Formats(Box::new(r))),
            Command::Info(r) => ("info", Described::Info(r)),
            Command::Preview(r) => ("preview", Described::Preview(r)),
            Command::Render(r) => ("render", Described::Render(r)),
            Command::Convert(r) => ("convert", Described::Convert(r)),
        };
        describe(name, &described, None)
    }
    #[cfg(not(feature = "toml"))]
    {
        let _ = argv;
        Err(capabilities::unsupported("cli_parser", "ops.parseArgs()"))
    }
}
