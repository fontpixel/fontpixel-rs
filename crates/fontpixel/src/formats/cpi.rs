//! DOS CPI, Linux CP, and FreeDOS CPX codepage fonts.
use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
use crate::resources::{ReadContext, Written};
use crate::{FontSet, Report, ResourceLimits, Result};
/// CPI wire container.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Version {
    /// DOS FONT with absolute file offsets.
    #[default]
    Font,
    /// FONT.NT with relative entry offsets.
    FontNt,
    /// DRDOS shared bitmap tables.
    DrFont,
    /// Standalone Linux codepage, without CPI file header.
    Cp,
    /// UPX-compressed DOS COM carrying CPI data.
    Cpx,
}
/// Interpretation of DOS pointers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PointerMode {
    /// 32-bit linear file offsets.
    #[default]
    Linear,
    /// Explicit segment:offset addressing.
    Segmented,
}
/// CPI import settings.
#[derive(Debug, Clone, Default)]
pub struct CpiReadOptions {
    /// Pointer representation.
    pub pointer_mode: PointerMode,
}
/// CPI export settings.
#[derive(Debug, Clone)]
pub struct CpiWriteOptions {
    /// Output container.
    pub version: Version,
    /// Fallback page for models lacking codepage metadata.
    pub code_page: Option<u16>,
    /// Explicit grayscale threshold.
    pub gray_threshold: Option<u8>,
    /// Output byte limit.
    pub max_output_bytes: usize,
    /// Model and read-back limits.
    pub limits: ResourceLimits,
}
impl Default for CpiWriteOptions {
    fn default() -> Self {
        Self {
            version: Version::Font,
            code_page: None,
            gray_threshold: None,
            max_output_bytes: 64 * 1024 * 1024,
            limits: ResourceLimits::default(),
        }
    }
}
/// CPI registry codec.
#[derive(Debug, Clone, Default)]
pub struct CpiCodec {
    /// Import settings.
    pub read_options: CpiReadOptions,
    /// Export settings.
    pub write_options: CpiWriteOptions,
}
impl FormatDetector for CpiCodec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(detect(c.bytes()))
    }
}
impl FormatReader<FontSet> for CpiCodec {
    fn read(&self, c: &mut ReadContext<'_>, o: &ReadOptions) -> Result<(FontSet, Report)> {
        {
            let (set, mut report) = reader::parse(
                c.bytes(),
                &self.read_options,
                &o.limits,
                c.retained_input_bytes(),
                c.name().map_or(0, str::len),
            )?;
            for entry in &mut report.entries {
                entry.location.asset = c.name().map(str::to_owned);
            }
            Ok((set, report))
        }
    }
}
impl FormatWriter<FontSet> for CpiCodec {
    fn write(&self, s: &FontSet, o: &WriteOptions) -> Result<Written> {
        let (bytes, report) = write(s, &self.write_options)?;
        Ok(Written {
            output: crate::resources::OutputBundle::single(o.primary_name.clone(), bytes),
            report,
        })
    }
}

mod cpx;
mod pack;
mod reader;
use super::console::w41 as support;
mod writer;
use super::console::{
    Budget, Output, bitmap_limits, cell_size, check, glyph_metrics, invalid, metrics, raster,
};
use crate::bitmap::{Bitmap, Bounds};
use crate::model::{Codepoint, Font, Glyph, Property, PropertyValue};
use crate::report::Impact;
use support::{BoundedReport, compare, fail, failed_projection, model_cost, note};
/// Identify CPI signature or a validated standalone CP header.
pub fn detect(b: &[u8]) -> Confidence {
    if b.starts_with(b"\xffFONT   ")
        || b.starts_with(b"\xffFONT.NT")
        || b.starts_with(b"\x7fDRFONT ")
    {
        Confidence::Certain
    } else if cpx::signature(b)
        || (b.len() >= 40
            && b[..2] == [28, 0]
            && b[6..8] == [1, 0]
            && b[28..30] == [1, 0]
            && b[30..32] != [0, 0]
            && b[34] > 0
            && b[35] > 0)
    {
        Confidence::Possible
    } else {
        Confidence::NoMatch
    }
}
/// Decode all codepages and sizes using explicit pointer semantics.
pub fn read(b: &[u8], o: &CpiReadOptions, l: &ResourceLimits) -> Result<(FontSet, Report)> {
    reader::parse(b, o, l, b.len(), 0)
}
/// Encode all codepages and sizes, returning complete conversion diagnostics.
pub fn write(s: &FontSet, o: &CpiWriteOptions) -> Result<(Vec<u8>, Report)> {
    writer::write(s, o)
}
