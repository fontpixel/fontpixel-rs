//! Table codecs under `sfnt`: `sfnt.cmap`, `sfnt.name`, `sfnt.os2`,
//! `sfnt.post`, `sfnt.metrics`, `sfnt.gasp`, `sfnt.gsub`, `sfnt.glyf` and
//! `sfnt.bitmap` (EBDT/EBLC, bdat/bloc, CBDT/CBLC, EBSC and sbix).
//!
//! Tables are plain data with the Rust field names (`NameTable`,
//! `Os2Table`, `GlyfTable`, …). Encoders return `Uint8Array`s; results with
//! several byte fields are objects whose byte fields are `Uint8Array`s. Rust
//! methods on value types (`NameRecord::text`, `Header::recompute`,
//! `Gsub::closure`, …) are namespace functions that take the plain data and
//! return a new value; the input object is never mutated. Results Python
//! returns as tuples are arrays (`kindTags`) or objects (`metrics.decode` →
//! `{header, metrics}`).
use crate::conv;
use crate::errors::{IntoJs, JsResult};
use crate::sfnt::sfnt_arg;
use crate::sfnt_support::{
    byte_result, int, mono, object, opt, optional_int, req, tag_str, with_bytes,
};
use fontpixel::sfnt::{
    self, bitmap, cmap, gasp::GaspTable, glyf, gsub, metrics, name, os2, post::PostTable,
};
use js_sys::{Array, Uint8Array};
use std::collections::{BTreeMap, BTreeSet};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

fn bytes_of(data: &JsValue, what: &str) -> JsResult<Vec<u8>> {
    conv::bytes_from_js(data, what)
}

// ---------------------------------------------------------------- cmap

fn encoded_cmap(mut e: cmap::EncodedCmap) -> JsResult<JsValue> {
    let data = std::mem::take(&mut e.data);
    with_bytes(&e, &[("data", &data)])
}

/// `cmap::encode`: encode `{codepoint: glyphId}` (object or `Map`) with the
/// default record set. Returns `{data, has_bmp_subtable}`.
#[wasm_bindgen(js_namespace = ["sfnt", "cmap"], js_name = encode, unchecked_return_type = "BytesFields<EncodedCmap, \"data\">")]
pub fn cmap_encode(
    #[wasm_bindgen(unchecked_param_type = "Record<string, number> | Map<number, number>")]
    mapping: JsValue,
    #[wasm_bindgen(js_name = numGlyphs)] num_glyphs: f64,
    #[wasm_bindgen(unchecked_optional_param_type = "SfntLimits")] limits: Option<JsValue>,
) -> JsResult<JsValue> {
    let m: cmap::Mapping = req(&mapping)?;
    let n: u16 = int(num_glyphs, "numGlyphs")?;
    let l: sfnt::Limits = opt(limits)?;
    encoded_cmap(cmap::encode(&m, n, &l).js()?)
}

/// `cmap::encode_records`: as `encode`, optionally adding the Unicode full
/// record even for BMP-only mappings.
#[wasm_bindgen(js_namespace = ["sfnt", "cmap"], js_name = encodeRecords, unchecked_return_type = "BytesFields<EncodedCmap, \"data\">")]
pub fn cmap_encode_records(
    #[wasm_bindgen(unchecked_param_type = "Record<string, number> | Map<number, number>")]
    mapping: JsValue,
    #[wasm_bindgen(js_name = numGlyphs)] num_glyphs: f64,
    #[wasm_bindgen(js_name = unicodeFullForBmp)] unicode_full_for_bmp: bool,
    #[wasm_bindgen(unchecked_optional_param_type = "SfntLimits")] limits: Option<JsValue>,
) -> JsResult<JsValue> {
    let m: cmap::Mapping = req(&mapping)?;
    let n: u16 = int(num_glyphs, "numGlyphs")?;
    let l: sfnt::Limits = opt(limits)?;
    encoded_cmap(cmap::encode_records(&m, n, &l, unicode_full_for_bmp).js()?)
}

/// `cmap::encode_with_records`: choose the full-repertoire records
/// (`"always"`, `"non-bmp"` or `"when-needed"`).
#[wasm_bindgen(js_namespace = ["sfnt", "cmap"], js_name = encodeWithRecords, unchecked_return_type = "BytesFields<EncodedCmap, \"data\">")]
pub fn cmap_encode_with_records(
    #[wasm_bindgen(unchecked_param_type = "Record<string, number> | Map<number, number>")]
    mapping: JsValue,
    #[wasm_bindgen(js_name = numGlyphs)] num_glyphs: f64,
    #[wasm_bindgen(unchecked_param_type = "FullRecords")] records: JsValue,
    #[wasm_bindgen(unchecked_optional_param_type = "SfntLimits")] limits: Option<JsValue>,
) -> JsResult<JsValue> {
    let m: cmap::Mapping = req(&mapping)?;
    let n: u16 = int(num_glyphs, "numGlyphs")?;
    let r: cmap::FullRecords = req(&records)?;
    let l: sfnt::Limits = opt(limits)?;
    encoded_cmap(cmap::encode_with_records(&m, n, &l, r).js()?)
}

/// `cmap::decode`: the preferred Unicode subtable as `{codepoint: glyphId}`
/// (decimal-string keys).
#[wasm_bindgen(js_namespace = ["sfnt", "cmap"], js_name = decode, unchecked_return_type = "Record<string, number>")]
pub fn cmap_decode(
    #[wasm_bindgen(unchecked_param_type = "BytesLike")] data: JsValue,
    #[wasm_bindgen(js_name = numGlyphs)] num_glyphs: f64,
    #[wasm_bindgen(unchecked_optional_param_type = "SfntLimits")] limits: Option<JsValue>,
) -> JsResult<JsValue> {
    let n: u16 = int(num_glyphs, "numGlyphs")?;
    let l: sfnt::Limits = opt(limits)?;
    conv::to_js(&cmap::decode(&bytes_of(&data, "data")?, n, &l).js()?)
}

// ---------------------------------------------------------------- name

/// `NameTable::decode`: a `NameTable` (`{records, language_tags}`).
#[wasm_bindgen(js_namespace = ["sfnt", "name"], js_name = decode, unchecked_return_type = "NameTable")]
pub fn name_decode(
    #[wasm_bindgen(unchecked_param_type = "BytesLike")] data: JsValue,
    #[wasm_bindgen(unchecked_optional_param_type = "SfntLimits")] limits: Option<JsValue>,
) -> JsResult<JsValue> {
    let l: sfnt::Limits = opt(limits)?;
    conv::to_js(&name::NameTable::decode(&bytes_of(&data, "data")?, &l).js()?)
}

/// `NameTable::encode`: canonical sorted name table bytes.
#[wasm_bindgen(js_namespace = ["sfnt", "name"], js_name = encode)]
pub fn name_encode(
    #[wasm_bindgen(unchecked_param_type = "NameTable")] table: JsValue,
    #[wasm_bindgen(unchecked_optional_param_type = "SfntLimits")] limits: Option<JsValue>,
) -> JsResult<Uint8Array> {
    let t: name::NameTable = req(&table)?;
    let l: sfnt::Limits = opt(limits)?;
    Ok(byte_result(&t.encode(&l).js()?))
}

/// `NameTable::encode_configured`: `validateConfigured`, then encode.
#[wasm_bindgen(js_namespace = ["sfnt", "name"], js_name = encodeConfigured)]
pub fn name_encode_configured(
    #[wasm_bindgen(unchecked_param_type = "NameTable")] table: JsValue,
    #[wasm_bindgen(unchecked_optional_param_type = "SfntLimits")] limits: Option<JsValue>,
) -> JsResult<Uint8Array> {
    let t: name::NameTable = req(&table)?;
    let l: sfnt::Limits = opt(limits)?;
    Ok(byte_result(&t.encode_configured(&l).js()?))
}

/// `NameTable::validate_configured`: check platform/encoding/language/ID
/// rules of a caller-supplied table; throws `InvalidInputError`.
#[wasm_bindgen(js_namespace = ["sfnt", "name"], js_name = validateConfigured)]
pub fn name_validate_configured(
    #[wasm_bindgen(unchecked_param_type = "NameTable")] table: JsValue,
    #[wasm_bindgen(unchecked_optional_param_type = "SfntLimits")] limits: Option<JsValue>,
) -> JsResult<()> {
    let t: name::NameTable = req(&table)?;
    let l: sfnt::Limits = opt(limits)?;
    t.validate_configured(&l).js()
}

/// `NameTable::retained_bytes`: retained logical storage of the table as
/// deserialized from `table` (spare capacity included), bounded by
/// `maximum` (`ResourceLimitError` above it).
#[wasm_bindgen(js_namespace = ["sfnt", "name"], js_name = retainedBytes)]
pub fn name_retained_bytes(
    #[wasm_bindgen(unchecked_param_type = "NameTable")] table: JsValue,
    maximum: f64,
) -> JsResult<usize> {
    let t: name::NameTable = req(&table)?;
    t.retained_bytes(int(maximum, "maximum")?).js()
}

/// `NameRecord::unicode`: a Windows Unicode (3, 1) `NameRecord`.
#[wasm_bindgen(js_namespace = ["sfnt", "name"], js_name = recordUnicode, unchecked_return_type = "NameRecord")]
pub fn name_record_unicode(
    #[wasm_bindgen(js_name = nameId)] name_id: f64,
    #[wasm_bindgen(js_name = languageId)] language_id: f64,
    value: &str,
) -> JsResult<JsValue> {
    conv::to_js(&name::NameRecord::unicode(
        int(name_id, "nameId")?,
        int(language_id, "languageId")?,
        value,
    ))
}

/// `NameRecord::text`: the decoded text of a record, or `undefined` for
/// encodings without a text decoding.
#[wasm_bindgen(js_namespace = ["sfnt", "name"], js_name = recordText)]
pub fn name_record_text(
    #[wasm_bindgen(unchecked_param_type = "NameRecord")] record: JsValue,
) -> JsResult<Option<String>> {
    let r: name::NameRecord = req(&record)?;
    r.text().js()
}

/// `name::language_tag_well_formed`: RFC 5646 well-formedness only.
#[wasm_bindgen(js_namespace = ["sfnt", "name"], js_name = languageTagWellFormed)]
pub fn name_language_tag_well_formed(tag: &str) -> bool {
    name::language_tag_well_formed(tag)
}

// ---------------------------------------------------------------- OS/2

/// `Os2Table::decode`: an `Os2Table`.
#[wasm_bindgen(js_namespace = ["sfnt", "os2"], js_name = decode, unchecked_return_type = "Os2Table")]
pub fn os2_decode(
    #[wasm_bindgen(unchecked_param_type = "BytesLike")] data: JsValue,
    #[wasm_bindgen(unchecked_optional_param_type = "SfntLimits")] limits: Option<JsValue>,
) -> JsResult<JsValue> {
    let l: sfnt::Limits = opt(limits)?;
    conv::to_js(&os2::Os2Table::decode(&bytes_of(&data, "data")?, &l).js()?)
}

/// `Os2Table::encode`: OS/2 table bytes of the table's version.
#[wasm_bindgen(js_namespace = ["sfnt", "os2"], js_name = encode)]
pub fn os2_encode(
    #[wasm_bindgen(unchecked_param_type = "Os2Table")] table: JsValue,
    #[wasm_bindgen(unchecked_optional_param_type = "SfntLimits")] limits: Option<JsValue>,
) -> JsResult<Uint8Array> {
    let t: os2::Os2Table = req(&table)?;
    let l: sfnt::Limits = opt(limits)?;
    Ok(byte_result(&t.encode(&l).js()?))
}

fn codepoint_set(value: &JsValue) -> JsResult<BTreeSet<u32>> {
    if let Some(set) = value.dyn_ref::<js_sys::Set>() {
        let items: Array = Array::from(set);
        return req(&items);
    }
    req(value)
}

/// `Os2Table::set_unicode_coverage`: a copy of `table` with Unicode ranges
/// and first/last character indexes derived from `codepoints` (array or
/// `Set` of numbers).
#[wasm_bindgen(js_namespace = ["sfnt", "os2"], js_name = setUnicodeCoverage, unchecked_return_type = "Os2Table")]
pub fn os2_set_unicode_coverage(
    #[wasm_bindgen(unchecked_param_type = "Os2Table")] table: JsValue,
    #[wasm_bindgen(unchecked_param_type = "Iterable<number>")] codepoints: JsValue,
) -> JsResult<JsValue> {
    let mut t: os2::Os2Table = req(&table)?;
    t.set_unicode_coverage(&codepoint_set(&codepoints)?).js()?;
    conv::to_js(&t)
}

/// `os2::unicode_ranges`: the four ulUnicodeRange words.
#[wasm_bindgen(js_namespace = ["sfnt", "os2"], js_name = unicodeRanges, unchecked_return_type = "[number, number, number, number]")]
pub fn os2_unicode_ranges(
    #[wasm_bindgen(unchecked_param_type = "Iterable<number>")] codepoints: JsValue,
) -> JsResult<JsValue> {
    conv::to_js(&os2::unicode_ranges(&codepoint_set(&codepoints)?).js()?)
}

/// `os2::code_page_ranges`: the two ulCodePageRange words for `codepoints`
/// and explicitly declared Windows code `pages`.
#[wasm_bindgen(js_namespace = ["sfnt", "os2"], js_name = codePageRanges, unchecked_return_type = "[number, number]")]
pub fn os2_code_page_ranges(
    #[wasm_bindgen(unchecked_param_type = "Iterable<number>")] codepoints: JsValue,
    #[wasm_bindgen(unchecked_param_type = "number[]")] pages: JsValue,
) -> JsResult<JsValue> {
    let pages: Vec<u16> = req(&pages)?;
    conv::to_js(&os2::code_page_ranges(&codepoint_set(&codepoints)?, &pages).js()?)
}

/// `os2::average_width`: xAvgCharWidth of the given advances.
#[wasm_bindgen(js_namespace = ["sfnt", "os2"], js_name = averageWidth)]
pub fn os2_average_width(
    #[wasm_bindgen(unchecked_param_type = "number[]")] advances: JsValue,
) -> JsResult<i16> {
    let advances: Vec<u16> = req(&advances)?;
    os2::average_width(advances).js()
}

/// `os2::check_embedding_flags`: `undefined` when valid, else the reason.
#[wasm_bindgen(js_namespace = ["sfnt", "os2"], js_name = checkEmbeddingFlags)]
pub fn os2_check_embedding_flags(flags: f64) -> JsResult<Option<String>> {
    Ok(os2::check_embedding_flags(int(flags, "flags")?)
        .err()
        .map(str::to_owned))
}

/// `os2::check_selection_flags`: `undefined` when valid for `version`, else
/// the reason.
#[wasm_bindgen(js_namespace = ["sfnt", "os2"], js_name = checkSelectionFlags)]
pub fn os2_check_selection_flags(flags: f64, version: f64) -> JsResult<Option<String>> {
    Ok(
        os2::check_selection_flags(int(flags, "flags")?, int(version, "version")?)
            .err()
            .map(str::to_owned),
    )
}

/// `os2::check_code_page_ranges`: `undefined` when valid, else the reason.
#[wasm_bindgen(js_namespace = ["sfnt", "os2"], js_name = checkCodePageRanges)]
pub fn os2_check_code_page_ranges(
    #[wasm_bindgen(unchecked_param_type = "[number, number]")] ranges: JsValue,
) -> JsResult<Option<String>> {
    let r: [u32; 2] = req(&ranges)?;
    Ok(os2::check_code_page_ranges(r).err().map(str::to_owned))
}

// ---------------------------------------------------------------- post

/// `PostTable::decode`: a `PostTable`.
#[wasm_bindgen(js_namespace = ["sfnt", "post"], js_name = decode, unchecked_return_type = "PostTable")]
pub fn post_decode(
    #[wasm_bindgen(unchecked_param_type = "BytesLike")] data: JsValue,
    #[wasm_bindgen(js_name = numGlyphs)] num_glyphs: f64,
    #[wasm_bindgen(unchecked_optional_param_type = "SfntLimits")] limits: Option<JsValue>,
) -> JsResult<JsValue> {
    let l: sfnt::Limits = opt(limits)?;
    let n: u16 = int(num_glyphs, "numGlyphs")?;
    conv::to_js(&PostTable::decode(&bytes_of(&data, "data")?, n, &l).js()?)
}

/// `PostTable::encode`: post table bytes.
#[wasm_bindgen(js_namespace = ["sfnt", "post"], js_name = encode)]
pub fn post_encode(
    #[wasm_bindgen(unchecked_param_type = "PostTable")] table: JsValue,
    #[wasm_bindgen(js_name = numGlyphs)] num_glyphs: f64,
    #[wasm_bindgen(unchecked_optional_param_type = "SfntLimits")] limits: Option<JsValue>,
) -> JsResult<Uint8Array> {
    let t: PostTable = req(&table)?;
    let l: sfnt::Limits = opt(limits)?;
    Ok(byte_result(
        &t.encode(int(num_glyphs, "numGlyphs")?, &l).js()?,
    ))
}

// ---------------------------------------------------------------- metrics

/// `metrics::Header::new`: a default header for `axis` (`"horizontal"` or
/// `"vertical"`).
#[wasm_bindgen(js_namespace = ["sfnt", "metrics"], js_name = headerNew, unchecked_return_type = "MetricsHeader")]
pub fn metrics_header_new(
    #[wasm_bindgen(unchecked_param_type = "Axis")] axis: JsValue,
) -> JsResult<JsValue> {
    conv::to_js(&metrics::Header::new(req(&axis)?))
}

/// `Header::recompute`: a copy of `header` with extents recomputed from
/// `metrics` (`LongMetric[]`) and per-glyph ink extents (`null` = blank).
#[wasm_bindgen(js_namespace = ["sfnt", "metrics"], js_name = headerRecompute, unchecked_return_type = "MetricsHeader")]
pub fn metrics_header_recompute(
    #[wasm_bindgen(unchecked_param_type = "MetricsHeader")] header: JsValue,
    #[wasm_bindgen(unchecked_param_type = "LongMetric[]")] metrics: JsValue,
    #[wasm_bindgen(js_name = inkExtents, unchecked_param_type = "(number | null)[]")]
    ink_extents: JsValue,
) -> JsResult<JsValue> {
    let mut h: metrics::Header = req(&header)?;
    let m: Vec<metrics::LongMetric> = req(&metrics)?;
    let ink: Vec<Option<u16>> = req(&ink_extents)?;
    h.recompute(&m, &ink).js()?;
    conv::to_js(&h)
}

/// `metrics::encode`: `{header, metrics, num_long_metrics}` with the two
/// tables as `Uint8Array`s.
#[wasm_bindgen(js_namespace = ["sfnt", "metrics"], js_name = encode, unchecked_return_type = "BytesFields<EncodedMetrics, \"header\" | \"metrics\">")]
pub fn metrics_encode(
    #[wasm_bindgen(unchecked_param_type = "Axis")] axis: JsValue,
    #[wasm_bindgen(unchecked_param_type = "MetricsHeader")] header: JsValue,
    #[wasm_bindgen(unchecked_param_type = "LongMetric[]")] metrics: JsValue,
    compact: bool,
    #[wasm_bindgen(unchecked_optional_param_type = "SfntLimits")] limits: Option<JsValue>,
) -> JsResult<JsValue> {
    let h: metrics::Header = req(&header)?;
    let m: Vec<metrics::LongMetric> = req(&metrics)?;
    let l: sfnt::Limits = opt(limits)?;
    let mut e = metrics::encode(req(&axis)?, &h, &m, compact, &l).js()?;
    let header = std::mem::take(&mut e.header);
    let table = std::mem::take(&mut e.metrics);
    with_bytes(&e, &[("header", &header), ("metrics", &table)])
}

/// `metrics::decode`: `{header, metrics}` from the header (hhea/vhea) and
/// metrics (hmtx/vmtx) tables.
#[wasm_bindgen(js_namespace = ["sfnt", "metrics"], js_name = decode, unchecked_return_type = "{ header: MetricsHeader; metrics: LongMetric[] }")]
pub fn metrics_decode(
    #[wasm_bindgen(unchecked_param_type = "Axis")] axis: JsValue,
    #[wasm_bindgen(unchecked_param_type = "BytesLike")] header: JsValue,
    #[wasm_bindgen(unchecked_param_type = "BytesLike")] data: JsValue,
    #[wasm_bindgen(js_name = numGlyphs)] num_glyphs: f64,
    #[wasm_bindgen(unchecked_optional_param_type = "SfntLimits")] limits: Option<JsValue>,
) -> JsResult<JsValue> {
    let l: sfnt::Limits = opt(limits)?;
    let (h, m) = metrics::decode(
        req(&axis)?,
        &bytes_of(&header, "header")?,
        &bytes_of(&data, "data")?,
        int(num_glyphs, "numGlyphs")?,
        &l,
    )
    .js()?;
    object(&[("header", conv::to_js(&h)?), ("metrics", conv::to_js(&m)?)])
}

// ---------------------------------------------------------------- gasp

/// `GaspTable::decode`: a `GaspTable`.
#[wasm_bindgen(js_namespace = ["sfnt", "gasp"], js_name = decode, unchecked_return_type = "GaspTable")]
pub fn gasp_decode(
    #[wasm_bindgen(unchecked_param_type = "BytesLike")] data: JsValue,
    #[wasm_bindgen(unchecked_optional_param_type = "SfntLimits")] limits: Option<JsValue>,
) -> JsResult<JsValue> {
    let l: sfnt::Limits = opt(limits)?;
    conv::to_js(&GaspTable::decode(&bytes_of(&data, "data")?, &l).js()?)
}

/// `GaspTable::encode`: gasp table bytes.
#[wasm_bindgen(js_namespace = ["sfnt", "gasp"], js_name = encode)]
pub fn gasp_encode(
    #[wasm_bindgen(unchecked_param_type = "GaspTable")] table: JsValue,
    #[wasm_bindgen(unchecked_optional_param_type = "SfntLimits")] limits: Option<JsValue>,
) -> JsResult<Uint8Array> {
    let t: GaspTable = req(&table)?;
    let l: sfnt::Limits = opt(limits)?;
    Ok(byte_result(&t.encode(&l).js()?))
}

// ---------------------------------------------------------------- GSUB

/// `Gsub::decode`: a `Gsub` (scripts keyed by tag strings).
#[wasm_bindgen(js_namespace = ["sfnt", "gsub"], js_name = decode, unchecked_return_type = "Gsub")]
pub fn gsub_decode(
    #[wasm_bindgen(unchecked_param_type = "BytesLike")] data: JsValue,
    #[wasm_bindgen(js_name = numGlyphs)] num_glyphs: f64,
    #[wasm_bindgen(unchecked_optional_param_type = "LayoutLimits")] limits: Option<JsValue>,
) -> JsResult<JsValue> {
    let l: gsub::LayoutLimits = opt(limits)?;
    let n: u16 = int(num_glyphs, "numGlyphs")?;
    conv::to_js(&gsub::Gsub::decode(&bytes_of(&data, "data")?, n, &l).js()?)
}

/// `Gsub::encode`: GSUB table bytes.
#[wasm_bindgen(js_namespace = ["sfnt", "gsub"], js_name = encode)]
pub fn gsub_encode(
    #[wasm_bindgen(unchecked_param_type = "Gsub")] gsub: JsValue,
    #[wasm_bindgen(js_name = numGlyphs)] num_glyphs: f64,
    #[wasm_bindgen(unchecked_optional_param_type = "LayoutLimits")] limits: Option<JsValue>,
) -> JsResult<Uint8Array> {
    let g: gsub::Gsub = req(&gsub)?;
    let l: gsub::LayoutLimits = opt(limits)?;
    Ok(byte_result(
        &g.encode(int(num_glyphs, "numGlyphs")?, &l).js()?,
    ))
}

/// `Gsub::closure`: `glyphs` (array or `Set`) plus every glyph reachable by
/// substitution, as a sorted array.
#[wasm_bindgen(js_namespace = ["sfnt", "gsub"], js_name = closure, unchecked_return_type = "number[]")]
pub fn gsub_closure(
    #[wasm_bindgen(unchecked_param_type = "Gsub")] gsub: JsValue,
    #[wasm_bindgen(unchecked_param_type = "Iterable<number>")] glyphs: JsValue,
) -> JsResult<JsValue> {
    let g: gsub::Gsub = req(&gsub)?;
    let mut set: BTreeSet<u16> = match glyphs.dyn_ref::<js_sys::Set>() {
        Some(s) => req(&Array::from(s))?,
        None => req(&glyphs)?,
    };
    g.closure(&mut set);
    conv::to_js(&set)
}

/// `Gsub::subset`: the `Gsub` remapped through `{oldId: newId}`.
#[wasm_bindgen(js_namespace = ["sfnt", "gsub"], js_name = subset, unchecked_return_type = "Gsub")]
pub fn gsub_subset(
    #[wasm_bindgen(unchecked_param_type = "Gsub")] gsub: JsValue,
    #[wasm_bindgen(unchecked_param_type = "Record<string, number> | Map<number, number>")]
    mapping: JsValue,
) -> JsResult<JsValue> {
    let g: gsub::Gsub = req(&gsub)?;
    let m: BTreeMap<u16, u16> = req(&mapping)?;
    conv::to_js(&g.subset(&m))
}

// ---------------------------------------------------------------- glyf/loca

/// `GlyfTable::decode`: a `GlyfTable` from glyf and loca bytes.
#[wasm_bindgen(js_namespace = ["sfnt", "glyf"], js_name = decode, unchecked_return_type = "GlyfTable")]
pub fn glyf_decode(
    #[wasm_bindgen(unchecked_param_type = "BytesLike")] glyf: JsValue,
    #[wasm_bindgen(unchecked_param_type = "BytesLike")] loca: JsValue,
    #[wasm_bindgen(js_name = numGlyphs)] num_glyphs: f64,
    #[wasm_bindgen(js_name = indexToLocFormat)] index_to_loc_format: f64,
    #[wasm_bindgen(unchecked_optional_param_type = "GlyfLimits")] limits: Option<JsValue>,
) -> JsResult<JsValue> {
    let l: glyf::Limits = opt(limits)?;
    let t = glyf::GlyfTable::decode(
        &bytes_of(&glyf, "glyf")?,
        &bytes_of(&loca, "loca")?,
        int(num_glyphs, "numGlyphs")?,
        int(index_to_loc_format, "indexToLocFormat")?,
        &l,
    )
    .js()?;
    conv::to_js(&t)
}

/// `GlyfTable::from_sfnt`: the `GlyfTable` of an `Sfnt` handle.
#[wasm_bindgen(js_namespace = ["sfnt", "glyf"], js_name = fromSfnt, unchecked_return_type = "GlyfTable")]
pub fn glyf_from_sfnt(
    #[wasm_bindgen(unchecked_param_type = "Sfnt")] font: JsValue,
    #[wasm_bindgen(unchecked_optional_param_type = "GlyfLimits")] limits: Option<JsValue>,
) -> JsResult<JsValue> {
    let font = sfnt_arg(&font)?;
    let l: glyf::Limits = opt(limits)?;
    font.with(|s| conv::to_js(&glyf::GlyfTable::from_sfnt(s, &l).js()?))
}

/// `GlyfTable::contours`: flattened contours of `glyphId` (composites
/// expanded).
#[wasm_bindgen(js_namespace = ["sfnt", "glyf"], js_name = contours, unchecked_return_type = "GlyfContour[]")]
pub fn glyf_contours(
    #[wasm_bindgen(unchecked_param_type = "GlyfTable")] table: JsValue,
    #[wasm_bindgen(js_name = glyphId)] glyph_id: f64,
    #[wasm_bindgen(unchecked_optional_param_type = "GlyfLimits")] limits: Option<JsValue>,
) -> JsResult<JsValue> {
    let t: glyf::GlyfTable = req(&table)?;
    let l: glyf::Limits = opt(limits)?;
    conv::to_js(&t.contours(int(glyph_id, "glyphId")?, &l).js()?)
}

/// `GlyfTable::encode`: `{glyf, loca, index_to_loc_format, bounds,
/// statistics}` with `glyf`/`loca` as `Uint8Array`s; `format` is
/// `"short"`, `"long"` or `"auto"`.
#[wasm_bindgen(js_namespace = ["sfnt", "glyf"], js_name = encode, unchecked_return_type = "BytesFields<GlyfEncoded, \"glyf\" | \"loca\">")]
pub fn glyf_encode(
    #[wasm_bindgen(unchecked_param_type = "GlyfTable")] table: JsValue,
    #[wasm_bindgen(unchecked_param_type = "LocaFormat")] format: JsValue,
    #[wasm_bindgen(unchecked_optional_param_type = "GlyfLimits")] limits: Option<JsValue>,
) -> JsResult<JsValue> {
    let t: glyf::GlyfTable = req(&table)?;
    let l: glyf::Limits = opt(limits)?;
    let mut e = t.encode(req(&format)?, &l).js()?;
    let g = std::mem::take(&mut e.glyf);
    let loca = std::mem::take(&mut e.loca);
    with_bytes(&e, &[("glyf", &g), ("loca", &loca)])
}

/// `Component::translated`: a component offset by `(x, y)`.
#[wasm_bindgen(js_namespace = ["sfnt", "glyf"], js_name = componentTranslated, unchecked_return_type = "Component")]
pub fn glyf_component_translated(
    #[wasm_bindgen(js_name = glyphId)] glyph_id: f64,
    x: f64,
    y: f64,
) -> JsResult<JsValue> {
    conv::to_js(&glyf::Component::translated(
        int(glyph_id, "glyphId")?,
        int(x, "x")?,
        int(y, "y")?,
    ))
}

/// `Glyph::simple`: a simple glyph from `[[x, y, onCurve], …]` contours.
#[wasm_bindgen(js_namespace = ["sfnt", "glyf"], js_name = glyphSimple, unchecked_return_type = "SfntGlyfGlyph")]
pub fn glyf_glyph_simple(
    #[wasm_bindgen(unchecked_param_type = "GlyfContour[]")] contours: JsValue,
) -> JsResult<JsValue> {
    conv::to_js(&glyf::Glyph::simple(req(&contours)?))
}

/// `Glyph::pixels`: a glyph tracing a Mono `Bitmap` handle or
/// `GlyphBitmap` plain data (Dot shapes reference `sharedGlyph`).
#[wasm_bindgen(js_namespace = ["sfnt", "glyf"], js_name = glyphPixels, unchecked_return_type = "SfntGlyfGlyph")]
pub fn glyf_glyph_pixels(
    #[wasm_bindgen(unchecked_param_type = "Bitmap | GlyphBitmap")] bitmap: JsValue,
    #[wasm_bindgen(unchecked_optional_param_type = "OutlineOptions")] options: Option<JsValue>,
    #[wasm_bindgen(js_name = sharedGlyph, unchecked_optional_param_type = "number")]
    shared_glyph: Option<JsValue>,
    #[wasm_bindgen(unchecked_optional_param_type = "GlyfLimits")] limits: Option<JsValue>,
) -> JsResult<JsValue> {
    let b = mono(&bitmap)?;
    let o: fontpixel::outline::Options = opt(options)?;
    let shared: u16 = optional_int(shared_glyph, 0, "sharedGlyph")?;
    let l: glyf::Limits = opt(limits)?;
    conv::to_js(&glyf::Glyph::pixels(&b, &o, shared, &l).js()?)
}

/// `Statistics::unhinted_maxp`: maxp 1.0 bytes from encode statistics.
#[wasm_bindgen(js_namespace = ["sfnt", "glyf"], js_name = unhintedMaxp)]
pub fn glyf_unhinted_maxp(
    #[wasm_bindgen(unchecked_param_type = "Statistics")] statistics: JsValue,
    #[wasm_bindgen(js_name = numGlyphs)] num_glyphs: f64,
) -> JsResult<Uint8Array> {
    let s: glyf::Statistics = req(&statistics)?;
    Ok(byte_result(
        &s.unhinted_maxp(int(num_glyphs, "numGlyphs")?).js()?,
    ))
}

// ---------------------------------------------------------------- bitmaps

fn tables_js(mut t: bitmap::Tables) -> JsResult<JsValue> {
    let data = std::mem::take(&mut t.data);
    let location = std::mem::take(&mut t.location);
    with_bytes(&t, &[("data", &data), ("location", &location)])
}

/// `bitmap::decode`: strikes from location (EBLC/bloc/CBLC) and data tables.
#[wasm_bindgen(js_namespace = ["sfnt", "bitmap"], js_name = decode, unchecked_return_type = "Strike[]")]
pub fn bitmap_decode(
    #[wasm_bindgen(unchecked_param_type = "BytesLike")] location: JsValue,
    #[wasm_bindgen(unchecked_param_type = "BytesLike")] data: JsValue,
    #[wasm_bindgen(unchecked_param_type = "BitmapKind")] kind: JsValue,
    #[wasm_bindgen(js_name = numGlyphs)] num_glyphs: f64,
    #[wasm_bindgen(unchecked_optional_param_type = "SfntBitmapLimits")] limits: Option<JsValue>,
) -> JsResult<JsValue> {
    let l: bitmap::Limits = opt(limits)?;
    let s = bitmap::decode(
        &bytes_of(&location, "location")?,
        &bytes_of(&data, "data")?,
        req(&kind)?,
        int(num_glyphs, "numGlyphs")?,
        &l,
    )
    .js()?;
    conv::to_js(&s)
}

/// `bitmap::decode_with_color`: as `decode`, converting color strikes to
/// gray (`darkness` uses darkness rather than luminance).
#[cfg(feature = "all-formats")]
#[wasm_bindgen(js_namespace = ["sfnt", "bitmap"], js_name = decodeWithColor, unchecked_return_type = "Strike[]")]
pub fn bitmap_decode_with_color(
    #[wasm_bindgen(unchecked_param_type = "BytesLike")] location: JsValue,
    #[wasm_bindgen(unchecked_param_type = "BytesLike")] data: JsValue,
    #[wasm_bindgen(unchecked_param_type = "BitmapKind")] kind: JsValue,
    #[wasm_bindgen(js_name = numGlyphs)] num_glyphs: f64,
    darkness: bool,
    #[wasm_bindgen(unchecked_optional_param_type = "SfntBitmapLimits")] limits: Option<JsValue>,
) -> JsResult<JsValue> {
    let l: bitmap::Limits = opt(limits)?;
    let s = bitmap::decode_with_color(
        &bytes_of(&location, "location")?,
        &bytes_of(&data, "data")?,
        req(&kind)?,
        int(num_glyphs, "numGlyphs")?,
        &l,
        darkness,
    )
    .js()?;
    conv::to_js(&s)
}

/// `bitmap::encode`: `{data, location}` table bytes for `strikes`.
#[wasm_bindgen(js_namespace = ["sfnt", "bitmap"], js_name = encode, unchecked_return_type = "BytesFields<Tables, \"data\" | \"location\">")]
pub fn bitmap_encode(
    #[wasm_bindgen(unchecked_param_type = "Strike[]")] strikes: JsValue,
    #[wasm_bindgen(unchecked_param_type = "BitmapKind")] kind: JsValue,
    #[wasm_bindgen(js_name = numGlyphs)] num_glyphs: f64,
    #[wasm_bindgen(unchecked_optional_param_type = "SfntBitmapLimits")] limits: Option<JsValue>,
) -> JsResult<JsValue> {
    let s: Vec<bitmap::Strike> = req(&strikes)?;
    let l: bitmap::Limits = opt(limits)?;
    tables_js(bitmap::encode(&s, req(&kind)?, int(num_glyphs, "numGlyphs")?, &l).js()?)
}

/// `bitmap::encode_spanning`: as `encode`, with index subtables spanning gaps.
#[wasm_bindgen(js_namespace = ["sfnt", "bitmap"], js_name = encodeSpanning, unchecked_return_type = "BytesFields<Tables, \"data\" | \"location\">")]
pub fn bitmap_encode_spanning(
    #[wasm_bindgen(unchecked_param_type = "Strike[]")] strikes: JsValue,
    #[wasm_bindgen(unchecked_param_type = "BitmapKind")] kind: JsValue,
    #[wasm_bindgen(js_name = numGlyphs)] num_glyphs: f64,
    #[wasm_bindgen(unchecked_optional_param_type = "SfntBitmapLimits")] limits: Option<JsValue>,
) -> JsResult<JsValue> {
    let s: Vec<bitmap::Strike> = req(&strikes)?;
    let l: bitmap::Limits = opt(limits)?;
    tables_js(bitmap::encode_spanning(&s, req(&kind)?, int(num_glyphs, "numGlyphs")?, &l).js()?)
}

/// `runs::encode_shared_metric_runs`: `{tables: {data, location},
/// shared_runs}`.
#[wasm_bindgen(js_namespace = ["sfnt", "bitmap"], js_name = encodeSharedMetricRuns, unchecked_return_type = "{ tables: BytesFields<Tables, \"data\" | \"location\">; shared_runs: number }")]
pub fn bitmap_encode_shared_metric_runs(
    #[wasm_bindgen(unchecked_param_type = "Strike[]")] strikes: JsValue,
    #[wasm_bindgen(unchecked_param_type = "BitmapKind")] kind: JsValue,
    #[wasm_bindgen(js_name = numGlyphs)] num_glyphs: f64,
    #[wasm_bindgen(js_name = minimumRun)] minimum_run: f64,
    #[wasm_bindgen(unchecked_optional_param_type = "SfntBitmapLimits")] limits: Option<JsValue>,
) -> JsResult<JsValue> {
    let s: Vec<bitmap::Strike> = req(&strikes)?;
    let l: bitmap::Limits = opt(limits)?;
    let r = bitmap::encode_shared_metric_runs(
        &s,
        req(&kind)?,
        int(num_glyphs, "numGlyphs")?,
        int(minimum_run, "minimumRun")?,
        &l,
    )
    .js()?;
    object(&[
        ("tables", tables_js(r.tables)?),
        ("shared_runs", JsValue::from_f64(r.shared_runs as f64)),
    ])
}

/// `bitmap::decode_scaling`: EBSC `Scale` records.
#[wasm_bindgen(js_namespace = ["sfnt", "bitmap"], js_name = decodeScaling, unchecked_return_type = "Scale[]")]
pub fn bitmap_decode_scaling(
    #[wasm_bindgen(unchecked_param_type = "BytesLike")] data: JsValue,
    #[wasm_bindgen(unchecked_optional_param_type = "SfntBitmapLimits")] limits: Option<JsValue>,
) -> JsResult<JsValue> {
    let l: bitmap::Limits = opt(limits)?;
    conv::to_js(&bitmap::decode_scaling(&bytes_of(&data, "data")?, &l).js()?)
}

/// `bitmap::encode_scaling`: EBSC bytes.
#[wasm_bindgen(js_namespace = ["sfnt", "bitmap"], js_name = encodeScaling)]
pub fn bitmap_encode_scaling(
    #[wasm_bindgen(unchecked_param_type = "Scale[]")] scales: JsValue,
    #[wasm_bindgen(unchecked_optional_param_type = "SfntBitmapLimits")] limits: Option<JsValue>,
) -> JsResult<Uint8Array> {
    let s: Vec<bitmap::Scale> = req(&scales)?;
    let l: bitmap::Limits = opt(limits)?;
    Ok(byte_result(&bitmap::encode_scaling(&s, &l).js()?))
}

/// `Kind::tags`: `[dataTag, locationTag]`, e.g. `["EBDT", "EBLC"]`.
#[wasm_bindgen(js_namespace = ["sfnt", "bitmap"], js_name = kindTags, unchecked_return_type = "[string, string]")]
pub fn bitmap_kind_tags(
    #[wasm_bindgen(unchecked_param_type = "BitmapKind")] kind: JsValue,
) -> JsResult<Array> {
    let (a, b) = req::<bitmap::Kind>(&kind)?.tags();
    Ok([tag_str(a), tag_str(b)]
        .iter()
        .map(|s| JsValue::from_str(s))
        .collect())
}

/// `LineMetrics::ascender`: the signed ascender of a `LineMetrics` value.
#[wasm_bindgen(js_namespace = ["sfnt", "bitmap"], js_name = lineAscender)]
pub fn bitmap_line_ascender(
    #[wasm_bindgen(js_name = lineMetrics, unchecked_param_type = "LineMetrics")]
    line_metrics: JsValue,
) -> JsResult<i8> {
    Ok(req::<bitmap::LineMetrics>(&line_metrics)?.ascender())
}

/// `LineMetrics::descender`: the signed descender of a `LineMetrics` value.
#[wasm_bindgen(js_namespace = ["sfnt", "bitmap"], js_name = lineDescender)]
pub fn bitmap_line_descender(
    #[wasm_bindgen(js_name = lineMetrics, unchecked_param_type = "LineMetrics")]
    line_metrics: JsValue,
) -> JsResult<i8> {
    Ok(req::<bitmap::LineMetrics>(&line_metrics)?.descender())
}

/// `color::decode_sbix`: an `Sbix` (PNG payloads converted to gray;
/// `darkness` uses darkness rather than luminance).
#[cfg(feature = "all-formats")]
#[wasm_bindgen(js_namespace = ["sfnt", "bitmap"], js_name = decodeSbix, unchecked_return_type = "Sbix")]
pub fn bitmap_decode_sbix(
    #[wasm_bindgen(unchecked_param_type = "BytesLike")] data: JsValue,
    #[wasm_bindgen(js_name = numGlyphs)] num_glyphs: f64,
    darkness: Option<bool>,
    #[wasm_bindgen(unchecked_optional_param_type = "SfntBitmapLimits")] limits: Option<JsValue>,
) -> JsResult<JsValue> {
    let l: bitmap::Limits = opt(limits)?;
    conv::to_js(
        &bitmap::color::decode_sbix(
            &bytes_of(&data, "data")?,
            int(num_glyphs, "numGlyphs")?,
            &l,
            darkness.unwrap_or(false),
        )
        .js()?,
    )
}

/// `color::encode_sbix`: sbix table bytes.
#[cfg(feature = "all-formats")]
#[wasm_bindgen(js_namespace = ["sfnt", "bitmap"], js_name = encodeSbix)]
pub fn bitmap_encode_sbix(
    #[wasm_bindgen(unchecked_param_type = "Sbix")] sbix: JsValue,
    #[wasm_bindgen(js_name = numGlyphs)] num_glyphs: f64,
    #[wasm_bindgen(unchecked_optional_param_type = "SfntBitmapLimits")] limits: Option<JsValue>,
) -> JsResult<Uint8Array> {
    let s: bitmap::color::Sbix = req(&sbix)?;
    let l: bitmap::Limits = opt(limits)?;
    Ok(byte_result(
        &bitmap::color::encode_sbix(&s, int(num_glyphs, "numGlyphs")?, &l).js()?,
    ))
}
