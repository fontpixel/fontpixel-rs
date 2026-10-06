//! Native half of `fontpixel.compat.bdfparser` (the bdfparser SPEC-v3 API).
//!
//! The classes here bind `fontpixel::compat::bdfparser` one to one and live in
//! the private native submodule `fontpixel._native._bdfparser`:
//!
//! - `Bitmap` is the public compatibility bitmap class itself (it is re-exported
//!   unchanged as `fontpixel.compat.bdfparser.Bitmap`);
//! - `RawFont` and `RawGlyph` are handles over the Rust `Font`/`Glyph`; the Python
//!   package wraps them in the bdfparser `Font`/`Glyph` classes, which add Python
//!   idioms (keyword arguments, live `headers`/`props`/`glyphs` views, iterators and
//!   `warnings` integration).
//!
//! The Rust façade shares one native model between a font and its attached glyph
//! handles through `Rc`, so these classes are `unsendable`: a handle may only be
//! used from the thread that created it. Errors are translated by
//! `fontpixel.compat.bdfparser.errors._translate` into bdfparser's exception
//! classes (`ValueError`, `KeyError`, `BDFParseError`, `PCFError`, …), which also
//! derive from the matching `fontpixel` exception and keep its attributes.
//! Exceptions raised by Python callbacks (converters, transform functions) are
//! re-raised unchanged once the Rust call returns.
use crate::compat_bitmap::Bitmap;
use crate::compat_glyph::RawGlyph;
use crate::errors::to_py;
use crate::handles::PyFont;
use fontpixel::compat::bdfparser as bp;
use fontpixel::formats::pcf::PcfWriteOptions;
use pyo3::exceptions::{PyIndexError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyBytes, PyDict, PyFloat, PyInt, PyList, PyString, PyTuple};
use serde_json::{Map, Number, Value as V};
use std::cell::RefCell;

thread_local! {
    /// The first exception raised by a Python callback during the current Rust call.
    static PENDING: RefCell<Option<PyErr>> = const { RefCell::new(None) };
}

/// Remember a callback exception; only the first one of a call is kept.
pub(crate) fn stash(err: PyErr) {
    PENDING.with(|p| {
        let mut p = p.borrow_mut();
        if p.is_none() {
            *p = Some(err);
        }
    });
}

fn take_pending() -> Option<PyErr> {
    PENDING.with(|p| p.borrow_mut().take())
}

/// Translate a native error with the given context (`""`, `"pcf"`, …).
pub(crate) fn translate(err: PyErr, ctx: &str) -> PyErr {
    Python::attach(|py| {
        let converted = py
            .import("fontpixel.compat.bdfparser.errors")
            .and_then(|m| m.getattr("_translate"))
            .and_then(|f| f.call1((err.value(py), ctx)));
        match converted {
            Ok(exc) => PyErr::from_value(exc),
            Err(_) => err,
        }
    })
}

/// Finish a Rust call: a pending callback exception wins, then the Rust error.
pub(crate) fn done<T>(result: fontpixel::Result<T>) -> PyResult<T> {
    done_ctx(result, "")
}

/// [`done`] with an error-translation context.
pub(crate) fn done_ctx<T>(result: fontpixel::Result<T>, ctx: &str) -> PyResult<T> {
    if let Some(err) = take_pending() {
        return Err(err);
    }
    result.map_err(|e| translate(to_py(e), ctx))
}

/// Emit a `BDFParserWarning`.
pub(crate) fn warn(py: Python<'_>, message: &str) -> PyResult<()> {
    let category = py
        .import("fontpixel.compat.bdfparser.errors")?
        .getattr("BDFParserWarning")?;
    py.import("warnings")?
        .getattr("warn")?
        .call1((message, category, 3))?;
    Ok(())
}

/// Convert a JSON value (arbitrary-precision numbers) to a Python object.
pub(crate) fn v2py<'py>(py: Python<'py>, v: &V) -> PyResult<Bound<'py, PyAny>> {
    Ok(match v {
        V::Null => py.None().into_bound(py),
        V::Bool(b) => PyBool::new(py, *b).to_owned().into_any(),
        V::Number(n) => {
            if let Some(i) = n.as_i64() {
                i.into_pyobject(py)?.into_any()
            } else if let Some(u) = n.as_u64() {
                u.into_pyobject(py)?.into_any()
            } else {
                let text = n.to_string();
                if text.contains(['.', 'e', 'E']) {
                    PyFloat::new(py, n.as_f64().unwrap_or(f64::NAN)).into_any()
                } else {
                    py.get_type::<PyInt>().call1((text,))?
                }
            }
        }
        V::String(s) => PyString::new(py, s).into_any(),
        V::Array(a) => {
            let items = a.iter().map(|x| v2py(py, x)).collect::<PyResult<Vec<_>>>()?;
            PyList::new(py, items)?.into_any()
        }
        V::Object(o) => {
            let d = PyDict::new(py);
            for (k, x) in o {
                d.set_item(k, v2py(py, x)?)?;
            }
            d.into_any()
        }
    })
}

/// Convert a Python value (None, bool, int, float, str, list/tuple, dict) to JSON.
pub(crate) fn py2v(obj: &Bound<'_, PyAny>) -> PyResult<V> {
    if obj.is_none() {
        return Ok(V::Null);
    }
    if let Ok(b) = obj.cast::<PyBool>() {
        return Ok(V::Bool(b.is_true()));
    }
    if obj.is_instance_of::<PyInt>() {
        if let Ok(i) = obj.extract::<i64>() {
            return Ok(V::from(i));
        }
        if let Ok(u) = obj.extract::<u64>() {
            return Ok(V::from(u));
        }
        let text = obj.str()?.to_string();
        return serde_json::from_str::<Number>(&text)
            .map(V::Number)
            .map_err(|e| PyValueError::new_err(e.to_string()));
    }
    if let Ok(f) = obj.cast::<PyFloat>() {
        let f = f.value();
        return Number::from_f64(f)
            .map(V::Number)
            .ok_or_else(|| PyValueError::new_err(format!("non-finite number {f}")));
    }
    if let Ok(s) = obj.cast::<PyString>() {
        return Ok(V::String(s.to_str()?.to_owned()));
    }
    if let Ok(d) = obj.cast::<PyDict>() {
        return py2map(d);
    }
    if obj.is_instance_of::<PyList>() || obj.is_instance_of::<PyTuple>() {
        return Ok(V::Array(
            obj.try_iter()?
                .map(|x| py2v(&x?))
                .collect::<PyResult<Vec<_>>>()?,
        ));
    }
    Err(PyTypeError::new_err(format!(
        "unsupported value of type {}",
        obj.get_type().name()?
    )))
}

fn py2map(d: &Bound<'_, PyDict>) -> PyResult<V> {
    let mut m = Map::new();
    for (k, x) in d.iter() {
        m.insert(k.str()?.to_string(), py2v(&x)?);
    }
    Ok(V::Object(m))
}

/// A Python mapping as a JSON object.
pub(crate) fn py2obj(obj: &Bound<'_, PyAny>) -> PyResult<Map<String, V>> {
    match py2v(obj)? {
        V::Object(m) => Ok(m),
        _ => Err(PyTypeError::new_err("expected a dict")),
    }
}

/// Optional Python value (None → JSON null).
pub(crate) fn opt2v(obj: Option<&Bound<'_, PyAny>>) -> PyResult<V> {
    obj.map_or(Ok(V::Null), py2v)
}

/// Wrap a Python `str -> int | None` (`char2cp`) callable as a numeric converter.
fn char2cp_converter(f: Py<PyAny>) -> bp::Converter {
    bp::Converter::new(move |n| {
        Python::attach(|py| {
            let ch = match py
                .import("builtins")
                .and_then(|b| b.getattr("chr"))
                .and_then(|c| c.call1((n,)))
            {
                Ok(ch) => ch,
                Err(e) => {
                    stash(e);
                    return None;
                }
            };
            match f.call1(py, (ch,)) {
                Ok(r) if r.is_none(py) => None,
                Ok(r) => match r.extract::<u32>(py) {
                    Ok(v) => Some(v),
                    Err(e) => {
                        stash(e);
                        None
                    }
                },
                Err(e) => {
                    stash(e);
                    None
                }
            }
        })
    })
}

/// Wrap a Python `int -> str | None` (`cp2char`) callable as a numeric converter.
fn cp2char_converter(f: Py<PyAny>) -> bp::Converter {
    bp::Converter::new(move |n| {
        Python::attach(|py| match f.call1(py, (n,)) {
            Ok(r) if r.is_none(py) => None,
            Ok(r) => {
                let r = r.bind(py);
                match r.cast::<PyString>() {
                    Ok(s) => {
                        let ord = py.import("builtins").and_then(|b| b.getattr("ord"));
                        match s.len() {
                            Ok(1) => ord
                                .and_then(|o| o.call1((s,)))
                                .and_then(|v| v.extract::<u32>())
                                .ok(),
                            _ => None,
                        }
                    }
                    Err(_) => {
                        stash(PyTypeError::new_err("cp2char must return a str or None"));
                        None
                    }
                }
            }
            Err(e) => {
                stash(e);
                None
            }
        })
    })
}

/// Python file-like object (`read(n) -> bytes`) as a Rust reader.
struct PyReader(Py<PyAny>);
impl std::io::Read for PyReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        Python::attach(|py| {
            let chunk = self
                .0
                .call_method1(py, "read", (buf.len(),))
                .map_err(|e| {
                    stash(e);
                    std::io::Error::other("Python read() failed")
                })?;
            let chunk = chunk.bind(py);
            let data: Vec<u8> = if let Ok(s) = chunk.cast::<PyString>() {
                s.to_str()
                    .map_err(|e| {
                        stash(e);
                        std::io::Error::other("invalid text")
                    })?
                    .as_bytes()
                    .to_vec()
            } else {
                chunk.extract::<Vec<u8>>().map_err(|e| {
                    stash(e);
                    std::io::Error::other("read() must return bytes")
                })?
            };
            if data.len() > buf.len() {
                stash(PyValueError::new_err("read() returned more data than requested"));
                return Err(std::io::Error::other("oversized read"));
            }
            buf[..data.len()].copy_from_slice(&data);
            Ok(data.len())
        })
    }
}

/// Python file-like object (`write(bytes)`) as a Rust writer.
struct PyWriter(Py<PyAny>);
impl std::io::Write for PyWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        Python::attach(|py| {
            self.0
                .call_method1(py, "write", (PyBytes::new(py, buf),))
                .map_err(|e| {
                    stash(e);
                    std::io::Error::other("Python write() failed")
                })?;
            Ok(buf.len())
        })
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Typed `topcf`/`write_pcf` options from a dict with `PcfWriteOptions` field names.
pub(crate) fn pcf_options(opts: Option<&Bound<'_, PyAny>>) -> PyResult<PcfWriteOptions> {
    crate::conv::from_py(opts)
}

/// Typed `tottf`/`write_ttf` options from a dict with `TtfOptions` field names.
pub(crate) fn ttf_options(opts: Option<&Bound<'_, PyAny>>) -> PyResult<bp::ttf::TtfOptions> {
    crate::conv::from_py(opts)
}

fn save_options(format: Option<&str>, opts: Option<&Bound<'_, PyAny>>) -> PyResult<Option<bp::SaveOptions>> {
    Ok(match format.map(str::to_ascii_lowercase).as_deref() {
        None => None,
        Some("bdf") => Some(bp::SaveOptions::Bdf),
        Some("pcf") => Some(bp::SaveOptions::Pcf(pcf_options(opts)?)),
        Some("ttf") => Some(bp::SaveOptions::Ttf(ttf_options(opts)?)),
        Some("svg") => Some(bp::SaveOptions::Svg),
        Some(other) => {
            return Err(PyValueError::new_err(format!(
                "Unknown output format {other:?} (use bdf, pcf, ttf or svg)"
            )));
        }
    })
}

/// Handle over `fontpixel::compat::bdfparser::Font` (wrapped by the Python `Font`).
#[pyclass(name = "RawFont", module = "fontpixel._native._bdfparser", unsendable)]
pub struct RawFont {
    /// The Rust compatibility font; attached glyph handles share its model.
    pub font: bp::Font,
}

impl RawFont {
    pub(crate) fn wrap(font: bp::Font) -> Self {
        Self { font }
    }
}

fn header_property(key: &str) -> Option<(&'static str, &'static [&'static str])> {
    Some(match key {
        "bdfversion" => ("STARTFONT", &["bdfversion"]),
        "fontname" => ("FONT", &["fontname"]),
        "pointsize" | "xres" | "yres" => ("SIZE", &["pointsize", "xres", "yres"]),
        "fbbx" | "fbby" | "fbbxoff" | "fbbyoff" => {
            ("FONTBOUNDINGBOX", &["fbbx", "fbby", "fbbxoff", "fbbyoff"])
        }
        "swx0" | "swy0" => ("SWIDTH", &["swx0", "swy0"]),
        "dwx0" | "dwy0" => ("DWIDTH", &["dwx0", "dwy0"]),
        "swx1" | "swy1" => ("SWIDTH1", &["swx1", "swy1"]),
        "dwx1" | "dwy1" => ("DWIDTH1", &["dwx1", "dwy1"]),
        "vvectorx" | "vvectory" => ("VVECTOR", &["vvectorx", "vvectory"]),
        "metricsset" => ("METRICSSET", &["metricsset"]),
        "contentversion" => ("CONTENTVERSION", &["contentversion"]),
        "comment" => ("COMMENT", &["comment"]),
        _ => return None,
    })
}

fn property_value(v: &V) -> fontpixel::model::PropertyValue {
    use fontpixel::model::PropertyValue as P;
    match v {
        V::Null => P::Flag,
        V::Bool(b) => P::Integer(i64::from(*b)),
        V::Number(n) => n
            .as_i64()
            .map(P::Integer)
            .or_else(|| n.as_u64().map(P::Unsigned))
            .unwrap_or(P::Decimal(n.to_string())),
        V::String(s) => P::Text(s.clone()),
        V::Array(a) => P::List(a.iter().map(property_value).collect()),
        V::Object(_) => P::Text(v.to_string()),
    }
}

impl RawFont {
    /// Edit a copy of the native model and re-wrap it with `Font::from_core`.
    ///
    /// The compatibility façade has no public header or unencoded-slot editors,
    /// so these edits go through the native model. The rebuilt font starts a new
    /// shared state: glyph handles attached before the edit no longer write
    /// through, and the warning report restarts empty.
    fn rebuild(
        &mut self,
        edit: impl FnOnce(&mut fontpixel::model::Font) -> PyResult<()>,
    ) -> PyResult<()> {
        let mut core = self.font.core();
        edit(&mut core)?;
        let charset = core.charset.name.clone();
        let mut font = done(bp::Font::from_core(core))?;
        if let Some(name) = charset {
            font.encoding(&name);
        }
        self.font = font;
        Ok(())
    }
}

fn unencoded_ids(core: &fontpixel::model::Font) -> Vec<fontpixel::model::GlyphId> {
    core.glyphs()
        .filter(|(_, g)| g.codepoints.is_empty())
        .map(|(id, _)| id)
        .collect()
}

fn replace_headers(core: &mut fontpixel::model::Font, headers: &Map<String, V>) {
    core.headers = Default::default();
    let mut emitted = std::collections::BTreeSet::new();
    for k in headers.keys() {
        let Some((name, group)) = header_property(k) else {
            continue;
        };
        if !emitted.insert(name) {
            continue;
        }
        let value = if group.len() == 1 {
            headers[k].clone()
        } else {
            V::Array(
                group
                    .iter()
                    .map(|g| headers.get(*g).cloned().unwrap_or(V::Null))
                    .collect(),
            )
        };
        core.headers
            .push(fontpixel::model::Property::new(name, property_value(&value)));
    }
}

#[pymethods]
impl RawFont {
    /// An empty font (`Font::default`).
    #[new]
    fn new() -> Self {
        Self::wrap(bp::Font::default())
    }

    /// Load BDF text (`Font::load_string`).
    fn load_string(&mut self, text: &str) -> PyResult<()> {
        done(self.font.load_string(text).map(|_| ()))
    }

    /// Load BDF lines (`Font::load_lines`); line delimiters are not changed.
    fn load_lines(&mut self, lines: Vec<String>) -> PyResult<()> {
        done(self.font.load_lines(lines).map(|_| ()))
    }

    /// Load auto-detected BDF/PCF/TTF bytes (`Font::load_bytes_with_options`).
    #[pyo3(signature = (data, pixelsize=None))]
    fn load_bytes(&mut self, data: &[u8], pixelsize: Option<i64>) -> PyResult<()> {
        let ctx = if data.starts_with(b"\x01fcp") { "pcf" } else { "" };
        done_ctx(
            self.font
                .load_bytes_with_options(data, pixelsize)
                .map(|_| ()),
            ctx,
        )
    }

    /// Load glyph-based TrueType bytes (`Font::load_ttf`).
    #[pyo3(signature = (data, pixelsize=None))]
    fn load_ttf(&mut self, data: &[u8], pixelsize: Option<i64>) -> PyResult<()> {
        done(self.font.load_ttf(data, pixelsize).map(|_| ()))
    }

    /// Load a file by path (`Font::load_file_path`).
    fn load_file_path(&mut self, path: std::path::PathBuf) -> PyResult<()> {
        done(self.font.load_file_path(path).map(|_| ()))
    }

    /// Load from a binary file object with `read(n)` (`Font::load_file_obj`).
    fn load_file_obj(&mut self, reader: Py<PyAny>) -> PyResult<()> {
        done(self.font.load_file_obj(PyReader(reader)).map(|_| ()))
    }

    /// New font from `CreateOptions` field names (`Font::create`).
    #[staticmethod]
    #[pyo3(signature = (options=None))]
    fn create(options: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        let o: bp::CreateOptions = crate::conv::from_py(options)?;
        done(bp::Font::create(&o)).map(Self::wrap)
    }

    /// Wrap a native `fontpixel.Font` (`Font::from_core`); the native font is copied.
    #[staticmethod]
    fn from_core(font: PyRef<'_, PyFont>) -> PyResult<Self> {
        let core = font.snapshot()?;
        done(bp::Font::from_core(core)).map(Self::wrap)
    }

    /// A detached native `fontpixel.Font` copy of the model (`Font::core`).
    fn core(&self) -> PyFont {
        PyFont::new(self.font.core())
    }

    /// Deep copy (`Font::clone`): converters are shared, the model is copied.
    fn clone(&self) -> Self {
        Self::wrap(self.font.clone())
    }

    /// Structured warnings collected so far (`Font::report`), as plain data.
    fn report<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        crate::conv::to_py(py, &self.font.report())
    }

    /// Messages of the warning-level report entries from index `start` on (the
    /// ones bdfparser emits as `BDFParserWarning`), and the new entry count.
    /// Info-level entries (native repairs and derivations) are not warnings.
    fn warnings_since(&self, start: usize) -> (Vec<String>, usize) {
        let report = self.font.report();
        let n = report.entries.len();
        (
            report
                .entries
                .into_iter()
                .skip(start)
                .filter(|e| !matches!(e.level, fontpixel::report::Level::Info))
                .map(|e| e.message)
                .collect(),
            n,
        )
    }

    /// Projected SPEC headers (`Font::headers`).
    fn headers<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        v2py(py, &V::Object(self.font.headers()))
    }

    /// Replace all headers (rebuilds the model; attached glyph handles detach).
    fn set_headers(&mut self, headers: &Bound<'_, PyAny>) -> PyResult<()> {
        let h = py2obj(headers)?;
        self.rebuild(|core| {
            replace_headers(core, &h);
            Ok(())
        })
    }

    /// Set the alternate code (`bdf.altcode`) of the `index`-th unencoded glyph;
    /// None removes it (rebuilds the model; attached glyph handles detach).
    #[pyo3(signature = (index, altcode=None))]
    fn set_altcode(&mut self, index: usize, altcode: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        let alt = opt2v(altcode)?;
        self.rebuild(|core| {
            let id = *unencoded_ids(core)
                .get(index)
                .ok_or_else(|| PyIndexError::new_err("unencoded glyph index out of range"))?;
            done(core.edit_glyph(id, |g| {
                if alt.is_null() {
                    g.properties.remove("bdf.altcode");
                } else {
                    g.properties.set("bdf.altcode", property_value(&alt));
                }
                Ok(())
            }))
        })
    }

    /// Remove the `index`-th unencoded glyph, or all of them when `index` is None
    /// (rebuilds the model; attached glyph handles detach).
    #[pyo3(signature = (index=None))]
    fn remove_unencoded(&mut self, index: Option<usize>) -> PyResult<()> {
        self.rebuild(|core| {
            let ids = unencoded_ids(core);
            let selected = match index {
                None => ids,
                Some(i) => vec![
                    *ids.get(i)
                        .ok_or_else(|| PyIndexError::new_err("unencoded glyph index out of range"))?,
                ],
            };
            for id in selected {
                core.remove_glyph(id);
            }
            Ok(())
        })
    }

    /// Case-normalized properties (`Font::props`).
    fn props<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        v2py(py, &V::Object(self.font.props()))
    }

    /// Case-insensitive property lookup (`Font::getprop`).
    fn getprop<'py>(&self, py: Python<'py>, key: &str) -> PyResult<Bound<'py, PyAny>> {
        v2py(py, &self.font.getprop(key))
    }

    /// Set a property (`Font::setprop`).
    #[pyo3(signature = (key, value=None))]
    fn setprop(&mut self, key: &str, value: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        self.font.setprop(key, opt2v(value)?);
        Ok(())
    }

    /// Delete a property (`Font::delprop`).
    fn delprop(&mut self, key: &str) {
        self.font.delprop(key);
    }

    /// Canonical charset key (`Font::charset`).
    fn charset(&self) -> String {
        self.font.charset()
    }

    /// Override the charset ('' clears it) (`Font::encoding`).
    fn encoding(&mut self, name: &str) {
        self.font.encoding(name);
    }

    /// Install Python converters; None restores the charset table (`Font::set_converters`).
    #[pyo3(signature = (char2cp=None, cp2char=None))]
    fn set_converters(&mut self, char2cp: Option<Py<PyAny>>, cp2char: Option<Py<PyAny>>) {
        self.font.set_converters(
            char2cp.map(char2cp_converter),
            cp2char.map(cp2char_converter),
        );
    }

    /// Unicode scalar → code (`Font::unicode2cp`).
    fn unicode2cp(&self, code: u32) -> PyResult<Option<u32>> {
        done(self.font.unicode2cp(code))
    }

    /// Character → code (`Font::char2cp`).
    fn char2cp(&self, ch: &str) -> PyResult<Option<u32>> {
        done(self.font.char2cp(ch))
    }

    /// Code → Unicode number, surrogates retained (`Font::cp2unicode`).
    fn cp2unicode(&self, cp: i64) -> PyResult<Option<u32>> {
        done(self.font.cp2unicode(cp))
    }

    /// Code → character (`Font::cp2char`).
    fn cp2char(&self, cp: i64) -> PyResult<Option<char>> {
        done(self.font.cp2char(cp))
    }

    /// Number of encoded glyphs (`Font::length`).
    fn length(&self) -> usize {
        self.font.length()
    }

    /// Encoded codes in an order (0 file, -1 reverse, 1 ascending, 2 descending)
    /// filtered by a SPEC range (`Font::itercps`).
    #[pyo3(signature = (order=1, r=None))]
    fn itercps(&self, order: i64, r: Option<&Bound<'_, PyAny>>) -> PyResult<Vec<i64>> {
        done(self.font.itercps(order, &opt2v(r)?))
    }

    /// Attached handle, or None with a warning (`Font::glyphbycp`).
    fn glyphbycp(&self, cp: i64) -> Option<RawGlyph> {
        self.font.glyphbycp(cp).map(RawGlyph::wrap)
    }

    /// Attached handle by character (`Font::glyph`).
    fn glyph(&self, ch: &str) -> PyResult<Option<RawGlyph>> {
        done(self.font.glyph(ch)).map(|g| g.map(RawGlyph::wrap))
    }

    /// Attached handles in order and range (`Font::iterglyphs`).
    #[pyo3(signature = (order=1, r=None))]
    fn iterglyphs(&self, order: i64, r: Option<&Bound<'_, PyAny>>) -> PyResult<Vec<RawGlyph>> {
        done(self.font.iterglyphs(order, &opt2v(r)?))
            .map(|v| v.into_iter().map(RawGlyph::wrap).collect())
    }

    /// Characters without a glyph, or None (`Font::lacksglyphs`).
    fn lacksglyphs(&self, text: &str) -> PyResult<Option<Vec<String>>> {
        done(self.font.lacksglyphs(text))
    }

    /// SWIDTH from DWIDTH (`Font::swidth`).
    fn swidth(&self, dwx: i64) -> i64 {
        self.font.swidth(dwx)
    }

    /// Add glyph metadata (`Font::addglyph`).
    #[pyo3(signature = (fields, replace=true))]
    fn addglyph(&mut self, fields: &Bound<'_, PyAny>, replace: bool) -> PyResult<()> {
        let m = py2obj(fields)?;
        done(self.font.addglyph(&m, replace).map(|_| ()))
    }

    /// Add and attach a glyph handle (`Font::add_handle`).
    #[pyo3(signature = (glyph, replace=true))]
    fn add_handle(&mut self, mut glyph: PyRefMut<'_, RawGlyph>, replace: bool) -> PyResult<()> {
        done(self.font.add_handle(&mut glyph.glyph, replace).map(|_| ()))
    }

    /// Remove a code; missing codes are a no-op (`Font::removeglyph`).
    fn removeglyph(&mut self, cp: i64) {
        self.font.removeglyph(cp);
    }

    /// Remove exactly a handle's slot (`Font::remove_handle`).
    fn remove_handle(&mut self, mut glyph: PyRefMut<'_, RawGlyph>) {
        self.font.remove_handle(&mut glyph.glyph);
    }

    /// Create an attached glyph (`Font::newglyph`).
    #[pyo3(signature = (cp, bitmap=None, mode=0, name=None, dwidth=None))]
    fn newglyph(
        &mut self,
        cp: i64,
        bitmap: Option<PyRef<'_, Bitmap>>,
        mode: i64,
        name: Option<&str>,
        dwidth: Option<i64>,
    ) -> PyResult<RawGlyph> {
        let b = bitmap.as_ref().map(|b| &b.inner);
        done(self.font.newglyph(cp, b, mode, name, dwidth)).map(RawGlyph::wrap)
    }

    /// Union of positive glyph boxes into FONTBOUNDINGBOX (`Font::updatebbox`).
    fn updatebbox(&mut self) -> PyResult<()> {
        done(self.font.updatebbox().map(|_| ()))
    }

    /// Deep copy restricted to a range (`Font::subset`).
    fn subset(&self, r: &Bound<'_, PyAny>) -> PyResult<Self> {
        done(self.font.subset(&py2v(r)?)).map(Self::wrap)
    }

    /// Copy encoded glyphs from another font (`Font::merge`).
    #[pyo3(signature = (other, replace=false))]
    fn merge(&mut self, other: PyRef<'_, RawFont>, replace: bool) -> PyResult<()> {
        let other = other.font.clone();
        done(self.font.merge(&other, replace).map(|_| ()))
    }

    /// Apply a Python bitmap function to glyphs (`Font::transform`).
    #[pyo3(signature = (function, r=None, mode=0))]
    fn transform(
        &mut self,
        function: Py<PyAny>,
        r: Option<&Bound<'_, PyAny>>,
        mode: i64,
    ) -> PyResult<()> {
        let r = opt2v(r)?;
        done(
            self.font
                .transform(|b| crate::compat_bitmap::call_bitmap_fn(&function, b), &r, mode)
                .map(|_| ()),
        )
    }

    /// Re-encode to another charset (`Font::charsetconvert`).
    fn charsetconvert(&mut self, name: &str) -> PyResult<()> {
        done(self.font.charsetconvert(name).map(|_| ()))
    }

    /// Canonical full model: headers, props, propnames, glyphs, unencoded (`Font::model`).
    fn model<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        v2py(py, &self.font.model())
    }

    /// Serialize BDF text (`Font::tobdf`).
    fn tobdf(&self) -> PyResult<String> {
        done(self.font.tobdf())
    }

    /// Serialize PCF with `PcfWriteOptions` field names (`Font::topcf`).
    #[pyo3(signature = (options=None))]
    fn topcf<'py>(&self, py: Python<'py>, options: Option<&Bound<'_, PyAny>>) -> PyResult<Bound<'py, PyBytes>> {
        let o = pcf_options(options)?;
        done_ctx(self.font.topcf(&o), "pcf").map(|b| PyBytes::new(py, &b))
    }

    /// Serialize TrueType with `TtfOptions` field names (`Font::tottf`).
    #[pyo3(signature = (options=None))]
    fn tottf<'py>(&self, py: Python<'py>, options: Option<&Bound<'_, PyAny>>) -> PyResult<Bound<'py, PyBytes>> {
        let o = ttf_options(options)?;
        done(self.font.tottf(&o)).map(|b| PyBytes::new(py, &b))
    }

    /// Serialize an SVG font (`Font::tosvgfont`).
    #[pyo3(signature = (xoff=0.0, yoff=0.0, controlcharnoskip=false, unicodeformat="hexncr"))]
    fn tosvgfont(
        &self,
        xoff: f64,
        yoff: f64,
        controlcharnoskip: bool,
        unicodeformat: &str,
    ) -> PyResult<String> {
        done(self.font.tosvgfont(xoff, yoff, controlcharnoskip, unicodeformat))
    }

    /// Save to a path; `format` (bdf/pcf/ttf/svg) or the extension selects the
    /// writer, `options` holds its typed options (`Font::save`).
    #[pyo3(signature = (path, format=None, options=None))]
    fn save(
        &self,
        path: std::path::PathBuf,
        format: Option<&str>,
        options: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        let o = save_options(format, options)?;
        let ctx = if format == Some("pcf") { "pcf" } else { "" };
        done_ctx(self.font.save(path, o.as_ref()), ctx)
    }

    /// Serialize to a binary file object with `write(bytes)` (`Font::save_to`).
    #[pyo3(signature = (writer, format, options=None))]
    fn save_to(
        &self,
        writer: Py<PyAny>,
        format: &str,
        options: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        let o = save_options(Some(format), options)?.unwrap_or(bp::SaveOptions::Bdf);
        done(self.font.save_to(PyWriter(writer), &o))
    }

    /// Draw explicit codes with SPEC draw keywords (`Font::drawcps`).
    #[pyo3(signature = (cps, kw=None))]
    fn drawcps(&self, cps: Vec<i64>, kw: Option<&Bound<'_, PyAny>>) -> PyResult<Bitmap> {
        let kw = kw.map(py2obj).transpose()?.unwrap_or_default();
        done(self.font.drawcps(&cps, &kw)).map(Bitmap::wrap)
    }

    /// Draw text after charset conversion (`Font::draw`).
    #[pyo3(signature = (text, kw=None))]
    fn draw(&self, text: &str, kw: Option<&Bound<'_, PyAny>>) -> PyResult<Bitmap> {
        let kw = kw.map(py2obj).transpose()?.unwrap_or_default();
        done(self.font.draw(text, &kw)).map(Bitmap::wrap)
    }

    /// Draw all filtered glyphs (`Font::drawall`).
    #[pyo3(signature = (kw=None))]
    fn drawall(&self, kw: Option<&Bound<'_, PyAny>>) -> PyResult<Bitmap> {
        let kw = kw.map(py2obj).transpose()?.unwrap_or_default();
        done(self.font.drawall(&kw)).map(Bitmap::wrap)
    }
}

/// `defaultglyphname(cp, charset_is_unicode=True)`: `uniXXXX`, `uXXXXX` or `charN`.
#[pyfunction]
#[pyo3(signature = (cp, charset_is_unicode=true))]
fn defaultglyphname(cp: i64, charset_is_unicode: bool) -> String {
    bp::defaultglyphname(cp, charset_is_unicode)
}

/// `fmtnum(n)`: shortest round-trip number text used by SVG output.
#[pyfunction]
fn fmtnum(n: f64) -> String {
    bp::svg::fmtnum(n)
}

/// `read_ttf(data, pixelsize=None)`: glyph-based TrueType → font handle.
#[pyfunction]
#[pyo3(signature = (data, pixelsize=None))]
fn read_ttf(data: &[u8], pixelsize: Option<i64>) -> PyResult<RawFont> {
    done(bp::ttf::read_ttf(data, pixelsize)).map(RawFont::wrap)
}

/// `write_ttf(font, options=None)`: SPEC TrueType bytes (`TtfOptions` field names).
#[pyfunction]
#[pyo3(signature = (font, options=None))]
fn write_ttf<'py>(
    py: Python<'py>,
    font: PyRef<'_, RawFont>,
    options: Option<&Bound<'_, PyAny>>,
) -> PyResult<Bound<'py, PyBytes>> {
    let o = ttf_options(options)?;
    done(bp::ttf::write_ttf(&font.font, &o)).map(|b| PyBytes::new(py, &b))
}

fn point<'py>(py: Python<'py>, v: f64) -> PyResult<Bound<'py, PyAny>> {
    if v.fract() == 0.0 && v.abs() < 9.0e15 {
        Ok((v as i64).into_pyobject(py)?.into_any())
    } else {
        Ok(PyFloat::new(py, v).into_any())
    }
}

/// `bitmap_contours(bindata, shape, dotsize, ppu, xoff, yoff)`: SPEC outlines as
/// lists of `(x, y, on_curve)` tuples (integral coordinates as `int`).
#[pyfunction]
#[pyo3(signature = (bindata, shape="square", dotsize=1.0, ppu=1, xoff=0, yoff=0))]
fn bitmap_contours<'py>(
    py: Python<'py>,
    bindata: Vec<String>,
    shape: &str,
    dotsize: f64,
    ppu: i64,
    xoff: i64,
    yoff: i64,
) -> PyResult<Bound<'py, PyList>> {
    let cs = done(bp::outline::bitmap_contours(
        &bindata, shape, dotsize, ppu, xoff, yoff,
    ))?;
    let out = PyList::empty(py);
    for c in cs {
        let pts = PyList::empty(py);
        for (x, y, on) in c {
            pts.append(PyTuple::new(
                py,
                [point(py, x)?, point(py, y)?, PyBool::new(py, on).to_owned().into_any()],
            )?)?;
        }
        out.append(pts)?;
    }
    Ok(out)
}

/// `rasterize(contours, ppu, xmin, ymin, w, h)`: nonzero-winding pixel-centre
/// rasterization of `(x, y, on_curve)` contours (`outline::rasterize`).
#[pyfunction]
fn rasterize(
    contours: Vec<Vec<(f64, f64, bool)>>,
    ppu: f64,
    xmin: i64,
    ymin: i64,
    w: i64,
    h: i64,
) -> PyResult<Bitmap> {
    done(bp::outline::rasterize(&contours, ppu, xmin, ymin, w, h)).map(Bitmap::wrap)
}

/// Charset helpers used by `fontpixel.compat.bdfparser.encoding` (they call
/// `fontpixel::charset`, whose native bindings belong to the charset area).
mod charset_helpers {
    use super::*;
    use fontpixel::charset as cs;

    fn kind_name(k: cs::Kind) -> &'static str {
        match k {
            cs::Kind::Unicode => "unicode",
            cs::Kind::SingleByte => "sbcs",
            cs::Kind::DoubleByte => "dbcs",
            cs::Kind::Gb18030 => "gb18030",
        }
    }

    /// `sanitize_charset(registry, encoding)`.
    #[pyfunction]
    #[pyo3(signature = (registry=None, encoding=None))]
    pub fn charset_sanitize(registry: Option<&str>, encoding: Option<&str>) -> String {
        cs::sanitize_charset(registry, encoding)
    }
    /// `canonical_charset(name)`.
    #[pyfunction]
    pub fn charset_canonical(name: &str) -> String {
        cs::canonical_charset(name)
    }
    /// `is_unicode_charset(key)`.
    #[pyfunction]
    pub fn charset_is_unicode(key: &str) -> bool {
        cs::is_unicode_charset(key)
    }
    /// `charset_props(key)` → (registry, encoding).
    #[pyfunction]
    pub fn charset_props(key: &str) -> (String, String) {
        cs::charset_props(key)
    }
    /// Table rows: (key, codec, kind, gl_capable, force_gl, available).
    #[pyfunction]
    pub fn charset_rows<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyList>> {
        let out = PyList::empty(py);
        for e in cs::charset_table() {
            let d = PyDict::new(py);
            d.set_item("key", e.key)?;
            d.set_item("codec", e.python_codec)?;
            d.set_item("kind", kind_name(e.kind))?;
            d.set_item("gl", e.gl_capable)?;
            d.set_item("forcegl", e.force_gl)?;
            d.set_item("props", e.properties)?;
            d.set_item("aliases", e.aliases.to_vec())?;
            d.set_item("available", e.available())?;
            out.append(d)?;
        }
        Ok(out)
    }

    /// Codec handle (`fontpixel::charset::CharsetCodec`).
    #[pyclass(name = "RawCodec", module = "fontpixel._native._bdfparser", frozen)]
    pub struct RawCodec {
        pub codec: cs::CharsetCodec,
    }
    #[pymethods]
    impl RawCodec {
        /// `CharsetCodec::from_key(key, gl)`.
        #[new]
        #[pyo3(signature = (key, gl=None))]
        fn new(key: &str, gl: Option<bool>) -> PyResult<Self> {
            cs::CharsetCodec::from_key(key, gl)
                .map(|codec| Self { codec })
                .map_err(|e| PyValueError::new_err(e.to_string()))
        }
        /// Canonical key.
        fn name(&self) -> String {
            self.codec.name().to_owned()
        }
        /// Whether the key is a known charset.
        fn known(&self) -> bool {
            self.codec.known()
        }
        /// Kind name: unicode, sbcs, dbcs or gb18030.
        fn kind(&self) -> &'static str {
            kind_name(self.codec.kind())
        }
        /// GL (7-bit) form.
        fn gl(&self) -> bool {
            self.codec.gl()
        }
        /// Unicode number → code.
        fn unicode2cp(&self, n: u32) -> Option<u32> {
            self.codec.unicode2cp(n)
        }
        /// Code → Unicode number (surrogates retained).
        fn cp2unicode(&self, cp: u32) -> Option<u32> {
            self.codec.cp2unicode(cp)
        }
    }
}

/// Register `fontpixel._native._bdfparser`.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = m.py();
    let sub = PyModule::new(py, "_bdfparser")?;
    sub.add_class::<Bitmap>()?;
    sub.add_class::<RawFont>()?;
    sub.add_class::<RawGlyph>()?;
    sub.add_class::<charset_helpers::RawCodec>()?;
    sub.add("META_TITLES", PyTuple::new(py, bp::META_TITLES)?)?;
    sub.add_function(wrap_pyfunction!(defaultglyphname, &sub)?)?;
    sub.add_function(wrap_pyfunction!(fmtnum, &sub)?)?;
    sub.add_function(wrap_pyfunction!(read_ttf, &sub)?)?;
    sub.add_function(wrap_pyfunction!(write_ttf, &sub)?)?;
    sub.add_function(wrap_pyfunction!(bitmap_contours, &sub)?)?;
    sub.add_function(wrap_pyfunction!(rasterize, &sub)?)?;
    sub.add_function(wrap_pyfunction!(charset_helpers::charset_sanitize, &sub)?)?;
    sub.add_function(wrap_pyfunction!(charset_helpers::charset_canonical, &sub)?)?;
    sub.add_function(wrap_pyfunction!(charset_helpers::charset_is_unicode, &sub)?)?;
    sub.add_function(wrap_pyfunction!(charset_helpers::charset_props, &sub)?)?;
    sub.add_function(wrap_pyfunction!(charset_helpers::charset_rows, &sub)?)?;
    m.add_submodule(&sub)?;
    Ok(())
}
