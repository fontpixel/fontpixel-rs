//! Plan 9 font manifests, subfonts and bounded image containers.
mod image;
mod reader;
mod writer;
use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
use crate::resources::{ReadContext, Written};
use crate::{FontSet, Report, ResourceLimits, Result};
pub use reader::{detect, read};
pub use writer::write;
/// Select the external Plan 9 representation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum Container {
    /// A Unicode font manifest and relative subfont assets.
    #[default]
    Font,
    /// A single subfont with legacy slot numbers.
    Subfont,
}
/// Plan 9 image storage within generated subfonts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum Compression {
    /// Uncompressed packed image rows.
    #[default]
    Raw,
    /// Standard LZ77 blocks using deterministic literal codewords.
    Literal,
}
/// Typed Plan 9 output choices.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct Plan9WriteOptions {
    /// Manifest bundle or direct subfont.
    pub container: Container,
    /// Image compression policy.
    pub compression: Compression,
    /// Maximum number of glyphs placed in each generated subfont.
    pub max_glyphs_per_subfont: usize,
    /// Optional grayscale threshold; None preserves grayscale coverage.
    pub gray_threshold: Option<u8>,
    /// Input, output and model resource bounds.
    pub limits: ResourceLimits,
    /// Maximum combined output size.
    pub max_output_bytes: usize,
}
impl Default for Plan9WriteOptions {
    fn default() -> Self {
        Self {
            container: Container::Font,
            compression: Compression::Raw,
            max_glyphs_per_subfont: 256,
            gray_threshold: None,
            limits: ResourceLimits::default(),
            max_output_bytes: 256 * 1024 * 1024,
        }
    }
}
/// Registry codec for both Plan 9 representations.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct Plan9Codec {
    /// Typed output choices.
    pub write_options: Plan9WriteOptions,
}
impl FormatDetector for Plan9Codec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(detect(c.bytes()))
    }
}
impl FormatReader<FontSet> for Plan9Codec {
    fn read(&self, c: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
        let (f, r) = read(c)?;
        let mut s = FontSet::from(f);
        s.source_format = Some("plan9".into());
        Ok((s, r))
    }
}
impl FormatWriter<FontSet> for Plan9Codec {
    fn write(&self, s: &FontSet, o: &WriteOptions) -> Result<Written> {
        write(s, &o.primary_name, &self.write_options)
    }
}
