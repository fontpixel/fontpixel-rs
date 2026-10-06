//! Native pcr bitmap font reader and writer.
use super::w5a1_common;
/// Typed pcr output choices.
pub use super::w5a1_common::NativeWriteOptions as PcrWriteOptions;
use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
use crate::resources::{ReadContext, Written};
use crate::{FontSet, Report, Result};
/// Typed pcr input choices.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct PcrReadOptions {}
/// Native pcr registry codec.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct PcrCodec {
    /// Input profile choices.
    pub read_options: PcrReadOptions,
    /// Output conversion and resource choices.
    pub write_options: PcrWriteOptions,
}
impl FormatDetector for PcrCodec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(detect(c.bytes()))
    }
}
impl FormatReader<FontSet> for PcrCodec {
    fn read(&self, c: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
        read(c, &self.read_options)
    }
}
impl FormatWriter<FontSet> for PcrCodec {
    fn write(&self, s: &FontSet, o: &WriteOptions) -> Result<Written> {
        write(s, o, &self.write_options)
    }
}
/// Inspect native content without constructing a font model.
pub fn detect(b: &[u8]) -> Confidence {
    w5a1_common::detect("pcr", b)
}
/// Decode one native bitmap font under the context's resource limits.
pub fn read(c: &mut ReadContext<'_>, _options: &PcrReadOptions) -> Result<(FontSet, Report)> {
    w5a1_common::read(c, "pcr", None)
}
/// Encode one font, auditing every changed or omitted model field.
pub fn write(s: &FontSet, o: &WriteOptions, p: &PcrWriteOptions) -> Result<Written> {
    w5a1_common::write(s, o, p, "pcr")
}
