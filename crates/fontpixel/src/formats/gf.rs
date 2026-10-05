//! Native, bounded TeX generic-font command streams.
mod reader;
mod writer;
use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
pub use crate::formats::tex_common::TexWriteOptions as GfWriteOptions;
use crate::resources::{OutputBundle, ReadContext, Written};
use crate::{FontSet, Report, Result};
pub use reader::read;
pub use writer::write;
/// Registry codec for GF fonts.
#[derive(Debug, Clone, Default)]
pub struct GfCodec {
    /// Typed TeX output choices.
    pub write_options: GfWriteOptions,
}
impl FormatDetector for GfCodec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(detect(c.bytes()))
    }
}
impl FormatReader<FontSet> for GfCodec {
    fn read(&self, c: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
        let (f, r) = reader::parse(c.bytes(), c.limits(), c.retained_input_bytes())?;
        let mut s = FontSet::from(f);
        s.source_format = Some("gf".into());
        Ok((s, r))
    }
}
impl FormatWriter<FontSet> for GfCodec {
    fn write(&self, s: &FontSet, o: &WriteOptions) -> Result<Written> {
        self.write_charged(s, o, super::ModelCharge::Estimate)
    }
}
impl GfCodec {
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
/// Recognize the GF preamble's format identifier.
pub fn detect(b: &[u8]) -> Confidence {
    if b.starts_with(&[247, 131]) {
        Confidence::Certain
    } else {
        Confidence::NoMatch
    }
}
