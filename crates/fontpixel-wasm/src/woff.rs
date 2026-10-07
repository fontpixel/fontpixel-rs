//! `woff`: standalone sfnt ↔ WOFF1 (`woff.woff1`) and WOFF2 (`woff.woff2`,
//! including collections) containers (`fontpixel::woff`).
//!
//! Input and output fonts are bytes (`BytesLike` in, `Uint8Array` out).
//! `decode` returns `{sfnt, auxiliary: {major_version, minor_version,
//! metadata, private_data}, report}` where `sfnt`, `metadata` and
//! `private_data` are `Uint8Array`s (the blocks `null` when absent);
//! `encode` returns `{bytes, report}`. Options and limits are plain objects
//! (see `woff.defaults()`); the `auxiliary` option accepts `Uint8Array`s or
//! number arrays for its blocks.
//!
//! Threads: `compression_threads` (default 2) selects the chunked parallel
//! Brotli layout for streams of at least 128 KiB; WebAssembly has no
//! threads, so the chunks are compressed one after another with the same
//! chunking and dictionaries (decisions.md, "wasm32 readiness of core and
//! ops"). `encodeMany(…, jobs)` likewise runs its jobs in input order with
//! the native validation and budgets.
//!
//! Byte identity with native WOFF2 output (open core issue): the WOFF1
//! output and the WOFF2 table layout are identical, and so are the Brotli
//! streams of small inputs, but brotli 9 estimates costs with the platform
//! libm's `f32::log2` for symbol counts of 256 and more (fontpixel enables
//! brotli `std` without `portable-float`), and the wasm32 libm differs from
//! glibc in the last bit for some arguments. Larger WOFF2 streams therefore
//! compress to different (equally valid, losslessly decoding) bytes until
//! the core enables `portable-float` on every target; see
//! `tests/woff.test.mjs` ("woff2 byte identity …", marked TODO).
use crate::conv;
use crate::errors::{IntoJs, JsResult};
use crate::sfnt::byte_list;
use crate::sfnt_support::{bytes_js, int, object, opt, optional_bytes, optional_int};
use fontpixel::woff::{self, Auxiliary, Decoded, Encoded, woff1, woff2};
use js_sys::Array;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(typescript_custom_section)]
const WOFF_TS: &str = r#"
/** `fontpixel::woff::Auxiliary` with the blocks as `Uint8Array`s (`null` when absent). */
export interface AuxiliaryBytes {
  major_version: number;
  minor_version: number;
  metadata: Uint8Array | null;
  private_data: Uint8Array | null;
}

/** `fontpixel::woff::Decoded`: the reconstructed sfnt (or `ttcf` collection), auxiliary data and report. */
export interface WoffDecoded {
  sfnt: Uint8Array;
  auxiliary: AuxiliaryBytes;
  report: Report;
}
"#;

/// The `Auxiliary` plain object with the byte blocks as `Uint8Array`s.
pub fn auxiliary(a: &Auxiliary) -> JsResult<JsValue> {
    object(&[
        (
            "major_version",
            JsValue::from_f64(f64::from(a.major_version)),
        ),
        (
            "minor_version",
            JsValue::from_f64(f64::from(a.minor_version)),
        ),
        ("metadata", optional_bytes(&a.metadata)),
        ("private_data", optional_bytes(&a.private_data)),
    ])
}

fn decoded(d: &Decoded) -> JsResult<JsValue> {
    object(&[
        ("sfnt", bytes_js(&d.sfnt)),
        ("auxiliary", auxiliary(&d.auxiliary)?),
        ("report", conv::to_js(&d.report)?),
    ])
}

fn encoded(e: &Encoded) -> JsResult<JsValue> {
    object(&[
        ("bytes", bytes_js(&e.bytes)),
        ("report", conv::to_js(&e.report)?),
    ])
}

fn input(data: &JsValue) -> JsResult<Vec<u8>> {
    conv::bytes_from_js(data, "data")
}

/// `woff1::decode`: reconstruct the standalone sfnt from WOFF1 bytes.
#[wasm_bindgen(js_namespace = ["woff", "woff1"], js_name = decode, unchecked_return_type = "WoffDecoded")]
pub fn woff1_decode(
    #[wasm_bindgen(unchecked_param_type = "BytesLike")] data: JsValue,
    #[wasm_bindgen(unchecked_optional_param_type = "WoffLimits")] limits: Option<JsValue>,
) -> JsResult<JsValue> {
    let l: woff::Limits = opt(limits)?;
    decoded(&woff1::decode(&input(&data)?, &l).js()?)
}

/// `woff1::encode`: deterministic WOFF1 of a standalone sfnt.
#[wasm_bindgen(js_namespace = ["woff", "woff1"], js_name = encode, unchecked_return_type = "BytesWithReport")]
pub fn woff1_encode(
    #[wasm_bindgen(unchecked_param_type = "BytesLike")] data: JsValue,
    #[wasm_bindgen(unchecked_optional_param_type = "Woff1Options")] options: Option<JsValue>,
    #[wasm_bindgen(unchecked_optional_param_type = "WoffLimits")] limits: Option<JsValue>,
) -> JsResult<JsValue> {
    let o: woff1::Options = opt(options)?;
    let l: woff::Limits = opt(limits)?;
    encoded(&woff1::encode(&input(&data)?, &o, &l).js()?)
}

/// `woff2::decode`: reconstruct the sfnt (or `ttcf` collection) from WOFF2.
#[wasm_bindgen(js_namespace = ["woff", "woff2"], js_name = decode, unchecked_return_type = "WoffDecoded")]
pub fn woff2_decode(
    #[wasm_bindgen(unchecked_param_type = "BytesLike")] data: JsValue,
    #[wasm_bindgen(unchecked_optional_param_type = "WoffLimits")] limits: Option<JsValue>,
) -> JsResult<JsValue> {
    let l: woff::Limits = opt(limits)?;
    decoded(&woff2::decode(&input(&data)?, &l).js()?)
}

/// `woff2::encode`: deterministic WOFF2 of an sfnt or `ttcf` collection.
/// The Brotli stream of larger inputs can differ from native output (see the
/// module documentation); it always decodes to the same font.
#[wasm_bindgen(js_namespace = ["woff", "woff2"], js_name = encode, unchecked_return_type = "BytesWithReport")]
pub fn woff2_encode(
    #[wasm_bindgen(unchecked_param_type = "BytesLike")] data: JsValue,
    #[wasm_bindgen(unchecked_optional_param_type = "Woff2Options")] options: Option<JsValue>,
    #[wasm_bindgen(unchecked_optional_param_type = "WoffLimits")] limits: Option<JsValue>,
) -> JsResult<JsValue> {
    let o: woff2::Options = opt(options)?;
    let l: woff::Limits = opt(limits)?;
    encoded(&woff2::encode(&input(&data)?, &o, &l).js()?)
}

/// `woff2::encode_many`: encode several fonts (bounded by
/// `max_batch_fonts`/`max_batch_bytes`); output order matches input order.
/// `jobs` (default 1) is the native worker count; here the jobs run in
/// input order with identical results.
#[wasm_bindgen(js_namespace = ["woff", "woff2"], js_name = encodeMany, unchecked_return_type = "BytesWithReport[]")]
pub fn woff2_encode_many(
    #[wasm_bindgen(unchecked_param_type = "BytesLike[]")] inputs: JsValue,
    #[wasm_bindgen(unchecked_optional_param_type = "Woff2Options")] options: Option<JsValue>,
    #[wasm_bindgen(unchecked_optional_param_type = "WoffLimits")] limits: Option<JsValue>,
    #[wasm_bindgen(unchecked_optional_param_type = "number")] jobs: Option<JsValue>,
) -> JsResult<Array> {
    let data = byte_list(&inputs, "inputs")?;
    let o: woff2::Options = opt(options)?;
    let l: woff::Limits = opt(limits)?;
    let jobs: usize = optional_int(jobs, 1, "jobs")?;
    let slices: Vec<&[u8]> = data.iter().map(Vec::as_slice).collect();
    let results = woff2::encode_many(&slices, &o, &l, jobs).js()?;
    results.iter().map(encoded).collect()
}

/// `woff2::brotli_encoder_heap_bound`: the admitted Brotli encoder heap
/// bound (proven for quality 0–9, an empirical envelope for 10–11), or
/// `undefined` on arithmetic overflow. WOFF2 always uses `lgwin = 22`.
///
/// The bound is computed for this target, as the encoder admits it here:
/// `usize` is 32-bit in WebAssembly (`inputLen`/`threads` beyond 2^32 − 1
/// throw `InvalidInputError`), and the terms that count Rust allocation
/// sizes use wasm32's smaller pointers, so some values are below the 64-bit
/// native ones (the quality 10–11 envelope is the same).
#[wasm_bindgen(js_namespace = ["woff", "woff2"], js_name = brotliEncoderHeapBound)]
pub fn brotli_encoder_heap_bound(
    quality: f64,
    lgwin: f64,
    #[wasm_bindgen(js_name = inputLen)] input_len: f64,
    threads: f64,
) -> JsResult<Option<usize>> {
    Ok(woff2::brotli_encoder_heap_bound(
        int(quality, "quality")?,
        int(lgwin, "lgwin")?,
        int(input_len, "inputLen")?,
        int(threads, "threads")?,
    ))
}

/// `woff2::brotli_encoder_proven_heap_bound`: the source-derived upper
/// bound of all encoder heap for the same call shape (computed for this
/// target, see `brotliEncoderHeapBound`), or `undefined` on overflow.
#[wasm_bindgen(js_namespace = ["woff", "woff2"], js_name = brotliEncoderProvenHeapBound)]
pub fn brotli_encoder_proven_heap_bound(
    quality: f64,
    lgwin: f64,
    #[wasm_bindgen(js_name = inputLen)] input_len: f64,
    threads: f64,
) -> JsResult<Option<usize>> {
    Ok(woff2::brotli_encoder_proven_heap_bound(
        int(quality, "quality")?,
        int(lgwin, "lgwin")?,
        int(input_len, "inputLen")?,
        int(threads, "threads")?,
    ))
}

/// Default limit and option objects of `woff` (`"Limits"`, `"Auxiliary"`,
/// `"woff1.Options"`, `"woff2.Options"`).
#[wasm_bindgen(js_namespace = woff, js_name = defaults, unchecked_return_type = "Record<string, unknown>")]
pub fn woff_defaults() -> JsResult<JsValue> {
    object(&[
        ("Limits", conv::to_js(&woff::Limits::default())?),
        ("Auxiliary", conv::to_js(&Auxiliary::default())?),
        ("woff1.Options", conv::to_js(&woff1::Options::default())?),
        ("woff2.Options", conv::to_js(&woff2::Options::default())?),
    ])
}
