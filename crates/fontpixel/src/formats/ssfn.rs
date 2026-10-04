//! Native SFN2/SFNC raster fragments and the author's ASCII source representation.
use super::w5e2_outline_common as c;
use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
use crate::resources::{ReadContext, Written};
use crate::{FontSet, Report, ResourceLimits, Result};
mod ascii;
mod binary;
/// Native SSFN representation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SsfnEncoding {
    /// Binary SFN2, or SFNC for several fonts.
    #[default]
    Binary,
    /// Author-defined ASCII source.
    Ascii,
}
/// Typed input choices.
#[derive(Debug, Clone, Default)]
pub struct SsfnReadOptions {}
/// Native output and explicit conversion choices.
#[derive(Debug, Clone)]
pub struct SsfnWriteOptions {
    /// Binary or ASCII output.
    pub encoding: SsfnEncoding,
    /// Optional explicit monochrome threshold; otherwise retain grayscale.
    pub gray_threshold: Option<u8>,
    /// Permit legacy numeric positions to become Unicode scalars.
    pub legacy_as_unicode: bool,
    /// Cumulative resource limits, including writer readback.
    pub limits: ResourceLimits,
    /// Additional maximum output size.
    pub max_output_bytes: usize,
}
impl Default for SsfnWriteOptions {
    fn default() -> Self {
        Self {
            encoding: SsfnEncoding::Binary,
            gray_threshold: None,
            legacy_as_unicode: false,
            limits: ResourceLimits::default(),
            max_output_bytes: 64 * 1024 * 1024,
        }
    }
}
/// Registry codec with typed read/write choices.
#[derive(Debug, Clone, Default)]
pub struct SsfnCodec {
    /// Reader options.
    pub read_options: SsfnReadOptions,
    /// Writer options.
    pub write_options: SsfnWriteOptions,
}
impl FormatDetector for SsfnCodec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(detect(c.bytes()))
    }
}
impl FormatReader<FontSet> for SsfnCodec {
    fn read(&self, c: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
        read(c, &self.read_options)
    }
}
impl FormatWriter<FontSet> for SsfnCodec {
    fn write(&self, s: &FontSet, o: &WriteOptions) -> Result<Written> {
        write(s, o, &self.write_options)
    }
}
/// Inspect native section boundaries without allocating glyphs.
pub fn detect(b: &[u8]) -> Confidence {
    if binary::probe(b).is_ok() {
        Confidence::Certain
    } else if b.starts_with(b"# Scalable Screen Font #") && b.trim_ascii_end().ends_with(b"# End #")
    {
        Confidence::Possible
    } else {
        Confidence::NoMatch
    }
}
fn decode(b: &[u8], l: &ResourceLimits, budget: &mut c::Budget, r: &mut Report) -> Result<FontSet> {
    if b.starts_with(b"# Scalable Screen Font #") {
        ascii::decode(b, l, budget, r)
    } else {
        binary::decode(b, l, budget, r)
    }
}
/// Decode native raster content without substituting rendered outlines.
pub fn read(c: &mut ReadContext<'_>, _: &SsfnReadOptions) -> Result<(FontSet, Report)> {
    c::input(c, decode)
}
/// Encode native fragments and audit every changed model field.
pub fn write(s: &FontSet, o: &WriteOptions, p: &SsfnWriteOptions) -> Result<Written> {
    c::output(
        s,
        o,
        &p.limits,
        p.max_output_bytes,
        "ssfn",
        |b, r| {
            let converted = if p.gray_threshold.is_some() {
                c::convert_gray(s, p.gray_threshold, &p.limits, b)?
            } else {
                std::borrow::Cow::Borrowed(s)
            };
            match p.encoding {
                SsfnEncoding::Binary => binary::encode(&converted, p, b, r),
                SsfnEncoding::Ascii => ascii::encode(&converted, p, b, r),
            }
        },
        decode,
    )
}
