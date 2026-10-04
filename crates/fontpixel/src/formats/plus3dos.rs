//! Native plus3dos bitmap font reader and writer.
use super::w5a1_common;
/// Typed plus3dos output choices.
pub use super::w5a1_common::NativeWriteOptions as Plus3dosWriteOptions;
use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
use crate::resources::{ReadContext, Written};
use crate::{FontSet, Report, Result};
/// Typed plus3dos input choices.
#[derive(Debug, Clone, Default)]
pub struct Plus3dosReadOptions {}
/// Native plus3dos registry codec.
#[derive(Debug, Clone, Default)]
pub struct Plus3dosCodec {
    /// Input profile choices.
    pub read_options: Plus3dosReadOptions,
    /// Output conversion and resource choices.
    pub write_options: Plus3dosWriteOptions,
}
impl FormatDetector for Plus3dosCodec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(detect(c.bytes()))
    }
}
impl FormatReader<FontSet> for Plus3dosCodec {
    fn read(&self, c: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
        read(c, &self.read_options)
    }
}
impl FormatWriter<FontSet> for Plus3dosCodec {
    fn write(&self, s: &FontSet, o: &WriteOptions) -> Result<Written> {
        write(s, o, &self.write_options)
    }
}
/// Inspect native content without constructing a font model.
pub fn detect(b: &[u8]) -> Confidence {
    w5a1_common::detect("plus3dos", b)
}
/// Decode one native bitmap font under the context's resource limits.
pub fn read(c: &mut ReadContext<'_>, _options: &Plus3dosReadOptions) -> Result<(FontSet, Report)> {
    w5a1_common::read(c, "plus3dos", None)
}
/// Encode one font, auditing every changed or omitted model field.
pub fn write(s: &FontSet, o: &WriteOptions, p: &Plus3dosWriteOptions) -> Result<Written> {
    w5a1_common::write(s, o, p, "plus3dos")
}
