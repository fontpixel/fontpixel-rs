//! Format registry, detection, reading and writing (`fontpixel::formats`).
//!
//! JS API (mirrors the Python `fontpixel` format functions):
//!
//! - `Registry`: the registry handle. `new Registry()` is the built-in
//!   registry (`fontpixel::registry()`), `new Registry({builtin: false})` the
//!   planned catalogue without codecs (`Registry::new`). Queries
//!   (`formats`, `status`, `extensionCandidates`, `formatFromName`),
//!   `detect`, `read`, `ensureWritable`, the four write routes (`write`,
//!   `writeWithReport`, `writeReservedWithReport`, `writeOwned`), typed
//!   option setters (`formats_options.rs`) and JS codecs (`register`,
//!   `formats_custom.rs`).
//! - Module functions `read`, `write`, `detect`, `registry`, `listFormats`
//!   over a fresh built-in registry.
//! - `Loaded` (read result) and `Written`/`OutputBundle` (write results,
//!   `resources.rs`).
//! - `formats.<module>` namespaces with each codec's direct functions
//!   (`formats_direct.rs`, `formats_extra.rs`).
//!
//! Inputs are bytes (`BytesLike`), or a `Written`/`OutputBundle` read back
//! with its own name and sidecars. `resources` supplies sidecars (an object
//! or `Map` of name → bytes, a resolver function or a resolver object);
//! `directory: "name"` reads a directory entry point whose files come from
//! `resources` (an explicit format is then required). Nothing reads a file
//! system or the network; asynchronous sources use `withAsyncResources`.
//!
//! Handle arguments (`FontSet`, `Font`, `Glyph`, `Bitmap`, `Written`,
//! `OutputBundle`) are taken as plain JS values so a parameter can accept
//! several classes (a `FontSet` or one `Font`, as in Python): the shared `model_args::handle_arg` protocol borrows the storage,
//! so the caller's handle stays valid. A released handle throws `InvalidInputError`.
use crate::callbacks::CallbackScope;
use crate::conv::{self, Options};
use crate::errors::{self, IntoJs, JsResult, invalid};
use crate::handles::{self, JsBitmap, JsFont, JsFontSet, JsGlyph};
use crate::model_args::{Handle, handle_arg};
use crate::resources::{JsWritten, path_arg};
use crate::resources::{ResourcesArg, SetResolver};
use fontpixel::formats::{Loaded, ReadOptions, WriteOptions};
use fontpixel::resources::{
    Input, OutputBundle, ResourceLimits, ResourcePath, ResourceResolver, TransportLimits, Written,
};
use fontpixel::{Error, Font, FontRegistry, FontSet, Report};
use js_sys::{Object, Reflect};
use std::cell::{Ref, RefCell, RefMut};
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::{Arc, RwLock};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

pub use crate::handles::Shared;

// ---------------------------------------------------------------------------
// Handle arguments
// ---------------------------------------------------------------------------

fn class_name(value: &JsValue) -> String {
    Reflect::get(value, &JsValue::from_str("constructor"))
        .ok()
        .and_then(|c| Reflect::get(&c, &JsValue::from_str("name")).ok())
        .and_then(|n| n.as_string())
        .unwrap_or_else(|| "object".to_owned())
}

fn describe(value: &JsValue) -> String {
    if value.is_undefined() {
        "undefined".into()
    } else if value.is_null() {
        "null".into()
    } else if value.is_object() {
        class_name(value)
    } else {
        value.js_typeof().as_string().unwrap_or_default()
    }
}

/// A font-set argument: a `FontSet` handle (read in place, shared) or a
/// `Font` handle (a one-strike set).
pub enum FontsArg {
    /// A `FontSet` handle.
    Set(JsFontSet),
    /// A `Font` handle (owned or a view).
    Font(JsFont),
}

/// Convert a `FontSet` or `Font` handle argument.
pub fn fonts_arg(value: &JsValue) -> JsResult<FontsArg> {
    let wrong = || {
        invalid(format!(
            "expected a FontSet or Font, got {}",
            describe(value)
        ))
    };
    match handle_arg(value)? {
        Some(Handle::FontSet(set)) => Ok(FontsArg::Set(JsFontSet { set })),
        Some(Handle::Font(loc)) => Ok(FontsArg::Font(JsFont { loc })),
        _ => Err(wrong()),
    }
}

impl FontsArg {
    /// Run `f` with the set (a `Font` becomes a one-strike copy).
    pub fn with_set<R>(&self, f: impl FnOnce(&FontSet) -> JsResult<R>) -> JsResult<R> {
        match self {
            Self::Set(set) => f(&*handles::borrow(&set.set, "FontSet")?),
            Self::Font(font) => f(&one_strike(font.snapshot()?)),
        }
    }
    /// Run `f` with one font (a `FontSet` must hold exactly one strike).
    pub fn with_font<R>(&self, f: impl FnOnce(&Font) -> JsResult<R>) -> JsResult<R> {
        match self {
            Self::Set(set) => {
                let set = handles::borrow(&set.set, "FontSet")?;
                f(set.single().js()?)
            }
            Self::Font(font) => font.read(f),
        }
    }
    /// The set by value: a `FontSet` handle is *moved* (left with zero
    /// strikes), a `Font` is copied into a one-strike set.
    pub fn take_set(&self) -> JsResult<FontSet> {
        match self {
            Self::Set(set) => Ok(std::mem::take(&mut *handles::borrow_mut(
                &set.set, "FontSet",
            )?)),
            Self::Font(font) => Ok(one_strike(font.snapshot()?)),
        }
    }
}

fn one_strike(font: Font) -> FontSet {
    FontSet {
        fonts: vec![font],
        ..FontSet::default()
    }
}

/// A `Glyph` handle argument (owned or a view).
pub fn glyph_arg(value: &JsValue) -> JsResult<JsGlyph> {
    match handle_arg(value)? {
        Some(Handle::Glyph(loc)) => Ok(JsGlyph { loc }),
        _ => Err(invalid(format!(
            "expected a Glyph, got {}",
            describe(value)
        ))),
    }
}

/// A `Bitmap` handle argument.
pub fn bitmap_arg(value: &JsValue) -> JsResult<JsBitmap> {
    match handle_arg(value)? {
        Some(Handle::Bitmap(bitmap)) => Ok(JsBitmap { bitmap }),
        _ => Err(invalid(format!(
            "expected a Bitmap, got {}",
            describe(value)
        ))),
    }
}

/// Copy the output bundle without consuming its JS handle.
pub fn bundle_arg(value: &JsValue) -> JsResult<Option<Rc<OutputBundle>>> {
    Ok(match handle_arg(value)? {
        Some(Handle::OutputBundle(bundle)) => Some(Rc::new(bundle)),
        Some(Handle::Written(written)) => Some(Rc::new(written.output)),
        _ => None,
    })
}

/// Copy a writer result without consuming its JS handle.
pub fn written_arg(value: &JsValue) -> JsResult<Option<Written>> {
    Ok(match handle_arg(value)? {
        Some(Handle::Written(written)) => Some(written),
        _ => None,
    })
}

// ---------------------------------------------------------------------------
// Inputs
// ---------------------------------------------------------------------------

enum InputResources {
    Arg(ResourcesArg),
    Bundle(SetResolver),
}

/// An owned input built from JS arguments.
pub struct InputArgs {
    data: Vec<u8>,
    name: Option<String>,
    directory: bool,
    resources: InputResources,
}

/// The input a reader sees for a written bundle: a file bundle's primary
/// bytes with every other asset as a sidecar, or a directory bundle's entry
/// point with its assets relative to the directory.
fn bundle_input(bundle: &OutputBundle) -> JsResult<InputArgs> {
    let primary = bundle.primary().as_str();
    let mut files = BTreeMap::new();
    let mut data = Vec::new();
    if bundle.is_directory() {
        let prefix = format!("{primary}/");
        for a in bundle.assets() {
            if let Some(rest) = a.path.as_str().strip_prefix(&prefix) {
                files.insert(path_arg(rest)?, a.bytes.clone());
            }
        }
    } else {
        for a in bundle.assets() {
            if a.path.as_str() == primary {
                data = a.bytes.clone();
            } else {
                files.insert(a.path.clone(), a.bytes.clone());
            }
        }
    }
    Ok(InputArgs {
        data,
        name: Some(primary.to_owned()),
        directory: bundle.is_directory(),
        resources: InputResources::Bundle(SetResolver(Arc::new(RwLock::new(files)))),
    })
}

fn is_bytes(value: &JsValue) -> bool {
    value.is_instance_of::<js_sys::Uint8Array>()
        || value.is_instance_of::<js_sys::ArrayBuffer>()
        || value.is_instance_of::<js_sys::DataView>()
        || value.is_instance_of::<js_sys::Uint8ClampedArray>()
}

impl InputArgs {
    /// Collect `data`, `name`, `resources` and `directory`. `data` is bytes,
    /// a `Written`/`OutputBundle` (which carries its own name and sidecars,
    /// so `name`, `resources` and `directory` must be absent) or
    /// `undefined` with `directory`.
    pub fn new(
        data: &JsValue,
        name: Option<String>,
        resources: &JsValue,
        directory: Option<String>,
    ) -> JsResult<Self> {
        let has_data = !data.is_undefined() && !data.is_null();
        let has_resources = !resources.is_undefined() && !resources.is_null();
        if has_data && !is_bytes(data) {
            if let Some(bundle) = bundle_arg(data)? {
                if name.is_some() || has_resources || directory.is_some() {
                    return Err(invalid(
                        "a Written or OutputBundle already carries its name and resources",
                    ));
                }
                return bundle_input(&bundle);
            }
            return Err(invalid(format!(
                "data: expected a Uint8Array, ArrayBuffer, DataView, Written or OutputBundle, got {}",
                describe(data)
            )));
        }
        let resources = InputResources::Arg(ResourcesArg::new(resources)?);
        if let Some(dir) = directory {
            if has_data || name.is_some() {
                return Err(invalid(
                    "a directory input takes no primary bytes or name; pass directory and resources",
                ));
            }
            return Ok(Self {
                data: Vec::new(),
                name: Some(dir),
                directory: true,
                resources,
            });
        }
        let data = if has_data {
            conv::bytes_from_js(data, "data")?
        } else {
            Vec::new()
        };
        Ok(Self {
            data,
            name,
            directory: false,
            resources,
        })
    }

    /// From an options object with `name`, `resources` and `directory`.
    pub fn from_options(data: &JsValue, options: &Options) -> JsResult<Self> {
        let resources = options.get("resources")?.unwrap_or(JsValue::UNDEFINED);
        Self::new(
            data,
            options.string("name")?,
            &resources,
            options.string("directory")?,
        )
    }

    fn resolver(&self) -> &dyn ResourceResolver {
        match &self.resources {
            InputResources::Arg(arg) => arg.resolver(),
            InputResources::Bundle(map) => map,
        }
    }

    /// Run `f` with the borrowed `Input` these arguments describe.
    pub fn with<R>(&self, f: impl FnOnce(&Input<'_>) -> R) -> R {
        let resolver = self.resolver();
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

    /// The exception a JS resolver threw during the call, if any.
    pub fn take_error(&self) -> Option<JsValue> {
        match &self.resources {
            InputResources::Arg(ResourcesArg::Js(scope, _)) => scope.take_error(),
            _ => None,
        }
    }

    /// `result`, unless a JS resolver threw: then its own exception.
    pub fn finish<T>(&self, result: fontpixel::Result<T>) -> JsResult<T> {
        match self.take_error() {
            Some(error) => Err(error),
            None => result.js(),
        }
    }
}

/// Optional `ResourceLimits` plain data (defaults when absent).
pub fn limits_of(options: &Options, key: &str) -> JsResult<ResourceLimits> {
    Ok(options.plain::<ResourceLimits>(key)?.unwrap_or_default())
}

// ---------------------------------------------------------------------------
// Results
// ---------------------------------------------------------------------------

/// A decoded model with its read report and the format actually used
/// (`fontpixel::formats::Loaded`).
#[wasm_bindgen(js_name = Loaded)]
pub struct JsLoaded {
    fonts: Shared<FontSet>,
    report: Report,
    format: &'static str,
}

impl JsLoaded {
    /// Wrap a registry result.
    pub fn new(loaded: Loaded<FontSet>) -> Self {
        Self {
            fonts: handles::shared(loaded.fonts),
            report: loaded.report,
            format: loaded.format,
        }
    }
}

#[wasm_bindgen(js_class = Loaded)]
impl JsLoaded {
    /// The decoded `FontSet` (a handle sharing this result's storage).
    #[wasm_bindgen(getter)]
    pub fn fonts(&self) -> JsFontSet {
        JsFontSet {
            set: self.fonts.clone(),
        }
    }
    /// The read report (plain data, every entry).
    #[wasm_bindgen(getter, unchecked_return_type = "Report")]
    pub fn report(&self) -> JsResult<JsValue> {
        conv::to_js(&self.report)
    }
    /// The format ID used to decode the input.
    #[wasm_bindgen(getter)]
    pub fn format(&self) -> String {
        self.format.to_owned()
    }
    /// The sole strike (`Loaded::into_single`): `{fonts: Font, report,
    /// format}` with a copy of the font. Throws `InvalidInputError` for
    /// several strikes (or none).
    #[wasm_bindgen(js_name = intoSingle, unchecked_return_type = "LoadedFont")]
    pub fn into_single(&self) -> JsResult<JsValue> {
        let fonts = handles::borrow(&self.fonts, "Loaded")?.clone();
        let loaded = Loaded {
            fonts,
            report: self.report.clone(),
            format: self.format,
        }
        .into_single()
        .js()?;
        let out = Object::new();
        Reflect::set(
            &out,
            &JsValue::from_str("fonts"),
            &JsFont::new(loaded.fonts).into(),
        )?;
        Reflect::set(
            &out,
            &JsValue::from_str("report"),
            &conv::to_js(&loaded.report)?,
        )?;
        Reflect::set(
            &out,
            &JsValue::from_str("format"),
            &JsValue::from_str(loaded.format),
        )?;
        Ok(out.into())
    }
    /// `Loaded(format=…, report_entries=…)`.
    #[wasm_bindgen(js_name = toString)]
    pub fn to_string_js(&self) -> String {
        format!(
            "Loaded(format={:?}, report_entries={})",
            self.format,
            self.report.entries.len()
        )
    }
}

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

/// The format registry (`fontpixel::Registry<FontSet>`, alias `FontRegistry`).
///
/// Methods borrow the registry for the whole call: a JS codec hook that
/// configures or registers on the same registry while it runs gets
/// `InvalidInputError` (nested reads and writes are fine).
#[wasm_bindgen(js_name = Registry)]
pub struct JsRegistry {
    registry: RefCell<FontRegistry>,
    /// JS hook objects of `register()`, parked for the registry's lifetime.
    pub(crate) hooks: RefCell<Vec<CallbackScope>>,
}

/// Arguments common to the write routes.
pub struct WriteArgs {
    /// Earlier diagnostics.
    pub report: Report,
    /// Primary output name (default `font.<first extension>`).
    pub primary_name: Option<String>,
    /// Strict mode.
    pub strict: bool,
    /// Common transport limits.
    pub transport_limits: Option<TransportLimits>,
}

/// Option keys of the write routes (besides `report`).
pub const WRITE_KEYS: &[&str] = &["primaryName", "strict", "transportLimits"];

impl WriteArgs {
    /// From an options object (`report` read only when `with_report`).
    pub fn from_options(options: &Options, with_report: bool) -> JsResult<Self> {
        Ok(Self {
            report: if with_report {
                options.plain::<Report>("report")?.unwrap_or_default()
            } else {
                Report::default()
            },
            primary_name: options.string("primaryName")?,
            strict: options.bool("strict")?.unwrap_or(false),
            transport_limits: options.plain::<TransportLimits>("transportLimits")?,
        })
    }
}

/// `font.<first extension>` (or `font`) for a format's metadata.
pub fn default_primary_name(extensions: &[&str]) -> String {
    match extensions.first() {
        Some(ext) => format!("font.{ext}"),
        None => "font".to_owned(),
    }
}

/// Default primary output path of a catalogue format.
pub fn default_primary(id: &str) -> JsResult<ResourcePath> {
    let info = fontpixel::formats::catalogue::CATALOGUE
        .iter()
        .find(|f| f.id == id)
        .ok_or_else(|| errors::to_js(Error::UnknownFormat(id.into())))?;
    path_arg(&default_primary_name(info.extensions))
}

/// `WriteOptions` for `id` from the common write arguments.
pub fn write_options(
    primary: ResourcePath,
    strict: bool,
    transport_limits: Option<TransportLimits>,
) -> WriteOptions {
    let mut options = WriteOptions::new(primary);
    options.strict = strict;
    options.transport_limits = transport_limits;
    options
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

impl JsRegistry {
    /// Wrap a Rust registry.
    pub fn wrap(registry: FontRegistry) -> Self {
        Self {
            registry: RefCell::new(registry),
            hooks: RefCell::new(Vec::new()),
        }
    }
    /// Shared access (busy while a hook re-enters with a mutation).
    pub fn reg(&self) -> JsResult<Ref<'_, FontRegistry>> {
        handles::borrow(&self.registry, "Registry")
    }
    /// Exclusive access.
    pub fn reg_mut(&self) -> JsResult<RefMut<'_, FontRegistry>> {
        handles::borrow_mut(&self.registry, "Registry")
    }

    /// The first exception one of this registry's JS hooks threw (all
    /// recorded hook exceptions are cleared).
    pub fn take_hook_error(&self) -> Option<JsValue> {
        let hooks = self.hooks.try_borrow().ok()?;
        let mut first = None;
        for scope in hooks.iter() {
            if let Some(error) = scope.take_error() {
                first.get_or_insert(error);
            }
        }
        first
    }

    /// Map a registry result, rethrowing unchanged the JS exception of a
    /// resource resolver (first) or of one of this registry's hooks.
    pub fn finish<T>(
        &self,
        input: Option<&InputArgs>,
        result: fontpixel::Result<T>,
    ) -> JsResult<T> {
        let resolver = input.and_then(InputArgs::take_error);
        let hook = self.take_hook_error();
        match resolver.or(hook) {
            Some(error) => Err(error),
            None => result.js(),
        }
    }

    /// Shared read implementation.
    pub fn read_impl(
        &self,
        input: &InputArgs,
        format: Option<String>,
        limits: ResourceLimits,
    ) -> JsResult<JsLoaded> {
        let options = ReadOptions { format, limits };
        let result = {
            let reg = self.reg()?;
            input.with(|i| reg.read(i, &options))
        };
        self.finish(Some(input), result).map(JsLoaded::new)
    }

    /// Shared detect implementation.
    pub fn detect_impl(&self, input: &InputArgs, limits: ResourceLimits) -> JsResult<JsValue> {
        let result = {
            let reg = self.reg()?;
            input.with(|i| reg.detect(i, &limits))
        };
        conv::to_js(&self.finish(Some(input), result)?)
    }

    /// Shared write implementation.
    pub fn write_impl(
        &self,
        fonts: &JsValue,
        format: &str,
        args: WriteArgs,
        mode: WriteMode,
    ) -> JsResult<JsWritten> {
        let fonts = fonts_arg(fonts)?;
        let primary = match &args.primary_name {
            Some(p) => path_arg(p)?,
            None => {
                let info = self.reg()?.status(format).js()?.format;
                path_arg(&default_primary_name(info.extensions))?
            }
        };
        let options = write_options(primary, args.strict, args.transport_limits);
        let report = args.report;
        let result = match mode {
            WriteMode::Owned => {
                let set = fonts.take_set()?;
                let reg = self.reg()?;
                reg.write_owned(set, format, &options, report)
            }
            WriteMode::Plain => fonts.with_set(|set| {
                let reg = self.reg()?;
                Ok(reg.write_with_report(set, format, &options, report))
            })?,
            WriteMode::Reserved => fonts.with_set(|set| {
                let reg = self.reg()?;
                Ok(reg.write_reserved_with_report(set, format, &options, report))
            })?,
        };
        self.finish(None, result).map(JsWritten::new)
    }
}

#[wasm_bindgen(js_class = Registry)]
impl JsRegistry {
    /// `new Registry()` is the built-in registry with every compiled codec
    /// (`fontpixel::registry()`); `new Registry({builtin: false})` is the
    /// planned catalogue without codecs (`Registry::new`).
    #[wasm_bindgen(constructor)]
    pub fn constructor(
        #[wasm_bindgen(unchecked_optional_param_type = "{ builtin?: boolean }")] options: Option<
            JsValue,
        >,
    ) -> JsResult<JsRegistry> {
        let options = Options::new(
            &options.unwrap_or(JsValue::UNDEFINED),
            &["builtin"],
            "Registry options",
        )?;
        Ok(Self::wrap(if options.bool("builtin")?.unwrap_or(true) {
            fontpixel::registry()
        } else {
            FontRegistry::new()
        }))
    }

    /// Every catalogue format with its capabilities, as `FormatStatus`
    /// records (`{format: {id, name, extensions, feature_enabled}, detect,
    /// read, write}`).
    #[wasm_bindgen(unchecked_return_type = "FormatStatus[]")]
    pub fn formats(&self) -> JsResult<JsValue> {
        let all: Vec<_> = self.reg()?.formats().collect();
        conv::to_js(&all)
    }

    /// The `FormatStatus` of one format ID; throws `UnknownFormatError`.
    #[wasm_bindgen(unchecked_return_type = "FormatStatus")]
    pub fn status(&self, format: &str) -> JsResult<JsValue> {
        conv::to_js(&self.reg()?.status(format).js()?)
    }

    /// Formats whose extension is the longest matching suffix of `name`
    /// (`FormatInfo` records; ties are all returned).
    #[wasm_bindgen(js_name = extensionCandidates, unchecked_return_type = "FormatInfo[]")]
    pub fn extension_candidates(&self, name: &str) -> JsResult<JsValue> {
        conv::to_js(&self.reg()?.extension_candidates(name))
    }

    /// The unique `FormatInfo` for a filename; throws
    /// `UnrecognizedFormatError` or `AmbiguousFormatError` (with
    /// `candidates`).
    #[wasm_bindgen(js_name = formatFromName, unchecked_return_type = "FormatInfo")]
    pub fn format_from_name(&self, name: &str) -> JsResult<JsValue> {
        conv::to_js(&self.reg()?.format_from_name(name).js()?)
    }

    /// Detect the format of an input: content detectors first, the filename
    /// hint only when no detector matches. Returns `{format: FormatInfo,
    /// confidence}`. A directory input (`directory`) throws: directory
    /// inputs need an explicit format.
    #[wasm_bindgen(unchecked_return_type = "Detected")]
    pub fn detect(
        &self,
        #[wasm_bindgen(unchecked_param_type = "BytesLike | Written | OutputBundle | undefined")]
        data: JsValue,
        #[wasm_bindgen(unchecked_optional_param_type = "DetectArgs")] options: Option<JsValue>,
    ) -> JsResult<JsValue> {
        let options = Options::new(
            &options.unwrap_or(JsValue::UNDEFINED),
            &["name", "resources", "limits", "directory"],
            "detect options",
        )?;
        let input = InputArgs::from_options(&data, &options)?;
        self.detect_impl(&input, limits_of(&options, "limits")?)
    }

    /// Throw `NotImplementedFormatError` unless `format` has a writer.
    #[wasm_bindgen(js_name = ensureWritable)]
    pub fn ensure_writable(&self, format: &str) -> JsResult<()> {
        self.reg()?.ensure_writable(format).js()
    }

    /// Decode an input (`Registry::read`). `format` bypasses detection;
    /// `limits` is a `ResourceLimits` object; `resources` supplies sidecars;
    /// `directory: "name"` reads a directory entry point whose files come
    /// from `resources` (an explicit format is then required).
    pub fn read(
        &self,
        #[wasm_bindgen(unchecked_param_type = "BytesLike | Written | OutputBundle | undefined")]
        data: JsValue,
        #[wasm_bindgen(unchecked_optional_param_type = "RegistryReadArgs")] options: Option<
            JsValue,
        >,
    ) -> JsResult<JsLoaded> {
        let options = Options::new(
            &options.unwrap_or(JsValue::UNDEFINED),
            &["format", "name", "resources", "limits", "directory"],
            "read options",
        )?;
        let input = InputArgs::from_options(&data, &options)?;
        self.read_impl(
            &input,
            options.string("format")?,
            limits_of(&options, "limits")?,
        )
    }

    /// Encode a `FontSet` (or one `Font`) as `format` (`Registry::write`,
    /// or `write_with_report` when `report` carries earlier read
    /// diagnostics). `primaryName` defaults to `font.<first extension>`; a
    /// `.gz` suffix selects the gzip envelope. `strict: true` throws
    /// `StrictLossError` on any read or write loss. `transportLimits` is a
    /// `TransportLimits` object.
    pub fn write(
        &self,
        #[wasm_bindgen(unchecked_param_type = "FontSet | Font")] fonts: JsValue,
        format: &str,
        #[wasm_bindgen(unchecked_optional_param_type = "RegistryWriteArgs")] options: Option<
            JsValue,
        >,
    ) -> JsResult<JsWritten> {
        let options = Options::new(
            &options.unwrap_or(JsValue::UNDEFINED),
            &["report", "primaryName", "strict", "transportLimits"],
            "write options",
        )?;
        let args = WriteArgs::from_options(&options, true)?;
        self.write_impl(&fonts, format, args, WriteMode::Plain)
    }

    /// `Registry::write_with_report`: combine `report` (earlier read
    /// diagnostics) with the write diagnostics before strict checks.
    #[wasm_bindgen(js_name = writeWithReport)]
    pub fn write_with_report(
        &self,
        #[wasm_bindgen(unchecked_param_type = "FontSet | Font")] fonts: JsValue,
        format: &str,
        #[wasm_bindgen(unchecked_param_type = "Report")] report: JsValue,
        #[wasm_bindgen(unchecked_optional_param_type = "WriteCommonArgs")] options: Option<JsValue>,
    ) -> JsResult<JsWritten> {
        let options = Options::new(
            &options.unwrap_or(JsValue::UNDEFINED),
            WRITE_KEYS,
            "write options",
        )?;
        let mut args = WriteArgs::from_options(&options, false)?;
        args.report = conv::from_js(&report)?;
        self.write_impl(&fonts, format, args, WriteMode::Plain)
    }

    /// `Registry::write_reserved_with_report`: as `writeWithReport` while the
    /// caller keeps the model's retained usage reserved outside
    /// `transportLimits`. Output and report are identical.
    #[wasm_bindgen(js_name = writeReservedWithReport)]
    pub fn write_reserved_with_report(
        &self,
        #[wasm_bindgen(unchecked_param_type = "FontSet | Font")] fonts: JsValue,
        format: &str,
        #[wasm_bindgen(unchecked_optional_param_type = "RegistryWriteArgs")] options: Option<
            JsValue,
        >,
    ) -> JsResult<JsWritten> {
        let options = Options::new(
            &options.unwrap_or(JsValue::UNDEFINED),
            &["report", "primaryName", "strict", "transportLimits"],
            "write options",
        )?;
        let args = WriteArgs::from_options(&options, true)?;
        self.write_impl(&fonts, format, args, WriteMode::Reserved)
    }

    /// `Registry::write_owned`: hand the model over so it can be released
    /// before later encoder stages. A `FontSet` argument is *moved*: the
    /// handle is left empty (zero strikes) afterwards. Output and report
    /// equal `writeWithReport`; `transportLimits` include the model.
    #[wasm_bindgen(js_name = writeOwned)]
    pub fn write_owned(
        &self,
        #[wasm_bindgen(unchecked_param_type = "FontSet | Font")] fonts: JsValue,
        format: &str,
        #[wasm_bindgen(unchecked_optional_param_type = "RegistryWriteArgs")] options: Option<
            JsValue,
        >,
    ) -> JsResult<JsWritten> {
        let options = Options::new(
            &options.unwrap_or(JsValue::UNDEFINED),
            &["report", "primaryName", "strict", "transportLimits"],
            "write options",
        )?;
        let args = WriteArgs::from_options(&options, true)?;
        self.write_impl(&fonts, format, args, WriteMode::Owned)
    }

    /// `Registry(99 formats, N with codecs)`.
    #[wasm_bindgen(js_name = toString)]
    pub fn to_string_js(&self) -> JsResult<String> {
        let reg = self.reg()?;
        let live = reg
            .formats()
            .filter(|s| s.read || s.write || s.detect)
            .count();
        Ok(format!(
            "Registry({} formats, {live} with codecs)",
            reg.formats().len()
        ))
    }
}

// ---------------------------------------------------------------------------
// Module functions
// ---------------------------------------------------------------------------

/// The built-in registry (`fontpixel::registry()`).
#[wasm_bindgen]
pub fn registry() -> JsRegistry {
    JsRegistry::wrap(fontpixel::registry())
}

/// Every catalogue format with its capabilities in this build
/// (`FormatStatus` records of the built-in registry).
#[wasm_bindgen(js_name = listFormats, unchecked_return_type = "FormatStatus[]")]
pub fn list_formats() -> JsResult<JsValue> {
    let all: Vec<_> = fontpixel::registry().formats().collect();
    conv::to_js(&all)
}

/// Decode font bytes with the built-in registry (`fontpixel::read`).
///
/// `format` selects the reader (otherwise content detection, then `name`
/// as a filename hint). `options` is the format's typed read options
/// (applied to the detected format when `format` is absent). `resources`
/// supplies sidecars (see `ResourcesLike`); `limits` is a `ResourceLimits`
/// object; `directory: "name"` reads a directory entry point through
/// `resources`. `data` may also be a `Written`/`OutputBundle`.
#[wasm_bindgen]
pub fn read(
    #[wasm_bindgen(unchecked_param_type = "BytesLike | Written | OutputBundle | undefined")]
    data: JsValue,
    #[wasm_bindgen(unchecked_optional_param_type = "ReadArgs")] options: Option<JsValue>,
) -> JsResult<JsLoaded> {
    let options = Options::new(
        &options.unwrap_or(JsValue::UNDEFINED),
        &[
            "format",
            "options",
            "name",
            "resources",
            "limits",
            "directory",
        ],
        "read options",
    )?;
    let input = InputArgs::from_options(&data, &options)?;
    let limits = limits_of(&options, "limits")?;
    let reg = registry();
    let format = match (options.string("format")?, options.get("options")?) {
        (Some(f), Some(o)) => {
            crate::formats_options::set_read(&mut *reg.reg_mut()?, &f, &o)?;
            Some(f)
        }
        (None, Some(o)) => {
            let detected = {
                let r = reg.reg()?;
                input.with(|i| r.detect(i, &limits))
            };
            let id = reg.finish(Some(&input), detected)?.format.id.to_owned();
            crate::formats_options::set_read(&mut *reg.reg_mut()?, &id, &o)?;
            Some(id)
        }
        (f, None) => f,
    };
    reg.read_impl(&input, format, limits)
}

/// Encode a `FontSet` (or one `Font`) with the built-in registry
/// (`fontpixel::write`, or `write_with_report` when `report` is given).
///
/// `options` is the format's typed write options; `primaryName` defaults
/// to `font.<first extension>` (a `.gz` suffix selects the gzip envelope);
/// `strict: true` throws `StrictLossError` on any loss; `transportLimits`
/// is a `TransportLimits` object. Returns a `Written`.
#[wasm_bindgen]
pub fn write(
    #[wasm_bindgen(unchecked_param_type = "FontSet | Font")] fonts: JsValue,
    format: &str,
    #[wasm_bindgen(unchecked_optional_param_type = "WriteArgs")] options: Option<JsValue>,
) -> JsResult<JsWritten> {
    let options = Options::new(
        &options.unwrap_or(JsValue::UNDEFINED),
        &[
            "options",
            "report",
            "primaryName",
            "strict",
            "transportLimits",
        ],
        "write options",
    )?;
    let reg = registry();
    if let Some(o) = options.get("options")? {
        crate::formats_options::set_write(&mut *reg.reg_mut()?, format, &o)?;
    }
    let args = WriteArgs::from_options(&options, true)?;
    reg.write_impl(&fonts, format, args, WriteMode::Plain)
}

/// Detect the format of font bytes with the built-in registry. Returns
/// `{format: FormatInfo, confidence}`.
#[wasm_bindgen(unchecked_return_type = "Detected")]
pub fn detect(
    #[wasm_bindgen(unchecked_param_type = "BytesLike | Written | OutputBundle")] data: JsValue,
    #[wasm_bindgen(unchecked_optional_param_type = "DetectArgs")] options: Option<JsValue>,
) -> JsResult<JsValue> {
    let options = Options::new(
        &options.unwrap_or(JsValue::UNDEFINED),
        &["name", "resources", "limits"],
        "detect options",
    )?;
    let input = InputArgs::from_options(&data, &options)?;
    registry().detect_impl(&input, limits_of(&options, "limits")?)
}
