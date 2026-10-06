//! Per-codec namespaces: `fontpixel.formats.<module>` for every Rust module
//! of `fontpixel::formats` that has a codec.
//!
//! Each namespace binds that module's own direct functions with their native
//! shapes:
//!
//! - `detect(data) -> str`: the module's structural detector (`Confidence`
//!   as `"no_match"`, `"possible"` or `"certain"`). Modules without a direct
//!   `detect` function run their registry codec's detector hook instead.
//! - `read(...)`: `(model, report)`, where `model` is a `FontSet` or `Font`
//!   handle exactly as the Rust function returns it. Context readers take
//!   `data`, `options`, `name`, `resources`, `limits` and `directory`; byte
//!   readers take `data`, `options` (when they have any) and `limits`.
//! - `write(...)`: a `Written` for writers that return one (they take
//!   `primary_name`, `strict`, `transport_limits`), otherwise
//!   `(bytes, report)`.
//!
//! `FORMAT_IDS` lists the catalogue IDs a namespace serves.
use crate::conv;
use crate::errors::IntoPy;
use crate::formats::{
    InputArgs, PyRegistry, SetArg, WriteArgs, WriteMode, bytes_arg, limits_arg,
    path_arg, with_font, write_options,
};
use crate::handles::{PyFont, PyFontSet};
use crate::resources::PyWritten;
use fontpixel::formats::{self as f, Confidence, FormatDetector};
use fontpixel::resources::{ReadContext, ResourcePath, Written};
use fontpixel::{Font, FontSet, Report, ResourceLimits};
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use serde::de::DeserializeOwned;

/// A decoded model returned to Python as its handle class.
pub trait IntoPyModel: Send {
    /// Wrap as a handle.
    fn into_handle(self, py: Python<'_>) -> PyResult<Py<PyAny>>;
}
impl IntoPyModel for FontSet {
    fn into_handle(self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        Ok(Py::new(py, PyFontSet::new(self))?.into_any())
    }
}
impl IntoPyModel for Font {
    fn into_handle(self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        Ok(Py::new(py, PyFont::new(self))?.into_any())
    }
}

/// A model argument: `Font` functions take a `Font` (or one-strike `FontSet`),
/// `FontSet` functions a `FontSet` (or one `Font`).
pub trait ModelArg: Sync {
    /// Run `f` with the model.
    fn with<R>(obj: &Bound<'_, PyAny>, f: impl FnOnce(&Self) -> PyResult<R>) -> PyResult<R>;
}
impl ModelArg for Font {
    fn with<R>(obj: &Bound<'_, PyAny>, f: impl FnOnce(&Self) -> PyResult<R>) -> PyResult<R> {
        with_font(obj, f)
    }
}
impl ModelArg for FontSet {
    fn with<R>(obj: &Bound<'_, PyAny>, f: impl FnOnce(&Self) -> PyResult<R>) -> PyResult<R> {
        SetArg::extract(obj)?.with(f)
    }
}

/// `(model, report)` result tuple.
pub type ModelReport = (Py<PyAny>, Py<PyAny>);

/// Convert a direct read result.
pub fn model_report<M: IntoPyModel>(
    py: Python<'_>,
    result: fontpixel::Result<(M, Report)>,
) -> PyResult<ModelReport> {
    let (model, report) = result.py()?;
    Ok((model.into_handle(py)?, conv::to_py(py, &report)?.unbind()))
}

/// Convert a direct `(bytes, report)` write result.
pub fn bytes_report(
    py: Python<'_>,
    result: fontpixel::Result<(Vec<u8>, Report)>,
) -> PyResult<(Py<PyBytes>, Py<PyAny>)> {
    let (bytes, report) = result.py()?;
    Ok((
        PyBytes::new(py, &bytes).unbind(),
        conv::to_py(py, &report)?.unbind(),
    ))
}

/// Serialize a `Confidence` (`"no_match"`, `"extension"`, `"possible"`,
/// `"certain"`).
pub fn confidence(py: Python<'_>, c: Confidence) -> PyResult<Py<PyAny>> {
    Ok(conv::to_py(py, &c)?.unbind())
}

fn options<O: DeserializeOwned + Default>(obj: Option<&Bound<'_, PyAny>>) -> PyResult<O> {
    conv::from_py(obj)
}

/// `read(c: &mut ReadContext, o: &Options)` shape.
pub fn read_ctx_opts<M, O>(
    input: &InputArgs,
    options_obj: Option<&Bound<'_, PyAny>>,
    limits: Option<&Bound<'_, PyAny>>,
    f: impl for<'a, 'b> FnOnce(&'a mut ReadContext<'b>, &O) -> fontpixel::Result<(M, Report)> + Send,
) -> PyResult<ModelReport>
where
    M: IntoPyModel,
    O: DeserializeOwned + Default + Sync,
{
    let o: O = options(options_obj)?;
    let limits = limits_arg(limits)?;
    Python::attach(|py| {
        let result = py.detach(|| {
            input.with(|i| {
                let mut c = ReadContext::new(i, &limits)?;
                f(&mut c, &o)
            })
        });
        model_report(py, result)
    })
}

/// `read(c: &mut ReadContext)` shape.
pub fn read_ctx<M: IntoPyModel>(
    input: &InputArgs,
    limits: Option<&Bound<'_, PyAny>>,
    f: impl for<'a, 'b> FnOnce(&'a mut ReadContext<'b>) -> fontpixel::Result<(M, Report)> + Send,
) -> PyResult<ModelReport> {
    let limits = limits_arg(limits)?;
    Python::attach(|py| {
        let result = py.detach(|| {
            input.with(|i| {
                let mut c = ReadContext::new(i, &limits)?;
                f(&mut c)
            })
        });
        model_report(py, result)
    })
}

/// `read(bytes, limits)` shape.
pub fn read_bytes_limits<M: IntoPyModel>(
    data: &Bound<'_, PyAny>,
    limits: Option<&Bound<'_, PyAny>>,
    f: impl FnOnce(&[u8], &ResourceLimits) -> fontpixel::Result<(M, Report)> + Send,
) -> PyResult<ModelReport> {
    let bytes = bytes_arg(data)?;
    let limits = limits_arg(limits)?;
    let py = data.py();
    let result = py.detach(|| f(&bytes, &limits));
    model_report(py, result)
}

/// `read(bytes, options, limits)` shape.
pub fn read_bytes_opts_limits<M, O>(
    data: &Bound<'_, PyAny>,
    options_obj: Option<&Bound<'_, PyAny>>,
    limits: Option<&Bound<'_, PyAny>>,
    f: impl FnOnce(&[u8], &O, &ResourceLimits) -> fontpixel::Result<(M, Report)> + Send,
) -> PyResult<ModelReport>
where
    M: IntoPyModel,
    O: DeserializeOwned + Default + Sync,
{
    let bytes = bytes_arg(data)?;
    let o: O = options(options_obj)?;
    let limits = limits_arg(limits)?;
    let py = data.py();
    let result = py.detach(|| f(&bytes, &o, &limits));
    model_report(py, result)
}

/// `read(bytes, options)` shape.
pub fn read_bytes_opts<M, O>(
    data: &Bound<'_, PyAny>,
    options_obj: Option<&Bound<'_, PyAny>>,
    f: impl FnOnce(&[u8], &O) -> fontpixel::Result<(M, Report)> + Send,
) -> PyResult<ModelReport>
where
    M: IntoPyModel,
    O: DeserializeOwned + Default + Sync,
{
    let bytes = bytes_arg(data)?;
    let o: O = options(options_obj)?;
    let py = data.py();
    let result = py.detach(|| f(&bytes, &o));
    model_report(py, result)
}

/// `write(set, &WriteOptions, &Options) -> Written` shape.
pub fn write_set_wo_opts<O>(
    id: &str,
    fonts: &Bound<'_, PyAny>,
    options_obj: Option<&Bound<'_, PyAny>>,
    args: WriteArgs<'_, '_>,
    f: impl FnOnce(&FontSet, &f::WriteOptions, &O) -> fontpixel::Result<Written> + Send,
) -> PyResult<PyWritten>
where
    O: DeserializeOwned + Default + Sync,
{
    let o: O = options(options_obj)?;
    let wo = write_options(id, args.primary_name, args.strict, args.transport_limits)?;
    let py = fonts.py();
    let written = SetArg::extract(fonts)?.with(|set| Ok(py.detach(|| f(set, &wo, &o))))?;
    PyWritten::new(py, written.py()?)
}

/// `write(model, &Options) -> (Vec<u8>, Report)` shape.
pub fn write_x_opts<X, O>(
    fonts: &Bound<'_, PyAny>,
    options_obj: Option<&Bound<'_, PyAny>>,
    f: impl FnOnce(&X, &O) -> fontpixel::Result<(Vec<u8>, Report)> + Send,
) -> PyResult<(Py<PyBytes>, Py<PyAny>)>
where
    X: ModelArg,
    O: DeserializeOwned + Default + Sync,
{
    let o: O = options(options_obj)?;
    let py = fonts.py();
    let result = X::with(fonts, |x| Ok(py.detach(|| f(x, &o))))?;
    bytes_report(py, result)
}

/// `write(model, &ResourcePath, &Options) -> Written` shape.
pub fn write_x_primary_opts<X, O>(
    id: &str,
    fonts: &Bound<'_, PyAny>,
    options_obj: Option<&Bound<'_, PyAny>>,
    primary_name: Option<&str>,
    f: impl FnOnce(&X, &ResourcePath, &O) -> fontpixel::Result<Written> + Send,
) -> PyResult<PyWritten>
where
    X: ModelArg,
    O: DeserializeOwned + Default + Sync,
{
    let o: O = options(options_obj)?;
    let primary = match primary_name {
        Some(p) => path_arg(p)?,
        None => crate::formats::default_primary(id)?,
    };
    let py = fonts.py();
    let written = X::with(fonts, |x| Ok(py.detach(|| f(x, &primary, &o))))?;
    PyWritten::new(py, written.py()?)
}

/// Run a codec's `FormatDetector` hook on an input.
pub fn detect_codec<C: FormatDetector>(
    codec: &C,
    data: &Bound<'_, PyAny>,
    name: Option<String>,
    limits: Option<&Bound<'_, PyAny>>,
) -> PyResult<Py<PyAny>> {
    let input = InputArgs::new(Some(data), name, None, None)?;
    let limits = limits_arg(limits)?;
    let py = data.py();
    let c = py
        .detach(|| {
            input.with(|i| {
                let mut ctx = ReadContext::new(i, &limits)?;
                codec.detect(&mut ctx)
            })
        })
        .py()?;
    confidence(py, c)
}

/// Read through the built-in registry with `options` applied (modules
/// without direct functions).
pub fn read_registry(
    id: &str,
    input: &InputArgs,
    options_obj: Option<&Bound<'_, PyAny>>,
    limits: Option<&Bound<'_, PyAny>>,
) -> PyResult<ModelReport> {
    let mut reg = PyRegistry {
        registry: fontpixel::registry(),
    };
    if let Some(o) = options_obj.filter(|o| !o.is_none()) {
        crate::formats_options::set_read(&mut reg.registry, id, o)?;
    }
    let limits = limits_arg(limits)?;
    Python::attach(|py| {
        let loaded = reg.read_impl(py, input, Some(id.to_owned()), limits)?;
        Ok((loaded.fonts, loaded.report))
    })
}

/// Write through the built-in registry with `options` applied.
pub fn write_registry(
    id: &str,
    fonts: &Bound<'_, PyAny>,
    options_obj: Option<&Bound<'_, PyAny>>,
    args: WriteArgs<'_, '_>,
) -> PyResult<PyWritten> {
    let mut reg = PyRegistry {
        registry: fontpixel::registry(),
    };
    if let Some(o) = options_obj.filter(|o| !o.is_none()) {
        crate::formats_options::set_write(&mut reg.registry, id, o)?;
    }
    reg.write_impl(fonts, id, args, WriteMode::Plain)
}

// ---------------------------------------------------------------------------
// Namespace generator
// ---------------------------------------------------------------------------

macro_rules! detect_fn {
    (none) => {};
    (bytes $path:path) => {
        /// Structural detector of this format (no filename evidence). Returns
        /// `"no_match"`, `"possible"` or `"certain"`.
        #[pyfunction]
        fn detect(data: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
            let bytes = bytes_arg(data)?;
            confidence(data.py(), $path(&bytes))
        }
    };
    (codec $codec:expr) => {
        /// Run this format's registry detector hook (gzip transport is
        /// decoded first; `name` is passed as the filename hint). Returns
        /// `"no_match"`, `"possible"` or `"certain"`.
        #[pyfunction]
        #[pyo3(signature = (data, *, name = None, limits = None))]
        fn detect(
            data: &Bound<'_, PyAny>,
            name: Option<String>,
            limits: Option<&Bound<'_, PyAny>>,
        ) -> PyResult<Py<PyAny>> {
            detect_codec(&$codec, data, name, limits)
        }
    };
}

macro_rules! read_fn {
    (ctx_opts $path:path, $opts:ty) => {
        /// Decode one input through this module's context reader. `options`
        /// is the module's read-options dict; `resources` supplies sidecars;
        /// `directory` names a directory entry point. Returns
        /// `(model, report)`.
        #[pyfunction]
        #[pyo3(signature = (data = None, options = None, *, name = None, resources = None, limits = None, directory = None))]
        fn read(
            data: Option<&Bound<'_, PyAny>>,
            options: Option<&Bound<'_, PyAny>>,
            name: Option<String>,
            resources: Option<&Bound<'_, PyAny>>,
            limits: Option<&Bound<'_, PyAny>>,
            directory: Option<String>,
        ) -> PyResult<ModelReport> {
            let input = InputArgs::new(data, name, resources, directory)?;
            read_ctx_opts::<_, $opts>(&input, options, limits, |c, o| $path(c, o))
        }
    };
    (ctx $path:path) => {
        /// Decode one input through this module's context reader
        /// (`resources` supplies sidecars). Returns `(model, report)`.
        #[pyfunction]
        #[pyo3(signature = (data = None, *, name = None, resources = None, limits = None, directory = None))]
        fn read(
            data: Option<&Bound<'_, PyAny>>,
            name: Option<String>,
            resources: Option<&Bound<'_, PyAny>>,
            limits: Option<&Bound<'_, PyAny>>,
            directory: Option<String>,
        ) -> PyResult<ModelReport> {
            let input = InputArgs::new(data, name, resources, directory)?;
            read_ctx(&input, limits, |c| $path(c))
        }
    };
    (bytes_limits $path:path) => {
        /// Decode standalone bytes under `limits` (a `ResourceLimits` dict).
        /// Returns `(model, report)`.
        #[pyfunction]
        #[pyo3(signature = (data, *, limits = None))]
        fn read(
            data: &Bound<'_, PyAny>,
            limits: Option<&Bound<'_, PyAny>>,
        ) -> PyResult<ModelReport> {
            read_bytes_limits(data, limits, |b, l| $path(b, l))
        }
    };
    (bytes_opts_limits $path:path, $opts:ty) => {
        /// Decode standalone bytes with this module's read-options dict under
        /// `limits`. Returns `(model, report)`.
        #[pyfunction]
        #[pyo3(signature = (data, options = None, *, limits = None))]
        fn read(
            data: &Bound<'_, PyAny>,
            options: Option<&Bound<'_, PyAny>>,
            limits: Option<&Bound<'_, PyAny>>,
        ) -> PyResult<ModelReport> {
            read_bytes_opts_limits::<_, $opts>(data, options, limits, |b, o, l| $path(b, o, l))
        }
    };
    (bytes_opts $path:path, $opts:ty) => {
        /// Decode standalone bytes with this module's read-options dict
        /// (limits are part of the options). Returns `(model, report)`.
        #[pyfunction]
        #[pyo3(signature = (data, options = None))]
        fn read(data: &Bound<'_, PyAny>, options: Option<&Bound<'_, PyAny>>) -> PyResult<ModelReport> {
            read_bytes_opts::<_, $opts>(data, options, |b, o| $path(b, o))
        }
    };
    (registry $id:literal) => {
        /// Decode through the built-in registry's reader for this format
        /// with `options` (a read-options dict) applied. Returns
        /// `(FontSet, report)`.
        #[pyfunction]
        #[pyo3(signature = (data = None, options = None, *, name = None, resources = None, limits = None, directory = None))]
        fn read(
            data: Option<&Bound<'_, PyAny>>,
            options: Option<&Bound<'_, PyAny>>,
            name: Option<String>,
            resources: Option<&Bound<'_, PyAny>>,
            limits: Option<&Bound<'_, PyAny>>,
            directory: Option<String>,
        ) -> PyResult<ModelReport> {
            let input = InputArgs::new(data, name, resources, directory)?;
            read_registry($id, &input, options, limits)
        }
    };
}

macro_rules! write_fn {
    (set_wo_opts $id:literal, $path:path, $opts:ty) => {
        /// Encode a `FontSet` with this module's write-options dict.
        /// `primary_name` defaults to `font.<first extension>`. Returns a
        /// `Written` with every asset and the full report.
        #[pyfunction]
        #[pyo3(signature = (fonts, options = None, *, primary_name = None, strict = false, transport_limits = None))]
        fn write(
            fonts: &Bound<'_, PyAny>,
            options: Option<&Bound<'_, PyAny>>,
            primary_name: Option<&str>,
            strict: bool,
            transport_limits: Option<&Bound<'_, PyAny>>,
        ) -> PyResult<PyWritten> {
            let args = WriteArgs {
                report: None,
                primary_name,
                strict,
                transport_limits,
            };
            write_set_wo_opts::<$opts>($id, fonts, options, args, |s, w, o| $path(s, w, o))
        }
    };
    (x_opts $x:ty, $path:path, $opts:ty) => {
        /// Encode with this module's write-options dict. Returns
        /// `(bytes, report)`.
        #[pyfunction]
        #[pyo3(signature = (fonts, options = None))]
        fn write(
            fonts: &Bound<'_, PyAny>,
            options: Option<&Bound<'_, PyAny>>,
        ) -> PyResult<(Py<PyBytes>, Py<PyAny>)> {
            write_x_opts::<$x, $opts>(fonts, options, |x, o| $path(x, o))
        }
    };
    (x_primary_opts $id:literal, $x:ty, $path:path, $opts:ty) => {
        /// Encode with this module's write-options dict into a bundle whose
        /// entry point is `primary_name` (default `font.<first extension>`).
        /// Returns a `Written`.
        #[pyfunction]
        #[pyo3(signature = (fonts, options = None, *, primary_name = None))]
        fn write(
            fonts: &Bound<'_, PyAny>,
            options: Option<&Bound<'_, PyAny>>,
            primary_name: Option<&str>,
        ) -> PyResult<PyWritten> {
            write_x_primary_opts::<$x, $opts>($id, fonts, options, primary_name, |x, p, o| $path(x, p, o))
        }
    };
    (registry $id:literal) => {
        /// Encode through the built-in registry's writer for this format with
        /// `options` (a write-options dict) applied. Returns a `Written`.
        #[pyfunction]
        #[pyo3(signature = (fonts, options = None, *, primary_name = None, strict = false, transport_limits = None))]
        fn write(
            fonts: &Bound<'_, PyAny>,
            options: Option<&Bound<'_, PyAny>>,
            primary_name: Option<&str>,
            strict: bool,
            transport_limits: Option<&Bound<'_, PyAny>>,
        ) -> PyResult<PyWritten> {
            let args = WriteArgs {
                report: None,
                primary_name,
                strict,
                transport_limits,
            };
            write_registry($id, fonts, options, args)
        }
    };
}

macro_rules! add_detect {
    ($m:ident, none) => {};
    ($m:ident, $($rest:tt)*) => {
        $m.add_function(wrap_pyfunction!(detect, &$m)?)?;
    };
}

macro_rules! namespaces {
    ($(
        $name:ident [$($id:literal),+] {
            detect: ($($detect:tt)*),
            read: ($($read:tt)*),
            write: ($($write:tt)*) $(,)?
        }
    )*) => {
        $(
            mod $name {
                use super::*;
                detect_fn!($($detect)*);
                read_fn!($($read)*);
                write_fn!($($write)*);
                pub fn build<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyModule>> {
                    let m = PyModule::new(py, stringify!($name))?;
                    m.add("__doc__", concat!(
                        "Direct codec functions of `fontpixel::formats::", stringify!($name), "`."
                    ))?;
                    m.add("FORMAT_IDS", vec![$($id),+])?;
                    add_detect!(m, $($detect)*);
                    m.add_function(wrap_pyfunction!(read, &m)?)?;
                    m.add_function(wrap_pyfunction!(write, &m)?)?;
                    Ok(m)
                }
            }
        )*
        /// Names of the generated namespaces.
        pub const GENERATED: &[&str] = &[$(stringify!($name)),*];
        fn build_all(parent: &Bound<'_, PyModule>) -> PyResult<()> {
            let py = parent.py();
            $(
                let sub = $name::build(py)?;
                parent.add(stringify!($name), &sub)?;
            )*
            Ok(())
        }
    };
}

namespaces! {
    amiga ["amiga"] { detect: (codec f::amiga::AmigaCodec::default()), read: (ctx_opts f::amiga::read, f::amiga::AmigaReadOptions), write: (set_wo_opts "amiga", f::amiga::write, f::amiga::AmigaWriteOptions) }
    bbc ["bbc"] { detect: (bytes f::bbc::detect), read: (ctx_opts f::bbc::read, f::bbc::BbcReadOptions), write: (set_wo_opts "bbc", f::bbc::write, f::bbc::BbcWriteOptions) }
    bdf ["bdf"] { detect: (bytes f::bdf::detect), read: (bytes_limits f::bdf::read), write: (x_opts Font, f::bdf::write, f::bdf::BdfWriteOptions) }
    beos ["beos"] { detect: (codec f::beos::BeosCodec::default()), read: (ctx_opts f::beos::read, f::beos::BeosReadOptions), write: (set_wo_opts "beos", f::beos::write, f::beos::BeosWriteOptions) }
    bmfont ["bmfont"] { detect: (bytes f::bmfont::detect), read: (ctx_opts f::bmfont::read, f::bmfont::BmfontReadOptions), write: (x_primary_opts "bmfont", Font, f::bmfont::write, f::bmfont::BmfontWriteOptions) }
    c ["c"] { detect: (codec f::c::CCodec::default()), read: (registry "c"), write: (registry "c") }
    catalyst ["catalyst"] { detect: (bytes f::catalyst::detect), read: (ctx_opts f::catalyst::read, f::catalyst::CatalystReadOptions), write: (set_wo_opts "catalyst", f::catalyst::write, f::catalyst::CatalystWriteOptions) }
    chiwriter ["chiwriter"] { detect: (bytes f::chiwriter::detect), read: (ctx_opts f::chiwriter::read, f::chiwriter::ChiwriterReadOptions), write: (set_wo_opts "chiwriter", f::chiwriter::write, f::chiwriter::ChiwriterWriteOptions) }
    comfont ["comfont"] { detect: (bytes f::comfont::detect), read: (ctx_opts f::comfont::read, f::comfont::ComfontReadOptions), write: (set_wo_opts "comfont", f::comfont::write, f::comfont::ComfontWriteOptions) }
    consoleet ["consoleet"] { detect: (codec f::consoleet::ConsoleetCodec::default()), read: (ctx_opts f::consoleet::read, f::consoleet::ConsoleetReadOptions), write: (set_wo_opts "consoleet", f::consoleet::write, f::consoleet::ConsoleetWriteOptions) }
    cpi ["cpi"] { detect: (bytes f::cpi::detect), read: (bytes_opts_limits f::cpi::read, f::cpi::CpiReadOptions), write: (x_opts FontSet, f::cpi::write, f::cpi::CpiWriteOptions) }
    cybiko ["cybiko"] { detect: (bytes f::cybiko::detect), read: (ctx_opts f::cybiko::read, f::cybiko::CybikoReadOptions), write: (set_wo_opts "cybiko", f::cybiko::write, f::cybiko::CybikoWriteOptions) }
    daisy ["daisy"] { detect: (bytes f::daisy::detect), read: (ctx_opts f::daisy::read, f::daisy::DaisyReadOptions), write: (set_wo_opts "daisy", f::daisy::write, f::daisy::DaisyWriteOptions) }
    dashen ["dashen"] { detect: (bytes f::dashen::detect), read: (ctx_opts f::dashen::read, f::dashen::DashenReadOptions), write: (set_wo_opts "dashen", f::dashen::write, f::dashen::DashenWriteOptions) }
    dosstart ["dosstart"] { detect: (bytes f::dosstart::detect), read: (ctx_opts f::dosstart::read, f::dosstart::DosstartReadOptions), write: (set_wo_opts "dosstart", f::dosstart::write, f::dosstart::DosstartWriteOptions) }
    draw ["draw"] { detect: (bytes f::draw::detect), read: (bytes_limits f::draw::read), write: (x_opts FontSet, f::draw::write, f::draw::DrawWriteOptions) }
    drcs ["drcs"] { detect: (bytes f::drcs::detect), read: (bytes_opts_limits f::drcs::read, f::drcs::DrcsReadOptions), write: (x_opts FontSet, f::drcs::write, f::drcs::DrcsWriteOptions) }
    drhalo ["drhalo"] { detect: (bytes f::drhalo::detect), read: (ctx_opts f::drhalo::read, f::drhalo::DrhaloReadOptions), write: (set_wo_opts "drhalo", f::drhalo::write, f::drhalo::DrhaloWriteOptions) }
    edwin ["edwin"] { detect: (bytes f::edwin::detect), read: (ctx_opts f::edwin::read, f::edwin::EdwinReadOptions), write: (set_wo_opts "edwin", f::edwin::write, f::edwin::EdwinWriteOptions) }
    fbit ["fbit"] { detect: (bytes f::fbit::detect), read: (bytes_opts_limits f::fbit::read, f::fbit::FbitReadOptions), write: (x_opts FontSet, f::fbit::write, f::fbit::FbitWriteOptions) }
    figlet ["figlet"] { detect: (bytes f::figlet::detect), read: (ctx_opts f::figlet::read, f::figlet::FigletReadOptions), write: (set_wo_opts "figlet", f::figlet::write, f::figlet::FigletWriteOptions) }
    fnt ["fnt"] { detect: (bytes f::fnt::detect), read: (bytes_opts_limits f::fnt::read, f::fnt::FntReadOptions), write: (x_opts Font, f::fnt::write, f::fnt::FntWriteOptions) }
    fon ["fon"] { detect: (bytes f::fon::detect), read: (bytes_opts_limits f::fon::read, f::fnt::FntReadOptions), write: (x_opts FontSet, f::fon::write, f::fnt::FntWriteOptions) }
    fontx2 ["fontx2"] { detect: (bytes f::fontx2::detect), read: (bytes_opts_limits f::fontx2::read, f::fontx2::Fontx2ReadOptions), write: (x_opts Font, f::fontx2::write, f::fontx2::Fontx2WriteOptions) }
    fzx ["fzx"] { detect: (bytes f::fzx::detect), read: (ctx_opts f::fzx::read, f::fzx::FzxReadOptions), write: (set_wo_opts "fzx", f::fzx::write, f::fzx::FzxWriteOptions) }
    gdr ["gdr"] { detect: (codec f::gdr::GdrCodec::default()), read: (ctx_opts f::gdr::read, f::gdr::GdrReadOptions), write: (set_wo_opts "gdr", f::gdr::write, f::gdr::GdrWriteOptions) }
    gem ["gem"] { detect: (codec f::gem::GemCodec::default()), read: (ctx_opts f::gem::read, f::gem::GemReadOptions), write: (set_wo_opts "gem", f::gem::write, f::gem::GemWriteOptions) }
    geos ["geos"] { detect: (codec f::geos::GeosCodec::default()), read: (ctx_opts f::geos::read, f::geos::GeosReadOptions), write: (set_wo_opts "geos", f::geos::write, f::geos::GeosWriteOptions) }
    gf ["gf"] { detect: (bytes f::gf::detect), read: (bytes_limits f::gf::read), write: (x_opts Font, f::gf::write, f::gf::GfWriteOptions) }
    gpi ["gpi"] { detect: (codec f::gpi::GpiCodec::default()), read: (ctx_opts f::gpi::read, f::gpi::GpiReadOptions), write: (set_wo_opts "gpi", f::gpi::write, f::gpi::GpiWriteOptions) }
    grasp ["grasp"] { detect: (bytes f::grasp::detect), read: (ctx_opts f::grasp::read, f::grasp::GraspReadOptions), write: (set_wo_opts "grasp", f::grasp::write, f::grasp::GraspWriteOptions) }
    hanfont ["hanfont"] { detect: (bytes f::hanfont::detect), read: (bytes_opts_limits f::hanfont::read, f::hanfont::HanfontReadOptions), write: (x_opts FontSet, f::hanfont::write, f::hanfont::HanfontWriteOptions) }
    hbf ["hbf"] { detect: (bytes f::hbf::detect), read: (ctx f::hbf::read), write: (x_primary_opts "hbf", Font, f::hbf::write, f::hbf::HbfWriteOptions) }
    hex ["hex"] { detect: (bytes f::hex::detect), read: (bytes_opts_limits f::hex::read, f::hex::HexReadOptions), write: (x_opts Font, f::hex::write, f::hex::HexWriteOptions) }
    hmzk ["hmzk"] { detect: (bytes f::hmzk::detect), read: (ctx_opts f::hmzk::read, f::hmzk::HmzkReadOptions), write: (set_wo_opts "hmzk", f::hmzk::write, f::hmzk::HmzkWriteOptions) }
    hppcl ["hppcl"] { detect: (bytes f::hppcl::detect), read: (ctx_opts f::hppcl::read, f::hppcl::HppclReadOptions), write: (set_wo_opts "hppcl", f::hppcl::write, f::hppcl::HppclWriteOptions) }
    hrcg ["hrcg"] { detect: (bytes f::hrcg::detect), read: (ctx_opts f::hrcg::read, f::hrcg::HrcgReadOptions), write: (set_wo_opts "hrcg", f::hrcg::write, f::hrcg::HrcgWriteOptions) }
    hzk ["hzk"] { detect: (bytes f::hzk::detect), read: (bytes_opts_limits f::hzk::read, f::hzk::HzkReadOptions), write: (x_opts Font, f::hzk::write, f::hzk::HzkWriteOptions) }
    iigs ["iigs"] { detect: (bytes f::iigs::detect), read: (bytes_opts_limits f::iigs::read, f::iigs::IigsReadOptions), write: (x_opts FontSet, f::iigs::write, f::iigs::IigsWriteOptions) }
    image ["image"] { detect: (codec f::image::ImageCodec::default()), read: (registry "image"), write: (registry "image") }
    kbits ["kbits"] { detect: (bytes f::kbits::detect), read: (bytes_limits f::kbits::read), write: (x_opts Font, f::kbits::write, f::kbits::KbitsWriteOptions) }
    kbitx ["kbitx"] { detect: (bytes f::kbitx::detect), read: (bytes_opts_limits f::kbitx::read, f::kbitx::KbitxReadOptions), write: (x_opts Font, f::kbitx::write, f::kbitx::KbitxWriteOptions) }
    lisa ["lisa"] { detect: (bytes f::lisa::detect), read: (bytes_opts_limits f::lisa::read, f::lisa::LisaReadOptions), write: (x_opts FontSet, f::lisa::write, f::lisa::LisaWriteOptions) }
    mgtk ["mgtk"] { detect: (bytes f::mgtk::detect), read: (bytes_opts_limits f::mgtk::read, f::mgtk::MgtkReadOptions), write: (x_opts FontSet, f::mgtk::write, f::mgtk::MgtkWriteOptions) }
    mkwinfont ["mkwinfont"] { detect: (bytes f::mkwinfont::detect), read: (ctx_opts f::mkwinfont::read, f::mkwinfont::MkwinfontReadOptions), write: (set_wo_opts "mkwinfont", f::mkwinfont::write, f::mkwinfont::MkwinfontWriteOptions) }
    nfnt ["nfnt"] { detect: (bytes f::nfnt::detect), read: (bytes_opts_limits f::nfnt::read, f::nfnt::NfntReadOptions), write: (x_opts FontSet, f::nfnt::write, f::nfnt::NfntWriteOptions) }
    oberon ["oberon"] { detect: (codec f::oberon::OberonCodec::default()), read: (ctx_opts f::oberon::read, f::oberon::OberonReadOptions), write: (set_wo_opts "oberon", f::oberon::write, f::oberon::OberonWriteOptions) }
    palm ["palm"] { detect: (codec f::palm::PalmCodec::default()), read: (ctx_opts f::palm::read, f::palm::PalmReadOptions), write: (set_wo_opts "palm", f::palm::write, f::palm::PalmWriteOptions) }
    pbf ["pbf"] { detect: (bytes f::pbf::detect), read: (bytes_opts_limits f::pbf::read, f::pbf::PbfReadOptions), write: (x_opts Font, f::pbf::write, f::pbf::PbfWriteOptions) }
    pcf ["pcf"] { detect: (bytes f::pcf::detect), read: (bytes_limits f::pcf::read), write: (x_opts Font, f::pcf::write, f::pcf::PcfWriteOptions) }
    pcgeos ["pcgeos"] { detect: (codec f::pcgeos::PcgeosCodec::default()), read: (ctx_opts f::pcgeos::read, f::pcgeos::PcgeosReadOptions), write: (set_wo_opts "pcgeos", f::pcgeos::write, f::pcgeos::PcgeosWriteOptions) }
    pcr ["pcr"] { detect: (bytes f::pcr::detect), read: (ctx_opts f::pcr::read, f::pcr::PcrReadOptions), write: (set_wo_opts "pcr", f::pcr::write, f::pcr::PcrWriteOptions) }
    pf2 ["pf2"] { detect: (bytes f::pf2::detect), read: (bytes_opts_limits f::pf2::read, f::pf2::Pf2ReadOptions), write: (x_opts Font, f::pf2::write, f::pf2::Pf2WriteOptions) }
    pfr ["pfr"] { detect: (bytes f::pfr::detect), read: (ctx_opts f::pfr::read, f::pfr::PfrReadOptions), write: (set_wo_opts "pfr", f::pfr::write, f::pfr::PfrWriteOptions) }
    pilfont ["pilfont"] { detect: (bytes f::pilfont::detect), read: (ctx_opts f::pilfont::read, f::pilfont::PilfontReadOptions), write: (set_wo_opts "pilfont", f::pilfont::write, f::pilfont::PilfontWriteOptions) }
    pk ["pk"] { detect: (bytes f::pk::detect), read: (bytes_limits f::pk::read), write: (x_opts Font, f::pk::write, f::pk::PkWriteOptions) }
    plan9 ["plan9"] { detect: (bytes f::plan9::detect), read: (ctx f::plan9::read), write: (x_primary_opts "plan9", FontSet, f::plan9::write, f::plan9::Plan9WriteOptions) }
    playdate ["playdate"] { detect: (bytes f::playdate::detect), read: (ctx_opts f::playdate::read, f::playdate::PlaydateReadOptions), write: (set_wo_opts "playdate", f::playdate::write, f::playdate::PlaydateWriteOptions) }
    plus3dos ["plus3dos"] { detect: (bytes f::plus3dos::detect), read: (ctx_opts f::plus3dos::read, f::plus3dos::Plus3dosReadOptions), write: (set_wo_opts "plus3dos", f::plus3dos::write, f::plus3dos::Plus3dosWriteOptions) }
    polyprint ["polyprint"] { detect: (bytes f::polyprint::detect), read: (ctx_opts f::polyprint::read, f::polyprint::PolyprintReadOptions), write: (set_wo_opts "polyprint", f::polyprint::write, f::polyprint::PolyprintWriteOptions) }
    prebuilt ["prebuilt"] { detect: (bytes f::prebuilt::detect), read: (ctx_opts f::prebuilt::read, f::prebuilt::PrebuiltReadOptions), write: (set_wo_opts "prebuilt", f::prebuilt::write, f::prebuilt::PrebuiltWriteOptions) }
    printshop ["printshop"] { detect: (bytes f::printshop::detect), read: (ctx_opts f::printshop::read, f::printshop::PrintshopReadOptions), write: (set_wo_opts "printshop", f::printshop::write, f::printshop::PrintshopWriteOptions) }
    psf ["psf"] { detect: (bytes f::psf::detect), read: (bytes_limits f::psf::read), write: (x_opts Font, f::psf::write, f::psf::PsfWriteOptions) }
    psf2txt ["psf2txt"] { detect: (bytes f::psf2txt::detect), read: (ctx_opts f::psf2txt::read, f::psf2txt::Psf2txtReadOptions), write: (set_wo_opts "psf2txt", f::psf2txt::write, f::psf2txt::Psf2txtWriteOptions) }
    pxl ["pxl"] { detect: (bytes f::pxl::detect), read: (ctx_opts f::pxl::read, f::pxl::PxlReadOptions), write: (set_wo_opts "pxl", f::pxl::write, f::pxl::PxlWriteOptions) }
    qpf ["qpf"] { detect: (bytes f::qpf::detect), read: (bytes_opts_limits f::qpf::read, f::qpf::QpfReadOptions), write: (x_opts FontSet, f::qpf::write, f::qpf::QpfWriteOptions) }
    qpf2 ["qpf2"] { detect: (bytes f::qpf2::detect), read: (bytes_opts_limits f::qpf2::read, f::qpf2::Qpf2ReadOptions), write: (x_opts FontSet, f::qpf2::write, f::qpf2::Qpf2WriteOptions) }
    raw ["raw"] { detect: (none), read: (bytes_opts_limits f::raw::read, f::raw::RawReadOptions), write: (x_opts FontSet, f::raw::write, f::raw::RawWriteOptions) }
    riscos ["riscos"] { detect: (bytes f::riscos::detect), read: (ctx_opts f::riscos::read, f::riscos::RiscosReadOptions), write: (set_wo_opts "riscos", f::riscos::write, f::riscos::RiscosWriteOptions) }
    rockbox ["rockbox"] { detect: (bytes f::rockbox::detect), read: (ctx_opts f::rockbox::read, f::rockbox::RockboxReadOptions), write: (set_wo_opts "rockbox", f::rockbox::write, f::rockbox::RockboxWriteOptions) }
    sbf ["sbf"] { detect: (bytes f::sbf::detect), read: (ctx_opts f::sbf::read, f::sbf::SbfReadOptions), write: (set_wo_opts "sbf", f::sbf::write, f::sbf::SbfWriteOptions) }
    sfd ["sfd"] { detect: (bytes f::sfd::detect), read: (bytes_limits f::sfd::read), write: (x_opts FontSet, f::sfd::write, f::sfd::SfdWriteOptions) }
    sfont ["sfont"] { detect: (bytes f::sfont::detect), read: (ctx_opts f::sfont::read, f::sfont::SfontReadOptions), write: (set_wo_opts "sfont", f::sfont::write, f::sfont::SfontWriteOptions) }
    signum ["signum"] { detect: (bytes f::signum::detect), read: (ctx_opts f::signum::read, f::signum::SignumReadOptions), write: (set_wo_opts "signum", f::signum::write, f::signum::SignumWriteOptions) }
    snf ["snf"] { detect: (codec f::snf::SnfCodec::default()), read: (ctx_opts f::snf::read, f::snf::SnfReadOptions), write: (set_wo_opts "snf", f::snf::write, f::snf::SnfWriteOptions) }
    ssfn ["ssfn"] { detect: (bytes f::ssfn::detect), read: (ctx_opts f::ssfn::read, f::ssfn::SsfnReadOptions), write: (set_wo_opts "ssfn", f::ssfn::write, f::ssfn::SsfnWriteOptions) }
    svg ["svg"] { detect: (codec f::svg::SvgCodec::default()), read: (bytes_opts f::svg::read, f::svg::ReadOptions), write: (x_opts FontSet, f::svg::write, f::svg::WriteOptions) }
    symbos ["symbos"] { detect: (codec f::symbos::SymbosCodec::default()), read: (ctx_opts f::symbos::read, f::symbos::SymbosReadOptions), write: (set_wo_opts "symbos", f::symbos::write, f::symbos::SymbosWriteOptions) }
    tasprint ["tasprint"] { detect: (bytes f::tasprint::detect), read: (ctx_opts f::tasprint::read, f::tasprint::TasprintReadOptions), write: (set_wo_opts "tasprint", f::tasprint::write, f::tasprint::TasprintWriteOptions) }
    u8m ["u8m"] { detect: (bytes f::u8m::detect), read: (ctx_opts f::u8m::read, f::u8m::U8mReadOptions), write: (set_wo_opts "u8m", f::u8m::write, f::u8m::U8mWriteOptions) }
    vfont ["vfont"] { detect: (bytes f::vfont::detect), read: (ctx_opts f::vfont::read, f::vfont::VfontReadOptions), write: (set_wo_opts "vfont", f::vfont::write, f::vfont::VfontWriteOptions) }
    vlw ["vlw"] { detect: (bytes f::vlw::detect), read: (ctx_opts f::vlw::read, f::vlw::VlwReadOptions), write: (set_wo_opts "vlw", f::vlw::write, f::vlw::VlwWriteOptions) }
    vtfont ["vtfont"] { detect: (codec f::vtfont::VtfontCodec::default()), read: (ctx_opts f::vtfont::read, f::vtfont::VtfontReadOptions), write: (set_wo_opts "vtfont", f::vtfont::write, f::vtfont::VtfontWriteOptions) }
    writeon ["writeon"] { detect: (bytes f::writeon::detect), read: (ctx_opts f::writeon::read, f::writeon::WriteonReadOptions), write: (set_wo_opts "writeon", f::writeon::write, f::writeon::WriteonWriteOptions) }
    wsfont ["wsfont"] { detect: (codec f::wsfont::WsfontCodec::default()), read: (ctx_opts f::wsfont::read, f::wsfont::WsfontReadOptions), write: (set_wo_opts "wsfont", f::wsfont::write, f::wsfont::WsfontWriteOptions) }
    wyse ["wyse"] { detect: (bytes f::wyse::detect), read: (ctx_opts f::wyse::read, f::wyse::WyseReadOptions), write: (set_wo_opts "wyse", f::wyse::write, f::wyse::WyseWriteOptions) }
    xbin ["xbin"] { detect: (bytes f::xbin::detect), read: (ctx_opts f::xbin::read, f::xbin::XbinReadOptions), write: (set_wo_opts "xbin", f::xbin::write, f::xbin::XbinWriteOptions) }
    xerox ["xerox"] { detect: (bytes f::xerox::detect), read: (ctx_opts f::xerox::read, f::xerox::XeroxReadOptions), write: (set_wo_opts "xerox", f::xerox::write, f::xerox::XeroxWriteOptions) }
    yaff ["yaff"] { detect: (bytes f::yaff::detect), read: (bytes_limits f::yaff::read), write: (x_opts FontSet, f::yaff::write, f::yaff::YaffWriteOptions) }
    zapfont ["zapfont"] { detect: (bytes f::zapfont::detect), read: (ctx_opts f::zapfont::read, f::zapfont::ZapfontReadOptions), write: (set_wo_opts "zapfont", f::zapfont::write, f::zapfont::ZapfontWriteOptions) }
}

/// Register every generated namespace on `fontpixel.formats`.
pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    build_all(parent)
}
