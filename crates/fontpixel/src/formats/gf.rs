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
        let reserved = super::tex_common::container_allowance(s, &self.write_options.limits)?;
        let mut options = self.write_options.clone();
        options.limits.max_total_bytes -= reserved;
        let (b, mut r) = write(s.single()?, &options)?;
        crate::formats::tex_common::container_losses(
            s,
            "gf",
            &mut r,
            &self.write_options.limits,
            b.len(),
        )?;
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
