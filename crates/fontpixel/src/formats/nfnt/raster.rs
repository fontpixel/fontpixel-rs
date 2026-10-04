use super::NfntReadOptions;
use super::support::*;
use crate::bitmap::{Bounds, Gray8};
use crate::model::{
    Codepoint, Font, FontCharset, FontMetrics, Glyph, GlyphBitmap, Property, PropertyValue,
    Rational, Spacing, Vector,
};
use crate::{Bitmap, ResourceLimits, Result};

pub(super) fn plausible(b: &[u8]) -> bool {
    if b.len() < 26 {
        return false;
    }
    let Ok(kind) = u16be(b, 0) else { return false };
    kind & 0x1000 != 0 && tables_plausible(b).unwrap_or(false)
}
/// Table validation for detection without allocating bitmaps or model records.
/// A resource type supplies identity even when reserved fontType bits vary.
pub(super) fn tables_plausible(b: &[u8]) -> Result<bool> {
    let kind = u16be(b, 0)?;
    let first = usize::from(u16be(b, 2)?);
    let last = usize::from(u16be(b, 4)?);
    if first > last || last > 255 {
        return Ok(false);
    }
    let count = last - first + 2;
    let height = usize::from(u16be(b, 14)?);
    let row = usize::from(u16be(b, 24)?) * 2;
    let Some(loc) = height.checked_mul(row).and_then(|v| v.checked_add(26)) else {
        return Ok(false);
    };
    let ndes = u16be(b, 10)? as i16;
    let high = if ndes > 0 { ndes as usize } else { 0 };
    let Some(ow) = ((high << 16) | usize::from(u16be(b, 16)?))
        .checked_mul(2)
        .and_then(|v| v.checked_add(16))
    else {
        return Ok(false);
    };
    let Some(loc_end) = loc.checked_add((count + 1) * 2) else {
        return Ok(false);
    };
    if ow < loc_end {
        return Ok(false);
    }
    slice(b, loc, (count + 1) * 2)?;
    let optional = count * 2 * (usize::from(kind & 1 != 0) + usize::from(kind & 2 != 0));
    slice(b, ow, (count + 1) * 2 + optional)?;
    if u16be(b, ow + count * 2)? != 0xffff {
        return Ok(false);
    }
    let depth = 1usize << ((kind >> 2) & 3);
    let mut previous = usize::from(u16be(b, loc)?);
    let mut defined = false;
    for i in 0..count {
        let next = usize::from(u16be(b, loc + (i + 1) * 2)?);
        if next < previous || next > row * 8 / depth {
            return Ok(false);
        }
        let present = u16be(b, ow + i * 2)? != 0xffff;
        if !present && next != previous {
            return Ok(false);
        }
        defined |= present;
        previous = next;
    }
    Ok(defined)
}
pub(super) fn parse(
    b: &[u8],
    o: &NfntReadOptions,
    l: &ResourceLimits,
    budget: &mut Budget,
) -> Result<Font> {
    parse_with_palette(b, o, l, budget, None)
}
pub(super) fn parse_with_palette(
    b: &[u8],
    o: &NfntReadOptions,
    l: &ResourceLimits,
    budget: &mut Budget,
    palette: Option<&super::palette::Palette>,
) -> Result<Font> {
    check(b.len(), l.max_resource_bytes, "NFNT resource bytes")?;
    check(b.len(), l.max_table_bytes, "NFNT raster bytes")?;
    let kind = u16be(b, 0)?;
    if kind & 0x0100 != 0 || (kind & 0x0280 != 0 && palette.is_none()) {
        return Err(invalid(
            "synthetic/Lisa-compressed fonts or bare color fonts without resource context are unsupported",
        ));
    }
    let depth = 1usize << ((kind >> 2) & 3);
    let mono = depth == 1 && palette.is_none();
    let first = usize::from(u16be(b, 2)?);
    let last = usize::from(u16be(b, 4)?);
    if first > last || last > 255 {
        return Err(invalid("NFNT character range is not a byte range"));
    }
    let count = last - first + 2;
    let h = usize::from(u16be(b, 14)?);
    let row = usize::from(u16be(b, 24)?) * 2;
    let bitmap_len = h
        .checked_mul(row)
        .ok_or_else(|| invalid("strike size overflow"))?;
    check(bitmap_len, l.max_table_bytes, "NFNT strike table")?;
    let strike = slice(b, 26, bitmap_len)?;
    let loc = 26 + bitmap_len;
    let ndes = u16be(b, 10)? as i16;
    let high = if ndes > 0 { ndes as usize } else { 0 };
    let ow = (high << 16 | usize::from(u16be(b, 16)?))
        .checked_mul(2)
        .and_then(|v| v.checked_add(16))
        .ok_or_else(|| invalid("width table offset overflow"))?;
    let loc_end = loc + (count + 1) * 2;
    if ow < loc_end {
        return Err(invalid("width table overlaps location table"));
    }
    slice(b, loc, (count + 1) * 2)?;
    slice(b, ow, (count + 1) * 2)?;
    if u16be(b, ow + count * 2)? != 0xffff {
        return Err(invalid("missing width table terminator"));
    }
    let optional = count * 2 * (usize::from(kind & 2 != 0) + usize::from(kind & 1 != 0));
    slice(b, ow + (count + 1) * 2, optional)?;
    let kern = i64::from(u16be(b, 8)? as i16);
    let descent = i64::from(u16be(b, 20)?);
    let ascent = i64::from(u16be(b, 18)?);
    if let Some(name) = &o.charset {
        check(name.len(), l.max_table_bytes, "NFNT charset name")?;
        budget.add(name.len())?;
    }
    budget.add(2048 + b.len())?;
    let mut f = Font::default();
    f.charset = FontCharset {
        name: Some(o.charset.clone().unwrap_or_else(|| "unknown".into())),
        ..FontCharset::default()
    };
    f.source_format = Some("nfnt".into());
    f.properties.push(Property::new(
        "nfnt.source",
        PropertyValue::Bytes(b.to_vec()),
    ));
    let px = u32::try_from(ascent + descent).map_err(|_| invalid("pixel size overflow"))?;
    f.metrics = FontMetrics {
        pixel_size: Some(px),
        point_size: Some(Rational::integer(i64::from(px))),
        resolution: Some(Vector::new(72, 72)),
        ascent: Some(ascent),
        descent: Some(descent),
        bounds: Some(Bounds::new(
            u32::from(u16be(b, 12)?),
            h as u32,
            kern,
            -descent,
        )),
        line_gap: Some(i64::from(u16be(b, 22)?)),
        spacing: Some(if kind & 0x2000 != 0 {
            Spacing::Monospaced
        } else {
            Spacing::Proportional
        }),
        ..FontMetrics::default()
    };
    let mut previous = usize::from(u16be(b, loc)?);
    for i in 0..count {
        let next = usize::from(u16be(b, loc + (i + 1) * 2)?);
        if next < previous || next > row * 8 / depth {
            return Err(invalid("invalid bitmap location table"));
        }
        let width = next - previous;
        let wo = u16be(b, ow + i * 2)?;
        if wo == 0xffff {
            if width != 0 {
                return Err(invalid("absent glyph has bitmap data"));
            }
            previous = next;
            continue;
        }
        let pixels = width
            .checked_mul(h)
            .ok_or_else(|| invalid("glyph area overflow"))?;
        check(pixels, l.max_pixels_per_glyph, "NFNT glyph pixels")?;
        budget.glyph(
            if mono {
                width.div_ceil(8) * h
            } else {
                width * h
            },
            l,
        )?;
        let stride = if mono { width.div_ceil(8) } else { width };
        let scratch_len = stride * h;
        budget.add(scratch_len)?;
        let mut samples = vec![0u8; scratch_len];
        if mono {
            let shift = previous % 8;
            for y in 0..h {
                let src = &strike[y * row..(y + 1) * row];
                for x in 0..stride {
                    let at = previous / 8 + x;
                    samples[y * stride + x] = src[at] << shift
                        | if shift == 0 {
                            0
                        } else {
                            src.get(at + 1).copied().unwrap_or(0) >> (8 - shift)
                        };
                }
            }
        } else if depth == 8 && palette.is_none() {
            for y in 0..h {
                samples[y * stride..(y + 1) * stride]
                    .copy_from_slice(&strike[y * row + previous..y * row + previous + width]);
            }
        } else {
            let mask = (1u16 << depth) - 1;
            for y in 0..h {
                for x in 0..width {
                    let bit = (previous + x) * depth;
                    let value =
                        (u16::from(strike[y * row + bit / 8]) >> (8 - depth - bit % 8)) & mask;
                    samples[y * stride + x] = palette.map_or((value * 255 / mask) as u8, |p| {
                        p.samples[usize::from(value)]
                    });
                }
            }
        }
        let bitmap: GlyphBitmap = if mono {
            Bitmap::<crate::bitmap::Mono>::from_raw(
                width as u32,
                h as u32,
                &samples,
                bitmap_limits(l),
            )?
            .into()
        } else {
            Bitmap::<Gray8>::from_raw(width as u32, h as u32, &samples, bitmap_limits(l))?.into()
        };
        drop(samples);
        budget.used -= scratch_len;
        let mut g = Glyph::new(bitmap);
        g.metrics.bounds.x = kern + i64::from(wo >> 8);
        g.metrics.bounds.y = -descent;
        g.metrics.advance = Some(Vector::new(i64::from(wo & 255), 0));
        if i < count - 1 {
            g.codepoints.push(Codepoint::Legacy((first + i) as u32));
        }
        let id = f.add_glyph(g)?;
        if i == count - 1 {
            f.metrics.default_glyph = Some(id);
        }
        previous = next;
    }
    if f.is_empty() {
        return Err(invalid("font contains no defined glyphs"));
    }
    Ok(f)
}
