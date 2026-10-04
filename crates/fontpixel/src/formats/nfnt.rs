//! Macintosh NFNT/FONT bitmap resources and resource-fork containers.
use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
use crate::resources::{ReadContext, ResourcePath, Written};
use crate::{FontSet, Report, ResourceLimits, Result};
mod binhex;
#[cfg(any(feature = "fbit", feature = "lisa"))]
pub(crate) mod bridge;
mod container;
mod palette;
mod probe;
mod raster;
mod support;
mod writer;
pub use writer::{write, write_bundle};

/// Outer representation for Macintosh bitmap font resources.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Container {
    /// One bare NFNT/FONT resource.
    Bare,
    /// A classic resource fork.
    ResourceFork,
    /// A resource fork stored in the data fork.
    #[default]
    Dfont,
    /// A font suitcase resource fork.
    Suitcase,
    /// A MacBinary II envelope (reader also accepts I/III).
    MacBinary,
    /// AppleSingle version 2.
    AppleSingle,
    /// AppleDouble version 2 and a separate data asset.
    AppleDouble,
    /// BinHex 4.0 HQX/RLE transport with checked header and fork CRCs.
    BinHex,
}
/// Bitmap resource type.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ResourceType {
    /// Flexible resource IDs linked by FOND.
    #[default]
    Nfnt,
    /// Classic FONT resource IDs.
    Font,
}
/// Explicit interpretation of Macintosh legacy codes and paired resources.
#[derive(Debug, Clone, Default)]
pub struct NfntReadOptions {
    /// Legacy encoding label; no implicit remapping to Unicode.
    pub charset: Option<String>,
    /// AppleDouble companion to resolve when the primary is its data fork.
    pub apple_double_resource: Option<ResourcePath>,
}
/// Deterministic, bounded Macintosh font output.
#[derive(Debug, Clone)]
pub struct NfntWriteOptions {
    /// Outer representation.
    pub container: Container,
    /// Type used for newly authored bitmap resources.
    pub resource_type: ResourceType,
    /// Optional explicit Gray8 reduction; absence preserves eight-bit samples.
    pub gray_threshold: Option<u8>,
    /// Limits for source, generated resources and readback.
    pub limits: ResourceLimits,
    /// Maximum bytes across all output assets.
    pub max_output_bytes: usize,
}
impl Default for NfntWriteOptions {
    fn default() -> Self {
        Self {
            container: Container::Dfont,
            resource_type: ResourceType::Nfnt,
            gray_threshold: None,
            limits: ResourceLimits::default(),
            max_output_bytes: 64 * 1024 * 1024,
        }
    }
}
/// Registry implementation with strongly typed options.
#[derive(Debug, Clone, Default)]
pub struct NfntCodec {
    /// Input interpretation.
    pub read_options: NfntReadOptions,
    /// Output policy.
    pub write_options: NfntWriteOptions,
}
impl FormatDetector for NfntCodec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(detect(c.bytes()))
    }
}
impl FormatReader<FontSet> for NfntCodec {
    fn read(&self, c: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
        container::read_context(c, &self.read_options)
    }
}
impl FormatWriter<FontSet> for NfntCodec {
    fn write(&self, f: &FontSet, o: &WriteOptions) -> Result<Written> {
        write_bundle(f, &self.write_options, &o.primary_name)
    }
}
/// Recognize validated structural signatures without using file extensions.
pub fn detect(bytes: &[u8]) -> Confidence {
    container::detect(bytes)
}
/// Read a standalone resource or envelope; paired AppleDouble needs a ReadContext.
pub fn read(
    bytes: &[u8],
    options: &NfntReadOptions,
    limits: &ResourceLimits,
) -> Result<(FontSet, Report)> {
    let input = crate::resources::Input::new(bytes);
    let mut context = ReadContext::new(&input, limits)?;
    container::read_context(&mut context, options)
}
