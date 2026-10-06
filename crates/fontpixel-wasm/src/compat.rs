//! Native half of the bdfparser-js compatible API (`fontpixel/compat/bdfparser`).
//!
//! The classes here bind `fontpixel::compat::bdfparser` (the SPEC-v3 façade
//! shared with the Python binding) one to one under glue names that do not
//! collide with the native API:
//!
//! - [`JsCompatFont`] (`BdfparserRawFont`) and [`JsCompatGlyph`]
//!   (`BdfparserRawGlyph`) are handles over the Rust `Font`/`Glyph`;
//! - [`JsCompatBitmap`](crate::compat_bitmap::JsCompatBitmap)
//!   (`BdfparserRawBitmap`) wraps the Rust compatibility bitmap;
//! - [`JsCompatCodec`](crate::compat_misc::JsCompatCodec) (`BdfparserRawCodec`)
//!   is a charset converter over `fontpixel::charset`.
//!
//! `js/compat_bdfparser.js` wraps them in bdfparser-js's `Font`, `Glyph` and
//! `Bitmap` classes (live `headers`/`props`/`glyphs`/`unencoded` views, options
//! objects, the warning handler and bdfparser-js's error classes). The font data
//! stays in WASM memory; plain data crosses only when a view reads it.
//!
//! Errors: Rust failures become the package's `FontpixelError` subclasses (the JS
//! layer translates them to bdfparser-js's classes). An exception thrown by a JS
//! callback (converter, transform function, reader/writer object) is recorded
//! and rethrown unchanged when the Rust call returns ([`done`]).
use crate::compat_bitmap::JsCompatBitmap;
use crate::compat_glyph::JsCompatGlyph;
use crate::conv;
use crate::errors::{self, JsResult};
use crate::handles::{self, JsFont};
use fontpixel::compat::bdfparser as bp;
use fontpixel::formats::pcf::PcfWriteOptions;
use js_sys::{Array, Function, Uint8Array};
use serde_json::{Map, Value as V};
use std::cell::RefCell;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

thread_local! {
    /// The first exception thrown by a JS callback during the current Rust call.
    static PENDING: RefCell<Option<JsValue>> = const { RefCell::new(None) };
}

/// Remember a callback exception; only the first one of a call is kept.
pub(crate) fn stash(error: JsValue) {
    PENDING.with(|p| {
        p.borrow_mut().get_or_insert(error);
    });
}

/// Drop a callback exception recorded during a call whose failure is ignored.
pub(crate) fn clear_pending() {
    PENDING.with(|p| p.borrow_mut().take());
}

/// Finish a Rust call: a pending callback exception wins, then the Rust error.
pub(crate) fn done<T>(result: fontpixel::Result<T>) -> JsResult<T> {
    if let Some(error) = PENDING.with(|p| p.borrow_mut().take()) {
        return Err(error);
    }
    result.map_err(errors::to_js)
}

/// A `fontpixel::Error::InvalidInput` (reported for failed callbacks).
pub(crate) fn callback_failed(what: &str) -> fontpixel::Error {
    fontpixel::Error::InvalidInput(format!("{what} failed"))
}

/// Plain JS value → JSON value (exact integers; handles and other non-plain
/// values are rejected with `InvalidInputError`).
pub(crate) fn js2v(value: &JsValue) -> JsResult<V> {
    if value.is_undefined() {
        return Ok(V::Null);
    }
    conv::from_js(value)
}

/// Plain JS object → JSON object.
pub(crate) fn js2obj(value: &JsValue) -> JsResult<Map<String, V>> {
    match js2v(value)? {
        V::Object(m) => Ok(m),
        V::Null => Ok(Map::new()),
        _ => Err(errors::invalid("expected a plain object")),
    }
}

/// JSON value → JS value (numbers per value, `bigint` beyond 2^53 − 1).
pub(crate) fn v2js(value: &V) -> JsResult<JsValue> {
    conv::to_js(value)
}

/// An optional integer argument (`undefined`/`null` → None).
pub(crate) fn opt_int(value: &JsValue, what: &str) -> JsResult<Option<i64>> {
    if value.is_undefined() || value.is_null() {
        return Ok(None);
    }
    conv::int_from_js::<i64>(value, what).map(Some)
}

/// The single code point of a JS string, or None (empty, several, lone
/// surrogates are code points too, as in `String.prototype.codePointAt`).
pub(crate) fn single_code_point(value: &JsValue) -> Option<u32> {
    let s = value.dyn_ref::<js_sys::JsString>()?;
    let n = s.length();
    let cp = s.code_point_at(0).as_f64()? as u32;
    let units = if cp > 0xffff { 2 } else { 1 };
    (n == units).then_some(cp)
}

/// A JS string holding the code point `cp` (lone surrogates included).
pub(crate) fn code_point_string(cp: u32) -> JsValue {
    match char::from_u32(cp) {
        Some(c) => JsValue::from_str(c.encode_utf8(&mut [0; 4])),
        None => js_sys::JsString::from_char_code1(cp).into(),
    }
}

/// Wrap a JS `(char: string) => number | null | undefined` as a numeric converter.
fn char2cp_converter(f: Function) -> bp::Converter {
    bp::Converter::new(move |n| match f.call1(&JsValue::UNDEFINED, &code_point_string(n)) {
        Ok(r) => r
            .as_f64()
            .filter(|v| v.fract() == 0.0 && *v >= 0.0 && *v <= f64::from(u32::MAX))
            .map(|v| v as u32),
        Err(e) => {
            stash(e);
            None
        }
    })
}

/// Wrap a JS `(cp: number) => string | null | undefined` as a numeric converter
/// (a result of exactly one code point; anything else is "not representable").
fn cp2char_converter(f: Function) -> bp::Converter {
    bp::Converter::new(move |n| match f.call1(&JsValue::UNDEFINED, &JsValue::from(n)) {
        Ok(r) => single_code_point(&r),
        Err(e) => {
            stash(e);
            None
        }
    })
}

/// A JS reader object (`read(n) => Uint8Array | null`) as a Rust reader.
struct JsReader(JsValue);
impl std::io::Read for JsReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let read = js_sys::Reflect::get(&self.0, &JsValue::from_str("read"))
            .ok()
            .and_then(|f| f.dyn_into::<Function>().ok())
            .ok_or_else(|| {
                stash(errors::invalid("the reader object has no read(n) method"));
                std::io::Error::other("no read method")
            })?;
        let chunk = read
            .call1(&self.0, &JsValue::from(buf.len() as u32))
            .map_err(|e| {
                stash(e);
                std::io::Error::other("read() failed")
            })?;
        if chunk.is_null() || chunk.is_undefined() {
            return Ok(0);
        }
        let data = conv::bytes_from_js(&chunk, "read() result").map_err(|e| {
            stash(e);
            std::io::Error::other("read() must return bytes")
        })?;
        if data.len() > buf.len() {
            stash(errors::invalid("read() returned more bytes than requested"));
            return Err(std::io::Error::other("oversized read"));
        }
        buf[..data.len()].copy_from_slice(&data);
        Ok(data.len())
    }
}

/// A JS writer object (`write(bytes: Uint8Array)`) as a Rust writer.
struct JsWriter(JsValue);
impl std::io::Write for JsWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let write = js_sys::Reflect::get(&self.0, &JsValue::from_str("write"))
            .ok()
            .and_then(|f| f.dyn_into::<Function>().ok())
            .ok_or_else(|| {
                stash(errors::invalid("the writer object has no write(bytes) method"));
                std::io::Error::other("no write method")
            })?;
        write
            .call1(&self.0, &conv::bytes_to_js(buf))
            .map_err(|e| {
                stash(e);
                std::io::Error::other("write() failed")
            })?;
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// `PcfWriteOptions` plain data (`byte_order`, `bit_order`, `glyph_pad`, ...).
pub(crate) fn pcf_options(value: &JsValue) -> JsResult<PcfWriteOptions> {
    conv::from_js_or_default(value)
}

/// `TtfOptions` plain data (`shape`, `dotsize`, `ppu`, `metadata`, `timestamp`).
pub(crate) fn ttf_options(value: &JsValue) -> JsResult<bp::ttf::TtfOptions> {
    conv::from_js_or_default(value)
}

fn save_options(format: &str, options: &JsValue) -> JsResult<bp::SaveOptions> {
    Ok(match format.to_ascii_lowercase().as_str() {
        "bdf" => bp::SaveOptions::Bdf,
        "pcf" => bp::SaveOptions::Pcf(pcf_options(options)?),
        "ttf" => bp::SaveOptions::Ttf(ttf_options(options)?),
        "svg" => bp::SaveOptions::Svg,
        other => {
            return Err(errors::invalid(format!(
                "Unknown output format {other:?} (use bdf, pcf, ttf or svg)"
            )));
        }
    })
}

fn glyphs_to_js(glyphs: Vec<bp::Glyph>) -> Array {
    glyphs
        .into_iter()
        .map(|g| JsValue::from(JsCompatGlyph::wrap(g)))
        .collect()
}

/// Handle over `fontpixel::compat::bdfparser::Font` (wrapped by the
/// bdfparser-js `Font` of `fontpixel/compat/bdfparser`). Attached glyph
/// handles share its model.
#[wasm_bindgen(js_name = BdfparserRawFont)]
pub struct JsCompatFont {
    pub(crate) font: RefCell<bp::Font>,
}

impl JsCompatFont {
    /// Wrap a Rust compatibility font.
    pub fn wrap(font: bp::Font) -> Self {
        Self {
            font: RefCell::new(font),
        }
    }
    /// Run `f` with shared access.
    pub fn read<R>(&self, f: impl FnOnce(&bp::Font) -> JsResult<R>) -> JsResult<R> {
        f(&*handles::borrow(&self.font, "BdfparserRawFont")?)
    }
    /// Run `f` with exclusive access.
    pub fn write<R>(&self, f: impl FnOnce(&mut bp::Font) -> JsResult<R>) -> JsResult<R> {
        f(&mut *handles::borrow_mut(&self.font, "BdfparserRawFont")?)
    }
}

#[wasm_bindgen(js_class = BdfparserRawFont)]
impl JsCompatFont {
    /// An empty font (`Font::default`).
    #[wasm_bindgen(constructor)]
    pub fn constructor() -> JsCompatFont {
        Self::wrap(bp::Font::default())
    }

    /// Load BDF text (`Font::load_string`).
    #[wasm_bindgen(js_name = loadString)]
    pub fn load_string(&self, text: &str) -> JsResult<()> {
        self.write(|f| done(f.load_string(text).map(|_| ())))
    }

    /// Load BDF lines (`Font::load_lines`); line delimiters are not changed.
    #[wasm_bindgen(js_name = loadLines)]
    pub fn load_lines(&self, lines: Vec<String>) -> JsResult<()> {
        self.write(|f| done(f.load_lines(lines).map(|_| ())))
    }

    /// Load auto-detected BDF/PCF/TTF bytes, also gzip-compressed
    /// (`Font::load_bytes_with_options`; `Font::load_bytes` without `pixelsize`).
    #[wasm_bindgen(js_name = loadBytes)]
    pub fn load_bytes(
        &self,
        #[wasm_bindgen(unchecked_param_type = "BytesLike")] data: JsValue,
        #[wasm_bindgen(unchecked_optional_param_type = "number")] pixelsize: Option<JsValue>,
    ) -> JsResult<()> {
        let data = conv::bytes_from_js(&data, "data")?;
        let pixelsize = opt_int(&pixelsize.unwrap_or(JsValue::UNDEFINED), "pixelsize")?;
        self.write(|f| done(f.load_bytes_with_options(&data, pixelsize).map(|_| ())))
    }

    /// Load glyph-based TrueType bytes (`Font::load_ttf`).
    #[wasm_bindgen(js_name = loadTtf)]
    pub fn load_ttf(
        &self,
        #[wasm_bindgen(unchecked_param_type = "BytesLike")] data: JsValue,
        #[wasm_bindgen(unchecked_optional_param_type = "number")] pixelsize: Option<JsValue>,
    ) -> JsResult<()> {
        let data = conv::bytes_from_js(&data, "data")?;
        let pixelsize = opt_int(&pixelsize.unwrap_or(JsValue::UNDEFINED), "pixelsize")?;
        self.write(|f| done(f.load_ttf(&data, pixelsize).map(|_| ())))
    }

    /// Load from a reader object `{read(n) => Uint8Array | null}` with the
    /// bounded format detection of `loadBytes` (`Font::load_file_obj`).
    #[wasm_bindgen(js_name = loadFileObj)]
    pub fn load_file_obj(
        &self,
        #[wasm_bindgen(unchecked_param_type = "{ read(n: number): BytesLike | null | undefined }")]
        reader: JsValue,
    ) -> JsResult<()> {
        self.write(|f| done(f.load_file_obj(JsReader(reader)).map(|_| ())))
    }

    /// Load a file by path (`Font::load_file_path`): unavailable, the package
    /// never reads paths (capability `filesystem`).
    #[wasm_bindgen(js_name = loadFilePath)]
    pub fn load_file_path(&self, _path: &str) -> JsResult<()> {
        Err(crate::capabilities::unsupported(
            "filesystem",
            "BdfparserRawFont.loadFilePath",
        ))
    }

    /// A new font from `CreateOptions` plain data (`Font::create`).
    pub fn create(
        #[wasm_bindgen(unchecked_optional_param_type = "CreateOptions")] options: Option<JsValue>,
    ) -> JsResult<JsCompatFont> {
        let o: bp::CreateOptions = conv::from_js_or_default(&options.unwrap_or(JsValue::UNDEFINED))?;
        done(bp::Font::create(&o)).map(Self::wrap)
    }

    /// Wrap a copy of a native `Font` (`Font::from_core`).
    #[wasm_bindgen(js_name = fromCore)]
    pub fn from_core(font: &JsFont) -> JsResult<JsCompatFont> {
        let core = font.snapshot()?;
        done(bp::Font::from_core(core)).map(Self::wrap)
    }

    /// A detached native `Font` copy of the model (`Font::core`).
    pub fn core(&self) -> JsResult<JsFont> {
        self.read(|f| Ok(JsFont::new(f.core())))
    }

    /// Deep copy (`Font::clone`): converters are shared, the model is copied.
    #[wasm_bindgen(js_name = clone)]
    pub fn clone_font(&self) -> JsResult<JsCompatFont> {
        self.read(|f| Ok(Self::wrap(f.clone())))
    }

    /// Structured warnings collected so far (`Font::report`), as plain data.
    #[wasm_bindgen(unchecked_return_type = "Report")]
    pub fn report(&self) -> JsResult<JsValue> {
        self.read(|f| conv::to_js(&f.report()))
    }

    /// Messages of the warning-level report entries from index `start` on (the
    /// ones bdfparser-js passes to its warning handler) and the new entry count
    /// (`Font::warnings_since`).
    #[wasm_bindgen(js_name = warningsSince, unchecked_return_type = "[string[], number]")]
    pub fn warnings_since(&self, start: f64) -> JsResult<JsValue> {
        let start: usize = conv::int(start, "start")?;
        self.read(|f| {
            let (messages, count) = f.warnings_since(start);
            let list: Array = messages.iter().map(|m| JsValue::from_str(m)).collect();
            Ok(Array::of2(&list, &JsValue::from(count as u32)).into())
        })
    }

    /// Projected SPEC headers in insertion order (`Font::headers`).
    #[wasm_bindgen(unchecked_return_type = "Record<string, unknown>")]
    pub fn headers(&self) -> JsResult<JsValue> {
        self.read(|f| v2js(&V::Object(f.headers())))
    }

    /// Replace all headers in place (`Font::set_headers`); glyph handles stay attached.
    #[wasm_bindgen(js_name = setHeaders)]
    pub fn set_headers(
        &self,
        #[wasm_bindgen(unchecked_param_type = "Record<string, unknown>")] headers: JsValue,
    ) -> JsResult<()> {
        let h = js2obj(&headers)?;
        self.write(|f| {
            f.set_headers(&h);
            Ok(())
        })
    }

    /// Set one header key, appended when new (`Font::set_header`).
    #[wasm_bindgen(js_name = setHeader)]
    pub fn set_header(&self, key: &str, value: JsValue) -> JsResult<()> {
        let v = js2v(&value)?;
        self.write(|f| {
            f.set_header(key, v);
            Ok(())
        })
    }

    /// Remove one header key; false if absent (`Font::remove_header`).
    #[wasm_bindgen(js_name = removeHeader)]
    pub fn remove_header(&self, key: &str) -> JsResult<bool> {
        self.write(|f| Ok(f.remove_header(key)))
    }

    /// Attached handles of the unencoded glyphs in order (`Font::unencoded_handles`).
    #[wasm_bindgen(js_name = unencodedHandles, unchecked_return_type = "BdfparserRawGlyph[]")]
    pub fn unencoded_handles(&self) -> JsResult<Array> {
        self.read(|f| Ok(glyphs_to_js(f.unencoded_handles())))
    }

    /// Attached handle of the `index`-th unencoded glyph (`Font::unencoded_handle`).
    #[wasm_bindgen(js_name = unencodedHandle)]
    pub fn unencoded_handle(&self, index: f64) -> JsResult<Option<JsCompatGlyph>> {
        let index: usize = conv::int(index, "index")?;
        self.read(|f| Ok(f.unencoded_handle(index).map(JsCompatGlyph::wrap)))
    }

    /// Alternate code of the `index`-th unencoded glyph, `null` if none,
    /// `undefined` if there is no such glyph (`Font::unencoded_altcode`).
    #[wasm_bindgen(js_name = unencodedAltcode)]
    pub fn unencoded_altcode(&self, index: f64) -> JsResult<JsValue> {
        let index: usize = conv::int(index, "index")?;
        self.read(|f| match f.unencoded_altcode(index) {
            Some(v) => v2js(&v),
            None => Ok(JsValue::UNDEFINED),
        })
    }

    /// Set the alternate code of the `index`-th unencoded glyph (`null` removes
    /// it); false if there is no such glyph (`Font::set_unencoded_altcode`).
    #[wasm_bindgen(js_name = setUnencodedAltcode)]
    pub fn set_unencoded_altcode(&self, index: f64, altcode: JsValue) -> JsResult<bool> {
        let index: usize = conv::int(index, "index")?;
        let alt = js2v(&altcode)?;
        self.write(|f| done(f.set_unencoded_altcode(index, &alt)))
    }

    /// Set metadata keys of the `index`-th unencoded glyph; false if there is
    /// no such glyph (`Font::set_unencoded_meta`).
    #[wasm_bindgen(js_name = setUnencodedMeta)]
    pub fn set_unencoded_meta(
        &self,
        index: f64,
        #[wasm_bindgen(unchecked_param_type = "Record<string, unknown>")] fields: JsValue,
    ) -> JsResult<bool> {
        let index: usize = conv::int(index, "index")?;
        let m = js2obj(&fields)?;
        self.write(|f| done(f.set_unencoded_meta(index, &m)))
    }

    /// Remove the `index`-th unencoded glyph, or all of them without an index;
    /// false if the index is out of range (`Font::remove_unencoded`).
    #[wasm_bindgen(js_name = removeUnencoded)]
    pub fn remove_unencoded(&self, index: Option<f64>) -> JsResult<bool> {
        let index = index.map(|i| conv::int::<usize>(i, "index")).transpose()?;
        self.write(|f| Ok(f.remove_unencoded(index)))
    }

    /// The 17 metadata slots of a fields object, normalized as `addglyph`
    /// does (`Font::prepare_meta`).
    #[wasm_bindgen(js_name = prepareMeta, unchecked_return_type = "unknown[]")]
    pub fn prepare_meta(
        &self,
        #[wasm_bindgen(unchecked_param_type = "Record<string, unknown>")] fields: JsValue,
    ) -> JsResult<JsValue> {
        let m = js2obj(&fields)?;
        self.read(|f| v2js(&V::Array(done(f.prepare_meta(&m))?)))
    }

    /// Add normalized glyph metadata and, for an unencoded glyph, its
    /// alternate code (`Font::prepare_meta` + `Font::put_with_alt`).
    #[wasm_bindgen(js_name = putWithAlt)]
    pub fn put_with_alt(
        &self,
        #[wasm_bindgen(unchecked_param_type = "Record<string, unknown>")] fields: JsValue,
        altcode: JsValue,
    ) -> JsResult<()> {
        let m = js2obj(&fields)?;
        let alt = js2v(&altcode)?;
        self.write(|f| {
            let meta = done(f.prepare_meta(&m))?;
            done(f.put_with_alt(meta, alt))
        })
    }

    /// Case-normalized properties in insertion order (`Font::props`).
    #[wasm_bindgen(unchecked_return_type = "Record<string, unknown>")]
    pub fn props(&self) -> JsResult<JsValue> {
        self.read(|f| v2js(&V::Object(f.props())))
    }

    /// Case-insensitive property lookup, `null` if absent (`Font::getprop`).
    pub fn getprop(&self, key: &str) -> JsResult<JsValue> {
        self.read(|f| v2js(&f.getprop(key)))
    }

    /// Set a property, keeping the first spelling of an existing key (`Font::setprop`).
    pub fn setprop(&self, key: &str, value: JsValue) -> JsResult<()> {
        let v = js2v(&value)?;
        self.write(|f| {
            f.setprop(key, v);
            Ok(())
        })
    }

    /// Delete every case-insensitive occurrence of a property (`Font::delprop`).
    pub fn delprop(&self, key: &str) -> JsResult<()> {
        self.write(|f| {
            f.delprop(key);
            Ok(())
        })
    }

    /// Canonical charset key (`Font::charset`).
    pub fn charset(&self) -> JsResult<String> {
        self.read(|f| Ok(f.charset()))
    }

    /// Override the charset; `''` clears the override (`Font::encoding`).
    pub fn encoding(&self, name: &str) -> JsResult<()> {
        self.write(|f| {
            f.encoding(name);
            Ok(())
        })
    }

    /// Install JS converters `char2cp(char) => number | null` and
    /// `cp2char(cp) => string | null`; `undefined`/`null` restores the
    /// charset table in that direction (`Font::set_converters`).
    #[wasm_bindgen(js_name = setConverters)]
    pub fn set_converters(&self, char2cp: JsValue, cp2char: JsValue) -> JsResult<()> {
        let to_fn = |v: JsValue, what: &str| -> JsResult<Option<Function>> {
            if v.is_undefined() || v.is_null() {
                return Ok(None);
            }
            v.dyn_into::<Function>()
                .map(Some)
                .map_err(|_| errors::invalid(format!("{what} must be a function")))
        };
        let a = to_fn(char2cp, "char2cp")?.map(char2cp_converter);
        let b = to_fn(cp2char, "cp2char")?.map(cp2char_converter);
        self.write(|f| {
            f.set_converters(a, b);
            Ok(())
        })
    }

    /// Unicode code point → code, `undefined` if not representable (`Font::unicode2cp`).
    pub fn unicode2cp(&self, code: f64) -> JsResult<Option<u32>> {
        let code: u32 = conv::int(code, "code")?;
        self.read(|f| done(f.unicode2cp(code)))
    }

    /// One-character string → code (`Font::char2cp`).
    pub fn char2cp(&self, ch: &str) -> JsResult<Option<u32>> {
        self.read(|f| done(f.char2cp(ch)))
    }

    /// Code → Unicode code point, surrogates retained (`Font::cp2unicode`).
    pub fn cp2unicode(&self, cp: JsValue) -> JsResult<Option<u32>> {
        let cp: i64 = conv::int_from_js(&cp, "cp")?;
        self.read(|f| done(f.cp2unicode(cp)))
    }

    /// Code → one-character string (`Font::cp2char`).
    pub fn cp2char(&self, cp: JsValue) -> JsResult<Option<String>> {
        let cp: i64 = conv::int_from_js(&cp, "cp")?;
        self.read(|f| done(f.cp2char(cp)).map(|c| c.map(String::from)))
    }

    /// Number of encoded glyphs (`Font::length`).
    pub fn length(&self) -> JsResult<u32> {
        self.read(|f| Ok(f.length() as u32))
    }

    /// Encoded codes in an order (0 file, -1 reverse, 1 ascending, 2
    /// descending) filtered by a SPEC range (`Font::itercps`).
    #[wasm_bindgen(unchecked_return_type = "number[]")]
    pub fn itercps(&self, order: f64, r: JsValue) -> JsResult<JsValue> {
        let order: i64 = conv::int(order, "order")?;
        let r = js2v(&r)?;
        self.read(|f| conv::to_js(&done(f.itercps(order, &r))?))
    }

    /// Attached handle, or `undefined` with bdfparser's warning
    /// (`Font::glyphbycp`). bdfparser-js does not warn: its `glyphbycp` uses
    /// [`lookup`](Self::lookup).
    pub fn glyphbycp(&self, cp: JsValue) -> JsResult<Option<JsCompatGlyph>> {
        let cp: i64 = conv::int_from_js(&cp, "cp")?;
        self.read(|f| {
            let g = f.glyphbycp(cp).map(JsCompatGlyph::wrap);
            clear_pending();
            Ok(g)
        })
    }

    /// Attached handle, or `undefined` without a warning (`Font::lookup`).
    pub fn lookup(&self, cp: JsValue) -> JsResult<Option<JsCompatGlyph>> {
        let cp: i64 = conv::int_from_js(&cp, "cp")?;
        self.read(|f| Ok(f.lookup(cp).map(JsCompatGlyph::wrap)))
    }

    /// Attached handle by character, with bdfparser's warning on a miss (`Font::glyph`).
    pub fn glyph(&self, ch: &str) -> JsResult<Option<JsCompatGlyph>> {
        self.read(|f| done(f.glyph(ch)).map(|g| g.map(JsCompatGlyph::wrap)))
    }

    /// Attached handles in order and range (`Font::iterglyphs`).
    #[wasm_bindgen(unchecked_return_type = "BdfparserRawGlyph[]")]
    pub fn iterglyphs(&self, order: f64, r: JsValue) -> JsResult<Array> {
        let order: i64 = conv::int(order, "order")?;
        let r = js2v(&r)?;
        self.read(|f| done(f.iterglyphs(order, &r)).map(glyphs_to_js))
    }

    /// Characters without a glyph, or `undefined` (`Font::lacksglyphs`).
    pub fn lacksglyphs(&self, text: &str) -> JsResult<Option<Vec<String>>> {
        self.read(|f| done(f.lacksglyphs(text)))
    }

    /// SWIDTH from DWIDTH (`Font::swidth`).
    pub fn swidth(&self, dwx: JsValue) -> JsResult<JsValue> {
        let dwx: i64 = conv::int_from_js(&dwx, "dwx")?;
        self.read(|f| Ok(conv::int_to_js(i128::from(f.swidth(dwx)))))
    }

    /// Add glyph metadata (`Font::addglyph`).
    pub fn addglyph(
        &self,
        #[wasm_bindgen(unchecked_param_type = "Record<string, unknown>")] fields: JsValue,
        replace: bool,
    ) -> JsResult<()> {
        let m = js2obj(&fields)?;
        self.write(|f| done(f.addglyph(&m, replace).map(|_| ())))
    }

    /// Add and attach a glyph handle (`Font::add_handle`).
    #[wasm_bindgen(js_name = addHandle)]
    pub fn add_handle(&self, glyph: &JsCompatGlyph, replace: bool) -> JsResult<()> {
        let mut g = handles::borrow_mut(&glyph.glyph, "BdfparserRawGlyph")?;
        self.write(|f| done(f.add_handle(&mut g, replace).map(|_| ())))
    }

    /// Remove a code; missing codes are a no-op (`Font::removeglyph`).
    pub fn removeglyph(&self, cp: JsValue) -> JsResult<()> {
        let cp: i64 = conv::int_from_js(&cp, "cp")?;
        self.write(|f| {
            f.removeglyph(cp);
            Ok(())
        })
    }

    /// Remove exactly a handle's slot (`Font::remove_handle`).
    #[wasm_bindgen(js_name = removeHandle)]
    pub fn remove_handle(&self, glyph: &JsCompatGlyph) -> JsResult<()> {
        let mut g = handles::borrow_mut(&glyph.glyph, "BdfparserRawGlyph")?;
        self.write(|f| {
            f.remove_handle(&mut g);
            Ok(())
        })
    }

    /// Create an attached glyph (`Font::newglyph`).
    pub fn newglyph(
        &self,
        cp: JsValue,
        bitmap: Option<JsCompatBitmap>,
        mode: f64,
        name: Option<String>,
        dwidth: JsValue,
    ) -> JsResult<JsCompatGlyph> {
        let cp: i64 = conv::int_from_js(&cp, "cp")?;
        let mode: i64 = conv::int(mode, "mode")?;
        let dwidth = opt_int(&dwidth, "dwidth")?;
        let b = bitmap.map(|b| b.snapshot()).transpose()?;
        self.write(|f| {
            done(f.newglyph(cp, b.as_ref(), mode, name.as_deref(), dwidth)).map(JsCompatGlyph::wrap)
        })
    }

    /// FONTBOUNDINGBOX := union of the positive glyph boxes (`Font::updatebbox`).
    pub fn updatebbox(&self) -> JsResult<()> {
        self.write(|f| done(f.updatebbox().map(|_| ())))
    }

    /// Deep copy restricted to a range (`Font::subset`).
    pub fn subset(&self, r: JsValue) -> JsResult<JsCompatFont> {
        let r = js2v(&r)?;
        self.read(|f| done(f.subset(&r)).map(Self::wrap))
    }

    /// Copy the encoded glyphs of another font (`Font::merge`).
    pub fn merge(&self, other: &JsCompatFont, replace: bool) -> JsResult<()> {
        let other = other.read(|o| Ok(o.clone()))?;
        self.write(|f| done(f.merge(&other, replace).map(|_| ())))
    }

    /// Apply a JS bitmap function `(raw: BdfparserRawBitmap) => void` to
    /// glyphs (`Font::transform`): the function edits the bitmap handle it
    /// receives; the JS wrapper copies a returned bitmap into it. Its
    /// exception is rethrown after Rust's partial-mutation order.
    pub fn transform(&self, function: Function, r: JsValue, mode: f64) -> JsResult<()> {
        let r = js2v(&r)?;
        let mode: i64 = conv::int(mode, "mode")?;
        self.write(|f| {
            done(
                f.transform(|b| crate::compat_bitmap::call_bitmap_fn(&function, b), &r, mode)
                    .map(|_| ()),
            )
        })
    }

    /// Re-encode to another charset (`Font::charsetconvert`).
    pub fn charsetconvert(&self, name: &str) -> JsResult<()> {
        self.write(|f| done(f.charsetconvert(name).map(|_| ())))
    }

    /// Canonical full model: headers, props, propnames, glyphs, unencoded,
    /// charset (`Font::model`).
    #[wasm_bindgen(unchecked_return_type = "Record<string, unknown>")]
    pub fn model(&self) -> JsResult<JsValue> {
        self.read(|f| v2js(&f.model()))
    }

    /// Serialize BDF text (`Font::tobdf`).
    pub fn tobdf(&self) -> JsResult<String> {
        self.read(|f| done(f.tobdf()))
    }

    /// Serialize PCF with `PcfWriteOptions` plain data (`Font::topcf`).
    pub fn topcf(
        &self,
        #[wasm_bindgen(unchecked_optional_param_type = "PcfWriteOptions")] options: Option<JsValue>,
    ) -> JsResult<Uint8Array> {
        let o = pcf_options(&options.unwrap_or(JsValue::UNDEFINED))?;
        self.read(|f| done(f.topcf(&o)).map(|b| conv::bytes_to_js(&b)))
    }

    /// Serialize TrueType with `TtfOptions` plain data (`Font::tottf`).
    pub fn tottf(
        &self,
        #[wasm_bindgen(unchecked_optional_param_type = "TtfOptions")] options: Option<JsValue>,
    ) -> JsResult<Uint8Array> {
        let o = ttf_options(&options.unwrap_or(JsValue::UNDEFINED))?;
        self.read(|f| done(f.tottf(&o)).map(|b| conv::bytes_to_js(&b)))
    }

    /// Serialize an SVG font (`Font::tosvgfont`).
    pub fn tosvgfont(
        &self,
        xoff: f64,
        yoff: f64,
        controlcharnoskip: bool,
        unicodeformat: &str,
    ) -> JsResult<String> {
        self.read(|f| done(f.tosvgfont(xoff, yoff, controlcharnoskip, unicodeformat)))
    }

    /// Serialize to a writer object `{write(bytes: Uint8Array)}`; `format`
    /// `bdf`, `pcf`, `ttf` or `svg` with the writer's plain-data options
    /// (`Font::save_to`).
    #[wasm_bindgen(js_name = saveTo)]
    pub fn save_to(
        &self,
        #[wasm_bindgen(unchecked_param_type = "{ write(bytes: Uint8Array): unknown }")]
        writer: JsValue,
        format: &str,
        options: Option<JsValue>,
    ) -> JsResult<()> {
        let o = save_options(format, &options.unwrap_or(JsValue::UNDEFINED))?;
        self.read(|f| done(f.save_to(JsWriter(writer), &o)))
    }

    /// Save to a path (`Font::save`): unavailable, the package never writes
    /// paths (capability `filesystem`); use `saveTo` or the byte outputs.
    pub fn save(&self, _path: &str) -> JsResult<()> {
        Err(crate::capabilities::unsupported(
            "filesystem",
            "BdfparserRawFont.save",
        ))
    }

    /// Draw explicit codes with SPEC draw keywords (`linelimit`, `mode`,
    /// `direction`, `ucgs`, `missing`) (`Font::drawcps`).
    pub fn drawcps(
        &self,
        cps: Vec<f64>,
        #[wasm_bindgen(unchecked_param_type = "Record<string, unknown>")] kw: JsValue,
    ) -> JsResult<JsCompatBitmap> {
        let cps = cps
            .into_iter()
            .map(|c| conv::int::<i64>(c, "cps"))
            .collect::<JsResult<Vec<_>>>()?;
        let kw = js2obj(&kw)?;
        self.read(|f| done(f.drawcps(&cps, &kw)).map(JsCompatBitmap::wrap))
    }

    /// Draw text after charset conversion (`Font::draw`).
    pub fn draw(
        &self,
        text: &str,
        #[wasm_bindgen(unchecked_param_type = "Record<string, unknown>")] kw: JsValue,
    ) -> JsResult<JsCompatBitmap> {
        let kw = js2obj(&kw)?;
        self.read(|f| done(f.draw(text, &kw)).map(JsCompatBitmap::wrap))
    }

    /// Draw all filtered glyphs (`order`, `r` plus the draw keywords) (`Font::drawall`).
    pub fn drawall(
        &self,
        #[wasm_bindgen(unchecked_param_type = "Record<string, unknown>")] kw: JsValue,
    ) -> JsResult<JsCompatBitmap> {
        let kw = js2obj(&kw)?;
        self.read(|f| done(f.drawall(&kw)).map(JsCompatBitmap::wrap))
    }
}
