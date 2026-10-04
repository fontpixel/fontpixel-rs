//! Native RB11/RB12 fonts, including the RB12 4-bit coverage layout.
use super::device_common::*;
use super::{Confidence, WriteOptions};
use crate::bitmap::{Bitmap, Bounds, Gray8};
use crate::model::{Codepoint, Font, FontSet, Glyph, GlyphBitmap, Property, Vector};
use crate::resources::{OutputBundle, ReadContext, Written};
use crate::{Report, ResourceLimits, Result};
use std::borrow::Cow;
use std::collections::BTreeMap;
/// Native Rockbox font generations.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RockboxFlavor {
    /// Row-packed little-endian words, with embedded name fields.
    Rb11,
    /// Current vertically packed mono or nibble-packed grayscale format.
    #[default]
    Rb12,
}
/// Native raster encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RockboxDepth {
    /// One-bit coverage, depth field zero.
    Mono,
    /// Four-bit coverage, depth field one, RB12 only.
    Gray4,
}
/// Both native generations are recognized from their signatures.
#[derive(Debug, Clone, Default)]
pub struct RockboxReadOptions {}
/// Bounded native output choices.
#[derive(Debug, Clone)]
pub struct RockboxWriteOptions {
    /// Native file generation.
    pub flavor: RockboxFlavor,
    /// Override native depth; otherwise retain the source depth or choose mono.
    pub depth: Option<RockboxDepth>,
    /// Explicit threshold when reducing grayscale to mono.
    pub gray_threshold: Option<u8>,
    /// Cumulative source, conversion, output and readback limits.
    pub limits: ResourceLimits,
    /// Additional primary output cap.
    pub max_output_bytes: usize,
}
impl Default for RockboxWriteOptions {
    fn default() -> Self {
        Self {
            flavor: RockboxFlavor::Rb12,
            depth: None,
            gray_threshold: None,
            limits: ResourceLimits::default(),
            max_output_bytes: 64 * 1024 * 1024,
        }
    }
}
codec!(RockboxCodec, RockboxReadOptions, RockboxWriteOptions);
// Rockbox's loader and official converters use this threshold, not 0x10000.
const LONG_OFFSETS: usize = 0xffdb;
/// Detect the native generation signature.
pub fn detect(b: &[u8]) -> Confidence {
    if b.starts_with(b"RB11") || b.starts_with(b"RB12") {
        Confidence::Certain
    } else {
        Confidence::NoMatch
    }
}
/// Read native offsets, aliases, metrics and packed coverage.
pub fn read(c: &mut ReadContext<'_>, _: &RockboxReadOptions) -> Result<(FontSet, Report)> {
    let mut budget = Budget::new(c.retained_input_bytes(), c.limits())?;
    parse(c.bytes(), c.limits(), &mut budget)
}
fn name(b: &[u8]) -> Result<String> {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    std::str::from_utf8(&b[..end])
        .map(str::to_owned)
        .map_err(|_| invalid("Rockbox name is not UTF-8"))
}
fn parse(b: &[u8], l: &ResourceLimits, budget: &mut Budget) -> Result<(FontSet, Report)> {
    let old = b.starts_with(b"RB11");
    if !old && !b.starts_with(b"RB12") {
        return Err(invalid("invalid Rockbox signature"));
    }
    let head = if old { 324 } else { 4 };
    slice(b, 0, head + 32)?;
    check(b.len(), l.max_table_bytes, "Rockbox table bytes")?;
    let max_w = u32::from(r16(b, head)?);
    let h = u32::from(r16(b, head + 2)?);
    let ascent = i64::from(r16(b, head + 4)?);
    let depth = r16(b, head + 6)?;
    if depth > 1 || (old && depth != 0) || ascent > i64::from(h) || max_w > 32767 || h > 32767 {
        return Err(invalid("unsupported Rockbox depth or invalid ascent"));
    }
    let first = r32(b, head + 8)?;
    let default = r32(b, head + 12)?;
    let n = r32(b, head + 16)? as usize;
    let bits = r32(b, head + 20)? as usize;
    let offsets = r32(b, head + 24)? as usize;
    let widths = r32(b, head + 28)? as usize;
    if n == 0
        || first.checked_add(n as u32 - 1).is_none_or(|c| c > 0x10ffff)
        || (offsets != 0 && offsets != n)
        || (widths != 0 && widths != n)
    {
        return Err(invalid("invalid Rockbox character range/table counts"));
    }
    allocation(n, l, budget)?;
    let raster_start = head + 32;
    let raster_bytes = bits
        .checked_mul(if old { 2 } else { 1 })
        .ok_or_else(|| invalid("Rockbox raster overflow"))?;
    let packed = slice(b, raster_start, raster_bytes)?;
    let osize = if old || bits >= LONG_OFFSETS { 4 } else { 2 };
    let table_start = (raster_start + raster_bytes).next_multiple_of(osize);
    let width_start = table_start
        .checked_add(offsets * osize)
        .ok_or_else(|| invalid("Rockbox table overflow"))?;
    if b.len() != width_start + widths {
        return Err(invalid("Rockbox length disagrees with tables"));
    }
    let mut f = Font::default();
    let mut report = Report::default();
    if old {
        f.names.family = Some(name(&b[4..68])?);
        f.names.copyright = Some(name(&b[68..324])?);
    }
    f.metrics.ascent = Some(ascent);
    f.metrics.descent = Some(i64::from(h) - ascent);
    f.metrics.line_height = Some(i64::from(h));
    f.metrics.pixel_size = Some(h);
    f.properties
        .push(Property::new("rockbox.depth", i64::from(depth)));
    f.properties
        .push(Property::new("rockbox.max_width", i64::from(max_w)));
    f.metrics.default_character = char::from_u32(default).map(|_| Codepoint::Unicode(default));
    let mut groups: BTreeMap<(usize, u32), Vec<Codepoint>> = BTreeMap::new();
    let fixed_bytes = if old {
        (max_w as usize).div_ceil(16) * 2 * h as usize
    } else if depth != 0 {
        (max_w as usize * h as usize).div_ceil(2)
    } else {
        max_w as usize * (h as usize).div_ceil(8)
    };
    // Older generators (including BitsNPicas 2.2.2) pack offsetless odd-byte
    // cells without the current runtime's word padding. nbits identifies that
    // layout exactly. Our writer always emits offsets to avoid this ambiguity.
    let fixed_stride = if old || raster_bytes == n.saturating_mul(fixed_bytes) {
        fixed_bytes
    } else {
        fixed_bytes.next_multiple_of(2)
    };
    for i in 0..n {
        let w = if widths == 0 {
            max_w
        } else {
            u32::from(b[width_start + i])
        };
        if w > max_w {
            return Err(invalid("Rockbox glyph exceeds maximum width"));
        }
        let off = if offsets == 0 {
            i.checked_mul(fixed_stride)
                .ok_or_else(|| invalid("Rockbox fixed offset overflow"))?
        } else if osize == 2 {
            usize::from(r16(b, table_start + i * 2)?)
        } else {
            (r32(b, table_start + i * 4)? as usize)
                .checked_mul(if old { 2 } else { 1 })
                .ok_or_else(|| invalid("Rockbox word offset overflow"))?
        };
        let cp = first + i as u32;
        if char::from_u32(cp).is_none() {
            budget.loss(
                &mut report,
                "rockbox",
                "read.non_scalar",
                Some(0),
                Some(i),
                "Non-scalar Unicode range member has no model mapping",
            )?;
            continue;
        }
        budget.add(128)?;
        groups
            .entry((off, w))
            .or_default()
            .push(Codepoint::Unicode(cp));
    }
    // Physical offset order is deterministic and retains aliases without duplicating pixels.
    for ((off, w), cps) in groups {
        let area = w as usize * h as usize;
        check(area, l.max_pixels_per_glyph, "Rockbox glyph pixels")?;
        let mut g = if depth != 0 {
            let data = slice(packed, off, area.div_ceil(2))?;
            budget.add(area.saturating_mul(2))?;
            let pixels: Vec<u8> = (0..area)
                .map(|i| 255 - ((data[i / 2] >> ((i % 2) * 4)) & 15) * 17)
                .collect();
            let mut g = Glyph::new(Bitmap::<Gray8>::from_raw(w, h, &pixels, bitmap_limits(l))?);
            g.metrics.bounds = Bounds::new(w, h, 0, ascent - i64::from(h));
            g.metrics.advance = Some(Vector::new(i64::from(w), 0));
            g.metrics.left_bearing = Some(0);
            g.metrics.right_bearing = Some(0);
            g
        } else {
            let mut data = raster(w, h, l, budget)?;
            let stride = (w as usize).div_ceil(8);
            let native_stride = (w as usize).div_ceil(16) * 2;
            slice(
                packed,
                off,
                if old {
                    native_stride * h as usize
                } else {
                    w as usize * (h as usize).div_ceil(8)
                },
            )?;
            for y in 0..h as usize {
                for x in 0..w as usize {
                    let ink = if old {
                        packed[off + y * native_stride + (x / 16) * 2 + usize::from(x % 16 < 8)]
                            & (0x80 >> (x % 8))
                            != 0
                    } else {
                        packed[off + (y / 8) * w as usize + x] & (1 << (y % 8)) != 0
                    };
                    if ink {
                        data[y * stride + x / 8] |= 0x80 >> (x % 8);
                    }
                }
            }
            glyph(w, h, 0, ascent - i64::from(h), i64::from(w), &data, l)?
        };
        g.codepoints = cps;
        f.add_glyph(g)?;
    }
    f.metrics.default_glyph = f.metrics.default_character.and_then(|c| f.glyph_id(c));
    Ok((set(f, "rockbox"), report))
}
fn text_field(dst: &mut [u8], value: Option<&str>) -> Result<()> {
    if let Some(s) = value {
        if s.len() > dst.len() || s.contains('\0') {
            return Err(invalid("Rockbox name exceeds native field"));
        }
        dst[..s.len()].copy_from_slice(s.as_bytes());
    }
    Ok(())
}
/// Encode a native file, then audit a bounded native readback.
pub fn write(s: &FontSet, o: &WriteOptions, v: &RockboxWriteOptions) -> Result<Written> {
    let mut report = Report::default();
    let result = (|| {
        let mut budget = source_budget(s, &v.limits)?;
        let depth = v
            .depth
            .unwrap_or(if int(&s.single()?.properties, "rockbox.depth", 0)? == 0 {
                RockboxDepth::Mono
            } else {
                RockboxDepth::Gray4
            });
        let old = v.flavor == RockboxFlavor::Rb11;
        if old && depth != RockboxDepth::Mono {
            return Err(invalid("RB11 cannot store grayscale"));
        }
        let converted = if depth == RockboxDepth::Mono {
            convert_gray(s, v.gray_threshold, &v.limits, &mut budget)?
        } else {
            Cow::Borrowed(s)
        };
        let f = converted.single()?;
        let map = select(f, 0x10ffff, true, false, &mut budget)?;
        let first = *map.keys().next().unwrap();
        let last = *map.keys().next_back().unwrap();
        let n = (last - first + 1) as usize;
        check(n, v.limits.max_glyphs, "Rockbox output range")?;
        let default = f
            .metrics
            .default_character
            .and_then(|c| {
                if let Codepoint::Unicode(c) = c {
                    Some(c)
                } else {
                    None
                }
            })
            .filter(|c| map.contains_key(c))
            .unwrap_or(first);
        let fallback = map[&default];
        let ascent = f.metrics.ascent.unwrap_or_else(|| {
            map.values()
                .map(|g| g.metrics.bounds.y + i64::from(g.bitmap.height()))
                .max()
                .unwrap_or(0)
        });
        let height = f
            .metrics
            .line_height
            .or(f.metrics.pixel_size.map(i64::from))
            .unwrap_or(ascent + f.metrics.descent.unwrap_or(0));
        let h = u16::try_from(height).map_err(|_| invalid("Rockbox cell height exceeds u16"))?;
        let a = u16::try_from(ascent).map_err(|_| invalid("Rockbox ascent exceeds u16"))?;
        if a > h || h > 32767 {
            return Err(invalid("Rockbox ascent exceeds height"));
        }
        // Source glyph identity deduplicates aliases in O(log n), including fallback gaps.
        let mut unique: BTreeMap<usize, (usize, u32, &Glyph)> = BTreeMap::new();
        let mut bits = 0usize;
        let mut max_width = 0;
        let mut uniform = None;
        let mut proportional = false;
        budget.items::<(usize, u32)>(n)?;
        let mut entries = Vec::with_capacity(n);
        for cp in first..=last {
            let g = map.get(&cp).copied().unwrap_or(fallback);
            let width = u16::try_from(
                g.metrics
                    .advance
                    .or(f.metrics.glyph_defaults.advance)
                    .map_or(
                        i64::from(g.bitmap.width()) + g.metrics.bounds.x.max(0),
                        |v| v.x,
                    ),
            )
            .map_err(|_| invalid("Rockbox advance exceeds u16"))?;
            let w = u32::from(width);
            max_width = max_width.max(width);
            if uniform.is_some_and(|prev| prev != width) {
                proportional = true;
            }
            uniform.get_or_insert(width);
            let key = std::ptr::from_ref(g) as usize;
            let entry = if let Some(&(off, w, _)) = unique.get(&key) {
                (off, w)
            } else {
                let off = bits;
                let size = if old {
                    (w as usize).div_ceil(16) * 2 * usize::from(h)
                } else if depth == RockboxDepth::Gray4 {
                    (w as usize * usize::from(h)).div_ceil(2)
                } else {
                    w as usize * usize::from(h).div_ceil(8)
                };
                bits = bits
                    .checked_add(size)
                    .ok_or_else(|| invalid("Rockbox raster size overflow"))?;
                budget.add(128)?;
                unique.insert(key, (off, w, g));
                (off, w)
            };
            entries.push(entry);
        }
        if max_width > 32767 || (proportional && max_width > 255) {
            return Err(invalid("Rockbox proportional width exceeds byte table"));
        }
        let head = if old { 324 } else { 4 };
        let start = head + 32;
        let osize = if old || bits >= LONG_OFFSETS { 4 } else { 2 };
        let table = (start + bits).next_multiple_of(osize);
        let mut b = output(
            table + n * osize + if proportional { n } else { 0 },
            v.max_output_bytes,
            &v.limits,
            &mut budget,
        )?;
        b[..4].copy_from_slice(if old { b"RB11" } else { b"RB12" });
        if old {
            text_field(&mut b[4..68], f.names.family.as_deref())?;
            text_field(&mut b[68..324], f.names.copyright.as_deref())?;
        }
        p16(&mut b, head, max_width);
        p16(&mut b, head + 2, h);
        p16(&mut b, head + 4, a);
        p16(&mut b, head + 6, u16::from(depth == RockboxDepth::Gray4));
        for (at, value) in [
            (8, first),
            (12, default),
            (16, n as u32),
            (
                20,
                u32::try_from(if old { bits / 2 } else { bits })
                    .map_err(|_| invalid("Rockbox raster exceeds u32"))?,
            ),
            (24, n as u32),
            (28, if proportional { n as u32 } else { 0 }),
        ] {
            p32(&mut b, head + at, value);
        }
        for (i, &(off, w)) in entries.iter().enumerate() {
            let off = if old { off / 2 } else { off };
            if osize == 2 {
                p16(&mut b, table + i * 2, off as u16);
            } else {
                p32(&mut b, table + i * 4, off as u32);
            }
            if proportional {
                b[table + n * osize + i] = w as u8;
            }
        }
        for (_, (off, w, g)) in unique {
            check(
                w as usize * usize::from(h),
                v.limits.max_pixels_per_glyph,
                "Rockbox output pixels",
            )?;
            if depth == RockboxDepth::Mono {
                let data = canvas(g, w, u32::from(h), ascent, &v.limits, &mut budget)?;
                let stride = (w as usize).div_ceil(8);
                for y in 0..usize::from(h) {
                    for x in 0..w as usize {
                        if data[y * stride + x / 8] & (0x80 >> (x % 8)) == 0 {
                            continue;
                        }
                        if old {
                            b[start
                                + off
                                + y * (w as usize).div_ceil(16) * 2
                                + (x / 16) * 2
                                + usize::from(x % 16 < 8)] |= 0x80 >> (x % 8);
                        } else {
                            b[start + off + (y / 8) * w as usize + x] |= 1 << (y % 8);
                        }
                    }
                }
            } else {
                let bb = g.metrics.bounds;
                let top = ascent - bb.y - i64::from(bb.height);
                if bb.width != 0
                    && bb.height != 0
                    && (bb.x < 0
                        || top < 0
                        || bb.x + i64::from(bb.width) > i64::from(w)
                        || top + i64::from(bb.height) > i64::from(h))
                {
                    return Err(invalid("Rockbox grayscale glyph exceeds cell"));
                }
                let area = w as usize * usize::from(h);
                b[start + off..start + off + area.div_ceil(2)].fill(255);
                for y in 0..bb.height {
                    for x in 0..bb.width {
                        let coverage = match &g.bitmap {
                            GlyphBitmap::Mono(m) => m.get_pixel(x, y)? * 255,
                            GlyphBitmap::Gray8(m) => m.get_pixel(x, y)?,
                        };
                        let q = ((u16::from(coverage) + 8) / 17) as u8;
                        let p =
                            (top as usize + y as usize) * w as usize + bb.x as usize + x as usize;
                        let shift = (p % 2) * 4;
                        b[start + off + p / 2] =
                            (b[start + off + p / 2] & !(15 << shift)) | ((15 - q) << shift);
                    }
                }
            }
        }
        let (target, r) = parse(&b, &v.limits, &mut budget)?;
        report.extend(r);
        audit(s, &target, "rockbox", &mut report, &mut budget)?;
        Ok(OutputBundle::single(o.primary_name.clone(), b))
    })();
    finish(result, report, "rockbox", o.strict)
}
