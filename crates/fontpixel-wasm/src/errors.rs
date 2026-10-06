//! Error classes mirroring `fontpixel::Error`.
//!
//! The classes live in `js/errors.js` (copied next to the wasm glue), so the
//! glue and the package entry point share one module instance and
//! `instanceof` works. Every error carries `kind` (the Rust variant name),
//! `exitCode` (the CLI convention) and, where the Rust error has them,
//! `format`, `operation`, `candidates`, `line`, `resource`, `limit`, `path`,
//! `count`, `ioKind` and `report` (plain data, every entry).
//!
//! `UnsupportedError` (kind `Unsupported`) is thrown for capabilities this
//! platform or build lacks (see [`crate::capabilities`]) and for
//! `fontpixel::Error::Io` errors of kind `Unsupported` (for example a batch
//! that would need worker threads).
use js_sys::{Array, Object, Reflect};
use wasm_bindgen::prelude::*;

#[wasm_bindgen(raw_module = "./errors.js")]
extern "C" {
    #[wasm_bindgen(js_name = makeError)]
    fn make_error(kind: &str, message: &str, fields: &JsValue) -> JsValue;
}

/// Result of a binding function: the error side is the thrown JS value.
pub type JsResult<T> = Result<T, JsValue>;

/// Structured attributes of an error under construction.
pub struct Fields(Object);

impl Fields {
    /// No attributes yet.
    pub fn new() -> Self {
        Self(Object::new())
    }
    /// Set one attribute.
    pub fn set(self, key: &str, value: impl Into<JsValue>) -> Self {
        // Setting a data property on a fresh plain object cannot fail.
        let _ = Reflect::set(&self.0, &JsValue::from_str(key), &value.into());
        self
    }
    /// Build the error of class `kind` with these attributes.
    pub fn error(self, kind: &str, message: &str) -> JsValue {
        let fields = self.set("kind", kind);
        make_error(kind, message, &fields.0)
    }
}

impl Default for Fields {
    fn default() -> Self {
        Self::new()
    }
}

/// `InvalidInputError` with `message` (exit code 1).
pub fn invalid(message: impl AsRef<str>) -> JsValue {
    Fields::new()
        .set("exitCode", 1)
        .error("InvalidInput", message.as_ref())
}

/// `InvalidInputError` for a request that cannot be built (exit code 2,
/// as the command line's usage errors).
pub fn request_error(message: impl AsRef<str>) -> JsValue {
    Fields::new()
        .set("exitCode", 2)
        .error("InvalidInput", message.as_ref())
}

/// Convert a `fontpixel::Error` into the matching JS error.
pub fn to_js(err: fontpixel::Error) -> JsValue {
    let code = err.exit_code();
    with_exit_code(err, code)
}

/// Convert a `fontpixel::Error`, overriding the exit code.
pub fn with_exit_code(err: fontpixel::Error, exit_code: u8) -> JsValue {
    use fontpixel::Error as E;
    let message = err.to_string();
    let mut fields = Fields::new().set("exitCode", exit_code);
    if let Some(report) = err.report() {
        match crate::conv::to_js(report) {
            Ok(value) => fields = fields.set("report", value),
            Err(error) => return error,
        }
    }
    let kind = match &err {
        E::UnknownFormat(id) => {
            fields = fields.set("format", id.as_str());
            "UnknownFormat"
        }
        E::FeatureDisabled { format } => {
            fields = fields.set("format", format.as_str());
            "FeatureDisabled"
        }
        E::NotImplemented { format, operation } => {
            fields = fields
                .set("format", format.as_str())
                .set("operation", format!("{operation:?}"));
            "NotImplemented"
        }
        E::AmbiguousFormat { candidates } => {
            let list: Array = candidates.iter().map(|c| JsValue::from_str(c)).collect();
            fields = fields.set("candidates", list);
            "AmbiguousFormat"
        }
        E::UnrecognizedFormat => "UnrecognizedFormat",
        E::InvalidInput(_) => "InvalidInput",
        E::BdfParse { line, .. } => {
            fields = fields.set("line", *line as f64);
            "BdfParse"
        }
        E::ResourceLimit { resource, limit } => {
            fields = fields
                .set("resource", *resource)
                .set("limit", *limit as f64);
            "ResourceLimit"
        }
        E::MissingResource(path) => {
            fields = fields.set("path", path.as_str());
            "MissingResource"
        }
        E::MultipleAssets { count } => {
            fields = fields.set("count", *count as f64);
            "MultipleAssets"
        }
        E::StrictLoss(_) => "StrictLoss",
        E::ReportedErrors(_) => "ReportedErrors",
        E::Cancelled => "Cancelled",
        E::Io(io) => {
            fields = fields.set("ioKind", format!("{:?}", io.kind()));
            if io.kind() == std::io::ErrorKind::Unsupported {
                "Unsupported"
            } else {
                "Io"
            }
        }
        _ => "Other",
    };
    fields.error(kind, &message)
}

/// Map the error side of a `fontpixel::Result` to the JS error classes.
pub trait IntoJs<T> {
    /// `fontpixel::Error` → JS error.
    fn js(self) -> JsResult<T>;
}

impl<T> IntoJs<T> for fontpixel::Result<T> {
    fn js(self) -> JsResult<T> {
        self.map_err(to_js)
    }
}
