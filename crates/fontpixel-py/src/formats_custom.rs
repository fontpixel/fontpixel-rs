//! Python-implemented codecs (`Registry.register`).
//!
//! A registration names a format (an existing catalogue ID whose entry has no
//! codec yet, or a new ID) and supplies any of three hooks:
//!
//! - `detector(ctx) -> "no_match" | "possible" | "certain"`
//!   (`FormatDetector::detect`);
//! - `reader(ctx, options) -> (FontSet, report)` (`FormatReader::read`;
//!   `options` is the `ReadOptions` dict);
//! - `writer`: a callable `writer(font_set, options)` or an object with a
//!   `write(font_set, options)` method (`FormatWriter::write`; `options` is
//!   the `WriteOptions` dict) returning a `Written`, an `OutputBundle` or
//!   `(OutputBundle, report)`. A writer object may also define
//!   `transport_limits() -> dict` and
//!   `validate_transport(font_set, options, limits)`; the other writer hooks
//!   keep their Rust defaults (borrowed route, no reservation).
//!
//! `ctx` is a `ReadContext` over the input the registry decoded: its primary
//! bytes (gzip transport already removed), name, directory flag and the
//! caller's resources, with limits reduced by what the registry already
//! retained. An exception raised by a hook propagates unchanged out of the
//! registry call that ran it.
use crate::conv;
use crate::errors::IntoPy;
use crate::formats::PyRegistry;
use crate::handles::PyFontSet;
use crate::resources::{InputParts, PyInput, PyOutputBundle, PyReadContext, PyWritten};
use fontpixel::formats::{
    Confidence, FormatDetector, FormatInfo, FormatReader, FormatWriter, ReadOptions, Registration,
    WriteOptions,
};
use fontpixel::resources::{NoResources, ReadContext, ResourceResolver, TransportLimits, Written};
use fontpixel::{Error, FontSet, Report};
use pyo3::prelude::*;
use pyo3::types::PyTuple;
use std::cell::RefCell;
use std::sync::Arc;

thread_local! {
    /// Resources of the input the registry is reading on this thread.
    static RESOURCES: RefCell<Option<Arc<dyn ResourceResolver>>> = const { RefCell::new(None) };
    /// The exception a Python hook raised during the current registry call.
    static RAISED: RefCell<Option<PyErr>> = const { RefCell::new(None) };
}

/// Run `f` with `resolver` visible to Python hooks on this thread.
pub fn with_resources<R>(resolver: &Arc<dyn ResourceResolver>, f: impl FnOnce() -> R) -> R {
    let previous = RESOURCES.with(|r| r.replace(Some(resolver.clone())));
    let out = f();
    RESOURCES.with(|r| *r.borrow_mut() = previous);
    out
}

/// Map a registry result to Python, re-raising a hook's own exception.
pub fn py_result<T>(result: fontpixel::Result<T>) -> PyResult<T> {
    match result {
        Ok(v) => {
            RAISED.with(|r| r.borrow_mut().take());
            Ok(v)
        }
        Err(e) => match RAISED.with(|r| r.borrow_mut().take()) {
            Some(raised) => Err(raised),
            None => Err(crate::errors::to_py(e)),
        },
    }
}

fn hook_error(err: PyErr) -> Error {
    let message = err.to_string();
    RAISED.with(|r| *r.borrow_mut() = Some(err));
    Error::InvalidInput(format!("Python codec hook failed: {message}"))
}

fn hook<T>(f: impl FnOnce(Python<'_>) -> PyResult<T>) -> fontpixel::Result<T> {
    Python::attach(f).map_err(hook_error)
}

/// A `ReadContext` handle for a hook, over the bytes the registry decoded.
fn context<'py>(py: Python<'py>, ctx: &ReadContext<'_>) -> PyResult<Bound<'py, PyAny>> {
    let resolver = RESOURCES
        .with(|r| r.borrow().clone())
        .unwrap_or_else(|| Arc::new(NoResources));
    let input = Py::new(
        py,
        PyInput {
            parts: InputParts {
                data: Arc::new(ctx.bytes().to_vec()),
                name: ctx.name().map(str::to_owned),
                resolver,
                directory: ctx.is_directory(),
            },
        },
    )?;
    let mut limits = ctx.limits().clone();
    limits.max_total_bytes = limits
        .max_total_bytes
        .saturating_sub(ctx.retained_input_bytes());
    let limits = conv::to_py(py, &limits)?;
    py.get_type::<PyReadContext>().call1((input, limits))
}

struct PyCodec {
    detector: Option<Py<PyAny>>,
    reader: Option<Py<PyAny>>,
    writer: Option<Py<PyAny>>,
}

impl FormatDetector for PyCodec {
    fn detect(&self, ctx: &mut ReadContext<'_>) -> fontpixel::Result<Confidence> {
        let f = self.detector.as_ref().ok_or(Error::UnrecognizedFormat)?;
        hook(|py| {
            let out = f.bind(py).call1((context(py, ctx)?,))?;
            conv::from_py_required(&out)
        })
    }
}

impl FormatReader<FontSet> for PyCodec {
    fn read(
        &self,
        ctx: &mut ReadContext<'_>,
        options: &ReadOptions,
    ) -> fontpixel::Result<(FontSet, Report)> {
        let f = self.reader.as_ref().ok_or(Error::UnrecognizedFormat)?;
        hook(|py| {
            let out = f
                .bind(py)
                .call1((context(py, ctx)?, conv::to_py(py, options)?))?;
            let (fonts, report): (Bound<'_, PyAny>, Bound<'_, PyAny>) = out.extract()?;
            let fonts = crate::formats::SetArg::extract(&fonts)?.into_owned()?;
            Ok((fonts, conv::from_py(Some(&report))?))
        })
    }
}

fn written(py: Python<'_>, out: &Bound<'_, PyAny>) -> PyResult<Written> {
    if out.cast::<PyWritten>().is_ok() {
        let output = out.getattr("output")?;
        let report = out.getattr("report")?;
        return Ok(Written {
            output: output.cast::<PyOutputBundle>()?.get().bundle.clone(),
            report: conv::from_py(Some(&report))?,
        });
    }
    if let Ok(bundle) = out.cast::<PyOutputBundle>() {
        return Ok(Written {
            output: bundle.get().bundle.clone(),
            report: Report::default(),
        });
    }
    if let Ok(pair) = out.cast::<PyTuple>() {
        let (bundle, report): (Bound<'_, PyOutputBundle>, Bound<'_, PyAny>) = pair.extract()?;
        return Ok(Written {
            output: bundle.get().bundle.clone(),
            report: conv::from_py(Some(&report))?,
        });
    }
    let _ = py;
    Err(pyo3::exceptions::PyTypeError::new_err(
        "a writer returns a Written, an OutputBundle or (OutputBundle, report)",
    ))
}

impl PyCodec {
    fn writer<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let w = self
            .writer
            .as_ref()
            .ok_or_else(|| crate::errors::to_py(Error::UnrecognizedFormat))?
            .bind(py)
            .clone();
        if w.hasattr("write")? {
            Ok(w.getattr("write")?)
        } else {
            Ok(w)
        }
    }
}

impl FormatWriter<FontSet> for PyCodec {
    fn write(&self, fonts: &FontSet, options: &WriteOptions) -> fontpixel::Result<Written> {
        hook(|py| {
            let set = Py::new(py, PyFontSet::new(fonts.clone()))?;
            let out = self.writer(py)?.call1((set, conv::to_py(py, options)?))?;
            written(py, &out)
        })
    }

    fn transport_limits(&self) -> TransportLimits {
        Python::attach(|py| {
            let w = self.writer.as_ref()?.bind(py);
            if !w.hasattr("transport_limits").ok()? {
                return None;
            }
            let out = w.call_method0("transport_limits").ok()?;
            conv::from_py_required(&out).ok()
        })
        .unwrap_or_default()
    }

    fn validate_transport(
        &self,
        fonts: &FontSet,
        options: &WriteOptions,
        limits: &TransportLimits,
    ) -> fontpixel::Result<()> {
        hook(|py| {
            let Some(w) = self.writer.as_ref() else {
                return Ok(());
            };
            let w = w.bind(py);
            if w.hasattr("validate_transport")? {
                let set = Py::new(py, PyFontSet::new(fonts.clone()))?;
                w.call_method1(
                    "validate_transport",
                    (set, conv::to_py(py, options)?, conv::to_py(py, limits)?),
                )?;
            }
            Ok(())
        })
    }
}

/// Catalogue metadata for a registration: the static catalogue entry for a
/// known ID, otherwise new metadata (its strings live for the process).
fn format_info(format: &Bound<'_, PyAny>) -> PyResult<FormatInfo> {
    if let Ok(id) = format.extract::<String>() {
        return fontpixel::formats::catalogue::CATALOGUE
            .iter()
            .copied()
            .find(|f| f.id == id)
            .ok_or_else(|| crate::errors::to_py(Error::UnknownFormat(id)));
    }
    let id: String = format.get_item("id")?.extract()?;
    if let Some(known) = fontpixel::formats::catalogue::CATALOGUE
        .iter()
        .copied()
        .find(|f| f.id == id)
    {
        return Ok(known);
    }
    let name: String = format.get_item("name")?.extract()?;
    let extensions: Vec<String> = match format.get_item("extensions") {
        Ok(e) => e.extract()?,
        Err(_) => Vec::new(),
    };
    let leak = |s: String| -> &'static str { Box::leak(s.into_boxed_str()) };
    let extensions: Vec<&'static str> = extensions.into_iter().map(leak).collect();
    Ok(FormatInfo {
        id: leak(id),
        name: leak(name),
        extensions: Box::leak(extensions.into_boxed_slice()),
        feature_enabled: true,
    })
}

#[pymethods]
impl PyRegistry {
    /// Register a format implemented in Python (`Registry::register` with a
    /// `Registration`). `format` is a catalogue ID or
    /// `{"id", "name", "extensions"}` for a new ID (lowercase ASCII letters,
    /// digits and hyphens). Without hooks this is an honest planned entry
    /// (`Registration::planned`). A live entry cannot be replaced: use
    /// `Registry(builtin=False)` to register a catalogue ID.
    #[pyo3(signature = (format, *, detector = None, reader = None, writer = None))]
    fn register(
        &mut self,
        format: &Bound<'_, PyAny>,
        detector: Option<Py<PyAny>>,
        reader: Option<Py<PyAny>>,
        writer: Option<Py<PyAny>>,
    ) -> PyResult<()> {
        let info = format_info(format)?;
        let mut registration = Registration::planned(info);
        let codec = || {
            Python::attach(|py| PyCodec {
                detector: detector.as_ref().map(|d| d.clone_ref(py)),
                reader: reader.as_ref().map(|d| d.clone_ref(py)),
                writer: writer.as_ref().map(|d| d.clone_ref(py)),
            })
        };
        if detector.is_some() {
            registration.detector = Some(Box::new(codec()));
        }
        if reader.is_some() {
            registration.reader = Some(Box::new(codec()));
        }
        if writer.is_some() {
            registration.writer = Some(Box::new(codec()));
        }
        self.registry.register(registration).py()
    }
}
