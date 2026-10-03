//! Bitmap-font core with bounded Mono/Gray8 operations, loss-preserving strikes,
//! and exact charset maps. Feature-gated BDF/PCF support includes bounded gzip input.
//! The catalogue always distinguishes planned formats from real capabilities.
#![forbid(unsafe_code)]

pub mod bitmap;
pub mod charset;
#[cfg(feature = "compat-bdfparser")]
pub mod compat;
pub mod error;
pub mod formats;
pub mod model;
pub mod render;
pub mod report;
pub mod resources;
#[cfg(feature = "sfnt")]
pub mod sfnt;

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

/// Construct the built-in font registry with feature-gated implemented codecs.
pub fn registry() -> FontRegistry {
    #[allow(unused_mut)]
    let mut registry = FontRegistry::new();
    #[cfg(feature = "bdf")]
    registry.install_bdf();
    #[cfg(feature = "pcf")]
    registry.install_pcf();
    #[cfg(feature = "yaff")]
    registry.install_yaff();
    registry
}

/// Decode bounded standalone bytes using the real font model and built-in registry.
/// Content recognition precedes filename hints; errors preserve explicit diagnostics.
pub fn read(bytes: &[u8], options: &ReadOptions) -> Result<formats::Loaded<FontSet>> {
    registry().read(&Input::new(bytes), options)
}

/// Encode a complete font set into validated in-memory assets with diagnostics.
/// Strict mode is enforced by the registry before any output is returned.
/// BDF and PCF use their respective features; planned formats remain explicit errors.
pub fn write(fonts: &FontSet, format: &str, options: &WriteOptions) -> Result<resources::Written> {
    registry().write(fonts, format, options)
}
