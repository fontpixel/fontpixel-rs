//! JS-implemented codecs (`Registry.register`).
//!
//! A registration names a format (an existing catalogue ID whose entry has no
//! codec yet, or a new ID given as `{id, name, extensions}`) and supplies any
//! of three hooks (`FormatDetector`, `FormatReader`, `FormatWriter`):
//!
//! - `detector(ctx)` returns `"no_match"`, `"possible"` or `"certain"`;
//! - `reader(ctx, options)` returns `[fonts, report]` (`fonts` a `FontSet` or
//!   `Font`, `report` plain data; `options` is the `ReadOptions` plain data);
//! - `writer`: a function `writer(fonts, options)` or an object with
//!   `write(fonts, options)` returning a `Written`, an `OutputBundle` or
//!   `[OutputBundle, report]` (`fonts` is a new `FontSet` holding a copy of
//!   the model, `options` the `WriteOptions` plain data). A writer object may
//!   also define `transportLimits()` (a `TransportLimits` object) and
//!   `validateTransport(fonts, options, limits)`; the other writer hooks keep
//!   their Rust defaults (borrowed route, intersected limits).
//!
//! `ctx` is a `HookContext` over the registry's own `ReadContext` for the
//! running call: its bytes (gzip transport already removed), name,
//! directory flag, limits and the caller's resources, with every
//! `resource*`/`listResourcesLimited` call counted against the same budget
//! the built-in codecs use. It is valid only while the hook runs (afterwards
//! it throws `InvalidInputError`).
//!
//! Hooks run synchronously inside the registry call. An exception a hook
//! throws propagates unchanged out of that call. A hook may read and write
//! with the same registry, but configuring or registering on it, or
//! mutating a handle the running call holds (such as the `FontSet` being
//! written), throws `InvalidInputError` (re-entrant access).
use crate::callbacks::{Callback, CallbackScope};
use crate::conv;
use crate::errors::{self, IntoJs, JsResult, invalid};
use crate::formats::{FontsArg, JsRegistry, bundle_arg, fonts_arg, written_arg};
use crate::handles::{self, JsFontSet};
use fontpixel::formats::{
    Confidence, FormatDetector, FormatInfo, FormatReader, FormatWriter, ReadOptions, Registration,
    WriteOptions,
};
use fontpixel::resources::{ReadContext, ResourcePath, TransportLimits, Written};
use fontpixel::{Error, FontSet, Report};
use js_sys::{Array, Function, Reflect, Uint8Array};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::ScopedClosure;
use wasm_bindgen::prelude::*;

// ---------------------------------------------------------------------------
// HookContext
// ---------------------------------------------------------------------------

/// The input of a JS detector or reader hook: the registry's `ReadContext`
/// of the running call (`fontpixel::resources::ReadContext`), valid only
/// while the hook runs.
#[wasm_bindgen(js_name = HookContext)]
pub struct JsHookContext {
    name: Option<String>,
    directory: bool,
    limits: fontpixel::ResourceLimits,
    bytes: Function,
    retained: Function,
    resource: Function,
    list: Function,
    alive: Rc<Cell<bool>>,
}

impl JsHookContext {
    fn live(&self, function: &Function) -> JsResult<Function> {
        if self.alive.get() {
            Ok(function.clone())
        } else {
            Err(invalid(
                "this HookContext belongs to a hook call that has returned",
            ))
        }
    }
}

#[wasm_bindgen(js_class = HookContext)]
impl JsHookContext {
    /// Primary bytes after transport decoding (`ReadContext::bytes`), a copy.
    pub fn bytes(&self) -> JsResult<Uint8Array> {
        Ok(self
            .live(&self.bytes)?
            .call0(&JsValue::UNDEFINED)?
            .unchecked_into())
    }
    /// Bytes retained so far, including resolved resources
    /// (`ReadContext::retained_input_bytes`).
    #[wasm_bindgen(js_name = retainedInputBytes)]
    pub fn retained_input_bytes(&self) -> JsResult<f64> {
        let value = self.live(&self.retained)?.call0(&JsValue::UNDEFINED)?;
        Ok(value.as_f64().unwrap_or_default())
    }
    /// The filename hint (`ReadContext::name`).
    #[wasm_bindgen(getter)]
    pub fn name(&self) -> Option<String> {
        self.name.clone()
    }
    /// Whether the input is a directory entry point (`ReadContext::is_directory`).
    #[wasm_bindgen(getter, js_name = isDirectory)]
    pub fn is_directory(&self) -> bool {
        self.directory
    }
    /// The budgets in force (`ReadContext::limits`).
    #[wasm_bindgen(getter, unchecked_return_type = "ResourceLimits")]
    pub fn limits(&self) -> JsResult<JsValue> {
        conv::to_js(&self.limits)
    }
    /// Resolve a sidecar under the remaining budget (`ReadContext::resource`).
    pub fn resource(&self, path: &str) -> JsResult<Uint8Array> {
        let f = self.live(&self.resource)?;
        Ok(f.call2(
            &JsValue::UNDEFINED,
            &JsValue::from_str(path),
            &JsValue::from_f64(-1.0),
        )?
        .unchecked_into())
    }
    /// Resolve with an extra byte allowance (`ReadContext::resource_limited`).
    #[wasm_bindgen(js_name = resourceLimited)]
    pub fn resource_limited(&self, path: &str, max_bytes: f64) -> JsResult<Uint8Array> {
        conv::int::<usize>(max_bytes, "maxBytes")?;
        let f = self.live(&self.resource)?;
        Ok(f.call2(
            &JsValue::UNDEFINED,
            &JsValue::from_str(path),
            &JsValue::from_f64(max_bytes),
        )?
        .unchecked_into())
    }
    /// Immediate files of `directory` (`null` for the root) with shared
    /// accounting (`ReadContext::list_resources_limited`).
    #[wasm_bindgen(js_name = listResourcesLimited)]
    pub fn list_resources_limited(
        &self,
        directory: Option<String>,
        max_entries: f64,
        max_bytes: f64,
    ) -> JsResult<Vec<String>> {
        conv::int::<usize>(max_entries, "maxEntries")?;
        conv::int::<usize>(max_bytes, "maxBytes")?;
        let f = self.live(&self.list)?;
        let dir = directory.map_or(JsValue::NULL, |d| JsValue::from_str(&d));
        let out = f.call3(
            &JsValue::UNDEFINED,
            &dir,
            &JsValue::from_f64(max_entries),
            &JsValue::from_f64(max_bytes),
        )?;
        Ok(Array::from(&out)
            .iter()
            .filter_map(|v| v.as_string())
            .collect())
    }
    /// `HookContext(name=…, directory=…)`.
    #[wasm_bindgen(js_name = toString)]
    pub fn to_string_js(&self) -> String {
        format!(
            "HookContext(name={:?}, directory={})",
            self.name, self.directory
        )
    }
}

type BytesFn<'a> = dyn Fn() -> Result<Uint8Array, JsValue> + 'a;
type RetainedFn<'a> = dyn Fn() -> Result<f64, JsValue> + 'a;
type ResourceFn<'a> = dyn Fn(String, f64) -> Result<Uint8Array, JsValue> + 'a;
type ListFn<'a> = dyn Fn(JsValue, f64, f64) -> Result<JsValue, JsValue> + 'a;

/// Run `f` with a `HookContext` bound to `ctx` for the duration of the call.
fn with_hook_context<R>(ctx: &mut ReadContext<'_>, f: impl FnOnce(JsValue) -> R) -> R {
    let name = ctx.name().map(str::to_owned);
    let directory = ctx.is_directory();
    let limits = ctx.limits().clone();
    let cell = RefCell::new(ctx);
    let bytes = || -> Result<Uint8Array, JsValue> {
        let c = handles::borrow(&cell, "HookContext")?;
        Ok(conv::bytes_to_js(c.bytes()))
    };
    let retained = || -> Result<f64, JsValue> {
        let c = handles::borrow(&cell, "HookContext")?;
        Ok(c.retained_input_bytes() as f64)
    };
    let resource = |path: String, max: f64| -> Result<Uint8Array, JsValue> {
        let path = ResourcePath::new(path).js()?;
        let mut c = handles::borrow_mut(&cell, "HookContext")?;
        let data = if max < 0.0 {
            c.resource(&path)
        } else {
            c.resource_limited(&path, conv::int::<usize>(max, "maxBytes")?)
        }
        .js()?;
        Ok(conv::bytes_to_js(&data))
    };
    let list = |dir: JsValue, entries: f64, bytes: f64| -> Result<JsValue, JsValue> {
        let dir = match dir.as_string() {
            Some(d) => Some(ResourcePath::new(d).js()?),
            None => None,
        };
        let entries = conv::int::<usize>(entries, "maxEntries")?;
        let bytes = conv::int::<usize>(bytes, "maxBytes")?;
        let mut c = handles::borrow_mut(&cell, "HookContext")?;
        let paths = c
            .list_resources_limited(dir.as_ref(), entries, bytes)
            .js()?;
        let names: Vec<&str> = paths.iter().map(ResourcePath::as_str).collect();
        conv::to_js(&names)
    };
    let c_bytes: ScopedClosure<'_, BytesFn<'static>> = ScopedClosure::borrow(&bytes);
    let c_retained: ScopedClosure<'_, RetainedFn<'static>> = ScopedClosure::borrow(&retained);
    let c_resource: ScopedClosure<'_, ResourceFn<'static>> = ScopedClosure::borrow(&resource);
    let c_list: ScopedClosure<'_, ListFn<'static>> = ScopedClosure::borrow(&list);
    let alive = Rc::new(Cell::new(true));
    let context = JsHookContext {
        name,
        directory,
        limits,
        bytes: c_bytes.as_js_value().clone().unchecked_into(),
        retained: c_retained.as_js_value().clone().unchecked_into(),
        resource: c_resource.as_js_value().clone().unchecked_into(),
        list: c_list.as_js_value().clone().unchecked_into(),
        alive: alive.clone(),
    };
    let out = f(context.into());
    alive.set(false);
    out
}

// ---------------------------------------------------------------------------
// Codec
// ---------------------------------------------------------------------------

/// A hook failure: the JS exception is recorded on the hook's scope (and
/// rethrown by the registry call); the Rust error unwinds the call.
fn hook_failed(token: Callback, error: JsValue) -> Error {
    let message = Reflect::get(&error, &JsValue::from_str("message"))
        .ok()
        .and_then(|m| m.as_string())
        .unwrap_or_else(|| "exception".to_owned());
    token.record(error);
    Error::InvalidInput(format!("JS codec hook failed: {message}"))
}

fn hook<T>(token: Callback, f: impl FnOnce(&JsValue) -> JsResult<T>) -> fontpixel::Result<T> {
    let Some(object) = token.object() else {
        return Err(Error::InvalidInput(
            "JS codec hook is no longer registered".into(),
        ));
    };
    f(&object).map_err(|error| hook_failed(token, error))
}

struct JsCodec {
    detector: Option<Callback>,
    reader: Option<Callback>,
    writer: Option<Callback>,
}

impl FormatDetector for JsCodec {
    fn detect(&self, ctx: &mut ReadContext<'_>) -> fontpixel::Result<Confidence> {
        let token = self.detector.ok_or(Error::UnrecognizedFormat)?;
        hook(token, |f| {
            let f: &Function = f.unchecked_ref();
            let out = with_hook_context(ctx, |c| f.call1(&JsValue::UNDEFINED, &c))?;
            conv::from_js(&out)
        })
    }
}

impl FormatReader<FontSet> for JsCodec {
    fn read(
        &self,
        ctx: &mut ReadContext<'_>,
        options: &ReadOptions,
    ) -> fontpixel::Result<(FontSet, Report)> {
        let token = self.reader.ok_or(Error::UnrecognizedFormat)?;
        hook(token, |f| {
            let f: &Function = f.unchecked_ref();
            let options = conv::to_js(options)?;
            let out = with_hook_context(ctx, |c| f.call2(&JsValue::UNDEFINED, &c, &options))?;
            if !Array::is_array(&out) || Array::from(&out).length() != 2 {
                return Err(invalid("a reader hook returns [fonts, report]"));
            }
            let pair = Array::from(&out);
            let fonts = match fonts_arg(&pair.get(0))? {
                FontsArg::Set(set) => set.snapshot()?,
                FontsArg::Font(font) => FontSet {
                    fonts: vec![font.snapshot()?],
                    ..FontSet::default()
                },
            };
            let report: Report = conv::from_js_or_default(&pair.get(1))?;
            Ok((fonts, report))
        })
    }
}

/// A writer hook's result: `Written`, `OutputBundle` or `[OutputBundle, report]`.
fn written_of(out: &JsValue) -> JsResult<Written> {
    if let Some(written) = written_arg(out)? {
        return Ok(written);
    }
    if let Some(bundle) = bundle_arg(out)? {
        return Ok(Written {
            output: (*bundle).clone(),
            report: Report::default(),
        });
    }
    if Array::is_array(out) {
        let pair = Array::from(out);
        if pair.length() == 2
            && let Some(bundle) = bundle_arg(&pair.get(0))?
        {
            return Ok(Written {
                output: (*bundle).clone(),
                report: conv::from_js_or_default(&pair.get(1))?,
            });
        }
    }
    Err(invalid(
        "a writer hook returns a Written, an OutputBundle or [OutputBundle, report]",
    ))
}

/// The writer object's method `name` (`this` = the object), or the writer
/// itself when it is a function and `name` is `write`.
fn writer_method(writer: &JsValue, name: &str) -> JsResult<Option<(JsValue, Function)>> {
    if let Some(f) = writer.dyn_ref::<Function>() {
        return Ok((name == "write").then(|| (JsValue::UNDEFINED, f.clone())));
    }
    let method = Reflect::get(writer, &JsValue::from_str(name))?;
    Ok(method
        .dyn_into::<Function>()
        .ok()
        .map(|f| (writer.clone(), f)))
}

impl FormatWriter<FontSet> for JsCodec {
    fn write(&self, fonts: &FontSet, options: &WriteOptions) -> fontpixel::Result<Written> {
        let token = self.writer.ok_or(Error::UnrecognizedFormat)?;
        hook(token, |w| {
            let (this, write) = writer_method(w, "write")?
                .ok_or_else(|| invalid("a writer hook is a function or has write()"))?;
            let set = JsFontSet::new(fonts.clone());
            let out = write.call2(&this, &set.into(), &conv::to_js(options)?)?;
            written_of(&out)
        })
    }

    fn transport_limits(&self) -> TransportLimits {
        let Some(token) = self.writer else {
            return TransportLimits::default();
        };
        let Some(w) = token.object() else {
            return TransportLimits::default();
        };
        let limits = || -> JsResult<Option<TransportLimits>> {
            let Some((this, f)) = writer_method(&w, "transportLimits")? else {
                return Ok(None);
            };
            if w.is_function() {
                return Ok(None);
            }
            conv::from_js(&f.call0(&this)?).map(Some)
        };
        limits().ok().flatten().unwrap_or_default()
    }

    fn validate_transport(
        &self,
        fonts: &FontSet,
        options: &WriteOptions,
        limits: &TransportLimits,
    ) -> fontpixel::Result<()> {
        let Some(token) = self.writer else {
            return Ok(());
        };
        hook(token, |w| {
            if w.is_function() {
                return Ok(());
            }
            if let Some((this, f)) = writer_method(w, "validateTransport")? {
                let set = JsFontSet::new(fonts.clone());
                f.call3(
                    &this,
                    &set.into(),
                    &conv::to_js(options)?,
                    &conv::to_js(limits)?,
                )?;
            }
            Ok(())
        })
    }
}

/// Catalogue metadata for a registration: the static catalogue entry for a
/// known ID, otherwise new metadata (its strings live for the program).
fn format_info(format: &JsValue) -> JsResult<FormatInfo> {
    let find = |id: &str| {
        fontpixel::formats::catalogue::CATALOGUE
            .iter()
            .copied()
            .find(|f| f.id == id)
    };
    if let Some(id) = format.as_string() {
        return find(&id).ok_or_else(|| errors::to_js(Error::UnknownFormat(id)));
    }
    let options = conv::Options::new(format, &["id", "name", "extensions"], "format")?;
    let id = options
        .string("id")?
        .ok_or_else(|| invalid("format: `id` is required"))?;
    if let Some(known) = find(&id) {
        return Ok(known);
    }
    let name = options
        .string("name")?
        .ok_or_else(|| invalid("format: `name` is required for a new format ID"))?;
    let extensions: Vec<String> = options.plain("extensions")?.unwrap_or_default();
    let leak = |s: String| -> &'static str { Box::leak(s.into_boxed_str()) };
    let extensions: Vec<&'static str> = extensions.into_iter().map(leak).collect();
    Ok(FormatInfo {
        id: leak(id),
        name: leak(name),
        extensions: Box::leak(extensions.into_boxed_slice()),
        feature_enabled: true,
    })
}

#[wasm_bindgen(js_class = Registry)]
impl JsRegistry {
    /// Register a format implemented in JS (`Registry::register` with a
    /// `Registration`). `format` is a catalogue ID or `{id, name,
    /// extensions}` for a new ID (lowercase ASCII letters, digits and
    /// hyphens). Without hooks this is an honest planned entry
    /// (`Registration::planned`). A live entry cannot be replaced: use
    /// `new Registry({builtin: false})` to register a catalogue ID. The hook
    /// objects stay referenced until this registry is freed.
    pub fn register(
        &self,
        #[wasm_bindgen(
            unchecked_param_type = "string | { id: string; name?: string; extensions?: string[] }"
        )]
        format: JsValue,
        #[wasm_bindgen(unchecked_optional_param_type = "CodecHooks")] hooks: Option<JsValue>,
    ) -> JsResult<()> {
        let info = format_info(&format)?;
        let hooks = conv::Options::new(
            &hooks.unwrap_or(JsValue::UNDEFINED),
            &["detector", "reader", "writer"],
            "register hooks",
        )?;
        let detector = hooks.get("detector")?;
        let reader = hooks.get("reader")?;
        let writer = hooks.get("writer")?;
        for (name, hook) in [("detector", &detector), ("reader", &reader)] {
            if hook.as_ref().is_some_and(|h| !h.is_function()) {
                return Err(invalid(format!("register: `{name}` must be a function")));
            }
        }
        if writer
            .as_ref()
            .is_some_and(|w| !w.is_function() && !w.is_object())
        {
            return Err(invalid(
                "register: `writer` must be a function or an object with write()",
            ));
        }
        let mut reg = self.reg_mut()?;
        let mut scopes = Vec::new();
        let mut park = |hook: Option<JsValue>| {
            hook.map(|h| {
                let scope = CallbackScope::new(h);
                let token = scope.token();
                scopes.push(scope);
                token
            })
        };
        let codec = JsCodec {
            detector: park(detector),
            reader: park(reader),
            writer: park(writer),
        };
        let mut registration = Registration::planned(info);
        let make = || JsCodec {
            detector: codec.detector,
            reader: codec.reader,
            writer: codec.writer,
        };
        if codec.detector.is_some() {
            registration.detector = Some(Box::new(make()));
        }
        if codec.reader.is_some() {
            registration.reader = Some(Box::new(make()));
        }
        if codec.writer.is_some() {
            registration.writer = Some(Box::new(make()));
        }
        reg.register(registration).js()?;
        handles::borrow_mut(&self.hooks, "Registry")?.extend(scopes);
        Ok(())
    }
}
