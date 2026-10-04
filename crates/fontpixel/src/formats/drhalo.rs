//! Native drhalo bitmap font reader and writer.
use super::w5a1_common;
/// Typed drhalo output choices.
pub use super::w5a1_common::NativeWriteOptions as DrhaloWriteOptions;
use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
use crate::resources::{ReadContext, Written};
use crate::{FontSet, Report, Result};
/// Typed drhalo input choices.
#[derive(Debug, Clone, Default)]
pub struct DrhaloReadOptions {
    /// Override the historically inferred first legacy position.
    pub first_codepoint: Option<u32>,
}
/// Native drhalo registry codec.
#[derive(Debug, Clone, Default)]
pub struct DrhaloCodec {
    /// Input profile choices.
    pub read_options: DrhaloReadOptions,
    /// Output conversion and resource choices.
    pub write_options: DrhaloWriteOptions,
}
impl FormatDetector for DrhaloCodec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(detect(c.bytes()))
    }
}
impl FormatReader<FontSet> for DrhaloCodec {
    fn read(&self, c: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
        read(c, &self.read_options)
    }
}
impl FormatWriter<FontSet> for DrhaloCodec {
    fn write(&self, s: &FontSet, o: &WriteOptions) -> Result<Written> {
        write(s, o, &self.write_options)
    }
}
/// Inspect native content without constructing a font model.
pub fn detect(b: &[u8]) -> Confidence {
    w5a1_common::detect("drhalo", b)
}
/// Decode one native bitmap font under the context's resource limits.
pub fn read(c: &mut ReadContext<'_>, options: &DrhaloReadOptions) -> Result<(FontSet, Report)> {
    w5a1_common::read(c, "drhalo", options.first_codepoint)
}
/// Encode one font, auditing every changed or omitted model field.
pub fn write(s: &FontSet, o: &WriteOptions, p: &DrhaloWriteOptions) -> Result<Written> {
    w5a1_common::write(s, o, p, "drhalo")
}
