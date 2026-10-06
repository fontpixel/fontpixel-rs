//! bdfparser SPEC-v3 façade over the native Font/Glyph/Bitmap model.
//!
//! Fonts own a shared native model so attached glyph handles write through.
//! Cloning a Font deep-copies it; cloning a Glyph detaches its metadata snapshot.
//! Fallible methods preserve Python's partial-mutation order. Source-only and
//! dynamically typed metadata lives in native ordered properties, never in a
//! second font store. Native model access is via [`Font::core`].
mod bitmap;
mod font;
mod glyph;
mod projection;
pub use bitmap::Bitmap;
pub use font::{Converter, CreateOptions, Font, fmtver};
pub use glyph::{Glyph, META_TITLES, defaultglyphname};
pub use projection::Meta;

mod io;
pub mod outline;
pub mod svg;
pub mod ttf;
pub use io::SaveOptions;
