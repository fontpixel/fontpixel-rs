//! Native SDL SFont marker strips and Kreative RFont control-pixel PNGs.
mod rfont;
use super::image_bundle_common::*;
use super::{Confidence, WriteOptions};
use crate::model::{Codepoint, Font, FontSet, Glyph, Vector};
use crate::resources::{OutputBundle, ReadContext, Written};
use crate::{Report, ResourceLimits, Result};
use std::collections::{BTreeMap, BTreeSet};
/// Native marker-image dialect.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum SfontFlavor {
    /// SDL SFont, implicit ASCII 33..126 by default.
    #[default]
    Sfont,
    /// Kreative RFont, explicit Unicode codepoint and metric control columns.
    Rfont,
}
/// Caller-declared interpretation of native image fields.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct SfontReadOptions {
    /// Force a dialect; absent detects RFont's control columns/chunks.
    pub flavor: Option<SfontFlavor>,
    /// Explicit SFont character order; absent uses consecutive ASCII from 33.
    pub codepoints: Option<Vec<Codepoint>>,
    /// SFont baseline measured from glyph top; absent uses image glyph height.
    pub baseline: Option<i64>,
}
/// Deterministic native PNG output options.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct SfontWriteOptions {
    /// Native dialect, default SFont.
    pub flavor: SfontFlavor,
    /// Explicit SFont order; absent emits ASCII 33..126 including blank slots.
    pub codepoints: Option<Vec<Codepoint>>,
    /// Cumulative source, image codec, output and readback allowance.
    pub limits: ResourceLimits,
    /// Additional PNG byte cap.
    pub max_output_bytes: usize,
}
impl Default for SfontWriteOptions {
    fn default() -> Self {
        Self {
            flavor: SfontFlavor::Sfont,
            codepoints: None,
            limits: ResourceLimits::default(),
            max_output_bytes: 64 * 1024 * 1024,
        }
    }
}
codec!(SfontCodec, SfontReadOptions, SfontWriteOptions);
/// Generic image signatures are possible, not unique font identifiers.
pub fn detect(b: &[u8]) -> Confidence {
    if rfont::table_evidence(b) {
        return Confidence::Certain;
    }
    if b.starts_with(b"\x89PNG\r\n\x1a\n") || b.starts_with(b"BM") || b.starts_with(b"GIF8") {
        Confidence::Possible
    } else {
        Confidence::NoMatch
    }
}
const PINK: [u8; 4] = [255, 0, 255, 255];
fn at(r: &Raster, x: u32, y: u32) -> [u8; 4] {
    let i = (u64::from(y) * u64::from(r.width) + u64::from(x)) as usize * 4;
    r.pixels[i..i + 4].try_into().unwrap()
}
fn put(p: &mut [u8], w: u32, x: u32, y: u32, c: [u8; 4]) {
    let i = (u64::from(y) * u64::from(w) + u64::from(x)) as usize * 4;
    p[i..i + 4].copy_from_slice(&c);
}
fn codes(custom: &Option<Vec<Codepoint>>, n: usize, b: &mut Budget) -> Result<Vec<Codepoint>> {
    check(n, 255, "SFont marker glyphs")?;
    b.items::<Codepoint>(n)?;
    b.add(n.saturating_mul(64))?;
    let out = if let Some(c) = custom {
        if c.len() != n {
            return Err(invalid(
                "SFont character order length differs from marker count",
            ));
        }
        c.clone()
    } else {
        (0..n).map(|i| Codepoint::Unicode(33 + i as u32)).collect()
    };
    let mut unique = BTreeSet::new();
    for &cp in &out {
        if matches!(cp,Codepoint::Unicode(x) if char::from_u32(x).is_none()) || !unique.insert(cp) {
            return Err(invalid("invalid or duplicate SFont character order"));
        }
    }
    Ok(out)
}
fn sfont(
    bytes: &[u8],
    r: Raster,
    o: &SfontReadOptions,
    l: &ResourceLimits,
    b: &mut Budget,
) -> Result<(FontSet, Report)> {
    if r.height < 2 {
        return Err(invalid("SFont needs marker and glyph rows"));
    }
    b.items::<(u32, u32)>(256)?;
    let mut markers = Vec::new();
    let mut x = 0;
    while x < r.width {
        if at(&r, x, 0) != PINK {
            return Err(invalid("SFont marker row must start and end in magenta"));
        }
        let start = x;
        while x < r.width && at(&r, x, 0) == PINK {
            x += 1;
        }
        markers.push((start, x));
        check(markers.len(), 256, "SFont marker runs")?;
        while x < r.width && at(&r, x, 0) != PINK {
            x += 1;
        }
    }
    if markers.len() < 2 || markers.last().unwrap().1 != r.width {
        return Err(invalid("incomplete SFont marker row"));
    }
    let n = markers.len() - 1;
    let labels = codes(&o.codepoints, n, b)?;
    allocation(n, l, b)?;
    let h = r.height - 1;
    let baseline = o.baseline.unwrap_or(i64::from(h));
    let bottom = baseline
        .checked_sub(i64::from(h))
        .ok_or_else(|| invalid("SFont baseline overflow"))?;
    let descent = bottom
        .checked_neg()
        .ok_or_else(|| invalid("SFont baseline overflow"))?;
    let alpha = r.pixels.as_chunks::<4>().0.iter().any(|p| p[3] != 255);
    let paper = at(&r, 0, r.height - 1);
    let mut f = Font::default();
    f.metrics.ascent = Some(baseline);
    f.metrics.descent = Some(descent);
    f.metrics.line_height = Some(i64::from(h));
    let mut report = Report::default();
    let mut color = false;
    for (i, cp) in labels.into_iter().enumerate() {
        let (a, z) = markers[i];
        let (next, end) = markers[i + 1];
        let left = a + (z - a) / 2;
        let right = next + (end - next) / 2;
        let w = right - left;
        let count = area(w, h)?;
        check(count, l.max_pixels_per_glyph, "SFont glyph pixels")?;
        b.add(count)?;
        let mut data = Vec::with_capacity(count);
        for y in 1..r.height {
            for x in left..right {
                let p = at(&r, x, y);
                let value = if alpha {
                    color |= p[3] != 0 && (p[0] != p[1] || p[1] != p[2]);
                    p[3]
                } else if p == paper {
                    0
                } else {
                    color |= p[0] != p[1]
                        || p[1] != p[2]
                        || paper[0] != paper[1]
                        || paper[1] != paper[2];
                    if paper[0] == paper[1] && paper[1] == paper[2] && p[0] == p[1] && p[1] == p[2]
                    {
                        p[0].abs_diff(paper[0])
                    } else {
                        255
                    }
                };
                data.push(value);
            }
        }
        let mut g = glyph(
            w,
            h,
            i64::from(left) - i64::from(z),
            bottom,
            Vector::new(i64::from(next - z), 0),
            &data,
            false,
            l,
            b,
        )?;
        g.codepoints.push(cp);
        f.add_glyph(g)?;
    }
    let first_mid = markers[0].0 + (markers[0].1 - markers[0].0) / 2;
    let (last_start, last_end) = *markers.last().unwrap();
    let last_mid = last_start + (last_end - last_start) / 2;
    let discarded = (1..r.height).any(|y| {
        (0..first_mid).chain(last_mid..r.width).any(|x| {
            let p = at(&r, x, y);
            if alpha { p[3] != 0 } else { p != paper }
        })
    });
    if discarded {
        b.diagnostic(
            &mut report,
            "sfont",
            "read.outside_markers",
            crate::report::Impact::Loss,
            Some(0),
            None,
            "Pixels outside the SDL marker midpoint cells are not rendered",
        )?;
    }
    if color {
        b.diagnostic(
            &mut report,
            "sfont",
            "read.color",
            crate::report::Impact::Loss,
            Some(0),
            None,
            "Colored pixels were projected to coverage",
        )?;
    }
    if r.color.bits_per_pixel() > r.color.channel_count() as u16 * 8 {
        b.diagnostic(
            &mut report,
            "sfont",
            "read.depth",
            crate::report::Impact::Loss,
            Some(0),
            None,
            "Image samples were reduced to 8-bit coverage",
        )?;
    }
    rfont::read_text(bytes, &mut f, b)?;
    Ok((set(f, "sfont"), report))
}
fn parse(
    bytes: &[u8],
    name: &str,
    o: &SfontReadOptions,
    l: &ResourceLimits,
    b: &mut Budget,
) -> Result<(FontSet, Report)> {
    let r = decode(bytes, name, l, b)?;
    let flavor = o.flavor.unwrap_or_else(|| {
        if rfont::recognized(bytes, &r) {
            SfontFlavor::Rfont
        } else {
            SfontFlavor::Sfont
        }
    });
    match flavor {
        SfontFlavor::Sfont => sfont(bytes, r, o, l, b),
        SfontFlavor::Rfont => {
            if o.codepoints.is_some() || o.baseline.is_some() {
                return Err(invalid("RFont contains its own codepoints and baseline"));
            }
            rfont::read(bytes, r, l, b)
        }
    }
}
/// Read native marker image data through caller-authorized input.
pub fn read(c: &mut ReadContext<'_>, o: &SfontReadOptions) -> Result<(FontSet, Report)> {
    let mut b = Budget::new(c.retained_input_bytes(), c.limits())?;
    parse(
        c.bytes(),
        c.name().unwrap_or("font.png"),
        o,
        c.limits(),
        &mut b,
    )
}
fn sfont_write(f: &Font, v: &SfontWriteOptions, b: &mut Budget) -> Result<Vec<u8>> {
    let count = v.codepoints.as_ref().map_or(94, Vec::len);
    if count == 0 {
        return Err(invalid("empty SFont character order"));
    }
    let labels = codes(&v.codepoints, count, b)?;
    b.items::<(Codepoint, &Glyph)>(f.len())?;
    let mut lookup = BTreeMap::new();
    for (_, g) in f.glyphs() {
        for &cp in &g.codepoints {
            b.add(96)?;
            lookup.insert(cp, g);
        }
    }
    b.items::<(Option<&Glyph>, u32, u32, u32)>(count)?;
    let mut entries = Vec::with_capacity(count);
    let mut top = f.metrics.ascent.unwrap_or(0).max(0);
    let mut bottom = f.metrics.descent.unwrap_or(0).max(0);
    for cp in labels {
        let g = lookup.get(&cp).copied();
        let (left, advance, right) = if let Some(g) = g {
            let bb = g.metrics.bounds;
            let adv = g.metrics.advance.map_or(i64::from(bb.width), |v| v.x);
            top = top.max(
                bb.y.checked_add(i64::from(bb.height))
                    .ok_or_else(|| invalid("SFont vertical overflow"))?,
            );
            bottom = bottom.max(
                bb.y.checked_neg()
                    .ok_or_else(|| invalid("SFont vertical overflow"))?,
            );
            (
                bb.x.checked_neg()
                    .ok_or_else(|| invalid("SFont bearing overflow"))?
                    .max(1),
                adv,
                bb.x.checked_add(i64::from(bb.width))
                    .and_then(|n| n.checked_sub(adv))
                    .ok_or_else(|| invalid("SFont bearing overflow"))?
                    .max(1),
            )
        } else {
            (1, 1, 1)
        };
        let (l, a, r) = (
            u32::try_from(left).map_err(|_| invalid("SFont bearing exceeds u32"))?,
            u32::try_from(advance).map_err(|_| invalid("SFont advance must be positive"))?,
            u32::try_from(right).map_err(|_| invalid("SFont bearing exceeds u32"))?,
        );
        if a == 0 {
            return Err(invalid("SFont marker strips cannot encode zero advance"));
        }
        entries.push((g, l, a, r));
    }
    let h = u32::try_from(
        top.checked_add(bottom)
            .ok_or_else(|| invalid("SFont height overflow"))?
            .max(1),
    )
    .map_err(|_| invalid("SFont height exceeds u32"))?;
    let mut spans = Vec::with_capacity(count + 1);
    spans.push(entries[0].1);
    for pair in entries.windows(2) {
        spans.push(pair[0].3.max(pair[1].1));
    }
    spans.push(entries[count - 1].3);
    let width = entries
        .iter()
        .try_fold(0u32, |sum, e| sum.checked_add(e.2))
        .and_then(|n| {
            spans
                .iter()
                .try_fold(n, |sum, &w| sum.checked_add(w.checked_mul(2)?))
        })
        .ok_or_else(|| invalid("SFont strip width overflow"))?;
    let height = h
        .checked_add(1)
        .ok_or_else(|| invalid("SFont image height overflow"))?;
    let n = area(width, height)?;
    check(
        n.saturating_mul(4),
        v.limits.max_decompressed_bytes,
        "SFont image pixels",
    )?;
    b.add(n.saturating_mul(4))?;
    let mut pixels = vec![0; n * 4];
    let mut x = 0;
    for (i, (g, _, advance, _)) in entries.into_iter().enumerate() {
        for col in x..x + 2 * spans[i] {
            put(&mut pixels, width, col, 0, PINK);
        }
        x += 2 * spans[i];
        if let Some(g) = g {
            let gx = i64::from(x)
                .checked_add(g.metrics.bounds.x)
                .ok_or_else(|| invalid("SFont placement overflow"))?;
            let gy = 1 + top - g.metrics.bounds.y - i64::from(g.bitmap.height());
            for y in 0..if g.bitmap.width() == 0 {
                0
            } else {
                g.bitmap.height()
            } {
                for xx in 0..g.bitmap.width() {
                    let px = u32::try_from(gx + i64::from(xx))
                        .map_err(|_| invalid("SFont glyph outside atlas"))?;
                    let py = u32::try_from(gy + i64::from(y))
                        .map_err(|_| invalid("SFont glyph outside atlas"))?;
                    if px >= width || py >= height {
                        return Err(invalid("SFont glyph outside atlas"));
                    }
                    put(&mut pixels, width, px, py, [255, 255, 255, pixel(g, xx, y)]);
                }
            }
        }
        x += advance;
    }
    for col in x..width {
        put(&mut pixels, width, col, 0, PINK);
    }
    let base = png(
        width,
        height,
        &pixels,
        true,
        v.max_output_bytes,
        &v.limits,
        b,
    )?;
    rfont::add_text(base, f, v, b)
}
/// Write native PNG, then audit every model field against native readback.
pub fn write(s: &FontSet, o: &WriteOptions, v: &SfontWriteOptions) -> Result<Written> {
    let mut report = Report::default();
    let result = (|| {
        let mut b = source_budget(s, &v.limits)?;
        b.add(o.primary_name.as_str().len().saturating_mul(4))?;
        let f = s.single()?;
        let bytes = match v.flavor {
            SfontFlavor::Sfont => sfont_write(f, v, &mut b)?,
            SfontFlavor::Rfont => {
                if v.codepoints.is_some() {
                    return Err(invalid(
                        "RFont writer uses Unicode labels stored in the image",
                    ));
                }
                rfont::write(f, v, &mut b)?
            }
        };
        let ro = SfontReadOptions {
            flavor: Some(v.flavor),
            codepoints: v.codepoints.clone(),
            baseline: None,
        };
        let (target, r) = parse(&bytes, "font.png", &ro, &v.limits, &mut b)?;
        report.extend(r);
        audit(s, &target, "sfont", &mut report, &mut b)?;
        Ok(OutputBundle::single(o.primary_name.clone(), bytes))
    })();
    finish(result, report, "sfont", o.strict)
}
