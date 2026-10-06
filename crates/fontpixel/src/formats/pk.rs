//! Native, bounded TeX packed-font decoding and encoding.
mod reader;
mod writer;
pub use super::tex_common::TexWriteOptions as PkWriteOptions;
use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
use crate::resources::{OutputBundle, ReadContext, Written};
use crate::{FontSet, Report, Result};
pub use reader::read;
pub use writer::write;
/// Registry codec for PK font packets.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct PkCodec {
    /// Typed TeX output choices.
    pub write_options: PkWriteOptions,
}
impl FormatDetector for PkCodec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(detect(c.bytes()))
    }
}
impl FormatReader<FontSet> for PkCodec {
    fn read(&self, c: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
        let (f, r) = reader::parse(c.bytes(), c.limits(), c.retained_input_bytes())?;
        let mut s = FontSet::from(f);
        s.source_format = Some("pk".into());
        Ok((s, r))
    }
}
impl FormatWriter<FontSet> for PkCodec {
    fn write(&self, s: &FontSet, o: &WriteOptions) -> Result<Written> {
        self.write_charged(s, o, super::ModelCharge::Estimate)
    }
}
impl PkCodec {
    /// As [`FormatWriter::write`] when the caller has reserved the model's
    /// exact retained usage outside this codec's limits.
    pub(crate) fn write_reserved(&self, s: &FontSet, o: &WriteOptions) -> Result<Written> {
        self.write_charged(s, o, super::ModelCharge::Reserved)
    }
    fn write_charged(
        &self,
        s: &FontSet,
        o: &WriteOptions,
        charge: super::ModelCharge,
    ) -> Result<Written> {
        let (b, r) = writer::write_set(s, &self.write_options, charge)?;
        Ok(Written {
            output: OutputBundle::single(o.primary_name.clone(), b),
            report: r,
        })
    }
}
/// Recognize the PK preamble's format identifier.
pub fn detect(b: &[u8]) -> Confidence {
    if b.starts_with(&[247, 89]) {
        Confidence::Certain
    } else {
        Confidence::NoMatch
    }
}
