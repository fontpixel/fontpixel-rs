//! Inputs, sidecar resources, read contexts and output bundles
//! (`fontpixel::resources`).
//!
//! Synchronous WASM calls never read files or the network. A `resources`
//! argument (`ResourcesLike`) is one of:
//!
//! - a `ResourceSet` (named in-memory files, shared: later inserts are seen);
//! - a plain object or `Map` of relative path → bytes (`Uint8Array`,
//!   `ArrayBuffer`, `DataView`), copied into WASM memory once;
//! - a function `(path, maxBytes) => bytes | null | undefined`, or an object
//!   with `resolve(path, maxBytes)` and optionally
//!   `list(directory, maxEntries, maxBytes) => string[]` (`directory` is
//!   `null` for the root). It is called synchronously while the operation
//!   runs; `null`/`undefined` (or a thrown `MissingResourceError`) means the
//!   file does not exist. Any other exception aborts the call and is
//!   rethrown unchanged once it returns.
//!
//! Asynchronous sources (fetch, `node:fs/promises`, IndexedDB) go through
//! `withAsyncResources` in `js/resources.js`: it runs the deterministic
//! operation with a recording resolver over the files fetched so far, awaits
//! every name the run asked for and did not have, and runs it again until a
//! run asks for nothing new. No I/O is hidden inside a synchronous call.
//!
//! `ReadContext` (shared accounting for one input) borrows its input in
//! Rust; Python keeps it on a worker thread, which WASM does not have. The
//! JS class owns the input and replays: each call rebuilds
//! `ReadContext::new` and re-runs the earlier operations against a resolver
//! that answers from their recorded results (the JS resolver is not called
//! again), then runs the new operation live. The accounting is therefore
//! exactly the core's. The context is borrowed while a JS resolver runs, so
//! a resolver that re-enters the same context gets `InvalidInputError`.
use crate::callbacks::{Callback, CallbackScope};
use crate::conv;
use crate::errors::{self, IntoJs, JsResult};
use crate::model_args::{Handle, array_arg, handle_arg, stash};
use fontpixel::resources::{
    Input, NoResources, OutputAsset, OutputBundle, ReadContext, ResourceLimits, ResourcePath,
    ResourceResolver, TransportLimits, Written,
};
use fontpixel::{Error, Report, Result};
use js_sys::{Array, Function, Map, Object, Reflect, Uint8Array};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

fn limit(resource: &'static str, limit: usize) -> Error {
    Error::ResourceLimit { resource, limit }
}

/// Validate a portable relative asset path (`ResourcePath::new`).
pub fn path_arg(path: &str) -> JsResult<ResourcePath> {
    ResourcePath::new(path).js()
}

// ---------------------------------------------------------------- resolvers

/// Named in-memory files shared by a `ResourceSet` and the resolvers made
/// from it (`Arc<RwLock>` so the resolver is `Send + Sync` on native hosts,
/// where this crate also compiles).
pub type Files = Arc<RwLock<BTreeMap<ResourcePath, Vec<u8>>>>;

fn poisoned() -> Error {
    Error::InvalidInput("resource set is in use by the running call".into())
}

/// In-memory resolver over named files; `list` returns the immediate files
/// of a directory prefix (`ResourceResolver` for `ResourceSet`s and
/// object/`Map` resources).
pub struct SetResolver(pub Files);

impl ResourceResolver for SetResolver {
    fn resolve(&self, path: &ResourcePath, max_bytes: usize) -> Result<Vec<u8>> {
        let files = self.0.try_read().map_err(|_| poisoned())?;
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
        let files = self.0.try_read().map_err(|_| poisoned())?;
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

/// A JS resolver function or object, called synchronously.
pub struct JsResolver(pub Callback);

fn is_missing(error: &JsValue) -> bool {
    Reflect::get(error, &JsValue::from_str("kind"))
        .ok()
        .and_then(|k| k.as_string())
        .as_deref()
        == Some("MissingResource")
}

fn callback_failed(what: &str) -> Error {
    Error::Io(std::io::Error::other(format!(
        "resource {what} callback threw"
    )))
}

impl JsResolver {
    /// `(this, function)` for `name` (`resolve` or `list`).
    fn target(&self, name: &str) -> Option<(JsValue, Function)> {
        let object = self.0.object()?;
        if let Some(f) = object.dyn_ref::<Function>() {
            return (name == "resolve").then(|| (JsValue::UNDEFINED, f.clone()));
        }
        let method = Reflect::get(&object, &JsValue::from_str(name)).ok()?;
        method.dyn_into::<Function>().ok().map(|f| (object, f))
    }
}

impl ResourceResolver for JsResolver {
    fn resolve(&self, path: &ResourcePath, max_bytes: usize) -> Result<Vec<u8>> {
        let (this, resolve) = self
            .target("resolve")
            .ok_or_else(|| Error::InvalidInput("resource resolver has no resolve()".into()))?;
        let value = match resolve.call2(
            &this,
            &JsValue::from_str(path.as_str()),
            &JsValue::from_f64(max_bytes as f64),
        ) {
            Ok(value) => value,
            Err(error) if is_missing(&error) => {
                return Err(Error::MissingResource(path.as_str().into()));
            }
            Err(error) => {
                self.0.record(error);
                return Err(callback_failed("resolve"));
            }
        };
        if value.is_undefined() || value.is_null() {
            return Err(Error::MissingResource(path.as_str().into()));
        }
        let bytes = match conv::bytes_from_js(&value, "resolve() result") {
            Ok(bytes) => bytes,
            Err(error) => {
                self.0.record(error);
                return Err(callback_failed("resolve"));
            }
        };
        if bytes.len() > max_bytes {
            return Err(limit("resource bytes", max_bytes));
        }
        Ok(bytes)
    }

    fn list(
        &self,
        directory: Option<&ResourcePath>,
        max_entries: usize,
        max_bytes: usize,
    ) -> Result<Vec<ResourcePath>> {
        let Some((this, list)) = self.target("list") else {
            return Err(Error::InvalidInput(
                "resource enumeration is unavailable".into(),
            ));
        };
        let dir = directory.map_or(JsValue::NULL, |d| JsValue::from_str(d.as_str()));
        let value = list
            .call3(
                &this,
                &dir,
                &JsValue::from_f64(max_entries as f64),
                &JsValue::from_f64(max_bytes as f64),
            )
            .map_err(|error| {
                self.0.record(error);
                callback_failed("list")
            })?;
        let Some(names) = value.dyn_ref::<Array>() else {
            return Err(Error::InvalidInput(
                "resource list() must return an array of strings".into(),
            ));
        };
        if names.length() as usize > max_entries {
            return Err(limit("directory entries", max_entries));
        }
        names
            .iter()
            .map(|name| {
                let name = name.as_string().ok_or_else(|| {
                    Error::InvalidInput("resource list() must return an array of strings".into())
                })?;
                ResourcePath::new(name)
            })
            .collect()
    }
}

/// A `resources` argument kept beyond one call (by `Input`): named files or
/// the JS resolver object itself.
#[derive(Clone)]
pub enum ResourcesSpec {
    /// No sidecars (`NoResources`).
    None,
    /// Named in-memory files (a `ResourceSet`'s shared storage, or a copy of
    /// an object/`Map`).
    Files(Files),
    /// A JS resolver function or object.
    Js(JsValue),
}

impl ResourcesSpec {
    /// Convert `value` (`undefined`/`null`, `ResourceSet`, object, `Map`,
    /// function or resolver object).
    pub fn new(value: &JsValue) -> JsResult<Self> {
        if value.is_undefined() || value.is_null() {
            return Ok(Self::None);
        }
        if value.is_function() {
            return Ok(Self::Js(value.clone()));
        }
        match handle_arg(value)? {
            Some(Handle::ResourceSet(files)) => return Ok(Self::Files(files)),
            Some(other) => {
                return Err(crate::model_args::wrong_handle(
                    "resources",
                    "ResourceSet",
                    &other,
                ));
            }
            None => {}
        }
        if let Some(map) = value.dyn_ref::<Map>() {
            let mut files = BTreeMap::new();
            let mut failure = None;
            map.for_each(&mut |bytes, key| {
                if failure.is_some() {
                    return;
                }
                match file_entry(&key, &bytes) {
                    Ok((name, data)) => {
                        files.insert(name, data);
                    }
                    Err(error) => failure = Some(error),
                }
            });
            return match failure {
                Some(error) => Err(error),
                None => Ok(Self::Files(Arc::new(RwLock::new(files)))),
            };
        }
        let Some(object) = value.dyn_ref::<Object>() else {
            return Err(errors::invalid(
                "resources: expected a ResourceSet, an object or Map of name to bytes, a resolver function, or an object with resolve()",
            ));
        };
        if Reflect::get(object, &JsValue::from_str("resolve"))?.is_function() {
            return Ok(Self::Js(value.clone()));
        }
        Ok(Self::Files(Arc::new(RwLock::new(files_from_object(
            object,
        )?))))
    }

    /// The per-call resolver (parks a JS resolver for the call).
    pub fn scope(&self) -> ResourcesArg {
        match self {
            Self::None => ResourcesArg::None,
            Self::Files(files) => ResourcesArg::Files(SetResolver(files.clone())),
            Self::Js(value) => {
                let scope = CallbackScope::new(value.clone());
                let token = scope.token();
                ResourcesArg::Js(scope, JsResolver(token))
            }
        }
    }
}

/// A converted `resources` argument for one call; keep it alive for the
/// whole call and pass the call's result through [`ResourcesArg::finish`].
pub enum ResourcesArg {
    /// No sidecars (`NoResources`).
    None,
    /// Named in-memory files.
    Files(SetResolver),
    /// A JS resolver, parked for the call.
    Js(CallbackScope, JsResolver),
}

impl ResourcesArg {
    /// Convert `value` (`undefined`/`null`, `ResourceSet`, object, `Map`,
    /// function or resolver object).
    pub fn new(value: &JsValue) -> JsResult<Self> {
        Ok(ResourcesSpec::new(value)?.scope())
    }

    /// The resolver for `Input::with_resources`.
    pub fn resolver(&self) -> &dyn ResourceResolver {
        static NONE: NoResources = NoResources;
        match self {
            Self::None => &NONE,
            Self::Files(files) => files,
            Self::Js(_, resolver) => resolver,
        }
    }

    /// The call's result, or the exception a JS resolver threw.
    pub fn finish<T>(&self, result: JsResult<T>) -> JsResult<T> {
        match self {
            Self::Js(scope, _) => scope.rethrow(result),
            _ => result,
        }
    }
}

fn file_entry(key: &JsValue, bytes: &JsValue) -> JsResult<(ResourcePath, Vec<u8>)> {
    let name = key
        .as_string()
        .ok_or_else(|| errors::invalid("resources: names must be strings"))?;
    let path = path_arg(&name)?;
    let data = conv::bytes_from_js(bytes, &format!("resources[{name:?}]"))?;
    Ok((path, data))
}

fn files_from_object(object: &Object) -> JsResult<BTreeMap<ResourcePath, Vec<u8>>> {
    let mut files = BTreeMap::new();
    for entry in Object::entries(object).iter() {
        let entry: Array = entry.unchecked_into();
        let (name, data) = file_entry(&entry.get(0), &entry.get(1))?;
        files.insert(name, data);
    }
    Ok(files)
}

fn files_from_js(value: &JsValue) -> JsResult<BTreeMap<ResourcePath, Vec<u8>>> {
    if value.is_undefined() || value.is_null() {
        return Ok(BTreeMap::new());
    }
    match ResourcesSpec::new(value)? {
        ResourcesSpec::Files(files) => Ok(files.try_read().map_err(|_| busy_set())?.clone()),
        _ => Err(errors::invalid(
            "ResourceSet: expected an object or Map of name to bytes",
        )),
    }
}

fn busy_set() -> JsValue {
    errors::invalid("ResourceSet is in use by the running fontpixel call")
}

// ---------------------------------------------------------------- ResourceSet

/// Named in-memory files offered to readers as sidecars (a
/// `ResourceResolver`). Paths are validated `ResourcePath`s (`/`-separated,
/// relative, no `.`/`..`). A set passed as `resources` is shared, not
/// copied.
#[wasm_bindgen(js_name = ResourceSet)]
pub struct JsResourceSet {
    files: Files,
}

impl JsResourceSet {
    fn read<R>(&self, f: impl FnOnce(&BTreeMap<ResourcePath, Vec<u8>>) -> R) -> JsResult<R> {
        Ok(f(&*self.files.try_read().map_err(|_| busy_set())?))
    }
    fn write<R>(&self, f: impl FnOnce(&mut BTreeMap<ResourcePath, Vec<u8>>) -> R) -> JsResult<R> {
        Ok(f(&mut *self.files.try_write().map_err(|_| busy_set())?))
    }
}

#[wasm_bindgen(js_class = ResourceSet)]
impl JsResourceSet {
    /// A set holding copies of `files` (an object or `Map` of path → bytes).
    #[wasm_bindgen(constructor)]
    pub fn constructor(
        #[wasm_bindgen(
            unchecked_optional_param_type = "Record<string, BytesLike> | Map<string, BytesLike>"
        )]
        files: Option<JsValue>,
    ) -> JsResult<JsResourceSet> {
        let files = files_from_js(&files.unwrap_or(JsValue::UNDEFINED))?;
        Ok(Self {
            files: Arc::new(RwLock::new(files)),
        })
    }
    /// Add or replace a file (bytes are copied).
    pub fn insert(
        &self,
        path: &str,
        #[wasm_bindgen(unchecked_param_type = "BytesLike")] data: JsValue,
    ) -> JsResult<()> {
        let path = path_arg(path)?;
        let data = conv::bytes_from_js(&data, "data")?;
        self.write(|files| {
            files.insert(path, data);
        })
    }
    /// Remove a file; returns whether it existed.
    pub fn remove(&self, path: &str) -> JsResult<bool> {
        let path = path_arg(path)?;
        self.write(|files| files.remove(&path).is_some())
    }
    /// A copy of one file, or `undefined`.
    pub fn get(&self, path: &str) -> JsResult<Option<Uint8Array>> {
        let path = path_arg(path)?;
        self.read(|files| files.get(&path).map(|b| conv::bytes_to_js(b)))
    }
    /// Whether `path` is present (`false` for invalid paths).
    pub fn has(&self, path: &str) -> JsResult<bool> {
        let Ok(path) = ResourcePath::new(path) else {
            return Ok(false);
        };
        self.read(|files| files.contains_key(&path))
    }
    /// Every path in sorted order.
    pub fn paths(&self) -> JsResult<Vec<String>> {
        self.read(|files| files.keys().map(|p| p.as_str().to_owned()).collect())
    }
    /// Number of files.
    #[wasm_bindgen(getter)]
    pub fn length(&self) -> JsResult<usize> {
        self.read(BTreeMap::len)
    }
    /// Resolve like a reader would (`ResourceResolver::resolve`):
    /// `MissingResourceError` when absent, `ResourceLimitError` above
    /// `maxBytes` (default unbounded).
    pub fn resolve(
        &self,
        path: &str,
        #[wasm_bindgen(js_name = maxBytes)] max_bytes: Option<f64>,
    ) -> JsResult<Uint8Array> {
        let path = path_arg(path)?;
        let max = max_or_unbounded(max_bytes, "maxBytes")?;
        let bytes = SetResolver(self.files.clone()).resolve(&path, max).js()?;
        Ok(conv::bytes_to_js(&bytes))
    }
    /// Immediate files of `directory` (`null`/`undefined`: the root) in
    /// sorted order (`ResourceResolver::list`), bounded by `maxEntries` and
    /// `maxBytes` (default unbounded).
    pub fn list(
        &self,
        directory: Option<String>,
        #[wasm_bindgen(js_name = maxEntries)] max_entries: Option<f64>,
        #[wasm_bindgen(js_name = maxBytes)] max_bytes: Option<f64>,
    ) -> JsResult<Vec<String>> {
        let directory = directory.as_deref().map(path_arg).transpose()?;
        let paths = SetResolver(self.files.clone())
            .list(
                directory.as_ref(),
                max_or_unbounded(max_entries, "maxEntries")?,
                max_or_unbounded(max_bytes, "maxBytes")?,
            )
            .js()?;
        Ok(paths.into_iter().map(|p| p.as_str().to_owned()).collect())
    }
    /// `ResourceSet(["a.bin", …])`.
    #[wasm_bindgen(js_name = toString)]
    pub fn to_string_js(&self) -> JsResult<String> {
        Ok(format!("ResourceSet({:?})", self.paths()?))
    }
    /// Internal: see `FontSet.__fpStash`.
    #[doc(hidden)]
    #[wasm_bindgen(js_name = __fpStash, skip_typescript)]
    pub fn fp_stash(&self) {
        stash(Handle::ResourceSet(self.files.clone()));
    }
}

fn max_or_unbounded(value: Option<f64>, what: &str) -> JsResult<usize> {
    value.map_or(Ok(usize::MAX), |v| conv::int(v, what))
}

// ---------------------------------------------------------------- Input

/// Owned parts of a `fontpixel::resources::Input`.
#[derive(Clone)]
pub struct InputParts {
    /// Primary bytes.
    pub data: Rc<Vec<u8>>,
    /// Filename hint.
    pub name: Option<String>,
    /// Sidecar resources.
    pub resources: ResourcesSpec,
    /// Directory entry point (no primary bytes).
    pub directory: bool,
}

impl InputParts {
    /// Standalone input over `data`.
    pub fn bytes(data: Vec<u8>) -> Self {
        Self {
            data: Rc::new(data),
            name: None,
            resources: ResourcesSpec::None,
            directory: false,
        }
    }

    /// Run `f` with the `Input` these parts describe, over `resolver`.
    pub fn with_resolver<R>(
        &self,
        resolver: &dyn ResourceResolver,
        f: impl FnOnce(&Input<'_>) -> R,
    ) -> R {
        let input = if self.directory {
            Input::directory(self.name.as_deref().unwrap_or(""), resolver)
        } else {
            let mut input = Input::new(&self.data).with_resources(resolver);
            if let Some(name) = &self.name {
                input = input.with_name(name);
            }
            input
        };
        f(&input)
    }

    /// Run `f` with the `Input` (a JS resolver is parked for the call and
    /// its first exception rethrown, see [`ResourcesArg::finish`]).
    pub fn with<R>(&self, f: impl FnOnce(&Input<'_>) -> JsResult<R>) -> JsResult<R> {
        let resources = self.resources.scope();
        let result = self.with_resolver(resources.resolver(), f);
        resources.finish(result)
    }
}

/// An `Input` handle or plain bytes (standalone input).
pub fn input_arg(value: &JsValue) -> JsResult<InputParts> {
    match handle_arg(value)? {
        Some(Handle::Input(parts)) => Ok(parts),
        Some(other) => Err(crate::model_args::wrong_handle("input", "Input", &other)),
        None => Ok(InputParts::bytes(conv::bytes_from_js(value, "input")?)),
    }
}

/// Primary bytes, a filename hint and sidecar resources
/// (`fontpixel::resources::Input`). Immutable: `withName` and
/// `withResources` return new inputs.
#[wasm_bindgen(js_name = Input)]
pub struct JsInput {
    parts: InputParts,
}

impl JsInput {
    /// The owned input description.
    pub fn parts(&self) -> &InputParts {
        &self.parts
    }
}

#[wasm_bindgen(js_class = Input)]
impl JsInput {
    /// A standalone input over a copy of `data` (`Input::new`), optionally
    /// with a filename hint (`options.name`, `Input::with_name`) and sidecars
    /// (`options.resources`, `Input::with_resources`).
    #[wasm_bindgen(constructor)]
    pub fn constructor(
        #[wasm_bindgen(unchecked_param_type = "BytesLike")] data: JsValue,
        #[wasm_bindgen(unchecked_optional_param_type = "InputArgs")] options: Option<JsValue>,
    ) -> JsResult<JsInput> {
        let options = conv::Options::new(
            &options.unwrap_or(JsValue::UNDEFINED),
            &["name", "resources"],
            "Input options",
        )?;
        let data = conv::bytes_from_js(&data, "data")?;
        Ok(Self {
            parts: InputParts {
                data: Rc::new(data),
                name: options.string("name")?,
                resources: ResourcesSpec::new(
                    &options.get("resources")?.unwrap_or(JsValue::UNDEFINED),
                )?,
                directory: false,
            },
        })
    }
    /// A directory entry point without primary bytes; `resources` is rooted
    /// at the directory (`Input::directory`).
    pub fn directory(
        name: String,
        #[wasm_bindgen(unchecked_param_type = "ResourcesLike")] resources: JsValue,
    ) -> JsResult<JsInput> {
        Ok(Self {
            parts: InputParts {
                data: Rc::new(Vec::new()),
                name: Some(name),
                resources: ResourcesSpec::new(&resources)?,
                directory: true,
            },
        })
    }
    /// A copy with a filename hint (`Input::with_name`).
    #[wasm_bindgen(js_name = withName)]
    pub fn with_name(&self, name: String) -> JsInput {
        let mut parts = self.parts.clone();
        parts.name = Some(name);
        Self { parts }
    }
    /// A copy with sidecar resources (`Input::with_resources`).
    #[wasm_bindgen(js_name = withResources)]
    pub fn with_resources(
        &self,
        #[wasm_bindgen(unchecked_param_type = "ResourcesLike")] resources: JsValue,
    ) -> JsResult<JsInput> {
        let mut parts = self.parts.clone();
        parts.resources = ResourcesSpec::new(&resources)?;
        Ok(Self { parts })
    }
    /// The filename hint.
    #[wasm_bindgen(getter)]
    pub fn name(&self) -> Option<String> {
        self.parts.name.clone()
    }
    /// Whether this is a directory entry point.
    #[wasm_bindgen(getter, js_name = isDirectory)]
    pub fn is_directory(&self) -> bool {
        self.parts.directory
    }
    /// A copy of the primary bytes.
    #[wasm_bindgen(getter)]
    pub fn data(&self) -> Uint8Array {
        conv::bytes_to_js(&self.parts.data)
    }
    /// Length of the primary bytes.
    #[wasm_bindgen(getter)]
    pub fn length(&self) -> usize {
        self.parts.data.len()
    }
    /// `Input(bytes=3, name="font.bdf", directory=false)`.
    #[wasm_bindgen(js_name = toString)]
    pub fn to_string_js(&self) -> String {
        format!(
            "Input(bytes={}, name={}, directory={})",
            self.parts.data.len(),
            self.parts
                .name
                .as_deref()
                .map_or_else(|| "null".to_owned(), |n| format!("{n:?}")),
            self.parts.directory
        )
    }
    /// Internal: see `FontSet.__fpStash`.
    #[doc(hidden)]
    #[wasm_bindgen(js_name = __fpStash, skip_typescript)]
    pub fn fp_stash(&self) {
        stash(Handle::Input(self.parts.clone()));
    }
}

// ---------------------------------------------------------------- ReadContext

/// One operation of a `ReadContext`, replayed on every later call.
enum Op {
    Resource(ResourcePath, usize),
    List(Option<ResourcePath>, usize, usize),
}

/// What the resolver returned for one call (in call order).
enum Recorded {
    /// `Some(capacity)` of the returned bytes, `None` for a failure.
    Resolve(Option<usize>),
    /// The returned listing, `None` for a failure.
    List(Option<Vec<ResourcePath>>),
}

#[derive(Default)]
struct Replay {
    ops: Vec<Op>,
    calls: Vec<Recorded>,
}

/// Answers the first `recorded` calls from their recorded results, then
/// calls the real resolver and records.
struct Recorder<'a> {
    inner: &'a dyn ResourceResolver,
    log: Mutex<Vec<Recorded>>,
    cursor: AtomicUsize,
}

fn replayed() -> Error {
    Error::InvalidInput("replayed resource failure".into())
}

impl Recorder<'_> {
    fn next(&self) -> (usize, bool) {
        let index = self.cursor.fetch_add(1, Ordering::Relaxed);
        let recorded = self.log.lock().map_or(0, |log| log.len());
        (index, index < recorded)
    }
    fn record(&self, entry: Recorded) {
        if let Ok(mut log) = self.log.lock() {
            log.push(entry);
        }
    }
}

impl ResourceResolver for Recorder<'_> {
    fn resolve(&self, path: &ResourcePath, max_bytes: usize) -> Result<Vec<u8>> {
        let (index, replay) = self.next();
        if replay {
            let log = self.log.lock().map_err(|_| replayed())?;
            return match log.get(index) {
                Some(Recorded::Resolve(Some(capacity))) => Ok(Vec::with_capacity(*capacity)),
                _ => Err(replayed()),
            };
        }
        let result = self.inner.resolve(path, max_bytes);
        self.record(Recorded::Resolve(result.as_ref().ok().map(Vec::capacity)));
        result
    }

    fn list(
        &self,
        directory: Option<&ResourcePath>,
        max_entries: usize,
        max_bytes: usize,
    ) -> Result<Vec<ResourcePath>> {
        let (index, replay) = self.next();
        if replay {
            let log = self.log.lock().map_err(|_| replayed())?;
            return match log.get(index) {
                Some(Recorded::List(Some(paths))) => Ok(paths.clone()),
                _ => Err(replayed()),
            };
        }
        let result = self.inner.list(directory, max_entries, max_bytes);
        self.record(Recorded::List(result.as_ref().ok().cloned()));
        result
    }
}

/// Shared resource accounting for one input
/// (`fontpixel::resources::ReadContext`).
///
/// Construction validates the primary input against `limits` and decodes a
/// gzip transport envelope. Each `resource*`/`listResourcesLimited` call
/// counts against `max_resources` and the aggregate byte budget exactly as
/// codecs see it (failed resolutions count too). See the module
/// documentation for how the context is kept without borrowing (replay of
/// recorded results; the resolver is called once per operation).
#[wasm_bindgen(js_name = ReadContext)]
pub struct JsReadContext {
    parts: InputParts,
    limits: ResourceLimits,
    replay: RefCell<Replay>,
}

impl JsReadContext {
    /// Rebuild the context, replay every recorded operation and run `op`
    /// (if any) live; the replay log is held for the whole call.
    fn run<R>(
        &self,
        op: Option<Op>,
        f: impl FnOnce(&mut ReadContext<'_>, Option<&Op>) -> Result<R>,
    ) -> JsResult<R> {
        let mut replay = crate::handles::borrow_mut(&self.replay, "ReadContext")?;
        let resources = self.parts.resources.scope();
        let recorder = Recorder {
            inner: resources.resolver(),
            log: Mutex::new(std::mem::take(&mut replay.calls)),
            cursor: AtomicUsize::new(0),
        };
        let result = self.parts.with_resolver(&recorder, |input| {
            let mut ctx = ReadContext::new(input, &self.limits)?;
            for past in &replay.ops {
                // Results are already known; only the accounting matters.
                let _ = apply(&mut ctx, past);
            }
            f(&mut ctx, op.as_ref())
        });
        replay.calls = recorder.log.into_inner().unwrap_or_default();
        if let Some(op) = op {
            replay.ops.push(op);
        }
        resources.finish(result.js())
    }
}

enum Outcome {
    Bytes(Vec<u8>),
    Paths(Vec<ResourcePath>),
}

fn apply(ctx: &mut ReadContext<'_>, op: &Op) -> Result<Outcome> {
    match op {
        Op::Resource(path, max) => ctx.resource_limited(path, *max).map(Outcome::Bytes),
        Op::List(dir, entries, bytes) => ctx
            .list_resources_limited(dir.as_ref(), *entries, *bytes)
            .map(Outcome::Paths),
    }
}

#[wasm_bindgen(js_class = ReadContext)]
impl JsReadContext {
    /// `new ReadContext(input, limits?)`: `input` is an `Input` or bytes;
    /// `limits` a `ResourceLimits` object (`ReadContext::new`).
    #[wasm_bindgen(constructor)]
    pub fn constructor(
        #[wasm_bindgen(unchecked_param_type = "Input | BytesLike")] input: JsValue,
        #[wasm_bindgen(unchecked_optional_param_type = "ResourceLimits")] limits: Option<JsValue>,
    ) -> JsResult<JsReadContext> {
        let parts = input_arg(&input)?;
        let limits: ResourceLimits =
            conv::from_js_or_default(&limits.unwrap_or(JsValue::UNDEFINED))?;
        parts
            .with_resolver(&NoResources, |input| {
                ReadContext::new(input, &limits).map(|_| ())
            })
            .js()?;
        Ok(Self {
            parts,
            limits,
            replay: RefCell::new(Replay::default()),
        })
    }
    /// Primary bytes after transport decoding (`ReadContext::bytes`).
    pub fn bytes(&self) -> JsResult<Uint8Array> {
        self.parts
            .with_resolver(&NoResources, |input| {
                ReadContext::new(input, &self.limits).map(|ctx| conv::bytes_to_js(ctx.bytes()))
            })
            .js()
    }
    /// Bytes retained so far, including resolved resources
    /// (`ReadContext::retained_input_bytes`).
    #[wasm_bindgen(js_name = retainedInputBytes)]
    pub fn retained_input_bytes(&self) -> JsResult<usize> {
        self.run(None, |ctx, _| Ok(ctx.retained_input_bytes()))
    }
    /// The filename hint (`ReadContext::name`).
    #[wasm_bindgen(getter)]
    pub fn name(&self) -> Option<String> {
        self.parts.name.clone()
    }
    /// Whether the input is a directory entry point
    /// (`ReadContext::is_directory`).
    #[wasm_bindgen(getter, js_name = isDirectory)]
    pub fn is_directory(&self) -> bool {
        self.parts.directory
    }
    /// The budgets in force (`ReadContext::limits`).
    #[wasm_bindgen(getter, unchecked_return_type = "ResourceLimits")]
    pub fn limits(&self) -> JsResult<JsValue> {
        conv::to_js(&self.limits)
    }
    /// Resolve a sidecar under the remaining budget (`ReadContext::resource`).
    pub fn resource(&self, path: &str) -> JsResult<Uint8Array> {
        self.resource_with(path, usize::MAX)
    }
    /// Resolve with an extra byte allowance (`ReadContext::resource_limited`).
    #[wasm_bindgen(js_name = resourceLimited)]
    pub fn resource_limited(
        &self,
        path: &str,
        #[wasm_bindgen(js_name = maxBytes)] max_bytes: f64,
    ) -> JsResult<Uint8Array> {
        self.resource_with(path, conv::int(max_bytes, "maxBytes")?)
    }
    /// List the immediate files of `directory` (`null`: the root) with
    /// shared accounting (`ReadContext::list_resources_limited`).
    #[wasm_bindgen(js_name = listResourcesLimited)]
    pub fn list_resources_limited(
        &self,
        directory: Option<String>,
        #[wasm_bindgen(js_name = maxEntries)] max_entries: f64,
        #[wasm_bindgen(js_name = maxBytes)] max_bytes: f64,
    ) -> JsResult<Vec<String>> {
        let directory = directory.as_deref().map(path_arg).transpose()?;
        let op = Op::List(
            directory,
            conv::int(max_entries, "maxEntries")?,
            conv::int(max_bytes, "maxBytes")?,
        );
        let out = self.run(Some(op), |ctx, op| apply(ctx, op.expect("an operation")))?;
        match out {
            Outcome::Paths(paths) => Ok(paths.into_iter().map(|p| p.as_str().to_owned()).collect()),
            Outcome::Bytes(_) => unreachable!("a listing returns paths"),
        }
    }
    /// `ReadContext(name="p.fnt", directory=false)`.
    #[wasm_bindgen(js_name = toString)]
    pub fn to_string_js(&self) -> String {
        format!(
            "ReadContext(name={}, directory={})",
            self.parts
                .name
                .as_deref()
                .map_or_else(|| "null".to_owned(), |n| format!("{n:?}")),
            self.parts.directory
        )
    }
}

impl JsReadContext {
    fn resource_with(&self, path: &str, max_bytes: usize) -> JsResult<Uint8Array> {
        let op = Op::Resource(path_arg(path)?, max_bytes);
        let out = self.run(Some(op), |ctx, op| apply(ctx, op.expect("an operation")))?;
        match out {
            Outcome::Bytes(bytes) => Ok(conv::bytes_to_js(&bytes)),
            Outcome::Paths(_) => unreachable!("a resolution returns bytes"),
        }
    }
}

// ---------------------------------------------------------------- outputs

/// One in-memory output file (`fontpixel::resources::OutputAsset`).
#[wasm_bindgen(js_name = OutputAsset)]
pub struct JsOutputAsset {
    asset: OutputAsset,
}

impl JsOutputAsset {
    /// Wrap an asset.
    pub fn new(asset: OutputAsset) -> Self {
        Self { asset }
    }
}

#[wasm_bindgen(js_class = OutputAsset)]
impl JsOutputAsset {
    /// `new OutputAsset(path, data)` (bytes copied; `path` validated).
    #[wasm_bindgen(constructor)]
    pub fn constructor(
        path: &str,
        #[wasm_bindgen(unchecked_param_type = "BytesLike")] data: JsValue,
    ) -> JsResult<JsOutputAsset> {
        Ok(Self::new(OutputAsset {
            path: path_arg(path)?,
            bytes: conv::bytes_from_js(&data, "data")?,
        }))
    }
    /// The validated relative path.
    #[wasm_bindgen(getter)]
    pub fn path(&self) -> String {
        self.asset.path.as_str().to_owned()
    }
    /// A copy of the bytes.
    #[wasm_bindgen(getter)]
    pub fn data(&self) -> Uint8Array {
        conv::bytes_to_js(&self.asset.bytes)
    }
    /// Number of bytes.
    #[wasm_bindgen(getter)]
    pub fn length(&self) -> usize {
        self.asset.bytes.len()
    }
    /// Whether `other` is an `OutputAsset` with the same path and bytes.
    pub fn equals(
        &self,
        #[wasm_bindgen(unchecked_param_type = "unknown")] other: JsValue,
    ) -> JsResult<bool> {
        Ok(matches!(handle_arg(&other)?, Some(Handle::OutputAsset(a)) if a == self.asset))
    }
    /// `OutputAsset(path="font.fnt", bytes=1)`.
    #[wasm_bindgen(js_name = toString)]
    pub fn to_string_js(&self) -> String {
        format!(
            "OutputAsset(path={:?}, bytes={})",
            self.asset.path.as_str(),
            self.asset.bytes.len()
        )
    }
    /// Internal: see `FontSet.__fpStash`.
    #[doc(hidden)]
    #[wasm_bindgen(js_name = __fpStash, skip_typescript)]
    pub fn fp_stash(&self) {
        stash(Handle::OutputAsset(self.asset.clone()));
    }
}

fn asset_arg(value: &JsValue) -> JsResult<OutputAsset> {
    match handle_arg(value)? {
        Some(Handle::OutputAsset(asset)) => return Ok(asset),
        Some(other) => {
            return Err(crate::model_args::wrong_handle(
                "asset",
                "OutputAsset",
                &other,
            ));
        }
        None => {}
    }
    if Array::is_array(value) {
        let pair: Array = value.clone().unchecked_into();
        if pair.length() == 2 {
            let path = pair
                .get(0)
                .as_string()
                .ok_or_else(|| errors::invalid("asset: the path must be a string"))?;
            return Ok(OutputAsset {
                path: path_arg(&path)?,
                bytes: conv::bytes_from_js(&pair.get(1), "asset bytes")?,
            });
        }
    }
    if value.is_object() && !Array::is_array(value) {
        let options = conv::Options::new(value, &["path", "data"], "asset")?;
        let path = options
            .string("path")?
            .ok_or_else(|| errors::invalid("asset: path must be a string"))?;
        let data = Reflect::get(value, &JsValue::from_str("data"))?;
        return Ok(OutputAsset {
            path: path_arg(&path)?,
            bytes: conv::bytes_from_js(&data, "asset data")?,
        });
    }
    Err(errors::invalid(
        "asset: expected an OutputAsset, {path, data} or a [path, bytes] pair",
    ))
}

/// In-memory output files with a primary entry point
/// (`fontpixel::resources::OutputBundle`); immutable.
#[wasm_bindgen(js_name = OutputBundle)]
pub struct JsOutputBundle {
    bundle: OutputBundle,
}

impl JsOutputBundle {
    /// Wrap a bundle.
    pub fn new(bundle: OutputBundle) -> Self {
        Self { bundle }
    }
    /// The wrapped bundle.
    pub fn bundle(&self) -> &OutputBundle {
        &self.bundle
    }
    fn asset_handles(&self) -> Vec<JsOutputAsset> {
        self.bundle
            .assets()
            .iter()
            .map(|a| JsOutputAsset::new(a.clone()))
            .collect()
    }
}

#[wasm_bindgen(js_class = OutputBundle)]
impl JsOutputBundle {
    /// `new OutputBundle(primary, assets, {directory})`: `assets` holds
    /// `OutputAsset`s or `[path, bytes]` pairs. A file bundle must contain
    /// `primary` (`OutputBundle::new`); a directory bundle names its
    /// directory and holds files beneath it (`OutputBundle::directory`).
    #[wasm_bindgen(constructor)]
    pub fn constructor(
        primary: &str,
        #[wasm_bindgen(
            unchecked_param_type = "(OutputAsset | OutputAssetLike | [string, BytesLike])[]"
        )]
        assets: JsValue,
        #[wasm_bindgen(unchecked_optional_param_type = "{ directory?: boolean }")] options: Option<
            JsValue,
        >,
    ) -> JsResult<JsOutputBundle> {
        let options = conv::Options::new(
            &options.unwrap_or(JsValue::UNDEFINED),
            &["directory"],
            "OutputBundle options",
        )?;
        let primary = path_arg(primary)?;
        let assets = array_arg(&assets, "assets")?
            .iter()
            .map(|a| asset_arg(&a))
            .collect::<JsResult<Vec<_>>>()?;
        let bundle = if options.bool("directory")?.unwrap_or(false) {
            OutputBundle::directory(primary, assets)
        } else {
            OutputBundle::new(primary, assets)
        };
        Ok(Self::new(bundle.js()?))
    }
    /// A one-file bundle (`OutputBundle::single`).
    pub fn single(
        path: &str,
        #[wasm_bindgen(unchecked_param_type = "BytesLike")] data: JsValue,
    ) -> JsResult<JsOutputBundle> {
        Ok(Self::new(OutputBundle::single(
            path_arg(path)?,
            conv::bytes_from_js(&data, "data")?,
        )))
    }
    /// From the plain-data form (`{primary, assets: [{path, bytes}],
    /// directory}`), validated like the constructors.
    #[wasm_bindgen(js_name = fromDict)]
    pub fn from_dict(
        #[wasm_bindgen(unchecked_param_type = "OutputBundleDict")] data: JsValue,
    ) -> JsResult<JsOutputBundle> {
        Ok(Self::new(conv::from_js(&data)?))
    }
    /// The plain-data form (`bytes` as number arrays).
    #[wasm_bindgen(js_name = toDict, unchecked_return_type = "OutputBundleDict")]
    pub fn to_dict(&self) -> JsResult<JsValue> {
        conv::to_js(&self.bundle)
    }
    /// Whether the primary path names a directory
    /// (`OutputBundle::is_directory`).
    #[wasm_bindgen(getter, js_name = isDirectory)]
    pub fn is_directory(&self) -> bool {
        self.bundle.is_directory()
    }
    /// The primary path (`OutputBundle::primary`).
    #[wasm_bindgen(getter)]
    pub fn primary(&self) -> String {
        self.bundle.primary().as_str().to_owned()
    }
    /// Copies of every asset, sorted by path (`OutputBundle::assets`).
    #[wasm_bindgen(getter)]
    pub fn assets(&self) -> Vec<JsOutputAsset> {
        self.asset_handles()
    }
    /// The assets as an array (`OutputBundle::into_assets`; the bundle stays
    /// usable).
    #[wasm_bindgen(js_name = intoAssets)]
    pub fn into_assets(&self) -> Vec<JsOutputAsset> {
        self.asset_handles()
    }
    /// The only file's bytes; `MultipleAssetsError` for several, and
    /// `InvalidInputError` for a directory (`OutputBundle::into_single`).
    #[wasm_bindgen(js_name = intoSingle)]
    pub fn into_single(&self) -> JsResult<Uint8Array> {
        let bytes = self.bundle.clone().into_single().js()?;
        Ok(conv::bytes_to_js(&bytes))
    }
    /// Retained bytes of the bundle, checked against `maxBytes` (default
    /// unbounded) (`OutputBundle::retained_bytes`; wasm32 sizes).
    #[wasm_bindgen(js_name = retainedBytes)]
    pub fn retained_bytes(
        &self,
        #[wasm_bindgen(js_name = maxBytes)] max_bytes: Option<f64>,
    ) -> JsResult<usize> {
        self.bundle
            .retained_bytes(max_or_unbounded(max_bytes, "maxBytes")?)
            .js()
    }
    /// Number of assets.
    #[wasm_bindgen(getter)]
    pub fn length(&self) -> usize {
        self.bundle.assets().len()
    }
    /// Whether `other` is an equal `OutputBundle`.
    pub fn equals(
        &self,
        #[wasm_bindgen(unchecked_param_type = "unknown")] other: JsValue,
    ) -> JsResult<bool> {
        Ok(matches!(handle_arg(&other)?, Some(Handle::OutputBundle(b)) if b == self.bundle))
    }
    /// `OutputBundle(primary="font.fnt", assets=2, directory=false)`.
    #[wasm_bindgen(js_name = toString)]
    pub fn to_string_js(&self) -> String {
        format!(
            "OutputBundle(primary={:?}, assets={}, directory={})",
            self.bundle.primary().as_str(),
            self.bundle.assets().len(),
            self.bundle.is_directory()
        )
    }
    /// Internal: see `FontSet.__fpStash`.
    #[doc(hidden)]
    #[wasm_bindgen(js_name = __fpStash, skip_typescript)]
    pub fn fp_stash(&self) {
        stash(Handle::OutputBundle(self.bundle.clone()));
    }
}

/// A writer result: the output bundle and its report
/// (`fontpixel::resources::Written`).
#[wasm_bindgen(js_name = Written)]
pub struct JsWritten {
    written: Written,
}

impl JsWritten {
    /// Wrap a writer result.
    pub fn new(written: Written) -> Self {
        Self { written }
    }
    /// The wrapped result.
    pub fn written(&self) -> &Written {
        &self.written
    }
}

#[wasm_bindgen(js_class = Written)]
impl JsWritten {
    /// Decode the complete plain-data writer result.
    #[wasm_bindgen(js_name = fromDict)]
    pub fn from_dict(
        #[wasm_bindgen(unchecked_param_type = "WrittenDict")] data: JsValue,
    ) -> JsResult<JsWritten> {
        Ok(Self::new(conv::from_js(&data)?))
    }
    /// Internal: borrow the writer result as a union argument.
    #[doc(hidden)]
    #[wasm_bindgen(js_name = __fpStash, skip_typescript)]
    pub fn fp_stash(&self) {
        stash(Handle::Written(self.written.clone()));
    }
    /// `new Written(output, report?)` (`report` defaults to no entries).
    #[wasm_bindgen(constructor)]
    pub fn constructor(
        #[wasm_bindgen(unchecked_param_type = "OutputBundle | OutputBundleDict")] output: JsValue,
        #[wasm_bindgen(unchecked_optional_param_type = "Report")] report: Option<JsValue>,
    ) -> JsResult<JsWritten> {
        let output = match handle_arg(&output)? {
            Some(Handle::OutputBundle(bundle)) => bundle,
            Some(other) => {
                return Err(crate::model_args::wrong_handle(
                    "output",
                    "OutputBundle",
                    &other,
                ));
            }
            None => conv::from_js(&output)?,
        };
        let report: Report = conv::from_js_or_default(&report.unwrap_or(JsValue::UNDEFINED))?;
        Ok(Self::new(Written { output, report }))
    }
    /// The output files (an `OutputBundle`).
    #[wasm_bindgen(getter)]
    pub fn output(&self) -> JsOutputBundle {
        JsOutputBundle::new(self.written.output.clone())
    }
    /// The report (plain data, every entry).
    #[wasm_bindgen(getter, unchecked_return_type = "Report")]
    pub fn report(&self) -> JsResult<JsValue> {
        conv::to_js(&self.written.report)
    }
    /// The plain-data form (`{output, report}`).
    #[wasm_bindgen(js_name = toDict, unchecked_return_type = "WrittenDict")]
    pub fn to_dict(&self) -> JsResult<JsValue> {
        conv::to_js(&self.written)
    }
    /// `Written(output=OutputBundle(…), entries=0)`.
    #[wasm_bindgen(js_name = toString)]
    pub fn to_string_js(&self) -> String {
        format!(
            "Written(output={}, entries={})",
            self.output().to_string_js(),
            self.written.report.entries.len()
        )
    }
}

// ---------------------------------------------------------------- functions

/// Validate a portable relative asset path and return it
/// (`ResourcePath::new`): `/`-separated, relative, no empty, `.` or `..`
/// components, no backslashes, drive prefixes or control characters.
#[wasm_bindgen(js_name = resourcePath)]
pub fn resource_path(path: &str) -> JsResult<String> {
    Ok(path_arg(path)?.as_str().to_owned())
}

/// The default `ResourceLimits` (plain data; `usize` fields are numbers).
#[wasm_bindgen(js_name = defaultResourceLimits, unchecked_return_type = "ResourceLimits")]
pub fn default_resource_limits() -> JsResult<JsValue> {
    conv::to_js(&ResourceLimits::default())
}

/// The default `TransportLimits` (plain data).
#[wasm_bindgen(js_name = defaultTransportLimits, unchecked_return_type = "TransportLimits")]
pub fn default_transport_limits() -> JsResult<JsValue> {
    conv::to_js(&TransportLimits::default())
}
