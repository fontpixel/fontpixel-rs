//! Table codecs under `fontpixel.sfnt`: `cmap`, `name`, `os2`, `post`,
//! `metrics`, `gasp`, `gsub`, `glyf` and `bitmap` (EBDT/EBLC, bdat/bloc,
//! CBDT/CBLC, EBSC and sbix).
//!
//! Tables are plain serde dicts (`NameTable`, `Os2Table`, `GlyfTable`, …) with
//! the Rust field names. Encoders return `bytes`; result structs with several
//! byte fields return dicts whose byte fields are `bytes`. Rust methods on
//! value types (`NameRecord::text`, `Header::recompute`, `Gsub::closure`, …)
//! are module functions that take the dict and return a new value; the input
//! dict is never mutated.
use crate::errors::IntoPy;

use crate::sfnt_support::{
    bytes_arg, mono, opt, out, pybytes, req, submodule, tag_str, with_bytes,
};
use fontpixel::sfnt::{
    self, bitmap, cmap, gasp::GaspTable, glyf, gsub, metrics, name, os2, post::PostTable,
};
use pyo3::prelude::*;
use std::collections::{BTreeMap, BTreeSet};

type Any<'py> = Bound<'py, PyAny>;
type Opt<'a, 'py> = Option<&'a Bound<'py, PyAny>>;

// ---- cmap --------------------------------------------------------------------

fn encoded_cmap<'py>(py: Python<'py>, e: &cmap::EncodedCmap) -> PyResult<Any<'py>> {
    with_bytes(py, e, &[("data", &e.data)])
}

/// `cmap::encode`: encode `{codepoint: glyph_id}` with the default record set.
/// Returns `{data: bytes, has_bmp_subtable}`.
#[pyfunction]
#[pyo3(name = "encode", signature = (mapping, num_glyphs, limits = None))]
fn cmap_encode<'py>(
    py: Python<'py>,
    mapping: BTreeMap<u32, u16>,
    num_glyphs: u16,
    limits: Opt<'_, 'py>,
) -> PyResult<Any<'py>> {
    let l: sfnt::Limits = opt(limits)?;
    encoded_cmap(py, &cmap::encode(&mapping, num_glyphs, &l).py()?)
}

/// `cmap::encode_records`: as `encode`, optionally adding the Unicode full
/// record even for BMP-only mappings.
#[pyfunction]
#[pyo3(name = "encode_records", signature = (mapping, num_glyphs, unicode_full_for_bmp, limits = None))]
fn cmap_encode_records<'py>(
    py: Python<'py>,
    mapping: BTreeMap<u32, u16>,
    num_glyphs: u16,
    unicode_full_for_bmp: bool,
    limits: Opt<'_, 'py>,
) -> PyResult<Any<'py>> {
    let l: sfnt::Limits = opt(limits)?;
    encoded_cmap(
        py,
        &cmap::encode_records(&mapping, num_glyphs, &l, unicode_full_for_bmp).py()?,
    )
}

/// `cmap::encode_with_records`: choose the full-repertoire records
/// (`"always"`, `"non-bmp"` or `"when-needed"`).
#[pyfunction]
#[pyo3(name = "encode_with_records", signature = (mapping, num_glyphs, records, limits = None))]
fn cmap_encode_with_records<'py>(
    py: Python<'py>,
    mapping: BTreeMap<u32, u16>,
    num_glyphs: u16,
    records: &Bound<'py, PyAny>,
    limits: Opt<'_, 'py>,
) -> PyResult<Any<'py>> {
    let l: sfnt::Limits = opt(limits)?;
    let r: cmap::FullRecords = req(records)?;
    encoded_cmap(
        py,
        &cmap::encode_with_records(&mapping, num_glyphs, &l, r).py()?,
    )
}

/// `cmap::decode`: the preferred Unicode subtable as `{codepoint: glyph_id}`.
#[pyfunction]
#[pyo3(name = "decode", signature = (data, num_glyphs, limits = None))]
fn cmap_decode(
    data: &Bound<'_, PyAny>,
    num_glyphs: u16,
    limits: Opt<'_, '_>,
) -> PyResult<BTreeMap<u32, u16>> {
    let l: sfnt::Limits = opt(limits)?;
    cmap::decode(&bytes_arg(data)?, num_glyphs, &l).py()
}

// ---- name --------------------------------------------------------------------

/// `NameTable::decode`: a NameTable dict `{records, language_tags}`.
#[pyfunction]
#[pyo3(name = "decode", signature = (data, limits = None))]
fn name_decode<'py>(py: Python<'py>, data: &Any<'py>, limits: Opt<'_, 'py>) -> PyResult<Any<'py>> {
    let l: sfnt::Limits = opt(limits)?;
    out(py, &name::NameTable::decode(&bytes_arg(data)?, &l).py()?)
}

/// `NameTable::encode`: canonical sorted name table bytes.
#[pyfunction]
#[pyo3(name = "encode", signature = (table, limits = None))]
fn name_encode<'py>(py: Python<'py>, table: &Any<'py>, limits: Opt<'_, 'py>) -> PyResult<Any<'py>> {
    let t: name::NameTable = req(table)?;
    let l: sfnt::Limits = opt(limits)?;
    Ok(pybytes(py, &t.encode(&l).py()?))
}

/// `NameTable::encode_configured`: `validate_configured`, then encode.
#[pyfunction]
#[pyo3(signature = (table, limits = None))]
fn encode_configured<'py>(
    py: Python<'py>,
    table: &Any<'py>,
    limits: Opt<'_, 'py>,
) -> PyResult<Any<'py>> {
    let t: name::NameTable = req(table)?;
    let l: sfnt::Limits = opt(limits)?;
    Ok(pybytes(py, &t.encode_configured(&l).py()?))
}

/// `NameTable::validate_configured`: check platform/encoding/language/ID rules
/// for a caller-supplied table; raises `InvalidInputError`.
#[pyfunction]
#[pyo3(signature = (table, limits = None))]
fn validate_configured(table: &Any<'_>, limits: Opt<'_, '_>) -> PyResult<()> {
    let t: name::NameTable = req(table)?;
    let l: sfnt::Limits = opt(limits)?;
    t.validate_configured(&l).py()
}

/// `NameTable::retained_bytes`: retained logical storage of the table as
/// deserialized from `table` (spare capacity included), bounded by `maximum`.
#[pyfunction]
fn retained_bytes(table: &Any<'_>, maximum: usize) -> PyResult<usize> {
    let t: name::NameTable = req(table)?;
    t.retained_bytes(maximum).py()
}

/// `NameRecord::unicode`: a Windows Unicode (3, 1) record dict.
#[pyfunction]
fn record_unicode<'py>(
    py: Python<'py>,
    name_id: u16,
    language_id: u16,
    value: &str,
) -> PyResult<Any<'py>> {
    out(py, &name::NameRecord::unicode(name_id, language_id, value))
}

/// `NameRecord::text`: the decoded text of a record dict, or `None` for
/// encodings without a text decoding.
#[pyfunction]
fn record_text(record: &Any<'_>) -> PyResult<Option<String>> {
    let r: name::NameRecord = req(record)?;
    r.text().py()
}

/// `name::language_tag_well_formed`: RFC 5646 well-formedness only.
#[pyfunction]
fn language_tag_well_formed(tag: &str) -> bool {
    name::language_tag_well_formed(tag)
}

// ---- OS/2 --------------------------------------------------------------------

/// `Os2Table::decode`: an Os2Table dict.
#[pyfunction]
#[pyo3(name = "decode", signature = (data, limits = None))]
fn os2_decode<'py>(py: Python<'py>, data: &Any<'py>, limits: Opt<'_, 'py>) -> PyResult<Any<'py>> {
    let l: sfnt::Limits = opt(limits)?;
    out(py, &os2::Os2Table::decode(&bytes_arg(data)?, &l).py()?)
}

/// `Os2Table::encode`: OS/2 table bytes of the dict's version.
#[pyfunction]
#[pyo3(name = "encode", signature = (table, limits = None))]
fn os2_encode<'py>(py: Python<'py>, table: &Any<'py>, limits: Opt<'_, 'py>) -> PyResult<Any<'py>> {
    let t: os2::Os2Table = req(table)?;
    let l: sfnt::Limits = opt(limits)?;
    Ok(pybytes(py, &t.encode(&l).py()?))
}

/// `Os2Table::set_unicode_coverage`: a copy of `table` with Unicode ranges and
/// first/last character indexes derived from `codepoints`.
#[pyfunction]
fn set_unicode_coverage<'py>(
    py: Python<'py>,
    table: &Any<'py>,
    codepoints: &Any<'py>,
) -> PyResult<Any<'py>> {
    let mut t: os2::Os2Table = req(table)?;
    t.set_unicode_coverage(&req(codepoints)?).py()?;
    out(py, &t)
}

/// `os2::unicode_ranges`: the four ulUnicodeRange words.
#[pyfunction]
fn unicode_ranges(codepoints: &Any<'_>) -> PyResult<[u32; 4]> {
    os2::unicode_ranges(&req(codepoints)?).py()
}

/// `os2::code_page_ranges`: the two ulCodePageRange words for `codepoints`
/// and explicitly declared Windows code `pages`.
#[pyfunction]
fn code_page_ranges(codepoints: &Any<'_>, pages: Vec<u16>) -> PyResult<[u32; 2]> {
    os2::code_page_ranges(&req(codepoints)?, &pages).py()
}

/// `os2::average_width`: xAvgCharWidth of the given advances.
#[pyfunction]
fn average_width(advances: Vec<u16>) -> PyResult<i16> {
    os2::average_width(advances).py()
}

/// `os2::check_embedding_flags`: `None` when valid, else the reason.
#[pyfunction]
fn check_embedding_flags(flags: u16) -> Option<&'static str> {
    os2::check_embedding_flags(flags).err()
}

/// `os2::check_selection_flags`: `None` when valid for `version`, else the reason.
#[pyfunction]
fn check_selection_flags(flags: u16, version: u16) -> Option<&'static str> {
    os2::check_selection_flags(flags, version).err()
}

/// `os2::check_code_page_ranges`: `None` when valid, else the reason.
#[pyfunction]
fn check_code_page_ranges(ranges: [u32; 2]) -> Option<&'static str> {
    os2::check_code_page_ranges(ranges).err()
}

// ---- post --------------------------------------------------------------------

/// `PostTable::decode`: a PostTable dict.
#[pyfunction]
#[pyo3(name = "decode", signature = (data, num_glyphs, limits = None))]
fn post_decode<'py>(
    py: Python<'py>,
    data: &Any<'py>,
    num_glyphs: u16,
    limits: Opt<'_, 'py>,
) -> PyResult<Any<'py>> {
    let l: sfnt::Limits = opt(limits)?;
    out(
        py,
        &PostTable::decode(&bytes_arg(data)?, num_glyphs, &l).py()?,
    )
}

/// `PostTable::encode`: post table bytes.
#[pyfunction]
#[pyo3(name = "encode", signature = (table, num_glyphs, limits = None))]
fn post_encode<'py>(
    py: Python<'py>,
    table: &Any<'py>,
    num_glyphs: u16,
    limits: Opt<'_, 'py>,
) -> PyResult<Any<'py>> {
    let t: PostTable = req(table)?;
    let l: sfnt::Limits = opt(limits)?;
    Ok(pybytes(py, &t.encode(num_glyphs, &l).py()?))
}

// ---- hhea/hmtx, vhea/vmtx ------------------------------------------------------

/// `metrics::Header::new`: a default header dict for `axis`
/// (`"horizontal"` or `"vertical"`).
#[pyfunction]
fn header_new<'py>(py: Python<'py>, axis: &Any<'py>) -> PyResult<Any<'py>> {
    out(py, &metrics::Header::new(req(axis)?))
}

/// `Header::recompute`: a copy of `header` with extents recomputed from
/// `metrics` (LongMetric dicts) and per-glyph ink extents (`None` = blank).
#[pyfunction]
fn header_recompute<'py>(
    py: Python<'py>,
    header: &Any<'py>,
    metrics: &Any<'py>,
    ink_extents: Vec<Option<u16>>,
) -> PyResult<Any<'py>> {
    let mut h: metrics::Header = req(header)?;
    let m: Vec<metrics::LongMetric> = req(metrics)?;
    h.recompute(&m, &ink_extents).py()?;
    out(py, &h)
}

/// `metrics::encode`: `{header: bytes, metrics: bytes, num_long_metrics}`.
#[pyfunction]
#[pyo3(name = "encode", signature = (axis, header, metrics, compact, limits = None))]
fn metrics_encode<'py>(
    py: Python<'py>,
    axis: &Any<'py>,
    header: &Any<'py>,
    metrics: &Any<'py>,
    compact: bool,
    limits: Opt<'_, 'py>,
) -> PyResult<Any<'py>> {
    let h: metrics::Header = req(header)?;
    let m: Vec<metrics::LongMetric> = req(metrics)?;
    let l: sfnt::Limits = opt(limits)?;
    let e = metrics::encode(req(axis)?, &h, &m, compact, &l).py()?;
    with_bytes(py, &e, &[("header", &e.header), ("metrics", &e.metrics)])
}

/// `metrics::decode`: `(header, metrics)` from the header and metrics tables.
#[pyfunction]
#[pyo3(name = "decode", signature = (axis, header, data, num_glyphs, limits = None))]
fn metrics_decode<'py>(
    py: Python<'py>,
    axis: &Any<'py>,
    header: &Any<'py>,
    data: &Any<'py>,
    num_glyphs: u16,
    limits: Opt<'_, 'py>,
) -> PyResult<(Any<'py>, Any<'py>)> {
    let l: sfnt::Limits = opt(limits)?;
    let (h, m) = metrics::decode(
        req(axis)?,
        &bytes_arg(header)?,
        &bytes_arg(data)?,
        num_glyphs,
        &l,
    )
    .py()?;
    Ok((out(py, &h)?, out(py, &m)?))
}

// ---- gasp --------------------------------------------------------------------

/// `GaspTable::decode`: a GaspTable dict.
#[pyfunction]
#[pyo3(name = "decode", signature = (data, limits = None))]
fn gasp_decode<'py>(py: Python<'py>, data: &Any<'py>, limits: Opt<'_, 'py>) -> PyResult<Any<'py>> {
    let l: sfnt::Limits = opt(limits)?;
    out(py, &GaspTable::decode(&bytes_arg(data)?, &l).py()?)
}

/// `GaspTable::encode`: gasp table bytes.
#[pyfunction]
#[pyo3(name = "encode", signature = (table, limits = None))]
fn gasp_encode<'py>(py: Python<'py>, table: &Any<'py>, limits: Opt<'_, 'py>) -> PyResult<Any<'py>> {
    let t: GaspTable = req(table)?;
    let l: sfnt::Limits = opt(limits)?;
    Ok(pybytes(py, &t.encode(&l).py()?))
}

// ---- GSUB --------------------------------------------------------------------

/// `Gsub::decode`: a Gsub dict (scripts keyed by tag strings).
#[pyfunction]
#[pyo3(name = "decode", signature = (data, num_glyphs, limits = None))]
fn gsub_decode<'py>(
    py: Python<'py>,
    data: &Any<'py>,
    num_glyphs: u16,
    limits: Opt<'_, 'py>,
) -> PyResult<Any<'py>> {
    let l: gsub::LayoutLimits = opt(limits)?;
    out(
        py,
        &gsub::Gsub::decode(&bytes_arg(data)?, num_glyphs, &l).py()?,
    )
}

/// `Gsub::encode`: GSUB table bytes.
#[pyfunction]
#[pyo3(name = "encode", signature = (gsub, num_glyphs, limits = None))]
fn gsub_encode<'py>(
    py: Python<'py>,
    gsub: &Any<'py>,
    num_glyphs: u16,
    limits: Opt<'_, 'py>,
) -> PyResult<Any<'py>> {
    let g: gsub::Gsub = req(gsub)?;
    let l: gsub::LayoutLimits = opt(limits)?;
    Ok(pybytes(py, &g.encode(num_glyphs, &l).py()?))
}

/// `Gsub::closure`: `glyphs` plus every glyph reachable by substitution,
/// returned as a sorted list.
#[pyfunction]
fn closure(gsub: &Any<'_>, glyphs: &Any<'_>) -> PyResult<Vec<u16>> {
    let g: gsub::Gsub = req(gsub)?;
    let mut set: BTreeSet<u16> = req(glyphs)?;
    g.closure(&mut set);
    Ok(set.into_iter().collect())
}

/// `Gsub::subset`: the Gsub dict remapped through `{old_id: new_id}`.
#[pyfunction]
#[pyo3(name = "subset")]
fn gsub_subset<'py>(
    py: Python<'py>,
    gsub: &Any<'py>,
    mapping: BTreeMap<u16, u16>,
) -> PyResult<Any<'py>> {
    let g: gsub::Gsub = req(gsub)?;
    out(py, &g.subset(&mapping))
}

// ---- glyf/loca ---------------------------------------------------------------

/// `GlyfTable::decode`: a GlyfTable dict from glyf and loca bytes.
#[pyfunction]
#[pyo3(name = "decode", signature = (glyf, loca, num_glyphs, index_to_loc_format, limits = None))]
fn glyf_decode<'py>(
    py: Python<'py>,
    glyf: &Any<'py>,
    loca: &Any<'py>,
    num_glyphs: u16,
    index_to_loc_format: i16,
    limits: Opt<'_, 'py>,
) -> PyResult<Any<'py>> {
    let l: glyf::Limits = opt(limits)?;
    let t = glyf::GlyfTable::decode(
        &bytes_arg(glyf)?,
        &bytes_arg(loca)?,
        num_glyphs,
        index_to_loc_format,
        &l,
    )
    .py()?;
    out(py, &t)
}

/// `GlyfTable::from_sfnt`: the GlyfTable dict of an `Sfnt` handle.
#[pyfunction]
#[pyo3(signature = (font, limits = None))]
fn from_sfnt<'py>(
    py: Python<'py>,
    font: &crate::sfnt::PySfnt,
    limits: Opt<'_, 'py>,
) -> PyResult<Any<'py>> {
    let l: glyf::Limits = opt(limits)?;
    font.with(|s| out(py, &glyf::GlyfTable::from_sfnt(s, &l).py()?))
}

/// `GlyfTable::contours`: flattened contours of `glyph_id` (composites expanded).
#[pyfunction]
#[pyo3(signature = (table, glyph_id, limits = None))]
fn contours<'py>(
    py: Python<'py>,
    table: &Any<'py>,
    glyph_id: u16,
    limits: Opt<'_, 'py>,
) -> PyResult<Any<'py>> {
    let t: glyf::GlyfTable = req(table)?;
    let l: glyf::Limits = opt(limits)?;
    out(py, &t.contours(glyph_id, &l).py()?)
}

/// `GlyfTable::encode`: `{glyf, loca, index_to_loc_format, bounds, statistics}`
/// with `glyf`/`loca` as `bytes`; `format` is `"short"`, `"long"` or `"auto"`.
#[pyfunction]
#[pyo3(name = "encode", signature = (table, format, limits = None))]
fn glyf_encode<'py>(
    py: Python<'py>,
    table: &Any<'py>,
    format: &Any<'py>,
    limits: Opt<'_, 'py>,
) -> PyResult<Any<'py>> {
    let t: glyf::GlyfTable = req(table)?;
    let l: glyf::Limits = opt(limits)?;
    let e = t.encode(req(format)?, &l).py()?;
    with_bytes(py, &e, &[("glyf", &e.glyf), ("loca", &e.loca)])
}

/// `Component::translated`: a component dict offset by `(x, y)`.
#[pyfunction]
fn component_translated<'py>(py: Python<'py>, glyph_id: u16, x: i16, y: i16) -> PyResult<Any<'py>> {
    out(py, &glyf::Component::translated(glyph_id, x, y))
}

/// `Glyph::simple`: a simple glyph dict from `[(x, y, on_curve), …]` contours.
#[pyfunction]
fn glyph_simple<'py>(py: Python<'py>, contours: &Any<'py>) -> PyResult<Any<'py>> {
    out(py, &glyf::Glyph::simple(req(contours)?))
}

/// `Glyph::pixels`: a glyph dict tracing a Mono bitmap handle (Dot shapes
/// reference `shared_glyph`).
#[pyfunction]
#[pyo3(signature = (bitmap, options = None, shared_glyph = 0, limits = None))]
fn glyph_pixels<'py>(
    py: Python<'py>,
    bitmap: &Any<'py>,
    options: Opt<'_, 'py>,
    shared_glyph: u16,
    limits: Opt<'_, 'py>,
) -> PyResult<Any<'py>> {
    let b = mono(bitmap)?;
    let o: fontpixel::outline::Options = opt(options)?;
    let l: glyf::Limits = opt(limits)?;
    out(py, &glyf::Glyph::pixels(&b, &o, shared_glyph, &l).py()?)
}

/// `Statistics::unhinted_maxp`: maxp 1.0 bytes from encode statistics.
#[pyfunction]
fn unhinted_maxp<'py>(
    py: Python<'py>,
    statistics: &Any<'py>,
    num_glyphs: u16,
) -> PyResult<Any<'py>> {
    let s: glyf::Statistics = req(statistics)?;
    Ok(pybytes(py, &s.unhinted_maxp(num_glyphs).py()?))
}

// ---- embedded bitmaps --------------------------------------------------------

fn tables_dict<'py>(py: Python<'py>, t: &bitmap::Tables) -> PyResult<Any<'py>> {
    with_bytes(py, t, &[("data", &t.data), ("location", &t.location)])
}

/// `bitmap::decode`: strikes from location (EBLC/bloc/CBLC) and data tables.
#[pyfunction]
#[pyo3(name = "decode", signature = (location, data, kind, num_glyphs, limits = None))]
fn bitmap_decode<'py>(
    py: Python<'py>,
    location: &Any<'py>,
    data: &Any<'py>,
    kind: &Any<'py>,
    num_glyphs: u16,
    limits: Opt<'_, 'py>,
) -> PyResult<Any<'py>> {
    let l: bitmap::Limits = opt(limits)?;
    let s = bitmap::decode(
        &bytes_arg(location)?,
        &bytes_arg(data)?,
        req(kind)?,
        num_glyphs,
        &l,
    )
    .py()?;
    out(py, &s)
}

/// `bitmap::decode_with_color`: as `decode`, converting color strikes to
/// gray (`darkness=True` uses darkness rather than luminance).
#[pyfunction]
#[pyo3(signature = (location, data, kind, num_glyphs, darkness, limits = None))]
fn decode_with_color<'py>(
    py: Python<'py>,
    location: &Any<'py>,
    data: &Any<'py>,
    kind: &Any<'py>,
    num_glyphs: u16,
    darkness: bool,
    limits: Opt<'_, 'py>,
) -> PyResult<Any<'py>> {
    let l: bitmap::Limits = opt(limits)?;
    let s = bitmap::decode_with_color(
        &bytes_arg(location)?,
        &bytes_arg(data)?,
        req(kind)?,
        num_glyphs,
        &l,
        darkness,
    )
    .py()?;
    out(py, &s)
}

/// `bitmap::encode`: `{data, location}` bytes for strike dicts.
#[pyfunction]
#[pyo3(name = "encode", signature = (strikes, kind, num_glyphs, limits = None))]
fn bitmap_encode<'py>(
    py: Python<'py>,
    strikes: &Any<'py>,
    kind: &Any<'py>,
    num_glyphs: u16,
    limits: Opt<'_, 'py>,
) -> PyResult<Any<'py>> {
    let s: Vec<bitmap::Strike> = req(strikes)?;
    let l: bitmap::Limits = opt(limits)?;
    tables_dict(py, &bitmap::encode(&s, req(kind)?, num_glyphs, &l).py()?)
}

/// `bitmap::encode_spanning`: as `encode`, with index subtables spanning gaps.
#[pyfunction]
#[pyo3(signature = (strikes, kind, num_glyphs, limits = None))]
fn encode_spanning<'py>(
    py: Python<'py>,
    strikes: &Any<'py>,
    kind: &Any<'py>,
    num_glyphs: u16,
    limits: Opt<'_, 'py>,
) -> PyResult<Any<'py>> {
    let s: Vec<bitmap::Strike> = req(strikes)?;
    let l: bitmap::Limits = opt(limits)?;
    tables_dict(
        py,
        &bitmap::encode_spanning(&s, req(kind)?, num_glyphs, &l).py()?,
    )
}

/// `runs::encode_shared_metric_runs`: `{tables: {data, location}, shared_runs}`.
#[pyfunction]
#[pyo3(signature = (strikes, kind, num_glyphs, minimum_run, limits = None))]
fn encode_shared_metric_runs<'py>(
    py: Python<'py>,
    strikes: &Any<'py>,
    kind: &Any<'py>,
    num_glyphs: u16,
    minimum_run: u16,
    limits: Opt<'_, 'py>,
) -> PyResult<Any<'py>> {
    let s: Vec<bitmap::Strike> = req(strikes)?;
    let l: bitmap::Limits = opt(limits)?;
    let r = bitmap::encode_shared_metric_runs(&s, req(kind)?, num_glyphs, minimum_run, &l).py()?;
    crate::sfnt_support::dict(
        py,
        vec![
            ("tables", tables_dict(py, &r.tables)?),
            ("shared_runs", r.shared_runs.into_pyobject(py)?.into_any()),
        ],
    )
}

/// `bitmap::decode_scaling`: EBSC scale dicts.
#[pyfunction]
#[pyo3(signature = (data, limits = None))]
fn decode_scaling<'py>(
    py: Python<'py>,
    data: &Any<'py>,
    limits: Opt<'_, 'py>,
) -> PyResult<Any<'py>> {
    let l: bitmap::Limits = opt(limits)?;
    out(py, &bitmap::decode_scaling(&bytes_arg(data)?, &l).py()?)
}

/// `bitmap::encode_scaling`: EBSC bytes.
#[pyfunction]
#[pyo3(signature = (scales, limits = None))]
fn encode_scaling<'py>(
    py: Python<'py>,
    scales: &Any<'py>,
    limits: Opt<'_, 'py>,
) -> PyResult<Any<'py>> {
    let s: Vec<bitmap::Scale> = req(scales)?;
    let l: bitmap::Limits = opt(limits)?;
    Ok(pybytes(py, &bitmap::encode_scaling(&s, &l).py()?))
}

/// `Kind::tags`: `(data_tag, location_tag)`, e.g. `("EBDT", "EBLC")`.
#[pyfunction]
fn kind_tags(kind: &Any<'_>) -> PyResult<(String, String)> {
    let (a, b) = req::<bitmap::Kind>(kind)?.tags();
    Ok((tag_str(a), tag_str(b)))
}

/// `LineMetrics::ascender`: the signed ascender of a 12-byte LineMetrics value.
#[pyfunction]
fn line_ascender(line_metrics: &Any<'_>) -> PyResult<i8> {
    Ok(req::<bitmap::LineMetrics>(line_metrics)?.ascender())
}

/// `LineMetrics::descender`: the signed descender of a LineMetrics value.
#[pyfunction]
fn line_descender(line_metrics: &Any<'_>) -> PyResult<i8> {
    Ok(req::<bitmap::LineMetrics>(line_metrics)?.descender())
}

/// `color::decode_sbix`: an Sbix dict (PNG payloads converted to gray).
#[pyfunction]
#[pyo3(signature = (data, num_glyphs, darkness = false, limits = None))]
fn decode_sbix<'py>(
    py: Python<'py>,
    data: &Any<'py>,
    num_glyphs: u16,
    darkness: bool,
    limits: Opt<'_, 'py>,
) -> PyResult<Any<'py>> {
    let l: bitmap::Limits = opt(limits)?;
    out(
        py,
        &bitmap::color::decode_sbix(&bytes_arg(data)?, num_glyphs, &l, darkness).py()?,
    )
}

/// `color::encode_sbix`: sbix table bytes.
#[pyfunction]
#[pyo3(signature = (sbix, num_glyphs, limits = None))]
fn encode_sbix<'py>(
    py: Python<'py>,
    sbix: &Any<'py>,
    num_glyphs: u16,
    limits: Opt<'_, 'py>,
) -> PyResult<Any<'py>> {
    let s: bitmap::color::Sbix = req(sbix)?;
    let l: bitmap::Limits = opt(limits)?;
    Ok(pybytes(
        py,
        &bitmap::color::encode_sbix(&s, num_glyphs, &l).py()?,
    ))
}

macro_rules! add {
    ($m:expr, $($f:ident),+ $(,)?) => {
        $( $m.add_function(wrap_pyfunction!($f, &$m)?)?; )+
    };
}

/// Register the table submodules under `fontpixel.sfnt` (`s`).
pub fn register(s: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = submodule(
        s,
        "cmap",
        "fontpixel.sfnt.cmap",
        "Character-to-glyph mapping table.",
    )?;
    add!(
        m,
        cmap_encode,
        cmap_encode_records,
        cmap_encode_with_records,
        cmap_decode
    );

    let m = submodule(s, "name", "fontpixel.sfnt.name", "Naming table.")?;
    add!(
        m,
        name_decode,
        name_encode,
        encode_configured,
        validate_configured,
        retained_bytes,
        record_unicode,
        record_text,
        language_tag_well_formed
    );

    let m = submodule(
        s,
        "os2",
        "fontpixel.sfnt.os2",
        "OS/2 and Windows metrics table.",
    )?;
    add!(
        m,
        os2_decode,
        os2_encode,
        set_unicode_coverage,
        unicode_ranges,
        code_page_ranges,
        average_width,
        check_embedding_flags,
        check_selection_flags,
        check_code_page_ranges
    );

    let m = submodule(s, "post", "fontpixel.sfnt.post", "PostScript table.")?;
    add!(m, post_decode, post_encode);

    let m = submodule(
        s,
        "metrics",
        "fontpixel.sfnt.metrics",
        "hhea/hmtx and vhea/vmtx tables.",
    )?;
    add!(
        m,
        header_new,
        header_recompute,
        metrics_encode,
        metrics_decode
    );

    let m = submodule(
        s,
        "gasp",
        "fontpixel.sfnt.gasp",
        "Grid-fitting and scan-conversion table.",
    )?;
    add!(m, gasp_decode, gasp_encode);

    let m = submodule(
        s,
        "gsub",
        "fontpixel.sfnt.gsub",
        "Glyph substitution table.",
    )?;
    add!(m, gsub_decode, gsub_encode, closure, gsub_subset);

    let m = submodule(
        s,
        "glyf",
        "fontpixel.sfnt.glyf",
        "TrueType glyf/loca tables.",
    )?;
    add!(
        m,
        glyf_decode,
        from_sfnt,
        contours,
        glyf_encode,
        component_translated,
        glyph_simple,
        glyph_pixels,
        unhinted_maxp
    );

    let m = submodule(
        s,
        "bitmap",
        "fontpixel.sfnt.bitmap",
        "Embedded bitmaps: EBDT/EBLC, bdat/bloc, CBDT/CBLC, EBSC and sbix.",
    )?;
    add!(
        m,
        bitmap_decode,
        decode_with_color,
        bitmap_encode,
        encode_spanning,
        encode_shared_metric_runs,
        decode_scaling,
        encode_scaling,
        kind_tags,
        line_ascender,
        line_descender,
        decode_sbix,
        encode_sbix
    );
    Ok(())
}
