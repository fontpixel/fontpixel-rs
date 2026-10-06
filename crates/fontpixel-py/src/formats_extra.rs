//! Format functions outside the uniform detect/read/write shape:
//! `fontpixel.formats.embedded`, `fontpixel.formats.vector_raster`,
//! `fontpixel.formats.x11`, extra helpers on the `bdf`, `fbit`, `fnt`,
//! `hanfont`, `hzk`, `nfnt`, `raw` and `svg` namespaces, and the format
//! catalogue (`fontpixel.formats.CATALOGUE`).
use crate::conv;
use crate::errors::{IntoPy, InvalidInputError};
use crate::formats::{InputArgs, SetArg, bytes_arg};
use crate::formats_direct::{
    GENERATED, ModelReport, bytes_report, detect_codec, read_bytes_limits, read_bytes_opts,
    read_ctx_opts, write_x_opts, write_x_primary_opts,
};
use crate::handles::{PyBitmap, PyFontSet, PyGlyph};
use crate::resources::PyWritten;
use fontpixel::formats::{self as f};
use fontpixel::model::GlyphBitmap;
use fontpixel::sfnt::bitmap::Kind;
use fontpixel::{Bitmap, FontSet};
use pyo3::prelude::*;
use pyo3::types::PyBytes;

fn kind_arg(kind: &Bound<'_, PyAny>) -> PyResult<Kind> {
    conv::from_py_required(kind)
}

fn vector_id(format: &str) -> PyResult<&'static str> {
    const IDS: [&str; 6] = ["ttf", "otf", "woff", "woff2", "ttc", "vector-raster"];
    IDS.iter().copied().find(|id| *id == format).ok_or_else(|| {
        InvalidInputError::new_err(format!(
            "'{format}' is not an outline format; expected one of {IDS:?}"
        ))
    })
}

fn mono(bitmap: &Bound<'_, PyAny>) -> PyResult<Bitmap> {
    let b = bitmap.cast::<PyBitmap>()?.get().snapshot()?;
    match b {
        GlyphBitmap::Mono(m) => Ok(m),
        GlyphBitmap::Gray8(_) => Err(InvalidInputError::new_err(
            "this function takes a Mono bitmap; threshold the Gray8 raster first",
        )),
    }
}

// --- embedded (otb / bdat / color) -----------------------------------------

/// Decode a standalone sfnt into bitmap strikes (`embedded::read`). `kind` is
/// `"open_type"` (EBDT/EBLC), `"apple"` (bdat/bloc) or `"color"` (CBDT/sbix);
/// `options` is an `EmbeddedReadOptions` dict. Returns `(FontSet, report)`.
#[pyfunction]
#[pyo3(name = "read", signature = (data, kind, options = None))]
fn embedded_read(
    data: &Bound<'_, PyAny>,
    kind: &Bound<'_, PyAny>,
    options: Option<&Bound<'_, PyAny>>,
) -> PyResult<ModelReport> {
    let kind = kind_arg(kind)?;
    read_bytes_opts::<_, f::embedded::EmbeddedReadOptions>(data, options, |b, o| {
        f::embedded::read(b, kind, o)
    })
}

/// Write strikes as a bitmap-only sfnt (`embedded::write`). `options` is an
/// `EmbeddedWriteOptions` dict. Returns `(bytes, report)`.
#[pyfunction]
#[pyo3(name = "write", signature = (fonts, kind, options = None))]
fn embedded_write(
    fonts: &Bound<'_, PyAny>,
    kind: &Bound<'_, PyAny>,
    options: Option<&Bound<'_, PyAny>>,
) -> PyResult<(Py<PyBytes>, Py<PyAny>)> {
    let kind = kind_arg(kind)?;
    write_x_opts::<FontSet, f::embedded::EmbeddedWriteOptions>(fonts, options, |s, o| {
        f::embedded::write(s, kind, o)
    })
}

/// Write coverage as CBDT/CBLC or sbix color bitmaps (`embedded::write_color`).
/// `options` is a `ColorWriteOptions` dict. Returns `(bytes, report)`.
#[pyfunction]
#[pyo3(signature = (fonts, options = None))]
fn write_color(
    fonts: &Bound<'_, PyAny>,
    options: Option<&Bound<'_, PyAny>>,
) -> PyResult<(Py<PyBytes>, Py<PyAny>)> {
    write_x_opts::<FontSet, f::embedded::ColorWriteOptions>(fonts, options, |s, o| {
        f::embedded::write_color(s, o)
    })
}

/// Run the embedded-bitmap detector of `kind` (`EmbeddedCodec::new(kind)`).
#[pyfunction]
#[pyo3(name = "detect", signature = (data, kind, *, name = None, limits = None))]
fn embedded_detect(
    data: &Bound<'_, PyAny>,
    kind: &Bound<'_, PyAny>,
    name: Option<String>,
    limits: Option<&Bound<'_, PyAny>>,
) -> PyResult<Py<PyAny>> {
    let codec = f::embedded::EmbeddedCodec::new(kind_arg(kind)?);
    detect_codec(&codec, data, name, limits)
}

/// The default codec of `kind` as a dict (`EmbeddedCodec::new(kind)`).
#[pyfunction]
fn codec<'py>(py: Python<'py>, kind: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    conv::to_py(py, &f::embedded::EmbeddedCodec::new(kind_arg(kind)?))
}

/// Validate an `EmbeddedMetadataOptions` dict for `kind`
/// (`EmbeddedMetadataOptions::validate`); raises `InvalidInputError`.
#[pyfunction]
fn validate_metadata(options: &Bound<'_, PyAny>, kind: &Bound<'_, PyAny>) -> PyResult<()> {
    let o: f::embedded::EmbeddedMetadataOptions = conv::from_py_required(options)?;
    o.validate(kind_arg(kind)?).py()
}

/// Retained bytes of an `EmbeddedMetadataOptions` dict's caller-owned
/// configuration, bounded by `maximum` (`EmbeddedMetadataOptions::retained_bytes`).
#[pyfunction]
#[pyo3(signature = (options, maximum = usize::MAX))]
fn metadata_retained_bytes(options: &Bound<'_, PyAny>, maximum: usize) -> PyResult<usize> {
    let o: f::embedded::EmbeddedMetadataOptions = conv::from_py_required(options)?;
    o.retained_bytes(maximum).py()
}

fn embedded_module(py: Python<'_>) -> PyResult<Bound<'_, PyModule>> {
    let m = PyModule::new(py, "embedded")?;
    m.add(
        "__doc__",
        "Embedded bitmap sfnt codecs (`fontpixel::formats::embedded`): otb, bdat, color.",
    )?;
    m.add("FORMAT_IDS", vec!["otb", "bdat", "color"])?;
    m.add_function(wrap_pyfunction!(embedded_read, &m)?)?;
    m.add_function(wrap_pyfunction!(embedded_write, &m)?)?;
    m.add_function(wrap_pyfunction!(write_color, &m)?)?;
    m.add_function(wrap_pyfunction!(embedded_detect, &m)?)?;
    m.add_function(wrap_pyfunction!(codec, &m)?)?;
    m.add_function(wrap_pyfunction!(validate_metadata, &m)?)?;
    m.add_function(wrap_pyfunction!(metadata_retained_bytes, &m)?)?;
    Ok(m)
}

// --- vector_raster (ttf / otf / woff / woff2 / ttc / vector-raster) --------

/// Rasterize a standalone outline font (`vector_raster::read`). `options` is
/// a `vector_raster::Options` dict. Returns `(FontSet, report)`.
#[pyfunction]
#[pyo3(name = "read", signature = (data, options = None))]
fn vector_read(
    data: &Bound<'_, PyAny>,
    options: Option<&Bound<'_, PyAny>>,
) -> PyResult<ModelReport> {
    read_bytes_opts::<_, f::vector_raster::Options>(data, options, |b, o| {
        f::vector_raster::read(b, o)
    })
}

/// Vectorize strikes as `format` (`vector_raster::write`). `options` is a
/// `vector_raster::WriteOptions` dict. Returns `(bytes, report)`.
#[pyfunction]
#[pyo3(name = "write", signature = (fonts, format, options = None))]
fn vector_write(
    fonts: &Bound<'_, PyAny>,
    format: &str,
    options: Option<&Bound<'_, PyAny>>,
) -> PyResult<(Py<PyBytes>, Py<PyAny>)> {
    let id = vector_id(format)?;
    write_x_opts::<FontSet, f::vector_raster::WriteOptions>(fonts, options, |s, o| {
        f::vector_raster::write(s, id, o)
    })
}

/// As `write`, but the model is handed over (`vector_raster::write_owned`):
/// a `FontSet` handle is left empty afterwards. Output and report are equal.
#[pyfunction]
#[pyo3(signature = (fonts, format, options = None))]
fn write_owned(
    py: Python<'_>,
    fonts: &Bound<'_, PyAny>,
    format: &str,
    options: Option<&Bound<'_, PyAny>>,
) -> PyResult<(Py<PyBytes>, Py<PyAny>)> {
    let id = vector_id(format)?;
    let o: f::vector_raster::WriteOptions = conv::from_py(options)?;
    let set = if let Ok(handle) = fonts.cast::<PyFontSet>() {
        handle.get().write(|s| Ok(std::mem::take(s)))?
    } else {
        SetArg::extract(fonts)?.into_owned()?
    };
    let result = py.detach(|| f::vector_raster::write_owned(set, id, &o));
    bytes_report(py, result)
}

/// Run the outline detector of `format` (`vector_raster::Codec::new(format)`).
#[pyfunction]
#[pyo3(name = "detect", signature = (data, format = "ttf", *, name = None, limits = None))]
fn vector_detect(
    data: &Bound<'_, PyAny>,
    format: &str,
    name: Option<String>,
    limits: Option<&Bound<'_, PyAny>>,
) -> PyResult<Py<PyAny>> {
    let codec = f::vector_raster::Codec::new(vector_id(format)?);
    detect_codec(&codec, data, name, limits)
}

/// The default codec options of `format` (`vector_raster::Codec::new`):
/// `{"id", "read_options", "write_options"}`.
#[pyfunction]
#[pyo3(name = "codec")]
fn vector_codec<'py>(py: Python<'py>, format: &str) -> PyResult<Bound<'py, PyAny>> {
    let c = f::vector_raster::Codec::new(vector_id(format)?);
    let d = pyo3::types::PyDict::new(py);
    d.set_item("id", c.id)?;
    d.set_item("read_options", conv::to_py(py, &c.read_options)?)?;
    d.set_item("write_options", conv::to_py(py, &c.write_options)?)?;
    Ok(d.into_any())
}

/// Prove a standalone sfnt's native integral pixel grid
/// (`vector_raster::detect_native_ppem`); `None` for a valid non-grid font.
/// `limits` is a `vector_raster::Limits` dict.
#[pyfunction]
#[pyo3(signature = (data, limits = None))]
fn detect_native_ppem(
    data: &Bound<'_, PyAny>,
    limits: Option<&Bound<'_, PyAny>>,
) -> PyResult<Option<u16>> {
    let bytes = bytes_arg(data)?;
    let l: f::vector_raster::Limits = conv::from_py(limits)?;
    data.py()
        .detach(|| f::vector_raster::detect_native_ppem(&bytes, &l))
        .py()
}

fn vector_module(py: Python<'_>) -> PyResult<Bound<'_, PyModule>> {
    let m = PyModule::new(py, "vector_raster")?;
    m.add(
        "__doc__",
        "Outline rasterization and vectorization (`fontpixel::formats::vector_raster`).",
    )?;
    m.add(
        "FORMAT_IDS",
        vec!["ttf", "otf", "woff", "woff2", "ttc", "vector-raster"],
    )?;
    m.add_function(wrap_pyfunction!(vector_read, &m)?)?;
    m.add_function(wrap_pyfunction!(vector_write, &m)?)?;
    m.add_function(wrap_pyfunction!(write_owned, &m)?)?;
    m.add_function(wrap_pyfunction!(vector_detect, &m)?)?;
    m.add_function(wrap_pyfunction!(vector_codec, &m)?)?;
    m.add_function(wrap_pyfunction!(detect_native_ppem, &m)?)?;
    Ok(m)
}

// --- x11 helpers (also on `bdf`) --------------------------------------------

/// Original stripped hex rows of a glyph read from BDF/PCF source, or `None`
/// when the glyph has no source provenance (`x11::source_rows`).
#[pyfunction]
fn source_rows(glyph: &Bound<'_, PyGlyph>) -> PyResult<Option<Vec<String>>> {
    glyph
        .get()
        .read(|g| Ok(f::bdf::source_rows(g).map(|rows| rows.map(str::to_owned).collect())))
}

/// Reject invalid source hex with the exact glyph/row error
/// (`x11::validate_bitmap`).
#[pyfunction]
fn validate_bitmap(glyph: &Bound<'_, PyGlyph>) -> PyResult<()> {
    glyph.get().read(|g| f::bdf::validate_bitmap(g).py())
}

fn x11_module(py: Python<'_>) -> PyResult<Bound<'_, PyModule>> {
    let m = PyModule::new(py, "x11")?;
    m.add(
        "__doc__",
        "X11 source-row helpers shared by BDF and PCF (`fontpixel::formats::x11`).",
    )?;
    m.add("FORMAT_IDS", vec!["bdf", "pcf"])?;
    m.add_function(wrap_pyfunction!(source_rows, &m)?)?;
    m.add_function(wrap_pyfunction!(validate_bitmap, &m)?)?;
    Ok(m)
}

// --- svg image helpers ------------------------------------------------------

/// A standalone SVG image of a Mono bitmap (`svg::image`). `options` is a
/// `svg::ImageOptions` dict.
#[pyfunction]
#[pyo3(signature = (bitmap, options = None))]
fn image(bitmap: &Bound<'_, PyAny>, options: Option<&Bound<'_, PyAny>>) -> PyResult<String> {
    let o: f::svg::ImageOptions = conv::from_py(options)?;
    f::svg::image(&mono(bitmap)?, &o).py()
}

/// One glyph's bitmap rectangle as SVG (`svg::glyph_image`); Gray8 glyphs
/// need `gray_threshold`.
#[pyfunction]
#[pyo3(signature = (glyph, options = None, gray_threshold = None))]
fn glyph_image(
    glyph: &Bound<'_, PyGlyph>,
    options: Option<&Bound<'_, PyAny>>,
    gray_threshold: Option<u8>,
) -> PyResult<String> {
    let o: f::svg::ImageOptions = conv::from_py(options)?;
    glyph
        .get()
        .read(|g| f::svg::glyph_image(g, &o, gray_threshold).py())
}

/// Deterministic upward-facing SVG path data of a Mono bitmap
/// (`svg::path_data`). `shape` is an `outline::Shape` name, `limits` an
/// `outline::Limits` dict.
#[pyfunction]
#[pyo3(signature = (bitmap, origin, units_per_pixel, shape = None, dot_size = 1.0, limits = None, max_bytes = 16777216))]
fn path_data(
    bitmap: &Bound<'_, PyAny>,
    origin: (i32, i32),
    units_per_pixel: u16,
    shape: Option<&Bound<'_, PyAny>>,
    dot_size: f64,
    limits: Option<&Bound<'_, PyAny>>,
    max_bytes: usize,
) -> PyResult<String> {
    let shape = match shape {
        Some(s) if !s.is_none() => conv::from_py_required(s)?,
        _ => fontpixel::outline::Shape::Square,
    };
    let l: fontpixel::outline::Limits = conv::from_py(limits)?;
    f::svg::path_data(
        &mono(bitmap)?,
        origin,
        units_per_pixel,
        shape,
        dot_size,
        &l,
        max_bytes,
    )
    .py()
}

// --- fnt / fbit / nfnt / hanfont / hzk / raw extras --------------------------

/// Exact code-page name of a Windows charset byte, or `None` for OEM, symbol,
/// DEFAULT and unknown identifiers (`fnt::charset_name`).
#[pyfunction]
fn charset_name(id: u8) -> Option<&'static str> {
    f::fnt::charset_name(id)
}

/// Decode FNT bytes with default charset interpretation
/// (`fnt::read_default`). Returns `(Font, report)`.
#[pyfunction]
#[pyo3(signature = (data, *, limits = None))]
fn read_default(
    data: &Bound<'_, PyAny>,
    limits: Option<&Bound<'_, PyAny>>,
) -> PyResult<ModelReport> {
    read_bytes_limits(data, limits, f::fnt::read_default)
}

/// Decode FONTBIT through a context with explicitly scoped data/resource
/// forks (`fbit::read_context`). Returns `(FontSet, report)`.
#[pyfunction]
#[pyo3(signature = (data = None, options = None, *, name = None, resources = None, limits = None, directory = None))]
fn read_context(
    data: Option<&Bound<'_, PyAny>>,
    options: Option<&Bound<'_, PyAny>>,
    name: Option<String>,
    resources: Option<&Bound<'_, PyAny>>,
    limits: Option<&Bound<'_, PyAny>>,
    directory: Option<String>,
) -> PyResult<ModelReport> {
    let input = InputArgs::new(data, name, resources, directory)?;
    read_ctx_opts::<_, f::fbit::FbitReadOptions>(&input, options, limits, |c, o| {
        f::fbit::read_context(c, o)
    })
}

/// Emit every FONTBIT fork as a bundle (`fbit::write_bundle`). Returns a
/// `Written`.
#[pyfunction]
#[pyo3(name = "write_bundle", signature = (fonts, options = None, *, primary_name = None))]
fn fbit_write_bundle(
    fonts: &Bound<'_, PyAny>,
    options: Option<&Bound<'_, PyAny>>,
    primary_name: Option<&str>,
) -> PyResult<PyWritten> {
    write_x_primary_opts::<FontSet, f::fbit::FbitWriteOptions>(
        "fbit",
        fonts,
        options,
        primary_name,
        |s, p, o| f::fbit::write_bundle(s, o, p),
    )
}

/// Encode every NFNT asset as a bundle (`nfnt::write_bundle`). Returns a
/// `Written`.
#[pyfunction]
#[pyo3(name = "write_bundle", signature = (fonts, options = None, *, primary_name = None))]
fn nfnt_write_bundle(
    fonts: &Bound<'_, PyAny>,
    options: Option<&Bound<'_, PyAny>>,
    primary_name: Option<&str>,
) -> PyResult<PyWritten> {
    write_x_primary_opts::<FontSet, f::nfnt::NfntWriteOptions>(
        "nfnt",
        fonts,
        options,
        primary_name,
        |s, p, o| f::nfnt::write_bundle(s, o, p),
    )
}

/// The `(initial, medial, final)` component slots of a modern Hangul
/// syllable; `final` is `None` when absent (`hanfont::component_slots`).
#[pyfunction]
fn component_slots(codepoint: u32) -> PyResult<(usize, usize, Option<usize>)> {
    f::hanfont::component_slots(codepoint).py()
}

/// Every HZK/ASC preset name in stable order (`hzk::Preset::ALL`).
#[pyfunction]
fn presets() -> Vec<&'static str> {
    f::hzk::Preset::ALL.iter().map(|p| p.name()).collect()
}

/// The `Geometry` dict of an HZK/ASC preset (`hzk::Preset::geometry`).
#[pyfunction]
fn preset_geometry<'py>(
    py: Python<'py>,
    preset: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let p: f::hzk::Preset = conv::from_py_required(preset)?;
    conv::to_py(py, &p.geometry())
}

/// The CLI spelling of an HZK/ASC preset (`hzk::Preset::name`).
#[pyfunction]
fn preset_name(preset: &Bound<'_, PyAny>) -> PyResult<&'static str> {
    let p: f::hzk::Preset = conv::from_py_required(preset)?;
    Ok(p.name())
}

/// The default `RawLayout` dict of a `width`×`height` cell
/// (`raw::RawLayout::new`).
#[pyfunction]
fn raw_layout(py: Python<'_>, width: u32, height: u32) -> PyResult<Bound<'_, PyAny>> {
    conv::to_py(py, &f::raw::RawLayout::new(width, height))
}

/// The `RawLayout` dict of a named raw preset (`raw::RawPreset::layout`).
#[pyfunction]
fn preset_layout<'py>(py: Python<'py>, preset: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    let p: f::raw::RawPreset = conv::from_py_required(preset)?;
    conv::to_py(py, &p.layout())
}

fn sub<'py>(parent: &Bound<'py, PyModule>, name: &str) -> PyResult<Bound<'py, PyModule>> {
    Ok(parent.getattr(name)?.cast_into::<PyModule>()?)
}

/// Register the special namespaces, the extra helpers and `CATALOGUE`/`MODULES`.
pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = parent.py();
    parent.add("embedded", embedded_module(py)?)?;
    parent.add("vector_raster", vector_module(py)?)?;
    parent.add("x11", x11_module(py)?)?;

    let bdf = sub(parent, "bdf")?;
    bdf.add_function(wrap_pyfunction!(source_rows, &bdf)?)?;
    bdf.add_function(wrap_pyfunction!(validate_bitmap, &bdf)?)?;
    let svg = sub(parent, "svg")?;
    svg.add_function(wrap_pyfunction!(image, &svg)?)?;
    svg.add_function(wrap_pyfunction!(glyph_image, &svg)?)?;
    svg.add_function(wrap_pyfunction!(path_data, &svg)?)?;
    let fnt = sub(parent, "fnt")?;
    fnt.add_function(wrap_pyfunction!(charset_name, &fnt)?)?;
    fnt.add_function(wrap_pyfunction!(read_default, &fnt)?)?;
    let fbit = sub(parent, "fbit")?;
    fbit.add_function(wrap_pyfunction!(read_context, &fbit)?)?;
    fbit.add_function(wrap_pyfunction!(fbit_write_bundle, &fbit)?)?;
    let nfnt = sub(parent, "nfnt")?;
    nfnt.add_function(wrap_pyfunction!(nfnt_write_bundle, &nfnt)?)?;
    let hanfont = sub(parent, "hanfont")?;
    hanfont.add_function(wrap_pyfunction!(component_slots, &hanfont)?)?;
    let hzk = sub(parent, "hzk")?;
    hzk.add_function(wrap_pyfunction!(presets, &hzk)?)?;
    hzk.add_function(wrap_pyfunction!(preset_geometry, &hzk)?)?;
    hzk.add_function(wrap_pyfunction!(preset_name, &hzk)?)?;
    let raw = sub(parent, "raw")?;
    raw.add_function(wrap_pyfunction!(raw_layout, &raw)?)?;
    raw.add_function(wrap_pyfunction!(preset_layout, &raw)?)?;

    let catalogue: Vec<_> = f::catalogue::CATALOGUE.to_vec();
    parent.add("CATALOGUE", conv::to_py(py, &catalogue)?)?;
    let mut modules: Vec<&str> = GENERATED.to_vec();
    modules.extend(["embedded", "vector_raster", "x11"]);
    modules.sort_unstable();
    parent.add("MODULES", modules)?;
    Ok(())
}
