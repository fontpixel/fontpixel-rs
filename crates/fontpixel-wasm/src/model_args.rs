//! Argument conversions shared by the model, metadata and resource classes
//! (and available to every area): handle-or-plain-data unions, codepoints and
//! property values.
//!
//! **Handles inside `JsValue`s.** Python accepts "a `Glyph` or its dict"
//! everywhere; wasm-bindgen can only borrow a handle that is a typed
//! parameter (`&JsGlyph`), and its `TryFromJsValue` takes ownership (the JS
//! object would be released). [`handle_arg`] instead asks `js/model.js`
//! (`stashHandle`) to check the value against the exported classes with
//! `instanceof` and call the class's hidden `__fpStash()` method, which
//! borrows the receiver like any method and leaves a clone of its storage
//! reference (an `Rc`, never the data) in a thread-local slot. Nothing is
//! copied until the caller snapshots it, and the JS object stays valid. A
//! released handle throws the guard's `InvalidInputError`.
use crate::conv;
use crate::errors::{JsResult, invalid};
use crate::handles::{FontLoc, GlyphLoc, JsBitmap, JsFont, JsFontSet, JsGlyph, Shared};
use fontpixel::model::{Codepoint, Font, FontSet, Glyph, GlyphBitmap, Properties, PropertyValue};
use js_sys::Array;
use std::cell::RefCell;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(raw_module = "./model.js")]
extern "C" {
    #[wasm_bindgen(js_name = stashHandle, catch)]
    fn stash_handle(value: &JsValue) -> Result<bool, JsValue>;
}

/// Where a `Properties` handle's list lives.
#[derive(Clone)]
pub enum PropsLoc {
    /// A standalone list.
    Owned(Shared<Properties>),
    /// `FontSet.properties`.
    Set(Shared<FontSet>),
    /// `Font.properties`.
    Font(FontLoc),
    /// `Font.headers`.
    Headers(FontLoc),
    /// `Font.names.localized`.
    Localized(FontLoc),
    /// `Glyph.properties`.
    Glyph(GlyphLoc),
}

/// The storage reference of a handle found inside a `JsValue`.
pub enum Handle {
    /// A `FontSet`.
    FontSet(Shared<FontSet>),
    /// A `Font` (owned or a view).
    Font(FontLoc),
    /// A `Glyph` (owned or a view).
    Glyph(GlyphLoc),
    /// A `Bitmap`.
    Bitmap(Shared<GlyphBitmap>),
    /// A `Codepoint` value.
    Codepoint(Codepoint),
    /// A `Properties` list (owned or a view).
    Properties(PropsLoc),
    /// A `ResourceSet`.
    ResourceSet(crate::resources::Files),
    /// An `Input`.
    Input(crate::resources::InputParts),
    /// An `OutputAsset`.
    OutputAsset(fontpixel::resources::OutputAsset),
    /// An `OutputBundle`.
    OutputBundle(fontpixel::resources::OutputBundle),
    /// A writer result.
    Written(fontpixel::resources::Written),
    /// A `BitmapProjection`.
    Projection(crate::metadata::ProjectionSpec),
}

thread_local! {
    static STASH: RefCell<Option<Handle>> = const { RefCell::new(None) };
}

/// Leave `handle` for [`handle_arg`] (called by the `__fpStash` methods).
pub fn stash(handle: Handle) {
    STASH.with(|slot| *slot.borrow_mut() = Some(handle));
}

/// The handle inside `value`, or `None` when `value` is not an instance of
/// an exported class (plain data, numbers, …). Throws the guard's
/// `InvalidInputError` for a released handle.
pub fn handle_arg(value: &JsValue) -> JsResult<Option<Handle>> {
    if !value.is_object() {
        return Ok(None);
    }
    STASH.with(|slot| slot.borrow_mut().take());
    if !stash_handle(value)? {
        return Ok(None);
    }
    Ok(STASH.with(|slot| slot.borrow_mut().take()))
}

/// Name of a handle's class, for error messages.
fn class_of(handle: &Handle) -> &'static str {
    match handle {
        Handle::FontSet(_) => "FontSet",
        Handle::Font(_) => "Font",
        Handle::Glyph(_) => "Glyph",
        Handle::Bitmap(_) => "Bitmap",
        Handle::Codepoint(_) => "Codepoint",
        Handle::Properties(_) => "Properties",
        Handle::ResourceSet(_) => "ResourceSet",
        Handle::Input(_) => "Input",
        Handle::OutputAsset(_) => "OutputAsset",
        Handle::OutputBundle(_) => "OutputBundle",
        Handle::Written(_) => "Written",
        Handle::Projection(_) => "BitmapProjection",
    }
}

/// `InvalidInputError`: `what` got a handle of the wrong class.
pub fn wrong_handle(what: &str, expected: &str, handle: &Handle) -> JsValue {
    invalid(format!(
        "{what}: expected a {expected} or its plain data, got a {}",
        class_of(handle)
    ))
}

// ------------------------------------------------------------ hidden stash methods

#[wasm_bindgen(js_class = FontSet)]
impl JsFontSet {
    /// Internal: leave this handle's storage reference for a Rust argument
    /// conversion (`model_args::handle_arg`).
    #[doc(hidden)]
    #[wasm_bindgen(js_name = __fpStash, skip_typescript)]
    pub fn fp_stash(&self) {
        stash(Handle::FontSet(self.set.clone()));
    }
}

#[wasm_bindgen(js_class = Font)]
impl JsFont {
    /// Internal: see `FontSet.__fpStash`.
    #[doc(hidden)]
    #[wasm_bindgen(js_name = __fpStash, skip_typescript)]
    pub fn fp_stash(&self) {
        stash(Handle::Font(self.loc.clone()));
    }
}

#[wasm_bindgen(js_class = Glyph)]
impl JsGlyph {
    /// Internal: see `FontSet.__fpStash`.
    #[doc(hidden)]
    #[wasm_bindgen(js_name = __fpStash, skip_typescript)]
    pub fn fp_stash(&self) {
        stash(Handle::Glyph(self.loc.clone()));
    }
}

#[wasm_bindgen(js_class = Bitmap)]
impl JsBitmap {
    /// Internal: see `FontSet.__fpStash`.
    #[doc(hidden)]
    #[wasm_bindgen(js_name = __fpStash, skip_typescript)]
    pub fn fp_stash(&self) {
        stash(Handle::Bitmap(self.bitmap.clone()));
    }
}

// ------------------------------------------------------------ model arguments

/// A copy of a `FontSet` handle or `FontSetDict` plain data.
pub fn font_set_arg(value: &JsValue) -> JsResult<FontSet> {
    match handle_arg(value)? {
        Some(Handle::FontSet(set)) => JsFontSet { set }.snapshot(),
        Some(other) => Err(wrong_handle("font set", "FontSet", &other)),
        None => conv::from_js(value),
    }
}

/// A copy of a `Font` handle (owned or view) or `FontDict` plain data.
pub fn font_arg(value: &JsValue) -> JsResult<Font> {
    match handle_arg(value)? {
        Some(Handle::Font(loc)) => JsFont { loc }.snapshot(),
        Some(other) => Err(wrong_handle("font", "Font", &other)),
        None => conv::from_js(value),
    }
}

/// A copy of a `Glyph` handle (owned or view) or `GlyphDict` plain data.
pub fn glyph_arg(value: &JsValue) -> JsResult<Glyph> {
    match handle_arg(value)? {
        Some(Handle::Glyph(loc)) => JsGlyph { loc }.snapshot(),
        Some(other) => Err(wrong_handle("glyph", "Glyph", &other)),
        None => conv::from_js(value),
    }
}

/// A copy of a `Bitmap` handle or `GlyphBitmap` plain data.
pub fn bitmap_arg(value: &JsValue) -> JsResult<GlyphBitmap> {
    match handle_arg(value)? {
        Some(Handle::Bitmap(bitmap)) => JsBitmap { bitmap }.snapshot(),
        Some(other) => Err(wrong_handle("bitmap", "Bitmap", &other)),
        None => conv::from_js(value),
    }
}

/// The elements of an array argument (`what` names it in errors).
pub fn array_arg(value: &JsValue, what: &str) -> JsResult<Array> {
    if Array::is_array(value) {
        Ok(value.clone().unchecked_into())
    } else {
        Err(invalid(format!("{what}: expected an array")))
    }
}

/// Copies of an array of fonts (handles or plain data).
pub fn fonts_arg(value: &JsValue) -> JsResult<Vec<Font>> {
    array_arg(value, "fonts")?
        .iter()
        .map(|f| font_arg(&f))
        .collect()
}

/// Copies of an array of glyphs (handles or plain data).
pub fn glyphs_arg(value: &JsValue) -> JsResult<Vec<Glyph>> {
    array_arg(value, "glyphs")?
        .iter()
        .map(|g| glyph_arg(&g))
        .collect()
}

/// A codepoint argument: a `Codepoint`, a number (Unicode), a one-character
/// string (Unicode) or the plain data `{Unicode: n}` / `{Legacy: n}`.
pub fn codepoint_arg(value: &JsValue) -> JsResult<Codepoint> {
    if let Some(n) = value.as_f64() {
        return Ok(Codepoint::Unicode(conv::int(n, "codepoint")?));
    }
    if let Some(text) = value.as_string() {
        let mut chars = text.chars();
        return match (chars.next(), chars.next()) {
            (Some(c), None) => Ok(Codepoint::Unicode(u32::from(c))),
            _ => Err(invalid(format!(
                "a codepoint string must be exactly one character, not {text:?}"
            ))),
        };
    }
    if value.as_bool().is_some() {
        return Err(invalid("a boolean is not a codepoint"));
    }
    match handle_arg(value)? {
        Some(Handle::Codepoint(cp)) => Ok(cp),
        Some(other) => Err(wrong_handle("codepoint", "Codepoint", &other)),
        None => conv::from_js(value),
    }
}

/// An array of codepoint arguments.
pub fn codepoints_arg(value: &JsValue) -> JsResult<Vec<Codepoint>> {
    array_arg(value, "codepoints")?
        .iter()
        .map(|c| codepoint_arg(&c))
        .collect()
}

/// A Unicode sequence: a string (its scalar values) or an array of numbers.
pub fn sequence_arg(value: &JsValue) -> JsResult<Vec<u32>> {
    if let Some(text) = value.as_string() {
        return Ok(text.chars().map(u32::from).collect());
    }
    array_arg(value, "sequence")?
        .iter()
        .map(|v| {
            v.as_f64()
                .ok_or_else(|| invalid("sequence: expected numbers"))
                .and_then(|n| conv::int(n, "sequence value"))
        })
        .collect()
}

/// A property value: `null` → `Flag`; a safe-integer number or a bigint →
/// `Integer` (bigints above the `i64` range → `Unsigned`); a string →
/// `Text`; a `Uint8Array`/`ArrayBuffer`/`DataView` → `Bytes`; an array →
/// `List` (recursively); any other object is the tagged plain data
/// (`{"Decimal": "1.5"}`, `{"Unsigned": 5}`, `"Flag"` as a string is Text).
/// Booleans and non-integral numbers are rejected.
pub fn property_value_arg(value: &JsValue) -> JsResult<PropertyValue> {
    if value.is_null() || value.is_undefined() {
        return Ok(PropertyValue::Flag);
    }
    if value.as_bool().is_some() {
        return Err(invalid(
            "a boolean is not a property value; use null for a flag",
        ));
    }
    if let Some(n) = value.as_f64() {
        return Ok(PropertyValue::Integer(conv::int(n, "property value")?));
    }
    if value.is_bigint() {
        let wide: i128 = conv::int_from_js(value, "property value")?;
        return match i64::try_from(wide) {
            Ok(v) => Ok(PropertyValue::Integer(v)),
            Err(_) => u64::try_from(wide)
                .map(PropertyValue::Unsigned)
                .map_err(|_| invalid(format!("property value {wide} is out of range"))),
        };
    }
    if let Some(text) = value.as_string() {
        return Ok(PropertyValue::Text(text));
    }
    if value.is_instance_of::<js_sys::Uint8Array>()
        || value.is_instance_of::<js_sys::ArrayBuffer>()
        || value.is_instance_of::<js_sys::DataView>()
    {
        return conv::bytes_from_js(value, "property value").map(PropertyValue::Bytes);
    }
    if Array::is_array(value) {
        return array_arg(value, "property value")?
            .iter()
            .map(|v| property_value_arg(&v))
            .collect::<JsResult<Vec<_>>>()
            .map(PropertyValue::List);
    }
    conv::from_js(value)
}
