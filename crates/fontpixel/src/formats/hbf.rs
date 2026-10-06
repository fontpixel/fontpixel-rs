//! HBF 1.1 headers and explicitly scoped, multi-file monochrome bitmaps.
mod header;
mod metadata;
mod writer;

use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
use crate::bitmap::{Bitmap, BitmapLimits, Bounds};
use crate::model::{Codepoint, Font, FontSet, Glyph, GlyphMetrics, Vector};
use crate::report::{Impact, Level, Location, ReportEntry};
use crate::resources::{ReadContext, Written};
use crate::{Error, Report, ResourceLimits, Result};
use std::collections::BTreeMap;
pub use writer::{HbfWriteOptions, write};

/// Registry HBF codec. Reads are controlled by the common resource limits.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct HbfCodec {
    /// Deterministic bitmap bundle options.
    pub write_options: HbfWriteOptions,
}
impl FormatDetector for HbfCodec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(detect(c.bytes()))
    }
}
impl FormatReader<FontSet> for HbfCodec {
    fn read(&self, c: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
        let (font, mut report) = read(c)?;
        for entry in &mut report.entries {
            entry.location.asset = c.name().map(str::to_owned);
        }
        let mut set = FontSet::from(font);
        set.source_format = Some("hbf".into());
        Ok((set, report))
    }
}
impl FormatWriter<FontSet> for HbfCodec {
    fn write(&self, set: &FontSet, o: &WriteOptions) -> Result<Written> {
        let mut out = write(set.single()?, &o.primary_name, &self.write_options)?;
        let mut budget = Budget::new(self.write_options.limits.max_total_bytes);
        for asset in out.output.assets() {
            budget.add(asset.bytes.len())?;
        }
        for entry in &out.report.entries {
            budget.add(256 + entry.message.len())?;
        }
        for p in set.properties.iter() {
            budget.add(256 + p.name.len())?;
            loss(
                &mut out.report,
                "container_property",
                None,
                format!("Container property {:?} has no HBF field", p.name),
            );
        }
        Ok(out)
    }
}
/// Recognize HBF magic after optional COMMENT records within an 8 KiB sniff.
pub fn detect(bytes: &[u8]) -> Confidence {
    for line in bytes[..bytes.len().min(8192)].split(|&b| b == b'\n') {
        let line = line.trim_ascii();
        let token = line
            .split(|b| b.is_ascii_whitespace())
            .next()
            .unwrap_or_default();
        if line.is_empty() || token == b"COMMENT" {
            continue;
        }
        return if token == b"HBF_START_FONT" {
            Confidence::Certain
        } else {
            Confidence::NoMatch
        };
    }
    Confidence::NoMatch
}
fn invalid(s: impl Into<String>) -> Error {
    Error::InvalidInput(format!("HBF: {}", s.into()))
}
fn limit(resource: &'static str, limit: usize) -> Error {
    Error::ResourceLimit { resource, limit }
}
fn check(n: usize, max: usize, resource: &'static str) -> Result<()> {
    if n > max {
        Err(limit(resource, max))
    } else {
        Ok(())
    }
}
struct Budget {
    used: usize,
    max: usize,
}
impl Budget {
    fn new(max: usize) -> Self {
        Self { used: 0, max }
    }
    fn add(&mut self, n: usize) -> Result<()> {
        self.used = self
            .used
            .checked_add(n)
            .ok_or(limit("HBF total bytes", self.max))?;
        check(self.used, self.max, "HBF total bytes")
    }
    fn remaining(&self) -> usize {
        self.max - self.used
    }
}
fn loss(r: &mut Report, code: &str, index: Option<usize>, message: impl Into<String>) {
    r.push(ReportEntry {
        level: Level::Warning,
        impact: Impact::Loss,
        code: format!("hbf.{code}"),
        location: Location {
            font_index: Some(0),
            glyph_index: index,
            ..Default::default()
        },
        message: message.into(),
    });
}
fn cell_bytes(b: Bounds, l: &ResourceLimits) -> Result<usize> {
    if b.width == 0 || b.height == 0 {
        return Err(invalid("empty bitmap cell"));
    }
    let area = u64::from(b.width) * u64::from(b.height);
    if area > l.max_pixels_per_glyph as u64 {
        return Err(limit("HBF glyph pixels", l.max_pixels_per_glyph));
    }
    b.x.checked_add(i64::from(b.width))
        .ok_or_else(|| invalid("bounding box x overflow"))?;
    b.y.checked_add(i64::from(b.height))
        .ok_or_else(|| invalid("bounding box y overflow"))?;
    let size = usize::try_from(u64::from(b.width).div_ceil(8) * u64::from(b.height))
        .map_err(|_| limit("HBF bitmap bytes", l.max_total_bytes))?;
    check(size, l.max_total_bytes, "HBF bitmap bytes")?;
    Ok(size)
}
fn contained(b: Bounds, f: Bounds) -> Result<()> {
    let end = |x: i64, w: u32| {
        x.checked_add(i64::from(w))
            .ok_or_else(|| invalid("bounding box overflow"))
    };
    if f.width == 0
        || f.height == 0
        || b.x < f.x
        || b.y < f.y
        || end(b.x, b.width)? > end(f.x, f.width)?
        || end(b.y, b.height)? > end(f.y, f.height)?
    {
        return Err(invalid(
            "bitmap bounding box does not fit font bounding box",
        ));
    }
    Ok(())
}
fn glyph_metrics(bitmap: Bounds, font: Bounds) -> GlyphMetrics {
    GlyphMetrics {
        bounds: bitmap,
        advance: Some(Vector::new(i64::from(font.width), 0)),
        ..Default::default()
    }
}
/// Read an HBF header and bitmap resources through the caller's scoped context.
/// No paths are opened directly and no partial fonts are returned on failure.
pub fn read(c: &mut ReadContext<'_>) -> Result<(Font, Report)> {
    let limits = c.limits().clone();
    let mut budget = Budget::new(limits.max_total_bytes);
    budget.add(c.retained_input_bytes())?;
    // Header-owned strings/properties, duplicate semantic projections and parser records.
    budget.add(
        c.bytes()
            .len()
            .checked_mul(32)
            .ok_or(limit("HBF header bytes", limits.max_total_bytes))?,
    )?;
    let mut h = header::parse(c.bytes(), &limits)?;
    let size = cell_bytes(h.bitmap, &limits)?;
    contained(h.bitmap, h.bounds)?;
    let mut count = 0usize;
    for range in &h.ranges {
        count = count
            .checked_add(h.space.count(range.first, range.last)?)
            .ok_or(limit("HBF glyphs", limits.max_glyphs))?;
        check(count, limits.max_glyphs, "HBF glyphs")?;
    }
    if let Some(expected) = h.chars
        && expected != count
    {
        return Err(invalid("CHARS count disagrees with code ranges"));
    }
    budget.add(
        count
            .checked_mul(
                size.checked_add(1536)
                    .ok_or(limit("HBF model bytes", limits.max_total_bytes))?,
            )
            .ok_or(limit("HBF model bytes", limits.max_total_bytes))?,
    )?;
    let mut resources = BTreeMap::new();
    for range in &h.ranges {
        if !resources.contains_key(&range.path) {
            budget.add(128 + range.path.as_str().len())?;
            let bytes = c.resource_limited(&range.path, budget.remaining())?;
            budget.add(bytes.len())?;
            resources.insert(range.path.clone(), bytes);
        }
        let n = h.space.count(range.first, range.last)?;
        let length = n
            .checked_mul(size)
            .ok_or_else(|| invalid("range bitmap length overflow"))?;
        let end = range
            .offset
            .checked_add(length)
            .ok_or_else(|| invalid("bitmap offset overflow"))?;
        if end > resources[&range.path].len() {
            return Err(invalid(format!(
                "bitmap file {} is truncated for code range",
                range.path
            )));
        }
    }
    let mut report = Report::default();
    let unicode = metadata::project(&mut h.font, &h.scheme, &h.ranges, h.bounds, &mut report)?;
    let bitmap_limits = BitmapLimits {
        max_dimension: u32::MAX,
        max_pixels: limits.max_pixels_per_glyph,
        max_bytes: limits.max_total_bytes,
    };
    let mut index = 0;
    for range in &h.ranges {
        let bytes = &resources[&range.path];
        let first = h.space.rank(range.first)?;
        let last = h.space.rank(range.last)?;
        for rank in first..=last {
            let cp = h.space.code(rank);
            if h.scheme.eq_ignore_ascii_case("Unicode 1.1") && cp > 0xffff {
                return Err(invalid("Unicode 1.1 uses the two-byte UCS-2 coding space"));
            }
            if unicode && char::from_u32(cp).is_none() {
                return Err(invalid("Unicode code is not a scalar value"));
            }
            let start = range.offset + (rank - first) * size;
            metadata::check_code(&h.font, cp)?;
            let data = &bytes[start..start + size];
            if h.bitmap.width % 8 != 0 {
                let stride = (h.bitmap.width as usize).div_ceil(8);
                let mask = (1u8 << (8 - h.bitmap.width % 8)) - 1;
                if data
                    .chunks_exact(stride)
                    .any(|row| row[stride - 1] & mask != 0)
                {
                    loss(
                        &mut report,
                        "padding_bits",
                        Some(index),
                        "Nonzero row padding bits are normalized",
                    );
                }
            }
            let bitmap: Bitmap =
                Bitmap::from_raw(h.bitmap.width, h.bitmap.height, data, bitmap_limits)?;
            let mut g = Glyph::new(bitmap);
            g.metrics = glyph_metrics(h.bitmap, h.bounds);
            g.codepoints.push(if unicode {
                Codepoint::Unicode(cp)
            } else {
                Codepoint::Legacy(cp)
            });
            h.font.add_glyph(g)?;
            index += 1;
        }
    }
    Ok((h.font, report))
}

#[cfg(all(test, feature = "serde"))]
mod serde_tests {
    use super::*;
    use crate::formats::serde_test::defaults;

    #[test]
    fn options_serde_defaults_and_rejections() {
        defaults::<HbfCodec>();
    }
}
