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

pub mod conv;
pub mod errors;
pub mod handles;
pub mod merge;
pub mod outline;
pub mod sfnt;
pub mod sfnt_support;
pub mod sfnt_tables;
pub mod variants;
pub mod woff;

use pyo3::prelude::*;

/// Native extension module. Area modules add their classes and functions here.
#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    errors::register(m)?;
    handles::register(m)?;
    sfnt::register(m)?;
    woff::register(m)?;
    outline::register(m)?;
    variants::register(m)?;
    merge::register(m)?;
    Ok(())
}
