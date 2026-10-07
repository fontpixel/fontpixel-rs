//! Per-codec namespaces: `formats.<module>` for every Rust module of
//! `fontpixel::formats` that has a codec (in the builds whose profile
//! compiles it).
//!
//! Each namespace binds that module's own direct functions with their native
//! shapes:
//!
//! - `detect(data)` (structural detectors) or `detect(data, {name, limits})`
//!   (modules without a direct detector run their registry codec's detector
//!   hook): `"no_match"`, `"possible"` or `"certain"`.
//! - `read(data, {...})`: `{model, report}`, where `model` is a `FontSet` or
//!   `Font` handle exactly as the Rust function returns it. Context readers
//!   take `options`, `name`, `resources`, `limits` and `directory`; byte
//!   readers take `options` (when they have any) and `limits`.
//! - `write(fonts, {...})`: a `Written` for writers that return one (they
//!   take `primaryName`, `strict`, `transportLimits`), otherwise `{data,
//!   report}` with the bytes as a `Uint8Array`.
//! - `formatIds()`: the catalogue IDs the namespace serves.
use crate::conv::{self, Options};
use crate::errors::{IntoJs, JsResult};
use crate::formats::{
    FontsArg, InputArgs, JsRegistry, WriteArgs, WriteMode, default_primary, fonts_arg, limits_of,
};
use crate::handles::{JsFont, JsFontSet};
use crate::resources::{JsWritten, path_arg};
use fontpixel::formats::{self as f, Confidence, FormatDetector};
use fontpixel::resources::{ReadContext, ResourceLimits, ResourcePath, Written};
use fontpixel::{Font, FontSet, Report};
use js_sys::{Object, Reflect};
use serde::de::DeserializeOwned;
use wasm_bindgen::prelude::*;

/// A decoded model returned to JS as its handle class.
pub trait IntoJsModel {
    /// Wrap as a handle.
    fn into_handle(self) -> JsValue;
}
impl IntoJsModel for FontSet {
    fn into_handle(self) -> JsValue {
        JsFontSet::new(self).into()
    }
}
impl IntoJsModel for Font {
    fn into_handle(self) -> JsValue {
        JsFont::new(self).into()
    }
}

/// A model argument: `Font` functions take a `Font` (or a one-strike
/// `FontSet`), `FontSet` functions a `FontSet` (or one `Font`).
pub trait ModelArg {
    /// Run `f` with the model.
    fn with<R>(arg: &FontsArg, f: impl FnOnce(&Self) -> JsResult<R>) -> JsResult<R>;
}
impl ModelArg for Font {
    fn with<R>(arg: &FontsArg, f: impl FnOnce(&Self) -> JsResult<R>) -> JsResult<R> {
        arg.with_font(f)
    }
}
impl ModelArg for FontSet {
    fn with<R>(arg: &FontsArg, f: impl FnOnce(&Self) -> JsResult<R>) -> JsResult<R> {
        arg.with_set(f)
    }
}

/// `{model, report}`.
pub fn model_report<M: IntoJsModel>((model, report): (M, Report)) -> JsResult<JsValue> {
    let out = Object::new();
    Reflect::set(&out, &JsValue::from_str("model"), &model.into_handle())?;
    Reflect::set(&out, &JsValue::from_str("report"), &conv::to_js(&report)?)?;
    Ok(out.into())
}

/// `{data, report}` with a fresh `Uint8Array`.
pub fn bytes_report((data, report): (Vec<u8>, Report)) -> JsResult<JsValue> {
    let out = Object::new();
    Reflect::set(&out, &JsValue::from_str("data"), &conv::bytes_to_js(&data))?;
    Reflect::set(&out, &JsValue::from_str("report"), &conv::to_js(&report)?)?;
    Ok(out.into())
}

/// A `Confidence` as its serde string.
pub fn confidence(c: Confidence) -> JsResult<JsValue> {
    conv::to_js(&c)
}

/// An options object with exactly `keys`.
pub fn options_of(value: Option<JsValue>, keys: &[&str], what: &'static str) -> JsResult<Options> {
    Options::new(&value.unwrap_or(JsValue::UNDEFINED), keys, what)
}

/// The `options` key as `O` (defaults when absent).
pub fn typed<O: DeserializeOwned + Default>(options: &Options) -> JsResult<O> {
    Ok(options.plain::<O>("options")?.unwrap_or_default())
}

/// `read(ctx, &O)` / `read(ctx)` shapes: decode through a `ReadContext`
/// over the input (`resources` and `directory` included).
pub fn read_ctx<M: IntoJsModel>(
    input: &InputArgs,
    limits: &ResourceLimits,
    f: impl for<'a, 'b> FnOnce(&'a mut ReadContext<'b>) -> fontpixel::Result<(M, Report)>,
) -> JsResult<JsValue> {
    let result = input.with(|i| {
        let mut c = ReadContext::new(i, limits)?;
        f(&mut c)
    });
    model_report(input.finish(result)?)
}

/// Write with the common write arguments into a `Written`.
pub fn written(result: fontpixel::Result<Written>) -> JsResult<JsWritten> {
    Ok(JsWritten::new(result.js()?))
}

/// Primary path from `primaryName`, else the format default.
pub fn primary_of(id: &str, options: &Options) -> JsResult<ResourcePath> {
    match options.string("primaryName")? {
        Some(p) => path_arg(&p),
        None => default_primary(id),
    }
}

/// Run a codec's `FormatDetector` hook on an input (gzip transport decoded
/// first, `name` as the filename hint).
pub fn detect_codec<C: FormatDetector>(
    codec: &C,
    data: &JsValue,
    options: Option<JsValue>,
) -> JsResult<JsValue> {
    let options = options_of(options, &["name", "limits"], "detect options")?;
    let input = InputArgs::new(data, options.string("name")?, &JsValue::UNDEFINED, None)?;
    let limits = limits_of(&options, "limits")?;
    let c = input.with(|i| {
        let mut ctx = ReadContext::new(i, &limits)?;
        codec.detect(&mut ctx)
    });
    confidence(input.finish(c)?)
}

/// Read through the built-in registry with `options` applied (modules
/// without direct functions).
pub fn read_registry(id: &str, data: &JsValue, options: Option<JsValue>) -> JsResult<JsValue> {
    let options = options_of(
        options,
        &["options", "name", "resources", "limits", "directory"],
        "read options",
    )?;
    let input = InputArgs::from_options(data, &options)?;
    let reg = crate::formats::registry();
    if let Some(o) = options.get("options")? {
        crate::formats_options::set_read(&mut *reg.reg_mut()?, id, &o)?;
    }
    let loaded = reg.read_impl(&input, Some(id.to_owned()), limits_of(&options, "limits")?)?;
    let out = Object::new();
    Reflect::set(&out, &JsValue::from_str("model"), &loaded.fonts().into())?;
    Reflect::set(&out, &JsValue::from_str("report"), &loaded.report()?)?;
    Ok(out.into())
}

/// Write through the built-in registry with `options` applied.
pub fn write_registry(id: &str, fonts: &JsValue, options: Option<JsValue>) -> JsResult<JsWritten> {
    let options = options_of(
        options,
        &["options", "primaryName", "strict", "transportLimits"],
        "write options",
    )?;
    let reg: JsRegistry = crate::formats::registry();
    if let Some(o) = options.get("options")? {
        crate::formats_options::set_write(&mut *reg.reg_mut()?, id, &o)?;
    }
    let args = WriteArgs::from_options(&options, false)?;
    reg.write_impl(fonts, id, args, WriteMode::Plain)
}

// ---------------------------------------------------------------------------
// Namespace generator
// ---------------------------------------------------------------------------

macro_rules! detect_fn {
    ($ns:tt, none) => {};
    ($ns:tt, bytes $path:path) => {
        /// Structural detector of this format (no filename evidence):
        /// `"no_match"`, `"possible"` or `"certain"`.
        #[wasm_bindgen(js_namespace = ["formats", $ns], js_name = detect, unchecked_return_type = "Confidence")]
        pub fn detect(#[wasm_bindgen(unchecked_param_type = "BytesLike")] data: JsValue) -> JsResult<JsValue> {
            let bytes = conv::bytes_from_js(&data, "data")?;
            confidence($path(&bytes))
        }
    };
    ($ns:tt, codec $codec:expr) => {
        /// Run this format's registry detector hook (gzip transport is
        /// decoded first; `name` is the filename hint): `"no_match"`,
        /// `"possible"` or `"certain"`.
        #[wasm_bindgen(js_namespace = ["formats", $ns], js_name = detect, unchecked_return_type = "Confidence")]
        pub fn detect(
            #[wasm_bindgen(unchecked_param_type = "BytesLike")] data: JsValue,
            #[wasm_bindgen(unchecked_optional_param_type = "{ name?: string; limits?: ResourceLimits }")]
            options: Option<JsValue>,
        ) -> JsResult<JsValue> {
            detect_codec(&$codec, &data, options)
        }
    };
}

macro_rules! read_fn {
    ($ns:tt, ctx_opts $path:path, $opts:ty, $bag:tt, $rts:tt) => {
        /// Decode one input through this module's context reader. `options`
        /// is the module's read options; `resources` supplies sidecars;
        /// `directory` names a directory entry point. Returns
        /// `{model, report}`.
        #[wasm_bindgen(js_namespace = ["formats", $ns], js_name = read, unchecked_return_type = $rts)]
        pub fn read(
            #[wasm_bindgen(
                unchecked_param_type = "BytesLike | Written | OutputBundle | undefined"
            )]
            data: JsValue,
            #[wasm_bindgen(unchecked_optional_param_type = $bag)] options: Option<JsValue>,
        ) -> JsResult<JsValue> {
            let options = options_of(
                options,
                &["options", "name", "resources", "limits", "directory"],
                "read options",
            )?;
            let o: $opts = typed(&options)?;
            let input = InputArgs::from_options(&data, &options)?;
            read_ctx(&input, &limits_of(&options, "limits")?, |c| $path(c, &o))
        }
    };
    ($ns:tt, ctx $path:path, $bag:tt, $rts:tt) => {
        /// Decode one input through this module's context reader
        /// (`resources` supplies sidecars). Returns `{model, report}`.
        #[wasm_bindgen(js_namespace = ["formats", $ns], js_name = read, unchecked_return_type = $rts)]
        pub fn read(
            #[wasm_bindgen(
                unchecked_param_type = "BytesLike | Written | OutputBundle | undefined"
            )]
            data: JsValue,
            #[wasm_bindgen(unchecked_optional_param_type = $bag)] options: Option<JsValue>,
        ) -> JsResult<JsValue> {
            let options = options_of(
                options,
                &["name", "resources", "limits", "directory"],
                "read options",
            )?;
            let input = InputArgs::from_options(&data, &options)?;
            read_ctx(&input, &limits_of(&options, "limits")?, |c| $path(c))
        }
    };
    ($ns:tt, bytes_limits $path:path, $bag:tt, $rts:tt) => {
        /// Decode standalone bytes under `limits` (a `ResourceLimits`
        /// object). Returns `{model, report}`.
        #[wasm_bindgen(js_namespace = ["formats", $ns], js_name = read, unchecked_return_type = $rts)]
        pub fn read(
            #[wasm_bindgen(unchecked_param_type = "BytesLike")] data: JsValue,
            #[wasm_bindgen(unchecked_optional_param_type = $bag)] options: Option<JsValue>,
        ) -> JsResult<JsValue> {
            let options = options_of(options, &["limits"], "read options")?;
            let bytes = conv::bytes_from_js(&data, "data")?;
            model_report($path(&bytes, &limits_of(&options, "limits")?).js()?)
        }
    };
    ($ns:tt, bytes_opts_limits $path:path, $opts:ty, $bag:tt, $rts:tt) => {
        /// Decode standalone bytes with this module's read options under
        /// `limits`. Returns `{model, report}`.
        #[wasm_bindgen(js_namespace = ["formats", $ns], js_name = read, unchecked_return_type = $rts)]
        pub fn read(
            #[wasm_bindgen(unchecked_param_type = "BytesLike")] data: JsValue,
            #[wasm_bindgen(unchecked_optional_param_type = $bag)] options: Option<JsValue>,
        ) -> JsResult<JsValue> {
            let options = options_of(options, &["options", "limits"], "read options")?;
            let o: $opts = typed(&options)?;
            let bytes = conv::bytes_from_js(&data, "data")?;
            model_report($path(&bytes, &o, &limits_of(&options, "limits")?).js()?)
        }
    };
    ($ns:tt, bytes_opts $path:path, $opts:ty, $bag:tt, $rts:tt) => {
        /// Decode standalone bytes with this module's read options (limits
        /// are part of the options). Returns `{model, report}`.
        #[wasm_bindgen(js_namespace = ["formats", $ns], js_name = read, unchecked_return_type = $rts)]
        pub fn read(
            #[wasm_bindgen(unchecked_param_type = "BytesLike")] data: JsValue,
            #[wasm_bindgen(unchecked_optional_param_type = $bag)] options: Option<JsValue>,
        ) -> JsResult<JsValue> {
            let options = options_of(options, &["options"], "read options")?;
            let o: $opts = typed(&options)?;
            let bytes = conv::bytes_from_js(&data, "data")?;
            model_report($path(&bytes, &o).js()?)
        }
    };
    ($ns:tt, registry $id:literal, $bag:tt, $rts:tt) => {
        /// Decode through the built-in registry's reader for this format
        /// with `options` (read options) applied. Returns `{model, report}`
        /// with a `FontSet`.
        #[wasm_bindgen(js_namespace = ["formats", $ns], js_name = read, unchecked_return_type = $rts)]
        pub fn read(
            #[wasm_bindgen(
                unchecked_param_type = "BytesLike | Written | OutputBundle | undefined"
            )]
            data: JsValue,
            #[wasm_bindgen(unchecked_optional_param_type = $bag)] options: Option<JsValue>,
        ) -> JsResult<JsValue> {
            read_registry($id, &data, options)
        }
    };
}

macro_rules! write_fn {
    ($ns:tt, set_wo_opts $id:literal, $path:path, $opts:ty, $bag:tt) => {
        /// Encode a `FontSet` (or one `Font`) with this module's write
        /// options. `primaryName` defaults to `font.<first extension>`.
        /// Returns a `Written` with every asset and the full report.
        #[wasm_bindgen(js_namespace = ["formats", $ns], js_name = write)]
        pub fn write(
            #[wasm_bindgen(unchecked_param_type = "FontSet | Font")] fonts: JsValue,
            #[wasm_bindgen(unchecked_optional_param_type = $bag)] options: Option<JsValue>,
        ) -> JsResult<JsWritten> {
            let options = options_of(
                options,
                &["options", "primaryName", "strict", "transportLimits"],
                "write options",
            )?;
            let o: $opts = typed(&options)?;
            let args = WriteArgs::from_options(&options, false)?;
            let wo = crate::formats::write_options(primary_of($id, &options)?, args.strict, args.transport_limits);
            let fonts = fonts_arg(&fonts)?;
            written(fonts.with_set(|set| Ok($path(set, &wo, &o)))?)
        }
    };
    ($ns:tt, x_opts $x:ty, $path:path, $opts:ty, $bag:tt) => {
        /// Encode with this module's write options. Returns `{data, report}`.
        #[wasm_bindgen(js_namespace = ["formats", $ns], js_name = write, unchecked_return_type = "BytesReport")]
        pub fn write(
            #[wasm_bindgen(unchecked_param_type = "FontSet | Font")] fonts: JsValue,
            #[wasm_bindgen(unchecked_optional_param_type = $bag)] options: Option<JsValue>,
        ) -> JsResult<JsValue> {
            let options = options_of(options, &["options"], "write options")?;
            let o: $opts = typed(&options)?;
            let fonts = fonts_arg(&fonts)?;
            bytes_report(<$x as ModelArg>::with(&fonts, |x| Ok($path(x, &o)))?.js()?)
        }
    };
    ($ns:tt, x_primary_opts $id:literal, $x:ty, $path:path, $opts:ty, $bag:tt) => {
        /// Encode with this module's write options into a bundle whose entry
        /// point is `primaryName` (default `font.<first extension>`).
        /// Returns a `Written`.
        #[wasm_bindgen(js_namespace = ["formats", $ns], js_name = write)]
        pub fn write(
            #[wasm_bindgen(unchecked_param_type = "FontSet | Font")] fonts: JsValue,
            #[wasm_bindgen(unchecked_optional_param_type = $bag)]
            options: Option<JsValue>,
        ) -> JsResult<JsWritten> {
            let options = options_of(options, &["options", "primaryName"], "write options")?;
            let o: $opts = typed(&options)?;
            let primary = primary_of($id, &options)?;
            let fonts = fonts_arg(&fonts)?;
            written(<$x as ModelArg>::with(&fonts, |x| Ok($path(x, &primary, &o)))?)
        }
    };
    ($ns:tt, registry $id:literal, $bag:tt) => {
        /// Encode through the built-in registry's writer for this format
        /// with `options` (write options) applied. Returns a `Written`.
        #[wasm_bindgen(js_namespace = ["formats", $ns], js_name = write)]
        pub fn write(
            #[wasm_bindgen(unchecked_param_type = "FontSet | Font")] fonts: JsValue,
            #[wasm_bindgen(unchecked_optional_param_type = $bag)] options: Option<JsValue>,
        ) -> JsResult<JsWritten> {
            write_registry($id, &fonts, options)
        }
    };
}

macro_rules! namespaces {
    ($(
        $(#[$m:meta])*
        $name:ident $ns:tt [$($id:literal),+] {
            detect: ($($detect:tt)*),
            read: ($($read:tt)*),
            write: ($($write:tt)*) $(,)?
        }
    )*) => {
        $(
            $(#[$m])*
            #[doc = concat!("`formats.", $ns, "`: direct functions of `fontpixel::formats::", stringify!($name), "`.")]
            pub mod $name {
                use super::*;
                detect_fn!($ns, $($detect)*);
                read_fn!($ns, $($read)*);
                write_fn!($ns, $($write)*);
                /// The catalogue format IDs this namespace serves.
                #[wasm_bindgen(js_namespace = ["formats", $ns], js_name = formatIds)]
                pub fn format_ids() -> Vec<String> {
                    vec![$($id.to_owned()),+]
                }
            }
        )*
        /// JS names of the generated namespaces in this build.
        pub const GENERATED: &[&str] = &[$($(#[$m])* $ns,)*];
    };
}

namespaces! {
    #[cfg(feature = "all-formats")]
    amiga "amiga" ["amiga"] {
        detect: (codec f::amiga::AmigaCodec::default()),
        read: (ctx_opts f::amiga::read, f::amiga::AmigaReadOptions, "DirectContextReadArgs<SerdeTypes['formats::amiga::AmigaReadOptions']>", "ModelReport<FontSet>"),
        write: (set_wo_opts "amiga", f::amiga::write, f::amiga::AmigaWriteOptions, "DirectWriteArgs<SerdeTypes['formats::amiga::AmigaWriteOptions']>"),
    }
    #[cfg(feature = "all-formats")]
    bbc "bbc" ["bbc"] {
        detect: (bytes f::bbc::detect),
        read: (ctx_opts f::bbc::read, f::bbc::BbcReadOptions, "DirectContextReadArgs<SerdeTypes['formats::bbc::BbcReadOptions']>", "ModelReport<FontSet>"),
        write: (set_wo_opts "bbc", f::bbc::write, f::bbc::BbcWriteOptions, "DirectWriteArgs<SerdeTypes['formats::bbc::BbcWriteOptions']>"),
    }
    #[cfg(any(feature = "all-formats", feature = "minimal", feature = "merge"))]
    bdf "bdf" ["bdf"] {
        detect: (bytes f::bdf::detect),
        read: (bytes_limits f::bdf::read, "{ limits?: ResourceLimits }", "ModelReport<Font>"),
        write: (x_opts Font, f::bdf::write, f::bdf::BdfWriteOptions, "{ options?: SerdeTypes['formats::bdf::BdfWriteOptions'] }"),
    }
    #[cfg(feature = "all-formats")]
    beos "beos" ["beos"] {
        detect: (codec f::beos::BeosCodec::default()),
        read: (ctx_opts f::beos::read, f::beos::BeosReadOptions, "DirectContextReadArgs<SerdeTypes['formats::beos::BeosReadOptions']>", "ModelReport<FontSet>"),
        write: (set_wo_opts "beos", f::beos::write, f::beos::BeosWriteOptions, "DirectWriteArgs<SerdeTypes['formats::beos::BeosWriteOptions']>"),
    }
    #[cfg(feature = "all-formats")]
    bmfont "bmfont" ["bmfont"] {
        detect: (bytes f::bmfont::detect),
        read: (ctx_opts f::bmfont::read, f::bmfont::BmfontReadOptions, "DirectContextReadArgs<SerdeTypes['formats::bmfont::BmfontReadOptions']>", "ModelReport<Font>"),
        write: (x_primary_opts "bmfont", Font, f::bmfont::write, f::bmfont::BmfontWriteOptions, "{ options?: SerdeTypes['formats::bmfont::BmfontWriteOptions']; primaryName?: string }"),
    }
    #[cfg(feature = "all-formats")]
    c "c" ["c"] {
        detect: (codec f::c::CCodec::default()),
        read: (registry "c", "DirectContextReadArgs<object>", "ModelReport<FontSet>"),
        write: (registry "c", "DirectWriteArgs<object>"),
    }
    #[cfg(feature = "all-formats")]
    catalyst "catalyst" ["catalyst"] {
        detect: (bytes f::catalyst::detect),
        read: (ctx_opts f::catalyst::read, f::catalyst::CatalystReadOptions, "DirectContextReadArgs<SerdeTypes['formats::catalyst::CatalystReadOptions']>", "ModelReport<FontSet>"),
        write: (set_wo_opts "catalyst", f::catalyst::write, f::catalyst::CatalystWriteOptions, "DirectWriteArgs<SerdeTypes['formats::catalyst::CatalystWriteOptions']>"),
    }
    #[cfg(feature = "all-formats")]
    chiwriter "chiwriter" ["chiwriter"] {
        detect: (bytes f::chiwriter::detect),
        read: (ctx_opts f::chiwriter::read, f::chiwriter::ChiwriterReadOptions, "DirectContextReadArgs<SerdeTypes['formats::chiwriter::ChiwriterReadOptions']>", "ModelReport<FontSet>"),
        write: (set_wo_opts "chiwriter", f::chiwriter::write, f::chiwriter::ChiwriterWriteOptions, "DirectWriteArgs<SerdeTypes['formats::chiwriter::ChiwriterWriteOptions']>"),
    }
    #[cfg(feature = "all-formats")]
    comfont "comfont" ["comfont"] {
        detect: (bytes f::comfont::detect),
        read: (ctx_opts f::comfont::read, f::comfont::ComfontReadOptions, "DirectContextReadArgs<SerdeTypes['formats::comfont::ComfontReadOptions']>", "ModelReport<FontSet>"),
        write: (set_wo_opts "comfont", f::comfont::write, f::comfont::ComfontWriteOptions, "DirectWriteArgs<SerdeTypes['formats::comfont::ComfontWriteOptions']>"),
    }
    #[cfg(feature = "all-formats")]
    consoleet "consoleet" ["consoleet"] {
        detect: (codec f::consoleet::ConsoleetCodec::default()),
        read: (ctx_opts f::consoleet::read, f::consoleet::ConsoleetReadOptions, "DirectContextReadArgs<SerdeTypes['formats::consoleet::ConsoleetReadOptions']>", "ModelReport<FontSet>"),
        write: (set_wo_opts "consoleet", f::consoleet::write, f::consoleet::ConsoleetWriteOptions, "DirectWriteArgs<SerdeTypes['formats::consoleet::ConsoleetWriteOptions']>"),
    }
    #[cfg(feature = "all-formats")]
    cpi "cpi" ["cpi"] {
        detect: (bytes f::cpi::detect),
        read: (bytes_opts_limits f::cpi::read, f::cpi::CpiReadOptions, "{ options?: SerdeTypes['formats::cpi::CpiReadOptions']; limits?: ResourceLimits }", "ModelReport<FontSet>"),
        write: (x_opts FontSet, f::cpi::write, f::cpi::CpiWriteOptions, "{ options?: SerdeTypes['formats::cpi::CpiWriteOptions'] }"),
    }
    #[cfg(feature = "all-formats")]
    cybiko "cybiko" ["cybiko"] {
        detect: (bytes f::cybiko::detect),
        read: (ctx_opts f::cybiko::read, f::cybiko::CybikoReadOptions, "DirectContextReadArgs<SerdeTypes['formats::cybiko::CybikoReadOptions']>", "ModelReport<FontSet>"),
        write: (set_wo_opts "cybiko", f::cybiko::write, f::cybiko::CybikoWriteOptions, "DirectWriteArgs<SerdeTypes['formats::cybiko::CybikoWriteOptions']>"),
    }
    #[cfg(feature = "all-formats")]
    daisy "daisy" ["daisy"] {
        detect: (bytes f::daisy::detect),
        read: (ctx_opts f::daisy::read, f::daisy::DaisyReadOptions, "DirectContextReadArgs<SerdeTypes['formats::daisy::DaisyReadOptions']>", "ModelReport<FontSet>"),
        write: (set_wo_opts "daisy", f::daisy::write, f::daisy::DaisyWriteOptions, "DirectWriteArgs<SerdeTypes['formats::daisy::DaisyWriteOptions']>"),
    }
    #[cfg(feature = "all-formats")]
    dashen "dashen" ["dashen"] {
        detect: (bytes f::dashen::detect),
        read: (ctx_opts f::dashen::read, f::dashen::DashenReadOptions, "DirectContextReadArgs<SerdeTypes['formats::dashen::DashenReadOptions']>", "ModelReport<FontSet>"),
        write: (set_wo_opts "dashen", f::dashen::write, f::dashen::DashenWriteOptions, "DirectWriteArgs<SerdeTypes['formats::dashen::DashenWriteOptions']>"),
    }
    #[cfg(feature = "all-formats")]
    dosstart "dosstart" ["dosstart"] {
        detect: (bytes f::dosstart::detect),
        read: (ctx_opts f::dosstart::read, f::dosstart::DosstartReadOptions, "DirectContextReadArgs<SerdeTypes['formats::dosstart::DosstartReadOptions']>", "ModelReport<FontSet>"),
        write: (set_wo_opts "dosstart", f::dosstart::write, f::dosstart::DosstartWriteOptions, "DirectWriteArgs<SerdeTypes['formats::dosstart::DosstartWriteOptions']>"),
    }
    #[cfg(feature = "all-formats")]
    draw "draw" ["draw"] {
        detect: (bytes f::draw::detect),
        read: (bytes_limits f::draw::read, "{ limits?: ResourceLimits }", "ModelReport<FontSet>"),
        write: (x_opts FontSet, f::draw::write, f::draw::DrawWriteOptions, "{ options?: SerdeTypes['formats::draw::DrawWriteOptions'] }"),
    }
    #[cfg(feature = "all-formats")]
    drcs "drcs" ["drcs"] {
        detect: (bytes f::drcs::detect),
        read: (bytes_opts_limits f::drcs::read, f::drcs::DrcsReadOptions, "{ options?: SerdeTypes['formats::drcs::DrcsReadOptions']; limits?: ResourceLimits }", "ModelReport<FontSet>"),
        write: (x_opts FontSet, f::drcs::write, f::drcs::DrcsWriteOptions, "{ options?: SerdeTypes['formats::drcs::DrcsWriteOptions'] }"),
    }
    #[cfg(feature = "all-formats")]
    drhalo "drhalo" ["drhalo"] {
        detect: (bytes f::drhalo::detect),
        read: (ctx_opts f::drhalo::read, f::drhalo::DrhaloReadOptions, "DirectContextReadArgs<SerdeTypes['formats::drhalo::DrhaloReadOptions']>", "ModelReport<FontSet>"),
        write: (set_wo_opts "drhalo", f::drhalo::write, f::drhalo::DrhaloWriteOptions, "DirectWriteArgs<SerdeTypes['formats::drhalo::DrhaloWriteOptions']>"),
    }
    #[cfg(feature = "all-formats")]
    edwin "edwin" ["edwin"] {
        detect: (bytes f::edwin::detect),
        read: (ctx_opts f::edwin::read, f::edwin::EdwinReadOptions, "DirectContextReadArgs<SerdeTypes['formats::edwin::EdwinReadOptions']>", "ModelReport<FontSet>"),
        write: (set_wo_opts "edwin", f::edwin::write, f::edwin::EdwinWriteOptions, "DirectWriteArgs<SerdeTypes['formats::edwin::EdwinWriteOptions']>"),
    }
    #[cfg(feature = "all-formats")]
    fbit "fbit" ["fbit"] {
        detect: (bytes f::fbit::detect),
        read: (bytes_opts_limits f::fbit::read, f::fbit::FbitReadOptions, "{ options?: SerdeTypes['formats::fbit::FbitReadOptions']; limits?: ResourceLimits }", "ModelReport<FontSet>"),
        write: (x_opts FontSet, f::fbit::write, f::fbit::FbitWriteOptions, "{ options?: SerdeTypes['formats::fbit::FbitWriteOptions'] }"),
    }
    #[cfg(feature = "all-formats")]
    figlet "figlet" ["figlet"] {
        detect: (bytes f::figlet::detect),
        read: (ctx_opts f::figlet::read, f::figlet::FigletReadOptions, "DirectContextReadArgs<SerdeTypes['formats::figlet::FigletReadOptions']>", "ModelReport<FontSet>"),
        write: (set_wo_opts "figlet", f::figlet::write, f::figlet::FigletWriteOptions, "DirectWriteArgs<SerdeTypes['formats::figlet::FigletWriteOptions']>"),
    }
    #[cfg(feature = "all-formats")]
    fnt "fnt" ["fnt"] {
        detect: (bytes f::fnt::detect),
        read: (bytes_opts_limits f::fnt::read, f::fnt::FntReadOptions, "{ options?: SerdeTypes['formats::fnt::FntReadOptions']; limits?: ResourceLimits }", "ModelReport<Font>"),
        write: (x_opts Font, f::fnt::write, f::fnt::FntWriteOptions, "{ options?: SerdeTypes['formats::fnt::FntWriteOptions'] }"),
    }
    #[cfg(feature = "all-formats")]
    fon "fon" ["fon"] {
        detect: (bytes f::fon::detect),
        read: (bytes_opts_limits f::fon::read, f::fnt::FntReadOptions, "{ options?: SerdeTypes['formats::fnt::FntReadOptions']; limits?: ResourceLimits }", "ModelReport<FontSet>"),
        write: (x_opts FontSet, f::fon::write, f::fnt::FntWriteOptions, "{ options?: SerdeTypes['formats::fnt::FntWriteOptions'] }"),
    }
    #[cfg(feature = "all-formats")]
    fontx2 "fontx2" ["fontx2"] {
        detect: (bytes f::fontx2::detect),
        read: (bytes_opts_limits f::fontx2::read, f::fontx2::Fontx2ReadOptions, "{ options?: SerdeTypes['formats::fontx2::Fontx2ReadOptions']; limits?: ResourceLimits }", "ModelReport<Font>"),
        write: (x_opts Font, f::fontx2::write, f::fontx2::Fontx2WriteOptions, "{ options?: SerdeTypes['formats::fontx2::Fontx2WriteOptions'] }"),
    }
    #[cfg(feature = "all-formats")]
    fzx "fzx" ["fzx"] {
        detect: (bytes f::fzx::detect),
        read: (ctx_opts f::fzx::read, f::fzx::FzxReadOptions, "DirectContextReadArgs<SerdeTypes['formats::fzx::FzxReadOptions']>", "ModelReport<FontSet>"),
        write: (set_wo_opts "fzx", f::fzx::write, f::fzx::FzxWriteOptions, "DirectWriteArgs<SerdeTypes['formats::fzx::FzxWriteOptions']>"),
    }
    #[cfg(feature = "all-formats")]
    gdr "gdr" ["gdr"] {
        detect: (codec f::gdr::GdrCodec::default()),
        read: (ctx_opts f::gdr::read, f::gdr::GdrReadOptions, "DirectContextReadArgs<SerdeTypes['formats::gdr::GdrReadOptions']>", "ModelReport<FontSet>"),
        write: (set_wo_opts "gdr", f::gdr::write, f::gdr::GdrWriteOptions, "DirectWriteArgs<SerdeTypes['formats::gdr::GdrWriteOptions']>"),
    }
    #[cfg(feature = "all-formats")]
    gem "gem" ["gem"] {
        detect: (codec f::gem::GemCodec::default()),
        read: (ctx_opts f::gem::read, f::gem::GemReadOptions, "DirectContextReadArgs<SerdeTypes['formats::gem::GemReadOptions']>", "ModelReport<FontSet>"),
        write: (set_wo_opts "gem", f::gem::write, f::gem::GemWriteOptions, "DirectWriteArgs<SerdeTypes['formats::gem::GemWriteOptions']>"),
    }
    #[cfg(feature = "all-formats")]
    geos "geos" ["geos"] {
        detect: (codec f::geos::GeosCodec::default()),
        read: (ctx_opts f::geos::read, f::geos::GeosReadOptions, "DirectContextReadArgs<SerdeTypes['formats::geos::GeosReadOptions']>", "ModelReport<FontSet>"),
        write: (set_wo_opts "geos", f::geos::write, f::geos::GeosWriteOptions, "DirectWriteArgs<SerdeTypes['formats::geos::GeosWriteOptions']>"),
    }
    #[cfg(feature = "all-formats")]
    gf "gf" ["gf"] {
        detect: (bytes f::gf::detect),
        read: (bytes_limits f::gf::read, "{ limits?: ResourceLimits }", "ModelReport<Font>"),
        write: (x_opts Font, f::gf::write, f::gf::GfWriteOptions, "{ options?: SerdeTypes['formats::gf::GfWriteOptions'] }"),
    }
    #[cfg(feature = "all-formats")]
    gpi "gpi" ["gpi"] {
        detect: (codec f::gpi::GpiCodec::default()),
        read: (ctx_opts f::gpi::read, f::gpi::GpiReadOptions, "DirectContextReadArgs<SerdeTypes['formats::gpi::GpiReadOptions']>", "ModelReport<FontSet>"),
        write: (set_wo_opts "gpi", f::gpi::write, f::gpi::GpiWriteOptions, "DirectWriteArgs<SerdeTypes['formats::gpi::GpiWriteOptions']>"),
    }
    #[cfg(feature = "all-formats")]
    grasp "grasp" ["grasp"] {
        detect: (bytes f::grasp::detect),
        read: (ctx_opts f::grasp::read, f::grasp::GraspReadOptions, "DirectContextReadArgs<SerdeTypes['formats::grasp::GraspReadOptions']>", "ModelReport<FontSet>"),
        write: (set_wo_opts "grasp", f::grasp::write, f::grasp::GraspWriteOptions, "DirectWriteArgs<SerdeTypes['formats::grasp::GraspWriteOptions']>"),
    }
    #[cfg(feature = "all-formats")]
    hanfont "hanfont" ["hanfont"] {
        detect: (bytes f::hanfont::detect),
        read: (bytes_opts_limits f::hanfont::read, f::hanfont::HanfontReadOptions, "{ options?: SerdeTypes['formats::hanfont::HanfontReadOptions']; limits?: ResourceLimits }", "ModelReport<FontSet>"),
        write: (x_opts FontSet, f::hanfont::write, f::hanfont::HanfontWriteOptions, "{ options?: SerdeTypes['formats::hanfont::HanfontWriteOptions'] }"),
    }
    #[cfg(feature = "all-formats")]
    hbf "hbf" ["hbf"] {
        detect: (bytes f::hbf::detect),
        read: (ctx f::hbf::read, "DirectContextReadArgs<never>", "ModelReport<Font>"),
        write: (x_primary_opts "hbf", Font, f::hbf::write, f::hbf::HbfWriteOptions, "{ options?: SerdeTypes['formats::hbf::HbfWriteOptions']; primaryName?: string }"),
    }
    #[cfg(feature = "all-formats")]
    hex "hex" ["hex"] {
        detect: (bytes f::hex::detect),
        read: (bytes_opts_limits f::hex::read, f::hex::HexReadOptions, "{ options?: SerdeTypes['formats::hex::HexReadOptions']; limits?: ResourceLimits }", "ModelReport<Font>"),
        write: (x_opts Font, f::hex::write, f::hex::HexWriteOptions, "{ options?: SerdeTypes['formats::hex::HexWriteOptions'] }"),
    }
    #[cfg(feature = "all-formats")]
    hmzk "hmzk" ["hmzk"] {
        detect: (bytes f::hmzk::detect),
        read: (ctx_opts f::hmzk::read, f::hmzk::HmzkReadOptions, "DirectContextReadArgs<SerdeTypes['formats::hmzk::HmzkReadOptions']>", "ModelReport<FontSet>"),
        write: (set_wo_opts "hmzk", f::hmzk::write, f::hmzk::HmzkWriteOptions, "DirectWriteArgs<SerdeTypes['formats::hmzk::HmzkWriteOptions']>"),
    }
    #[cfg(feature = "all-formats")]
    hppcl "hppcl" ["hppcl"] {
        detect: (bytes f::hppcl::detect),
        read: (ctx_opts f::hppcl::read, f::hppcl::HppclReadOptions, "DirectContextReadArgs<SerdeTypes['formats::hppcl::HppclReadOptions']>", "ModelReport<FontSet>"),
        write: (set_wo_opts "hppcl", f::hppcl::write, f::hppcl::HppclWriteOptions, "DirectWriteArgs<SerdeTypes['formats::hppcl::HppclWriteOptions']>"),
    }
    #[cfg(feature = "all-formats")]
    hrcg "hrcg" ["hrcg"] {
        detect: (bytes f::hrcg::detect),
        read: (ctx_opts f::hrcg::read, f::hrcg::HrcgReadOptions, "DirectContextReadArgs<SerdeTypes['formats::hrcg::HrcgReadOptions']>", "ModelReport<FontSet>"),
        write: (set_wo_opts "hrcg", f::hrcg::write, f::hrcg::HrcgWriteOptions, "DirectWriteArgs<SerdeTypes['formats::hrcg::HrcgWriteOptions']>"),
    }
    #[cfg(feature = "all-formats")]
    hzk "hzk" ["hzk"] {
        detect: (bytes f::hzk::detect),
        read: (bytes_opts_limits f::hzk::read, f::hzk::HzkReadOptions, "{ options?: SerdeTypes['formats::hzk::HzkReadOptions']; limits?: ResourceLimits }", "ModelReport<Font>"),
        write: (x_opts Font, f::hzk::write, f::hzk::HzkWriteOptions, "{ options?: SerdeTypes['formats::hzk::HzkWriteOptions'] }"),
    }
    #[cfg(feature = "all-formats")]
    iigs "iigs" ["iigs"] {
        detect: (bytes f::iigs::detect),
        read: (bytes_opts_limits f::iigs::read, f::iigs::IigsReadOptions, "{ options?: SerdeTypes['formats::iigs::IigsReadOptions']; limits?: ResourceLimits }", "ModelReport<FontSet>"),
        write: (x_opts FontSet, f::iigs::write, f::iigs::IigsWriteOptions, "{ options?: SerdeTypes['formats::iigs::IigsWriteOptions'] }"),
    }
    #[cfg(feature = "all-formats")]
    image "image" ["image"] {
        detect: (codec f::image::ImageCodec::default()),
        read: (registry "image", "DirectContextReadArgs<object>", "ModelReport<FontSet>"),
        write: (registry "image", "DirectWriteArgs<object>"),
    }
    #[cfg(feature = "all-formats")]
    kbits "kbits" ["kbits"] {
        detect: (bytes f::kbits::detect),
        read: (bytes_limits f::kbits::read, "{ limits?: ResourceLimits }", "ModelReport<Font>"),
        write: (x_opts Font, f::kbits::write, f::kbits::KbitsWriteOptions, "{ options?: SerdeTypes['formats::kbits::KbitsWriteOptions'] }"),
    }
    #[cfg(feature = "all-formats")]
    kbitx "kbitx" ["kbitx"] {
        detect: (bytes f::kbitx::detect),
        read: (bytes_opts_limits f::kbitx::read, f::kbitx::KbitxReadOptions, "{ options?: SerdeTypes['formats::kbitx::KbitxReadOptions']; limits?: ResourceLimits }", "ModelReport<Font>"),
        write: (x_opts Font, f::kbitx::write, f::kbitx::KbitxWriteOptions, "{ options?: SerdeTypes['formats::kbitx::KbitxWriteOptions'] }"),
    }
    #[cfg(feature = "all-formats")]
    lisa "lisa" ["lisa"] {
        detect: (bytes f::lisa::detect),
        read: (bytes_opts_limits f::lisa::read, f::lisa::LisaReadOptions, "{ options?: SerdeTypes['formats::lisa::LisaReadOptions']; limits?: ResourceLimits }", "ModelReport<FontSet>"),
        write: (x_opts FontSet, f::lisa::write, f::lisa::LisaWriteOptions, "{ options?: SerdeTypes['formats::lisa::LisaWriteOptions'] }"),
    }
    #[cfg(feature = "all-formats")]
    mgtk "mgtk" ["mgtk"] {
        detect: (bytes f::mgtk::detect),
        read: (bytes_opts_limits f::mgtk::read, f::mgtk::MgtkReadOptions, "{ options?: SerdeTypes['formats::mgtk::MgtkReadOptions']; limits?: ResourceLimits }", "ModelReport<FontSet>"),
        write: (x_opts FontSet, f::mgtk::write, f::mgtk::MgtkWriteOptions, "{ options?: SerdeTypes['formats::mgtk::MgtkWriteOptions'] }"),
    }
    #[cfg(feature = "all-formats")]
    mkwinfont "mkwinfont" ["mkwinfont"] {
        detect: (bytes f::mkwinfont::detect),
        read: (ctx_opts f::mkwinfont::read, f::mkwinfont::MkwinfontReadOptions, "DirectContextReadArgs<SerdeTypes['formats::mkwinfont::MkwinfontReadOptions']>", "ModelReport<FontSet>"),
        write: (set_wo_opts "mkwinfont", f::mkwinfont::write, f::mkwinfont::MkwinfontWriteOptions, "DirectWriteArgs<SerdeTypes['formats::mkwinfont::MkwinfontWriteOptions']>"),
    }
    #[cfg(feature = "all-formats")]
    nfnt "nfnt" ["nfnt"] {
        detect: (bytes f::nfnt::detect),
        read: (bytes_opts_limits f::nfnt::read, f::nfnt::NfntReadOptions, "{ options?: SerdeTypes['formats::nfnt::NfntReadOptions']; limits?: ResourceLimits }", "ModelReport<FontSet>"),
        write: (x_opts FontSet, f::nfnt::write, f::nfnt::NfntWriteOptions, "{ options?: SerdeTypes['formats::nfnt::NfntWriteOptions'] }"),
    }
    #[cfg(feature = "all-formats")]
    oberon "oberon" ["oberon"] {
        detect: (codec f::oberon::OberonCodec::default()),
        read: (ctx_opts f::oberon::read, f::oberon::OberonReadOptions, "DirectContextReadArgs<SerdeTypes['formats::oberon::OberonReadOptions']>", "ModelReport<FontSet>"),
        write: (set_wo_opts "oberon", f::oberon::write, f::oberon::OberonWriteOptions, "DirectWriteArgs<SerdeTypes['formats::oberon::OberonWriteOptions']>"),
    }
    #[cfg(feature = "all-formats")]
    palm "palm" ["palm"] {
        detect: (codec f::palm::PalmCodec::default()),
        read: (ctx_opts f::palm::read, f::palm::PalmReadOptions, "DirectContextReadArgs<SerdeTypes['formats::palm::PalmReadOptions']>", "ModelReport<FontSet>"),
        write: (set_wo_opts "palm", f::palm::write, f::palm::PalmWriteOptions, "DirectWriteArgs<SerdeTypes['formats::palm::PalmWriteOptions']>"),
    }
    #[cfg(feature = "all-formats")]
    pbf "pbf" ["pbf"] {
        detect: (bytes f::pbf::detect),
        read: (bytes_opts_limits f::pbf::read, f::pbf::PbfReadOptions, "{ options?: SerdeTypes['formats::pbf::PbfReadOptions']; limits?: ResourceLimits }", "ModelReport<Font>"),
        write: (x_opts Font, f::pbf::write, f::pbf::PbfWriteOptions, "{ options?: SerdeTypes['formats::pbf::PbfWriteOptions'] }"),
    }
    #[cfg(any(feature = "all-formats", feature = "minimal", feature = "merge"))]
    pcf "pcf" ["pcf"] {
        detect: (bytes f::pcf::detect),
        read: (bytes_limits f::pcf::read, "{ limits?: ResourceLimits }", "ModelReport<Font>"),
        write: (x_opts Font, f::pcf::write, f::pcf::PcfWriteOptions, "{ options?: SerdeTypes['formats::pcf::PcfWriteOptions'] }"),
    }
    #[cfg(feature = "all-formats")]
    pcgeos "pcgeos" ["pcgeos"] {
        detect: (codec f::pcgeos::PcgeosCodec::default()),
        read: (ctx_opts f::pcgeos::read, f::pcgeos::PcgeosReadOptions, "DirectContextReadArgs<SerdeTypes['formats::pcgeos::PcgeosReadOptions']>", "ModelReport<FontSet>"),
        write: (set_wo_opts "pcgeos", f::pcgeos::write, f::pcgeos::PcgeosWriteOptions, "DirectWriteArgs<SerdeTypes['formats::pcgeos::PcgeosWriteOptions']>"),
    }
    #[cfg(feature = "all-formats")]
    pcr "pcr" ["pcr"] {
        detect: (bytes f::pcr::detect),
        read: (ctx_opts f::pcr::read, f::pcr::PcrReadOptions, "DirectContextReadArgs<SerdeTypes['formats::pcr::PcrReadOptions']>", "ModelReport<FontSet>"),
        write: (set_wo_opts "pcr", f::pcr::write, f::pcr::PcrWriteOptions, "DirectWriteArgs<SerdeTypes['formats::pcr::PcrWriteOptions']>"),
    }
    #[cfg(feature = "all-formats")]
    pf2 "pf2" ["pf2"] {
        detect: (bytes f::pf2::detect),
        read: (bytes_opts_limits f::pf2::read, f::pf2::Pf2ReadOptions, "{ options?: SerdeTypes['formats::pf2::Pf2ReadOptions']; limits?: ResourceLimits }", "ModelReport<Font>"),
        write: (x_opts Font, f::pf2::write, f::pf2::Pf2WriteOptions, "{ options?: SerdeTypes['formats::pf2::Pf2WriteOptions'] }"),
    }
    #[cfg(feature = "all-formats")]
    pfr "pfr" ["pfr"] {
        detect: (bytes f::pfr::detect),
        read: (ctx_opts f::pfr::read, f::pfr::PfrReadOptions, "DirectContextReadArgs<SerdeTypes['formats::pfr::PfrReadOptions']>", "ModelReport<FontSet>"),
        write: (set_wo_opts "pfr", f::pfr::write, f::pfr::PfrWriteOptions, "DirectWriteArgs<SerdeTypes['formats::pfr::PfrWriteOptions']>"),
    }
    #[cfg(feature = "all-formats")]
    pilfont "pilfont" ["pilfont"] {
        detect: (bytes f::pilfont::detect),
        read: (ctx_opts f::pilfont::read, f::pilfont::PilfontReadOptions, "DirectContextReadArgs<SerdeTypes['formats::pilfont::PilfontReadOptions']>", "ModelReport<FontSet>"),
        write: (set_wo_opts "pilfont", f::pilfont::write, f::pilfont::PilfontWriteOptions, "DirectWriteArgs<SerdeTypes['formats::pilfont::PilfontWriteOptions']>"),
    }
    #[cfg(feature = "all-formats")]
    pk "pk" ["pk"] {
        detect: (bytes f::pk::detect),
        read: (bytes_limits f::pk::read, "{ limits?: ResourceLimits }", "ModelReport<Font>"),
        write: (x_opts Font, f::pk::write, f::pk::PkWriteOptions, "{ options?: SerdeTypes['formats::pk::PkWriteOptions'] }"),
    }
    #[cfg(feature = "all-formats")]
    plan9 "plan9" ["plan9"] {
        detect: (bytes f::plan9::detect),
        read: (ctx f::plan9::read, "DirectContextReadArgs<never>", "ModelReport<Font>"),
        write: (x_primary_opts "plan9", FontSet, f::plan9::write, f::plan9::Plan9WriteOptions, "{ options?: SerdeTypes['formats::plan9::Plan9WriteOptions']; primaryName?: string }"),
    }
    #[cfg(feature = "all-formats")]
    playdate "playdate" ["playdate"] {
        detect: (bytes f::playdate::detect),
        read: (ctx_opts f::playdate::read, f::playdate::PlaydateReadOptions, "DirectContextReadArgs<SerdeTypes['formats::playdate::PlaydateReadOptions']>", "ModelReport<FontSet>"),
        write: (set_wo_opts "playdate", f::playdate::write, f::playdate::PlaydateWriteOptions, "DirectWriteArgs<SerdeTypes['formats::playdate::PlaydateWriteOptions']>"),
    }
    #[cfg(feature = "all-formats")]
    plus3dos "plus3dos" ["plus3dos"] {
        detect: (bytes f::plus3dos::detect),
        read: (ctx_opts f::plus3dos::read, f::plus3dos::Plus3dosReadOptions, "DirectContextReadArgs<SerdeTypes['formats::plus3dos::Plus3dosReadOptions']>", "ModelReport<FontSet>"),
        write: (set_wo_opts "plus3dos", f::plus3dos::write, f::plus3dos::Plus3dosWriteOptions, "DirectWriteArgs<SerdeTypes['formats::plus3dos::Plus3dosWriteOptions']>"),
    }
    #[cfg(feature = "all-formats")]
    polyprint "polyprint" ["polyprint"] {
        detect: (bytes f::polyprint::detect),
        read: (ctx_opts f::polyprint::read, f::polyprint::PolyprintReadOptions, "DirectContextReadArgs<SerdeTypes['formats::polyprint::PolyprintReadOptions']>", "ModelReport<FontSet>"),
        write: (set_wo_opts "polyprint", f::polyprint::write, f::polyprint::PolyprintWriteOptions, "DirectWriteArgs<SerdeTypes['formats::polyprint::PolyprintWriteOptions']>"),
    }
    #[cfg(feature = "all-formats")]
    prebuilt "prebuilt" ["prebuilt"] {
        detect: (bytes f::prebuilt::detect),
        read: (ctx_opts f::prebuilt::read, f::prebuilt::PrebuiltReadOptions, "DirectContextReadArgs<SerdeTypes['formats::prebuilt::PrebuiltReadOptions']>", "ModelReport<FontSet>"),
        write: (set_wo_opts "prebuilt", f::prebuilt::write, f::prebuilt::PrebuiltWriteOptions, "DirectWriteArgs<SerdeTypes['formats::prebuilt::PrebuiltWriteOptions']>"),
    }
    #[cfg(feature = "all-formats")]
    printshop "printshop" ["printshop"] {
        detect: (bytes f::printshop::detect),
        read: (ctx_opts f::printshop::read, f::printshop::PrintshopReadOptions, "DirectContextReadArgs<SerdeTypes['formats::printshop::PrintshopReadOptions']>", "ModelReport<FontSet>"),
        write: (set_wo_opts "printshop", f::printshop::write, f::printshop::PrintshopWriteOptions, "DirectWriteArgs<SerdeTypes['formats::printshop::PrintshopWriteOptions']>"),
    }
    #[cfg(feature = "all-formats")]
    psf "psf" ["psf"] {
        detect: (bytes f::psf::detect),
        read: (bytes_limits f::psf::read, "{ limits?: ResourceLimits }", "ModelReport<Font>"),
        write: (x_opts Font, f::psf::write, f::psf::PsfWriteOptions, "{ options?: SerdeTypes['formats::psf::PsfWriteOptions'] }"),
    }
    #[cfg(feature = "all-formats")]
    psf2txt "psf2txt" ["psf2txt"] {
        detect: (bytes f::psf2txt::detect),
        read: (ctx_opts f::psf2txt::read, f::psf2txt::Psf2txtReadOptions, "DirectContextReadArgs<SerdeTypes['formats::psf2txt::Psf2txtReadOptions']>", "ModelReport<FontSet>"),
        write: (set_wo_opts "psf2txt", f::psf2txt::write, f::psf2txt::Psf2txtWriteOptions, "DirectWriteArgs<SerdeTypes['formats::psf2txt::Psf2txtWriteOptions']>"),
    }
    #[cfg(feature = "all-formats")]
    pxl "pxl" ["pxl"] {
        detect: (bytes f::pxl::detect),
        read: (ctx_opts f::pxl::read, f::pxl::PxlReadOptions, "DirectContextReadArgs<SerdeTypes['formats::pxl::PxlReadOptions']>", "ModelReport<FontSet>"),
        write: (set_wo_opts "pxl", f::pxl::write, f::pxl::PxlWriteOptions, "DirectWriteArgs<SerdeTypes['formats::pxl::PxlWriteOptions']>"),
    }
    #[cfg(feature = "all-formats")]
    qpf "qpf" ["qpf"] {
        detect: (bytes f::qpf::detect),
        read: (bytes_opts_limits f::qpf::read, f::qpf::QpfReadOptions, "{ options?: SerdeTypes['formats::qpf::QpfReadOptions']; limits?: ResourceLimits }", "ModelReport<FontSet>"),
        write: (x_opts FontSet, f::qpf::write, f::qpf::QpfWriteOptions, "{ options?: SerdeTypes['formats::qpf::QpfWriteOptions'] }"),
    }
    #[cfg(feature = "all-formats")]
    qpf2 "qpf2" ["qpf2"] {
        detect: (bytes f::qpf2::detect),
        read: (bytes_opts_limits f::qpf2::read, f::qpf2::Qpf2ReadOptions, "{ options?: SerdeTypes['formats::qpf2::Qpf2ReadOptions']; limits?: ResourceLimits }", "ModelReport<FontSet>"),
        write: (x_opts FontSet, f::qpf2::write, f::qpf2::Qpf2WriteOptions, "{ options?: SerdeTypes['formats::qpf2::Qpf2WriteOptions'] }"),
    }
    #[cfg(feature = "all-formats")]
    raw "raw" ["raw"] {
        detect: (none),
        read: (bytes_opts_limits f::raw::read, f::raw::RawReadOptions, "{ options?: SerdeTypes['formats::raw::RawReadOptions']; limits?: ResourceLimits }", "ModelReport<FontSet>"),
        write: (x_opts FontSet, f::raw::write, f::raw::RawWriteOptions, "{ options?: SerdeTypes['formats::raw::RawWriteOptions'] }"),
    }
    #[cfg(feature = "all-formats")]
    riscos "riscos" ["riscos"] {
        detect: (bytes f::riscos::detect),
        read: (ctx_opts f::riscos::read, f::riscos::RiscosReadOptions, "DirectContextReadArgs<SerdeTypes['formats::riscos::RiscosReadOptions']>", "ModelReport<FontSet>"),
        write: (set_wo_opts "riscos", f::riscos::write, f::riscos::RiscosWriteOptions, "DirectWriteArgs<SerdeTypes['formats::riscos::RiscosWriteOptions']>"),
    }
    #[cfg(feature = "all-formats")]
    rockbox "rockbox" ["rockbox"] {
        detect: (bytes f::rockbox::detect),
        read: (ctx_opts f::rockbox::read, f::rockbox::RockboxReadOptions, "DirectContextReadArgs<SerdeTypes['formats::rockbox::RockboxReadOptions']>", "ModelReport<FontSet>"),
        write: (set_wo_opts "rockbox", f::rockbox::write, f::rockbox::RockboxWriteOptions, "DirectWriteArgs<SerdeTypes['formats::rockbox::RockboxWriteOptions']>"),
    }
    #[cfg(feature = "all-formats")]
    sbf "sbf" ["sbf"] {
        detect: (bytes f::sbf::detect),
        read: (ctx_opts f::sbf::read, f::sbf::SbfReadOptions, "DirectContextReadArgs<SerdeTypes['formats::sbf::SbfReadOptions']>", "ModelReport<FontSet>"),
        write: (set_wo_opts "sbf", f::sbf::write, f::sbf::SbfWriteOptions, "DirectWriteArgs<SerdeTypes['formats::sbf::SbfWriteOptions']>"),
    }
    #[cfg(feature = "all-formats")]
    sfd "sfd" ["sfd"] {
        detect: (bytes f::sfd::detect),
        read: (bytes_limits f::sfd::read, "{ limits?: ResourceLimits }", "ModelReport<FontSet>"),
        write: (x_opts FontSet, f::sfd::write, f::sfd::SfdWriteOptions, "{ options?: SerdeTypes['formats::sfd::SfdWriteOptions'] }"),
    }
    #[cfg(feature = "all-formats")]
    sfont "sfont" ["sfont"] {
        detect: (bytes f::sfont::detect),
        read: (ctx_opts f::sfont::read, f::sfont::SfontReadOptions, "DirectContextReadArgs<SerdeTypes['formats::sfont::SfontReadOptions']>", "ModelReport<FontSet>"),
        write: (set_wo_opts "sfont", f::sfont::write, f::sfont::SfontWriteOptions, "DirectWriteArgs<SerdeTypes['formats::sfont::SfontWriteOptions']>"),
    }
    #[cfg(feature = "all-formats")]
    signum "signum" ["signum"] {
        detect: (bytes f::signum::detect),
        read: (ctx_opts f::signum::read, f::signum::SignumReadOptions, "DirectContextReadArgs<SerdeTypes['formats::signum::SignumReadOptions']>", "ModelReport<FontSet>"),
        write: (set_wo_opts "signum", f::signum::write, f::signum::SignumWriteOptions, "DirectWriteArgs<SerdeTypes['formats::signum::SignumWriteOptions']>"),
    }
    #[cfg(feature = "all-formats")]
    snf "snf" ["snf"] {
        detect: (codec f::snf::SnfCodec::default()),
        read: (ctx_opts f::snf::read, f::snf::SnfReadOptions, "DirectContextReadArgs<SerdeTypes['formats::snf::SnfReadOptions']>", "ModelReport<FontSet>"),
        write: (set_wo_opts "snf", f::snf::write, f::snf::SnfWriteOptions, "DirectWriteArgs<SerdeTypes['formats::snf::SnfWriteOptions']>"),
    }
    #[cfg(feature = "all-formats")]
    ssfn "ssfn" ["ssfn"] {
        detect: (bytes f::ssfn::detect),
        read: (ctx_opts f::ssfn::read, f::ssfn::SsfnReadOptions, "DirectContextReadArgs<SerdeTypes['formats::ssfn::SsfnReadOptions']>", "ModelReport<FontSet>"),
        write: (set_wo_opts "ssfn", f::ssfn::write, f::ssfn::SsfnWriteOptions, "DirectWriteArgs<SerdeTypes['formats::ssfn::SsfnWriteOptions']>"),
    }
    #[cfg(feature = "all-formats")]
    svg "svg" ["svg"] {
        detect: (codec f::svg::SvgCodec::default()),
        read: (bytes_opts f::svg::read, f::svg::ReadOptions, "{ options?: SerdeTypes['formats::svg::ReadOptions'] }", "ModelReport<FontSet>"),
        write: (x_opts FontSet, f::svg::write, f::svg::WriteOptions, "{ options?: SerdeTypes['formats::svg::WriteOptions'] }"),
    }
    #[cfg(feature = "all-formats")]
    symbos "symbos" ["symbos"] {
        detect: (codec f::symbos::SymbosCodec::default()),
        read: (ctx_opts f::symbos::read, f::symbos::SymbosReadOptions, "DirectContextReadArgs<SerdeTypes['formats::symbos::SymbosReadOptions']>", "ModelReport<FontSet>"),
        write: (set_wo_opts "symbos", f::symbos::write, f::symbos::SymbosWriteOptions, "DirectWriteArgs<SerdeTypes['formats::symbos::SymbosWriteOptions']>"),
    }
    #[cfg(feature = "all-formats")]
    tasprint "tasprint" ["tasprint"] {
        detect: (bytes f::tasprint::detect),
        read: (ctx_opts f::tasprint::read, f::tasprint::TasprintReadOptions, "DirectContextReadArgs<SerdeTypes['formats::tasprint::TasprintReadOptions']>", "ModelReport<FontSet>"),
        write: (set_wo_opts "tasprint", f::tasprint::write, f::tasprint::TasprintWriteOptions, "DirectWriteArgs<SerdeTypes['formats::tasprint::TasprintWriteOptions']>"),
    }
    #[cfg(feature = "all-formats")]
    u8m "u8m" ["u8m"] {
        detect: (bytes f::u8m::detect),
        read: (ctx_opts f::u8m::read, f::u8m::U8mReadOptions, "DirectContextReadArgs<SerdeTypes['formats::u8m::U8mReadOptions']>", "ModelReport<FontSet>"),
        write: (set_wo_opts "u8m", f::u8m::write, f::u8m::U8mWriteOptions, "DirectWriteArgs<SerdeTypes['formats::u8m::U8mWriteOptions']>"),
    }
    #[cfg(feature = "all-formats")]
    vfont "vfont" ["vfont"] {
        detect: (bytes f::vfont::detect),
        read: (ctx_opts f::vfont::read, f::vfont::VfontReadOptions, "DirectContextReadArgs<SerdeTypes['formats::vfont::VfontReadOptions']>", "ModelReport<FontSet>"),
        write: (set_wo_opts "vfont", f::vfont::write, f::vfont::VfontWriteOptions, "DirectWriteArgs<SerdeTypes['formats::vfont::VfontWriteOptions']>"),
    }
    #[cfg(feature = "all-formats")]
    vlw "vlw" ["vlw"] {
        detect: (bytes f::vlw::detect),
        read: (ctx_opts f::vlw::read, f::vlw::VlwReadOptions, "DirectContextReadArgs<SerdeTypes['formats::vlw::VlwReadOptions']>", "ModelReport<FontSet>"),
        write: (set_wo_opts "vlw", f::vlw::write, f::vlw::VlwWriteOptions, "DirectWriteArgs<SerdeTypes['formats::vlw::VlwWriteOptions']>"),
    }
    #[cfg(feature = "all-formats")]
    vtfont "vtfont" ["vtfont"] {
        detect: (codec f::vtfont::VtfontCodec::default()),
        read: (ctx_opts f::vtfont::read, f::vtfont::VtfontReadOptions, "DirectContextReadArgs<SerdeTypes['formats::vtfont::VtfontReadOptions']>", "ModelReport<FontSet>"),
        write: (set_wo_opts "vtfont", f::vtfont::write, f::vtfont::VtfontWriteOptions, "DirectWriteArgs<SerdeTypes['formats::vtfont::VtfontWriteOptions']>"),
    }
    #[cfg(feature = "all-formats")]
    writeon "writeon" ["writeon"] {
        detect: (bytes f::writeon::detect),
        read: (ctx_opts f::writeon::read, f::writeon::WriteonReadOptions, "DirectContextReadArgs<SerdeTypes['formats::writeon::WriteonReadOptions']>", "ModelReport<FontSet>"),
        write: (set_wo_opts "writeon", f::writeon::write, f::writeon::WriteonWriteOptions, "DirectWriteArgs<SerdeTypes['formats::writeon::WriteonWriteOptions']>"),
    }
    #[cfg(feature = "all-formats")]
    wsfont "wsfont" ["wsfont"] {
        detect: (codec f::wsfont::WsfontCodec::default()),
        read: (ctx_opts f::wsfont::read, f::wsfont::WsfontReadOptions, "DirectContextReadArgs<SerdeTypes['formats::wsfont::WsfontReadOptions']>", "ModelReport<FontSet>"),
        write: (set_wo_opts "wsfont", f::wsfont::write, f::wsfont::WsfontWriteOptions, "DirectWriteArgs<SerdeTypes['formats::wsfont::WsfontWriteOptions']>"),
    }
    #[cfg(feature = "all-formats")]
    wyse "wyse" ["wyse"] {
        detect: (bytes f::wyse::detect),
        read: (ctx_opts f::wyse::read, f::wyse::WyseReadOptions, "DirectContextReadArgs<SerdeTypes['formats::wyse::WyseReadOptions']>", "ModelReport<FontSet>"),
        write: (set_wo_opts "wyse", f::wyse::write, f::wyse::WyseWriteOptions, "DirectWriteArgs<SerdeTypes['formats::wyse::WyseWriteOptions']>"),
    }
    #[cfg(feature = "all-formats")]
    xbin "xbin" ["xbin"] {
        detect: (bytes f::xbin::detect),
        read: (ctx_opts f::xbin::read, f::xbin::XbinReadOptions, "DirectContextReadArgs<SerdeTypes['formats::xbin::XbinReadOptions']>", "ModelReport<FontSet>"),
        write: (set_wo_opts "xbin", f::xbin::write, f::xbin::XbinWriteOptions, "DirectWriteArgs<SerdeTypes['formats::xbin::XbinWriteOptions']>"),
    }
    #[cfg(feature = "all-formats")]
    xerox "xerox" ["xerox"] {
        detect: (bytes f::xerox::detect),
        read: (ctx_opts f::xerox::read, f::xerox::XeroxReadOptions, "DirectContextReadArgs<SerdeTypes['formats::xerox::XeroxReadOptions']>", "ModelReport<FontSet>"),
        write: (set_wo_opts "xerox", f::xerox::write, f::xerox::XeroxWriteOptions, "DirectWriteArgs<SerdeTypes['formats::xerox::XeroxWriteOptions']>"),
    }
    #[cfg(any(feature = "all-formats", feature = "minimal"))]
    yaff "yaff" ["yaff"] {
        detect: (bytes f::yaff::detect),
        read: (bytes_limits f::yaff::read, "{ limits?: ResourceLimits }", "ModelReport<FontSet>"),
        write: (x_opts FontSet, f::yaff::write, f::yaff::YaffWriteOptions, "{ options?: SerdeTypes['formats::yaff::YaffWriteOptions'] }"),
    }
    #[cfg(feature = "all-formats")]
    zapfont "zapfont" ["zapfont"] {
        detect: (bytes f::zapfont::detect),
        read: (ctx_opts f::zapfont::read, f::zapfont::ZapfontReadOptions, "DirectContextReadArgs<SerdeTypes['formats::zapfont::ZapfontReadOptions']>", "ModelReport<FontSet>"),
        write: (set_wo_opts "zapfont", f::zapfont::write, f::zapfont::ZapfontWriteOptions, "DirectWriteArgs<SerdeTypes['formats::zapfont::ZapfontWriteOptions']>"),
    }
}
