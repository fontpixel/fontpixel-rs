//! `Bitmap` handle methods (`fontpixel::bitmap`, `fontpixel::model::GlyphBitmap`).
//!
//! Foundation pilot: construction, pixels, bytes and plain data. The bitmap
//! area extends the class in this file.
use crate::conv::{self, Options};
use crate::errors::{IntoJs, JsResult, invalid};
use crate::handles::JsBitmap;
use fontpixel::bitmap::{Bitmap, BitmapLimits, ByteLayout, Gray8, Mono};
use fontpixel::model::GlyphBitmap;
use js_sys::Uint8Array;
use wasm_bindgen::prelude::*;

macro_rules! each {
    ($value:expr, |$b:ident| $body:expr) => {
        match $value {
            GlyphBitmap::Mono($b) => $body,
            GlyphBitmap::Gray8($b) => $body,
        }
    };
}

/// Pixel format selected by a `depth` argument (`"mono"` or `"gray8"`).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Depth {
    /// `Bitmap<Mono>`.
    Mono,
    /// `Bitmap<Gray8>`.
    Gray8,
}

/// Parse an optional `depth` string (default `"mono"`).
pub fn depth_arg(depth: Option<String>) -> JsResult<Depth> {
    match depth.as_deref() {
        None | Some("mono") => Ok(Depth::Mono),
        Some("gray8") => Ok(Depth::Gray8),
        Some(other) => Err(invalid(format!(
            "depth must be \"mono\" or \"gray8\", not {other:?}"
        ))),
    }
}

fn depth_of(bitmap: &GlyphBitmap) -> Depth {
    match bitmap {
        GlyphBitmap::Mono(_) => Depth::Mono,
        GlyphBitmap::Gray8(_) => Depth::Gray8,
    }
}

#[wasm_bindgen(js_class = Bitmap)]
impl JsBitmap {
    /// A blank `width` × `height` raster (`depth` `"mono"` (default) or
    /// `"gray8"`).
    #[wasm_bindgen(constructor)]
    pub fn constructor(
        width: Option<f64>,
        height: Option<f64>,
        #[wasm_bindgen(unchecked_param_type = "Depth")] depth: Option<String>,
    ) -> JsResult<JsBitmap> {
        let width: u32 = conv::int(width.unwrap_or(0.0), "width")?;
        let height: u32 = conv::int(height.unwrap_or(0.0), "height")?;
        Ok(JsBitmap::new(match depth_arg(depth)? {
            Depth::Mono => Bitmap::<Mono>::new(width, height).js()?.into(),
            Depth::Gray8 => Bitmap::<Gray8>::new(width, height).js()?.into(),
        }))
    }
    /// Parse Mono text rows (`#`/`1` ink, `.`/`0` paper); blank leading and
    /// trailing lines are ignored (`Bitmap::from_text`).
    #[wasm_bindgen(js_name = fromText)]
    pub fn from_text(text: &str) -> JsResult<JsBitmap> {
        let palette = [('#', 1), ('.', 0), ('1', 1), ('0', 0)];
        Ok(JsBitmap::new(
            Bitmap::<Mono>::from_text(text, &palette, BitmapLimits::default())
                .js()?
                .into(),
        ))
    }
    /// From an exact-sized byte stream in `options.layout` (a `ByteLayout`;
    /// default MSB-first byte rows, top-down) with `options.depth` and
    /// `options.limits` (`Bitmap::from_bytes`).
    #[wasm_bindgen(js_name = fromBytes)]
    pub fn from_bytes(
        width: f64,
        height: f64,
        #[wasm_bindgen(unchecked_param_type = "BytesLike")] data: JsValue,
        #[wasm_bindgen(
            unchecked_param_type = "{ layout?: ByteLayout; depth?: Depth; limits?: BitmapLimits }"
        )]
        options: JsValue,
    ) -> JsResult<JsBitmap> {
        let width: u32 = conv::int(width, "width")?;
        let height: u32 = conv::int(height, "height")?;
        let data = conv::bytes_from_js(&data, "data")?;
        let options = Options::new(&options, &["layout", "depth", "limits"], "fromBytes options")?;
        let layout: ByteLayout = options.plain("layout")?.unwrap_or_default();
        let limits: BitmapLimits = options.plain("limits")?.unwrap_or_default();
        Ok(JsBitmap::new(match depth_arg(options.string("depth")?)? {
            Depth::Mono => Bitmap::<Mono>::from_bytes(width, height, &data, layout, limits)
                .js()?
                .into(),
            Depth::Gray8 => Bitmap::<Gray8>::from_bytes(width, height, &data, layout, limits)
                .js()?
                .into(),
        }))
    }
    /// Build from the plain-data form produced by `toDict()`.
    #[wasm_bindgen(js_name = fromDict)]
    pub fn from_dict(
        #[wasm_bindgen(unchecked_param_type = "GlyphBitmap")] data: JsValue,
    ) -> JsResult<JsBitmap> {
        Ok(JsBitmap::new(conv::from_js(&data)?))
    }
    /// The plain-data form (`{mono: {width, height, data}}` or `{gray8: …}`).
    #[wasm_bindgen(js_name = toDict, unchecked_return_type = "GlyphBitmap")]
    pub fn to_dict(&self) -> JsResult<JsValue> {
        self.read(conv::to_js)
    }
    /// `"mono"` or `"gray8"`.
    #[wasm_bindgen(getter, unchecked_return_type = "Depth")]
    pub fn depth(&self) -> JsResult<String> {
        self.read(|b| {
            Ok(match depth_of(b) {
                Depth::Mono => "mono".to_owned(),
                Depth::Gray8 => "gray8".to_owned(),
            })
        })
    }
    /// Width in pixels.
    #[wasm_bindgen(getter)]
    pub fn width(&self) -> JsResult<u32> {
        self.read(|b| Ok(each!(b, |b| b.width())))
    }
    /// Height in pixels.
    #[wasm_bindgen(getter)]
    pub fn height(&self) -> JsResult<u32> {
        self.read(|b| Ok(each!(b, |b| b.height())))
    }
    /// Pixel value at `(x, y)` (`Bitmap::get_pixel`).
    #[wasm_bindgen(js_name = getPixel)]
    pub fn get_pixel(&self, x: f64, y: f64) -> JsResult<u8> {
        let (x, y) = (conv::int(x, "x")?, conv::int(y, "y")?);
        self.read(|b| each!(b, |b| b.get_pixel(x, y)).js())
    }
    /// Set the pixel at `(x, y)`; the value must fit the depth.
    #[wasm_bindgen(js_name = setPixel)]
    pub fn set_pixel(&self, x: f64, y: f64, value: f64) -> JsResult<()> {
        let (x, y) = (conv::int(x, "x")?, conv::int(y, "y")?);
        let value: u8 = conv::int(value, "value")?;
        self.write(|b| each!(b, |b| b.set_pixel(x, y, value)).js())
    }
    /// A copy of the canonical storage (`Bitmap::as_bytes`): top-down rows,
    /// Mono packed MSB-first and padded to whole bytes, Gray8 one byte per
    /// pixel.
    #[wasm_bindgen(js_name = asBytes)]
    pub fn as_bytes(&self) -> JsResult<Uint8Array> {
        self.read(|b| Ok(conv::bytes_to_js(each!(b, |b| b.as_bytes()))))
    }
    /// Text rows, `.` paper and `#` ink (Mono only) (`Bitmap::to_text`).
    #[wasm_bindgen(js_name = toText)]
    pub fn to_text(&self) -> JsResult<String> {
        self.read(|b| match b {
            GlyphBitmap::Mono(b) => b.to_text(&[(0, '.'), (1, '#')]).js(),
            GlyphBitmap::Gray8(_) => Err(invalid("toText needs a mono bitmap")),
        })
    }
    /// Whether `other` has the same depth, size and pixels.
    pub fn equals(&self, other: &JsBitmap) -> JsResult<bool> {
        let other = other.snapshot()?;
        self.read(|b| Ok(*b == other))
    }
    /// An independent copy.
    pub fn copy(&self) -> JsResult<JsBitmap> {
        Ok(JsBitmap::new(self.snapshot()?))
    }
}
