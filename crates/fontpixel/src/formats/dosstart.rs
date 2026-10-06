//! Native dosstart bitmap font reader and writer.
use super::w5a1_common;
/// Typed dosstart output choices.
pub use super::w5a1_common::NativeWriteOptions as DosstartWriteOptions;
use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
use crate::resources::{ReadContext, Written};
use crate::{FontSet, Report, Result};
/// Typed dosstart input choices.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct DosstartReadOptions {}
/// Native dosstart registry codec.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct DosstartCodec {
    /// Input profile choices.
    pub read_options: DosstartReadOptions,
    /// Output conversion and resource choices.
    pub write_options: DosstartWriteOptions,
}
impl FormatDetector for DosstartCodec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(detect(c.bytes()))
    }
}
impl FormatReader<FontSet> for DosstartCodec {
    fn read(&self, c: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
        read(c, &self.read_options)
    }
}
impl FormatWriter<FontSet> for DosstartCodec {
    fn write(&self, s: &FontSet, o: &WriteOptions) -> Result<Written> {
        write(s, o, &self.write_options)
    }
}
/// Inspect native content without constructing a font model.
pub fn detect(b: &[u8]) -> Confidence {
    w5a1_common::detect("dosstart", b)
}
/// Decode one native bitmap font under the context's resource limits.
pub fn read(c: &mut ReadContext<'_>, _options: &DosstartReadOptions) -> Result<(FontSet, Report)> {
    w5a1_common::read(c, "dosstart", None)
}
/// Encode one font, auditing every changed or omitted model field.
pub fn write(s: &FontSet, o: &WriteOptions, p: &DosstartWriteOptions) -> Result<Written> {
    w5a1_common::write(s, o, p, "dosstart")
}

#[cfg(all(test, feature = "serde"))]
mod serde_tests {
    use super::*;
    use crate::formats::serde_test::defaults;

    #[test]
    fn options_serde_defaults_and_rejections() {
        defaults::<DosstartReadOptions>();
        defaults::<DosstartCodec>();
    }
}
