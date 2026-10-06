//! Shared helpers of the OpenType (`sfnt`), `woff`, `variants` and `merge`
//! bindings.
//!
//! - **Handle arguments inside plain data.** Python's `font_arg` accepts a
//!   `Font` handle or a font dict anywhere, also nested (`merge` parts,
//!   `Variants.languages`). wasm-bindgen can only borrow an exported struct
//!   that is a direct parameter, and converting a `JsValue` back into one
//!   moves it (the JS handle would be released). So the handle classes
//!   used here get one internal method, `__fpShareStorage()`: the running
//!   call invokes it on the JS object, the method hands a clone of its
//!   storage reference (an `Rc`, never the data) to a thread-local slot,
//!   and the call takes it from there ([`handle_of`]). A released handle
//!   throws `InvalidInputError` ("this Font handle was released"), as the
//!   package's handle guard does; another class throws `InvalidInputError`
//!   naming it. No `unsafe`, no copy of the font until the call snapshots
//!   it, exactly like Python's `handle.snapshot()`.
//! - **Tags**: four-byte OpenType tags are four-character strings (Latin-1,
//!   the serde form, so `"OS/2"`) or four bytes (`BytesLike`) on input and
//!   strings on output.
//! - **Results with byte fields**: Rust results whose `Vec<u8>` fields are
//!   the payload (`EncodedCmap.data`, `glyf::Encoded.glyf`, …) come back as
//!   plain objects whose byte fields are `Uint8Array`s (the rest keeps the
//!   serde shape); nested model data keeps `number[]` for bytes, which input
//!   accepts as `Uint8Array` too.
use crate::conv;
use crate::errors::{self, JsResult};
use crate::handles::{FontLoc, JsBitmap, JsFont, Shared, read_font};
use fontpixel::model::{Font, GlyphBitmap};
use js_sys::{Array, Function, Object, Reflect, Uint8Array};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::any::Any;
use std::cell::RefCell;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

thread_local! {
    static HANDOFF: RefCell<Option<Box<dyn Any>>> = const { RefCell::new(None) };
}

/// Name of the internal storage-sharing method (see the module docs).
pub const SHARE_METHOD: &str = "__fpShareStorage";

/// Called by a handle's `__fpShareStorage()`: offer `storage` to the call
/// that invoked it.
pub fn offer(storage: Box<dyn Any>) {
    HANDOFF.with(|slot| *slot.borrow_mut() = Some(storage));
}

fn class_name(value: &JsValue) -> String {
    Reflect::get(value, &JsValue::from_str("constructor"))
        .ok()
        .and_then(|c| Reflect::get(&c, &JsValue::from_str("name")).ok())
        .and_then(|n| n.as_string())
        .unwrap_or_else(|| "object".to_owned())
}

/// The storage of handle `value` when it is a handle of class `class`
/// (storage type `T`); `None` when `value` is not a handle at all (plain
/// data). Released handles and handles of other classes throw
/// `InvalidInputError`.
pub fn handle_of<T: Any + Clone>(value: &JsValue, class: &str) -> JsResult<Option<T>> {
    if !value.is_object() {
        return Ok(None);
    }
    let pointer = Reflect::get(value, &JsValue::from_str("__wbg_ptr"))?;
    if pointer.is_undefined() {
        return Ok(None);
    }
    let found = class_name(value);
    if pointer.as_f64() == Some(0.0) {
        return Err(errors::invalid(format!(
            "this {found} handle was released (free() was called); use copy() to keep a value"
        )));
    }
    let wrong = || {
        errors::invalid(format!(
            "expected a {class} handle or plain data, got a {found} handle"
        ))
    };
    let share = Reflect::get(value, &JsValue::from_str(SHARE_METHOD))?;
    let Some(share) = share.dyn_ref::<Function>() else {
        return Err(wrong());
    };
    HANDOFF.with(|slot| slot.borrow_mut().take());
    share.call0(value)?;
    let storage = HANDOFF.with(|slot| slot.borrow_mut().take());
    match storage.and_then(|s| s.downcast::<T>().ok()) {
        Some(storage) => Ok(Some(*storage)),
        None => Err(wrong()),
    }
}

#[wasm_bindgen(js_class = Font)]
impl JsFont {
    /// Internal: share this handle's storage with the running fontpixel
    /// call (handle arguments inside plain data).
    #[wasm_bindgen(js_name = __fpShareStorage, skip_typescript)]
    pub fn share_storage_j4(&self) -> JsResult<()> {
        offer(Box::new(self.loc.clone()));
        Ok(())
    }
}

#[wasm_bindgen(js_class = Bitmap)]
impl JsBitmap {
    /// Internal: share this handle's storage with the running fontpixel
    /// call (handle arguments inside plain data).
    #[wasm_bindgen(js_name = __fpShareStorage, skip_typescript)]
    pub fn share_storage_j4(&self) -> JsResult<()> {
        offer(Box::new(self.bitmap.clone()));
        Ok(())
    }
}

/// A model font from a `Font` handle (copied, also a view) or a `FontDict`.
pub fn font_arg(value: &JsValue, what: &str) -> JsResult<Font> {
    if value.is_undefined() || value.is_null() {
        return Err(errors::invalid(format!(
            "{what}: expected a Font handle or a FontDict"
        )));
    }
    match handle_of::<FontLoc>(value, "Font")? {
        Some(loc) => read_font(&loc, |f| Ok(f.clone())),
        None => conv::from_js(value),
    }
}

/// A raster from a `Bitmap` handle (copied) or `GlyphBitmap` plain data.
pub fn bitmap_arg(value: &JsValue) -> JsResult<GlyphBitmap> {
    match handle_of::<Shared<GlyphBitmap>>(value, "Bitmap")? {
        Some(bitmap) => Ok(crate::handles::borrow(&bitmap, "Bitmap")?.clone()),
        None => conv::from_js(value),
    }
}

/// The Mono raster of a `Bitmap` handle or `GlyphBitmap` plain data; Gray8 needs explicit
/// thresholding first.
pub fn mono(value: &JsValue) -> JsResult<fontpixel::Bitmap> {
    match bitmap_arg(value)? {
        GlyphBitmap::Mono(m) => Ok(m),
        GlyphBitmap::Gray8(_) => Err(errors::invalid(
            "this operation needs a Mono bitmap; threshold Gray8 explicitly first",
        )),
    }
}

/// A four-byte tag from a four-character Latin-1 string or four bytes.
pub fn tag_arg(value: &JsValue, what: &str) -> JsResult<[u8; 4]> {
    if let Some(text) = value.as_string() {
        let mut out = [0u8; 4];
        let mut count = 0;
        for c in text.chars() {
            let byte = u8::try_from(u32::from(c)).map_err(|_| {
                errors::invalid(format!("{what}: tag {text:?} has a character above U+00FF"))
            })?;
            if count == 4 {
                return Err(errors::invalid(format!(
                    "{what}: tag {text:?} must be four characters"
                )));
            }
            out[count] = byte;
            count += 1;
        }
        if count != 4 {
            return Err(errors::invalid(format!(
                "{what}: tag {text:?} must be four characters"
            )));
        }
        return Ok(out);
    }
    let bytes = conv::bytes_from_js(value, what).map_err(|_| {
        errors::invalid(format!(
            "{what}: expected a four-character string or four bytes"
        ))
    })?;
    <[u8; 4]>::try_from(bytes.as_slice())
        .map_err(|_| errors::invalid(format!("{what}: tag bytes must have length four")))
}

/// A list of tags (array of strings or byte arrays).
pub fn tags_arg(value: &JsValue, what: &str) -> JsResult<Vec<[u8; 4]>> {
    let list = value
        .dyn_ref::<Array>()
        .ok_or_else(|| errors::invalid(format!("{what}: expected an array of tags")))?;
    list.iter().map(|t| tag_arg(&t, what)).collect()
}

/// The serde string form of a tag (each byte one Latin-1 character).
pub fn tag_str(tag: [u8; 4]) -> String {
    tag.iter().map(|&b| char::from(b)).collect()
}

/// Deserialize an optional options/limits object; `undefined`/`null` give
/// the default.
pub fn opt<T: DeserializeOwned + Default>(value: Option<JsValue>) -> JsResult<T> {
    conv::from_js_or_default(&value.unwrap_or(JsValue::UNDEFINED))
}

/// Deserialize required plain data.
pub fn req<T: DeserializeOwned>(value: &JsValue) -> JsResult<T> {
    conv::from_js(value)
}

/// A plain object from `(key, value)` pairs.
pub fn object(items: &[(&str, JsValue)]) -> JsResult<JsValue> {
    let out = Object::new();
    for (key, value) in items {
        Reflect::set(&out, &JsValue::from_str(key), value)?;
    }
    Ok(out.into())
}

/// The plain data of `value` with the byte fields `fields` set to
/// `Uint8Array`s. Take the byte fields out of `value` first (they then
/// serialize as empty arrays and are replaced).
pub fn with_bytes<T: Serialize>(value: &T, fields: &[(&str, &[u8])]) -> JsResult<JsValue> {
    let out = conv::to_js(value)?;
    for (key, bytes) in fields {
        Reflect::set(&out, &JsValue::from_str(key), &bytes_js(bytes))?;
    }
    Ok(out)
}

/// A fresh `Uint8Array` as a `JsValue`.
pub fn bytes_js(bytes: &[u8]) -> JsValue {
    conv::bytes_to_js(bytes).into()
}

/// `Uint8Array` or `null`.
pub fn optional_bytes(bytes: &Option<Vec<u8>>) -> JsValue {
    match bytes {
        Some(b) => bytes_js(b),
        None => JsValue::NULL,
    }
}

/// A `Uint8Array` result for a function returning bytes.
pub fn byte_result(bytes: &[u8]) -> Uint8Array {
    conv::bytes_to_js(bytes)
}

/// An integer parameter checked to fit `T`.
pub fn int<T: TryFrom<i64>>(value: f64, what: &str) -> JsResult<T> {
    conv::int(value, what)
}

/// An optional integer parameter (`undefined`/`null` give `default`).
pub fn optional_int<T: TryFrom<i64>>(
    value: Option<JsValue>,
    default: T,
    what: &str,
) -> JsResult<T> {
    match value {
        Some(v) if !v.is_undefined() && !v.is_null() => {
            let n = v
                .as_f64()
                .ok_or_else(|| errors::invalid(format!("{what}: expected a number")))?;
            conv::int(n, what)
        }
        _ => Ok(default),
    }
}
