//! Apple IIgs QuickDraw II data-fork bitmap font records, versions 1.1 and 1.5.
use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
use crate::resources::{OutputBundle, ReadContext, Written};
use crate::{FontSet, Report, ResourceLimits, Result};
mod binscii;
mod reader;
pub(super) mod support;
mod writer;
pub use reader::{detect, read};
pub use writer::write;
/// Explicit interpretation of otherwise unlabelled byte character codes.
#[derive(Debug, Clone, Default)]
pub struct IigsReadOptions {
    /// Legacy charset name. Codes are never implicitly converted to Unicode.
    pub charset: Option<String>,
}
/// QuickDraw II font record version.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Version {
    /// Preserve native version, otherwise use 1.1 unless a 32-bit offset is needed.
    #[default]
    Auto,
    /// Version 1.1 with 16-bit offset/width word offset.
    V1_1,
    /// Version 1.5 with the additional offset/width high word.
    V1_5,
}
/// Outer representation of one QuickDraw II font data fork.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Container {
    /// Native ProDOS font data fork.
    #[default]
    Bare,
    /// CRC-checked segmented BinSCII text transport.
    BinScii,
}
/// Deterministic bounded QuickDraw II output options.
#[derive(Debug, Clone)]
pub struct IigsWriteOptions {
    /// Native data fork or segmented text transport.
    pub container: Container,
    /// Header version policy.
    pub version: Version,
    /// Explicit Gray8-to-Mono reduction; absence rejects Gray8 output.
    pub gray_threshold: Option<u8>,
    /// Source, native, output and default-readback allocation limits.
    pub limits: ResourceLimits,
    /// Maximum output size.
    pub max_output_bytes: usize,
}
impl Default for IigsWriteOptions {
    fn default() -> Self {
        Self {
            container: Container::Bare,
            version: Version::Auto,
            gray_threshold: None,
            limits: ResourceLimits::default(),
            max_output_bytes: 64 * 1024 * 1024,
        }
    }
}
/// Registry codec with independent read and write choices.
#[derive(Debug, Clone, Default)]
pub struct IigsCodec {
    /// Legacy code interpretation.
    pub read_options: IigsReadOptions,
    /// Native output policy.
    pub write_options: IigsWriteOptions,
}
impl FormatDetector for IigsCodec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(detect(c.bytes()))
    }
}
impl FormatReader<FontSet> for IigsCodec {
    fn read(&self, c: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
        read(c.bytes(), &self.read_options, c.limits())
    }
}
impl FormatWriter<FontSet> for IigsCodec {
    fn write(&self, s: &FontSet, o: &WriteOptions) -> Result<Written> {
        let (bytes, report) = write(s, &self.write_options)?;
        Ok(Written {
            output: OutputBundle::single(o.primary_name.clone(), bytes),
            report,
        })
    }
}
