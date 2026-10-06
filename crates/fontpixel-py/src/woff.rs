//! `fontpixel.woff`: standalone sfnt ↔ WOFF1 (`fontpixel.woff.woff1`) and WOFF2
//! (`fontpixel.woff.woff2`, including collections) containers.
//!
//! Input and output fonts are `bytes`. `decode` returns
//! `{sfnt: bytes, auxiliary: {major_version, minor_version, metadata,
//! private_data}, report}` where `metadata`/`private_data` are `bytes` or
//! `None`; `encode` returns `{bytes, report}`. Options and limits are dicts
//! (see `defaults()`); the `auxiliary` option accepts `bytes` for its blocks.
//! The GIL is released while compressing or decompressing.
use crate::errors::IntoPy;
use crate::sfnt_support::{bytes_arg, dict, opt, out, pybytes, submodule};
use fontpixel::woff::{self, Auxiliary, Decoded, Encoded, woff1, woff2};
use pyo3::prelude::*;
use pyo3::types::PyList;

fn optional_bytes<'py>(py: Python<'py>, value: &Option<Vec<u8>>) -> Bound<'py, PyAny> {
    match value {
        Some(v) => pybytes(py, v),
        None => py.None().into_bound(py),
    }
}

/// The Auxiliary dict with byte blocks as `bytes`.
pub fn auxiliary<'py>(py: Python<'py>, a: &Auxiliary) -> PyResult<Bound<'py, PyAny>> {
    dict(
        py,
        vec![
            (
                "major_version",
                a.major_version.into_pyobject(py)?.into_any(),
            ),
            (
                "minor_version",
                a.minor_version.into_pyobject(py)?.into_any(),
            ),
            ("metadata", optional_bytes(py, &a.metadata)),
            ("private_data", optional_bytes(py, &a.private_data)),
        ],
    )
}

fn decoded<'py>(py: Python<'py>, d: &Decoded) -> PyResult<Bound<'py, PyAny>> {
    dict(
        py,
        vec![
            ("sfnt", pybytes(py, &d.sfnt)),
            ("auxiliary", auxiliary(py, &d.auxiliary)?),
            ("report", out(py, &d.report)?),
        ],
    )
}

fn encoded<'py>(py: Python<'py>, e: &Encoded) -> PyResult<Bound<'py, PyAny>> {
    dict(
        py,
        vec![
            ("bytes", pybytes(py, &e.bytes)),
            ("report", out(py, &e.report)?),
        ],
    )
}

/// `woff1::decode`: reconstruct the standalone sfnt from WOFF1 bytes.
#[pyfunction]
#[pyo3(name = "decode", signature = (data, limits = None))]
fn woff1_decode<'py>(
    py: Python<'py>,
    data: &Bound<'py, PyAny>,
    limits: Option<&Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyAny>> {
    let input = bytes_arg(data)?;
    let l: woff::Limits = opt(limits)?;
    decoded(py, &py.detach(|| woff1::decode(&input, &l)).py()?)
}

/// `woff1::encode`: deterministic WOFF1 of a standalone sfnt.
#[pyfunction]
#[pyo3(name = "encode", signature = (data, options = None, limits = None))]
fn woff1_encode<'py>(
    py: Python<'py>,
    data: &Bound<'py, PyAny>,
    options: Option<&Bound<'py, PyAny>>,
    limits: Option<&Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyAny>> {
    let input = bytes_arg(data)?;
    let o: woff1::Options = opt(options)?;
    let l: woff::Limits = opt(limits)?;
    encoded(py, &py.detach(|| woff1::encode(&input, &o, &l)).py()?)
}

/// `woff2::decode`: reconstruct the sfnt (or `ttcf` collection) from WOFF2.
#[pyfunction]
#[pyo3(name = "decode", signature = (data, limits = None))]
fn woff2_decode<'py>(
    py: Python<'py>,
    data: &Bound<'py, PyAny>,
    limits: Option<&Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyAny>> {
    let input = bytes_arg(data)?;
    let l: woff::Limits = opt(limits)?;
    decoded(py, &py.detach(|| woff2::decode(&input, &l)).py()?)
}

/// `woff2::encode`: deterministic WOFF2 of an sfnt or `ttcf` collection.
#[pyfunction]
#[pyo3(name = "encode", signature = (data, options = None, limits = None))]
fn woff2_encode<'py>(
    py: Python<'py>,
    data: &Bound<'py, PyAny>,
    options: Option<&Bound<'py, PyAny>>,
    limits: Option<&Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyAny>> {
    let input = bytes_arg(data)?;
    let o: woff2::Options = opt(options)?;
    let l: woff::Limits = opt(limits)?;
    encoded(py, &py.detach(|| woff2::encode(&input, &o, &l)).py()?)
}

/// `woff2::encode_many`: encode several fonts with up to `jobs` worker
/// threads (bounded by `max_batch_fonts`/`max_batch_bytes`); output order
/// matches input order.
#[pyfunction]
#[pyo3(signature = (inputs, options = None, limits = None, jobs = 1))]
fn encode_many<'py>(
    py: Python<'py>,
    inputs: Vec<Bound<'py, PyAny>>,
    options: Option<&Bound<'py, PyAny>>,
    limits: Option<&Bound<'py, PyAny>>,
    jobs: usize,
) -> PyResult<Bound<'py, PyList>> {
    let data = inputs
        .iter()
        .map(|i| bytes_arg(i))
        .collect::<PyResult<Vec<_>>>()?;
    let o: woff2::Options = opt(options)?;
    let l: woff::Limits = opt(limits)?;
    let results = py
        .detach(|| {
            let slices: Vec<&[u8]> = data.iter().map(|d| &**d).collect();
            woff2::encode_many(&slices, &o, &l, jobs)
        })
        .py()?;
    let list = PyList::empty(py);
    for e in &results {
        list.append(encoded(py, e)?)?;
    }
    Ok(list)
}

/// `woff2::brotli_encoder_heap_bound`: the admitted Brotli encoder heap bound
/// (proven for quality 0-9, an empirical envelope for 10-11), or `None` on
/// arithmetic overflow. WOFF2 always uses `lgwin = 22`.
#[pyfunction]
fn brotli_encoder_heap_bound(
    quality: u32,
    lgwin: u32,
    input_len: usize,
    threads: usize,
) -> Option<usize> {
    woff2::brotli_encoder_heap_bound(quality, lgwin, input_len, threads)
}

/// `woff2::brotli_encoder_proven_heap_bound`: the source-derived upper bound
/// of all encoder heap for the same call shape, or `None` on overflow.
#[pyfunction]
fn brotli_encoder_proven_heap_bound(
    quality: u32,
    lgwin: u32,
    input_len: usize,
    threads: usize,
) -> Option<usize> {
    woff2::brotli_encoder_proven_heap_bound(quality, lgwin, input_len, threads)
}

/// Default limit and option dicts of `fontpixel.woff`.
#[pyfunction]
#[pyo3(name = "defaults")]
fn woff_defaults<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
    dict(
        py,
        vec![
            ("Limits", out(py, &woff::Limits::default())?),
            ("Auxiliary", out(py, &Auxiliary::default())?),
            ("woff1.Options", out(py, &woff1::Options::default())?),
            ("woff2.Options", out(py, &woff2::Options::default())?),
        ],
    )
}

/// Register `fontpixel.woff`, `fontpixel.woff.woff1` and `fontpixel.woff.woff2`.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let w = submodule(
        m,
        "woff",
        "fontpixel.woff",
        "Bounded standalone sfnt <-> WOFF1/WOFF2 containers.",
    )?;
    w.add_function(wrap_pyfunction!(woff_defaults, &w)?)?;
    let one = submodule(&w, "woff1", "fontpixel.woff.woff1", "WOFF 1.0 (zlib).")?;
    one.add_function(wrap_pyfunction!(woff1_decode, &one)?)?;
    one.add_function(wrap_pyfunction!(woff1_encode, &one)?)?;
    let two = submodule(
        &w,
        "woff2",
        "fontpixel.woff.woff2",
        "WOFF 2.0 (Brotli, glyf/loca/hmtx transforms, collections).",
    )?;
    two.add_function(wrap_pyfunction!(woff2_decode, &two)?)?;
    two.add_function(wrap_pyfunction!(woff2_encode, &two)?)?;
    two.add_function(wrap_pyfunction!(encode_many, &two)?)?;
    two.add_function(wrap_pyfunction!(brotli_encoder_heap_bound, &two)?)?;
    two.add_function(wrap_pyfunction!(brotli_encoder_proven_heap_bound, &two)?)?;
    Ok(())
}
