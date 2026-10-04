//! Project Oberon word-based screen and printer bitmap font resources.
use super::legacy_common::*;
use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
use crate::bitmap::{Bitmap, Bounds, Mono};
use crate::model::{Codepoint, Font, FontSet, Glyph, Property, Vector};
use crate::resources::{OutputBundle, ReadContext, Written};
use crate::{Report, ResourceLimits, Result};
use std::collections::BTreeMap;
/// Input policy for the documented bitmap wire format.
#[derive(Debug, Clone, Default)]
pub struct OberonReadOptions {}
/// Bounded native output choices.
#[derive(Debug, Clone)]
pub struct OberonWriteOptions {
    /// Native family byte; absent preserves its source property.
    pub family: Option<u8>,
    /// Native variant byte; absent preserves its source property, otherwise 32.
    pub variant: Option<u8>,
    /// Explicit permission to interpret Unicode numbers as native byte codes.
    pub unicode_as_legacy: bool,
    /// Explicit threshold for grayscale input.
    pub gray_threshold: Option<u8>,
    /// Shared resource limits.
    pub limits: ResourceLimits,
    /// Output cap, also bounded by max_input_bytes.
    pub max_output_bytes: usize,
}
impl Default for OberonWriteOptions {
    fn default() -> Self {
        Self {
            family: None,
            variant: None,
            unicode_as_legacy: false,
            gray_threshold: None,
            limits: ResourceLimits::default(),
            max_output_bytes: 64 * 1024 * 1024,
        }
    }
}
/// Native registry codec.
#[derive(Debug, Clone, Default)]
pub struct OberonCodec {
    /// Input options.
    pub read_options: OberonReadOptions,
    /// Output options.
    pub write_options: OberonWriteOptions,
}
impl FormatDetector for OberonCodec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(if c.bytes().starts_with(&[0xdb, 0]) {
            Confidence::Certain
        } else {
            Confidence::NoMatch
        })
    }
}
impl FormatReader<FontSet> for OberonCodec {
    fn read(&self, c: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
        read(c, &self.read_options)
    }
}
impl FormatWriter<FontSet> for OberonCodec {
    fn write(&self, s: &FontSet, w: &WriteOptions) -> Result<Written> {
        write(s, w, &self.write_options)
    }
}
fn word(b: &[u8], at: usize) -> Result<u16> {
    u16_at(b, at, ByteOrder::Little)
}
fn signed(b: &[u8], at: usize) -> Result<i64> {
    Ok(i64::from(word(b, at)? as i16))
}
/// Read the complete native run table, all boxes and bottom-up LSB-first rasters.
pub fn read(c: &mut ReadContext<'_>, _: &OberonReadOptions) -> Result<(FontSet, Report)> {
    let mut budget = Budget::new(c.retained_input_bytes(), c.limits())?;
    parse(c.bytes(), c.limits(), &mut budget)
}
fn parse(b: &[u8], l: &ResourceLimits, budget: &mut Budget) -> Result<(FontSet, Report)> {
    slice(b, 0, 16)?;
    if b[0] != 0xdb || b[1] != 0 {
        return Err(invalid(
            "not an Oberon bitmap resource (abstraction must be zero)",
        ));
    }
    check(b.len(), l.max_table_bytes, "Oberon resource bytes")?;
    check(1, l.max_resources, "Oberon strike")?;
    let nr = usize::from(word(b, 14)?);
    if nr > 16 {
        return Err(invalid("Oberon has at most sixteen character runs"));
    }
    let table_end = 16 + nr * 4;
    slice(b, 16, nr * 4)?;
    let mut codes = Vec::new();
    budget.add(256 * 4)?;
    let mut previous = 0usize;
    for i in 0..nr {
        let begin = usize::from(word(b, 16 + 4 * i)?);
        let end = usize::from(word(b, 18 + 4 * i)?);
        if begin >= end || begin < previous || end > 256 {
            return Err(invalid("invalid or overlapping Oberon character run"));
        }
        codes.extend(begin as u32..end as u32);
        previous = end;
    }
    check(codes.len(), l.max_glyphs, "Oberon glyph count")?;
    budget.items::<Font>(1)?;
    budget.items::<Glyph>(codes.len())?;
    budget.add(codes.len().saturating_mul(128).saturating_add(1024))?;
    slice(b, table_end, codes.len() * 10)?;
    let xmin = signed(b, 6)?;
    let xmax = signed(b, 8)?;
    let ymin = signed(b, 10)?;
    let ymax = signed(b, 12)?;
    if xmax < xmin || ymax < ymin {
        return Err(invalid("inverted Oberon font extrema"));
    }
    let mut f = Font::default();
    f.source_format = Some("oberon".into());
    f.charset.name = Some("oberon-native".into());
    f.metrics.bounds = Some(Bounds::new(
        (xmax - xmin) as u32,
        (ymax - ymin) as u32,
        xmin,
        ymin,
    ));
    f.metrics.line_height = Some(i64::from(word(b, 4)?));
    f.properties
        .push(Property::new("oberon.family", i64::from(b[2])));
    f.properties
        .push(Property::new("oberon.variant", i64::from(b[3])));
    let mut at = table_end + codes.len() * 10;
    for (i, code) in codes.into_iter().enumerate() {
        let p = table_end + 10 * i;
        let w = signed(b, p + 6)?;
        let h = signed(b, p + 8)?;
        if w < 0 || h < 0 {
            return Err(invalid("negative Oberon glyph dimensions"));
        }
        check(
            w as usize * h as usize,
            l.max_pixels_per_glyph,
            "Oberon glyph pixels",
        )?;
        let stride = (w as usize).div_ceil(8);
        let n = stride * h as usize;
        let native = slice(b, at, n)?;
        at += n;
        budget.add(n.saturating_mul(2))?;
        let mut raster = vec![0; n];
        for y in 0..h as usize {
            for x in 0..stride {
                raster[y * stride + x] = native[(h as usize - y - 1) * stride + x].reverse_bits();
            }
        }
        let mut g = Glyph::new(Bitmap::<Mono>::from_raw(
            w as u32,
            h as u32,
            &raster,
            bitmap_limits(l),
        )?);
        g.metrics.advance = Some(Vector::new(signed(b, p)?, 0));
        g.metrics.bounds.x = signed(b, p + 2)?;
        g.metrics.bounds.y = signed(b, p + 4)?;
        g.codepoints.push(Codepoint::Legacy(code));
        f.add_glyph(g)?;
    }
    if at != b.len() {
        return Err(invalid("trailing bytes after Oberon bitmap data"));
    }
    let mut set = FontSet::from(f);
    set.source_format = Some("oberon".into());
    Ok((set, Report::default()))
}
/// Encode native words and raster bytes, then audit a same-limit reread.
pub fn write(s: &FontSet, w: &WriteOptions, o: &OberonWriteOptions) -> Result<Written> {
    let mut r = Report::default();
    let result = encode(s, w, o, &mut r);
    finish(result, r, "oberon", w.strict)
}
fn encode(
    s: &FontSet,
    w: &WriteOptions,
    o: &OberonWriteOptions,
    r: &mut Report,
) -> Result<OutputBundle> {
    let mut budget = source_budget(s, &o.limits)?;
    let converted = convert_gray(s, o.gray_threshold, &o.limits, &mut budget)?;
    if converted.fonts.len() != 1 {
        return Err(invalid("Oberon output requires one selected strike"));
    }
    let f = &converted.fonts[0];
    let mut map = BTreeMap::new();
    let (mut xmin, mut xmax, mut ymin, mut ymax) = (0i64, 0i64, 0i64, 0i64);
    for (id, g) in f.glyphs() {
        xmin = xmin.min(g.metrics.bounds.x);
        ymin = ymin.min(g.metrics.bounds.y);
        xmax = xmax.max(g.metrics.bounds.x + i64::from(g.bitmap.width()));
        ymax = ymax.max(g.metrics.bounds.y + i64::from(g.bitmap.height()));
        for &cp in &g.codepoints {
            if cp.value() > 255 || matches!(cp, Codepoint::Unicode(_)) && !o.unicode_as_legacy {
                budget.loss(
                    r,
                    "oberon",
                    "write.mapping",
                    Some(0),
                    Some(id.index()),
                    format!("Mapping {cp:?} cannot be represented in the native byte domain"),
                )?;
                continue;
            }
            budget.add(64)?;
            if map.insert(cp.value(), g).is_some() {
                budget.loss(
                    r,
                    "oberon",
                    "write.duplicate",
                    Some(0),
                    Some(id.index()),
                    format!("Last glyph wins for {cp:?}"),
                )?;
            }
        }
    }
    check(map.len(), o.limits.max_glyphs, "Oberon glyph count")?;
    let mut runs: Vec<(u16, u16)> = Vec::new();
    for &code in map.keys() {
        if let Some(last) = runs.last_mut()
            && u32::from(last.1) == code
        {
            last.1 += 1;
        } else {
            runs.push((code as u16, code as u16 + 1));
        }
    }
    if runs.len() > 16 {
        return Err(invalid(
            "Oberon source needs more than sixteen character runs",
        ));
    }
    if let Some(bounds) = f.metrics.bounds {
        xmin = bounds.x;
        xmax = xmin + i64::from(bounds.width);
        ymin = bounds.y;
        ymax = ymin + i64::from(bounds.height);
    }
    let family = o
        .family
        .map(i64::from)
        .unwrap_or(int(&f.properties, "oberon.family", 0)?);
    let variant = o
        .variant
        .map(i64::from)
        .unwrap_or(int(&f.properties, "oberon.variant", 32)?);
    let family = u8::try_from(family).map_err(|_| invalid("Oberon family exceeds byte"))?;
    let variant = u8::try_from(variant).map_err(|_| invalid("Oberon variant exceeds byte"))?;
    let height = f.metrics.line_height.unwrap_or(ymax - ymin);
    let height = u16::try_from(height).map_err(|_| invalid("Oberon line height exceeds word"))?;
    let n = map
        .values()
        .try_fold(16 + 4 * runs.len() + 10 * map.len(), |n, g| {
            n.checked_add(g.bitmap.width().div_ceil(8) as usize * g.bitmap.height() as usize)
                .ok_or_else(|| invalid("Oberon output size overflow"))
        })?;
    check(
        n,
        o.max_output_bytes
            .min(o.limits.max_input_bytes)
            .min(o.limits.max_table_bytes),
        "Oberon output bytes",
    )?;
    budget.add(n.saturating_mul(2))?;
    let mut out = vec![0; n];
    out[..4].copy_from_slice(&[0xdb, 0, family, variant]);
    put16(&mut out, 4, height, ByteOrder::Little);
    for (at, v) in [(6, xmin), (8, xmax), (10, ymin), (12, ymax)] {
        put16(
            &mut out,
            at,
            i16::try_from(v).map_err(|_| invalid("Oberon font extremum exceeds signed word"))?
                as u16,
            ByteOrder::Little,
        );
    }
    put16(&mut out, 14, runs.len() as u16, ByteOrder::Little);
    for (i, &(begin, end)) in runs.iter().enumerate() {
        put16(&mut out, 16 + i * 4, begin, ByteOrder::Little);
        put16(&mut out, 18 + i * 4, end, ByteOrder::Little);
    }
    let boxes = 16 + runs.len() * 4;
    let mut at = boxes + map.len() * 10;
    for (i, g) in map.values().enumerate() {
        let bm = mono(g)?;
        let advance = g.metrics.advance.map_or(i64::from(bm.width()), |v| v.x);
        for (j, v) in [
            advance,
            g.metrics.bounds.x,
            g.metrics.bounds.y,
            i64::from(bm.width()),
            i64::from(bm.height()),
        ]
        .into_iter()
        .enumerate()
        {
            put16(
                &mut out,
                boxes + i * 10 + j * 2,
                i16::try_from(v).map_err(|_| invalid("Oberon glyph field exceeds signed word"))?
                    as u16,
                ByteOrder::Little,
            );
        }
        for y in (0..bm.height() as usize).rev() {
            for &v in &bm.as_bytes()[y * bm.stride()..(y + 1) * bm.stride()] {
                out[at] = v.reverse_bits();
                at += 1;
            }
        }
    }
    let mut rb = Budget::new(out.len(), &o.limits)?;
    let (back, rr) = parse(&out, &o.limits, &mut rb)?;
    r.extend(rr);
    budget.add(rb.used)?;
    audit(s, &back, "oberon", r, &mut budget)?;
    Ok(OutputBundle::single(w.primary_name.clone(), out))
}
