//! Charset tables, codecs and Unicode merge categories (`fontpixel::charset`).
//!
//! `CharsetCodec` is a handle over the static mapping tables. Registry
//! records (`CharsetInfo` plus `available`), Unicode ranges and Windows
//! charsets are returned as plain data; enums use their serde strings
//! (`CharsetKind`: `"unicode"`, `"single_byte"`, `"double_byte"`,
//! `"gb18030"`; `EastAsianWidth`: `"ambiguous"`, `"fullwidth"`, …). Lookups
//! that find nothing return `undefined`. A charset whose table is compiled
//! out of this build (`cjk_charsets` capability) throws
//! `FeatureDisabledError` with `kind` `"CharsetFeatureDisabled"`, `charset`
//! and `feature`.
use crate::conv;
use crate::errors::{Fields, JsResult, invalid};
use crate::handles::JsFont;
use fontpixel::charset::merge_categories::{
    self as mc, CJK_RANGES, GEOMETRIC_RANGES, UNICODE_BLOCKS, UNICODE_VERSION,
};
use fontpixel::charset::{self, CharsetCodec, CharsetError};
use fontpixel::model::FontCharset;
use js_sys::{Array, Reflect};
use wasm_bindgen::prelude::*;

/// `FeatureDisabledError` for a charset table compiled out of this build:
/// `kind` `"CharsetFeatureDisabled"`, `charset` (the canonical key) and
/// `feature` (`"cjk-charsets"`).
pub fn charset_error(err: CharsetError) -> JsValue {
    let message = err.to_string();
    match &err {
        CharsetError::FeatureDisabled(key) => {
            let error = Fields::new()
                .set("exitCode", 1)
                .set("charset", key.as_str())
                .set("feature", "cjk-charsets")
                .error("FeatureDisabled", &message);
            // `Fields::error` sets `kind` to the class's variant; Python
            // reports this case as "CharsetFeatureDisabled".
            let _ = Reflect::set(
                &error,
                &JsValue::from_str("kind"),
                &JsValue::from_str("CharsetFeatureDisabled"),
            );
            error
        }
    }
}

fn u32_arg(value: f64, what: &str) -> JsResult<u32> {
    conv::int(value, what)
}

/// An exact charset mapping (`fontpixel::charset::CharsetCodec`).
#[wasm_bindgen(js_name = CharsetCodec)]
pub struct JsCharsetCodec {
    codec: CharsetCodec,
}

impl JsCharsetCodec {
    /// Wrap a codec.
    pub fn new(codec: CharsetCodec) -> Self {
        Self { codec }
    }
    /// The wrapped codec.
    pub fn codec(&self) -> &CharsetCodec {
        &self.codec
    }
}

#[wasm_bindgen(js_class = CharsetCodec)]
impl JsCharsetCodec {
    /// Resolve a user-facing charset name (`CharsetCodec::new`). `gl`
    /// `undefined` selects GL for `iso2022cn` only. Unknown names give an
    /// identity codec with `known === false`.
    #[wasm_bindgen(constructor)]
    pub fn constructor(name: &str, gl: Option<bool>) -> JsResult<JsCharsetCodec> {
        CharsetCodec::new(name, gl)
            .map(Self::new)
            .map_err(charset_error)
    }
    /// Resolve a canonical key or cleaned alias without re-sanitizing it
    /// (`CharsetCodec::from_key`).
    #[wasm_bindgen(js_name = fromKey)]
    pub fn from_key(key: &str, gl: Option<bool>) -> JsResult<JsCharsetCodec> {
        CharsetCodec::from_key(key, gl)
            .map(Self::new)
            .map_err(charset_error)
    }
    /// The codec of a font's charset (`FontCharset::codec`).
    #[wasm_bindgen(js_name = forFont)]
    pub fn for_font(font: &JsFont) -> JsResult<JsCharsetCodec> {
        let charset = font.read(|f| Ok(f.charset.clone()))?;
        charset.codec().map(Self::new).map_err(charset_error)
    }
    /// The codec of `FontCharset` plain data (`FontCharset::codec`).
    #[wasm_bindgen(js_name = forCharset)]
    pub fn for_charset(
        #[wasm_bindgen(unchecked_param_type = "FontCharset")] charset: JsValue,
    ) -> JsResult<JsCharsetCodec> {
        let charset: FontCharset = conv::from_js(&charset)?;
        charset.codec().map(Self::new).map_err(charset_error)
    }
    /// Canonical key (`CharsetCodec::name`).
    #[wasm_bindgen(getter)]
    pub fn name(&self) -> String {
        self.codec.name().to_owned()
    }
    /// Whether the name resolved to a known table (`CharsetCodec::known`).
    #[wasm_bindgen(getter)]
    pub fn known(&self) -> bool {
        self.codec.known()
    }
    /// Allowed code lengths (`CharsetCodec::kind`).
    #[wasm_bindgen(getter, unchecked_return_type = "CharsetKind")]
    pub fn kind(&self) -> JsResult<JsValue> {
        conv::to_js(&self.codec.kind())
    }
    /// Whether 94×94 GL codes are used (`CharsetCodec::gl`).
    #[wasm_bindgen(getter)]
    pub fn gl(&self) -> bool {
        self.codec.gl()
    }
    /// Unicode value for a glyph code, or `undefined`
    /// (`CharsetCodec::cp2unicode`).
    pub fn cp2unicode(&self, code: f64) -> JsResult<Option<u32>> {
        Ok(self.codec.cp2unicode(u32_arg(code, "code")?))
    }
    /// Character for a glyph code, or `undefined` (`CharsetCodec::cp2char`).
    pub fn cp2char(&self, code: f64) -> JsResult<Option<String>> {
        Ok(self.codec.cp2char(u32_arg(code, "code")?).map(String::from))
    }
    /// Glyph code for a Unicode value, or `undefined`
    /// (`CharsetCodec::unicode2cp`).
    pub fn unicode2cp(&self, value: f64) -> JsResult<Option<u32>> {
        Ok(self.codec.unicode2cp(u32_arg(value, "value")?))
    }
    /// Glyph code for a one-character string, or `undefined`
    /// (`CharsetCodec::char2cp`).
    pub fn char2cp(&self, value: &str) -> JsResult<Option<u32>> {
        let mut chars = value.chars();
        match (chars.next(), chars.next()) {
            (Some(c), None) => Ok(self.codec.char2cp(c)),
            _ => Err(invalid(format!(
                "char2cp expects exactly one character, got {value:?}"
            ))),
        }
    }
    /// `CharsetCodec(name=…, known=…, gl=…)`.
    #[wasm_bindgen(js_name = toString)]
    pub fn to_string_js(&self) -> String {
        format!(
            "CharsetCodec(name={:?}, known={}, gl={})",
            self.codec.name(),
            self.codec.known(),
            self.codec.gl()
        )
    }
}

/// Every canonical charset (`charset_table`): `CharsetInfo` records (`key`,
/// `python_codec`, `kind`, `gl_capable`, `force_gl`, `properties`,
/// `aliases`) plus `available` (`CharsetInfo::available`: false when the
/// table is compiled out of this build).
#[wasm_bindgen(js_name = charsetTable, unchecked_return_type = "(CharsetInfo & { available: boolean })[]")]
pub fn charset_table() -> JsResult<Array> {
    charset::charset_table()
        .iter()
        .map(|info| {
            let record = conv::to_js(info)?;
            Reflect::set(
                &record,
                &JsValue::from_str("available"),
                &JsValue::from_bool(info.available()),
            )?;
            Ok(record)
        })
        .collect()
}

/// Whether a canonical key denotes Unicode identity (`is_unicode_charset`).
#[wasm_bindgen(js_name = isUnicodeCharset)]
pub fn is_unicode_charset(key: &str) -> bool {
    charset::is_unicode_charset(key)
}

/// Canonical key from XLFD registry/encoding (`sanitize_charset`); either
/// may be `undefined`/`null`.
#[wasm_bindgen(js_name = sanitizeCharset)]
pub fn sanitize_charset(registry: Option<String>, encoding: Option<String>) -> String {
    charset::sanitize_charset(registry.as_deref(), encoding.as_deref())
}

/// Canonical key for a user-facing name (`canonical_charset`).
#[wasm_bindgen(js_name = canonicalCharset)]
pub fn canonical_charset(name: &str) -> String {
    charset::canonical_charset(name)
}

/// `[CHARSET_REGISTRY, CHARSET_ENCODING]` for a key (`charset_props`).
#[wasm_bindgen(js_name = charsetProps, unchecked_return_type = "[string, string]")]
pub fn charset_props(key: &str) -> JsResult<JsValue> {
    conv::to_js(&charset::charset_props(key))
}

/// Infer the GL form from glyph codes (`detect_gl`); `encoding` may be
/// `undefined`/`null`.
#[wasm_bindgen(js_name = detectGl)]
pub fn detect_gl(
    key: &str,
    encoding: Option<String>,
    #[wasm_bindgen(unchecked_param_type = "number[]")] codes: JsValue,
) -> JsResult<bool> {
    let codes: Vec<u32> = conv::from_js(&codes)?;
    Ok(charset::detect_gl(key, encoding.as_deref(), codes))
}

/// Resolve a Windows `dfCharSet` byte (`windows_charset`):
/// `{code_page: "cp1252"}`, `"system_ansi"`, `"system_oem"`,
/// `"system_mac"`, `"symbol"` or `{unknown: n}`.
#[wasm_bindgen(js_name = windowsCharset, unchecked_return_type = "WindowsCharset")]
pub fn windows_charset(value: f64) -> JsResult<JsValue> {
    conv::to_js(&charset::windows_charset(conv::int(value, "value")?))
}

/// One-based 94×94 row/cell to an EUC code, or `undefined` (`kuten_to_euc`).
#[wasm_bindgen(js_name = kutenToEuc)]
pub fn kuten_to_euc(row: f64, cell: f64) -> JsResult<Option<u16>> {
    Ok(charset::kuten_to_euc(
        conv::int(row, "row")?,
        conv::int(cell, "cell")?,
    ))
}

/// EUC code to one-based `[row, cell]`, or `undefined` (`euc_to_kuten`).
#[wasm_bindgen(js_name = eucToKuten, unchecked_return_type = "[number, number] | undefined")]
pub fn euc_to_kuten(code: f64) -> JsResult<JsValue> {
    match charset::euc_to_kuten(conv::int(code, "code")?) {
        Some(pair) => conv::to_js(&pair),
        None => Ok(JsValue::UNDEFINED),
    }
}

/// Canonical charset name of `FontCharset` plain data
/// (`FontCharset::canonical_name`).
#[wasm_bindgen(js_name = charsetCanonicalName)]
pub fn charset_canonical_name(
    #[wasm_bindgen(unchecked_param_type = "FontCharset")] charset: JsValue,
) -> JsResult<String> {
    let charset: FontCharset = conv::from_js(&charset)?;
    Ok(charset.canonical_name())
}

// merge_categories ----------------------------------------------------------

/// Whether a value is a Unicode scalar (`merge_categories::is_scalar`).
#[wasm_bindgen(js_name = isScalar)]
pub fn is_scalar(cp: f64) -> JsResult<bool> {
    Ok(mc::is_scalar(u32_arg(cp, "cp")?))
}
/// Explicit CJK block policy (`merge_categories::is_cjk`).
#[wasm_bindgen(js_name = isCjk)]
pub fn is_cjk(cp: f64) -> JsResult<bool> {
    Ok(mc::is_cjk(u32_arg(cp, "cp")?))
}
/// Scalar complement of CJK (`merge_categories::is_western`).
#[wasm_bindgen(js_name = isWestern)]
pub fn is_western(cp: f64) -> JsResult<bool> {
    Ok(mc::is_western(u32_arg(cp, "cp")?))
}
/// Box/block/braille/Powerline ranges (`merge_categories::is_geometric`).
#[wasm_bindgen(js_name = isGeometric)]
pub fn is_geometric(cp: f64) -> JsResult<bool> {
    Ok(mc::is_geometric(u32_arg(cp, "cp")?))
}
/// Han ideograph blocks (`merge_categories::is_han`).
#[wasm_bindgen(js_name = isHan)]
pub fn is_han(cp: f64) -> JsResult<bool> {
    Ok(mc::is_han(u32_arg(cp, "cp")?))
}
/// The Unicode block `{start, end, name}`, or `undefined`
/// (`merge_categories::unicode_block`).
#[wasm_bindgen(js_name = unicodeBlock, unchecked_return_type = "UnicodeRange | undefined")]
pub fn unicode_block(cp: f64) -> JsResult<JsValue> {
    match mc::unicode_block(u32_arg(cp, "cp")?) {
        Some(range) => conv::to_js(range),
        None => Ok(JsValue::UNDEFINED),
    }
}
/// East Asian Width property, or `undefined` for non-scalars
/// (`merge_categories::east_asian_width`).
#[wasm_bindgen(js_name = eastAsianWidth, unchecked_return_type = "EastAsianWidth | undefined")]
pub fn east_asian_width(cp: f64) -> JsResult<JsValue> {
    match mc::east_asian_width(u32_arg(cp, "cp")?) {
        Some(width) => conv::to_js(&width),
        None => Ok(JsValue::UNDEFINED),
    }
}
/// The Unicode version of the category tables
/// (`merge_categories::UNICODE_VERSION`).
#[wasm_bindgen(js_name = unicodeVersion)]
pub fn unicode_version() -> String {
    UNICODE_VERSION.to_owned()
}
/// The CJK ranges, in order (`merge_categories::CJK_RANGES`; a fresh array
/// on every call).
#[wasm_bindgen(js_name = cjkRanges, unchecked_return_type = "UnicodeRange[]")]
pub fn cjk_ranges() -> JsResult<JsValue> {
    conv::to_js(CJK_RANGES)
}
/// The geometric ranges, in order (`merge_categories::GEOMETRIC_RANGES`).
#[wasm_bindgen(js_name = geometricRanges, unchecked_return_type = "UnicodeRange[]")]
pub fn geometric_ranges() -> JsResult<JsValue> {
    conv::to_js(GEOMETRIC_RANGES)
}
/// Every Unicode block, in order (`merge_categories::UNICODE_BLOCKS`).
#[wasm_bindgen(js_name = unicodeBlocks, unchecked_return_type = "UnicodeRange[]")]
pub fn unicode_blocks() -> JsResult<JsValue> {
    conv::to_js(UNICODE_BLOCKS)
}
