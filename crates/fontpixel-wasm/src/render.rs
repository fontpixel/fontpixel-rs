//! Text rendering (`Font.draw*`), `Rendered` results and PNG/SVG export
//! (`fontpixel::render`).
//!
//! `RenderOptions` is a plain object with the Rust field names
//! (`line_limit`, `mode` `"advance"`/`"cell"`/`"ink"`, `direction`
//! `{glyphs, lines}`, `use_current_glyph_spacing`, `letter_spacing`,
//! `line_spacing`, `missing` `"blank"`/`"error"`, `gray_threshold`, `strict`,
//! `limits`); omitted fields take the Rust defaults. The
//! `MissingGlyph::Glyph` replacement is the separate `missingGlyph` argument
//! (a `Glyph` handle or `GlyphDict` plain data), because the Rust option
//! borrows it. Colors are `{red, green, blue, alpha}` plain data.
use crate::bitmap::copied_handle;
use crate::conv;
use crate::errors::{IntoJs, JsResult, invalid};
use crate::handles::{JsBitmap, JsFont, JsGlyph};
use crate::model::codepoint_arg;
use fontpixel::model::{Codepoint, Glyph, GlyphBitmap};
use fontpixel::render::{
    GlyphOrder, ImageOptions, MissingGlyph, RenderLimits, RenderOptions, Rendered, Rgba,
    WritingDirection,
};
use js_sys::{Array, Uint8Array};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

/// Result of a render call (`fontpixel::render::Rendered`): the Mono
/// `bitmap` (top-left origin) and the `report` with every diagnostic.
#[wasm_bindgen(js_name = Rendered)]
pub struct JsRendered {
    rendered: Rendered,
}

impl JsRendered {
    /// Wrap a Rust result.
    pub fn new(rendered: Rendered) -> Self {
        Self { rendered }
    }
    /// The wrapped result.
    pub fn rendered(&self) -> &Rendered {
        &self.rendered
    }
}

#[wasm_bindgen(js_class = Rendered)]
impl JsRendered {
    /// The rendered pixels: a fresh Mono `Bitmap` copy on every read.
    #[wasm_bindgen(getter)]
    pub fn bitmap(&self) -> JsBitmap {
        JsBitmap::new(self.rendered.bitmap.clone().into())
    }
    /// The complete report plain data (`{entries: [...]}`).
    #[wasm_bindgen(getter, unchecked_return_type = "Report")]
    pub fn report(&self) -> JsResult<JsValue> {
        conv::to_js(&self.rendered.report)
    }
    /// Plain data: `{bitmap: {width, height, data}, report: {...}}`.
    #[wasm_bindgen(js_name = toDict, unchecked_return_type = "RenderedDict")]
    pub fn to_dict(&self) -> JsResult<JsValue> {
        conv::to_js(&self.rendered)
    }
    /// `Rendered(bitmap=WxH, report_entries=N)`.
    #[wasm_bindgen(js_name = toString)]
    pub fn to_string_js(&self) -> String {
        format!(
            "Rendered(bitmap={}x{}, report_entries={})",
            self.rendered.bitmap.width(),
            self.rendered.bitmap.height(),
            self.rendered.report.entries.len()
        )
    }
}

/// The replacement glyph argument: a `Glyph` handle (copied) or `GlyphDict`
/// plain data; `undefined`/`null` for none.
fn missing_arg(value: Option<JsValue>) -> JsResult<Option<Glyph>> {
    let Some(value) = value.filter(|v| !v.is_undefined() && !v.is_null()) else {
        return Ok(None);
    };
    if let Some(handle) = copied_handle::<JsGlyph>(&value) {
        return handle?.snapshot().map(Some);
    }
    conv::from_js(&value).map(Some)
}

/// `RenderOptions` from plain data, with an optional borrowed replacement.
pub fn render_options<'a>(
    options: Option<JsValue>,
    missing_glyph: Option<&'a Glyph>,
) -> JsResult<RenderOptions<'a>> {
    let options: RenderOptions<'static> =
        conv::from_js_or_default(&options.unwrap_or(JsValue::UNDEFINED))?;
    let mut options: RenderOptions<'a> = options;
    if let Some(glyph) = missing_glyph {
        options.missing = MissingGlyph::Glyph(glyph);
    }
    Ok(options)
}

/// One `drawCodepoints` entry: `null`/`undefined` (unencodable), a number
/// (Unicode), a one-character string (Unicode) or `{Unicode: n}` /
/// `{Legacy: n}`.
fn code_arg(value: &JsValue) -> JsResult<Option<Codepoint>> {
    if value.is_undefined() || value.is_null() {
        return Ok(None);
    }
    if let Some(text) = value.as_string() {
        let mut chars = text.chars();
        return match (chars.next(), chars.next()) {
            (Some(c), None) => Ok(Some(Codepoint::Unicode(c as u32))),
            _ => Err(invalid(format!(
                "codepoint strings must be exactly one character, got {text:?}"
            ))),
        };
    }
    codepoint_arg(value).map(Some)
}

#[wasm_bindgen(js_class = Font)]
impl JsFont {
    /// Render Unicode `text` through the font's charset map (`Font::draw`).
    /// Returns `Rendered`; throws `StrictLossError` (with `report`) in strict
    /// mode and `ResourceLimitError` over budget. `missingGlyph` (a `Glyph`
    /// or `GlyphDict`) replaces missing codes.
    pub fn draw(
        &self,
        text: &str,
        #[wasm_bindgen(unchecked_optional_param_type = "RenderOptions")] options: Option<JsValue>,
        #[wasm_bindgen(unchecked_optional_param_type = "Glyph | GlyphDict")] missing_glyph: Option<
            JsValue,
        >,
    ) -> JsResult<JsRendered> {
        let missing = missing_arg(missing_glyph)?;
        let options = render_options(options, missing.as_ref())?;
        self.read(|f| f.draw(text, &options).js().map(JsRendered::new))
    }
    /// Render explicit codes (`Font::draw_codepoints`): numbers or
    /// one-character strings are Unicode, `{Legacy: n}` names legacy codes,
    /// `null` stands for an unencodable character.
    #[wasm_bindgen(js_name = drawCodepoints)]
    pub fn draw_codepoints(
        &self,
        #[wasm_bindgen(unchecked_param_type = "(CodepointLike | string | null)[]")] codes: JsValue,
        #[wasm_bindgen(unchecked_optional_param_type = "RenderOptions")] options: Option<JsValue>,
        #[wasm_bindgen(unchecked_optional_param_type = "Glyph | GlyphDict")] missing_glyph: Option<
            JsValue,
        >,
    ) -> JsResult<JsRendered> {
        let list = codes
            .dyn_ref::<Array>()
            .ok_or_else(|| invalid("drawCodepoints: codes must be an array"))?;
        let codes = list
            .iter()
            .map(|c| code_arg(&c))
            .collect::<JsResult<Vec<_>>>()?;
        let missing = missing_arg(missing_glyph)?;
        let options = render_options(options, missing.as_ref())?;
        self.read(|f| f.draw_codepoints(codes, &options).js().map(JsRendered::new))
    }
    /// Render every effective mapping in `order` (`"ascending"` (default),
    /// `"descending"`, `"file"`, `"reverse_file"`), optionally restricted to
    /// inclusive `[start, end]` ranges (`Font::draw_all`).
    #[wasm_bindgen(js_name = drawAll)]
    pub fn draw_all(
        &self,
        #[wasm_bindgen(unchecked_optional_param_type = "RenderGlyphOrder")] order: Option<JsValue>,
        #[wasm_bindgen(unchecked_optional_param_type = "[number, number][]")] ranges: Option<
            JsValue,
        >,
        #[wasm_bindgen(unchecked_optional_param_type = "RenderOptions")] options: Option<JsValue>,
        #[wasm_bindgen(unchecked_optional_param_type = "Glyph | GlyphDict")] missing_glyph: Option<
            JsValue,
        >,
    ) -> JsResult<JsRendered> {
        let order: GlyphOrder = conv::from_js_or_default(&order.unwrap_or(JsValue::UNDEFINED))?;
        let ranges: Vec<(u32, u32)> =
            conv::from_js_or_default(&ranges.unwrap_or(JsValue::UNDEFINED))?;
        let missing = missing_arg(missing_glyph)?;
        let options = render_options(options, missing.as_ref())?;
        self.read(|f| {
            f.draw_all(order, &ranges, &options)
                .js()
                .map(JsRendered::new)
        })
    }
}

fn mono_arg(bitmap: &JsBitmap) -> JsResult<fontpixel::Bitmap> {
    match bitmap.snapshot()? {
        GlyphBitmap::Mono(b) => Ok(b),
        GlyphBitmap::Gray8(_) => Err(invalid(
            "image export takes a mono bitmap; convert with toMono(threshold)",
        )),
    }
}

/// Encode a Mono bitmap as an indexed 1-bit PNG (`render::image::to_png`).
/// `options` is an `ImageOptions` object (`scale`, `foreground`,
/// `background`, `limits`).
#[wasm_bindgen(js_name = toPng)]
pub fn to_png(
    bitmap: &JsBitmap,
    #[wasm_bindgen(unchecked_optional_param_type = "ImageOptions")] options: Option<JsValue>,
) -> JsResult<Uint8Array> {
    let bitmap = mono_arg(bitmap)?;
    let options: ImageOptions = conv::from_js_or_default(&options.unwrap_or(JsValue::UNDEFINED))?;
    let bytes = fontpixel::render::to_png(&bitmap, &options).js()?;
    Ok(conv::bytes_to_js(&bytes))
}

/// Export a Mono bitmap as a standalone SVG string (`render::image::to_svg`).
#[wasm_bindgen(js_name = toSvg)]
pub fn to_svg(
    bitmap: &JsBitmap,
    #[wasm_bindgen(unchecked_optional_param_type = "ImageOptions")] options: Option<JsValue>,
) -> JsResult<String> {
    let bitmap = mono_arg(bitmap)?;
    let options: ImageOptions = conv::from_js_or_default(&options.unwrap_or(JsValue::UNDEFINED))?;
    fontpixel::render::to_svg(&bitmap, &options).js()
}

/// An RGBA color (`Rgba::new`); `alpha` defaults to 255.
#[wasm_bindgen(unchecked_return_type = "Rgba")]
pub fn rgba(red: f64, green: f64, blue: f64, alpha: Option<f64>) -> JsResult<JsValue> {
    let channel = |v: f64, what| conv::int::<u8>(v, what);
    conv::to_js(&Rgba::new(
        channel(red, "red")?,
        channel(green, "green")?,
        channel(blue, "blue")?,
        channel(alpha.unwrap_or(255.0), "alpha")?,
    ))
}

/// Parse `#RGB`, `#RGBA`, `#RRGGBB`, `#RRGGBBAA` or `transparent`
/// (`Rgba: FromStr`); throws `InvalidInputError` otherwise.
#[wasm_bindgen(js_name = parseRgba, unchecked_return_type = "Rgba")]
pub fn parse_rgba(text: &str) -> JsResult<JsValue> {
    let color: Rgba = text.parse().js()?;
    conv::to_js(&color)
}

/// `Rgba::BLACK` (opaque black).
#[wasm_bindgen(js_name = rgbaBlack, unchecked_return_type = "Rgba")]
pub fn rgba_black() -> JsResult<JsValue> {
    conv::to_js(&Rgba::BLACK)
}

/// `Rgba::WHITE` (opaque white).
#[wasm_bindgen(js_name = rgbaWhite, unchecked_return_type = "Rgba")]
pub fn rgba_white() -> JsResult<JsValue> {
    conv::to_js(&Rgba::WHITE)
}

/// `Rgba::TRANSPARENT` (all channels zero).
#[wasm_bindgen(js_name = rgbaTransparent, unchecked_return_type = "Rgba")]
pub fn rgba_transparent() -> JsResult<JsValue> {
    conv::to_js(&Rgba::TRANSPARENT)
}

/// Lenient SPEC direction parsing: invalid strings give `lrtb`
/// (`WritingDirection::from_spec`). Returns `{glyphs, lines}`.
#[wasm_bindgen(js_name = writingDirectionFromSpec, unchecked_return_type = "WritingDirection")]
pub fn writing_direction_from_spec(spec: &str) -> JsResult<JsValue> {
    conv::to_js(&WritingDirection::from_spec(spec))
}

/// Strict direction parsing (`WritingDirection: FromStr`); throws
/// `InvalidInputError` for unknown spellings.
#[wasm_bindgen(js_name = parseWritingDirection, unchecked_return_type = "WritingDirection")]
pub fn parse_writing_direction(spec: &str) -> JsResult<JsValue> {
    let direction: WritingDirection = spec.parse().js()?;
    conv::to_js(&direction)
}

/// The default `RenderOptions` (plain data).
#[wasm_bindgen(js_name = defaultRenderOptions, unchecked_return_type = "RenderOptions")]
pub fn default_render_options() -> JsResult<JsValue> {
    conv::to_js(&RenderOptions::default())
}

/// The default `RenderLimits` (plain data).
#[wasm_bindgen(js_name = defaultRenderLimits, unchecked_return_type = "RenderLimits")]
pub fn default_render_limits() -> JsResult<JsValue> {
    conv::to_js(&RenderLimits::default())
}

/// The default `ImageOptions` (plain data).
#[wasm_bindgen(js_name = defaultImageOptions, unchecked_return_type = "ImageOptions")]
pub fn default_image_options() -> JsResult<JsValue> {
    conv::to_js(&ImageOptions::default())
}
