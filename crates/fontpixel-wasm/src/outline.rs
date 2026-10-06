//! Deterministic bitmap contours, segments and rasterization
//! (`fontpixel::outline`), plus line/quadratic/cubic edge rasterization
//! (`fontpixel::outline::path`).
//!
//! Points are `[x, y, onCurve]` arrays in upward design units; contours are
//! arrays of points without a repeated closing point (`Contour`). Path
//! edges are `{line: [p0, p1]}`, `{quad: [p0, p1, p2]}` or
//! `{cubic: [p0, p1, p2, p3]}` with `[x, y]` points (`Edge`). Bitmap inputs
//! are Mono `Bitmap` handles (Gray8 throws `InvalidInputError`: threshold it
//! with `toMono` first); rasterizers return new Mono `Bitmap` handles.
//! `OutlineOptions` and `OutlineLimits` are plain objects with omitted
//! fields defaulted (`outlineDefaults()`).
use crate::conv;
use crate::errors::{IntoJs, JsResult, invalid};
use crate::handles::JsBitmap;
use fontpixel::Bitmap;
use fontpixel::model::GlyphBitmap;
use fontpixel::outline::{self, Contour, Limits, Options, path};
use js_sys::{Object, Reflect};
use wasm_bindgen::prelude::*;

fn mono(bitmap: &JsBitmap) -> JsResult<Bitmap> {
    match bitmap.snapshot()? {
        GlyphBitmap::Mono(b) => Ok(b),
        GlyphBitmap::Gray8(_) => Err(invalid(
            "outline functions take a mono bitmap; threshold gray8 explicitly with toMono",
        )),
    }
}

fn opt<T: serde::de::DeserializeOwned + Default>(value: Option<JsValue>) -> JsResult<T> {
    conv::from_js_or_default(&value.unwrap_or(JsValue::UNDEFINED))
}

/// Trace a Mono bitmap into closed contours (`outline::contours`).
#[wasm_bindgen(js_name = outlineContours, unchecked_return_type = "Contour[]")]
pub fn contours(
    bitmap: &JsBitmap,
    #[wasm_bindgen(unchecked_optional_param_type = "OutlineOptions | null")] options: Option<
        JsValue,
    >,
    #[wasm_bindgen(unchecked_optional_param_type = "OutlineLimits | null")] limits: Option<JsValue>,
) -> JsResult<JsValue> {
    let b = mono(bitmap)?;
    let (o, l): (Options, Limits) = (opt(options)?, opt(limits)?);
    conv::to_js(&outline::contours(&b, &o, &l).js()?)
}

/// One independent square/dot inside the cell at `(0, 0)`
/// (`outline::pixel_contour`).
#[wasm_bindgen(js_name = outlinePixelContour, unchecked_return_type = "Contour")]
pub fn pixel_contour(
    #[wasm_bindgen(unchecked_optional_param_type = "OutlineOptions | null")] options: Option<
        JsValue,
    >,
) -> JsResult<JsValue> {
    let o: Options = opt(options)?;
    conv::to_js(&outline::pixel_contour(&o).js()?)
}

/// Lower-left positions of ink cells, top-to-bottom, as `[x, y]`
/// (`outline::pixel_origins`).
#[wasm_bindgen(js_name = outlinePixelOrigins, unchecked_return_type = "[number, number][]")]
pub fn pixel_origins(
    bitmap: &JsBitmap,
    #[wasm_bindgen(unchecked_optional_param_type = "OutlineOptions | null")] options: Option<
        JsValue,
    >,
    #[wasm_bindgen(unchecked_optional_param_type = "OutlineLimits | null")] limits: Option<JsValue>,
) -> JsResult<JsValue> {
    let b = mono(bitmap)?;
    let (o, l): (Options, Limits) = (opt(options)?, opt(limits)?);
    conv::to_js(&outline::pixel_origins(&b, &o, &l).js()?)
}

/// Segments of a contour (`{line: [a, b]}` / `{quad: [a, c, b]}`), with
/// implied on-curve midpoints resolved (`outline::segments`).
#[wasm_bindgen(js_name = outlineSegments, unchecked_return_type = "Segment[]")]
pub fn segments(
    #[wasm_bindgen(unchecked_param_type = "Contour")] contour: JsValue,
) -> JsResult<JsValue> {
    let c: Contour = conv::from_js(&contour)?;
    conv::to_js(&outline::segments(&c).js()?)
}

/// Unhinted nonzero-winding pixel-centre fill of TrueType contours into a
/// `width`×`height` Mono bitmap (`outline::rasterize`); `origin` (`[x, y]`)
/// and the dimensions are in pixels, `unitsPerPixel` is positive.
#[wasm_bindgen(js_name = outlineRasterize)]
pub fn rasterize(
    #[wasm_bindgen(unchecked_param_type = "Contour[]")] contours: JsValue,
    units_per_pixel: f64,
    #[wasm_bindgen(unchecked_param_type = "[number, number]")] origin: JsValue,
    width: f64,
    height: f64,
    #[wasm_bindgen(unchecked_optional_param_type = "OutlineLimits | null")] limits: Option<JsValue>,
) -> JsResult<JsBitmap> {
    let cs: Vec<Contour> = conv::from_js(&contours)?;
    let origin: (i32, i32) = conv::from_js(&origin)?;
    let (width, height) = (conv::int(width, "width")?, conv::int(height, "height")?);
    let l: Limits = opt(limits)?;
    let b = outline::rasterize(&cs, units_per_pixel, origin, width, height, &l).js()?;
    Ok(JsBitmap::new(b.into()))
}

/// The exact SPEC row adapter shared with the bdfparser compatibility layer
/// (`'0'` is paper, any other character ink) (`outline::spec_bitmap_contours`).
#[wasm_bindgen(js_name = specBitmapContours, unchecked_return_type = "Contour[]")]
pub fn spec_bitmap_contours(
    #[wasm_bindgen(unchecked_param_type = "string[]")] rows: JsValue,
    shape: &str,
    dotsize: f64,
    #[wasm_bindgen(unchecked_param_type = "WideInt")] ppu: JsValue,
    #[wasm_bindgen(unchecked_param_type = "WideInt")] xoff: JsValue,
    #[wasm_bindgen(unchecked_param_type = "WideInt")] yoff: JsValue,
) -> JsResult<JsValue> {
    let rows: Vec<String> = conv::from_js(&rows)?;
    let ppu = conv::int_from_js(&ppu, "ppu")?;
    let xoff = conv::int_from_js(&xoff, "xoff")?;
    let yoff = conv::int_from_js(&yoff, "yoff")?;
    conv::to_js(&outline::spec_bitmap_contours(&rows, shape, dotsize, ppu, xoff, yoff).js()?)
}

/// Nonzero-winding fill at exact pixel centres of `Edge`s into a Mono
/// bitmap (`outline::path::rasterize`); `origin` is `[x, y]`.
#[wasm_bindgen(js_name = pathRasterize)]
pub fn path_rasterize(
    #[wasm_bindgen(unchecked_param_type = "Edge[]")] edges: JsValue,
    scale: f64,
    #[wasm_bindgen(unchecked_param_type = "[number, number]")] origin: JsValue,
    width: f64,
    height: f64,
    #[wasm_bindgen(unchecked_optional_param_type = "OutlineLimits | null")] limits: Option<JsValue>,
) -> JsResult<JsBitmap> {
    let e: Vec<path::Edge> = conv::from_js(&edges)?;
    let origin: (f64, f64) = conv::from_js(&origin)?;
    let (width, height) = (conv::int(width, "width")?, conv::int(height, "height")?);
    let l: Limits = opt(limits)?;
    let b = path::rasterize(&e, scale, origin, width, height, &l).js()?;
    Ok(JsBitmap::new(b.into()))
}

/// Endpoints and controls of an edge, in path order (`Edge::points`).
#[wasm_bindgen(js_name = edgePoints, unchecked_return_type = "[number, number][]")]
pub fn edge_points(
    #[wasm_bindgen(unchecked_param_type = "Edge")] edge: JsValue,
) -> JsResult<JsValue> {
    conv::to_js(&conv::from_js::<path::Edge>(&edge)?.points())
}

/// The largest y of an edge, including interior cubic turns (`Edge::max_y`).
#[wasm_bindgen(js_name = edgeMaxY)]
pub fn edge_max_y(#[wasm_bindgen(unchecked_param_type = "Edge")] edge: JsValue) -> JsResult<f64> {
    Ok(conv::from_js::<path::Edge>(&edge)?.max_y())
}

/// The default `OutlineOptions` and `OutlineLimits`:
/// `{Options: {...}, Limits: {...}}`.
#[wasm_bindgen(
    js_name = outlineDefaults,
    unchecked_return_type = "{ Options: OutlineOptions; Limits: OutlineLimits }"
)]
pub fn outline_defaults() -> JsResult<JsValue> {
    let out = Object::new();
    Reflect::set(
        &out,
        &JsValue::from_str("Options"),
        &conv::to_js(&Options::default())?,
    )?;
    Reflect::set(
        &out,
        &JsValue::from_str("Limits"),
        &conv::to_js(&Limits::default())?,
    )?;
    Ok(out.into())
}
