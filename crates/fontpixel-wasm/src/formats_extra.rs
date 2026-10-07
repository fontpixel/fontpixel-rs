//! Format functions outside the uniform detect/read/write shape:
//! `formats.embedded`, `formats.vectorRaster`, `formats.x11`, the extra
//! helpers of the `bdf`, `fbit`, `fnt`, `hanfont`, `hzk`, `nfnt`, `raw` and
//! `svg` namespaces, and the format catalogue (`formats.catalogue()`,
//! `formats.modules()`).
use crate::conv;
use crate::errors::JsResult;
use crate::formats_direct::GENERATED;
use wasm_bindgen::prelude::*;

/// The format catalogue: every format ID of this library with its name,
/// extensions and whether this build compiled it
/// (`formats::catalogue::CATALOGUE`).
#[wasm_bindgen(js_namespace = formats, js_name = catalogue, unchecked_return_type = "FormatInfo[]")]
pub fn catalogue() -> JsResult<JsValue> {
    let all: Vec<_> = fontpixel::formats::catalogue::CATALOGUE.to_vec();
    conv::to_js(&all)
}

/// JS names of every `formats.<module>` namespace in this build, sorted.
#[wasm_bindgen(js_namespace = formats, js_name = modules)]
pub fn modules() -> Vec<String> {
    let mut names: Vec<&str> = GENERATED.to_vec();
    names.extend(SPECIAL);
    names.sort_unstable();
    names.into_iter().map(str::to_owned).collect()
}

/// The IDs of `ids` this build compiles (catalogue `feature_enabled`).
pub fn compiled(ids: &[&str]) -> Vec<String> {
    ids.iter()
        .filter(|id| {
            fontpixel::formats::catalogue::CATALOGUE
                .iter()
                .any(|f| f.id == **id && f.feature_enabled)
        })
        .map(|id| (*id).to_owned())
        .collect()
}

const SPECIAL: &[&str] = &[
    #[cfg(any(feature = "all-formats", feature = "merge", feature = "web"))]
    "embedded",
    #[cfg(any(feature = "all-formats", feature = "merge", feature = "web"))]
    "vectorRaster",
    #[cfg(any(
        feature = "all-formats",
        feature = "minimal",
        feature = "merge",
        feature = "web"
    ))]
    "x11",
];

// --- embedded (otb / bdat / color) -----------------------------------------

/// `formats.embedded`: embedded bitmap sfnt codecs (otb, bdat, color).
#[cfg(any(feature = "all-formats", feature = "merge", feature = "web"))]
pub mod embedded {
    use crate::conv;
    use crate::errors::{IntoJs, JsResult};
    use crate::formats::fonts_arg;
    use crate::formats_direct::{bytes_report, detect_codec, model_report, options_of, typed};
    use fontpixel::formats::embedded as e;
    use fontpixel::sfnt::bitmap::Kind;
    use wasm_bindgen::prelude::*;

    fn kind_arg(kind: &JsValue) -> JsResult<Kind> {
        conv::from_js(kind)
    }

    /// Decode a standalone sfnt into bitmap strikes (`embedded::read`).
    /// `kind` is `"open_type"` (EBDT/EBLC), `"apple"` (bdat/bloc) or
    /// `"color"` (CBDT/sbix); `options.options` is an `EmbeddedReadOptions`
    /// object. Returns `{model: FontSet, report}`.
    #[wasm_bindgen(js_namespace = ["formats", "embedded"], js_name = read, unchecked_return_type = "ModelReport<FontSet>")]
    pub fn read(
        #[wasm_bindgen(unchecked_param_type = "BytesLike")] data: JsValue,
        #[wasm_bindgen(unchecked_param_type = "BitmapKind")] kind: JsValue,
        #[wasm_bindgen(
            unchecked_optional_param_type = "{ options?: SerdeTypes['formats::embedded::EmbeddedReadOptions'] }"
        )]
        options: Option<JsValue>,
    ) -> JsResult<JsValue> {
        let kind = kind_arg(&kind)?;
        let options = options_of(options, &["options"], "read options")?;
        let o: e::EmbeddedReadOptions = typed(&options)?;
        let bytes = conv::bytes_from_js(&data, "data")?;
        model_report(e::read(&bytes, kind, &o).js()?)
    }

    /// Write strikes as a bitmap-only sfnt (`embedded::write`).
    /// `options.options` is an `EmbeddedWriteOptions` object. Returns
    /// `{data, report}`.
    #[wasm_bindgen(js_namespace = ["formats", "embedded"], js_name = write, unchecked_return_type = "BytesReport")]
    pub fn write(
        #[wasm_bindgen(unchecked_param_type = "FontSet | Font")] fonts: JsValue,
        #[wasm_bindgen(unchecked_param_type = "BitmapKind")] kind: JsValue,
        #[wasm_bindgen(
            unchecked_optional_param_type = "{ options?: SerdeTypes['formats::embedded::EmbeddedWriteOptions'] }"
        )]
        options: Option<JsValue>,
    ) -> JsResult<JsValue> {
        let kind = kind_arg(&kind)?;
        let options = options_of(options, &["options"], "write options")?;
        let o: e::EmbeddedWriteOptions = typed(&options)?;
        let fonts = fonts_arg(&fonts)?;
        bytes_report(fonts.with_set(|s| Ok(e::write(s, kind, &o)))?.js()?)
    }

    /// Write coverage as CBDT/CBLC or sbix color bitmaps
    /// (`embedded::write_color`). `options.options` is a `ColorWriteOptions`
    /// object. Returns `{data, report}`.
    #[cfg(feature = "all-formats")]
    #[wasm_bindgen(js_namespace = ["formats", "embedded"], js_name = writeColor, unchecked_return_type = "BytesReport")]
    pub fn write_color(
        #[wasm_bindgen(unchecked_param_type = "FontSet | Font")] fonts: JsValue,
        #[wasm_bindgen(
            unchecked_optional_param_type = "{ options?: SerdeTypes['formats::embedded::ColorWriteOptions'] }"
        )]
        options: Option<JsValue>,
    ) -> JsResult<JsValue> {
        let options = options_of(options, &["options"], "write options")?;
        let o: e::ColorWriteOptions = typed(&options)?;
        let fonts = fonts_arg(&fonts)?;
        bytes_report(fonts.with_set(|s| Ok(e::write_color(s, &o)))?.js()?)
    }

    /// Run the embedded-bitmap detector of `kind` (`EmbeddedCodec::new(kind)`).
    #[wasm_bindgen(js_namespace = ["formats", "embedded"], js_name = detect, unchecked_return_type = "Confidence")]
    pub fn detect(
        #[wasm_bindgen(unchecked_param_type = "BytesLike")] data: JsValue,
        #[wasm_bindgen(unchecked_param_type = "BitmapKind")] kind: JsValue,
        #[wasm_bindgen(
            unchecked_optional_param_type = "{ name?: string; limits?: ResourceLimits }"
        )]
        options: Option<JsValue>,
    ) -> JsResult<JsValue> {
        let codec = e::EmbeddedCodec::new(kind_arg(&kind)?);
        detect_codec(&codec, &data, options)
    }

    /// The default codec of `kind` (`EmbeddedCodec::new(kind)`).
    #[wasm_bindgen(js_namespace = ["formats", "embedded"], js_name = codec, unchecked_return_type = "EmbeddedCodec")]
    pub fn codec(
        #[wasm_bindgen(unchecked_param_type = "BitmapKind")] kind: JsValue,
    ) -> JsResult<JsValue> {
        conv::to_js(&e::EmbeddedCodec::new(kind_arg(&kind)?))
    }

    /// Validate `EmbeddedMetadataOptions` for `kind`
    /// (`EmbeddedMetadataOptions::validate`); throws `InvalidInputError`.
    #[wasm_bindgen(js_namespace = ["formats", "embedded"], js_name = validateMetadata)]
    pub fn validate_metadata(
        #[wasm_bindgen(
            unchecked_param_type = "SerdeTypes['formats::embedded::EmbeddedMetadataOptions']"
        )]
        options: JsValue,
        #[wasm_bindgen(unchecked_param_type = "BitmapKind")] kind: JsValue,
    ) -> JsResult<()> {
        let o: e::EmbeddedMetadataOptions = conv::from_js(&options)?;
        o.validate(kind_arg(&kind)?).js()
    }

    /// Retained bytes of `EmbeddedMetadataOptions`' caller-owned
    /// configuration, bounded by `maximum` (default unbounded;
    /// `EmbeddedMetadataOptions::retained_bytes`).
    #[wasm_bindgen(js_namespace = ["formats", "embedded"], js_name = metadataRetainedBytes)]
    pub fn metadata_retained_bytes(
        #[wasm_bindgen(
            unchecked_param_type = "SerdeTypes['formats::embedded::EmbeddedMetadataOptions']"
        )]
        options: JsValue,
        maximum: Option<f64>,
    ) -> JsResult<f64> {
        let o: e::EmbeddedMetadataOptions = conv::from_js(&options)?;
        let max = match maximum {
            Some(m) => conv::int::<usize>(m, "maximum")?,
            None => usize::MAX,
        };
        Ok(o.retained_bytes(max).js()? as f64)
    }

    /// The catalogue format IDs this namespace serves in this build.
    #[wasm_bindgen(js_namespace = ["formats", "embedded"], js_name = formatIds)]
    pub fn format_ids() -> Vec<String> {
        super::compiled(&["otb", "bdat", "color"])
    }
}

// --- vector_raster (ttf / otf / woff / woff2 / ttc / vector-raster) --------

/// `formats.vectorRaster`: outline rasterization and vectorization.
#[cfg(any(feature = "all-formats", feature = "merge", feature = "web"))]
pub mod vector_raster {
    use crate::conv;
    use crate::errors::{IntoJs, JsResult, invalid};
    use crate::formats::fonts_arg;
    use crate::formats_direct::{bytes_report, detect_codec, model_report, options_of, typed};
    use fontpixel::formats::vector_raster as v;
    use js_sys::{Object, Reflect};
    use wasm_bindgen::prelude::*;

    const IDS: [&str; 6] = ["ttf", "otf", "woff", "woff2", "ttc", "vector-raster"];

    fn vector_id(format: &str) -> JsResult<&'static str> {
        IDS.iter().copied().find(|id| *id == format).ok_or_else(|| {
            invalid(format!(
                "'{format}' is not an outline format; expected one of {IDS:?}"
            ))
        })
    }

    /// Rasterize a standalone outline font (`vector_raster::read`).
    /// `options.options` is a `vector_raster::Options` object. Returns
    /// `{model: FontSet, report}`.
    #[wasm_bindgen(js_namespace = ["formats", "vectorRaster"], js_name = read, unchecked_return_type = "ModelReport<FontSet>")]
    pub fn read(
        #[wasm_bindgen(unchecked_param_type = "BytesLike")] data: JsValue,
        #[wasm_bindgen(
            unchecked_optional_param_type = "{ options?: SerdeTypes['formats::vector_raster::Options'] }"
        )]
        options: Option<JsValue>,
    ) -> JsResult<JsValue> {
        let options = options_of(options, &["options"], "read options")?;
        let o: v::Options = typed(&options)?;
        let bytes = conv::bytes_from_js(&data, "data")?;
        model_report(v::read(&bytes, &o).js()?)
    }

    /// Vectorize strikes as `format` (`vector_raster::write`).
    /// `options.options` is a `vector_raster::WriteOptions` object. Returns
    /// `{data, report}`.
    #[wasm_bindgen(js_namespace = ["formats", "vectorRaster"], js_name = write, unchecked_return_type = "BytesReport")]
    pub fn write(
        #[wasm_bindgen(unchecked_param_type = "FontSet | Font")] fonts: JsValue,
        format: &str,
        #[wasm_bindgen(
            unchecked_optional_param_type = "{ options?: SerdeTypes['formats::vector_raster::WriteOptions'] }"
        )]
        options: Option<JsValue>,
    ) -> JsResult<JsValue> {
        let id = vector_id(format)?;
        let options = options_of(options, &["options"], "write options")?;
        let o: v::WriteOptions = typed(&options)?;
        let fonts = fonts_arg(&fonts)?;
        bytes_report(fonts.with_set(|s| Ok(v::write(s, id, &o)))?.js()?)
    }

    /// As `write`, but the model is handed over (`vector_raster::write_owned`):
    /// a `FontSet` handle is left empty afterwards. Output and report are
    /// equal.
    #[wasm_bindgen(js_namespace = ["formats", "vectorRaster"], js_name = writeOwned, unchecked_return_type = "BytesReport")]
    pub fn write_owned(
        #[wasm_bindgen(unchecked_param_type = "FontSet | Font")] fonts: JsValue,
        format: &str,
        #[wasm_bindgen(
            unchecked_optional_param_type = "{ options?: SerdeTypes['formats::vector_raster::WriteOptions'] }"
        )]
        options: Option<JsValue>,
    ) -> JsResult<JsValue> {
        let id = vector_id(format)?;
        let options = options_of(options, &["options"], "write options")?;
        let o: v::WriteOptions = typed(&options)?;
        let set = fonts_arg(&fonts)?.take_set()?;
        bytes_report(v::write_owned(set, id, &o).js()?)
    }

    /// Run the outline detector of `format` (default `"ttf"`;
    /// `vector_raster::Codec::new(format)`).
    #[wasm_bindgen(js_namespace = ["formats", "vectorRaster"], js_name = detect, unchecked_return_type = "Confidence")]
    pub fn detect(
        #[wasm_bindgen(unchecked_param_type = "BytesLike")] data: JsValue,
        format: Option<String>,
        #[wasm_bindgen(
            unchecked_optional_param_type = "{ name?: string; limits?: ResourceLimits }"
        )]
        options: Option<JsValue>,
    ) -> JsResult<JsValue> {
        let codec = v::Codec::new(vector_id(format.as_deref().unwrap_or("ttf"))?);
        detect_codec(&codec, &data, options)
    }

    /// The default codec options of `format` (`vector_raster::Codec::new`):
    /// `{id, read_options, write_options}`.
    #[wasm_bindgen(js_namespace = ["formats", "vectorRaster"], js_name = codec, unchecked_return_type = "{ id: string; read_options: SerdeTypes['formats::vector_raster::Options']; write_options: SerdeTypes['formats::vector_raster::WriteOptions'] }")]
    pub fn codec(format: &str) -> JsResult<JsValue> {
        let c = v::Codec::new(vector_id(format)?);
        let out = Object::new();
        Reflect::set(&out, &JsValue::from_str("id"), &JsValue::from_str(c.id))?;
        Reflect::set(
            &out,
            &JsValue::from_str("read_options"),
            &conv::to_js(&c.read_options)?,
        )?;
        Reflect::set(
            &out,
            &JsValue::from_str("write_options"),
            &conv::to_js(&c.write_options)?,
        )?;
        Ok(out.into())
    }

    /// Prove a standalone sfnt's native integral pixel grid
    /// (`vector_raster::detect_native_ppem`); `undefined` for a valid
    /// non-grid font. `limits` is a `vector_raster::Limits` object.
    #[wasm_bindgen(js_namespace = ["formats", "vectorRaster"], js_name = detectNativePpem)]
    pub fn detect_native_ppem(
        #[wasm_bindgen(unchecked_param_type = "BytesLike")] data: JsValue,
        #[wasm_bindgen(
            unchecked_optional_param_type = "SerdeTypes['formats::vector_raster::Limits']"
        )]
        limits: Option<JsValue>,
    ) -> JsResult<Option<u16>> {
        let bytes = conv::bytes_from_js(&data, "data")?;
        let l: v::Limits = conv::from_js_or_default(&limits.unwrap_or(JsValue::UNDEFINED))?;
        v::detect_native_ppem(&bytes, &l).js()
    }

    /// The catalogue format IDs this namespace serves in this build.
    #[wasm_bindgen(js_namespace = ["formats", "vectorRaster"], js_name = formatIds)]
    pub fn format_ids() -> Vec<String> {
        super::compiled(&IDS)
    }
}

// --- x11 helpers (also on `bdf`) --------------------------------------------

/// `formats.x11` (and the same helpers on `formats.bdf`): source rows of
/// glyphs read from BDF/PCF.
#[cfg(any(
    feature = "all-formats",
    feature = "minimal",
    feature = "merge",
    feature = "web"
))]
pub mod x11 {
    use crate::errors::{IntoJs, JsResult};
    use crate::formats::glyph_arg;
    use wasm_bindgen::prelude::*;

    fn rows(glyph: &JsValue) -> JsResult<Option<Vec<String>>> {
        glyph_arg(glyph)?.read(|g| {
            Ok(fontpixel::formats::bdf::source_rows(g)
                .map(|rows| rows.map(str::to_owned).collect()))
        })
    }

    fn validate(glyph: &JsValue) -> JsResult<()> {
        glyph_arg(glyph)?.read(|g| fontpixel::formats::bdf::validate_bitmap(g).js())
    }

    /// Original stripped hex rows of a glyph read from BDF/PCF source, or
    /// `undefined` when the glyph has no source provenance
    /// (`x11::source_rows`).
    #[wasm_bindgen(js_namespace = ["formats", "x11"], js_name = sourceRows)]
    pub fn source_rows(
        #[wasm_bindgen(unchecked_param_type = "Glyph")] glyph: JsValue,
    ) -> JsResult<Option<Vec<String>>> {
        rows(&glyph)
    }

    /// Reject invalid source hex with the exact glyph/row error
    /// (`x11::validate_bitmap`).
    #[wasm_bindgen(js_namespace = ["formats", "x11"], js_name = validateBitmap)]
    pub fn validate_bitmap(
        #[wasm_bindgen(unchecked_param_type = "Glyph")] glyph: JsValue,
    ) -> JsResult<()> {
        validate(&glyph)
    }

    /// `formats.bdf.sourceRows`: the same as `formats.x11.sourceRows`
    /// (`bdf::source_rows` re-exports it).
    #[wasm_bindgen(js_namespace = ["formats", "bdf"], js_name = sourceRows)]
    pub fn bdf_source_rows(
        #[wasm_bindgen(unchecked_param_type = "Glyph")] glyph: JsValue,
    ) -> JsResult<Option<Vec<String>>> {
        rows(&glyph)
    }

    /// `formats.bdf.validateBitmap`: the same as `formats.x11.validateBitmap`.
    #[wasm_bindgen(js_namespace = ["formats", "bdf"], js_name = validateBitmap)]
    pub fn bdf_validate_bitmap(
        #[wasm_bindgen(unchecked_param_type = "Glyph")] glyph: JsValue,
    ) -> JsResult<()> {
        validate(&glyph)
    }

    /// The catalogue format IDs this namespace serves.
    #[wasm_bindgen(js_namespace = ["formats", "x11"], js_name = formatIds)]
    pub fn format_ids() -> Vec<String> {
        vec!["bdf".to_owned(), "pcf".to_owned()]
    }
}

// --- svg / fnt / fbit / nfnt / hanfont / hzk / raw extras -------------------

/// Extra helpers of the `svg`, `fnt`, `fbit`, `nfnt`, `hanfont`, `hzk` and
/// `raw` namespaces.
#[cfg(feature = "all-formats")]
pub mod helpers {
    use crate::conv;
    use crate::errors::{IntoJs, JsResult, invalid};
    use crate::formats::{InputArgs, bitmap_arg, fonts_arg, glyph_arg, limits_of};
    use crate::formats_direct::{model_report, options_of, primary_of, read_ctx, typed, written};
    use crate::resources::JsWritten;
    use fontpixel::Bitmap;
    use fontpixel::formats as f;
    use fontpixel::model::GlyphBitmap;
    use wasm_bindgen::prelude::*;

    fn mono(bitmap: &JsValue) -> JsResult<Bitmap> {
        match bitmap_arg(bitmap)?.snapshot()? {
            GlyphBitmap::Mono(m) => Ok(m),
            GlyphBitmap::Gray8(_) => Err(invalid(
                "this function takes a Mono bitmap; threshold the Gray8 raster first",
            )),
        }
    }

    /// A standalone SVG image of a Mono bitmap (`svg::image`). `options` is
    /// an `svg::ImageOptions` object.
    #[wasm_bindgen(js_namespace = ["formats", "svg"], js_name = image)]
    pub fn image(
        #[wasm_bindgen(unchecked_param_type = "Bitmap")] bitmap: JsValue,
        #[wasm_bindgen(unchecked_optional_param_type = "SerdeTypes['formats::svg::ImageOptions']")]
        options: Option<JsValue>,
    ) -> JsResult<String> {
        let o: f::svg::ImageOptions =
            conv::from_js_or_default(&options.unwrap_or(JsValue::UNDEFINED))?;
        f::svg::image(&mono(&bitmap)?, &o).js()
    }

    /// One glyph's bitmap rectangle as SVG (`svg::glyph_image`); Gray8
    /// glyphs need `grayThreshold`.
    #[wasm_bindgen(js_namespace = ["formats", "svg"], js_name = glyphImage)]
    pub fn glyph_image(
        #[wasm_bindgen(unchecked_param_type = "Glyph")] glyph: JsValue,
        #[wasm_bindgen(
            unchecked_optional_param_type = "{ options?: SerdeTypes['formats::svg::ImageOptions']; grayThreshold?: number }"
        )]
        options: Option<JsValue>,
    ) -> JsResult<String> {
        let options = options_of(options, &["options", "grayThreshold"], "glyphImage options")?;
        let o: f::svg::ImageOptions = typed(&options)?;
        let threshold = match options.get("grayThreshold")? {
            Some(v) => Some(conv::int::<u8>(
                v.as_f64()
                    .ok_or_else(|| invalid("grayThreshold must be a number"))?,
                "grayThreshold",
            )?),
            None => None,
        };
        glyph_arg(&glyph)?.read(|g| f::svg::glyph_image(g, &o, threshold).js())
    }

    /// Deterministic upward-facing SVG path data of a Mono bitmap
    /// (`svg::path_data`). `origin` is `[x, y]`; `shape` an `outline::Shape`
    /// name (default `"square"`), `limits` an `outline::Limits` object,
    /// `maxBytes` the output cap (default 16 MiB).
    #[wasm_bindgen(js_namespace = ["formats", "svg"], js_name = pathData)]
    pub fn path_data(
        #[wasm_bindgen(unchecked_param_type = "Bitmap")] bitmap: JsValue,
        #[wasm_bindgen(unchecked_param_type = "[number, number]")] origin: JsValue,
        units_per_pixel: f64,
        #[wasm_bindgen(
            unchecked_optional_param_type = "{ shape?: Shape; dotSize?: number; limits?: SerdeTypes['outline::Limits']; maxBytes?: number }"
        )]
        options: Option<JsValue>,
    ) -> JsResult<String> {
        let origin: (i32, i32) = conv::from_js(&origin)?;
        let upp = conv::int::<u16>(units_per_pixel, "unitsPerPixel")?;
        let options = options_of(
            options,
            &["shape", "dotSize", "limits", "maxBytes"],
            "pathData options",
        )?;
        let shape: fontpixel::outline::Shape = options
            .plain("shape")?
            .unwrap_or(fontpixel::outline::Shape::Square);
        let dot_size = match options.get("dotSize")? {
            Some(v) => v
                .as_f64()
                .ok_or_else(|| invalid("dotSize must be a number"))?,
            None => 1.0,
        };
        let limits: fontpixel::outline::Limits = options.plain("limits")?.unwrap_or_default();
        let max_bytes = match options.get("maxBytes")? {
            Some(v) => conv::int::<usize>(
                v.as_f64()
                    .ok_or_else(|| invalid("maxBytes must be a number"))?,
                "maxBytes",
            )?,
            None => 16 * 1024 * 1024,
        };
        f::svg::path_data(
            &mono(&bitmap)?,
            origin,
            upp,
            shape,
            dot_size,
            &limits,
            max_bytes,
        )
        .js()
    }

    /// Exact code-page name of a Windows charset byte, or `undefined` for
    /// OEM, symbol, DEFAULT and unknown identifiers (`fnt::charset_name`).
    #[wasm_bindgen(js_namespace = ["formats", "fnt"], js_name = charsetName)]
    pub fn charset_name(id: f64) -> JsResult<Option<String>> {
        Ok(f::fnt::charset_name(conv::int::<u8>(id, "id")?).map(str::to_owned))
    }

    /// Decode FNT bytes with default charset interpretation
    /// (`fnt::read_default`). Returns `{model: Font, report}`.
    #[wasm_bindgen(js_namespace = ["formats", "fnt"], js_name = readDefault, unchecked_return_type = "ModelReport<Font>")]
    pub fn read_default(
        #[wasm_bindgen(unchecked_param_type = "BytesLike")] data: JsValue,
        #[wasm_bindgen(unchecked_optional_param_type = "{ limits?: ResourceLimits }")]
        options: Option<JsValue>,
    ) -> JsResult<JsValue> {
        let options = options_of(options, &["limits"], "read options")?;
        let bytes = conv::bytes_from_js(&data, "data")?;
        model_report(f::fnt::read_default(&bytes, &limits_of(&options, "limits")?).js()?)
    }

    /// Decode FONTBIT through a context with explicitly scoped data/resource
    /// forks (`fbit::read_context`). Returns `{model: FontSet, report}`.
    #[wasm_bindgen(js_namespace = ["formats", "fbit"], js_name = readContext, unchecked_return_type = "ModelReport<FontSet>")]
    pub fn read_context(
        #[wasm_bindgen(unchecked_param_type = "BytesLike | Written | OutputBundle | undefined")]
        data: JsValue,
        #[wasm_bindgen(
            unchecked_optional_param_type = "DirectContextReadArgs<SerdeTypes['formats::fbit::FbitReadOptions']>"
        )]
        options: Option<JsValue>,
    ) -> JsResult<JsValue> {
        let options = options_of(
            options,
            &["options", "name", "resources", "limits", "directory"],
            "read options",
        )?;
        let o: f::fbit::FbitReadOptions = typed(&options)?;
        let input = InputArgs::from_options(&data, &options)?;
        read_ctx(&input, &limits_of(&options, "limits")?, |c| {
            f::fbit::read_context(c, &o)
        })
    }

    /// Emit every FONTBIT fork as a bundle (`fbit::write_bundle`). Returns a
    /// `Written`.
    #[wasm_bindgen(js_namespace = ["formats", "fbit"], js_name = writeBundle)]
    pub fn fbit_write_bundle(
        #[wasm_bindgen(unchecked_param_type = "FontSet | Font")] fonts: JsValue,
        #[wasm_bindgen(
            unchecked_optional_param_type = "{ options?: SerdeTypes['formats::fbit::FbitWriteOptions']; primaryName?: string }"
        )]
        options: Option<JsValue>,
    ) -> JsResult<JsWritten> {
        let options = options_of(options, &["options", "primaryName"], "write options")?;
        let o: f::fbit::FbitWriteOptions = typed(&options)?;
        let primary = primary_of("fbit", &options)?;
        let fonts = fonts_arg(&fonts)?;
        written(fonts.with_set(|s| Ok(f::fbit::write_bundle(s, &o, &primary)))?)
    }

    /// Encode every NFNT asset as a bundle (`nfnt::write_bundle`). Returns a
    /// `Written`.
    #[wasm_bindgen(js_namespace = ["formats", "nfnt"], js_name = writeBundle)]
    pub fn nfnt_write_bundle(
        #[wasm_bindgen(unchecked_param_type = "FontSet | Font")] fonts: JsValue,
        #[wasm_bindgen(
            unchecked_optional_param_type = "{ options?: SerdeTypes['formats::nfnt::NfntWriteOptions']; primaryName?: string }"
        )]
        options: Option<JsValue>,
    ) -> JsResult<JsWritten> {
        let options = options_of(options, &["options", "primaryName"], "write options")?;
        let o: f::nfnt::NfntWriteOptions = typed(&options)?;
        let primary = primary_of("nfnt", &options)?;
        let fonts = fonts_arg(&fonts)?;
        written(fonts.with_set(|s| Ok(f::nfnt::write_bundle(s, &o, &primary)))?)
    }

    /// The `[initial, medial, final]` component slots of a modern Hangul
    /// syllable; `final` is `null` when absent (`hanfont::component_slots`).
    #[wasm_bindgen(js_namespace = ["formats", "hanfont"], js_name = componentSlots, unchecked_return_type = "[number, number, number | null]")]
    pub fn component_slots(codepoint: f64) -> JsResult<JsValue> {
        let slots = f::hanfont::component_slots(conv::int::<u32>(codepoint, "codepoint")?).js()?;
        conv::to_js(&slots)
    }

    /// Every HZK/ASC preset name in stable order (`hzk::Preset::ALL`).
    #[wasm_bindgen(js_namespace = ["formats", "hzk"], js_name = presets)]
    pub fn presets() -> Vec<String> {
        f::hzk::Preset::ALL
            .iter()
            .map(|p| p.name().to_owned())
            .collect()
    }

    /// The `Geometry` of an HZK/ASC preset (`hzk::Preset::geometry`).
    #[wasm_bindgen(js_namespace = ["formats", "hzk"], js_name = presetGeometry, unchecked_return_type = "SerdeTypes['formats::hzk::Geometry']")]
    pub fn preset_geometry(
        #[wasm_bindgen(unchecked_param_type = "SerdeTypes['formats::hzk::Preset']")]
        preset: JsValue,
    ) -> JsResult<JsValue> {
        let p: f::hzk::Preset = conv::from_js(&preset)?;
        conv::to_js(&p.geometry())
    }

    /// The command-line spelling of an HZK/ASC preset (`hzk::Preset::name`).
    #[wasm_bindgen(js_namespace = ["formats", "hzk"], js_name = presetName)]
    pub fn preset_name(
        #[wasm_bindgen(unchecked_param_type = "SerdeTypes['formats::hzk::Preset']")]
        preset: JsValue,
    ) -> JsResult<String> {
        let p: f::hzk::Preset = conv::from_js(&preset)?;
        Ok(p.name().to_owned())
    }

    /// The default `RawLayout` of a `width`×`height` cell
    /// (`raw::RawLayout::new`).
    #[wasm_bindgen(js_namespace = ["formats", "raw"], js_name = rawLayout, unchecked_return_type = "SerdeTypes['formats::raw::RawLayout']")]
    pub fn raw_layout(width: f64, height: f64) -> JsResult<JsValue> {
        conv::to_js(&f::raw::RawLayout::new(
            conv::int::<u32>(width, "width")?,
            conv::int::<u32>(height, "height")?,
        ))
    }

    /// The `RawLayout` of a named raw preset (`raw::RawPreset::layout`).
    #[wasm_bindgen(js_namespace = ["formats", "raw"], js_name = presetLayout, unchecked_return_type = "SerdeTypes['formats::raw::RawLayout']")]
    pub fn preset_layout(
        #[wasm_bindgen(unchecked_param_type = "SerdeTypes['formats::raw::RawPreset']")]
        preset: JsValue,
    ) -> JsResult<JsValue> {
        let p: f::raw::RawPreset = conv::from_js(&preset)?;
        conv::to_js(&p.layout())
    }
}
