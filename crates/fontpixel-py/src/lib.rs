//! Python bindings for fontpixel (`fontpixel._native`).
//!
//! The Python package `fontpixel` re-exports this module. It exposes the complete
//! native fontpixel API; `fontpixel.compat.bdfparser` adds a bdfparser-compatible
//! layer. Conventions shared by every area module:
//!
//! - Errors map to the exception hierarchy in [`errors`]; structured fields such
//!   as the resource name, limit and full report travel as exception attributes.
//! - Typed Rust options and plain data cross the boundary through serde
//!   ([`conv`]): Python dicts in, dicts out, with unknown keys rejected.
//! - Fonts, glyphs and bitmaps stay in Rust behind handle classes; pixel data
//!   is copied out only when Python asks for it.
//! - Each area lives in its own module file with a `register` function, called
//!   from [`_native`] in a fixed order.
#![deny(unsafe_code)]

pub mod bitmap;
pub mod charset;
pub mod conv;
pub mod errors;
pub mod formats;
pub mod formats_custom;
pub mod formats_direct;
pub mod formats_extra;
pub mod formats_options;
pub mod handles;
pub mod metadata;
pub mod model;
pub mod render;
pub mod resources;

use pyo3::prelude::*;

/// Native extension module. Area modules add their classes and functions here.
#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    errors::register(m)?;
    handles::register(m)?;
    model::register(m)?;
    bitmap::register(m)?;
    render::register(m)?;
    charset::register(m)?;
    metadata::register(m)?;
    resources::register(m)?;
    formats::register(m)?;
    Ok(())
}
