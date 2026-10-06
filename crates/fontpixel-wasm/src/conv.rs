//! Plain data, integers and bytes across the JS boundary.
//!
//! **Plain data** (every type deriving serde: options, reports, metrics, …)
//! crosses as JS objects with the Rust field names (snake_case); unknown keys
//! are rejected by the types' own `deny_unknown_fields`/strict maps.
//!
//! - In: `js/plain.js` `encodePlain` turns the JS value into JSON text with
//!   exact integers (numbers that are safe integers, and bigints); serde_json
//!   parses it into a `serde_json::Value` and the type deserializes from
//!   that value. Going through `Value` keeps `flatten`/tagged types exact under
//!   the workspace's `arbitrary_precision` feature.
//! - Out: serde_json writes JSON text with a formatter that turns integers
//!   beyond `Number.MAX_SAFE_INTEGER` into marked strings, and `decodePlain`
//!   parses it (`JSON.parse`, with a reviver turning marked strings into
//!   bigints only when there are any). If a string itself starts with the
//!   marker's NUL or raw JSON holds a wide integer, the value is converted
//!   through a `serde_json::Value` walk instead.
//!
//! **Integers**: per value, `number` when |v| ≤ 2^53 − 1, else `bigint`.
//! Inputs accept both; non-integral numbers, NaN/infinities and integral
//! numbers beyond 2^53 are rejected for integer fields. `usize`/`isize` are
//! 32-bit on `wasm32`, so they are always `number` (a larger input fails
//! deserialization). Integer-keyed maps are objects with decimal-string keys;
//! inputs may also be `Map`s with string, integer or bigint keys.
//!
//! **Bytes**: parameters accept `Uint8Array`, `ArrayBuffer` and `DataView`
//! ([`bytes_from_js`]); results are fresh `Uint8Array`s ([`bytes_to_js`]).
//! Byte lists inside plain data are arrays of numbers on output and accept
//! the same buffer types on input.
use crate::errors::{self, JsResult};
use js_sys::{Array, BigInt, Object, Reflect, Uint8Array};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::ser::{CharEscape, Formatter};
use std::cell::Cell;
use std::io;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

/// `Number.MAX_SAFE_INTEGER`.
pub const MAX_SAFE_INTEGER: u64 = (1 << 53) - 1;

#[wasm_bindgen(raw_module = "./plain.js")]
extern "C" {
    #[wasm_bindgen(js_name = encodePlain, catch)]
    fn encode_plain(value: &JsValue) -> Result<String, JsValue>;
    #[wasm_bindgen(js_name = decodePlain)]
    fn decode_plain(text: &str, markers: bool) -> JsValue;
}

// ------------------------------------------------------------ pure Rust part

/// serde's message without serde_json's position suffix (positions refer to
/// the internal JSON text, not to anything the caller wrote).
pub fn message(error: &serde_json::Error) -> String {
    let text = error.to_string();
    match text.rfind(" at line ") {
        Some(at) if error.line() > 0 => text[..at].to_owned(),
        _ => text,
    }
}

/// Deserialize `T` from JSON text written by `encodePlain`.
///
/// Types deserialize straight from the text, so serde's messages name the
/// expected type (``invalid type: floating point `1.5`, expected u32``).
/// Types that buffer their input (the `fontpixel-ops` requests) go through a
/// `serde_json::Value`, which keeps integers exact under the workspace's
/// `arbitrary_precision` feature; floats reach them intact because
/// `encodePlain` writes the shortest round-trip form (`1e21`, `1e-7`).
pub fn parse_text<T: DeserializeOwned>(text: &str) -> Result<T, String> {
    serde_json::from_str(text).map_err(|e| message(&e))
}

/// Serialized plain data, ready for `decodePlain` or the value walk.
#[derive(Debug)]
pub enum Written {
    /// JSON text; `markers` when it holds marked bigint strings.
    Text {
        /// The JSON document.
        text: String,
        /// Whether wide integers were written as marked strings.
        markers: bool,
    },
    /// A value the text form cannot carry unambiguously.
    Value(serde_json::Value),
}

#[derive(Default)]
struct Flags {
    markers: Cell<bool>,
    fallback: Cell<bool>,
    in_string: Cell<bool>,
    string_start: Cell<bool>,
}

/// serde_json formatter: wide integers become marked strings; a string
/// starting with NUL (the marker's first character) or raw JSON with a wide
/// integer requests the value walk.
struct MarkFormatter<'a>(&'a Flags);

impl MarkFormatter<'_> {
    fn wide<W: ?Sized + io::Write>(&mut self, writer: &mut W, digits: &str) -> io::Result<()> {
        self.0.markers.set(true);
        writer.write_all(b"\"\\u0000fp:n")?;
        writer.write_all(digits.as_bytes())?;
        writer.write_all(b"\"")
    }
    fn integer<W: ?Sized + io::Write>(
        &mut self,
        writer: &mut W,
        magnitude: u128,
        digits: &str,
    ) -> io::Result<()> {
        if !self.0.in_string.get() && magnitude > u128::from(MAX_SAFE_INTEGER) {
            self.wide(writer, digits)
        } else {
            writer.write_all(digits.as_bytes())
        }
    }
}

impl Formatter for MarkFormatter<'_> {
    fn write_i64<W: ?Sized + io::Write>(&mut self, writer: &mut W, value: i64) -> io::Result<()> {
        self.integer(writer, u128::from(value.unsigned_abs()), &value.to_string())
    }
    fn write_u64<W: ?Sized + io::Write>(&mut self, writer: &mut W, value: u64) -> io::Result<()> {
        self.integer(writer, u128::from(value), &value.to_string())
    }
    fn write_i128<W: ?Sized + io::Write>(&mut self, writer: &mut W, value: i128) -> io::Result<()> {
        self.integer(writer, value.unsigned_abs(), &value.to_string())
    }
    fn write_u128<W: ?Sized + io::Write>(&mut self, writer: &mut W, value: u128) -> io::Result<()> {
        self.integer(writer, value, &value.to_string())
    }
    fn write_number_str<W: ?Sized + io::Write>(
        &mut self,
        writer: &mut W,
        value: &str,
    ) -> io::Result<()> {
        if wide_integer_text(value) {
            if self.0.in_string.get() {
                return writer.write_all(value.as_bytes());
            }
            return self.wide(writer, value);
        }
        writer.write_all(value.as_bytes())
    }
    fn begin_string<W: ?Sized + io::Write>(&mut self, writer: &mut W) -> io::Result<()> {
        self.0.in_string.set(true);
        self.0.string_start.set(true);
        writer.write_all(b"\"")
    }
    fn end_string<W: ?Sized + io::Write>(&mut self, writer: &mut W) -> io::Result<()> {
        self.0.in_string.set(false);
        self.0.string_start.set(false);
        writer.write_all(b"\"")
    }
    fn write_string_fragment<W: ?Sized + io::Write>(
        &mut self,
        writer: &mut W,
        fragment: &str,
    ) -> io::Result<()> {
        if !fragment.is_empty() {
            self.0.string_start.set(false);
        }
        writer.write_all(fragment.as_bytes())
    }
    fn write_char_escape<W: ?Sized + io::Write>(
        &mut self,
        writer: &mut W,
        char_escape: CharEscape,
    ) -> io::Result<()> {
        if self.0.string_start.get() && matches!(char_escape, CharEscape::AsciiControl(0)) {
            self.0.fallback.set(true);
        }
        self.0.string_start.set(false);
        serde_json::ser::CompactFormatter.write_char_escape(writer, char_escape)
    }
    fn write_raw_fragment<W: ?Sized + io::Write>(
        &mut self,
        writer: &mut W,
        fragment: &str,
    ) -> io::Result<()> {
        if raw_needs_walk(fragment) {
            self.0.fallback.set(true);
        }
        writer.write_all(fragment.as_bytes())
    }
}

/// Whether `text` is an integer literal beyond `MAX_SAFE_INTEGER`.
fn wide_integer_text(text: &str) -> bool {
    let digits = text.strip_prefix('-').unwrap_or(text);
    !digits.is_empty()
        && digits.bytes().all(|b| b.is_ascii_digit())
        && digits
            .parse::<u128>()
            .map_or(true, |v| v > u128::from(MAX_SAFE_INTEGER))
}

/// Raw JSON (a `RawValue`) that might hold a wide integer or a NUL-led
/// string: conservatively, any run of 16+ digits or a `"\u0000`.
fn raw_needs_walk(fragment: &str) -> bool {
    let mut run = 0usize;
    for b in fragment.bytes() {
        if b.is_ascii_digit() {
            run += 1;
            if run >= 16 {
                return true;
            }
        } else {
            run = 0;
        }
    }
    fragment.contains("\"\\u0000")
}

/// Serialize `value` for [`decode`]: JSON text when unambiguous.
pub fn write_text<T: Serialize + ?Sized>(value: &T) -> Result<Written, String> {
    let flags = Flags::default();
    let mut buf = Vec::with_capacity(128);
    {
        let mut ser = serde_json::Serializer::with_formatter(&mut buf, MarkFormatter(&flags));
        value.serialize(&mut ser).map_err(|e| message(&e))?;
    }
    if flags.fallback.get() {
        return serde_json::to_value(value)
            .map(Written::Value)
            .map_err(|e| message(&e));
    }
    let text = String::from_utf8(buf).map_err(|e| e.to_string())?;
    Ok(Written::Text {
        text,
        markers: flags.markers.get(),
    })
}

// ------------------------------------------------------------ JS conversion

/// Deserialize plain data; `undefined` reads as `null`.
pub fn from_js<T: DeserializeOwned>(value: &JsValue) -> JsResult<T> {
    let text = if value.is_undefined() {
        "null".to_owned()
    } else {
        encode_plain(value)?
    };
    parse_text(&text).map_err(errors::invalid)
}

/// Deserialize plain data; `undefined`/`null` give `T::default()`.
pub fn from_js_or_default<T: DeserializeOwned + Default>(value: &JsValue) -> JsResult<T> {
    if value.is_undefined() || value.is_null() {
        return Ok(T::default());
    }
    from_js(value)
}

/// Deserialize optional plain data; `undefined`/`null` give `None`.
pub fn from_js_opt<T: DeserializeOwned>(value: &JsValue) -> JsResult<Option<T>> {
    if value.is_undefined() || value.is_null() {
        return Ok(None);
    }
    from_js(value).map(Some)
}

/// Serialize plain data to a JS value (per-value number/bigint).
pub fn to_js<T: Serialize + ?Sized>(value: &T) -> JsResult<JsValue> {
    match write_text(value).map_err(errors::invalid)? {
        Written::Text { text, markers } => Ok(decode_plain(&text, markers)),
        Written::Value(value) => value_to_js(&value),
    }
}

/// Convert a `serde_json::Value` (exact numbers) into a JS value.
pub fn value_to_js(value: &serde_json::Value) -> JsResult<JsValue> {
    use serde_json::Value as V;
    Ok(match value {
        V::Null => JsValue::NULL,
        V::Bool(b) => JsValue::from_bool(*b),
        V::Number(n) => number_to_js(n)?,
        V::String(s) => JsValue::from_str(s),
        V::Array(items) => {
            let out = Array::new_with_length(items.len() as u32);
            for (i, item) in items.iter().enumerate() {
                out.set(i as u32, value_to_js(item)?);
            }
            out.into()
        }
        V::Object(map) => {
            let out = Object::new();
            for (key, item) in map {
                Reflect::set(&out, &JsValue::from_str(key), &value_to_js(item)?)?;
            }
            out.into()
        }
    })
}

fn number_to_js(n: &serde_json::Number) -> JsResult<JsValue> {
    if let Some(v) = n.as_i64() {
        return Ok(int_to_js(i128::from(v)));
    }
    if let Some(v) = n.as_u64() {
        return Ok(int_to_js(i128::from(v)));
    }
    let text = n.to_string();
    if wide_integer_text(&text) {
        return BigInt::new(&JsValue::from_str(&text))
            .map(Into::into)
            .map_err(Into::into);
    }
    Ok(JsValue::from_f64(n.as_f64().unwrap_or(f64::NAN)))
}

/// An integer as `number` when |v| ≤ 2^53 − 1, else as `bigint`.
pub fn int_to_js(value: i128) -> JsValue {
    if value.unsigned_abs() <= u128::from(MAX_SAFE_INTEGER) {
        JsValue::from_f64(value as f64)
    } else {
        JsValue::from(value)
    }
}

/// An integer argument given as `number` or `bigint`, checked against `T`'s
/// range. Non-integral numbers and integral numbers beyond 2^53 − 1 are
/// rejected (`InvalidInputError`).
pub fn int_from_js<T: TryFrom<i128>>(value: &JsValue, what: &str) -> JsResult<T> {
    let wide: i128 = if let Some(n) = value.as_f64() {
        if n.fract() != 0.0 || !n.is_finite() || n.abs() > MAX_SAFE_INTEGER as f64 {
            return Err(errors::invalid(format!(
                "{what}: {n} is not an exact integer (pass a bigint beyond 2^53)"
            )));
        }
        n as i128
    } else if value.is_bigint() {
        let text = value
            .dyn_ref::<BigInt>()
            .and_then(|b| b.to_string(10).ok())
            .map(String::from)
            .unwrap_or_default();
        text.parse::<i128>()
            .map_err(|_| errors::invalid(format!("{what}: {text} is out of range")))?
    } else {
        return Err(errors::invalid(format!(
            "{what}: expected an integer number or bigint"
        )));
    };
    T::try_from(wide).map_err(|_| {
        errors::invalid(format!(
            "{what}: {wide} is out of range for {}",
            std::any::type_name::<T>()
        ))
    })
}

/// An integer parameter declared as `f64` in Rust (`number` in JS; bigints
/// are rejected by the JS engine), checked to be exact and within `T`.
/// Use this instead of `u32`/`i32`/`usize` parameters, which wasm-bindgen
/// would silently truncate or wrap.
pub fn int<T: TryFrom<i64>>(value: f64, what: &str) -> JsResult<T> {
    if value.fract() != 0.0 || !value.is_finite() || value.abs() > MAX_SAFE_INTEGER as f64 {
        return Err(errors::invalid(format!(
            "{what}: {value} is not an exact integer"
        )));
    }
    T::try_from(value as i64).map_err(|_| {
        errors::invalid(format!(
            "{what}: {value} is out of range for {}",
            std::any::type_name::<T>()
        ))
    })
}

/// Bytes from a `Uint8Array`, `Uint8ClampedArray`, `ArrayBuffer` or
/// `DataView` (copied once into WASM memory).
pub fn bytes_from_js(value: &JsValue, what: &str) -> JsResult<Vec<u8>> {
    if let Some(array) = value.dyn_ref::<Uint8Array>() {
        return Ok(array.to_vec());
    }
    if let Some(buffer) = value.dyn_ref::<js_sys::ArrayBuffer>() {
        return Ok(Uint8Array::new(buffer).to_vec());
    }
    if let Some(view) = value.dyn_ref::<js_sys::DataView>() {
        return Ok(Uint8Array::new_with_byte_offset_and_length(
            &view.buffer(),
            view.byte_offset() as u32,
            view.byte_length() as u32,
        )
        .to_vec());
    }
    if let Some(clamped) = value.dyn_ref::<js_sys::Uint8ClampedArray>() {
        return Ok(Uint8Array::new_with_byte_offset_and_length(
            &clamped.buffer(),
            clamped.byte_offset(),
            clamped.length(),
        )
        .to_vec());
    }
    Err(errors::invalid(format!(
        "{what}: expected a Uint8Array, ArrayBuffer or DataView"
    )))
}

/// A fresh `Uint8Array` holding a copy of `bytes` (never a view of WASM
/// memory, so it stays valid when memory grows).
pub fn bytes_to_js(bytes: &[u8]) -> Uint8Array {
    Uint8Array::from(bytes)
}

/// An options object: a plain object (or `undefined`/`null` for none) whose
/// keys must all be listed in `allowed`; values are read per key.
pub struct Options {
    object: Option<Object>,
    what: &'static str,
}

impl Options {
    /// Check `value` and its keys.
    pub fn new(value: &JsValue, allowed: &[&str], what: &'static str) -> JsResult<Self> {
        if value.is_undefined() || value.is_null() {
            return Ok(Self { object: None, what });
        }
        let object = value
            .dyn_ref::<Object>()
            .filter(|o| {
                let proto = Object::get_prototype_of(o);
                proto.is_null() || proto == Object::get_prototype_of(&Object::new())
            })
            .ok_or_else(|| errors::invalid(format!("{what}: expected a plain options object")))?;
        for key in Object::keys(object).iter() {
            let key = key.as_string().unwrap_or_default();
            if !allowed.contains(&key.as_str()) {
                return Err(errors::invalid(format!(
                    "{what}: unknown option `{key}`, expected one of: {}",
                    allowed.join(", ")
                )));
            }
        }
        Ok(Self {
            object: Some(object.clone()),
            what,
        })
    }
    /// The raw value of `key` (`None` when absent, `undefined` or `null`).
    pub fn get(&self, key: &str) -> JsResult<Option<JsValue>> {
        let Some(object) = &self.object else {
            return Ok(None);
        };
        let value = Reflect::get(object, &JsValue::from_str(key))?;
        Ok((!value.is_undefined() && !value.is_null()).then_some(value))
    }
    /// A string option.
    pub fn string(&self, key: &str) -> JsResult<Option<String>> {
        self.get(key)?
            .map(|v| {
                v.as_string().ok_or_else(|| {
                    errors::invalid(format!("{}: option `{key}` must be a string", self.what))
                })
            })
            .transpose()
    }
    /// A boolean option.
    pub fn bool(&self, key: &str) -> JsResult<Option<bool>> {
        self.get(key)?
            .map(|v| {
                v.as_bool().ok_or_else(|| {
                    errors::invalid(format!("{}: option `{key}` must be a boolean", self.what))
                })
            })
            .transpose()
    }
    /// A plain-data option deserialized as `T`.
    pub fn plain<T: DeserializeOwned>(&self, key: &str) -> JsResult<Option<T>> {
        self.get(key)?.map(|v| from_js(&v)).transpose()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fontpixel::model::PropertyValue;
    use std::collections::BTreeMap;

    fn text<T: Serialize>(value: &T) -> (String, bool) {
        match write_text(value).unwrap() {
            Written::Text { text, markers } => (text, markers),
            Written::Value(v) => panic!("unexpected fallback {v}"),
        }
    }

    #[test]
    fn wide_integers_become_marked_strings_per_value() {
        let small = PropertyValue::Unsigned(MAX_SAFE_INTEGER);
        assert_eq!(
            text(&small),
            (r#"{"Unsigned":9007199254740991}"#.into(), false)
        );
        let wide = PropertyValue::Unsigned(MAX_SAFE_INTEGER + 1);
        assert_eq!(
            text(&wide),
            (r#"{"Unsigned":"\u0000fp:n9007199254740992"}"#.into(), true)
        );
        let negative = PropertyValue::Integer(i64::MIN);
        assert_eq!(
            text(&negative).0,
            r#"{"Integer":"\u0000fp:n-9223372036854775808"}"#
        );
    }

    #[test]
    fn integer_map_keys_stay_plain_strings() {
        let map: BTreeMap<u64, u8> = [(u64::MAX, 1), (2, 3)].into();
        assert_eq!(
            text(&map),
            (r#"{"2":3,"18446744073709551615":1}"#.into(), false)
        );
        let back: BTreeMap<u64, u8> = parse_text(r#"{"2":3,"18446744073709551615":1}"#).unwrap();
        assert_eq!(back, map);
    }

    #[test]
    fn nul_led_strings_and_wide_raw_json_use_the_value_walk() {
        assert!(matches!(
            write_text(&"\u{0}fp:n1").unwrap(),
            Written::Value(_)
        ));
        assert!(matches!(
            write_text(&"a\u{0}").unwrap(),
            Written::Text { .. }
        ));
        let raw = serde_json::value::RawValue::from_string("[12345678901234567]".into()).unwrap();
        assert!(matches!(write_text(&raw).unwrap(), Written::Value(_)));
    }

    #[test]
    fn exact_input_integers_and_float_rejection() {
        let v: PropertyValue = parse_text(r#"{"Unsigned":18446744073709551615}"#).unwrap();
        assert_eq!(v, PropertyValue::Unsigned(u64::MAX));
        // An integral JS number beyond 2^53 is encoded as a float.
        let err = parse_text::<PropertyValue>(r#"{"Unsigned":1.152921504606847e18}"#).unwrap_err();
        assert!(err.contains("floating point"), "{err}");
        let err = parse_text::<u32>("1.5").unwrap_err();
        assert!(err.contains("floating point"), "{err}");
        let err = parse_text::<u8>("256").unwrap_err();
        assert!(err.contains("expected u8"), "{err}");
        assert_eq!(
            parse_text::<f64>("1.152921504606847e18").unwrap(),
            2f64.powi(60)
        );
    }

    #[derive(serde::Deserialize)]
    struct Group {
        x: f64,
    }
    #[derive(serde::Deserialize)]
    struct Flat {
        #[serde(flatten)]
        group: Group,
    }

    #[test]
    fn buffered_floats_in_shortest_form_survive_arbitrary_precision() {
        // encodePlain's float spellings (JS shortest round trip, `+` removed),
        // buffered through a Value as the fontpixel-ops requests do.
        for text in [
            "0.1",
            "2.5",
            "1e21",
            "1e-7",
            "1.152921504606847e18",
            "-3.25e-300",
            "12",
        ] {
            let json = format!(r#"{{"x":{text}}}"#);
            let value: serde_json::Value = serde_json::from_str(&json).unwrap();
            let flat: Flat =
                serde_json::from_value(value).unwrap_or_else(|e| panic!("{text}: {e}"));
            assert_eq!(flat.group.x, text.parse::<f64>().unwrap());
        }
    }

    #[test]
    fn flattened_and_tagged_requests_keep_numbers() {
        // `RenderRequest` flattens `LayoutOptions` (i64 `line_limit`) through
        // the strict buffered map; `TaskRequest` is tagged by `kind`.
        let request: fontpixel_ops::RenderRequest =
            parse_text(r#"{"input":"a.bdf","output":"a.png","text":"A","line_limit":-1000}"#)
                .unwrap();
        assert_eq!(request.layout.line_limit, -1000);
        let (out, _) = text(&request);
        assert!(out.contains(r#""line_limit":-1000"#), "{out}");
        let task: fontpixel_ops::TaskRequest = parse_text(
            r#"{"kind":"convert","inputs":["a.bdf"],"output":"a.pcf","max_total_bytes":1000}"#,
        )
        .unwrap();
        let (out, _) = text(&task);
        assert!(out.contains(r#""max_total_bytes":1000"#), "{out}");
        let err = parse_text::<fontpixel_ops::RenderRequest>(
            r#"{"input":"a.bdf","output":"a.png","text":"A","bogus":1}"#,
        )
        .unwrap_err();
        assert!(err.contains("unknown field `bogus`"), "{err}");
    }

    #[test]
    fn messages_drop_internal_positions() {
        let err = parse_text::<fontpixel::resources::ResourceLimits>(r#"{"max_glyphs":"x"}"#)
            .unwrap_err();
        assert!(!err.contains(" at line "), "{err}");
    }
}
