//! Sidecar resources: named collections and explicit resolvers.
//!
//! Synchronous WASM calls never read files or the network. A `resources`
//! argument is one of:
//!
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
use crate::callbacks::{Callback, CallbackScope};
use crate::conv;
use crate::errors::{self, JsResult};
use fontpixel::resources::{NoResources, ResourcePath, ResourceResolver};
use fontpixel::{Error, Result};
use js_sys::{Array, Function, Map, Object, Reflect};
use std::collections::BTreeMap;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

fn limit(resource: &'static str, limit: usize) -> Error {
    Error::ResourceLimit { resource, limit }
}

/// In-memory named files (`resources` given as an object or `Map`).
pub struct MapResolver(pub BTreeMap<String, Vec<u8>>);

impl ResourceResolver for MapResolver {
    fn resolve(&self, path: &ResourcePath, max_bytes: usize) -> Result<Vec<u8>> {
        let bytes = self
            .0
            .get(path.as_str())
            .ok_or_else(|| Error::MissingResource(path.as_str().into()))?;
        if bytes.len() > max_bytes {
            return Err(limit("resource bytes", max_bytes));
        }
        Ok(bytes.clone())
    }

    fn list(
        &self,
        directory: Option<&ResourcePath>,
        max_entries: usize,
        max_bytes: usize,
    ) -> Result<Vec<ResourcePath>> {
        let prefix = directory.map(|d| format!("{}/", d.as_str()));
        let mut out = Vec::new();
        let mut used = 0usize;
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
                return Err(limit("directory entries", max_entries));
            }
            used = used.saturating_add(
                (std::mem::size_of::<ResourcePath>() + key.len()).saturating_mul(2),
            );
            if used > max_bytes {
                return Err(limit("directory listing bytes", max_bytes));
            }
            out.push(ResourcePath::new(key.clone())?);
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
    Error::Io(std::io::Error::other(format!("resource {what} callback threw")))
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

/// A converted `resources` argument; keep it alive for the whole call and
/// pass the call's result through [`ResourcesArg::finish`].
pub enum ResourcesArg {
    /// No sidecars (`NoResources`).
    None,
    /// Named in-memory files.
    Map(MapResolver),
    /// A JS resolver, parked for the call.
    Js(CallbackScope, JsResolver),
}

impl ResourcesArg {
    /// Convert `value` (`undefined`/`null`, object, `Map`, function or
    /// resolver object).
    pub fn new(value: &JsValue) -> JsResult<Self> {
        if value.is_undefined() || value.is_null() {
            return Ok(Self::None);
        }
        if value.is_function() {
            return Ok(Self::js(value));
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
                None => Ok(Self::Map(MapResolver(files))),
            };
        }
        let Some(object) = value.dyn_ref::<Object>() else {
            return Err(errors::invalid(
                "resources: expected an object or Map of name to bytes, a resolver function, or an object with resolve()",
            ));
        };
        if Reflect::get(object, &JsValue::from_str("resolve"))?.is_function() {
            return Ok(Self::js(value));
        }
        let mut files = BTreeMap::new();
        for entry in Object::entries(object).iter() {
            let entry: Array = entry.unchecked_into();
            let (name, data) = file_entry(&entry.get(0), &entry.get(1))?;
            files.insert(name, data);
        }
        Ok(Self::Map(MapResolver(files)))
    }

    fn js(value: &JsValue) -> Self {
        let scope = CallbackScope::new(value.clone());
        let token = scope.token();
        Self::Js(scope, JsResolver(token))
    }

    /// The resolver for `Input::with_resources`.
    pub fn resolver(&self) -> &dyn ResourceResolver {
        static NONE: NoResources = NoResources;
        match self {
            Self::None => &NONE,
            Self::Map(map) => map,
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

fn file_entry(key: &JsValue, bytes: &JsValue) -> JsResult<(String, Vec<u8>)> {
    let name = key
        .as_string()
        .ok_or_else(|| errors::invalid("resources: names must be strings"))?;
    let path = ResourcePath::new(name.clone()).map_err(errors::to_js)?;
    let data = conv::bytes_from_js(bytes, &format!("resources[{name:?}]"))?;
    Ok((path.as_str().to_owned(), data))
}
