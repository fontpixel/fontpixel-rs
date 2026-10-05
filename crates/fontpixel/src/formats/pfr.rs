//! Portable Font Resource v4 embedded bitmap strikes, from DAVIC Annex A.
use super::w5e2_outline_common as c;
use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
use crate::resources::{ReadContext, Written};
use crate::{FontSet, Report, ResourceLimits, Result};
mod wire;
/// Bitmap image encoding within native glyph programs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PfrCompression {
    /// Continuous, MSB-first packed bits.
    #[default]
    Raw,
    /// Four-bit white/black run pairs.
    Rle4,
    /// Eight-bit white/black run pairs.
    Rle8,
}
/// Typed input choices.
#[derive(Debug, Clone, Default)]
pub struct PfrReadOptions {
    /// Override structural dialect identification for ambiguous native tables.
    pub dialect: Option<PfrDialect>,
}
/// Native conventions whose published specification and FreeType consumer differ.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PfrDialect {
    /// DAVIC Annex A offsets, fixed-point positions and polarity.
    Davic,
    /// FreeType-compatible offsets and integer positions.
    FreeType,
}
/// Native output and explicit conversion choices.
#[derive(Debug, Clone)]
pub struct PfrWriteOptions {
    /// Raster encoding.
    pub compression: PfrCompression,
    /// Preserve an imported dialect when absent; new fonts default to FreeType.
    pub dialect: Option<PfrDialect>,
    /// Explicit grayscale threshold; absent means monochrome only.
    pub gray_threshold: Option<u8>,
    /// Permit Unicode numeric positions as legacy codes.
    pub unicode_as_legacy: bool,
    /// Store raster rows from the top downward.
    pub top_down: bool,
    /// Raw-stream bit value representing ink.
    pub black_pixel: bool,
    /// Cumulative allocation and field limits.
    pub limits: ResourceLimits,
    /// Additional output byte bound.
    pub max_output_bytes: usize,
}
impl Default for PfrWriteOptions {
    fn default() -> Self {
        Self {
            compression: PfrCompression::Raw,
            dialect: None,
            gray_threshold: None,
            unicode_as_legacy: false,
            top_down: false,
            black_pixel: true,
            limits: ResourceLimits::default(),
            max_output_bytes: 64 * 1024 * 1024,
        }
    }
}
/// Native registry codec.
#[derive(Debug, Clone, Default)]
pub struct PfrCodec {
    /// Typed reader options.
    pub read_options: PfrReadOptions,
    /// Typed writer options.
    pub write_options: PfrWriteOptions,
}
impl FormatDetector for PfrCodec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(detect(c.bytes()))
    }
}
impl FormatReader<FontSet> for PfrCodec {
    fn read(&self, c: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
        read(c, &self.read_options)
    }
}
impl FormatWriter<FontSet> for PfrCodec {
    fn write(&self, s: &FontSet, o: &WriteOptions) -> Result<Written> {
        write(s, o, &self.write_options)
    }
}
/// Probe the full section directory and matching trailer without allocating glyphs.
pub fn detect(b: &[u8]) -> Confidence {
    if wire::header(b).is_ok() {
        Confidence::Certain
    } else {
        Confidence::NoMatch
    }
}
/// Import all embedded bitmap strikes; report omitted outlines explicitly.
pub fn read(c: &mut ReadContext<'_>, p: &PfrReadOptions) -> Result<(FontSet, Report)> {
    c::input(c, |b, l, budget, r| {
        wire::decode_with(b, l, budget, r, p.dialect)
    })
}
/// Write a complete PFR container and audit its independently decoded model.
pub fn write(s: &FontSet, o: &WriteOptions, p: &PfrWriteOptions) -> Result<Written> {
    c::output(
        s,
        o,
        &p.limits,
        p.max_output_bytes,
        "pfr",
        super::ModelCharge::Estimate,
        |b, r| wire::encode(s, p, b, r),
        wire::decode,
    )
}
