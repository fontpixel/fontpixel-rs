//! Unifont hexdraw text fonts.
use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
use crate::resources::{ReadContext, Written};
use crate::{FontSet, Report, ResourceLimits, Result};
/// Bounded hexdraw export settings.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct DrawWriteOptions {
    /// Explicit grayscale threshold; absent rejects grayscale.
    pub gray_threshold: Option<u8>,
    /// Maximum emitted text bytes.
    pub max_output_bytes: usize,
    /// Reader and model budgets.
    pub limits: ResourceLimits,
}
impl Default for DrawWriteOptions {
    fn default() -> Self {
        Self {
            gray_threshold: None,
            max_output_bytes: 64 * 1024 * 1024,
            limits: ResourceLimits::default(),
        }
    }
}
/// Hexdraw registry codec.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct DrawCodec {
    /// Export configuration.
    pub write_options: DrawWriteOptions,
}
impl FormatDetector for DrawCodec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(detect(c.bytes()))
    }
}
impl FormatReader<FontSet> for DrawCodec {
    fn read(&self, c: &mut ReadContext<'_>, o: &ReadOptions) -> Result<(FontSet, Report)> {
        // This strict parser has no repair diagnostics; no asset strings are cloned.
        parse(c.bytes(), &o.limits, c.retained_input_bytes())
    }
}
impl FormatWriter<FontSet> for DrawCodec {
    fn write(&self, s: &FontSet, o: &WriteOptions) -> Result<Written> {
        let (bytes, report) = write(s, &self.write_options)?;
        Ok(Written {
            output: crate::resources::OutputBundle::single(o.primary_name.clone(), bytes),
            report,
        })
    }
}

use super::console::w41 as support;
use super::console::{
    Budget, Output, bitmap_limits, cell_size, check, glyph_metrics, invalid, metrics, raster,
};
use crate::bitmap::{Bitmap, Bounds};
use crate::model::{Codepoint, Font, Glyph, Property, PropertyValue};
use crate::report::Impact;
use support::{BoundedReport, compare, fail, failed_projection, model_cost, note};
fn label(s: &str) -> Option<(u32, &str)> {
    let (label, row) = s.split_once(':')?;
    if !(4..=6).contains(&label.len()) || !label.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let cp = u32::from_str_radix(label, 16).ok()?;
    char::from_u32(cp)?;
    Some((cp, row.trim()))
}
fn pixel_row(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b == b'#' || b == b'-')
}
/// Recognize a Unicode label and at least one valid pixel row in bounded text.
pub fn detect(bytes: &[u8]) -> Confidence {
    let bytes = &bytes[..bytes.len().min(4096)];
    let Ok(s) = std::str::from_utf8(bytes) else {
        return Confidence::NoMatch;
    };
    let mut pending = false;
    for line in s.lines().take(80) {
        let line = line.trim();
        if line.is_empty() || line.starts_with('%') {
            continue;
        }
        if let Some((_, row)) = label(line) {
            if pixel_row(row) {
                return Confidence::Possible;
            }
            pending = row.is_empty();
        } else if pending && pixel_row(line) {
            return Confidence::Possible;
        } else {
            return Confidence::NoMatch;
        }
    }
    Confidence::NoMatch
}
/// Read rectangular hexdraw glyphs without guessing a legacy encoding.
pub fn read(bytes: &[u8], limits: &ResourceLimits) -> Result<(FontSet, Report)> {
    parse(bytes, limits, bytes.len())
}
fn parse(bytes: &[u8], limits: &ResourceLimits, retained: usize) -> Result<(FontSet, Report)> {
    check(bytes.len(), limits.max_input_bytes, "draw input bytes")?;
    let text = std::str::from_utf8(bytes).map_err(|_| invalid("draw input is not UTF-8"))?;
    let mut budget = Budget::new(retained, limits)?;
    budget.add(
        std::mem::size_of::<Font>()
            .saturating_mul(2)
            .saturating_add(512),
    )?;
    let mut font = Font::default();
    font.source_format = Some("draw".into());
    font.charset.name = Some("unicode".into());
    let mut pending: Option<(u32, u32, u32, Vec<u8>)> = None;
    let mut max_width = 0;
    let mut max_height = 0;
    fn finish(
        p: &mut Option<(u32, u32, u32, Vec<u8>)>,
        font: &mut Font,
        limits: &ResourceLimits,
        max_width: &mut u32,
        max_height: &mut u32,
    ) -> Result<()> {
        if let Some((cp, w, h, data)) = p.take() {
            if h == 0 {
                return Err(invalid("draw glyph has no rows"));
            }
            check(font.len() + 1, limits.max_glyphs, "draw glyph count")?;
            let mut g = Glyph::new(Bitmap::<crate::bitmap::Mono>::from_raw(
                w,
                h,
                &data,
                bitmap_limits(limits),
            )?);
            g.metrics = glyph_metrics(Bounds::new(w, h, 0, 0));
            g.codepoints.push(Codepoint::Unicode(cp));
            font.add_glyph(g)?;
            *max_width = (*max_width).max(w);
            *max_height = (*max_height).max(h);
        }
        Ok(())
    }
    for line in text.lines() {
        check(line.len(), limits.max_table_bytes, "draw line bytes")?;
        let line = line.trim();
        if line.is_empty() {
            finish(
                &mut pending,
                &mut font,
                limits,
                &mut max_width,
                &mut max_height,
            )?;
            continue;
        }
        if let Some(comment) = line.strip_prefix('%') {
            budget.add(comment.len().saturating_add(512))?;
            font.headers
                .push(Property::new("draw.comment", comment.trim_start()));
            continue;
        }
        let row = if let Some((cp, row)) = label(line) {
            finish(
                &mut pending,
                &mut font,
                limits,
                &mut max_width,
                &mut max_height,
            )?;
            budget.add(1152)?;
            pending = Some((cp, 0, 0, Vec::new()));
            row
        } else {
            line
        };
        if row.is_empty() {
            continue;
        }
        if !pixel_row(row) {
            return Err(invalid("invalid draw label or pixel row"));
        }
        let Some((_, width, height, data)) = &mut pending else {
            return Err(invalid("draw pixels precede first label"));
        };
        let w = u32::try_from(row.len()).map_err(|_| invalid("draw row width exceeds u32"))?;
        if *height == 0 {
            *width = w;
        } else if *width != w {
            return Err(invalid("draw rows have unequal widths"));
        }
        *height = height
            .checked_add(1)
            .ok_or_else(|| invalid("draw height overflow"))?;
        cell_size(*width, *height, limits)?;
        let n = row.len().div_ceil(8);
        budget.add(n.saturating_mul(3))?;
        data.try_reserve(n)
            .map_err(|_| invalid("draw allocation failed"))?;
        let start = data.len();
        data.resize(start + n, 0);
        for (x, b) in row.bytes().enumerate() {
            if b == b'#' {
                data[start + x / 8] |= 0x80 >> (x % 8);
            }
        }
    }
    finish(
        &mut pending,
        &mut font,
        limits,
        &mut max_width,
        &mut max_height,
    )?;
    if font.is_empty() {
        return Err(invalid("draw requires at least one glyph"));
    }
    font.metrics = metrics(max_width, max_height, 0);
    let mut set = FontSet::from(font);
    set.source_format = Some("draw".into());
    Ok((set, Report::default()))
}
/// Encode a single strike, preserving Unicode labels and rectangular pixels.
pub fn write(set: &FontSet, options: &DrawWriteOptions) -> Result<(Vec<u8>, Report)> {
    set.validate(&options.limits)?;
    let retained = model_cost(set, &options.limits)?;
    let font = set.single()?;
    let mut report = BoundedReport::new(retained, &options.limits)?;
    let result = (|| {
        // Bound the complete text before Output/row/raster allocation.
        let mut expected = 0usize;
        let mut raster_scratch = 0usize;
        for p in font.headers.iter() {
            if p.name == "draw.comment"
                && let PropertyValue::Text(s) = &p.value
                && !s.contains(['\n', '\r'])
            {
                expected = expected
                    .checked_add(s.len().saturating_add(3))
                    .ok_or_else(|| invalid("draw output size overflow"))?;
            }
        }
        for (_, g) in font.glyphs() {
            if g.codepoints.iter().any(|cp| cp.scalar().is_some()) {
                let w = g.bitmap.width() as usize;
                let h = g.bitmap.height() as usize;
                expected = expected
                    .checked_add(
                        w.checked_add(2)
                            .and_then(|n| n.checked_mul(h))
                            .and_then(|n| n.checked_add(10))
                            .ok_or_else(|| invalid("draw output size overflow"))?,
                    )
                    .ok_or_else(|| invalid("draw output size overflow"))?;
                raster_scratch = raster_scratch.max(
                    g.bitmap
                        .as_bytes()
                        .len()
                        .saturating_mul(3)
                        .saturating_add(w),
                );
            }
        }
        check(
            expected,
            options.max_output_bytes.min(options.limits.max_input_bytes),
            "draw output bytes",
        )?;
        report.reserve(
            expected
                .saturating_mul(2)
                .saturating_add(raster_scratch)
                .saturating_add(1024),
        )?;
        let mut out = Output::new(
            options
                .max_output_bytes
                .min(options.limits.max_input_bytes)
                .min(options.limits.max_total_bytes),
        );
        for p in font.headers.iter() {
            if p.name == "draw.comment"
                && let PropertyValue::Text(s) = &p.value
                && !s.contains(['\n', '\r'])
            {
                out.add(b"% ")?;
                out.add(s.as_bytes())?;
                out.add(b"\n")?;
            }
        }

        for (id, g) in font.glyphs() {
            let cp = g.codepoints.iter().find_map(|c| c.scalar());
            let Some(cp) = cp else {
                note(
                    &mut report,
                    "draw",
                    "write.unencoded",
                    Some(0),
                    Some(id.index()),
                    None,
                    Impact::Loss,
                    "Glyph without a Unicode scalar label omitted",
                );
                for cp in &g.codepoints {
                    note(
                        &mut report,
                        "draw",
                        "write.mapping",
                        Some(0),
                        Some(id.index()),
                        Some(cp.value()),
                        Impact::Loss,
                        "Mapping is not a Unicode scalar",
                    );
                }
                continue;
            };
            let cell = g.metrics.bounds;
            cell_size(cell.width, cell.height, &options.limits)?;
            let bm = raster(g, cell, options.gray_threshold, &options.limits)?;
            out.add(format!("{cp:04X}:\t", cp = cp as u32).as_bytes())?;
            let mut row = vec![b'-'; cell.width as usize];
            let stride = (cell.width as usize).div_ceil(8);
            for (y, bytes) in bm.as_bytes().chunks_exact(stride).enumerate() {
                if y > 0 {
                    out.add(b"\t")?;
                }
                for (x, v) in row.iter_mut().enumerate() {
                    *v = if bytes[x / 8] & (0x80 >> (x % 8)) != 0 {
                        b'#'
                    } else {
                        b'-'
                    };
                }
                out.add(&row)?;
                out.add(b"\n")?;
            }
            out.add(b"\n")?;
        }
        let before_readback = report.used();
        report.reserve(
            font.len()
                .saturating_mul(2048)
                .saturating_add(expected.saturating_mul(3))
                .saturating_add(
                    std::mem::size_of::<Font>()
                        .saturating_mul(2)
                        .saturating_add(2048),
                ),
        )?;
        let (decoded, _) = parse(&out.bytes, &options.limits, before_readback)?;
        compare(set, &decoded, "draw", &mut report);
        Ok(out.bytes)
    })();
    match result {
        Ok(bytes) => Ok((bytes, report.finish()?)),
        Err(e) => {
            if matches!(e, crate::Error::ResourceLimit { .. }) {
                return Err(e);
            }
            failed_projection(set, "draw", &mut report);
            fail(report, "draw", e)
        }
    }
}

#[cfg(all(test, feature = "serde"))]
mod serde_tests {
    use super::*;
    use crate::formats::serde_test::defaults;

    #[test]
    fn options_serde_defaults_and_rejections() {
        defaults::<DrawWriteOptions>();
        defaults::<DrawCodec>();
    }
}
