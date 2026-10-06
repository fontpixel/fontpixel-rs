//! Native bounded Xerox AL, BITBLT and PrePress AC raster families.
mod alto;
mod bitblt;
mod detection;
mod prepress;
use super::workstation_common::*;
use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
use crate::bitmap::Bounds;
use crate::model::{Codepoint, Font, FontSet, Glyph, Property, PropertyValue, Rational, Vector};
use crate::report::{Impact, Level, Location};
use crate::resources::{OutputBundle, ReadContext, Written};
use crate::{Report, ReportEntry, ResourceLimits, Result};
use std::collections::BTreeMap;
/// Xerox native raster family.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum XeroxVariant {
    /// AL screen fonts with extension chains.
    Alto,
    /// Plain/kerned BITBLT strikes, including indexes.
    #[default]
    Bitblt,
    /// PrePress AC raster segments.
    Prepress,
}
/// BITBLT native layout.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum XeroxBitbltLayout {
    /// Raster columns equal character advances.
    Plain,
    /// Independent bearings and advances.
    #[default]
    Kerned,
    /// One or more plain strike bodies with a code map.
    #[cfg_attr(feature = "serde", serde(rename = "plain-index"))]
    PlainIndex,
    /// One or more kerned strike bodies with a code map.
    #[cfg_attr(feature = "serde", serde(rename = "kerned-index"))]
    KernedIndex,
}
/// Input interpretation.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct XeroxReadOptions {
    /// Require a family; absent distinguishes structural headers before trying AL.
    pub variant: Option<XeroxVariant>,
    /// Explicit baseline for an old AL file whose header stores zero.
    pub alto_baseline: Option<u8>,
}
/// Bounded native output choices.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct XeroxWriteOptions {
    /// Output family.
    pub variant: XeroxVariant,
    /// BITBLT directory/raster arrangement.
    pub bitblt_layout: XeroxBitbltLayout,
    /// Explicit AL baseline, in 0..127.
    pub alto_baseline: Option<u8>,
    /// Explicit AC physical raster rotation metadata, in arc-minutes.
    pub rotation_minutes: Option<u16>,
    /// Permit numeric Unicode codes.
    pub unicode_as_legacy: bool,
    /// Explicit grayscale reduction.
    pub gray_threshold: Option<u8>,
    /// Shared ceilings.
    pub limits: ResourceLimits,
    /// Output byte cap.
    pub max_output_bytes: usize,
}
impl Default for XeroxWriteOptions {
    fn default() -> Self {
        Self {
            variant: XeroxVariant::Bitblt,
            bitblt_layout: XeroxBitbltLayout::Kerned,
            alto_baseline: None,
            rotation_minutes: None,
            unicode_as_legacy: false,
            gray_threshold: None,
            limits: ResourceLimits::default(),
            max_output_bytes: 64 * 1024 * 1024,
        }
    }
}
/// Registry codec.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct XeroxCodec {
    /// Input choices.
    pub read_options: XeroxReadOptions,
    /// Output choices.
    pub write_options: XeroxWriteOptions,
}
impl FormatDetector for XeroxCodec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(detect(c.bytes()))
    }
}
impl FormatReader<FontSet> for XeroxCodec {
    fn read(&self, c: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
        read(c, &self.read_options)
    }
}
impl FormatWriter<FontSet> for XeroxCodec {
    fn write(&self, s: &FontSet, o: &WriteOptions) -> Result<Written> {
        write(s, o, &self.write_options)
    }
}
fn variant(b: &[u8]) -> XeroxVariant {
    let word = u16_at(b, 0, ByteOrder::Big).unwrap_or(0);
    if word == 0x100c {
        XeroxVariant::Prepress
    } else if word & 0x8000 != 0 && word & 4095 == 0 {
        XeroxVariant::Bitblt
    } else {
        XeroxVariant::Alto
    }
}
/// Only structured AC/BITBLT headers are auto-detection candidates; AL needs an explicit input format.
pub fn detect(b: &[u8]) -> Confidence {
    if b.len() >= 24 && detection::plausible(b, variant(b)) {
        Confidence::Possible
    } else {
        Confidence::NoMatch
    }
}
fn loss(r: &mut Report, code: &str, message: &str, fi: usize) {
    r.push(ReportEntry {
        level: Level::Warning,
        impact: Impact::Loss,
        code: format!("xerox.{code}"),
        location: Location {
            font_index: Some(fi),
            ..Default::default()
        },
        message: message.into(),
    });
}
fn grow(out: &mut Vec<u8>, n: usize, v: &XeroxWriteOptions, budget: &mut Budget) -> Result<()> {
    let total = out
        .len()
        .checked_add(n)
        .ok_or_else(|| invalid("Xerox output size overflow"))?;
    check(
        total,
        v.max_output_bytes
            .min(v.limits.max_input_bytes)
            .min(v.limits.max_table_bytes),
        "Xerox output bytes",
    )?;
    budget.add(n.saturating_mul(2))?;
    out.resize(total, 0);
    Ok(())
}
/// Decode an explicitly selected or structurally recognized family.
pub fn read(c: &mut ReadContext<'_>, o: &XeroxReadOptions) -> Result<(FontSet, Report)> {
    let mut budget = Budget::new(c.retained_input_bytes(), c.limits())?;
    parse(c.bytes(), o, c.limits(), &mut budget)
}
fn parse(
    b: &[u8],
    o: &XeroxReadOptions,
    l: &ResourceLimits,
    budget: &mut Budget,
) -> Result<(FontSet, Report)> {
    check(b.len(), l.max_table_bytes, "Xerox table bytes")?;
    match o.variant.unwrap_or_else(|| variant(b)) {
        XeroxVariant::Alto => alto::read(b, o, l, budget),
        XeroxVariant::Bitblt => bitblt::read(b, l, budget),
        XeroxVariant::Prepress => prepress::read(b, l, budget),
    }
}
/// Encode the complete requested family and audit a same-limit reread.
pub fn write(s: &FontSet, o: &WriteOptions, v: &XeroxWriteOptions) -> Result<Written> {
    let mut report = Report::default();
    let result = (|| {
        let mut budget = source_budget(s, &v.limits)?;
        let converted = convert_gray(s, v.gray_threshold, &v.limits, &mut budget)?;
        let out = match v.variant {
            XeroxVariant::Alto => alto::write(converted.single()?, v, &mut budget, &mut report)?,
            XeroxVariant::Bitblt => {
                bitblt::write(converted.single()?, v, &mut budget, &mut report)?
            }
            XeroxVariant::Prepress => prepress::write(&converted, v, &mut budget, &mut report)?,
        };
        let (actual, r) = parse(
            &out,
            &XeroxReadOptions {
                variant: Some(v.variant),
                alto_baseline: v.alto_baseline,
            },
            &v.limits,
            &mut budget,
        )?;
        report.extend(r);
        audit(s, &actual, "xerox", &mut report, &mut budget)?;
        Ok(OutputBundle::single(o.primary_name.clone(), out))
    })();
    finish(result, report, "xerox", o.strict)
}
