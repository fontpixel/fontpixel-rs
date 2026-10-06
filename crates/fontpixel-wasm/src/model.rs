//! Model handles: `FontSet`, `Font`, `Glyph` (`fontpixel::model`), the
//! `Codepoint` value class and the `Properties` list handle.
//!
//! Semantics follow the Python binding (`crates/fontpixel-py/src/model.rs`):
//!
//! - **Views.** `set.font(i)`, `set.fonts`, `font.glyph(id)`, `font.glyphs()`
//!   and the `properties`/`headers`/`localizedNames` getters return live
//!   views addressed by index/ID; edits through them change the containing
//!   value, and a view whose target disappeared throws `InvalidInputError`.
//!   `copy()` is the explicit deep copy; constructors and `add*`/`append`
//!   copy their handle arguments.
//! - **Plain model data** (`FontMetrics`, `FontNames`, `FontCharset`,
//!   `GlyphMetrics`, `Bounds`, …) crosses as objects with the Rust field
//!   names; reading such a getter returns a copy, assigning the edited object
//!   writes it back (validated). Glyph IDs are numbers; codepoints are
//!   returned as `{Unicode: n}` / `{Legacy: n}` and accepted as
//!   `CodepointLike` (a number or one-character string is Unicode, a
//!   `Codepoint`, or the plain data).
//! - **Handle-or-plain-data arguments** (`Font | FontDict`, `Glyph |
//!   GlyphDict`, `Bitmap | GlyphBitmap`, `Properties | Property[]`) are
//!   resolved by [`crate::model_args`] without consuming the handle.
//! - **Indices** are JS-style: non-negative exact integers (`font(-1)` throws
//!   `InvalidInputError`); `pop()` without an argument takes the last strike.
//! - **Callbacks.** `Font.editGlyph(id, fn)` calls `fn` with an owned copy and
//!   holds no borrow while it runs (Python's `edit_glyph` pattern), so the
//!   callback may use the font freely; the edited copy is committed through
//!   `Font::edit_glyph` when it returns, and nothing changes if it throws
//!   (the same exception object propagates).
use crate::conv::{self, Options};
use crate::errors::{IntoJs, JsResult, invalid};
use crate::handles::{
    self, FontLoc, GlyphLoc, JsBitmap, JsFont, JsFontSet, JsGlyph, read_font, write_font,
};
use crate::model_args::{
    Handle, PropsLoc, bitmap_arg, codepoint_arg, codepoints_arg, fonts_arg, glyph_arg, glyphs_arg,
    handle_arg, property_value_arg, sequence_arg, stash, wrong_handle,
};
use fontpixel::ResourceLimits;
use fontpixel::bitmap::Bounds;
use fontpixel::model::{
    Codepoint, Font, FontCharset, FontMetrics, FontNames, FontSet, Glyph, GlyphId, GlyphMetrics,
    ModelUsage, Properties, Property,
};
use js_sys::Array;
use wasm_bindgen::prelude::*;

/// The `GlyphId` with index `id`. `GlyphId` has no public constructor; its
/// serde form is the bare index, which is what plain data uses too.
pub fn glyph_id(id: usize) -> GlyphId {
    serde_json::from_value(serde_json::Value::from(id)).expect("GlyphId deserializes from an index")
}

/// `Some(id)` when `id` names a live glyph of `font`.
fn live(font: &Font, id: usize) -> Option<GlyphId> {
    let id = glyph_id(id);
    font.glyph(id).map(|_| id)
}

fn unknown_glyph() -> JsValue {
    invalid("unknown or removed glyph ID")
}

/// The live `GlyphId` for `id`, or `InvalidInputError`.
fn live_gid(font: &Font, id: usize) -> JsResult<GlyphId> {
    live(font, id).ok_or_else(unknown_glyph)
}

/// An optional `max_bytes` budget argument (default: unbounded).
fn max_bytes_arg(value: Option<f64>) -> JsResult<usize> {
    value.map_or(Ok(usize::MAX), |v| conv::int(v, "maxBytes"))
}

fn usage(
    max_bytes: Option<f64>,
    f: impl FnOnce(usize) -> fontpixel::Result<ModelUsage>,
) -> JsResult<JsValue> {
    let usage = f(max_bytes_arg(max_bytes)?).js()?;
    conv::to_js(&usage)
}

/// JS-style quoting of an optional string for `toString()`.
fn opt_text(value: Option<&str>) -> String {
    value.map_or_else(|| "null".to_owned(), |v| format!("{v:?}"))
}

/// A copy of a `Properties` handle or a `Property[]` plain-data list.
pub fn properties_arg(value: &JsValue) -> JsResult<Properties> {
    match handle_arg(value)? {
        Some(Handle::Properties(loc)) => JsProperties { loc }.read(|p| Ok(p.clone())),
        Some(other) => Err(wrong_handle("properties", "Properties", &other)),
        None => conv::from_js(value),
    }
}

fn opt_properties(value: Option<JsValue>) -> JsResult<Properties> {
    match value {
        Some(v) if !v.is_undefined() && !v.is_null() => properties_arg(&v),
        _ => Ok(Properties::default()),
    }
}

// ---------------------------------------------------------------- Codepoint

/// A tagged character code (`fontpixel::model::Codepoint`): a Unicode
/// number or a legacy code in the font's own charset. Every function taking
/// a codepoint also accepts a number or one-character string (Unicode) and
/// the plain data `{Unicode: n}` / `{Legacy: n}`; results use the plain data.
#[derive(Clone, Copy)]
#[wasm_bindgen(js_name = Codepoint)]
pub struct JsCodepoint(Codepoint);

impl JsCodepoint {
    /// Wrap a codepoint.
    pub fn new(cp: Codepoint) -> Self {
        Self(cp)
    }
    /// The wrapped codepoint.
    pub fn get(&self) -> Codepoint {
        self.0
    }
}

#[wasm_bindgen(js_class = Codepoint)]
impl JsCodepoint {
    /// `new Codepoint(value, kind = "unicode")`; `kind` is `"unicode"` or
    /// `"legacy"`.
    #[wasm_bindgen(constructor)]
    pub fn constructor(
        value: f64,
        #[wasm_bindgen(unchecked_optional_param_type = "\"unicode\" | \"legacy\"")] kind: Option<
            String,
        >,
    ) -> JsResult<JsCodepoint> {
        let value: u32 = conv::int(value, "value")?;
        match kind.as_deref() {
            None | Some("unicode") => Ok(Self(Codepoint::Unicode(value))),
            Some("legacy") => Ok(Self(Codepoint::Legacy(value))),
            Some(other) => Err(invalid(format!(
                "codepoint kind must be \"unicode\" or \"legacy\", not {other:?}"
            ))),
        }
    }
    /// A Unicode codepoint (`Codepoint::Unicode`).
    pub fn unicode(value: f64) -> JsResult<JsCodepoint> {
        Ok(Self(Codepoint::Unicode(conv::int(value, "value")?)))
    }
    /// A legacy charset code (`Codepoint::Legacy`).
    pub fn legacy(value: f64) -> JsResult<JsCodepoint> {
        Ok(Self(Codepoint::Legacy(conv::int(value, "value")?)))
    }
    /// The numeric value regardless of kind (`Codepoint::value`).
    #[wasm_bindgen(getter)]
    pub fn value(&self) -> u32 {
        self.0.value()
    }
    /// `"unicode"` or `"legacy"`.
    #[wasm_bindgen(getter, unchecked_return_type = "\"unicode\" | \"legacy\"")]
    pub fn kind(&self) -> String {
        match self.0 {
            Codepoint::Unicode(_) => "unicode",
            Codepoint::Legacy(_) => "legacy",
        }
        .to_owned()
    }
    /// Whether this is a Unicode codepoint.
    #[wasm_bindgen(getter, js_name = isUnicode)]
    pub fn is_unicode(&self) -> bool {
        matches!(self.0, Codepoint::Unicode(_))
    }
    /// The character of a Unicode scalar value, or `undefined` for legacy
    /// codes, surrogates and values above U+10FFFF (`Codepoint::scalar`).
    pub fn scalar(&self) -> Option<String> {
        self.0.scalar().map(String::from)
    }
    /// Plain data: `{Unicode: n}` or `{Legacy: n}`.
    #[wasm_bindgen(js_name = toDict, unchecked_return_type = "CodepointDict")]
    pub fn to_dict(&self) -> JsResult<JsValue> {
        conv::to_js(&self.0)
    }
    /// From the plain data (or any `CodepointLike`).
    #[wasm_bindgen(js_name = fromDict)]
    pub fn from_dict(
        #[wasm_bindgen(unchecked_param_type = "CodepointLike")] value: JsValue,
    ) -> JsResult<JsCodepoint> {
        codepoint_arg(&value).map(Self)
    }
    /// Whether `other` (any `CodepointLike`) is the same codepoint (kind and
    /// value); `false` for values that are not codepoints.
    pub fn equals(&self, #[wasm_bindgen(unchecked_param_type = "unknown")] other: JsValue) -> bool {
        codepoint_arg(&other).is_ok_and(|cp| cp == self.0)
    }
    /// `-1`, `0` or `1`: the Rust order, in which every Unicode code precedes
    /// every legacy code (use as an `Array.prototype.sort` comparator body).
    pub fn compare(
        &self,
        #[wasm_bindgen(unchecked_param_type = "CodepointLike")] other: JsValue,
    ) -> JsResult<i32> {
        let other = codepoint_arg(&other)?;
        Ok(match self.0.cmp(&other) {
            std::cmp::Ordering::Less => -1,
            std::cmp::Ordering::Equal => 0,
            std::cmp::Ordering::Greater => 1,
        })
    }
    /// `Codepoint.unicode(0x0041)`.
    #[wasm_bindgen(js_name = toString)]
    pub fn to_string_js(&self) -> String {
        format!("Codepoint.{}(0x{:04X})", self.kind(), self.0.value())
    }
    /// Internal: see `FontSet.__fpStash`.
    #[doc(hidden)]
    #[wasm_bindgen(js_name = __fpStash, skip_typescript)]
    pub fn fp_stash(&self) {
        stash(Handle::Codepoint(self.0));
    }
}

// ---------------------------------------------------------------- Properties

/// An ordered list of named properties (`fontpixel::model::Properties`).
///
/// From `FontSet.properties`, `Font.properties`, `Font.headers`,
/// `Font.localizedNames` or `Glyph.properties` it is a live view: `set`,
/// `push` and `remove` edit the owning model in place. `new Properties(items)`
/// builds a standalone list; `copy()` detaches a view.
///
/// Values (`PropertyValueLike`): `null` → `Flag`; a safe-integer number or a
/// bigint → `Integer` (`Unsigned` above the `i64` range); a string → `Text`;
/// bytes → `Bytes`; an array → `List`; any other object is the tagged plain
/// data (`{Decimal: "1.5"}`). Values are returned in the tagged form, which
/// keeps every variant (`"Flag"`, `{Integer: n}`, …).
#[wasm_bindgen(js_name = Properties)]
pub struct JsProperties {
    pub(crate) loc: PropsLoc,
}

impl JsProperties {
    /// Wrap an owned list.
    pub fn owned(props: Properties) -> Self {
        Self {
            loc: PropsLoc::Owned(handles::shared(props)),
        }
    }
    /// A view at `loc`.
    pub fn view(loc: PropsLoc) -> Self {
        Self { loc }
    }
    /// Run `f` with shared access.
    pub fn read<R>(&self, f: impl FnOnce(&Properties) -> JsResult<R>) -> JsResult<R> {
        match &self.loc {
            PropsLoc::Owned(p) => f(&*handles::borrow(p, "Properties")?),
            PropsLoc::Set(s) => f(&handles::borrow(s, "FontSet")?.properties),
            PropsLoc::Font(l) => read_font(l, |font| f(&font.properties)),
            PropsLoc::Headers(l) => read_font(l, |font| f(&font.headers)),
            PropsLoc::Localized(l) => read_font(l, |font| f(&font.names.localized)),
            PropsLoc::Glyph(l) => JsGlyph { loc: l.clone() }.read(|g| f(&g.properties)),
        }
    }
    /// Run `f` with exclusive access.
    pub fn write<R>(&self, f: impl FnOnce(&mut Properties) -> JsResult<R>) -> JsResult<R> {
        match &self.loc {
            PropsLoc::Owned(p) => f(&mut *handles::borrow_mut(p, "Properties")?),
            PropsLoc::Set(s) => f(&mut handles::borrow_mut(s, "FontSet")?.properties),
            PropsLoc::Font(l) => write_font(l, |font| f(&mut font.properties)),
            PropsLoc::Headers(l) => write_font(l, |font| f(&mut font.headers)),
            PropsLoc::Localized(l) => write_font(l, |font| f(&mut font.names.localized)),
            PropsLoc::Glyph(l) => JsGlyph { loc: l.clone() }.write(|g| f(&mut g.properties)),
        }
    }
}

#[wasm_bindgen(js_class = Properties)]
impl JsProperties {
    /// A standalone list from another `Properties` or `Property[]` plain data.
    #[wasm_bindgen(constructor)]
    pub fn constructor(
        #[wasm_bindgen(unchecked_optional_param_type = "Properties | PropertiesDict")]
        items: Option<JsValue>,
    ) -> JsResult<JsProperties> {
        Ok(Self::owned(opt_properties(items)?))
    }
    /// Number of records, including repeated names (`Properties::len`).
    #[wasm_bindgen(getter)]
    pub fn length(&self) -> JsResult<usize> {
        self.read(|p| Ok(p.len()))
    }
    /// Whether the list is empty (`Properties::is_empty`).
    #[wasm_bindgen(js_name = isEmpty)]
    pub fn is_empty(&self) -> JsResult<bool> {
        self.read(|p| Ok(p.is_empty()))
    }
    /// The record at `index` (negative counts from the end, as
    /// `Array.prototype.at`), or `undefined`.
    #[wasm_bindgen(unchecked_return_type = "Property | undefined")]
    pub fn at(&self, index: f64) -> JsResult<JsValue> {
        let index: i64 = conv::int(index, "index")?;
        self.read(|p| {
            let len = p.len() as i64;
            let i = if index < 0 { index + len } else { index };
            match usize::try_from(i).ok().and_then(|i| p.iter().nth(i)) {
                Some(prop) => conv::to_js(prop),
                None => Ok(JsValue::UNDEFINED),
            }
        })
    }
    /// The value of the last property named exactly `name` (tagged form), or
    /// `undefined` (`Properties::get`).
    #[wasm_bindgen(unchecked_return_type = "PropertyValue | undefined")]
    pub fn get(&self, name: &str) -> JsResult<JsValue> {
        self.read(|p| p.get(name).map_or(Ok(JsValue::UNDEFINED), conv::to_js))
    }
    /// Like `get`, comparing names ASCII-case-insensitively
    /// (`Properties::get_case_insensitive`).
    #[wasm_bindgen(js_name = getCaseInsensitive, unchecked_return_type = "PropertyValue | undefined")]
    pub fn get_case_insensitive(&self, name: &str) -> JsResult<JsValue> {
        self.read(|p| {
            p.get_case_insensitive(name)
                .map_or(Ok(JsValue::UNDEFINED), conv::to_js)
        })
    }
    /// Replace the last property whose name matches case-insensitively
    /// (keeping its spelling and position), or append (`Properties::set`).
    pub fn set(
        &self,
        name: String,
        #[wasm_bindgen(unchecked_param_type = "PropertyValueLike")] value: JsValue,
    ) -> JsResult<()> {
        let value = property_value_arg(&value)?;
        self.write(|p| {
            p.set(name, value);
            Ok(())
        })
    }
    /// Append a property, keeping duplicates (`Properties::push` with
    /// `Property::new`); `raw` is the optional source spelling.
    pub fn push(
        &self,
        name: String,
        #[wasm_bindgen(unchecked_param_type = "PropertyValueLike")] value: JsValue,
        #[wasm_bindgen(unchecked_optional_param_type = "BytesLike")] raw: Option<JsValue>,
    ) -> JsResult<()> {
        let mut property = Property::new(name, property_value_arg(&value)?);
        property.raw = match raw {
            Some(r) if !r.is_undefined() && !r.is_null() => Some(conv::bytes_from_js(&r, "raw")?),
            _ => None,
        };
        self.write(|p| {
            p.push(property);
            Ok(())
        })
    }
    /// Remove every property whose name matches case-insensitively; returns
    /// how many were removed (`Properties::remove`).
    pub fn remove(&self, name: &str) -> JsResult<usize> {
        self.write(|p| Ok(p.remove(name)))
    }
    /// The records as plain data (`Properties::iter` / `into_vec`; the list
    /// stays usable).
    #[wasm_bindgen(js_name = toList, unchecked_return_type = "Property[]")]
    pub fn to_list(&self) -> JsResult<JsValue> {
        self.read(conv::to_js)
    }
    /// A standalone copy.
    pub fn copy(&self) -> JsResult<JsProperties> {
        self.read(|p| Ok(Self::owned(p.clone())))
    }
    /// Whether `other` (a `Properties` or `Property[]`) holds the same records.
    pub fn equals(
        &self,
        #[wasm_bindgen(unchecked_param_type = "unknown")] other: JsValue,
    ) -> JsResult<bool> {
        let Ok(other) = properties_arg(&other) else {
            return Ok(false);
        };
        self.read(|p| Ok(*p == other))
    }
    /// `Properties(["NAME", …])`.
    #[wasm_bindgen(js_name = toString)]
    pub fn to_string_js(&self) -> JsResult<String> {
        self.read(|p| {
            let names: Vec<_> = p.iter().map(|q| q.name.as_str()).collect();
            Ok(format!("Properties({names:?})"))
        })
    }
    /// Internal: see `FontSet.__fpStash`.
    #[doc(hidden)]
    #[wasm_bindgen(js_name = __fpStash, skip_typescript)]
    pub fn fp_stash(&self) {
        stash(Handle::Properties(self.loc.clone()));
    }
}

// ---------------------------------------------------------------- FontSet

#[wasm_bindgen(js_class = FontSet)]
impl JsFontSet {
    /// A set of strikes: copies of `fonts` (`Font` handles or plain data),
    /// with optional `properties` and `sourceFormat`.
    #[wasm_bindgen(constructor)]
    pub fn constructor(
        #[wasm_bindgen(unchecked_optional_param_type = "(Font | FontDict)[]")] fonts: Option<
            JsValue,
        >,
        #[wasm_bindgen(unchecked_optional_param_type = "FontSetArgs")] options: Option<JsValue>,
    ) -> JsResult<JsFontSet> {
        let options = Options::new(
            &options.unwrap_or(JsValue::UNDEFINED),
            &["properties", "sourceFormat"],
            "FontSet options",
        )?;
        let fonts = match fonts {
            Some(f) if !f.is_undefined() && !f.is_null() => fonts_arg(&f)?,
            _ => Vec::new(),
        };
        Ok(JsFontSet::new(FontSet {
            fonts,
            properties: opt_properties(options.get("properties")?)?,
            source_format: options.string("sourceFormat")?,
        }))
    }
    /// Build from the plain-data form produced by `toDict()`.
    #[wasm_bindgen(js_name = fromDict)]
    pub fn from_dict(
        #[wasm_bindgen(unchecked_param_type = "FontSetDict")] data: JsValue,
    ) -> JsResult<JsFontSet> {
        Ok(JsFontSet::new(conv::from_js(&data)?))
    }
    /// The complete plain-data form; copies every pixel.
    #[wasm_bindgen(js_name = toDict, unchecked_return_type = "FontSetDict")]
    pub fn to_dict(&self) -> JsResult<JsValue> {
        self.read(conv::to_js)
    }
    /// Number of strikes.
    #[wasm_bindgen(getter)]
    pub fn length(&self) -> JsResult<usize> {
        self.read(|s| Ok(s.fonts.len()))
    }
    /// Live views of every strike, in order. Assigning an array of fonts
    /// (handles or plain data) replaces them with copies.
    #[wasm_bindgen(getter)]
    pub fn fonts(&self) -> JsResult<Vec<JsFont>> {
        let n = self.read(|s| Ok(s.fonts.len()))?;
        Ok((0..n).map(|i| JsFont::view(self.storage(), i)).collect())
    }
    /// Replace every strike with copies of `fonts`.
    #[wasm_bindgen(setter)]
    pub fn set_fonts(
        &self,
        #[wasm_bindgen(unchecked_param_type = "(Font | FontDict)[]")] fonts: JsValue,
    ) -> JsResult<()> {
        let fonts = fonts_arg(&fonts)?;
        self.write(|s| {
            s.fonts = fonts;
            Ok(())
        })
    }
    /// A live view of strike `index`; edits through it change this set.
    pub fn font(&self, index: f64) -> JsResult<JsFont> {
        let index: usize = conv::int(index, "index")?;
        let len = self.read(|s| Ok(s.fonts.len()))?;
        if index >= len {
            return Err(invalid(format!(
                "font index {index} out of range for {len} strikes"
            )));
        }
        Ok(JsFont::view(self.storage(), index))
    }
    /// Append a copy of `font` (a handle or plain data); returns its index.
    pub fn append(
        &self,
        #[wasm_bindgen(unchecked_param_type = "Font | FontDict")] font: JsValue,
    ) -> JsResult<usize> {
        let font = crate::model_args::font_arg(&font)?;
        self.write(|s| {
            s.fonts.push(font);
            Ok(s.fonts.len() - 1)
        })
    }
    /// Remove and return strike `index` (default: the last) as an owned
    /// font. Views of later strikes now address the following font.
    pub fn pop(&self, index: Option<f64>) -> JsResult<JsFont> {
        self.write(|s| {
            let len = s.fonts.len();
            let index = match index {
                Some(i) => conv::int::<usize>(i, "index")?,
                None => len
                    .checked_sub(1)
                    .ok_or_else(|| invalid("pop from an empty FontSet"))?,
            };
            if index >= len {
                return Err(invalid(format!(
                    "font index {index} out of range for {len} strikes"
                )));
            }
            Ok(JsFont::new(s.fonts.remove(index)))
        })
    }
    /// A view of the only strike; `InvalidInputError` unless there is exactly
    /// one (`FontSet::single`).
    pub fn single(&self) -> JsResult<JsFont> {
        self.read(|s| s.single().map(|_| ()).js())?;
        Ok(JsFont::view(self.storage(), 0))
    }
    /// An owned copy of the only strike (`FontSet::into_single`; the set is
    /// kept, since JS cannot consume the handle).
    #[wasm_bindgen(js_name = intoSingle)]
    pub fn into_single(&self) -> JsResult<JsFont> {
        let set = self.snapshot()?;
        Ok(JsFont::new(set.into_single().js()?))
    }
    /// Check glyph counts, pixel and byte budgets of every strike
    /// (`FontSet::validate`); throws `ResourceLimitError`/`InvalidInputError`.
    pub fn validate(
        &self,
        #[wasm_bindgen(unchecked_optional_param_type = "ResourceLimits")] limits: Option<JsValue>,
    ) -> JsResult<()> {
        let limits: ResourceLimits =
            conv::from_js_or_default(&limits.unwrap_or(JsValue::UNDEFINED))?;
        self.read(|s| s.validate(&limits).js())
    }
    /// Conservative retained storage of the set (`FontSet::retained_usage`):
    /// `{bytes, fonts, glyphs, glyph_slots}`; `ResourceLimitError` above
    /// `maxBytes` (default unbounded). Sizes are those of this wasm32 build.
    #[wasm_bindgen(js_name = retainedUsage, unchecked_return_type = "ModelUsage")]
    pub fn retained_usage(
        &self,
        #[wasm_bindgen(js_name = maxBytes)] max_bytes: Option<f64>,
    ) -> JsResult<JsValue> {
        self.read(|s| usage(max_bytes, |m| s.retained_usage(m)))
    }
    /// Set-level properties (a live `Properties` view); assignable.
    #[wasm_bindgen(getter)]
    pub fn properties(&self) -> JsProperties {
        JsProperties::view(PropsLoc::Set(self.storage().clone()))
    }
    /// Replace the set-level properties with a copy of `value`.
    #[wasm_bindgen(setter)]
    pub fn set_properties(
        &self,
        #[wasm_bindgen(unchecked_param_type = "Properties | PropertiesDict")] value: JsValue,
    ) -> JsResult<()> {
        let props = properties_arg(&value)?;
        self.write(|s| {
            s.properties = props;
            Ok(())
        })
    }
    /// The format ID the set was read from, if any.
    #[wasm_bindgen(getter, js_name = sourceFormat)]
    pub fn source_format(&self) -> JsResult<Option<String>> {
        self.read(|s| Ok(s.source_format.clone()))
    }
    /// Set or clear (`null`/`undefined`) the source format ID.
    #[wasm_bindgen(setter, js_name = sourceFormat)]
    pub fn set_source_format(&self, value: Option<String>) -> JsResult<()> {
        self.write(|s| {
            s.source_format = value;
            Ok(())
        })
    }
    /// An independent deep copy.
    pub fn copy(&self) -> JsResult<JsFontSet> {
        Ok(JsFontSet::new(self.snapshot()?))
    }
    /// Whether `other` is a `FontSet` with equal contents.
    pub fn equals(
        &self,
        #[wasm_bindgen(unchecked_param_type = "unknown")] other: JsValue,
    ) -> JsResult<bool> {
        let other = match handle_arg(&other)? {
            Some(Handle::FontSet(set)) => JsFontSet { set }.snapshot()?,
            _ => return Ok(false),
        };
        self.read(|s| Ok(*s == other))
    }
    /// `FontSet(fonts=2, sourceFormat="bdf")`.
    #[wasm_bindgen(js_name = toString)]
    pub fn to_string_js(&self) -> JsResult<String> {
        self.read(|s| {
            Ok(format!(
                "FontSet(fonts={}, sourceFormat={})",
                s.fonts.len(),
                opt_text(s.source_format.as_deref())
            ))
        })
    }
}

// ---------------------------------------------------------------- Font

impl JsFont {
    fn glyph_view(&self, id: GlyphId) -> JsGlyph {
        JsGlyph::view(&self.loc, id)
    }
    fn ids(&self) -> JsResult<Vec<GlyphId>> {
        self.read(|f| Ok(f.glyphs().map(|(id, _)| id).collect()))
    }
}

#[wasm_bindgen(js_class = Font)]
impl JsFont {
    /// A strike built from copies of `glyphs` (`Font::from_glyphs`; IDs are
    /// assigned in order from 0). Without arguments: an empty strike.
    #[wasm_bindgen(constructor)]
    pub fn constructor(
        #[wasm_bindgen(unchecked_optional_param_type = "(Glyph | GlyphDict)[]")] glyphs: Option<
            JsValue,
        >,
    ) -> JsResult<JsFont> {
        let glyphs = match glyphs {
            Some(g) if !g.is_undefined() && !g.is_null() => glyphs_arg(&g)?,
            _ => Vec::new(),
        };
        Ok(JsFont::new(Font::from_glyphs(glyphs).js()?))
    }
    /// Build from the plain-data form produced by `toDict()` (removed glyph
    /// slots stay `null`, so IDs survive).
    #[wasm_bindgen(js_name = fromDict)]
    pub fn from_dict(
        #[wasm_bindgen(unchecked_param_type = "FontDict")] data: JsValue,
    ) -> JsResult<JsFont> {
        Ok(JsFont::new(conv::from_js(&data)?))
    }
    /// The complete plain-data form; copies every pixel.
    #[wasm_bindgen(js_name = toDict, unchecked_return_type = "FontDict")]
    pub fn to_dict(&self) -> JsResult<JsValue> {
        self.read(conv::to_js)
    }
    /// Whether this handle is a view into a `FontSet`.
    #[wasm_bindgen(getter, js_name = isView)]
    pub fn is_view(&self) -> bool {
        matches!(self.loc, FontLoc::InSet(..))
    }
    /// Number of live glyphs (`Font::len`).
    #[wasm_bindgen(getter)]
    pub fn length(&self) -> JsResult<usize> {
        self.read(|f| Ok(f.len()))
    }
    /// Number of assigned glyph IDs, including removed slots
    /// (`Font::slot_count`).
    #[wasm_bindgen(getter, js_name = slotCount)]
    pub fn slot_count(&self) -> JsResult<usize> {
        self.read(|f| Ok(f.slot_count()))
    }
    /// Whether the font has no live glyphs (`Font::is_empty`).
    #[wasm_bindgen(js_name = isEmpty)]
    pub fn is_empty(&self) -> JsResult<bool> {
        self.read(|f| Ok(f.is_empty()))
    }
    /// IDs of every live glyph in order.
    #[wasm_bindgen(js_name = glyphIds, unchecked_return_type = "number[]")]
    pub fn glyph_ids(&self) -> JsResult<Array> {
        Ok(self
            .ids()?
            .into_iter()
            .map(|id| JsValue::from_f64(id.index() as f64))
            .collect())
    }
    /// `[id, glyph view]` pairs of every live glyph in ID order
    /// (`Font::glyphs`).
    #[wasm_bindgen(unchecked_return_type = "[number, Glyph][]")]
    pub fn glyphs(&self) -> JsResult<Array> {
        Ok(self
            .ids()?
            .into_iter()
            .map(|id| {
                let pair = Array::new();
                pair.push(&JsValue::from_f64(id.index() as f64));
                pair.push(&JsValue::from(self.glyph_view(id)));
                JsValue::from(pair)
            })
            .collect())
    }
    /// A live view of glyph `id`, or `undefined` (`Font::glyph`).
    pub fn glyph(&self, id: f64) -> JsResult<Option<JsGlyph>> {
        let id: usize = conv::int(id, "id")?;
        let id = self.read(|f| Ok(live(f, id)))?;
        Ok(id.map(|id| self.glyph_view(id)))
    }
    /// The effective glyph ID of a codepoint, or `undefined`
    /// (`Font::glyph_id`).
    #[wasm_bindgen(js_name = glyphId)]
    pub fn glyph_id(
        &self,
        #[wasm_bindgen(unchecked_param_type = "CodepointLike")] codepoint: JsValue,
    ) -> JsResult<Option<usize>> {
        let cp = codepoint_arg(&codepoint)?;
        self.read(|f| Ok(f.glyph_id(cp).map(GlyphId::index)))
    }
    /// The effective glyph of a codepoint as a view, or `undefined`
    /// (`Font::glyph_for_codepoint`).
    #[wasm_bindgen(js_name = glyphForCodepoint)]
    pub fn glyph_for_codepoint(
        &self,
        #[wasm_bindgen(unchecked_param_type = "CodepointLike")] codepoint: JsValue,
    ) -> JsResult<Option<JsGlyph>> {
        let cp = codepoint_arg(&codepoint)?;
        let id = self.read(|f| Ok(f.glyph_for_codepoint(cp).and_then(|_| f.glyph_id(cp))))?;
        Ok(id.map(|id| self.glyph_view(id)))
    }
    /// Every glyph ID mapped to a codepoint, in file order; the last is
    /// effective (`Font::glyph_ids_for_codepoint`).
    #[wasm_bindgen(js_name = glyphIdsForCodepoint, unchecked_return_type = "number[]")]
    pub fn glyph_ids_for_codepoint(
        &self,
        #[wasm_bindgen(unchecked_param_type = "CodepointLike")] codepoint: JsValue,
    ) -> JsResult<Array> {
        let cp = codepoint_arg(&codepoint)?;
        self.read(|f| {
            Ok(f.glyph_ids_for_codepoint(cp)
                .iter()
                .map(|id| JsValue::from_f64(id.index() as f64))
                .collect())
        })
    }
    /// The effective glyph of a Unicode sequence (a string or an array of
    /// numbers) as a view, or `undefined` (`Font::glyph_for_sequence`).
    #[wasm_bindgen(js_name = glyphForSequence)]
    pub fn glyph_for_sequence(
        &self,
        #[wasm_bindgen(unchecked_param_type = "string | number[]")] sequence: JsValue,
    ) -> JsResult<Option<JsGlyph>> {
        let seq = sequence_arg(&sequence)?;
        let id = self.read(|f| {
            let Some(found) = f.glyph_for_sequence(&seq) else {
                return Ok(None);
            };
            Ok(f.glyphs()
                .find(|(_, g)| std::ptr::eq(*g, found))
                .map(|(id, _)| id))
        })?;
        Ok(id.map(|id| self.glyph_view(id)))
    }
    /// `[codepoint, id]` pairs of every effective mapping in codepoint order
    /// (`Font::codepoints`; Unicode codes precede legacy codes).
    #[wasm_bindgen(unchecked_return_type = "[CodepointDict, number][]")]
    pub fn codepoints(&self) -> JsResult<JsValue> {
        self.read(|f| {
            let pairs: Vec<(Codepoint, usize)> =
                f.codepoints().map(|(cp, id)| (cp, id.index())).collect();
            conv::to_js(&pairs)
        })
    }
    /// Effective mappings with `start <= codepoint <= end` (inclusive, in
    /// codepoint order; empty when `start > end`) (`Font::codepoint_range`).
    #[wasm_bindgen(js_name = codepointRange, unchecked_return_type = "[CodepointDict, number][]")]
    pub fn codepoint_range(
        &self,
        #[wasm_bindgen(unchecked_param_type = "CodepointLike")] start: JsValue,
        #[wasm_bindgen(unchecked_param_type = "CodepointLike")] end: JsValue,
    ) -> JsResult<JsValue> {
        let (start, end) = (codepoint_arg(&start)?, codepoint_arg(&end)?);
        self.read(|f| {
            let pairs: Vec<(Codepoint, usize)> = f
                .codepoint_range(start, end)
                .map(|(cp, id)| (cp, id.index()))
                .collect();
            conv::to_js(&pairs)
        })
    }
    /// Add a copy of `glyph` (a handle or plain data) after validating it;
    /// returns its new ID (`Font::add_glyph`).
    #[wasm_bindgen(js_name = addGlyph)]
    pub fn add_glyph(
        &self,
        #[wasm_bindgen(unchecked_param_type = "Glyph | GlyphDict")] glyph: JsValue,
    ) -> JsResult<usize> {
        let glyph = glyph_arg(&glyph)?;
        self.write(|f| f.add_glyph(glyph).js().map(GlyphId::index))
    }
    /// Replace glyph `id` with a copy of `glyph`, re-indexing its codes;
    /// returns the old glyph as an owned handle (`Font::replace_glyph`).
    #[wasm_bindgen(js_name = replaceGlyph)]
    pub fn replace_glyph(
        &self,
        id: f64,
        #[wasm_bindgen(unchecked_param_type = "Glyph | GlyphDict")] glyph: JsValue,
    ) -> JsResult<JsGlyph> {
        let id: usize = conv::int(id, "id")?;
        let glyph = glyph_arg(&glyph)?;
        self.write(|f| {
            let id = live_gid(f, id)?;
            f.replace_glyph(id, glyph).js().map(JsGlyph::new)
        })
    }
    /// Remove glyph `id`; returns it as an owned handle, or `undefined`.
    /// Its ID is never reused (`Font::remove_glyph`).
    #[wasm_bindgen(js_name = removeGlyph)]
    pub fn remove_glyph(&self, id: f64) -> JsResult<Option<JsGlyph>> {
        let id: usize = conv::int(id, "id")?;
        self.write(|f| {
            Ok(live(f, id)
                .and_then(|id| f.remove_glyph(id))
                .map(JsGlyph::new))
        })
    }
    /// Edit glyph `id` through `edit(glyph)`, which receives an owned copy
    /// (no borrow is held while it runs). The edited copy replaces the glyph
    /// when `edit` returns, validated by `Font::edit_glyph`; if `edit` throws,
    /// nothing changes and the exception propagates unchanged.
    #[wasm_bindgen(js_name = editGlyph)]
    pub fn edit_glyph(
        &self,
        id: f64,
        #[wasm_bindgen(unchecked_param_type = "(glyph: Glyph) => unknown")] edit: js_sys::Function,
    ) -> JsResult<()> {
        let id: usize = conv::int(id, "id")?;
        let original = self.read(|f| Ok(f.glyph(live_gid(f, id)?).cloned()))?;
        let copy = JsGlyph::new(original.ok_or_else(unknown_glyph)?);
        let GlyphLoc::Owned(storage) = copy.loc.clone() else {
            unreachable!("JsGlyph::new owns its glyph")
        };
        edit.call1(&JsValue::UNDEFINED, &JsValue::from(copy))?;
        let edited = handles::borrow(&storage, "Glyph")?.clone();
        self.write(|f| {
            let id = live_gid(f, id)?;
            f.edit_glyph(id, |g| {
                *g = edited;
                Ok(())
            })
            .js()
        })
    }
    /// Glyph `id` cropped or extended to the font's cell bounds
    /// (`Font::glyph_in_cell`).
    #[wasm_bindgen(js_name = glyphInCell)]
    pub fn glyph_in_cell(&self, id: f64) -> JsResult<JsBitmap> {
        let id: usize = conv::int(id, "id")?;
        self.read(|f| f.glyph_in_cell(glyph_id(id)).js().map(JsBitmap::new))
    }
    /// Set `metrics.bounds` to the union of all inked glyph boxes
    /// (`Font::update_bounds`).
    #[wasm_bindgen(js_name = updateBounds)]
    pub fn update_bounds(&self) -> JsResult<()> {
        self.write(|f| f.update_bounds().js())
    }
    /// Validate glyphs, the default glyph and resource budgets
    /// (`Font::validate`).
    pub fn validate(
        &self,
        #[wasm_bindgen(unchecked_optional_param_type = "ResourceLimits")] limits: Option<JsValue>,
    ) -> JsResult<()> {
        let limits: ResourceLimits =
            conv::from_js_or_default(&limits.unwrap_or(JsValue::UNDEFINED))?;
        self.read(|f| f.validate(&limits).js())
    }
    /// Conservative retained storage of this strike (`Font::retained_usage`);
    /// `ResourceLimitError` above `maxBytes` (inclusive budget, default
    /// unbounded).
    #[wasm_bindgen(js_name = retainedUsage, unchecked_return_type = "ModelUsage")]
    pub fn retained_usage(
        &self,
        #[wasm_bindgen(js_name = maxBytes)] max_bytes: Option<f64>,
    ) -> JsResult<JsValue> {
        self.read(|f| usage(max_bytes, |m| f.retained_usage(m)))
    }
    /// Storage a new owned copy would need, without copying
    /// (`Font::cloned_usage`).
    #[wasm_bindgen(js_name = clonedUsage, unchecked_return_type = "ModelUsage")]
    pub fn cloned_usage(
        &self,
        #[wasm_bindgen(js_name = maxBytes)] max_bytes: Option<f64>,
    ) -> JsResult<JsValue> {
        self.read(|f| usage(max_bytes, |m| f.cloned_usage(m)))
    }
    /// Font-wide metrics (`FontMetrics`) as a copy; assign to replace.
    #[wasm_bindgen(getter, unchecked_return_type = "FontMetrics")]
    pub fn metrics(&self) -> JsResult<JsValue> {
        self.read(|f| conv::to_js(&f.metrics))
    }
    /// Replace the metrics; a `default_glyph` must name a live glyph.
    #[wasm_bindgen(setter)]
    pub fn set_metrics(
        &self,
        #[wasm_bindgen(unchecked_param_type = "FontMetrics")] value: JsValue,
    ) -> JsResult<()> {
        let metrics: FontMetrics = conv::from_js(&value)?;
        self.write(|f| {
            if metrics
                .default_glyph
                .is_some_and(|id| f.glyph(id).is_none())
            {
                return Err(invalid(
                    "default glyph refers to a removed or nonexistent glyph",
                ));
            }
            f.metrics = metrics;
            Ok(())
        })
    }
    /// Font names (`FontNames`) as a copy; assign to replace.
    #[wasm_bindgen(getter, unchecked_return_type = "FontNames")]
    pub fn names(&self) -> JsResult<JsValue> {
        self.read(|f| conv::to_js(&f.names))
    }
    /// Replace the names.
    #[wasm_bindgen(setter)]
    pub fn set_names(
        &self,
        #[wasm_bindgen(unchecked_param_type = "FontNames")] value: JsValue,
    ) -> JsResult<()> {
        let names: FontNames = conv::from_js(&value)?;
        self.write(|f| {
            f.names = names;
            Ok(())
        })
    }
    /// A live view of `names.localized`.
    #[wasm_bindgen(getter, js_name = localizedNames)]
    pub fn localized_names(&self) -> JsProperties {
        JsProperties::view(PropsLoc::Localized(self.loc.clone()))
    }
    /// Charset identification (`FontCharset`) as a copy; assign to replace.
    #[wasm_bindgen(getter, unchecked_return_type = "FontCharset")]
    pub fn charset(&self) -> JsResult<JsValue> {
        self.read(|f| conv::to_js(&f.charset))
    }
    /// Replace the charset identification.
    #[wasm_bindgen(setter)]
    pub fn set_charset(
        &self,
        #[wasm_bindgen(unchecked_param_type = "FontCharset")] value: JsValue,
    ) -> JsResult<()> {
        let charset: FontCharset = conv::from_js(&value)?;
        self.write(|f| {
            f.charset = charset;
            Ok(())
        })
    }
    /// The canonical charset name (`FontCharset::canonical_name`).
    #[wasm_bindgen(js_name = charsetCanonicalName)]
    pub fn charset_canonical_name(&self) -> JsResult<String> {
        self.read(|f| Ok(f.charset.canonical_name()))
    }
    /// The format ID this strike was read from, if any.
    #[wasm_bindgen(getter, js_name = sourceFormat)]
    pub fn source_format(&self) -> JsResult<Option<String>> {
        self.read(|f| Ok(f.source_format.clone()))
    }
    /// Set or clear (`null`/`undefined`) the source format ID.
    #[wasm_bindgen(setter, js_name = sourceFormat)]
    pub fn set_source_format(&self, value: Option<String>) -> JsResult<()> {
        self.write(|f| {
            f.source_format = value;
            Ok(())
        })
    }
    /// Format header records (a live `Properties` view); assignable.
    #[wasm_bindgen(getter)]
    pub fn headers(&self) -> JsProperties {
        JsProperties::view(PropsLoc::Headers(self.loc.clone()))
    }
    /// Replace the header records with a copy of `value`.
    #[wasm_bindgen(setter)]
    pub fn set_headers(
        &self,
        #[wasm_bindgen(unchecked_param_type = "Properties | PropertiesDict")] value: JsValue,
    ) -> JsResult<()> {
        let props = properties_arg(&value)?;
        self.write(|f| {
            f.headers = props;
            Ok(())
        })
    }
    /// Font properties (a live `Properties` view); assignable.
    #[wasm_bindgen(getter)]
    pub fn properties(&self) -> JsProperties {
        JsProperties::view(PropsLoc::Font(self.loc.clone()))
    }
    /// Replace the font properties with a copy of `value`.
    #[wasm_bindgen(setter)]
    pub fn set_properties(
        &self,
        #[wasm_bindgen(unchecked_param_type = "Properties | PropertiesDict")] value: JsValue,
    ) -> JsResult<()> {
        let props = properties_arg(&value)?;
        self.write(|f| {
            f.properties = props;
            Ok(())
        })
    }
    /// An independent owned deep copy (also of a view).
    pub fn copy(&self) -> JsResult<JsFont> {
        Ok(JsFont::new(self.snapshot()?))
    }
    /// Whether `other` is a `Font` with equal contents.
    pub fn equals(
        &self,
        #[wasm_bindgen(unchecked_param_type = "unknown")] other: JsValue,
    ) -> JsResult<bool> {
        let other = match handle_arg(&other)? {
            Some(Handle::Font(loc)) => JsFont { loc }.snapshot()?,
            _ => return Ok(false),
        };
        self.read(|f| Ok(*f == other))
    }
    /// `Font(glyphs=2, family="Demo", view=true)`.
    #[wasm_bindgen(js_name = toString)]
    pub fn to_string_js(&self) -> JsResult<String> {
        let view = self.is_view();
        self.read(|f| {
            Ok(format!(
                "Font(glyphs={}, family={}, view={view})",
                f.len(),
                opt_text(f.names.family.as_deref())
            ))
        })
    }
}

// ---------------------------------------------------------------- Glyph

/// An optional 64-bit offset argument (number or bigint; default 0).
fn offset_arg(value: Option<JsValue>, what: &str) -> JsResult<i64> {
    match value {
        Some(v) if !v.is_undefined() && !v.is_null() => conv::int_from_js(&v, what),
        _ => Ok(0),
    }
}

#[wasm_bindgen(js_class = Glyph)]
impl JsGlyph {
    /// A glyph holding a copy of `bitmap` (a `Bitmap` or `GlyphBitmap` plain
    /// data; default an empty raster) whose bounds match the bitmap at offset
    /// (0, 0) (`Glyph::new`). `options.codepoints` lists `CodepointLike`
    /// values; `options.name` sets the glyph name.
    #[wasm_bindgen(constructor)]
    pub fn constructor(
        #[wasm_bindgen(unchecked_optional_param_type = "Bitmap | GlyphBitmap")] bitmap: Option<
            JsValue,
        >,
        #[wasm_bindgen(unchecked_optional_param_type = "GlyphArgs")] options: Option<JsValue>,
    ) -> JsResult<JsGlyph> {
        let options = Options::new(
            &options.unwrap_or(JsValue::UNDEFINED),
            &["codepoints", "name"],
            "Glyph options",
        )?;
        let bitmap = match bitmap {
            Some(b) if !b.is_undefined() && !b.is_null() => bitmap_arg(&b)?,
            _ => Default::default(),
        };
        let mut glyph = Glyph::new(bitmap);
        if let Some(list) = options.get("codepoints")? {
            glyph.codepoints = codepoints_arg(&list)?;
        }
        glyph.name = options.string("name")?;
        Ok(JsGlyph::new(glyph))
    }
    /// Build from the plain-data form produced by `toDict()`.
    #[wasm_bindgen(js_name = fromDict)]
    pub fn from_dict(
        #[wasm_bindgen(unchecked_param_type = "GlyphDict")] data: JsValue,
    ) -> JsResult<JsGlyph> {
        Ok(JsGlyph::new(conv::from_js(&data)?))
    }
    /// The complete plain-data form; copies the pixels.
    #[wasm_bindgen(js_name = toDict, unchecked_return_type = "GlyphDict")]
    pub fn to_dict(&self) -> JsResult<JsValue> {
        self.read(conv::to_js)
    }
    /// Whether this handle is a view into a font.
    #[wasm_bindgen(getter, js_name = isView)]
    pub fn is_view(&self) -> bool {
        matches!(self.loc, GlyphLoc::InFont(..))
    }
    /// The glyph ID of a view, or `undefined` for an owned glyph.
    #[wasm_bindgen(getter)]
    pub fn id(&self) -> Option<usize> {
        match &self.loc {
            GlyphLoc::InFont(_, id) => Some(id.index()),
            GlyphLoc::Owned(_) => None,
        }
    }
    /// A copy of the raster. Assigning a `Bitmap` (or plain data) replaces
    /// the pixels and resizes `metrics.bounds`, keeping its offset.
    #[wasm_bindgen(getter)]
    pub fn bitmap(&self) -> JsResult<JsBitmap> {
        self.read(|g| Ok(JsBitmap::new(g.bitmap.clone())))
    }
    /// Replace the raster with a copy of `bitmap` at the current offset
    /// (`Glyph::set_bitmap(bitmap, bounds.x, bounds.y, false)`).
    #[wasm_bindgen(setter)]
    pub fn set_bitmap(
        &self,
        #[wasm_bindgen(unchecked_param_type = "Bitmap | GlyphBitmap")] bitmap: JsValue,
    ) -> JsResult<()> {
        let bitmap = bitmap_arg(&bitmap)?;
        self.write(|g| {
            let (x, y) = (g.metrics.bounds.x, g.metrics.bounds.y);
            g.set_bitmap(bitmap, x, y, false).js()
        })
    }
    /// The mapped codepoints (`{Unicode: n}` / `{Legacy: n}` plain data);
    /// assignable from `CodepointLike` values. Edits of a view re-index the
    /// font.
    #[wasm_bindgen(getter, unchecked_return_type = "CodepointDict[]")]
    pub fn codepoints(&self) -> JsResult<JsValue> {
        self.read(|g| conv::to_js(&g.codepoints))
    }
    /// Replace the mapped codepoints.
    #[wasm_bindgen(setter)]
    pub fn set_codepoints(
        &self,
        #[wasm_bindgen(unchecked_param_type = "CodepointLike[]")] value: JsValue,
    ) -> JsResult<()> {
        let cps = codepoints_arg(&value)?;
        self.write(|g| {
            g.codepoints = cps;
            Ok(())
        })
    }
    /// Unicode sequences (ligatures/clusters) mapped to this glyph.
    #[wasm_bindgen(getter, js_name = unicodeSequences, unchecked_return_type = "number[][]")]
    pub fn unicode_sequences(&self) -> JsResult<JsValue> {
        self.read(|g| conv::to_js(&g.unicode_sequences))
    }
    /// Replace the Unicode sequences (validated when the glyph is added or
    /// committed into a font).
    #[wasm_bindgen(setter, js_name = unicodeSequences)]
    pub fn set_unicode_sequences(
        &self,
        #[wasm_bindgen(unchecked_param_type = "number[][]")] value: JsValue,
    ) -> JsResult<()> {
        let sequences: Vec<Vec<u32>> = conv::from_js(&value)?;
        self.write(|g| {
            g.unicode_sequences = sequences;
            Ok(())
        })
    }
    /// The glyph name, if any.
    #[wasm_bindgen(getter)]
    pub fn name(&self) -> JsResult<Option<String>> {
        self.read(|g| Ok(g.name.clone()))
    }
    /// Set or clear (`null`/`undefined`) the glyph name.
    #[wasm_bindgen(setter)]
    pub fn set_name(&self, value: Option<String>) -> JsResult<()> {
        self.write(|g| {
            g.name = value;
            Ok(())
        })
    }
    /// Glyph metrics (`GlyphMetrics`) as a copy; assign to replace (validated:
    /// the bounds must match the bitmap; a failed assignment changes nothing).
    #[wasm_bindgen(getter, unchecked_return_type = "GlyphMetrics")]
    pub fn metrics(&self) -> JsResult<JsValue> {
        self.read(|g| conv::to_js(&g.metrics))
    }
    /// Replace the metrics.
    #[wasm_bindgen(setter)]
    pub fn set_metrics(
        &self,
        #[wasm_bindgen(unchecked_param_type = "GlyphMetrics")] value: JsValue,
    ) -> JsResult<()> {
        let metrics: GlyphMetrics = conv::from_js(&value)?;
        self.write(|g| {
            // Owned glyphs are edited in place: restore on a failed check.
            let saved = std::mem::replace(&mut g.metrics, metrics);
            let checked = g.validate().js();
            if checked.is_err() {
                g.metrics = saved;
            }
            checked
        })
    }
    /// Glyph properties (a live `Properties` view); assignable.
    #[wasm_bindgen(getter)]
    pub fn properties(&self) -> JsProperties {
        JsProperties::view(PropsLoc::Glyph(self.loc.clone()))
    }
    /// Replace the glyph properties with a copy of `value`.
    #[wasm_bindgen(setter)]
    pub fn set_properties(
        &self,
        #[wasm_bindgen(unchecked_param_type = "Properties | PropertiesDict")] value: JsValue,
    ) -> JsResult<()> {
        let props = properties_arg(&value)?;
        self.write(|g| {
            g.properties = props;
            Ok(())
        })
    }
    /// Replace the bitmap and its bounds, placing its bottom-left corner at
    /// `(x, y)` (numbers or bigints, default 0); with `trim`, crop to the ink
    /// first (`Glyph::set_bitmap`).
    #[wasm_bindgen(js_name = setBitmap)]
    pub fn set_bitmap_at(
        &self,
        #[wasm_bindgen(unchecked_param_type = "Bitmap | GlyphBitmap")] bitmap: JsValue,
        #[wasm_bindgen(unchecked_optional_param_type = "WideInt")] x: Option<JsValue>,
        #[wasm_bindgen(unchecked_optional_param_type = "WideInt")] y: Option<JsValue>,
        #[wasm_bindgen(unchecked_optional_param_type = "boolean")] trim: Option<bool>,
    ) -> JsResult<()> {
        let bitmap = bitmap_arg(&bitmap)?;
        let (x, y) = (offset_arg(x, "x")?, offset_arg(y, "y")?);
        self.write(|g| g.set_bitmap(bitmap, x, y, trim.unwrap_or(false)).js())
    }
    /// Move the glyph box by `(dx, dy)` (numbers or bigints) with overflow
    /// checks (`Glyph::shift`).
    pub fn shift(
        &self,
        #[wasm_bindgen(unchecked_param_type = "WideInt")] dx: JsValue,
        #[wasm_bindgen(unchecked_param_type = "WideInt")] dy: JsValue,
    ) -> JsResult<()> {
        let dx: i64 = conv::int_from_js(&dx, "dx")?;
        let dy: i64 = conv::int_from_js(&dy, "dy")?;
        self.write(|g| g.shift(dx, dy).js())
    }
    /// Check bounds/bitmap agreement and Unicode sequences
    /// (`Glyph::validate`).
    pub fn validate(&self) -> JsResult<()> {
        self.read(|g| g.validate().js())
    }
    /// The glyph cropped or extended to `cell` (a `Bounds` object), in cell
    /// coordinates (`Glyph::bitmap_in_cell`).
    #[wasm_bindgen(js_name = bitmapInCell)]
    pub fn bitmap_in_cell(
        &self,
        #[wasm_bindgen(unchecked_param_type = "Bounds")] cell: JsValue,
    ) -> JsResult<JsBitmap> {
        let cell: Bounds = conv::from_js(&cell)?;
        self.read(|g| g.bitmap_in_cell(cell).js().map(JsBitmap::new))
    }
    /// An independent owned deep copy (also of a view).
    pub fn copy(&self) -> JsResult<JsGlyph> {
        Ok(JsGlyph::new(self.snapshot()?))
    }
    /// Whether `other` is a `Glyph` with equal contents.
    pub fn equals(
        &self,
        #[wasm_bindgen(unchecked_param_type = "unknown")] other: JsValue,
    ) -> JsResult<bool> {
        let other = match handle_arg(&other)? {
            Some(Handle::Glyph(loc)) => JsGlyph { loc }.snapshot()?,
            _ => return Ok(false),
        };
        self.read(|g| Ok(*g == other))
    }
    /// `Glyph(id=3, codepoints=[Codepoint.unicode(0x0041)], name="A", size=5x8)`.
    #[wasm_bindgen(js_name = toString)]
    pub fn to_string_js(&self) -> JsResult<String> {
        let id = self.id();
        self.read(|g| {
            let cps: Vec<String> = g
                .codepoints
                .iter()
                .map(|c| JsCodepoint(*c).to_string_js())
                .collect();
            Ok(format!(
                "Glyph(id={}, codepoints=[{}], name={}, size={}x{})",
                id.map_or_else(|| "undefined".to_owned(), |i| i.to_string()),
                cps.join(", "),
                opt_text(g.name.as_deref()),
                g.bitmap.width(),
                g.bitmap.height()
            ))
        })
    }
}
