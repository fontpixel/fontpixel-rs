//! Bitmap-font core with bounded Mono/Gray8 operations, loss-preserving strikes,
//! and exact charset maps. Built-in font codecs are not implemented yet.
//! The catalogue always distinguishes planned formats from real capabilities.
#![forbid(unsafe_code)]

pub mod bitmap;
pub mod charset;
pub mod error;
pub mod formats;
pub mod model;
pub mod report;
pub mod resources;

pub use error::{Error, Operation, Result};
pub use formats::{ReadOptions, Registry, WriteOptions};
pub use report::{Report, ReportEntry};
pub use resources::{
    Input, OutputAsset, OutputBundle, ResourceLimits, ResourcePath, ResourceResolver,
};

/// Core bitmap with packed monochrome storage by default.
pub use bitmap::Bitmap;
pub use model::{Font, FontSet, Glyph};

/// Registry specialized to the real font model.
pub type FontRegistry = Registry<FontSet>;

/// Construct the built-in font registry. No codec hooks are registered until
/// their corresponding implementation work item is complete.
pub fn registry() -> FontRegistry {
    FontRegistry::new()
}

/// Decode bounded standalone bytes using the real font model and built-in registry.
/// No implemented codec is currently available; custom codecs can use FontRegistry.
pub fn read(bytes: &[u8], options: &ReadOptions) -> Result<formats::Loaded<FontSet>> {
    registry().read(&Input::new(bytes), options)
}

/// Encode a complete font set into validated in-memory assets with diagnostics.
/// Strict mode is enforced by the registry before any output is returned.
/// Built-in codec hooks are not implemented at this stage.
pub fn write(fonts: &FontSet, format: &str, options: &WriteOptions) -> Result<resources::Written> {
    registry().write(fonts, format, options)
}
