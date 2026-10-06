//! `fontpixel.outline`: deterministic bitmap contours, segments and
//! rasterization (`fontpixel::outline`), plus `fontpixel.outline.path` for
//! line/quadratic/cubic edge rasterization.
//!
//! Points are tuples `(x, y, on_curve)` in upward design units; contours are
//! lists of points without a repeated closing point. Bitmap inputs are Mono
//! `fontpixel.Bitmap` handles (Gray8 raises `InvalidInputError`: threshold
//! explicitly first); rasterizers return new Mono `Bitmap` handles.
use crate::errors::IntoPy;
use crate::handles::PyBitmap;
use crate::sfnt_support::{dict, mono, mono_handle, opt, out, req, submodule};
use fontpixel::outline::{self, Contour, Limits, Options, path};
use pyo3::prelude::*;

/// `outline::contours`: trace a Mono bitmap into closed contours.
#[pyfunction]
#[pyo3(signature = (bitmap, options = None, limits = None))]
fn contours<'py>(
    py: Python<'py>,
    bitmap: &PyBitmap,
    options: Option<&Bound<'py, PyAny>>,
    limits: Option<&Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyAny>> {
    let b = mono(bitmap)?;
    let o: Options = opt(options)?;
    let l: Limits = opt(limits)?;
    out(py, &py.detach(|| outline::contours(&b, &o, &l)).py()?)
}

/// `outline::pixel_contour`: one independent square/dot inside the cell at `(0, 0)`.
#[pyfunction]
#[pyo3(signature = (options = None))]
fn pixel_contour<'py>(
    py: Python<'py>,
    options: Option<&Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyAny>> {
    let o: Options = opt(options)?;
    out(py, &outline::pixel_contour(&o).py()?)
}

/// `outline::pixel_origins`: lower-left positions of ink cells, top-to-bottom.
#[pyfunction]
#[pyo3(signature = (bitmap, options = None, limits = None))]
fn pixel_origins<'py>(
    py: Python<'py>,
    bitmap: &PyBitmap,
    options: Option<&Bound<'py, PyAny>>,
    limits: Option<&Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyAny>> {
    let b = mono(bitmap)?;
    let o: Options = opt(options)?;
    let l: Limits = opt(limits)?;
    out(py, &outline::pixel_origins(&b, &o, &l).py()?)
}

/// `outline::segments`: segment dicts of a contour, with implied on-curve
/// midpoints resolved.
#[pyfunction]
fn segments<'py>(py: Python<'py>, contour: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    let c: Contour = req(contour)?;
    out(py, &outline::segments(&c).py()?)
}

/// `outline::rasterize`: unhinted nonzero-winding pixel-centre fill of
/// TrueType contours into a `width`×`height` Mono bitmap; `origin` and the
/// dimensions are in pixels, `units_per_pixel` is positive.
#[pyfunction]
#[pyo3(signature = (contours, units_per_pixel, origin, width, height, limits = None))]
fn rasterize(
    py: Python<'_>,
    contours: &Bound<'_, PyAny>,
    units_per_pixel: f64,
    origin: (i32, i32),
    width: u32,
    height: u32,
    limits: Option<&Bound<'_, PyAny>>,
) -> PyResult<PyBitmap> {
    let cs: Vec<Contour> = req(contours)?;
    let l: Limits = opt(limits)?;
    let b = py
        .detach(|| outline::rasterize(&cs, units_per_pixel, origin, width, height, &l))
        .py()?;
    Ok(mono_handle(b))
}

/// `outline::spec_bitmap_contours`: the exact SPEC row adapter shared with the
/// bdfparser compatibility layer (`'0'` is paper, any other byte ink).
#[pyfunction]
fn spec_bitmap_contours<'py>(
    py: Python<'py>,
    rows: Vec<String>,
    shape: &str,
    dotsize: f64,
    ppu: i64,
    xoff: i64,
    yoff: i64,
) -> PyResult<Bound<'py, PyAny>> {
    out(
        py,
        &outline::spec_bitmap_contours(&rows, shape, dotsize, ppu, xoff, yoff).py()?,
    )
}

/// `path::rasterize`: nonzero-winding fill at exact pixel centres of
/// line/quad/cubic edge dicts into a Mono bitmap.
#[pyfunction]
#[pyo3(name = "rasterize", signature = (edges, scale, origin, width, height, limits = None))]
fn path_rasterize(
    py: Python<'_>,
    edges: &Bound<'_, PyAny>,
    scale: f64,
    origin: (f64, f64),
    width: u32,
    height: u32,
    limits: Option<&Bound<'_, PyAny>>,
) -> PyResult<PyBitmap> {
    let e: Vec<path::Edge> = req(edges)?;
    let l: Limits = opt(limits)?;
    let b = py
        .detach(|| path::rasterize(&e, scale, origin, width, height, &l))
        .py()?;
    Ok(mono_handle(b))
}

/// `Edge::points`: endpoints and controls of an edge dict, in path order.
#[pyfunction]
fn edge_points(edge: &Bound<'_, PyAny>) -> PyResult<Vec<(f64, f64)>> {
    Ok(req::<path::Edge>(edge)?.points())
}

/// `Edge::max_y`: the largest y of an edge dict's points.
#[pyfunction]
fn edge_max_y(edge: &Bound<'_, PyAny>) -> PyResult<f64> {
    Ok(req::<path::Edge>(edge)?.max_y())
}

/// Default `Options` and `Limits` dicts of `fontpixel.outline`.
#[pyfunction]
#[pyo3(name = "defaults")]
fn outline_defaults<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
    dict(
        py,
        vec![
            ("Options", out(py, &Options::default())?),
            ("Limits", out(py, &Limits::default())?),
        ],
    )
}

/// Register `fontpixel.outline` and `fontpixel.outline.path`.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let o = submodule(
        m,
        "outline",
        "fontpixel.outline",
        "Deterministic bitmap contours in upward-facing font design coordinates.",
    )?;
    o.add_function(wrap_pyfunction!(contours, &o)?)?;
    o.add_function(wrap_pyfunction!(pixel_contour, &o)?)?;
    o.add_function(wrap_pyfunction!(pixel_origins, &o)?)?;
    o.add_function(wrap_pyfunction!(segments, &o)?)?;
    o.add_function(wrap_pyfunction!(rasterize, &o)?)?;
    o.add_function(wrap_pyfunction!(spec_bitmap_contours, &o)?)?;
    o.add_function(wrap_pyfunction!(outline_defaults, &o)?)?;
    let p = submodule(
        &o,
        "path",
        "fontpixel.outline.path",
        "Line/quadratic/cubic edge rasterization.",
    )?;
    p.add_function(wrap_pyfunction!(path_rasterize, &p)?)?;
    p.add_function(wrap_pyfunction!(edge_points, &p)?)?;
    p.add_function(wrap_pyfunction!(edge_max_y, &p)?)?;
    Ok(())
}
