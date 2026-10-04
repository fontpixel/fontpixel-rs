//! Native Cybiko sequential glyph records.
use super::device_common::*;
use super::{Confidence, WriteOptions};
use crate::model::{Codepoint, Font, FontSet};
use crate::resources::{OutputBundle, ReadContext, Written};
use crate::{Report, ResourceLimits, Result};
/// Native Cybiko byte positions remain Legacy mappings.
#[derive(Debug, Clone, Default)]
pub struct CybikoReadOptions {}
/// Bounded Cybiko output choices.
#[derive(Debug, Clone)]
pub struct CybikoWriteOptions {
    /// Permit numerical Unicode-to-native byte mapping.
    pub unicode_as_legacy: bool,
    /// Explicit grayscale-to-monochrome threshold.
    pub gray_threshold: Option<u8>,
    /// Cumulative resource limits, including native readback.
    pub limits: ResourceLimits,
    /// Additional primary output byte cap.
    pub max_output_bytes: usize,
}
impl Default for CybikoWriteOptions {
    fn default() -> Self {
        Self {
            unicode_as_legacy: false,
            gray_threshold: None,
            limits: ResourceLimits::default(),
            max_output_bytes: 64 * 1024 * 1024,
        }
    }
}
codec!(CybikoCodec, CybikoReadOptions, CybikoWriteOptions);
fn layout(b: &[u8]) -> Result<usize> {
    slice(b, 0, 4)?;
    if b[0] != 1 {
        return Err(invalid("invalid Cybiko version"));
    }
    let mut at = 4;
    for _ in 0..b[1] {
        let h = slice(b, at, 4)?;
        at += 4 + usize::from(h[2]).div_ceil(8) * usize::from(h[3]);
        slice(b, 0, at)?;
    }
    if at != b.len() {
        return Err(invalid("Cybiko trailing or truncated records"));
    }
    Ok(usize::from(b[1]))
}
/// A complete native stream is weak evidence because its version is one byte.
pub fn detect(b: &[u8]) -> Confidence {
    if layout(b).is_ok() {
        Confidence::Possible
    } else {
        Confidence::NoMatch
    }
}
/// Read all byte positions and preserve explicit native geometry.
pub fn read(c: &mut ReadContext<'_>, _: &CybikoReadOptions) -> Result<(FontSet, Report)> {
    let mut budget = Budget::new(c.retained_input_bytes(), c.limits())?;
    parse(c.bytes(), c.limits(), &mut budget)
}
fn parse(b: &[u8], l: &ResourceLimits, budget: &mut Budget) -> Result<(FontSet, Report)> {
    let n = layout(b)?;
    check(b.len(), l.max_table_bytes, "Cybiko table bytes")?;
    allocation(n, l, budget)?;
    let mut f = Font::default();
    let mut report = Report::default();
    let mut ascent = i64::from(b[3]);
    let mut at = 4;
    f.charset.name = Some("Cybiko".into());
    for i in 0..n {
        let left = b[at];
        let top = b[at + 1];
        let w = u32::from(b[at + 2]);
        let h = u32::from(b[at + 3]);
        let len = (w as usize).div_ceil(8) * h as usize;
        let data = &b[at + 4..at + 4 + len];
        at += 4 + len;
        if left == 0 && top == 0 && w == 1 && h == 1 && data == [0] {
            continue;
        }
        if i == 0 {
            ascent = i64::from(top) + i64::from(h);
        }
        check(
            w as usize * h as usize,
            l.max_pixels_per_glyph,
            "Cybiko glyph pixels",
        )?;
        budget.add(len)?;
        let mut g = glyph(
            w,
            h,
            i64::from(left),
            ascent - i64::from(top) - i64::from(h),
            i64::from(w),
            data,
            l,
        )?;
        if g.bitmap.as_bytes() != data {
            budget.loss(
                &mut report,
                "cybiko",
                "read.padding",
                Some(0),
                Some(i),
                "Nonzero raster padding is removed",
            )?;
        }
        g.codepoints
            .push(Codepoint::Legacy(((i + 32) & 255) as u32));
        f.add_glyph(g)?;
    }
    f.metrics.ascent = Some(ascent);
    f.metrics.descent = Some((i64::from(b[3]) - ascent).max(0));
    f.metrics.line_height = Some(i64::from(b[3]));
    f.metrics.pixel_size = Some(u32::from(b[3]));
    f.properties.push(crate::model::Property::new(
        "cybiko.max_width",
        i64::from(b[2]),
    ));
    Ok((set(f, "cybiko"), report))
}
/// Write native records, preserving placement and padding narrower ink to advance.
pub fn write(s: &FontSet, o: &WriteOptions, v: &CybikoWriteOptions) -> Result<Written> {
    let mut report = Report::default();
    let result = (|| {
        let mut budget = source_budget(s, &v.limits)?;
        let converted = convert_gray(s, v.gray_threshold, &v.limits, &mut budget)?;
        let f = converted.single()?;
        let mut map = select(f, 255, false, v.unicode_as_legacy, &mut budget)?;
        map.remove(&31);
        let count = map
            .keys()
            .map(|&cp| ((cp + 224) & 255) as usize + 1)
            .max()
            .ok_or_else(|| invalid("Cybiko has no representable mappings"))?;
        check(count, v.limits.max_glyphs, "Cybiko output slots")?;
        let ascent = f.metrics.ascent.unwrap_or_else(|| {
            map.values()
                .map(|g| g.metrics.bounds.y + i64::from(g.bitmap.height()))
                .max()
                .unwrap_or(0)
        });
        i32::try_from(ascent).map_err(|_| invalid("Cybiko baseline exceeds i32"))?;
        let height = byte(
            f.metrics
                .line_height
                .or(f.metrics.pixel_size.map(i64::from))
                .unwrap_or(ascent.saturating_add(f.metrics.descent.unwrap_or(0))),
        )?;
        // Without a space record the wire's baseline is the header height.
        let native_ascent = if map.contains_key(&32) {
            ascent
        } else {
            i64::from(height)
        };
        let mut size = 4;
        let mut max_width = byte(int(&f.properties, "cybiko.max_width", 0)?)?;
        for i in 0..count {
            if let Some(g) = map.get(&(((i + 32) & 255) as u32)) {
                let adv = byte(
                    g.metrics
                        .advance
                        .or(f.metrics.glyph_defaults.advance)
                        .map_or(i64::from(g.bitmap.width()), |a| a.x),
                )?;
                if u32::from(adv) < g.bitmap.width() {
                    return Err(invalid("Cybiko advance is narrower than bitmap"));
                }
                max_width = max_width.max(adv);
                size += 4 + usize::from(adv).div_ceil(8) * g.bitmap.height() as usize;
            } else {
                size += 5;
            }
        }
        let mut b = output(size, v.max_output_bytes, &v.limits, &mut budget)?;
        b[..4].copy_from_slice(&[1, count as u8, max_width, height]);
        let mut at = 4;
        for i in 0..count {
            if let Some(g) = map.get(&(((i + 32) & 255) as u32)) {
                let bm = mono(g)?;
                let bb = g.metrics.bounds;
                let w = byte(
                    g.metrics
                        .advance
                        .or(f.metrics.glyph_defaults.advance)
                        .map_or(i64::from(bm.width()), |a| a.x),
                )?;
                let h = byte(i64::from(bm.height()))?;
                let top = if i == 0 {
                    native_ascent - i64::from(h)
                } else {
                    native_ascent - bb.y - i64::from(h)
                };
                b[at..at + 4].copy_from_slice(&[byte(bb.x)?, byte(top)?, w, h]);
                at += 4;
                let stride = usize::from(w).div_ceil(8);
                check(
                    usize::from(w) * usize::from(h),
                    v.limits.max_pixels_per_glyph,
                    "Cybiko output pixels",
                )?;
                for y in 0..usize::from(h) {
                    b[at + y * stride..at + y * stride + bm.stride()]
                        .copy_from_slice(&bm.as_bytes()[y * bm.stride()..(y + 1) * bm.stride()]);
                }
                at += stride * usize::from(h);
            } else {
                b[at..at + 5].copy_from_slice(&[0, 0, 1, 1, 0]);
                at += 5;
            }
        }
        let (target, r) = parse(&b, &v.limits, &mut budget)?;
        report.extend(r);
        audit(s, &target, "cybiko", &mut report, &mut budget)?;
        Ok(OutputBundle::single(o.primary_name.clone(), b))
    })();
    finish(result, report, "cybiko", o.strict)
}
