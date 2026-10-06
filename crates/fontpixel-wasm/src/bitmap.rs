//! `Bitmap` handle methods: every `fontpixel::bitmap::Bitmap` operation for
//! both pixel formats (`fontpixel::bitmap`, `fontpixel::model::GlyphBitmap`).
//!
//! One JS class covers `Bitmap<Mono>` and `Bitmap<Gray8>`; `depth` is
//! `"mono"` (packed 1-bit, values 0..1) or `"gray8"` (one byte per pixel,
//! values 0..255). Conversions are explicit (`toGray8`, `toGray8Palette`,
//! `toMono`). Methods that exist for one format only throw
//! `InvalidInputError` on the other; binary operations require equal depths.
//! Coordinates are `(x, y)` from the top-left pixel; crop/bbox rectangles
//! use bottom-left offsets as in the Rust API. Pixel data stays in WASM
//! memory: `asBytes`, `toBytes`, `toRows` and `toList` return fresh
//! `Uint8Array` copies. Signed offsets (`crop`, `shift`, `composite`, …) are
//! 64-bit: `number` or `bigint`. Enum options are the serde strings:
//! `CompositeMode` (`"over"`, `"replace"`, …), `Direction` (`"right"`,
//! `"left"`, `"down"`, `"up"`), `Alignment` (`"start"`, `"end"`);
//! `ByteLayout` and `BitmapLimits` are plain objects with omitted fields
//! defaulted. Bitmap arguments are `Bitmap` handles (`Bitmap.fromDict` turns
//! plain data into one); they are copied before the receiver is borrowed,
//! so `b.paste(b)` works.
use crate::conv::{self, Options};
use crate::errors::{IntoJs, JsResult, invalid};
use crate::handles::JsBitmap;
use fontpixel::bitmap::{
    Alignment, Bitmap, BitmapLimits, Bounds, ByteLayout, CompositeMode, Direction, Gray8, Mono,
    PixelFormat,
};
use fontpixel::model::GlyphBitmap;
use js_sys::{Array, Function, Map, Object, Reflect, Uint8Array};
use wasm_bindgen::JsCast;
use wasm_bindgen::convert::TryFromJsValue;
use wasm_bindgen::prelude::*;

/// Apply `$body` to the typed bitmap inside a `GlyphBitmap`.
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

fn depth_name(bitmap: &GlyphBitmap) -> &'static str {
    match bitmap {
        GlyphBitmap::Mono(_) => "mono",
        GlyphBitmap::Gray8(_) => "gray8",
    }
}

fn mismatch() -> JsValue {
    invalid("bitmaps have different depths; convert one explicitly first")
}

fn only(name: &str, depth: &str) -> JsValue {
    invalid(format!("{name} is only available for {depth} bitmaps"))
}

// ------------------------------------------------------------ argument helpers

/// A signed 64-bit argument (`number` or `bigint`); `undefined`/`null` give
/// `default`.
pub fn i64_arg(value: Option<JsValue>, default: i64, what: &str) -> JsResult<i64> {
    match value {
        Some(v) if !v.is_undefined() && !v.is_null() => conv::int_from_js(&v, what),
        _ => Ok(default),
    }
}

/// An optional integer parameter declared `f64`.
fn opt_int<T: TryFrom<i64>>(value: Option<f64>, default: T, what: &str) -> JsResult<T> {
    value.map_or(Ok(default), |v| conv::int(v, what))
}

/// A copy of the raster of each `Bitmap` handle in `list` (an array). The
/// handles stay valid: each is copied through its own `copy()` and only the
/// copy is taken over.
pub fn bitmap_list(list: &JsValue, what: &str) -> JsResult<Vec<GlyphBitmap>> {
    let array = list
        .dyn_ref::<Array>()
        .ok_or_else(|| invalid(format!("{what}: expected an array of Bitmap handles")))?;
    array
        .iter()
        .enumerate()
        .map(|(i, item)| {
            copied_handle::<JsBitmap>(&item)
                .ok_or_else(|| invalid(format!("{what}: element {i} is not a Bitmap handle")))??
                .snapshot()
        })
        .collect()
}

/// `Some(copy)` when `value` is a handle of class `T`: the copy comes from
/// the handle's own `copy()` method (which also reports a released handle),
/// so the caller's handle stays valid. `None` for anything else.
///
/// wasm-bindgen cannot borrow a handle out of a `JsValue` (its conversion
/// takes the object over and releases the JS side), and `Option<&T>`
/// parameters are not supported, so optional or listed handle arguments go
/// through this copy.
pub fn copied_handle<T: TryFromJsValue>(value: &JsValue) -> Option<JsResult<T>> {
    if !value.is_object() {
        return None;
    }
    let copy = Reflect::get(value, &JsValue::from_str("copy"))
        .ok()?
        .dyn_into::<Function>()
        .ok()?;
    let fresh = match copy.call0(value) {
        Ok(fresh) => fresh,
        Err(error) => return Some(Err(error)),
    };
    match T::try_from_js_value(fresh) {
        Ok(handle) => Some(Ok(handle)),
        Err(other) => {
            // Another handle class's copy: release it right away.
            if let Ok(free) = Reflect::get(&other, &JsValue::from_str("free"))
                && let Ok(free) = free.dyn_into::<Function>()
            {
                let _ = free.call0(&other);
            }
            None
        }
    }
}

/// `[key, value]` pairs of an array of pairs, a `Map` or a plain object.
/// Arrays and maps keep duplicates and order; object keys are strings.
pub fn pairs(value: &JsValue, what: &str) -> JsResult<Vec<(JsValue, JsValue)>> {
    let entry = |item: JsValue| -> JsResult<(JsValue, JsValue)> {
        let pair = item
            .dyn_into::<Array>()
            .ok()
            .filter(|a| a.length() == 2)
            .ok_or_else(|| invalid(format!("{what}: expected [key, value] pairs")))?;
        Ok((pair.get(0), pair.get(1)))
    };
    if Array::is_array(value) {
        return Array::from(value).iter().map(entry).collect();
    }
    if let Some(map) = value.dyn_ref::<Map>() {
        let mut out = Vec::new();
        map.for_each(&mut |v, k| out.push((k, v)));
        return Ok(out);
    }
    if value.is_object() && !value.is_function() {
        return Object::entries(value.unchecked_ref())
            .iter()
            .map(entry)
            .collect();
    }
    Err(invalid(format!(
        "{what}: expected an object, a Map or an array of [key, value] pairs"
    )))
}

/// A one-character string.
pub fn char_arg(value: &JsValue, what: &str) -> JsResult<char> {
    let text = value
        .as_string()
        .ok_or_else(|| invalid(format!("{what}: expected a one-character string")))?;
    let mut chars = text.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) => Ok(c),
        _ => Err(invalid(format!(
            "{what}: expected exactly one character, got {text:?}"
        ))),
    }
}

/// A pixel value: a number, or a decimal string (plain-object keys).
fn value_arg(value: &JsValue, what: &str) -> JsResult<u8> {
    if let Some(n) = value.as_f64() {
        return conv::int(n, what);
    }
    if let Some(text) = value.as_string()
        && let Ok(n) = text.parse::<f64>()
        && text.bytes().all(|b| b.is_ascii_digit())
    {
        return conv::int(n, what);
    }
    Err(invalid(format!("{what}: expected a pixel value 0..255")))
}

fn text_palette(palette: Option<JsValue>, depth: Depth) -> JsResult<Vec<(char, u8)>> {
    match palette {
        Some(p) => pairs(&p, "palette")?
            .into_iter()
            .map(|(k, v)| {
                Ok((
                    char_arg(&k, "palette key")?,
                    value_arg(&v, "palette value")?,
                ))
            })
            .collect(),
        None if depth == Depth::Mono => Ok(vec![('#', 1), ('.', 0), ('1', 1), ('0', 0)]),
        None => Err(invalid("a gray8 text bitmap needs an explicit palette")),
    }
}

fn output_palette(palette: Option<JsValue>, depth: Depth) -> JsResult<Vec<(u8, char)>> {
    match palette.filter(|p| !p.is_undefined() && !p.is_null()) {
        Some(p) => pairs(&p, "palette")?
            .into_iter()
            .map(|(k, v)| {
                Ok((
                    value_arg(&k, "palette key")?,
                    char_arg(&v, "palette value")?,
                ))
            })
            .collect(),
        None if depth == Depth::Mono => Ok(vec![(0, '.'), (1, '#')]),
        None => Err(invalid("a gray8 bitmap needs an explicit output palette")),
    }
}

fn limits_opt(options: &Options) -> JsResult<BitmapLimits> {
    Ok(options.plain("limits")?.unwrap_or_default())
}

fn from_rows<F: PixelFormat>(
    rows: &[Vec<u8>],
    limits: BitmapLimits,
) -> fontpixel::Result<Bitmap<F>> {
    let height = u32::try_from(rows.len())
        .map_err(|_| fontpixel::Error::InvalidInput("too many bitmap rows".into()))?;
    let width = rows.first().map_or(0, Vec::len);
    let width = u32::try_from(width)
        .map_err(|_| fontpixel::Error::InvalidInput("bitmap rows too wide".into()))?;
    let mut out = Bitmap::<F>::with_limits(width, height, limits)?;
    for (y, row) in rows.iter().enumerate() {
        if row.len() != width as usize {
            return Err(fontpixel::Error::InvalidInput(
                "bitmap rows have different lengths".into(),
            ));
        }
        for (x, &v) in row.iter().enumerate() {
            out.set_pixel(x as u32, y as u32, v)?;
        }
    }
    Ok(out)
}

/// Pixel rows of a `Bitmap<F>`, top row first, one byte per pixel.
fn pixel_rows<F: PixelFormat>(b: &Bitmap<F>) -> Vec<Vec<u8>> {
    (0..b.height())
        .map(|y| {
            (0..b.width())
                .map(|x| b.get_pixel(x, y).unwrap_or_default())
                .collect()
        })
        .collect()
}

fn rows_arg(rows: &JsValue) -> JsResult<Vec<Vec<u8>>> {
    let array = rows
        .dyn_ref::<Array>()
        .ok_or_else(|| invalid("rows: expected an array of rows"))?;
    array
        .iter()
        .enumerate()
        .map(|(y, row)| {
            if let Some(bytes) = row.dyn_ref::<Uint8Array>() {
                return Ok(bytes.to_vec());
            }
            conv::from_js::<Vec<u8>>(&row)
                .map_err(|_| invalid(format!("rows[{y}]: expected pixel values 0..255")))
        })
        .collect()
}

fn direction_opt(options: &Options) -> JsResult<Direction> {
    Ok(options.plain("direction")?.unwrap_or(Direction::Right))
}

fn alignment_opt(options: &Options) -> JsResult<Alignment> {
    Ok(options.plain("alignment")?.unwrap_or(Alignment::Start))
}

fn mode_arg(mode: Option<JsValue>) -> JsResult<CompositeMode> {
    match mode {
        Some(m) if !m.is_undefined() && !m.is_null() => conv::from_js(&m),
        _ => Ok(CompositeMode::Over),
    }
}

fn concat_all(
    parts: &[GlyphBitmap],
    direction: Direction,
    alignment: Alignment,
    gaps: &[i64],
    limits: BitmapLimits,
) -> JsResult<GlyphBitmap> {
    let gray = parts.iter().any(|b| matches!(b, GlyphBitmap::Gray8(_)));
    Ok(if gray {
        let typed = parts
            .iter()
            .map(|b| match b {
                GlyphBitmap::Gray8(b) => Ok(b),
                GlyphBitmap::Mono(_) => Err(mismatch()),
            })
            .collect::<JsResult<Vec<_>>>()?;
        Bitmap::concat_all(&typed, direction, alignment, gaps, limits)
            .js()?
            .into()
    } else {
        let typed = parts
            .iter()
            .map(|b| match b {
                GlyphBitmap::Mono(b) => Ok(b),
                GlyphBitmap::Gray8(_) => Err(mismatch()),
            })
            .collect::<JsResult<Vec<_>>>()?;
        Bitmap::concat_all(&typed, direction, alignment, gaps, limits)
            .js()?
            .into()
    })
}

impl JsBitmap {
    /// A new handle from `f` applied to the raster.
    fn derive(
        &self,
        f: impl FnOnce(&GlyphBitmap) -> fontpixel::Result<GlyphBitmap>,
    ) -> JsResult<JsBitmap> {
        self.read(|b| f(b).js().map(JsBitmap::new))
    }
    fn derive_mono(
        &self,
        name: &str,
        f: impl FnOnce(&Bitmap<Mono>) -> fontpixel::Result<Bitmap<Gray8>>,
    ) -> JsResult<JsBitmap> {
        self.read(|b| match b {
            GlyphBitmap::Mono(b) => Ok(JsBitmap::new(f(b).js()?.into())),
            GlyphBitmap::Gray8(_) => Err(only(name, "mono")),
        })
    }
    /// Combine a copy of `source` into this raster (equal depths).
    fn combine(
        &self,
        source: &JsBitmap,
        mono: impl FnOnce(&mut Bitmap<Mono>, &Bitmap<Mono>),
        gray: impl FnOnce(&mut Bitmap<Gray8>, &Bitmap<Gray8>),
    ) -> JsResult<()> {
        let source = source.snapshot()?;
        self.write(|b| match (b, &source) {
            (GlyphBitmap::Mono(a), GlyphBitmap::Mono(s)) => {
                mono(a, s);
                Ok(())
            }
            (GlyphBitmap::Gray8(a), GlyphBitmap::Gray8(s)) => {
                gray(a, s);
                Ok(())
            }
            _ => Err(mismatch()),
        })
    }
}

// ------------------------------------------------------------ the class

#[wasm_bindgen(js_class = Bitmap)]
impl JsBitmap {
    /// A blank `width` × `height` raster (`Bitmap::new`, or
    /// `Bitmap::with_limits` when `limits` is given); `depth` is `"mono"`
    /// (default) or `"gray8"`.
    #[wasm_bindgen(constructor)]
    pub fn constructor(
        width: Option<f64>,
        height: Option<f64>,
        #[wasm_bindgen(unchecked_optional_param_type = "Depth")] depth: Option<String>,
        #[wasm_bindgen(unchecked_optional_param_type = "BitmapLimits")] limits: Option<JsValue>,
    ) -> JsResult<JsBitmap> {
        let width: u32 = opt_int(width, 0, "width")?;
        let height: u32 = opt_int(height, 0, "height")?;
        let limits: Option<BitmapLimits> =
            conv::from_js_opt(&limits.unwrap_or(JsValue::UNDEFINED))?;
        Ok(JsBitmap::new(match (depth_arg(depth)?, limits) {
            (Depth::Mono, None) => Bitmap::<Mono>::new(width, height).js()?.into(),
            (Depth::Gray8, None) => Bitmap::<Gray8>::new(width, height).js()?.into(),
            (Depth::Mono, Some(l)) => Bitmap::<Mono>::with_limits(width, height, l).js()?.into(),
            (Depth::Gray8, Some(l)) => Bitmap::<Gray8>::with_limits(width, height, l).js()?.into(),
        }))
    }
    /// A blank raster under explicit limits (`Bitmap::with_limits`).
    #[wasm_bindgen(js_name = withLimits)]
    pub fn with_limits(
        width: f64,
        height: f64,
        #[wasm_bindgen(unchecked_param_type = "BitmapLimits")] limits: JsValue,
        #[wasm_bindgen(unchecked_optional_param_type = "Depth")] depth: Option<String>,
    ) -> JsResult<JsBitmap> {
        let limits: BitmapLimits = conv::from_js(&limits)?;
        let (width, height) = (conv::int(width, "width")?, conv::int(height, "height")?);
        Ok(JsBitmap::new(match depth_arg(depth)? {
            Depth::Mono => Bitmap::<Mono>::with_limits(width, height, limits)
                .js()?
                .into(),
            Depth::Gray8 => Bitmap::<Gray8>::with_limits(width, height, limits)
                .js()?
                .into(),
        }))
    }
    /// From canonical storage: top-down rows, Mono rows packed MSB-first
    /// and padded to whole bytes, Gray8 one byte per pixel
    /// (`Bitmap::from_raw`). `options`: `depth`, `limits`.
    #[wasm_bindgen(js_name = fromRaw)]
    pub fn from_raw(
        width: f64,
        height: f64,
        #[wasm_bindgen(unchecked_param_type = "BytesLike")] data: JsValue,
        #[wasm_bindgen(unchecked_optional_param_type = "{ depth?: Depth; limits?: BitmapLimits }")]
        options: Option<JsValue>,
    ) -> JsResult<JsBitmap> {
        let (width, height) = (conv::int(width, "width")?, conv::int(height, "height")?);
        let data = conv::bytes_from_js(&data, "data")?;
        let options = Options::new(
            &options.unwrap_or(JsValue::UNDEFINED),
            &["depth", "limits"],
            "fromRaw options",
        )?;
        let limits = limits_opt(&options)?;
        Ok(JsBitmap::new(match depth_arg(options.string("depth")?)? {
            Depth::Mono => Bitmap::<Mono>::from_raw(width, height, &data, limits)
                .js()?
                .into(),
            Depth::Gray8 => Bitmap::<Gray8>::from_raw(width, height, &data, limits)
                .js()?
                .into(),
        }))
    }
    /// From an exact-sized byte stream in `options.layout` (a `ByteLayout`:
    /// `bit_order` `"msb"`/`"lsb"`, `byte_order` `"big"`/`"little"`,
    /// `row_alignment`, `scan_unit`, `row_order` `"top-down"`/`"bottom-up"`;
    /// default MSB-first byte rows, top-down) with `options.depth` and
    /// `options.limits` (`Bitmap::from_bytes`).
    #[wasm_bindgen(js_name = fromBytes)]
    pub fn from_bytes(
        width: f64,
        height: f64,
        #[wasm_bindgen(unchecked_param_type = "BytesLike")] data: JsValue,
        #[wasm_bindgen(
            unchecked_optional_param_type = "{ layout?: ByteLayout; depth?: Depth; limits?: BitmapLimits }"
        )]
        options: Option<JsValue>,
    ) -> JsResult<JsBitmap> {
        let width: u32 = conv::int(width, "width")?;
        let height: u32 = conv::int(height, "height")?;
        let data = conv::bytes_from_js(&data, "data")?;
        let options = Options::new(
            &options.unwrap_or(JsValue::UNDEFINED),
            &["layout", "depth", "limits"],
            "fromBytes options",
        )?;
        let layout: ByteLayout = options.plain("layout")?.unwrap_or_default();
        let limits = limits_opt(&options)?;
        Ok(JsBitmap::new(match depth_arg(options.string("depth")?)? {
            Depth::Mono => Bitmap::<Mono>::from_bytes(width, height, &data, layout, limits)
                .js()?
                .into(),
            Depth::Gray8 => Bitmap::<Gray8>::from_bytes(width, height, &data, layout, limits)
                .js()?
                .into(),
        }))
    }
    /// Parse text rows through `options.palette` (character → value, as an
    /// object, a `Map` or `[char, value]` pairs); the Mono default accepts
    /// `#`/`1` as ink and `.`/`0` as paper. Blank leading and trailing lines
    /// are ignored (`Bitmap::from_text`). `options`: `palette`, `depth`,
    /// `limits`.
    #[wasm_bindgen(js_name = fromText)]
    pub fn from_text(
        text: &str,
        #[wasm_bindgen(
            unchecked_optional_param_type = "{ palette?: TextPalette; depth?: Depth; limits?: BitmapLimits }"
        )]
        options: Option<JsValue>,
    ) -> JsResult<JsBitmap> {
        let options = Options::new(
            &options.unwrap_or(JsValue::UNDEFINED),
            &["palette", "depth", "limits"],
            "fromText options",
        )?;
        let depth = depth_arg(options.string("depth")?)?;
        let palette = text_palette(options.get("palette")?, depth)?;
        let limits = limits_opt(&options)?;
        Ok(JsBitmap::new(match depth {
            Depth::Mono => Bitmap::<Mono>::from_text(text, &palette, limits)
                .js()?
                .into(),
            Depth::Gray8 => Bitmap::<Gray8>::from_text(text, &palette, limits)
                .js()?
                .into(),
        }))
    }
    /// Parse strict BDF-style byte-padded hex rows into a Mono bitmap
    /// (`Bitmap::<Mono>::from_hex_rows`). `options`: `limits`.
    #[wasm_bindgen(js_name = fromHexRows)]
    pub fn from_hex_rows(
        width: f64,
        #[wasm_bindgen(unchecked_param_type = "string[]")] rows: JsValue,
        #[wasm_bindgen(unchecked_optional_param_type = "{ limits?: BitmapLimits }")]
        options: Option<JsValue>,
    ) -> JsResult<JsBitmap> {
        let width = conv::int(width, "width")?;
        let rows: Vec<String> = conv::from_js(&rows)?;
        let options = Options::new(
            &options.unwrap_or(JsValue::UNDEFINED),
            &["limits"],
            "fromHexRows options",
        )?;
        let rows: Vec<&str> = rows.iter().map(String::as_str).collect();
        Ok(JsBitmap::new(
            Bitmap::<Mono>::from_hex_rows(width, &rows, limits_opt(&options)?)
                .js()?
                .into(),
        ))
    }
    /// From rows of pixel values (arrays of numbers or `Uint8Array`s), top
    /// row first; the inverse of `toRows`. `options`: `depth`, `limits`.
    #[wasm_bindgen(js_name = fromRows)]
    pub fn from_rows(
        #[wasm_bindgen(unchecked_param_type = "ArrayLike<number>[]")] rows: JsValue,
        #[wasm_bindgen(unchecked_optional_param_type = "{ depth?: Depth; limits?: BitmapLimits }")]
        options: Option<JsValue>,
    ) -> JsResult<JsBitmap> {
        let rows = rows_arg(&rows)?;
        let options = Options::new(
            &options.unwrap_or(JsValue::UNDEFINED),
            &["depth", "limits"],
            "fromRows options",
        )?;
        let limits = limits_opt(&options)?;
        Ok(JsBitmap::new(match depth_arg(options.string("depth")?)? {
            Depth::Mono => from_rows::<Mono>(&rows, limits).js()?.into(),
            Depth::Gray8 => from_rows::<Gray8>(&rows, limits).js()?.into(),
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
    /// Concatenate copies of several bitmaps in one allocation
    /// (`Bitmap::concat_all`). `options`: `direction` (default `"right"`),
    /// `alignment` (default `"start"`), `gaps` (empty, or one per boundary;
    /// numbers or bigints), `limits`.
    #[wasm_bindgen(js_name = concatAll)]
    pub fn concat_all(
        #[wasm_bindgen(unchecked_param_type = "Bitmap[]")] bitmaps: JsValue,
        #[wasm_bindgen(
            unchecked_optional_param_type = "{ direction?: Direction; alignment?: Alignment; gaps?: WideInt[]; limits?: BitmapLimits }"
        )]
        options: Option<JsValue>,
    ) -> JsResult<JsBitmap> {
        let options = Options::new(
            &options.unwrap_or(JsValue::UNDEFINED),
            &["direction", "alignment", "gaps", "limits"],
            "concatAll options",
        )?;
        let parts = bitmap_list(&bitmaps, "concatAll bitmaps")?;
        let gaps: Vec<i64> = options.plain("gaps")?.unwrap_or_default();
        let out = concat_all(
            &parts,
            direction_opt(&options)?,
            alignment_opt(&options)?,
            &gaps,
            limits_opt(&options)?,
        )?;
        Ok(JsBitmap::new(out))
    }

    /// `"mono"` or `"gray8"`.
    #[wasm_bindgen(getter, unchecked_return_type = "Depth")]
    pub fn depth(&self) -> JsResult<String> {
        self.read(|b| Ok(depth_name(b).to_owned()))
    }
    /// Width in pixels.
    #[wasm_bindgen(getter)]
    pub fn width(&self) -> JsResult<u32> {
        self.read(|b| Ok(b.width()))
    }
    /// Height in pixels.
    #[wasm_bindgen(getter)]
    pub fn height(&self) -> JsResult<u32> {
        self.read(|b| Ok(b.height()))
    }
    /// Bytes per canonical row (`Bitmap::stride`).
    #[wasm_bindgen(getter)]
    pub fn stride(&self) -> JsResult<usize> {
        self.read(|b| Ok(each!(b, |b| b.stride())))
    }
    /// Largest pixel value: 1 for Mono, 255 for Gray8 (`PixelFormat::MAX`).
    #[wasm_bindgen(getter, js_name = maxValue)]
    pub fn max_value(&self) -> JsResult<u8> {
        self.read(|b| {
            Ok(match b {
                GlyphBitmap::Mono(_) => Mono::MAX,
                GlyphBitmap::Gray8(_) => Gray8::MAX,
            })
        })
    }
    /// Whether rows are bit-packed (`PixelFormat::PACKED`).
    #[wasm_bindgen(getter)]
    pub fn packed(&self) -> JsResult<bool> {
        self.read(|b| {
            Ok(match b {
                GlyphBitmap::Mono(_) => Mono::PACKED,
                GlyphBitmap::Gray8(_) => Gray8::PACKED,
            })
        })
    }
    /// The bitmap's resource limits (`Bitmap::limits`), inherited by every
    /// derived bitmap.
    #[wasm_bindgen(getter, unchecked_return_type = "BitmapLimits")]
    pub fn limits(&self) -> JsResult<JsValue> {
        self.read(|b| conv::to_js(&each!(b, |b| b.limits())))
    }
    /// Replace the limits after checking the current size against them
    /// (`Bitmap::set_limits`).
    #[wasm_bindgen(js_name = setLimits)]
    pub fn set_limits(
        &self,
        #[wasm_bindgen(unchecked_param_type = "BitmapLimits")] limits: JsValue,
    ) -> JsResult<()> {
        let limits: BitmapLimits = conv::from_js(&limits)?;
        self.write(|b| each!(b, |b| b.set_limits(limits)).js())
    }
    /// Pixel value at `(x, y)` (`Bitmap::get_pixel`).
    #[wasm_bindgen(js_name = getPixel)]
    pub fn get_pixel(&self, x: f64, y: f64) -> JsResult<u8> {
        let (x, y) = (conv::int(x, "x")?, conv::int(y, "y")?);
        self.read(|b| each!(b, |b| b.get_pixel(x, y)).js())
    }
    /// Set the pixel at `(x, y)`; the value must fit the depth
    /// (`Bitmap::set_pixel`).
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
        self.read(|b| Ok(conv::bytes_to_js(b.as_bytes())))
    }
    /// Serialize in an explicit `ByteLayout` (default: MSB-first, byte rows,
    /// top-down) (`Bitmap::to_bytes`).
    #[wasm_bindgen(js_name = toBytes)]
    pub fn to_bytes(
        &self,
        #[wasm_bindgen(unchecked_optional_param_type = "ByteLayout")] layout: Option<JsValue>,
    ) -> JsResult<Uint8Array> {
        let layout: ByteLayout = conv::from_js_or_default(&layout.unwrap_or(JsValue::UNDEFINED))?;
        self.read(|b| {
            let bytes = each!(b, |b| b.to_bytes(layout)).js()?;
            Ok(conv::bytes_to_js(&bytes))
        })
    }
    /// Rows of pixel values, top row first: one fresh `Uint8Array` per row.
    #[wasm_bindgen(js_name = toRows, unchecked_return_type = "Uint8Array[]")]
    pub fn to_rows(&self) -> JsResult<Array> {
        self.read(|b| {
            Ok(each!(b, |b| pixel_rows(b))
                .iter()
                .map(|row| JsValue::from(conv::bytes_to_js(row)))
                .collect())
        })
    }
    /// Every pixel value in row-major order (one byte per pixel).
    #[wasm_bindgen(js_name = toList)]
    pub fn to_list(&self) -> JsResult<Uint8Array> {
        self.read(|b| Ok(conv::bytes_to_js(&each!(b, |b| pixel_rows(b)).concat())))
    }
    /// Text rows through `palette` (value → character, as an object, a `Map`
    /// or `[value, char]` pairs; Mono default `.`/`#`) (`Bitmap::to_text`).
    #[wasm_bindgen(js_name = toText)]
    pub fn to_text(
        &self,
        #[wasm_bindgen(unchecked_optional_param_type = "OutputPalette")] palette: Option<JsValue>,
    ) -> JsResult<String> {
        let depth = self.read(|b| Ok(depth_of(b)))?;
        let palette = output_palette(palette, depth)?;
        self.read(|b| each!(b, |b| b.to_text(&palette)).js())
    }
    /// One fixed-size byte string per pixel, from `palette` (value → bytes,
    /// as an object, a `Map` or `[value, bytes]` pairs; bytes are
    /// `BytesLike` or number arrays) (`Bitmap::to_palette_bytes`).
    #[wasm_bindgen(js_name = toPaletteBytes)]
    pub fn to_palette_bytes(
        &self,
        #[wasm_bindgen(unchecked_param_type = "BytesPalette")] palette: JsValue,
    ) -> JsResult<Uint8Array> {
        let entries = pairs(&palette, "palette")?
            .into_iter()
            .map(|(k, v)| {
                let bytes = if v.is_object() && !Array::is_array(&v) {
                    conv::bytes_from_js(&v, "palette value")?
                } else {
                    conv::from_js::<Vec<u8>>(&v)?
                };
                Ok((value_arg(&k, "palette key")?, bytes))
            })
            .collect::<JsResult<Vec<_>>>()?;
        let borrowed: Vec<(u8, &[u8])> = entries.iter().map(|(k, v)| (*k, v.as_slice())).collect();
        self.read(|b| {
            let bytes = each!(b, |b| b.to_palette_bytes(&borrowed)).js()?;
            Ok(conv::bytes_to_js(&bytes))
        })
    }
    /// Byte-padded uppercase hex rows (BDF storage)
    /// (`Bitmap::<Mono>::to_hex_rows`).
    #[wasm_bindgen(js_name = toHexRows)]
    pub fn to_hex_rows(&self) -> JsResult<Vec<String>> {
        self.read(|b| match b {
            GlyphBitmap::Mono(b) => b.to_hex_rows().js(),
            GlyphBitmap::Gray8(_) => Err(only("toHexRows", "mono")),
        })
    }
    /// Right-aligned lowercase hex rows (`Bitmap::<Mono>::to_numeric_hex_rows`).
    #[wasm_bindgen(js_name = toNumericHexRows)]
    pub fn to_numeric_hex_rows(&self) -> JsResult<Vec<String>> {
        self.read(|b| match b {
            GlyphBitmap::Mono(b) => b.to_numeric_hex_rows().js(),
            GlyphBitmap::Gray8(_) => Err(only("toNumericHexRows", "mono")),
        })
    }
    /// Rows as integers, leftmost pixel most significant; width ≤ 64
    /// (`Bitmap::<Mono>::to_row_values`). Each value is a `number` up to
    /// 2^53 − 1, else a `bigint`.
    #[wasm_bindgen(js_name = toRowValues, unchecked_return_type = "WideInt[]")]
    pub fn to_row_values(&self) -> JsResult<Array> {
        self.read(|b| match b {
            GlyphBitmap::Mono(b) => Ok(b
                .to_row_values()
                .js()?
                .into_iter()
                .map(|v| conv::int_to_js(i128::from(v)))
                .collect()),
            GlyphBitmap::Gray8(_) => Err(only("toRowValues", "mono")),
        })
    }

    /// Crop or zero-extend to `width`×`height` at bottom-left offset
    /// `(x, y)` (`Bitmap::crop`).
    pub fn crop(
        &self,
        width: f64,
        height: f64,
        #[wasm_bindgen(unchecked_optional_param_type = "WideInt")] x: Option<JsValue>,
        #[wasm_bindgen(unchecked_optional_param_type = "WideInt")] y: Option<JsValue>,
    ) -> JsResult<JsBitmap> {
        let (width, height) = (conv::int(width, "width")?, conv::int(height, "height")?);
        let (x, y) = (i64_arg(x, 0, "x")?, i64_arg(y, 0, "y")?);
        self.derive(|b| each!(b, |b| b.crop(width, height, x, y).map(GlyphBitmap::from)))
    }
    /// Crop to a `Bounds` (`{width, height, x, y}`, bottom-left offsets)
    /// (`Bitmap::crop_bounds`, also `GlyphBitmap::crop`).
    #[wasm_bindgen(js_name = cropBounds)]
    pub fn crop_bounds(
        &self,
        #[wasm_bindgen(unchecked_param_type = "Bounds")] bounds: JsValue,
    ) -> JsResult<JsBitmap> {
        let bounds: Bounds = conv::from_js(&bounds)?;
        self.derive(|b| b.crop(bounds))
    }
    /// Add blank margins (`Bitmap::extend`).
    pub fn extend(
        &self,
        left: Option<f64>,
        right: Option<f64>,
        top: Option<f64>,
        bottom: Option<f64>,
    ) -> JsResult<JsBitmap> {
        let left = opt_int(left, 0, "left")?;
        let right = opt_int(right, 0, "right")?;
        let top = opt_int(top, 0, "top")?;
        let bottom = opt_int(bottom, 0, "bottom")?;
        self.derive(|b| {
            each!(b, |b| b
                .extend(left, right, top, bottom)
                .map(GlyphBitmap::from))
        })
    }
    /// Move pixels right/up inside the same size, filling with 0
    /// (`Bitmap::shift`).
    pub fn shift(
        &self,
        #[wasm_bindgen(unchecked_param_type = "WideInt")] dx: JsValue,
        #[wasm_bindgen(unchecked_param_type = "WideInt")] dy: JsValue,
    ) -> JsResult<JsBitmap> {
        let (dx, dy) = (conv::int_from_js(&dx, "dx")?, conv::int_from_js(&dy, "dy")?);
        self.derive(|b| each!(b, |b| b.shift(dx, dy).map(GlyphBitmap::from)))
    }
    /// Tight ink rectangle in bottom-left coordinates (`Bitmap::bbox`).
    #[wasm_bindgen(unchecked_return_type = "Bounds")]
    pub fn bbox(&self) -> JsResult<JsValue> {
        self.read(|b| conv::to_js(&b.bbox()))
    }
    /// Crop to the ink (`Bitmap::trim`).
    pub fn trim(&self) -> JsResult<JsBitmap> {
        self.derive(|b| each!(b, |b| b.trim().map(GlyphBitmap::from)))
    }
    /// Whether any pixel is nonzero (`Bitmap::has_ink`).
    #[wasm_bindgen(js_name = hasInk)]
    pub fn has_ink(&self) -> JsResult<bool> {
        self.read(|b| Ok(each!(b, |b| b.has_ink())))
    }
    /// Mirror left/right (`Bitmap::flip_horizontal`).
    #[wasm_bindgen(js_name = flipHorizontal)]
    pub fn flip_horizontal(&self) -> JsResult<JsBitmap> {
        self.derive(|b| each!(b, |b| b.flip_horizontal().map(GlyphBitmap::from)))
    }
    /// Mirror top/bottom (`Bitmap::flip_vertical`).
    #[wasm_bindgen(js_name = flipVertical)]
    pub fn flip_vertical(&self) -> JsResult<JsBitmap> {
        self.derive(|b| each!(b, |b| b.flip_vertical().map(GlyphBitmap::from)))
    }
    /// Rotate clockwise by a multiple of 90 degrees (`Bitmap::rotate`).
    pub fn rotate(&self, degrees: f64) -> JsResult<JsBitmap> {
        let degrees: i32 = conv::int(degrees, "degrees")?;
        self.derive(|b| each!(b, |b| b.rotate(degrees).map(GlyphBitmap::from)))
    }
    /// Scale by integer factors (`Bitmap::enlarge`; `y` defaults to `x`).
    pub fn enlarge(&self, x: f64, y: Option<f64>) -> JsResult<JsBitmap> {
        let x: u32 = conv::int(x, "x")?;
        let y = opt_int(y, x, "y")?;
        self.derive(|b| each!(b, |b| b.enlarge(x, y).map(GlyphBitmap::from)))
    }
    /// Pad the width up to a multiple of `pixels` (`Bitmap::byte_pad`).
    #[wasm_bindgen(js_name = bytePad)]
    pub fn byte_pad(&self, pixels: f64) -> JsResult<JsBitmap> {
        let pixels = conv::int(pixels, "pixels")?;
        self.derive(|b| each!(b, |b| b.byte_pad(pixels).map(GlyphBitmap::from)))
    }
    /// Concatenate with a copy of `other` (`Bitmap::concat`); the later
    /// operand wins where both have ink. `options`: `direction` (default
    /// `"right"`), `alignment` (default `"start"`), `gap` (number or bigint).
    pub fn concat(
        &self,
        other: &JsBitmap,
        #[wasm_bindgen(
            unchecked_optional_param_type = "{ direction?: Direction; alignment?: Alignment; gap?: WideInt }"
        )]
        options: Option<JsValue>,
    ) -> JsResult<JsBitmap> {
        let options = Options::new(
            &options.unwrap_or(JsValue::UNDEFINED),
            &["direction", "alignment", "gap"],
            "concat options",
        )?;
        let direction = direction_opt(&options)?;
        let alignment = alignment_opt(&options)?;
        let gap = i64_arg(options.get("gap")?, 0, "gap")?;
        let other = other.snapshot()?;
        self.read(|b| {
            let out: GlyphBitmap = match (b, &other) {
                (GlyphBitmap::Mono(a), GlyphBitmap::Mono(o)) => {
                    a.concat(o, direction, alignment, gap).js()?.into()
                }
                (GlyphBitmap::Gray8(a), GlyphBitmap::Gray8(o)) => {
                    a.concat(o, direction, alignment, gap).js()?.into()
                }
                _ => return Err(mismatch()),
            };
            Ok(JsBitmap::new(out))
        })
    }
    /// Combine a copy of `source` into this bitmap in place at `(x, y)`
    /// (top-left offsets) with `mode` (default `"over"`) (`Bitmap::composite`).
    pub fn composite(
        &self,
        source: &JsBitmap,
        #[wasm_bindgen(unchecked_optional_param_type = "WideInt")] x: Option<JsValue>,
        #[wasm_bindgen(unchecked_optional_param_type = "WideInt")] y: Option<JsValue>,
        #[wasm_bindgen(unchecked_optional_param_type = "CompositeMode")] mode: Option<JsValue>,
    ) -> JsResult<()> {
        let (x, y) = (i64_arg(x, 0, "x")?, i64_arg(y, 0, "y")?);
        let mode = mode_arg(mode)?;
        self.combine(
            source,
            |a, s| a.composite(s, x, y, mode),
            |a, s| a.composite(s, x, y, mode),
        )
    }
    /// Combine `region` (a `Bounds` with top-left offsets into `source`) of
    /// `source` in place at `(x, y)` (`Bitmap::composite_region`).
    #[wasm_bindgen(js_name = compositeRegion)]
    pub fn composite_region(
        &self,
        source: &JsBitmap,
        #[wasm_bindgen(unchecked_param_type = "Bounds")] region: JsValue,
        #[wasm_bindgen(unchecked_optional_param_type = "WideInt")] x: Option<JsValue>,
        #[wasm_bindgen(unchecked_optional_param_type = "WideInt")] y: Option<JsValue>,
        #[wasm_bindgen(unchecked_optional_param_type = "CompositeMode")] mode: Option<JsValue>,
    ) -> JsResult<()> {
        let region: Bounds = conv::from_js(&region)?;
        let (x, y) = (i64_arg(x, 0, "x")?, i64_arg(y, 0, "y")?);
        let mode = mode_arg(mode)?;
        self.combine(
            source,
            |a, s| a.composite_region(s, region, x, y, mode),
            |a, s| a.composite_region(s, region, x, y, mode),
        )
    }
    /// `composite(source, x, y, "over")` (`Bitmap::paste`).
    pub fn paste(
        &self,
        source: &JsBitmap,
        #[wasm_bindgen(unchecked_optional_param_type = "WideInt")] x: Option<JsValue>,
        #[wasm_bindgen(unchecked_optional_param_type = "WideInt")] y: Option<JsValue>,
    ) -> JsResult<()> {
        let (x, y) = (i64_arg(x, 0, "x")?, i64_arg(y, 0, "y")?);
        self.combine(source, |a, s| a.paste(s, x, y), |a, s| a.paste(s, x, y))
    }
    /// `paste(source, 0, 0)` (`Bitmap::overlay`).
    pub fn overlay(&self, source: &JsBitmap) -> JsResult<()> {
        self.combine(source, |a, s| a.overlay(s), |a, s| a.overlay(s))
    }
    /// Binary inversion in place: zero becomes full ink, nonzero zero
    /// (`Bitmap::invert`).
    pub fn invert(&self) -> JsResult<()> {
        self.write(|b| {
            each!(b, |b| b.invert());
            Ok(())
        })
    }
    /// Binary inversion in place with ink value `ink` (`Bitmap::invert_with`).
    #[wasm_bindgen(js_name = invertWith)]
    pub fn invert_with(&self, ink: f64) -> JsResult<()> {
        let ink: u8 = conv::int(ink, "ink")?;
        self.write(|b| each!(b, |b| b.invert_with(ink)).js())
    }
    /// Replace every pixel equal to `old` with `new`, in place
    /// (`Bitmap::replace`).
    pub fn replace(&self, old: f64, new: f64) -> JsResult<()> {
        let (old, new): (u8, u8) = (conv::int(old, "old")?, conv::int(new, "new")?);
        self.write(|b| each!(b, |b| b.replace(old, new)).js())
    }
    /// Row-wise non-overlapping pattern replacement; patterns are pixel
    /// values (`BytesLike` or number arrays) (`Bitmap::replace_pattern`).
    #[wasm_bindgen(js_name = replacePattern)]
    pub fn replace_pattern(
        &self,
        #[wasm_bindgen(unchecked_param_type = "BytesLike | number[]")] old: JsValue,
        #[wasm_bindgen(unchecked_param_type = "BytesLike | number[]")] new: JsValue,
    ) -> JsResult<JsBitmap> {
        let pattern = |v: &JsValue, what| -> JsResult<Vec<u8>> {
            if Array::is_array(v) {
                conv::from_js(v)
            } else {
                conv::bytes_from_js(v, what)
            }
        };
        let (old, new) = (pattern(&old, "old")?, pattern(&new, "new")?);
        self.derive(|b| each!(b, |b| b.replace_pattern(&old, &new).map(GlyphBitmap::from)))
    }
    /// Displaced silhouette behind the ink (`Bitmap::shadow`).
    pub fn shadow(
        &self,
        #[wasm_bindgen(unchecked_param_type = "WideInt")] dx: JsValue,
        #[wasm_bindgen(unchecked_param_type = "WideInt")] dy: JsValue,
        value: f64,
    ) -> JsResult<JsBitmap> {
        let (dx, dy) = (conv::int_from_js(&dx, "dx")?, conv::int_from_js(&dy, "dy")?);
        let value: u8 = conv::int(value, "value")?;
        self.derive(|b| each!(b, |b| b.shadow(dx, dy, value).map(GlyphBitmap::from)))
    }
    /// One-pixel glow around pixels equal to `ink` (`Bitmap::glow`).
    pub fn glow(&self, diagonals: bool, ink: f64, value: f64) -> JsResult<JsBitmap> {
        let (ink, value): (u8, u8) = (conv::int(ink, "ink")?, conv::int(value, "value")?);
        self.derive(|b| each!(b, |b| b.glow(diagonals, ink, value).map(GlyphBitmap::from)))
    }
    /// Rightward emboldening by `n` pixels (default 1) (`Bitmap::bold`).
    pub fn bold(&self, n: Option<f64>) -> JsResult<JsBitmap> {
        let n = opt_int(n, 1, "n")?;
        self.derive(|b| each!(b, |b| b.bold(n).map(GlyphBitmap::from)))
    }
    /// Slant by `floor((row_from_bottom - base) / step)` (`Bitmap::italic`).
    pub fn italic(
        &self,
        #[wasm_bindgen(unchecked_param_type = "WideInt")] step: JsValue,
        #[wasm_bindgen(unchecked_optional_param_type = "WideInt")] base: Option<JsValue>,
    ) -> JsResult<JsBitmap> {
        let step = conv::int_from_js(&step, "step")?;
        let base = i64_arg(base, 0, "base")?;
        self.derive(|b| each!(b, |b| b.italic(step, base).map(GlyphBitmap::from)))
    }
    /// Continuous-tone inversion `255 - v` in place
    /// (`Bitmap::<Gray8>::negate`).
    pub fn negate(&self) -> JsResult<()> {
        self.write(|b| match b {
            GlyphBitmap::Gray8(b) => {
                b.negate();
                Ok(())
            }
            GlyphBitmap::Mono(_) => Err(only("negate", "gray8")),
        })
    }
    /// Gray8 copy with paper 0 and ink 255 (`Bitmap::<Mono>::to_gray8`).
    #[wasm_bindgen(js_name = toGray8)]
    pub fn to_gray8(&self) -> JsResult<JsBitmap> {
        self.derive_mono("toGray8", |b| b.to_gray8())
    }
    /// Gray8 copy with explicit paper and ink values
    /// (`Bitmap::<Mono>::to_gray8_palette`).
    #[wasm_bindgen(js_name = toGray8Palette)]
    pub fn to_gray8_palette(&self, paper: f64, ink: f64) -> JsResult<JsBitmap> {
        let (paper, ink): (u8, u8) = (conv::int(paper, "paper")?, conv::int(ink, "ink")?);
        self.derive_mono("toGray8Palette", |b| b.to_gray8_palette(paper, ink))
    }
    /// Mono copy: ink where the value is `>= threshold`
    /// (`Bitmap::<Gray8>::to_mono`).
    #[wasm_bindgen(js_name = toMono)]
    pub fn to_mono(&self, threshold: f64) -> JsResult<JsBitmap> {
        let threshold: u8 = conv::int(threshold, "threshold")?;
        self.read(|b| match b {
            GlyphBitmap::Gray8(b) => Ok(JsBitmap::new(b.to_mono(threshold).js()?.into())),
            GlyphBitmap::Mono(_) => Err(only("toMono", "gray8")),
        })
    }

    /// Whether `other` has the same depth, size and pixels (limits are not
    /// compared).
    pub fn equals(&self, other: &JsBitmap) -> JsResult<bool> {
        let other = other.snapshot()?;
        self.read(|b| Ok(*b == other))
    }
    /// An independent copy (limits included).
    pub fn copy(&self) -> JsResult<JsBitmap> {
        Ok(JsBitmap::new(self.snapshot()?))
    }
    /// Mono: rows of `.`/`#` (Rust `Display`); Gray8:
    /// `Bitmap(width=…, height=…, depth='gray8')`.
    #[wasm_bindgen(js_name = toString)]
    pub fn to_string_js(&self) -> JsResult<String> {
        self.read(|b| {
            Ok(match b {
                GlyphBitmap::Mono(b) => b.to_string(),
                GlyphBitmap::Gray8(_) => format!(
                    "Bitmap(width={}, height={}, depth='gray8')",
                    b.width(),
                    b.height()
                ),
            })
        })
    }
}

/// The default `BitmapLimits` (plain data).
#[wasm_bindgen(js_name = defaultBitmapLimits, unchecked_return_type = "BitmapLimits")]
pub fn default_bitmap_limits() -> JsResult<JsValue> {
    conv::to_js(&BitmapLimits::default())
}

/// The default `ByteLayout` (plain data).
#[wasm_bindgen(js_name = defaultByteLayout, unchecked_return_type = "ByteLayout")]
pub fn default_byte_layout() -> JsResult<JsValue> {
    conv::to_js(&ByteLayout::default())
}
