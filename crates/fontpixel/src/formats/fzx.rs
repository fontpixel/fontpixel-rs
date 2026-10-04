//! FZX v1 proportional fonts with entry-relative offsets and explicit tracking.
use super::legacy_common::*;
use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
use crate::bitmap::{Bitmap, Bounds, Mono};
use crate::model::{Codepoint, Font, FontSet, Glyph, Property, PropertyValue, Spacing, Vector};
use crate::resources::{OutputBundle, ReadContext, Written};
use crate::{Report, ResourceLimits, Result};
/// FZX reader options; the standard has a single wire layout.
#[derive(Debug, Clone, Default)]
pub struct FzxReadOptions {}
/// Bounded FZX writing choices.
#[derive(Debug, Clone)]
pub struct FzxWriteOptions {
    /// Explicit line height; otherwise use model line height or pixel size.
    pub height: Option<u8>,
    /// Explicit tracking; otherwise retain source value or infer a common gap.
    pub tracking: Option<u8>,
    /// Permit Unicode numbers as legacy byte positions.
    pub unicode_as_legacy: bool,
    /// Explicit grayscale reduction threshold.
    pub gray_threshold: Option<u8>,
    /// Shared resource budget.
    pub limits: ResourceLimits,
    /// Output cap, also bounded by reader input limits.
    pub max_output_bytes: usize,
}
impl Default for FzxWriteOptions {
    fn default() -> Self {
        Self {
            height: None,
            tracking: None,
            unicode_as_legacy: false,
            gray_threshold: None,
            limits: ResourceLimits::default(),
            max_output_bytes: 64 * 1024 * 1024,
        }
    }
}
/// Registry codec for FZX.
#[derive(Debug, Clone, Default)]
pub struct FzxCodec {
    /// Typed input choices.
    pub read_options: FzxReadOptions,
    /// Typed output choices.
    pub write_options: FzxWriteOptions,
}
impl FormatDetector for FzxCodec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(detect(c.bytes()))
    }
}
impl FormatReader<FontSet> for FzxCodec {
    fn read(&self, c: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
        read(c, &self.read_options)
    }
}
impl FormatWriter<FontSet> for FzxCodec {
    fn write(&self, s: &FontSet, o: &WriteOptions) -> Result<Written> {
        write(s, o, &self.write_options)
    }
}
fn layout(b: &[u8]) -> Result<usize> {
    slice(b, 0, 3)?;
    if b[0] == 0 || b[2] < 32 {
        return Err(invalid("invalid FZX height or character range"));
    }
    let n = usize::from(b[2]) - 31;
    let end = 3 + n * 3;
    slice(b, 0, end + 2)?;
    let final_offset = end + usize::from(u16_at(b, end, ByteOrder::Little)?);
    if final_offset != b.len() {
        return Err(invalid("FZX terminal offset disagrees with file length"));
    }
    let mut prev = end + 2;
    for i in 0..n {
        let at = 3 + i * 3;
        let offset = at + usize::from(u16_at(b, at, ByteOrder::Little)? & 0x3fff);
        if offset < prev || offset > final_offset {
            return Err(invalid("invalid FZX glyph offset"));
        }
        if i == 0 && offset != end + 2 {
            return Err(invalid("FZX first glyph does not follow table"));
        }
        if i > 0 {
            let width = (b[3 + (i - 1) * 3 + 2] & 15) + 1;
            if !(offset - prev).is_multiple_of(usize::from(width).div_ceil(8)) {
                return Err(invalid("FZX partial raster row"));
            }
        }
        prev = offset;
    }
    let width = (b[3 + (n - 1) * 3 + 2] & 15) + 1;
    if !(final_offset - prev).is_multiple_of(usize::from(width).div_ceil(8)) {
        return Err(invalid("FZX partial final raster row"));
    }
    Ok(n)
}
/// FZX has no magic; complete valid structure is weak detection evidence.
pub fn detect(b: &[u8]) -> Confidence {
    if layout(b).is_ok() {
        Confidence::Possible
    } else {
        Confidence::NoMatch
    }
}
/// Read one proportional font with legacy code positions.
pub fn read(c: &mut ReadContext<'_>, _: &FzxReadOptions) -> Result<(FontSet, Report)> {
    let mut budget = Budget::new(c.retained_input_bytes(), c.limits())?;
    parse(c.bytes(), c.limits(), &mut budget)
}
fn parse(b: &[u8], l: &ResourceLimits, budget: &mut Budget) -> Result<(FontSet, Report)> {
    let n = layout(b)?;
    check(n, l.max_glyphs, "FZX glyphs")?;
    check(b.len(), l.max_table_bytes, "FZX source bytes")?;
    budget.items::<Font>(1)?;
    budget.items::<Option<Glyph>>(n)?;
    budget.add(b.len().saturating_add(n * 256).saturating_add(1024))?;
    let height = i64::from(b[0]);
    let tracking = i64::from(b[1]);
    let mut f = Font::default();
    f.source_format = Some("fzx".into());
    f.properties.push(Property::new("fzx.tracking", tracking));
    f.properties.push(Property::new(
        "fzx.source",
        PropertyValue::Bytes(b.to_vec()),
    ));
    let mut report = Report::default();
    let mut min_y = height;
    let mut min_x = 0;
    let mut max_x = 0;
    for i in 0..n {
        let at = 3 + i * 3;
        let entry = u16_at(b, at, ByteOrder::Little)?;
        let start = at + usize::from(entry & 0x3fff);
        let end = if i + 1 == n {
            b.len()
        } else {
            at + 3 + usize::from(u16_at(b, at + 3, ByteOrder::Little)? & 0x3fff)
        };
        let w = u32::from((b[at + 2] & 15) + 1);
        let stride = (w as usize).div_ceil(8);
        let h = u32::try_from((end - start) / stride)
            .map_err(|_| invalid("FZX raster height overflow"))?;
        check(
            w as usize * h as usize,
            l.max_pixels_per_glyph,
            "FZX glyph pixels",
        )?;
        budget.add(end - start)?;
        let bitmap = Bitmap::<Mono>::from_raw(w, h, &b[start..end], bitmap_limits(l))?;
        if bitmap.as_bytes() != &b[start..end] {
            budget.loss(
                &mut report,
                "fzx",
                "read.padding",
                Some(0),
                Some(i),
                "Nonzero raster padding retained only in source bytes",
            )?;
        }
        let x = -i64::from(entry >> 14);
        let y = height - i64::from(b[at + 2] >> 4) - i64::from(h);
        let mut g = Glyph::new(bitmap);
        g.codepoints.push(Codepoint::Legacy(32 + i as u32));
        g.metrics.bounds = Bounds::new(w, h, x, y);
        g.metrics.advance = Some(Vector::new(i64::from(w) + x + tracking, 0));
        g.metrics.left_bearing = Some(x);
        g.metrics.right_bearing = Some(tracking);
        min_y = min_y.min(y);
        min_x = min_x.min(x);
        max_x = max_x.max(x + i64::from(w));
        f.add_glyph(g)?;
    }
    f.metrics.line_height = Some(height);
    f.metrics.pixel_size = Some(height as u32);
    f.metrics.ascent = Some(height);
    f.metrics.descent = Some((-min_y).max(0));
    f.metrics.bounds = Some(Bounds::new(
        (max_x - min_x) as u32,
        (height - min_y) as u32,
        min_x,
        min_y,
    ));
    f.metrics.spacing = Some(Spacing::Proportional);
    f.metrics.space_width = f
        .glyph_for_codepoint(Codepoint::Legacy(32))
        .and_then(|g| g.metrics.advance.map(|a| a.x));
    let mut set = FontSet::from(f);
    set.source_format = Some("fzx".into());
    Ok((set, report))
}
// FZX has no width-zero descriptor; preserve a blank's advance using width/kern.
fn wire_geometry(g: &Glyph, tracking: u8, default_advance: Option<Vector>) -> Result<(u32, i64)> {
    let b = g.metrics.bounds;
    if b.width != 0 {
        let width = i64::from(b.width) + b.x.max(0);
        if !(1..=16).contains(&width) || b.x < -3 {
            return Err(invalid(
                "FZX requires normalized width 1..16 and left bearing at least -3",
            ));
        }
        return Ok((width as u32, b.x.min(0)));
    }
    let advance = g.metrics.advance.or(default_advance).map_or(0, |v| v.x) - i64::from(tracking);
    let width = advance - b.x;
    if (1..=16).contains(&width) && (-3..=0).contains(&b.x) {
        return Ok((width as u32, b.x));
    }
    if !(-2..=16).contains(&advance) {
        return Err(invalid(
            "FZX blank advance cannot be represented by width/kern",
        ));
    }
    let width = advance.max(1);
    Ok((width as u32, advance - width))
}

/// Write one FZX font and audit the complete decoded target model.
pub fn write(s: &FontSet, o: &WriteOptions, v: &FzxWriteOptions) -> Result<Written> {
    let mut report = Report::default();
    let result = (|| {
        let mut budget = source_budget(s, &v.limits)?;
        let converted = convert_gray(s, v.gray_threshold, &v.limits, &mut budget)?;
        let f = converted.single()?;
        let mut slots = select(f, 255, v.unicode_as_legacy, "fzx", &mut report, &mut budget)?;
        for code in 0..32 {
            if slots.remove(&code).is_some() {
                budget.loss(
                    &mut report,
                    "fzx",
                    "write.unrepresentable_mapping",
                    Some(0),
                    None,
                    format!("Code {code} is below 32"),
                )?;
            }
        }
        let last = *slots
            .keys()
            .next_back()
            .ok_or_else(|| invalid("FZX has no mappings in 32..255"))?;
        let height = v
            .height
            .map(i64::from)
            .or(f.metrics.line_height)
            .or(f.metrics.pixel_size.map(i64::from))
            .unwrap_or_else(|| {
                slots
                    .values()
                    .map(|g| g.metrics.bounds.y + i64::from(g.bitmap.height()))
                    .max()
                    .unwrap_or(1)
            });
        let height = u8::try_from(height)
            .ok()
            .filter(|v| *v != 0)
            .ok_or_else(|| invalid("FZX height must be 1..255"))?;
        let tracking = if let Some(t) = v.tracking {
            t
        } else if f.properties.get("fzx.tracking").is_some() {
            u8::try_from(int(&f.properties, "fzx.tracking", 0)?)
                .map_err(|_| invalid("FZX tracking exceeds u8"))?
        } else {
            let mut value = None;
            for g in slots.values() {
                if g.bitmap.width() == 0 {
                    continue;
                }
                if let Some(a) = g.metrics.advance.or(f.metrics.glyph_defaults.advance) {
                    let t = a.x - g.metrics.bounds.x - i64::from(g.bitmap.width());
                    if value.is_some_and(|v| v != t) {
                        return Err(invalid(
                            "FZX needs common tracking; supply an explicit tracking override",
                        ));
                    }
                    value = Some(t);
                }
            }
            u8::try_from(value.unwrap_or(0))
                .map_err(|_| invalid("FZX inferred tracking exceeds u8"))?
        };
        let n = (last - 31) as usize;
        check(n, v.limits.max_glyphs, "FZX output glyphs")?;
        let table = 3 + n * 3 + 2;
        let mut size = table;
        for g in slots.values() {
            let bb = g.metrics.bounds;
            let (width, _) = wire_geometry(g, tracking, f.metrics.glyph_defaults.advance)?;
            let shift = i64::from(height) - bb.y - i64::from(bb.height);
            if shift < 0 {
                return Err(invalid("FZX glyph starts above its cell"));
            }
            let pad = usize::try_from((shift - 15).max(0))
                .map_err(|_| invalid("FZX top padding overflow"))?;
            let rows = (bb.height as usize)
                .checked_add(pad)
                .ok_or_else(|| invalid("FZX height overflow"))?;
            check(
                rows.saturating_mul(width as usize),
                v.limits.max_pixels_per_glyph,
                "FZX output pixels",
            )?;
            size = size
                .checked_add(
                    rows.checked_mul((width as usize).div_ceil(8))
                        .ok_or_else(|| invalid("FZX raster size overflow"))?,
                )
                .ok_or_else(|| invalid("FZX size overflow"))?;
        }
        check(
            size,
            v.max_output_bytes
                .min(v.limits.max_input_bytes)
                .min(v.limits.max_table_bytes),
            "FZX output bytes",
        )?;
        budget.add(size)?;
        let mut out = vec![0; size];
        out[..3].copy_from_slice(&[height, tracking, last as u8]);
        let mut off = table;
        for i in 0..n {
            let at = 3 + i * 3;
            let delta = off - at;
            if delta > 0x3fff {
                return Err(invalid("FZX relative offset exceeds 14 bits"));
            }
            let (kern, width, shift, pad, rows, left, bitmap) =
                if let Some(g) = slots.get(&(32 + i as u32)) {
                    let bb = g.metrics.bounds;
                    let shift = i64::from(height) - bb.y - i64::from(bb.height);
                    let (width, x) = wire_geometry(g, tracking, f.metrics.glyph_defaults.advance)?;
                    (
                        (-x) as u16,
                        width as u8,
                        shift.min(15) as u8,
                        (shift - 15).max(0) as usize,
                        bb.height as usize,
                        if bb.width == 0 {
                            0
                        } else {
                            (bb.x - x) as usize
                        },
                        Some(mono(g)?),
                    )
                } else {
                    (0, 1, 0, 0, 0, 0, None)
                };
            put16(&mut out, at, delta as u16 | (kern << 14), ByteOrder::Little);
            out[at + 2] = (shift << 4) | (width - 1);
            let stride = usize::from(width).div_ceil(8);
            off += pad * stride;
            if let Some(bitmap) = bitmap {
                let shift = left % 8;
                for y in 0..rows {
                    for (j, &value) in bitmap.as_bytes()
                        [y * bitmap.stride()..(y + 1) * bitmap.stride()]
                        .iter()
                        .enumerate()
                    {
                        let at = off + y * stride + left / 8 + j;
                        out[at] |= value >> shift;
                        if shift != 0 && left / 8 + j + 1 < stride {
                            out[at + 1] |= value << (8 - shift);
                        }
                    }
                }
            }
            off += rows * stride;
        }
        let end = 3 + n * 3;
        let terminal =
            u16::try_from(size - end).map_err(|_| invalid("FZX final offset exceeds u16"))?;
        put16(&mut out, end, terminal, ByteOrder::Little);
        let (target, rr) = parse(&out, &v.limits, &mut budget)?;
        report.extend(rr);
        audit(s, &target, "fzx", &mut report, &mut budget)?;
        Ok(OutputBundle::single(o.primary_name.clone(), out))
    })();
    finish(result, report, "fzx", o.strict)
}
