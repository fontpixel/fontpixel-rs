//! Module functions of the bdfparser-js compatible API: charset codecs and
//! names (`fontpixel::charset` as bdfparser's SPEC-v3 §4/§11.1 tables), the
//! TrueType reader/writer, outlines and number formatting
//! (`fontpixel::compat::bdfparser::{ttf, outline, svg}`).
use crate::compat::{JsCompatFont, done, ttf_options};
use crate::compat_bitmap::{JsCompatBitmap, string_rows};
use crate::conv;
use crate::errors::{self, JsResult};
use fontpixel::charset::{self, CharsetCodec, Kind};
use fontpixel::compat::bdfparser as bp;
use js_sys::Uint8Array;
use serde::Serialize;
use wasm_bindgen::prelude::*;

fn kind_name(kind: Kind) -> &'static str {
    match kind {
        Kind::Unicode => "unicode",
        Kind::SingleByte => "sbcs",
        Kind::DoubleByte => "dbcs",
        Kind::Gb18030 => "gb18030",
    }
}

/// A character ⇄ codepoint converter of one charset (`fontpixel::charset::CharsetCodec`).
#[wasm_bindgen(js_name = BdfparserRawCodec)]
pub struct JsCompatCodec {
    codec: CharsetCodec,
    gl94: bool,
}

#[wasm_bindgen(js_class = BdfparserRawCodec)]
impl JsCompatCodec {
    /// The codec of a charset name (any spelling, canonicalized with
    /// `canonicalCharset`); `gl` selects the 94×94 GL form (`undefined`: GL
    /// only for charsets that are always GL) (`CharsetCodec::new`).
    pub fn open(name: &str, gl: Option<bool>) -> JsResult<JsCompatCodec> {
        let codec = CharsetCodec::new(name, gl).map_err(|e| errors::invalid(e.to_string()))?;
        let gl94 = charset::charset_table()
            .iter()
            .find(|e| e.key == codec.name())
            .is_some_and(|e| e.gl_capable || e.force_gl);
        Ok(Self { codec, gl94 })
    }
    /// The canonical key (`CharsetCodec::name`).
    #[wasm_bindgen(getter)]
    pub fn charset(&self) -> String {
        self.codec.name().to_owned()
    }
    /// Whether the name is a known charset (`CharsetCodec::known`).
    #[wasm_bindgen(getter)]
    pub fn known(&self) -> bool {
        self.codec.known()
    }
    /// `'unicode'`, `'sbcs'`, `'dbcs'` or `'gb18030'` (`CharsetCodec::kind`).
    #[wasm_bindgen(getter)]
    pub fn kind(&self) -> String {
        kind_name(self.codec.kind()).to_owned()
    }
    /// Whether this codec uses 94×94 GL codes (`CharsetCodec::gl`).
    #[wasm_bindgen(getter)]
    pub fn gl(&self) -> bool {
        self.codec.gl()
    }
    /// Whether the charset is a GL-capable 94×94 set.
    #[wasm_bindgen(getter)]
    pub fn gl94(&self) -> bool {
        self.gl94
    }
    /// Code → Unicode code point (surrogates retained for Unicode identity),
    /// `undefined` if not mapped (`CharsetCodec::cp2unicode`).
    pub fn cp2unicode(&self, code: f64) -> Option<u32> {
        if code.fract() != 0.0 || !(0.0..=f64::from(u32::MAX)).contains(&code) {
            return None;
        }
        self.codec.cp2unicode(code as u32)
    }
    /// Unicode code point → code (round-trip exact), `undefined` if not
    /// representable (`CharsetCodec::unicode2cp`).
    pub fn unicode2cp(&self, value: u32) -> Option<u32> {
        self.codec.unicode2cp(value)
    }
    /// One character → code (`CharsetCodec::char2cp`).
    pub fn char2cp(&self, ch: char) -> Option<u32> {
        self.codec.char2cp(ch)
    }
    /// Code → one character (`CharsetCodec::cp2char`).
    pub fn cp2char(&self, code: f64) -> Option<String> {
        self.cp2unicode(code)
            .and_then(char::from_u32)
            .map(String::from)
    }
}

/// Canonical charset key from `CHARSET_REGISTRY` / `CHARSET_ENCODING` values
/// (`'iso10646'` without a registry) (`charset::sanitize_charset`).
#[wasm_bindgen(js_name = bdfparserSanitizeCharset)]
pub fn sanitize_charset(registry: Option<String>, encoding: Option<String>) -> String {
    charset::sanitize_charset(registry.as_deref(), encoding.as_deref())
}

/// Canonical key of any charset spelling (`charset::canonical_charset`).
#[wasm_bindgen(js_name = bdfparserCanonicalCharset)]
pub fn canonical_charset(name: &str) -> String {
    charset::canonical_charset(name)
}

/// `[CHARSET_REGISTRY, CHARSET_ENCODING]` for a canonical key (`charset::charset_props`).
#[wasm_bindgen(js_name = bdfparserCharsetProps)]
pub fn charset_props(key: &str) -> Vec<String> {
    let (r, e) = charset::charset_props(key);
    vec![r, e]
}

/// Whether a canonical key is a Unicode charset (`charset::is_unicode_charset`).
#[wasm_bindgen(js_name = bdfparserIsUnicodeCharset)]
pub fn is_unicode_charset(key: &str) -> bool {
    charset::is_unicode_charset(key)
}

/// GL form detection for 94×94 sets: `CHARSET_ENCODING` `"0"` or the first
/// two-byte code in GL range (`charset::detect_gl`).
#[wasm_bindgen(js_name = bdfparserDetectGl)]
pub fn detect_gl(key: &str, encoding: Option<String>, codes: Vec<f64>) -> bool {
    let codes = codes
        .into_iter()
        .filter(|c| c.fract() == 0.0 && (0.0..=f64::from(u32::MAX)).contains(c))
        .map(|c| c as u32);
    charset::detect_gl(key, encoding.as_deref(), codes)
}

#[derive(Serialize)]
struct TableRow {
    key: &'static str,
    codec: Option<&'static str>,
    kind: &'static str,
    gl: bool,
    forcegl: bool,
    props: (&'static str, &'static str),
    aliases: &'static [&'static str],
    available: bool,
}

/// The canonical charset table (`charset::charset_table`), one record per key:
/// `{key, codec, kind, gl, forcegl, props, aliases, available}`.
#[wasm_bindgen(js_name = bdfparserCharsetTable, unchecked_return_type = "Record<string, unknown>[]")]
pub fn charset_table() -> JsResult<JsValue> {
    let rows = charset::charset_table()
        .iter()
        .map(|e| TableRow {
            key: e.key,
            codec: e.python_codec,
            kind: kind_name(e.kind),
            gl: e.gl_capable,
            forcegl: e.force_gl,
            props: e.properties,
            aliases: e.aliases,
            available: e.available(),
        })
        .collect::<Vec<_>>();
    conv::to_js(&rows)
}

/// The 17 glyph meta keys in slot order (`glyph::META_TITLES`).
#[wasm_bindgen(js_name = bdfparserMetaTitles)]
pub fn meta_titles() -> Vec<String> {
    bp::META_TITLES.iter().map(|s| (*s).to_owned()).collect()
}

/// `uniXXXX`/`uXXXXX` (Unicode) or `char{cp}` (`glyph::defaultglyphname`).
#[wasm_bindgen(js_name = bdfparserDefaultGlyphName)]
pub fn defaultglyphname(cp: JsValue, unicode: bool) -> JsResult<String> {
    let cp: i64 = conv::int_from_js(&cp, "cp")?;
    Ok(bp::defaultglyphname(cp, unicode))
}

/// Shortest text of a number as written in SVG output (`svg::fmtnum`).
#[wasm_bindgen(js_name = bdfparserFmtnum)]
pub fn fmtnum(n: f64) -> String {
    bp::svg::fmtnum(n)
}

/// BDF version text: `2` → `'2.0'`, `2.1` → `'2.1'` (`font::fmtver`).
#[wasm_bindgen(js_name = bdfparserFmtver)]
pub fn fmtver(v: f64) -> String {
    bp::fmtver(v)
}

/// Read a TrueType (`glyf`) font: `write_ttf` output with `BDFP` metadata is
/// restored losslessly, others are rasterized at `pixelsize` (`ttf::read_ttf`).
#[wasm_bindgen(js_name = bdfparserReadTtf)]
pub fn read_ttf(
    #[wasm_bindgen(unchecked_param_type = "BytesLike")] data: JsValue,
    #[wasm_bindgen(unchecked_optional_param_type = "number")] pixelsize: Option<JsValue>,
) -> JsResult<JsCompatFont> {
    let data = conv::bytes_from_js(&data, "data")?;
    let pixelsize = crate::compat::opt_int(&pixelsize.unwrap_or(JsValue::UNDEFINED), "pixelsize")?;
    done(bp::ttf::read_ttf(&data, pixelsize)).map(JsCompatFont::wrap)
}

/// SPEC TrueType bytes with `TtfOptions` plain data (`ttf::write_ttf`).
#[wasm_bindgen(js_name = bdfparserWriteTtf)]
pub fn write_ttf(
    font: &JsCompatFont,
    #[wasm_bindgen(unchecked_optional_param_type = "TtfOptions")] options: Option<JsValue>,
) -> JsResult<Uint8Array> {
    let o = ttf_options(&options.unwrap_or(JsValue::UNDEFINED))?;
    font.read(|f| done(bp::ttf::write_ttf(f, &o)).map(|b| conv::bytes_to_js(&b)))
}

/// SPEC outlines of bitmap rows as contours of `[x, y, onCurve]` points
/// (`outline::bitmap_contours`).
#[wasm_bindgen(js_name = bdfparserBitmapContours, unchecked_return_type = "[number, number, boolean][][]")]
pub fn bitmap_contours(
    #[wasm_bindgen(unchecked_param_type = "string[]")] bindata: JsValue,
    shape: &str,
    dotsize: f64,
    ppu: JsValue,
    xoff: JsValue,
    yoff: JsValue,
) -> JsResult<JsValue> {
    let rows = string_rows(&bindata, "bindata")?;
    let ppu: i64 = conv::int_from_js(&ppu, "ppu")?;
    let xoff: i64 = conv::int_from_js(&xoff, "xoff")?;
    let yoff: i64 = conv::int_from_js(&yoff, "yoff")?;
    let cs = done(bp::outline::bitmap_contours(
        &rows, shape, dotsize, ppu, xoff, yoff,
    ))?;
    conv::to_js(&cs)
}

/// Nonzero-winding pixel-centre rasterization of `[x, y, onCurve]` contours
/// (16 chords per quadratic curve) (`outline::rasterize`).
#[wasm_bindgen(js_name = bdfparserRasterize)]
pub fn rasterize(
    #[wasm_bindgen(unchecked_param_type = "[number, number, boolean][][]")] contours: JsValue,
    ppu: f64,
    xmin: JsValue,
    ymin: JsValue,
    w: JsValue,
    h: JsValue,
) -> JsResult<JsCompatBitmap> {
    let cs: Vec<bp::outline::Contour> = conv::from_js(&contours)?;
    let int = |v: &JsValue, what: &str| conv::int_from_js::<i64>(v, what);
    let (xmin, ymin, w, h) = (
        int(&xmin, "xmin")?,
        int(&ymin, "ymin")?,
        int(&w, "w")?,
        int(&h, "h")?,
    );
    done(bp::outline::rasterize(&cs, ppu, xmin, ymin, w, h)).map(JsCompatBitmap::wrap)
}
