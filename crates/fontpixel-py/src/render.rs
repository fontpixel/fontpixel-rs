//! Text rendering (`Font.draw*`), `Rendered` results and PNG/SVG export
//! (`fontpixel::render`).
//!
//! `RenderOptions` is a dict with the Rust field names (`line_limit`, `mode`
//! `"advance"`/`"cell"`/`"ink"`, `direction` `{"glyphs", "lines"}`,
//! `use_current_glyph_spacing`, `letter_spacing`, `line_spacing`, `missing`
//! `"blank"`/`"error"`, `gray_threshold`, `strict`, `limits`); omitted
//! fields take the Rust defaults. The `MissingGlyph::Glyph` replacement is
//! passed as the separate `missing_glyph` argument (a `Glyph`), because the
//! Rust option borrows it.
use crate::bitmap::bitmap_arg;
use crate::conv::{from_py, from_py_required, to_py};
use crate::errors::IntoPy;
use crate::handles::{PyBitmap, PyFont};
use crate::model::invalid;
use crate::model::{codepoint_arg, glyph_arg};
use fontpixel::model::{Codepoint, Glyph, GlyphBitmap};
use fontpixel::render::{
    GlyphOrder, ImageOptions, MissingGlyph, RenderLimits, RenderOptions, Rendered, Rgba,
    WritingDirection,
};
use pyo3::prelude::*;
use pyo3::types::PyBytes;

/// Result of a render call (`fontpixel::render::Rendered`): the Mono
/// `bitmap` (top-left origin) and the `report` dict with every diagnostic.
#[pyclass(name = "Rendered", module = "fontpixel", frozen)]
pub struct PyRendered {
    /// Rendered pixels.
    #[pyo3(get)]
    bitmap: Py<PyBitmap>,
    /// Plain-data report (`{"entries": [...]}`).
    #[pyo3(get)]
    report: Py<PyAny>,
}

impl PyRendered {
    /// Wrap a Rust result.
    pub fn new(py: Python<'_>, rendered: Rendered) -> PyResult<Self> {
        Ok(Self {
            bitmap: Py::new(py, PyBitmap::new(rendered.bitmap.into()))?,
            report: to_py(py, &rendered.report)?.unbind(),
        })
    }
}

#[pymethods]
impl PyRendered {
    /// Plain data: `{"bitmap": {"width", "height", "data"}, "report": {...}}`.
    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let dict = pyo3::types::PyDict::new(py);
        dict.set_item(
            "bitmap",
            self.bitmap
                .bind(py)
                .call_method0("to_dict")?
                .get_item("mono")?,
        )?;
        dict.set_item("report", self.report.bind(py))?;
        Ok(dict.into_any())
    }
    fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        let b = self.bitmap.bind(py).get();
        let entries = self.report.bind(py).get_item("entries")?.len()?;
        b.read(|bm| {
            Ok(format!(
                "Rendered(bitmap={}x{}, report_entries={entries})",
                bm.width(),
                bm.height()
            ))
        })
    }
}

/// Render options with an optional borrowed replacement glyph.
pub fn render_options<'a>(
    options: Option<&Bound<'_, PyAny>>,
    missing_glyph: Option<&'a Glyph>,
) -> PyResult<RenderOptions<'a>> {
    let options: RenderOptions<'static> = from_py(options)?;
    let mut options: RenderOptions<'a> = options;
    if let Some(glyph) = missing_glyph {
        options.missing = MissingGlyph::Glyph(glyph);
    }
    Ok(options)
}

fn missing_arg(obj: Option<&Bound<'_, PyAny>>) -> PyResult<Option<Glyph>> {
    obj.filter(|o| !o.is_none()).map(glyph_arg).transpose()
}

#[pymethods]
impl PyFont {
    /// Render Unicode `text` through the font's charset map
    /// (`Font::draw`). Returns `Rendered`; raises `StrictLossError` (with
    /// `report`) in strict mode and `ResourceLimitError` over budget.
    #[pyo3(signature = (text, options = None, missing_glyph = None))]
    fn draw(
        &self,
        py: Python<'_>,
        text: &str,
        options: Option<&Bound<'_, PyAny>>,
        missing_glyph: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<PyRendered> {
        let missing = missing_arg(missing_glyph)?;
        let options = render_options(options, missing.as_ref())?;
        let rendered = self.read(|f| f.draw(text, &options).py())?;
        PyRendered::new(py, rendered)
    }
    /// Render explicit codes; `None` entries stand for unencodable
    /// characters (`Font::draw_codepoints`).
    #[pyo3(signature = (codes, options = None, missing_glyph = None))]
    fn draw_codepoints(
        &self,
        py: Python<'_>,
        codes: &Bound<'_, PyAny>,
        options: Option<&Bound<'_, PyAny>>,
        missing_glyph: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<PyRendered> {
        let codes = codes
            .try_iter()?
            .map(|c| {
                let c = c?;
                if c.is_none() {
                    Ok(None)
                } else {
                    codepoint_arg(&c).map(Some)
                }
            })
            .collect::<PyResult<Vec<Option<Codepoint>>>>()?;
        let missing = missing_arg(missing_glyph)?;
        let options = render_options(options, missing.as_ref())?;
        let rendered = self.read(|f| f.draw_codepoints(codes, &options).py())?;
        PyRendered::new(py, rendered)
    }
    /// Render every effective mapping in `order` (`"ascending"`,
    /// `"descending"`, `"file"`, `"reverse_file"`), optionally restricted to
    /// inclusive `(start, end)` ranges (`Font::draw_all`).
    #[pyo3(signature = (order = None, ranges = None, options = None, missing_glyph = None))]
    fn draw_all(
        &self,
        py: Python<'_>,
        order: Option<&Bound<'_, PyAny>>,
        ranges: Option<Vec<(u32, u32)>>,
        options: Option<&Bound<'_, PyAny>>,
        missing_glyph: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<PyRendered> {
        let order: GlyphOrder = match order {
            Some(o) if !o.is_none() => from_py_required(o)?,
            _ => GlyphOrder::Ascending,
        };
        let ranges = ranges.unwrap_or_default();
        let missing = missing_arg(missing_glyph)?;
        let options = render_options(options, missing.as_ref())?;
        let rendered = self.read(|f| f.draw_all(order, &ranges, &options).py())?;
        PyRendered::new(py, rendered)
    }
}

fn mono_arg(bitmap: &Bound<'_, PyAny>) -> PyResult<fontpixel::Bitmap> {
    match bitmap_arg(bitmap)? {
        GlyphBitmap::Mono(b) => Ok(b),
        GlyphBitmap::Gray8(_) => Err(invalid(
            "image export takes a mono bitmap; convert with to_mono(threshold)",
        )),
    }
}

/// Encode a Mono bitmap as an indexed 1-bit PNG (`render::image::to_png`).
/// `options` is an `ImageOptions` dict (`scale`, `foreground`, `background`,
/// `limits`); colors are `{red, green, blue, alpha}` dicts.
#[pyfunction]
#[pyo3(signature = (bitmap, options = None))]
fn to_png<'py>(
    py: Python<'py>,
    bitmap: &Bound<'_, PyAny>,
    options: Option<&Bound<'_, PyAny>>,
) -> PyResult<Bound<'py, PyBytes>> {
    let bitmap = mono_arg(bitmap)?;
    let options: ImageOptions = from_py(options)?;
    let bytes = fontpixel::render::to_png(&bitmap, &options).py()?;
    Ok(PyBytes::new(py, &bytes))
}

/// Export a Mono bitmap as a standalone SVG string (`render::image::to_svg`).
#[pyfunction]
#[pyo3(signature = (bitmap, options = None))]
fn to_svg(bitmap: &Bound<'_, PyAny>, options: Option<&Bound<'_, PyAny>>) -> PyResult<String> {
    let bitmap = mono_arg(bitmap)?;
    let options: ImageOptions = from_py(options)?;
    fontpixel::render::to_svg(&bitmap, &options).py()
}

/// An RGBA color dict (`Rgba::new`).
#[pyfunction]
#[pyo3(signature = (red, green, blue, alpha = 255))]
fn rgba(py: Python<'_>, red: u8, green: u8, blue: u8, alpha: u8) -> PyResult<Bound<'_, PyAny>> {
    to_py(py, &Rgba::new(red, green, blue, alpha))
}

/// Parse `#RGB`, `#RGBA`, `#RRGGBB`, `#RRGGBBAA` or `transparent` into a
/// color dict (`Rgba: FromStr`).
#[pyfunction]
#[pyo3(signature = (text))]
fn parse_rgba<'py>(py: Python<'py>, text: &str) -> PyResult<Bound<'py, PyAny>> {
    let color: Rgba = text.parse().py()?;
    to_py(py, &color)
}

/// Lenient SPEC direction parsing: invalid strings give `lrtb`
/// (`WritingDirection::from_spec`). Returns `{"glyphs", "lines"}`.
#[pyfunction]
#[pyo3(signature = (spec))]
fn writing_direction_from_spec<'py>(py: Python<'py>, spec: &str) -> PyResult<Bound<'py, PyAny>> {
    to_py(py, &WritingDirection::from_spec(spec))
}

/// Strict direction parsing (`WritingDirection: FromStr`); raises
/// `InvalidInputError` for unknown spellings.
#[pyfunction]
#[pyo3(signature = (spec))]
fn parse_writing_direction<'py>(py: Python<'py>, spec: &str) -> PyResult<Bound<'py, PyAny>> {
    let direction: WritingDirection = spec.parse().py()?;
    to_py(py, &direction)
}

/// Default `RenderOptions` as a dict.
#[pyfunction]
fn default_render_options(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    to_py(py, &RenderOptions::default())
}

/// Default `RenderLimits` as a dict.
#[pyfunction]
fn default_render_limits(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    to_py(py, &RenderLimits::default())
}

/// Default `ImageOptions` as a dict.
#[pyfunction]
fn default_image_options(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    to_py(py, &ImageOptions::default())
}

/// Register the render classes, functions and color constants.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = m.py();
    m.add_class::<PyRendered>()?;
    m.add_function(wrap_pyfunction!(to_png, m)?)?;
    m.add_function(wrap_pyfunction!(to_svg, m)?)?;
    m.add_function(wrap_pyfunction!(rgba, m)?)?;
    m.add_function(wrap_pyfunction!(parse_rgba, m)?)?;
    m.add_function(wrap_pyfunction!(writing_direction_from_spec, m)?)?;
    m.add_function(wrap_pyfunction!(parse_writing_direction, m)?)?;
    m.add_function(wrap_pyfunction!(default_render_options, m)?)?;
    m.add_function(wrap_pyfunction!(default_render_limits, m)?)?;
    m.add_function(wrap_pyfunction!(default_image_options, m)?)?;
    m.add("RGBA_BLACK", to_py(py, &Rgba::BLACK)?)?;
    m.add("RGBA_WHITE", to_py(py, &Rgba::WHITE)?)?;
    m.add("RGBA_TRANSPARENT", to_py(py, &Rgba::TRANSPARENT)?)?;
    Ok(())
}
