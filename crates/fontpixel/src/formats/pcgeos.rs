//! Native PC/GEOS BSWF SBCS and DBCS bitmap font collections.
use super::legacy_common::*;
use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
use crate::bitmap::{Bitmap, Mono};
use crate::model::{
    Codepoint, Font, FontSet, Glyph, Property, PropertyValue, Rational, Spacing, Vector,
};
use crate::report::Impact;
use crate::resources::{OutputBundle, ReadContext, Written};
use crate::{Report, ResourceLimits, Result};
use std::collections::BTreeMap;
mod reader;
mod writer;
/// Native layout families.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum PcgeosVariant {
    /// Major 1: byte codes, 20-byte family, 10-byte point entries.
    Sbcs,
    /// Major 2: Unicode words, 40-byte family, 9-byte point entries.
    Dbcs,
}
/// Native input policy.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct PcgeosReadOptions {
    /// Override major-version layout selection when importing a native resource.
    pub variant: Option<PcgeosVariant>,
    /// Explicitly round fractional integer-only metrics to nearest, ties even.
    pub round_fractional_metrics: bool,
}
/// Bounded native output choices.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct PcgeosWriteOptions {
    /// Layout; absent preserves source, otherwise SBCS.
    pub variant: Option<PcgeosVariant>,
    /// Native registered font ID.
    pub font_id: Option<u16>,
    /// Shared family name.
    pub family: Option<String>,
    /// Reuse native bytes only after complete same-limit model comparison.
    pub preserve_native: bool,
    /// Explicit permission to round fractional integer-only native metrics.
    pub round_fractional_metrics: bool,
    /// Explicit permission to interpret Unicode numbers as SBCS byte codes.
    pub unicode_as_legacy: bool,
    /// Explicit permission to interpret legacy numbers as DBCS Unicode scalars.
    pub legacy_as_unicode: bool,
    /// Explicit grayscale threshold.
    pub gray_threshold: Option<u8>,
    /// Shared resource limits.
    pub limits: ResourceLimits,
    /// Output cap, also bounded by max_input_bytes.
    pub max_output_bytes: usize,
}
impl Default for PcgeosWriteOptions {
    fn default() -> Self {
        Self {
            variant: None,
            font_id: None,
            family: None,
            preserve_native: true,
            round_fractional_metrics: false,
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
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct PcgeosCodec {
    /// Input policy.
    pub read_options: PcgeosReadOptions,
    /// Output policy.
    pub write_options: PcgeosWriteOptions,
}
impl FormatDetector for PcgeosCodec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(if c.bytes().starts_with(b"BSWF") {
            Confidence::Certain
        } else {
            Confidence::NoMatch
        })
    }
}
impl FormatReader<FontSet> for PcgeosCodec {
    fn read(&self, c: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
        read(c, &self.read_options)
    }
}
impl FormatWriter<FontSet> for PcgeosCodec {
    fn write(&self, s: &FontSet, w: &WriteOptions) -> Result<Written> {
        write(s, w, &self.write_options)
    }
}
fn word(b: &[u8], at: usize) -> Result<u16> {
    u16_at(b, at, ByteOrder::Little)
}
fn dword(b: &[u8], at: usize) -> Result<u32> {
    u32_at(b, at, ByteOrder::Little)
}
fn fixed(b: &[u8], at: usize) -> Result<i64> {
    Ok(i64::from(word(b, at + 1)? as i16) * 256 + i64::from(slice(b, at, 1)?[0]))
}
fn put(b: &mut [u8], at: usize, v: usize) -> Result<()> {
    put16(
        b,
        at,
        u16::try_from(v).map_err(|_| invalid("PC/GEOS field exceeds word"))?,
        ByteOrder::Little,
    );
    Ok(())
}
fn put_fixed(b: &mut [u8], at: usize, q: i64) -> Result<()> {
    let whole = q.div_euclid(256);
    b[at] = q.rem_euclid(256) as u8;
    put16(
        b,
        at + 1,
        i16::try_from(whole)
            .map_err(|_| invalid("PC/GEOS fixed point integral part exceeds signed word"))?
            as u16,
        ByteOrder::Little,
    );
    Ok(())
}
fn rounded(q: i64) -> i64 {
    let n = q.div_euclid(256);
    let f = q.rem_euclid(256);
    n + i64::from(f > 128 || f == 128 && n % 2 != 0)
}
/// Decode all strikes, native fixed-point fields, metrics and pair tables.
pub fn read(c: &mut ReadContext<'_>, o: &PcgeosReadOptions) -> Result<(FontSet, Report)> {
    let mut b = Budget::new(c.retained_input_bytes(), c.limits())?;
    reader::parse(c.bytes(), o, c.limits(), &mut b)
}
/// Encode native bitmap records and audit a same-limit reread.
pub fn write(s: &FontSet, w: &WriteOptions, o: &PcgeosWriteOptions) -> Result<Written> {
    let mut r = Report::default();
    let result = writer::encode(s, w, o, &mut r);
    finish(result, r, "pcgeos", w.strict)
}
