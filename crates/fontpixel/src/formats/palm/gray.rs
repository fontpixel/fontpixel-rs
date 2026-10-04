//! GrayFont wire layout from the author's GrayFormat.txt and Palm BitmapType docs.
use super::{PalmVersion, PalmWriteOptions};
use crate::bitmap::{Bitmap, Bounds, Gray8};
use crate::formats::legacy_common::*;
use crate::model::{Codepoint, Font, FontSet, Glyph, GlyphBitmap, Property, PropertyValue, Vector};
use crate::{Report, ResourceLimits, Result};
use std::collections::BTreeMap;
const E: ByteOrder = ByteOrder::Big;
pub(super) fn bitmap_tag(tag: &[u8]) -> bool {
    tag.len() == 4
        && tag[0] == b'G'
        && b"ULR".contains(&tag[1])
        && b"13".contains(&tag[2])
        && tag[3] == b'4'
}
pub(super) fn decode(
    b: &[u8],
    lookup: &BTreeMap<([u8; 4], u16), &[u8]>,
    l: &ResourceLimits,
    budget: &mut Budget,
    glyph_count: &mut usize,
    font_count: usize,
) -> Result<Vec<Font>> {
    let version = u16_at(b, 0, E)?;
    if version == 0 || version > 15 {
        return Err(invalid("unsupported GrayFont header version"));
    }
    slice(b, 0, if version == 1 { 22 } else { 34 })?;
    let first = u16_at(b, 2, E)?;
    let last = u16_at(b, 4, E)?;
    if first > last {
        return Err(invalid("GrayFont reversed character range"));
    }
    let count = usize::from(last - first) + 1;
    check(count, l.max_glyphs, "GrayFont glyph count")?;
    let nres = usize::from(u16_at(b, 16, E)?);
    check(nres, l.max_resources, "GrayFont bitmap resources")?;
    let res_at = usize::from(u16_at(b, 18, E)?);
    let glyph_at = usize::from(u16_at(b, 20, E)?);
    slice(b, res_at, nres * 8)?;
    slice(b, glyph_at, count * 12)?;
    let mut tags = Vec::new();
    for &tag in [*b"GU14", *b"GU34", *b"GL14", *b"GL34", *b"GR14", *b"GR34"].iter() {
        if (0..nres)
            .any(|i| lookup.contains_key(&(tag, u16_at(b, res_at + 8 * i + 4, E).unwrap_or(0))))
        {
            tags.push(tag);
        }
    }
    if tags.is_empty() {
        return Err(invalid("GrayFont has no supported bitmap resources"));
    }
    let present = (0..count)
        .filter(|&i| u16_at(b, glyph_at + 12 * i + 6, E).unwrap_or(0) != 0)
        .count();
    *glyph_count = glyph_count.saturating_add(present.saturating_mul(tags.len()));
    check(
        font_count.saturating_add(tags.len()),
        l.max_resources,
        "Palm aggregate strikes",
    )?;
    check(*glyph_count, l.max_glyphs, "GrayFont aggregate glyphs")?;
    let height = u32::from(u16_at(b, 8, E)?);
    let ascent = i64::from(u16_at(b, 10, E)? as i16);
    let descent = i64::from(u16_at(b, 12, E)? as i16);
    let mut fonts = Vec::new();
    for tag in tags {
        budget.items::<Font>(1)?;
        budget.add(1024)?;
        let mut f = Font::default();
        f.source_format = Some("palm".into());
        f.charset.name = Some("palm-os".into());
        f.metrics.pixel_size = Some(height);
        f.metrics.ascent = Some(ascent);
        f.metrics.descent = Some(descent);
        f.metrics.line_gap = Some(i64::from(u16_at(b, 14, E)? as i16));
        f.metrics.bounds = Some(Bounds::new(
            u32::from(u16_at(b, 6, E)?),
            height,
            0,
            ascent - i64::from(height),
        ));
        f.properties
            .push(Property::new("palm.gray_version", i64::from(version)));
        f.properties.push(Property::new(
            "palm.gray_type",
            PropertyValue::Bytes(tag.to_vec()),
        ));
        for i in 0..count {
            let at = glyph_at + 12 * i;
            let rn = usize::from(u16_at(b, at + 6, E)?);
            if rn == 0 {
                continue;
            }
            if rn > nres {
                return Err(invalid("GrayFont glyph resource number outside table"));
            }
            let ri = res_at + (rn - 1) * 8;
            let lo = u16_at(b, ri, E)?;
            let hi = u16_at(b, ri + 2, E)?;
            let id = u16_at(b, ri + 4, E)?;
            let code = u32::from(first) + i as u32;
            if lo > hi || code < u32::from(lo) || code > u32::from(hi) {
                return Err(invalid("GrayFont bitmap resource character range mismatch"));
            }
            let resource = lookup
                .get(&(tag, id))
                .ok_or_else(|| invalid("GrayFont missing bitmap resource"))?;
            let index = usize::from(u16_at(b, at + 8, E)?);
            let num = usize::from(hi - lo) + 1;
            if index >= num {
                return Err(invalid("GrayFont bitmap index outside resource range"));
            }
            slice(resource, 0, (index + 1) * 4)?;
            let pos = usize::from(u16_at(resource, index * 4, E)?);
            let len = usize::from(u16_at(resource, index * 4 + 2, E)?);
            if pos % 4 != 0 || pos < (index + 1) * 4 {
                return Err(invalid(
                    "GrayFont image offset overlaps index or is misaligned",
                ));
            }
            let width = u32::from(u16_at(b, at + 4, E)?);
            budget.items::<Glyph>(1)?;
            budget.add(128)?;
            let raster = decode_bitmap(slice(resource, pos, len)?, tag, width, height, l, budget)?;
            let mut g = Glyph::new(raster);
            g.codepoints.push(Codepoint::Legacy(code));
            g.metrics.bounds = Bounds::new(
                width,
                height,
                i64::from(u16_at(b, at, E)? as i16),
                ascent - i64::from(height),
            );
            g.metrics.advance = Some(Vector::new(i64::from(u16_at(b, at + 2, E)? as i16), 0));
            f.add_glyph(g)?;
        }
        fonts.push(f);
    }
    Ok(fonts)
}
fn decode_bitmap(
    b: &[u8],
    tag: [u8; 4],
    width: u32,
    height: u32,
    l: &ResourceLimits,
    budget: &mut Budget,
) -> Result<Bitmap<Gray8>> {
    slice(b, 0, 16)?;
    let w = u32::from(u16_at(b, 0, E)?);
    let h = u32::from(u16_at(b, 2, E)?);
    let row = usize::from(u16_at(b, 4, E)?);
    let flags = u16_at(b, 6, E)?;
    if b[8] != 4 || b[9] != tag[2] - b'0' || flags & 0x7600 != 0 {
        return Err(invalid("GrayFont bitmap depth/version/flags unsupported"));
    }
    if (tag[1] == b'U' && (w != width || h != height))
        || (tag[1] != b'U' && (h != width || w != height))
    {
        return Err(invalid(
            "GrayFont bitmap dimensions disagree with glyph table",
        ));
    }
    if row < (w as usize).div_ceil(2) {
        return Err(invalid("GrayFont bitmap row is too short"));
    }
    let head = if b[9] == 3 {
        slice(b, 0, 24)?;
        if b[10] < 24 {
            return Err(invalid("short Palm v3 bitmap header"));
        }
        usize::from(b[10])
    } else {
        16
    };
    let size = row
        .checked_mul(h as usize)
        .ok_or_else(|| invalid("GrayFont bitmap size overflow"))?;
    check(size, l.max_table_bytes, "GrayFont bitmap bytes")?;
    let pixels = (w as usize)
        .checked_mul(h as usize)
        .ok_or_else(|| invalid("GrayFont pixel overflow"))?;
    check(pixels, l.max_pixels_per_glyph, "GrayFont glyph pixels")?;
    budget.add(size.saturating_add(pixels.saturating_mul(2)))?;
    let mut packed = vec![0; size];
    if flags & 0x8000 == 0 {
        packed.copy_from_slice(slice(b, head, size)?);
    } else {
        if b[9] == 3 && b[13] != 0 {
            return Err(invalid("GrayFont only permits scanline compression"));
        }
        let prefix = if b[9] == 3 { 4 } else { 2 };
        let declared = if prefix == 4 {
            u32_at(b, head, E)? as usize
        } else {
            usize::from(u16_at(b, head, E)?)
        };
        if declared < prefix || declared > b.len() - head {
            return Err(invalid("GrayFont compressed length exceeds bitmap"));
        }
        let src = slice(b, head + prefix, declared - prefix)?;
        let mut at = 0usize;
        for y in 0..h as usize {
            if y != 0 {
                packed.copy_within((y - 1) * row..y * row, y * row);
            }
            for x in (0..row).step_by(8) {
                let mask = *src
                    .get(at)
                    .ok_or_else(|| invalid("truncated GrayFont scanline mask"))?;
                at += 1;
                for j in 0..8.min(row - x) {
                    if mask & (128 >> j) != 0 {
                        packed[y * row + x + j] = *src
                            .get(at)
                            .ok_or_else(|| invalid("truncated GrayFont scanline byte"))?;
                        at += 1;
                    }
                }
            }
        }
    }
    let mut out = vec![0; pixels];
    for y in 0..h as usize {
        for x in 0..w as usize {
            let v = (packed[y * row + x / 2] >> if x % 2 == 0 { 4 } else { 0 }) & 15;
            let (dx, dy) = match tag[1] {
                b'L' => (width as usize - 1 - y, x),
                b'R' => (y, height as usize - 1 - x),
                _ => (x, y),
            };
            out[dy * width as usize + dx] = v * 17;
        }
    }
    Bitmap::<Gray8>::from_raw(width, height, &out, bitmap_limits(l))
}

type EncodedRecord = (i64, Vec<u8>, Vec<u8>);
pub(super) fn encode(
    s: &FontSet,
    o: &PalmWriteOptions,
    r: &mut Report,
    budget: &mut Budget,
) -> Result<Vec<EncodedRecord>> {
    let mut records = Vec::new();
    let mut next_id = 128u16;
    for (fi, f) in s.fonts.iter().enumerate() {
        let mut slots = BTreeMap::new();
        for (id, g) in f.glyphs() {
            if g.codepoints.is_empty() {
                budget.loss(
                    r,
                    "palm",
                    "write.gray_unencoded",
                    Some(fi),
                    Some(id.index()),
                    "GrayFont requires a character code",
                )?;
            }
            for &cp in &g.codepoints {
                if cp.value() > 65535 || matches!(cp, Codepoint::Unicode(_)) && !o.unicode_as_legacy
                {
                    budget.loss(
                        r,
                        "palm",
                        "write.gray_mapping",
                        Some(fi),
                        Some(id.index()),
                        format!("Cannot encode {cp:?}"),
                    )?;
                } else {
                    budget.add(64)?;
                    slots.insert(cp.value() as u16, g);
                }
            }
        }
        let first = *slots
            .first_key_value()
            .ok_or_else(|| invalid("GrayFont has no character slots"))?
            .0;
        let last = *slots
            .last_key_value()
            .ok_or_else(|| invalid("GrayFont has no character slots"))?
            .0;
        let count = usize::from(last - first) + 1;
        check(count, o.limits.max_glyphs, "GrayFont character span")?;
        let mut ascent = 0i64;
        let mut descent = 0i64;
        for g in slots.values() {
            ascent = ascent.max(g.metrics.bounds.y + i64::from(g.metrics.bounds.height));
            descent = descent.max(-g.metrics.bounds.y);
        }
        ascent = ascent.max(f.metrics.ascent.unwrap_or(0));
        descent = descent.max(f.metrics.descent.unwrap_or(0));
        let height = u16::try_from(
            ascent
                .checked_add(descent)
                .ok_or_else(|| invalid("GrayFont height overflow"))?,
        )
        .map_err(|_| invalid("GrayFont height exceeds word"))?;
        let mut images = Vec::new();
        let mut max_advance = 0;
        let mut min_left = 0;
        let mut overhang = 0;
        for (&cp, g) in &slots {
            let w = g.bitmap.width();
            let advance = g.metrics.advance.map_or(i64::from(w), |v| v.x);
            max_advance = max_advance.max(advance);
            min_left = min_left.min(g.metrics.bounds.x);
            overhang = overhang.max(i64::from(w) + g.metrics.bounds.x - advance);
            let row = (w as usize).div_ceil(4) * 2;
            let size = row * usize::from(height);
            check(
                w as usize * usize::from(height),
                o.limits.max_pixels_per_glyph,
                "GrayFont output glyph pixels",
            )?;
            check(size, o.limits.max_table_bytes, "GrayFont output bitmap")?;
            budget.add(size.saturating_add(24).saturating_mul(2))?;
            let mut b = vec![0; 24 + size];
            put16(
                &mut b,
                0,
                u16::try_from(w).map_err(|_| invalid("GrayFont width exceeds word"))?,
                E,
            );
            put16(&mut b, 2, height, E);
            put16(
                &mut b,
                4,
                u16::try_from(row).map_err(|_| invalid("GrayFont row exceeds word"))?,
                E,
            );
            b[8] = 4;
            b[9] = 3;
            b[10] = 24;
            b[13] = 255;
            put16(&mut b, 14, 144, E);
            let top = ascent - g.metrics.bounds.y - i64::from(g.metrics.bounds.height);
            for y in 0..g.bitmap.height() as usize {
                for x in 0..w as usize {
                    let v = match &g.bitmap {
                        GlyphBitmap::Mono(m) => {
                            if m.as_bytes()[y * m.stride() + x / 8] & (128 >> (x % 8)) != 0 {
                                15
                            } else {
                                0
                            }
                        }
                        GlyphBitmap::Gray8(m) => {
                            (u16::from(m.as_bytes()[y * m.stride() + x]) + 8) / 17
                        }
                    };
                    let at = 24 + (top as usize + y) * row + x / 2;
                    b[at] |= (v as u8) << if x % 2 == 0 { 4 } else { 0 };
                }
            }
            images.push((cp, b, g));
        }
        // Split payloads at glyph boundaries before the native 16-bit address space ends.
        let mut chunks = Vec::new();
        let mut start = 0;
        while start < images.len() {
            let mut end = start;
            let mut size = 4;
            let mut payload = 0usize;
            while end < images.len() {
                let span = usize::from(images[end].0 - images[start].0) + 1;
                payload = payload.saturating_add((images[end].1.len() + 3) & !3);
                let candidate = (span + 1) * 4 + payload;
                if candidate > 65535 {
                    break;
                }
                size = candidate;
                end += 1;
            }
            if end == start {
                return Err(invalid("GrayFont glyph exceeds bitmap resource capacity"));
            }
            chunks.push((start, end, size));
            start = end;
        }
        check(
            chunks.len(),
            o.limits.max_resources,
            "GrayFont output resources",
        )?;
        let head = if o.version == PalmVersion::Gray1 {
            24
        } else {
            36
        };
        let g_at = head + chunks.len() * 8;
        let n = g_at + count * 12;
        if n > 65535 {
            return Err(invalid(
                "GrayFont character span exceeds header address space",
            ));
        }
        check(n, o.limits.max_table_bytes, "GrayFont header")?;
        budget.add(n)?;
        let mut header = vec![0; n];
        for (at, v) in [
            (
                0,
                if o.version == PalmVersion::Gray1 {
                    1
                } else {
                    2
                },
            ),
            (2, first),
            (4, last),
            (
                6,
                u16::try_from(max_advance).map_err(|_| invalid("GrayFont advance exceeds word"))?,
            ),
            (8, height),
            (
                10,
                i16::try_from(ascent).map_err(|_| invalid("GrayFont ascent exceeds signed word"))?
                    as u16,
            ),
            (
                12,
                i16::try_from(descent)
                    .map_err(|_| invalid("GrayFont descent exceeds signed word"))?
                    as u16,
            ),
            (16, chunks.len() as u16),
            (18, head as u16),
            (20, g_at as u16),
        ] {
            put16(&mut header, at, v, E);
        }
        put16(
            &mut header,
            14,
            i16::try_from(f.metrics.line_gap.unwrap_or(0))
                .map_err(|_| invalid("GrayFont leading exceeds signed word"))? as u16,
            E,
        );
        if o.version != PalmVersion::Gray1 {
            put16(
                &mut header,
                22,
                i16::try_from(min_left)
                    .map_err(|_| invalid("GrayFont left bearing exceeds signed word"))?
                    as u16,
                E,
            );
            put16(
                &mut header,
                24,
                i16::try_from(overhang)
                    .map_err(|_| invalid("GrayFont overhang exceeds signed word"))?
                    as u16,
                E,
            );
        }
        let header_id = next_id;
        next_id = next_id
            .checked_add(1)
            .ok_or_else(|| invalid("GrayFont resource IDs exhausted"))?;
        let mut bitmap_records = Vec::new();
        for (ci, &(start, end, size)) in chunks.iter().enumerate() {
            let id = next_id;
            next_id = next_id
                .checked_add(1)
                .ok_or_else(|| invalid("GrayFont resource IDs exhausted"))?;
            let lo = images[start].0;
            let hi = images[end - 1].0;
            let mut payload = vec![0; size];
            budget.add(size)?;
            let mut pos = (usize::from(hi - lo) + 2) * 4;
            for &(cp, ref image, g) in &images[start..end] {
                let index = usize::from(cp - lo);
                put16(&mut payload, index * 4, pos as u16, E);
                put16(
                    &mut payload,
                    index * 4 + 2,
                    u16::try_from(image.len())
                        .map_err(|_| invalid("GrayFont image length exceeds word"))?,
                    E,
                );
                payload[pos..pos + image.len()].copy_from_slice(image);
                pos += (image.len() + 3) & !3;
                let ga = g_at + usize::from(cp - first) * 12;
                for (at, v) in [
                    (
                        ga,
                        i16::try_from(g.metrics.bounds.x)
                            .map_err(|_| invalid("GrayFont left bearing exceeds signed word"))?
                            as u16,
                    ),
                    (
                        ga + 2,
                        i16::try_from(
                            g.metrics
                                .advance
                                .map_or(i64::from(g.bitmap.width()), |v| v.x),
                        )
                        .map_err(|_| invalid("GrayFont advance exceeds signed word"))?
                            as u16,
                    ),
                    (ga + 4, g.bitmap.width() as u16),
                    (ga + 6, (ci + 1) as u16),
                    (ga + 8, index as u16),
                ] {
                    put16(&mut header, at, v, E);
                }
            }
            let ra = head + ci * 8;
            put16(&mut header, ra, lo, E);
            put16(&mut header, ra + 2, hi, E);
            put16(&mut header, ra + 4, id, E);
            let mut info = b"GU34".to_vec();
            info.extend_from_slice(&id.to_be_bytes());
            bitmap_records.push((0, info, payload));
        }
        let mut info = b"GrFn".to_vec();
        info.extend_from_slice(&header_id.to_be_bytes());
        records.push((records.len() as i64, info, header));
        for mut rec in bitmap_records {
            rec.0 = records.len() as i64;
            records.push(rec);
        }
    }
    Ok(records)
}
