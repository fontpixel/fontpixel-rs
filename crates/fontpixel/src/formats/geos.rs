//! Commodore GEOS VLIR/CVT fonts, MEGA subsets and publisher Unicode extensions.
use super::legacy_common::*;
use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
use crate::model::{Codepoint, Font, FontSet, Glyph, Property, PropertyValue, Vector};
use crate::resources::{OutputBundle, ReadContext, Written};
use crate::{Report, ResourceLimits, Result};
use std::collections::{BTreeMap, BTreeSet};
mod reader;
mod writer;
/// Native resource packaging.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeosContainer {
    /// One VLIR strike, without directory metadata or auxiliary records.
    Bare,
    /// A Convert-file resource directory and VLIR record stream.
    Cvt,
}
/// GEOS strike families, including the documented Bits'N'Picas extensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeosVariant {
    /// Original byte-coded strip with advance equal to slice width.
    Standard,
    /// Signed offset and unsigned advance pairs.
    Extended,
    /// Unicode block maps and offset/advance pairs.
    Utf8,
    /// Six sixteen-character subsets plus an index-only record.
    Mega,
    /// MEGA subsets with Unicode auxiliary blocks.
    MegaUtf8,
}
/// Ambiguous final character policy.
#[derive(Debug, Clone, Default)]
pub struct GeosReadOptions {
    /// Decode the later DEL entry; false accepts original 95-character resources.
    pub include_del: bool,
}
/// Bounded native output choices.
#[derive(Debug, Clone)]
pub struct GeosWriteOptions {
    /// Container; absent preserves source packaging, otherwise CVT.
    pub container: Option<GeosContainer>,
    /// Strike family; absent preserves each strike's family.
    pub variant: Option<GeosVariant>,
    /// Include DEL; absent preserves the read policy, otherwise false.
    pub include_del: Option<bool>,
    /// Native ten-bit font identifier.
    pub font_id: Option<u16>,
    /// Directory name, up to sixteen Latin-1 bytes.
    pub family: Option<String>,
    /// Reuse a native resource only after a complete same-limit model comparison.
    pub preserve_native: bool,
    /// Explicit permission to interpret Unicode scalars as original byte codes.
    pub unicode_as_legacy: bool,
    /// Explicit permission to interpret legacy codes as Unicode in extended output.
    pub legacy_as_unicode: bool,
    /// Explicit grayscale threshold.
    pub gray_threshold: Option<u8>,
    /// Shared resource limits.
    pub limits: ResourceLimits,
    /// Output cap, also bounded by max_input_bytes.
    pub max_output_bytes: usize,
}
impl Default for GeosWriteOptions {
    fn default() -> Self {
        Self {
            container: None,
            variant: None,
            include_del: None,
            font_id: None,
            family: None,
            preserve_native: true,
            unicode_as_legacy: false,
            legacy_as_unicode: false,
            gray_threshold: None,
            limits: ResourceLimits::default(),
            max_output_bytes: 64 * 1024 * 1024,
        }
    }
}
/// Native registry codec.
#[derive(Debug, Clone, Default)]
pub struct GeosCodec {
    /// Input policy.
    pub read_options: GeosReadOptions,
    /// Output policy.
    pub write_options: GeosWriteOptions,
}
impl FormatDetector for GeosCodec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        let b = c.bytes();
        Ok(if reader::is_cvt(b) {
            Confidence::Certain
        } else if b.len() >= 8
            && b[3] > 0
            && matches!(word(b, 4), Ok(8 | 14))
            && usize::from(word(b, 6)?) < b.len()
        {
            Confidence::Possible
        } else {
            Confidence::NoMatch
        })
    }
}
impl FormatReader<FontSet> for GeosCodec {
    fn read(&self, c: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
        read(c, &self.read_options)
    }
}
impl FormatWriter<FontSet> for GeosCodec {
    fn write(&self, s: &FontSet, w: &WriteOptions) -> Result<Written> {
        write(s, w, &self.write_options)
    }
}
fn word(b: &[u8], at: usize) -> Result<u16> {
    u16_at(b, at, ByteOrder::Little)
}
fn put(b: &mut [u8], at: usize, v: usize) -> Result<()> {
    put16(
        b,
        at,
        u16::try_from(v).map_err(|_| invalid("GEOS offset exceeds native word"))?,
        ByteOrder::Little,
    );
    Ok(())
}
/// Decode all native strikes and Unicode blocks with cumulative allocation limits.
pub fn read(c: &mut ReadContext<'_>, o: &GeosReadOptions) -> Result<(FontSet, Report)> {
    let mut budget = Budget::new(c.retained_input_bytes(), c.limits())?;
    reader::parse(c.bytes(), o, c.limits(), &mut budget)
}
/// Encode native records and audit a complete reread under the same limits.
pub fn write(s: &FontSet, w: &WriteOptions, o: &GeosWriteOptions) -> Result<Written> {
    let mut r = Report::default();
    let result = writer::encode(s, w, o, &mut r);
    finish(result, r, "geos", w.strict)
}
