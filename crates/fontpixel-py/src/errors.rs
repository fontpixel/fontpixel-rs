//! Exception hierarchy mirroring `fontpixel::Error`.
//!
//! `FontpixelError` is the base class. Every exception carries `kind` (the Rust
//! variant name) and, where the Rust error has them, structured attributes:
//! `resource`/`limit` for resource limits, `format`/`operation`, `candidates`,
//! `line`, and `report` (a dict) for strict-loss and reported-error failures.
use pyo3::create_exception;
use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use pyo3::types::PyDict;

create_exception!(
    fontpixel,
    FontpixelError,
    PyException,
    "Base class of all fontpixel errors."
);
create_exception!(
    fontpixel,
    UnknownFormatError,
    FontpixelError,
    "Unknown format identifier."
);
create_exception!(
    fontpixel,
    FeatureDisabledError,
    FontpixelError,
    "Format compiled out of this build."
);
create_exception!(
    fontpixel,
    NotImplementedFormatError,
    FontpixelError,
    "Capability not implemented for this format."
);
create_exception!(
    fontpixel,
    AmbiguousFormatError,
    FontpixelError,
    "Several formats match; pass an explicit format."
);
create_exception!(
    fontpixel,
    UnrecognizedFormatError,
    FontpixelError,
    "No detector recognized the input."
);
create_exception!(
    fontpixel,
    InvalidInputError,
    FontpixelError,
    "Invalid options, registration, paths or data."
);
create_exception!(
    fontpixel,
    BdfParseError,
    FontpixelError,
    "Fatal BDF syntax error."
);
create_exception!(
    fontpixel,
    ResourceLimitError,
    FontpixelError,
    "A configured resource limit was exceeded."
);
create_exception!(
    fontpixel,
    MissingResourceError,
    FontpixelError,
    "A referenced sidecar was not supplied."
);
create_exception!(
    fontpixel,
    MultipleAssetsError,
    FontpixelError,
    "Several output assets where one was expected."
);
create_exception!(
    fontpixel,
    StrictLossError,
    FontpixelError,
    "Strict mode rejected an information loss."
);
create_exception!(
    fontpixel,
    ReportedErrorsError,
    FontpixelError,
    "A codec reported error diagnostics."
);
create_exception!(
    fontpixel,
    FontpixelIoError,
    FontpixelError,
    "Operating-system or caller resource I/O error."
);
create_exception!(
    fontpixel,
    UnsupportedError,
    FontpixelError,
    "Capability unavailable on this platform or build; see `reason`."
);
create_exception!(
    fontpixel,
    CancelledError,
    FontpixelError,
    "A progress callback cancelled the operation (returned False); nothing was written."
);

/// Register the exception classes on the native module.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = m.py();
    for (name, ty) in [
        ("FontpixelError", py.get_type::<FontpixelError>()),
        ("UnknownFormatError", py.get_type::<UnknownFormatError>()),
        (
            "FeatureDisabledError",
            py.get_type::<FeatureDisabledError>(),
        ),
        (
            "NotImplementedFormatError",
            py.get_type::<NotImplementedFormatError>(),
        ),
        (
            "AmbiguousFormatError",
            py.get_type::<AmbiguousFormatError>(),
        ),
        (
            "UnrecognizedFormatError",
            py.get_type::<UnrecognizedFormatError>(),
        ),
        ("InvalidInputError", py.get_type::<InvalidInputError>()),
        ("BdfParseError", py.get_type::<BdfParseError>()),
        ("ResourceLimitError", py.get_type::<ResourceLimitError>()),
        (
            "MissingResourceError",
            py.get_type::<MissingResourceError>(),
        ),
        ("MultipleAssetsError", py.get_type::<MultipleAssetsError>()),
        ("StrictLossError", py.get_type::<StrictLossError>()),
        ("ReportedErrorsError", py.get_type::<ReportedErrorsError>()),
        ("FontpixelIoError", py.get_type::<FontpixelIoError>()),
        ("UnsupportedError", py.get_type::<UnsupportedError>()),
        ("CancelledError", py.get_type::<CancelledError>()),
    ] {
        m.add(name, ty)?;
    }
    Ok(())
}

/// Convert a `fontpixel::Error` into the matching Python exception.
pub fn to_py(err: fontpixel::Error) -> PyErr {
    use fontpixel::Error as E;
    Python::attach(|py| {
        let message = err.to_string();
        let attrs = PyDict::new(py);
        let kind;
        let exc = match &err {
            E::UnknownFormat(id) => {
                kind = "UnknownFormat";
                let _ = attrs.set_item("format", id);
                UnknownFormatError::new_err(message)
            }
            E::FeatureDisabled { format } => {
                kind = "FeatureDisabled";
                let _ = attrs.set_item("format", format);
                FeatureDisabledError::new_err(message)
            }
            E::NotImplemented { format, operation } => {
                kind = "NotImplemented";
                let _ = attrs.set_item("format", format);
                let _ = attrs.set_item("operation", format!("{operation:?}"));
                NotImplementedFormatError::new_err(message)
            }
            E::AmbiguousFormat { candidates } => {
                kind = "AmbiguousFormat";
                let _ = attrs.set_item("candidates", candidates.clone());
                AmbiguousFormatError::new_err(message)
            }
            E::UnrecognizedFormat => {
                kind = "UnrecognizedFormat";
                UnrecognizedFormatError::new_err(message)
            }
            E::InvalidInput(_) => {
                kind = "InvalidInput";
                InvalidInputError::new_err(message)
            }
            E::BdfParse { line, .. } => {
                kind = "BdfParse";
                let _ = attrs.set_item("line", *line);
                BdfParseError::new_err(message)
            }
            E::ResourceLimit { resource, limit } => {
                kind = "ResourceLimit";
                let _ = attrs.set_item("resource", *resource);
                let _ = attrs.set_item("limit", *limit);
                ResourceLimitError::new_err(message)
            }
            E::MissingResource(path) => {
                kind = "MissingResource";
                let _ = attrs.set_item("path", path);
                MissingResourceError::new_err(message)
            }
            E::MultipleAssets { count } => {
                kind = "MultipleAssets";
                let _ = attrs.set_item("count", *count);
                MultipleAssetsError::new_err(message)
            }
            E::StrictLoss(_) => {
                kind = "StrictLoss";
                StrictLossError::new_err(message)
            }
            E::ReportedErrors(_) => {
                kind = "ReportedErrors";
                ReportedErrorsError::new_err(message)
            }
            E::Io(_) => {
                kind = "Io";
                FontpixelIoError::new_err(message)
            }
            E::Cancelled => {
                kind = "Cancelled";
                CancelledError::new_err(message)
            }
            _ => {
                kind = "Other";
                FontpixelError::new_err(message)
            }
        };
        let _ = attrs.set_item("kind", kind);
        let _ = attrs.set_item("exit_code", err.exit_code());
        if let Some(report) = err.report()
            && let Ok(value) = crate::conv::to_py(py, report)
        {
            let _ = attrs.set_item("report", value);
        }
        let value = exc.value(py);
        for (key, item) in attrs.iter() {
            let _ = value.setattr(key.extract::<String>().unwrap_or_default(), item);
        }
        exc
    })
}

/// `Result` alias for binding functions: `fontpixel::Result` mapped to `PyResult`.
pub trait IntoPy<T> {
    /// Map the error side to the Python exception hierarchy.
    fn py(self) -> PyResult<T>;
}
impl<T> IntoPy<T> for fontpixel::Result<T> {
    fn py(self) -> PyResult<T> {
        self.map_err(to_py)
    }
}
