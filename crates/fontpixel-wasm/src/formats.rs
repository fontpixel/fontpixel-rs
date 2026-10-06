//! Reading and writing formats (`fontpixel::formats`).
//!
//! Foundation pilot: `read(data, options)` and the `Loaded` result. The
//! formats area extends this file (registry, detection, writing, per-format
//! namespaces and options).
use crate::conv::{self, Options};
use crate::errors::{IntoJs, JsResult};
use crate::handles::JsFontSet;
use crate::resources::ResourcesArg;
use fontpixel::Report;
use fontpixel::formats::ReadOptions;
use fontpixel::resources::{Input, ResourceLimits};
use wasm_bindgen::prelude::*;

/// A decoded model with its read report and the format actually used
/// (`fontpixel::formats::Loaded`).
#[wasm_bindgen(js_name = Loaded)]
pub struct JsLoaded {
    fonts: crate::handles::Shared<fontpixel::FontSet>,
    report: Report,
    format: &'static str,
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
}

/// Decode `data` with the built-in registry (`fontpixel::read`, plus a name
/// hint and sidecars). `options.format` bypasses detection; `options.name`
/// is the filename hint (never content evidence); `options.resources`
/// supplies sidecars (see `ResourcesLike`); `options.limits` is a
/// `ResourceLimits` object.
#[wasm_bindgen]
pub fn read(
    #[wasm_bindgen(unchecked_param_type = "BytesLike")] data: JsValue,
    #[wasm_bindgen(unchecked_optional_param_type = "ReadArgs")] options: Option<JsValue>,
) -> JsResult<JsLoaded> {
    let data = conv::bytes_from_js(&data, "data")?;
    let options = Options::new(
        &options.unwrap_or(JsValue::UNDEFINED),
        &["format", "name", "resources", "limits"],
        "read options",
    )?;
    let name = options.string("name")?;
    let resources = ResourcesArg::new(&options.get("resources")?.unwrap_or(JsValue::UNDEFINED))?;
    let read_options = ReadOptions {
        format: options.string("format")?,
        limits: options
            .plain::<ResourceLimits>("limits")?
            .unwrap_or_default(),
    };
    let mut input = Input::new(&data).with_resources(resources.resolver());
    if let Some(name) = name.as_deref() {
        input = input.with_name(name);
    }
    let loaded = resources.finish(fontpixel::registry().read(&input, &read_options).js())?;
    Ok(JsLoaded {
        fonts: crate::handles::shared(loaded.fonts),
        report: loaded.report,
        format: loaded.format,
    })
}
