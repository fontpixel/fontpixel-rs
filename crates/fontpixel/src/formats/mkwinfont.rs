//! Native mkwinfont bitmap font reader and writer.
use super::w5a1_common;
/// Typed mkwinfont output choices.
pub use super::w5a1_common::NativeWriteOptions as MkwinfontWriteOptions;
use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
use crate::resources::{ReadContext, Written};
use crate::{FontSet, Report, Result};
/// Typed mkwinfont input choices.
#[derive(Debug, Clone, Default)]
pub struct MkwinfontReadOptions {}
/// Native mkwinfont registry codec.
#[derive(Debug, Clone, Default)]
pub struct MkwinfontCodec {
    /// Input profile choices.
    pub read_options: MkwinfontReadOptions,
    /// Output conversion and resource choices.
    pub write_options: MkwinfontWriteOptions,
}
impl FormatDetector for MkwinfontCodec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(detect(c.bytes()))
    }
}
impl FormatReader<FontSet> for MkwinfontCodec {
    fn read(&self, c: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
        read(c, &self.read_options)
    }
}
impl FormatWriter<FontSet> for MkwinfontCodec {
    fn write(&self, s: &FontSet, o: &WriteOptions) -> Result<Written> {
        write(s, o, &self.write_options)
    }
}
/// Inspect native content without constructing a font model.
pub fn detect(b: &[u8]) -> Confidence {
    w5a1_common::detect("mkwinfont", b)
}
/// Decode one native bitmap font under the context's resource limits.
pub fn read(c: &mut ReadContext<'_>, _options: &MkwinfontReadOptions) -> Result<(FontSet, Report)> {
    w5a1_common::read(c, "mkwinfont", None)
}
/// Encode one font, auditing every changed or omitted model field.
pub fn write(s: &FontSet, o: &WriteOptions, p: &MkwinfontWriteOptions) -> Result<Written> {
    w5a1_common::write(s, o, p, "mkwinfont")
}
