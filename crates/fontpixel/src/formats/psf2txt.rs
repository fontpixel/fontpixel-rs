//! Native psf2txt bitmap font reader and writer.
use super::w5a1_common;
/// Typed psf2txt output choices.
pub use super::w5a1_common::NativeWriteOptions as Psf2txtWriteOptions;
use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
use crate::resources::{ReadContext, Written};
use crate::{FontSet, Report, Result};
/// Typed psf2txt input choices.
#[derive(Debug, Clone, Default)]
pub struct Psf2txtReadOptions {}
/// Native psf2txt registry codec.
#[derive(Debug, Clone, Default)]
pub struct Psf2txtCodec {
    /// Input profile choices.
    pub read_options: Psf2txtReadOptions,
    /// Output conversion and resource choices.
    pub write_options: Psf2txtWriteOptions,
}
impl FormatDetector for Psf2txtCodec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(detect(c.bytes()))
    }
}
impl FormatReader<FontSet> for Psf2txtCodec {
    fn read(&self, c: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
        read(c, &self.read_options)
    }
}
impl FormatWriter<FontSet> for Psf2txtCodec {
    fn write(&self, s: &FontSet, o: &WriteOptions) -> Result<Written> {
        write(s, o, &self.write_options)
    }
}
/// Inspect native content without constructing a font model.
pub fn detect(b: &[u8]) -> Confidence {
    w5a1_common::detect("psf2txt", b)
}
/// Decode one native bitmap font under the context's resource limits.
pub fn read(c: &mut ReadContext<'_>, _options: &Psf2txtReadOptions) -> Result<(FontSet, Report)> {
    w5a1_common::read(c, "psf2txt", None)
}
/// Encode one font, auditing every changed or omitted model field.
pub fn write(s: &FontSet, o: &WriteOptions, p: &Psf2txtWriteOptions) -> Result<Written> {
    w5a1_common::write(s, o, p, "psf2txt")
}
