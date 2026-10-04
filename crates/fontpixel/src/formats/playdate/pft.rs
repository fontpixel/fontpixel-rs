//! Native compiled pages, kerning records and image cells.
use super::*;
use crate::model::Codepoint;
use std::borrow::Cow;
use std::io::Read;
fn bit_indices(bits: &[u8]) -> impl Iterator<Item = usize> + '_ {
    bits.iter().enumerate().flat_map(|(i, &b)| {
        (0..8).filter_map(move |j| {
            if b & (1 << j) != 0 {
                Some(i * 8 + j)
            } else {
                None
            }
        })
    })
}
pub(super) fn parse(
    b: &[u8],
    l: &ResourceLimits,
    budget: &mut Budget,
) -> Result<(FontSet, Report)> {
    if !b.starts_with(b"Playdate FNT") {
        return Err(invalid("invalid PFT signature"));
    }
    let flags = r32(b, 12)?;
    if flags & !0x80000001 != 0 {
        return Err(invalid("unsupported PFT flags"));
    }
    check(b.len(), l.max_table_bytes, "PFT input bytes")?;
    let raw = if flags & 0x80000000 != 0 {
        let size = r32(b, 16)? as usize;
        if r32(b, 28)? != 0 {
            return Err(invalid("unsupported PFT compressed reserved field"));
        }
        check(
            size,
            l.max_decompressed_bytes.min(l.max_table_bytes),
            "PFT decompressed bytes",
        )?;
        budget.add(size.saturating_add(65536))?;
        let mut decoder =
            flate2::bufread::ZlibDecoder::new(slice(b, 32, b.len().saturating_sub(32))?);
        let mut out = vec![0; size];
        decoder
            .read_exact(&mut out)
            .map_err(|e| invalid(format!("PFT zlib: {e}")))?;
        let extra = decoder
            .read(&mut [0; 1])
            .map_err(|e| invalid(format!("PFT zlib: {e}")))?;
        if extra != 0 || decoder.total_in() as usize != b.len() - 32 {
            return Err(invalid("PFT zlib length mismatch"));
        }
        if out.first().copied().map(u32::from) != Some(r32(b, 20)?)
            || out.get(1).copied().map(u32::from) != Some(r32(b, 24)?)
        {
            return Err(invalid("PFT compressed dimensions disagree"));
        }
        Cow::Owned(out)
    } else {
        Cow::Borrowed(slice(b, 16, b.len().saturating_sub(16))?)
    };
    let b = raw.as_ref();
    slice(b, 0, 4)?;
    let (w, h) = (u32::from(b[0]), u32::from(b[1]));
    if w == 0 || h == 0 {
        return Err(invalid("zero PFT cell dimension"));
    }
    let tracking = r16(b, 2)? as i16;
    let usage = if flags & 1 != 0 { 128 } else { 64 };
    let pages = slice(b, 4, usage)?;
    let count = pages.iter().map(|b| b.count_ones() as usize).sum::<usize>();
    if count == 0 {
        return Err(invalid("PFT contains no pages"));
    }
    let table = 4 + usage;
    let start = table + count * 4;
    slice(b, table, count * 4)?;
    let mut total = 0usize;
    let mut prev = 0;
    for i in 0..count {
        let end = r32(b, table + i * 4)? as usize;
        if end <= prev {
            return Err(invalid("PFT page ends are not increasing"));
        }
        let page = slice(b, start + prev, end - prev)?;
        slice(page, 0, 36)?;
        total += page[4..36]
            .iter()
            .map(|b| b.count_ones() as usize)
            .sum::<usize>();
        prev = end;
    }
    if start + prev != b.len() {
        return Err(invalid("PFT final page end disagrees with data length"));
    }
    allocation(total, l, budget)?;
    let mut kern = BTreeMap::new();
    let mut f = native_font(w, h, i64::from(h), tracking, &kern);
    let mut report = Report::default();
    let mut prev = 0;
    for (i, page_index) in bit_indices(pages).enumerate() {
        let end = r32(b, table + i * 4)? as usize;
        let page = &b[start + prev..start + end];
        prev = end;
        let present = &page[4..36];
        let n = present
            .iter()
            .map(|b| b.count_ones() as usize)
            .sum::<usize>();
        if n == 0 || page[3] != (n as u8) || page[..3] != [0, 0, 0] {
            return Err(invalid("invalid PFT page header"));
        }
        let data = 36 + (n * 2).next_multiple_of(4);
        slice(page, 36, data - 36)?;
        let mut previous = 0;
        for (j, low) in bit_indices(present).enumerate() {
            let cp = (page_index * 256 + low) as u32;
            if char::from_u32(cp).is_none() {
                return Err(invalid("PFT mapping is not a Unicode scalar"));
            }
            let end = usize::from(r16(page, 36 + j * 2)?);
            if end <= previous {
                return Err(invalid("PFT glyph ends are not increasing"));
            }
            let record = slice(page, data + previous, end - previous)?;
            previous = end;
            slice(record, 0, 4)?;
            let advance = i64::from(record[0]) + i64::from(tracking);
            let short = usize::from(record[1]);
            let long = usize::from(r16(record, 2)?);
            let long_at = (4 + short * 2).next_multiple_of(4);
            let image = long_at + long * 4;
            slice(record, 4, image - 4)?;
            for k in 0..short {
                budget.add(128)?;
                let target = (page_index * 256 + usize::from(record[4 + k * 2])) as u32;
                if kern.insert((cp, target), record[5 + k * 2] as i8).is_some() {
                    return Err(invalid("duplicate PFT kerning pair"));
                }
            }
            for k in 0..long {
                let v = r32(record, long_at + k * 4)?;
                let target = v & 0xffffff;
                if target > 0x3ffff || char::from_u32(target).is_none() || target >> 8 == cp >> 8 {
                    return Err(invalid("invalid PFT long kerning codepoint"));
                }
                budget.add(128)?;
                if kern.insert((cp, target), (v >> 24) as u8 as i8).is_some() {
                    return Err(invalid("duplicate PFT kerning pair"));
                }
            }
            let cell = slice(record, image, record.len().saturating_sub(image))?;
            slice(cell, 0, 16)?;
            let cw = u32::from(r16(cell, 0)?);
            let ch = u32::from(r16(cell, 2)?);
            let stride = usize::from(r16(cell, 4)?);
            let left = u32::from(r16(cell, 6)?);
            let right = u32::from(r16(cell, 8)?);
            let top = u32::from(r16(cell, 10)?);
            let bottom = u32::from(r16(cell, 12)?);
            let flags = r16(cell, 14)?;
            if flags & !7 != 0 || stride < (cw as usize).div_ceil(8) {
                return Err(invalid("invalid PFT image flags or stride"));
            }
            let width = left + cw + right;
            let height = top + ch + bottom;
            let len = stride * ch as usize;
            let colors = slice(cell, 16, len)?;
            let mask = if flags & 3 != 0 {
                Some(slice(cell, 16 + len, len)?)
            } else {
                None
            };
            let mut rows = raster(width, height, l, budget)?;
            let row = (width as usize).div_ceil(8);
            let mut white = false;
            for y in 0..ch as usize {
                for x in 0..cw as usize {
                    let bit = 0x80 >> (x % 8);
                    let opaque = mask.is_none_or(|m| m[y * stride + x / 8] & bit != 0);
                    if opaque {
                        if colors[y * stride + x / 8] & bit == 0 {
                            rows[(top as usize + y) * row + (left as usize + x) / 8] |=
                                0x80 >> ((left as usize + x) % 8);
                        } else {
                            white = true;
                        }
                    }
                }
            }
            if white {
                budget.loss(
                    &mut report,
                    "playdate",
                    "read.opaque_white",
                    Some(0),
                    Some(f.len()),
                    "Opaque white pixels cannot be represented by black-ink coverage",
                )?;
            }
            let mut g = glyph(
                width,
                height,
                0,
                i64::from(h) - i64::from(height),
                advance,
                &rows,
                l,
            )?;
            g.codepoints.push(Codepoint::Unicode(cp));
            f.add_glyph(g)?;
        }
        if data + previous != page.len() {
            return Err(invalid("PFT final glyph end disagrees with page length"));
        }
    }
    f.properties
        .set("playdate.kerning", kern_property(&kern).value);
    Ok((set(f, "playdate"), report))
}
pub(super) fn encode(
    p: &Prepared<'_>,
    v: &PlaydateWriteOptions,
    budget: &mut Budget,
) -> Result<Vec<u8>> {
    let (w, h) = (byte(i64::from(p.width))?, byte(i64::from(p.height))?);
    let wide = p.entries.last().unwrap().cp >= 0x20000;
    let usage = if wide { 128 } else { 64 };
    let mut pages: BTreeMap<u32, Vec<&Entry<'_>>> = BTreeMap::new();
    for e in &p.entries {
        budget.add(64)?;
        pages.entry(e.cp >> 8).or_default().push(e);
    }
    // An extra zero word after the image protects native aligned bitmap reads.
    let stride = (usize::from(w).div_ceil(8)).next_multiple_of(4);
    let image = 16 + stride * usize::from(h) * 2 + 4;
    let mut lengths = Vec::new();
    budget.items::<usize>(pages.len())?;
    for entries in pages.values() {
        let mut size = 36 + (entries.len() * 2).next_multiple_of(4);
        let mut payload = 0;
        for e in entries {
            let kern = p.kern.range((e.cp, 0)..=(e.cp, u32::MAX));
            let short = kern
                .clone()
                .filter(|&(&(a, b), _)| a >> 8 == b >> 8)
                .count();
            let long = kern.count() - short;
            check(short, 255, "PFT short kerning count")?;
            check(long, 65535, "PFT long kerning count")?;
            payload += (4 + short * 2).next_multiple_of(4) + long * 4 + image;
        }
        check(payload, 65535, "PFT page glyph offsets")?;
        size += payload;
        lengths.push(size);
    }
    let size = 16 + 4 + usage + pages.len() * 4 + lengths.iter().sum::<usize>();
    let mut out = output(size, v.max_output_bytes, &v.limits, budget)?;
    out[..12].copy_from_slice(b"Playdate FNT");
    p32(&mut out, 12, u32::from(wide));
    let b = &mut out[16..];
    b[0] = w;
    b[1] = h;
    p16(b, 2, p.tracking as u16);
    let table = 4 + usage;
    let start = table + pages.len() * 4;
    let mut page_at = start;
    for ((i, (index, entries)), page_len) in pages.into_iter().enumerate().zip(lengths) {
        b[4 + index as usize / 8] |= 1 << (index % 8);
        p32(b, table + i * 4, (page_at + page_len - start) as u32);
        let page = &mut b[page_at..page_at + page_len];
        page[3] = entries.len() as u8;
        let data = 36 + (entries.len() * 2).next_multiple_of(4);
        let mut at = data;
        for (j, e) in entries.iter().enumerate() {
            let low = (e.cp & 255) as usize;
            page[4 + low / 8] |= 1 << (low % 8);
            let kern = p.kern.range((e.cp, 0)..=(e.cp, u32::MAX));
            // The native runtime selects the short table for same-page pairs;
            // a same-page target in the long table is not a substitute.
            let short = kern
                .clone()
                .filter(|&(&(a, b), _)| a >> 8 == b >> 8)
                .count();
            let long = kern.clone().count() - short;
            let record = at;
            page[at] = e.width;
            page[at + 1] = short as u8;
            p16(page, at + 2, long as u16);
            at += 4;
            for (&(a, b), &k) in kern.clone().filter(|&(&(a, b), _)| a >> 8 == b >> 8) {
                debug_assert_eq!(a, e.cp);
                page[at] = b as u8;
                page[at + 1] = k as u8;
                at += 2;
            }
            at = record + (4 + short * 2).next_multiple_of(4);
            for (&(a, b), &k) in kern.filter(|&(&(a, b), _)| a >> 8 != b >> 8) {
                debug_assert_eq!(a, e.cp);
                p32(page, at, b | ((k as u8 as u32) << 24));
                at += 4;
            }
            p16(page, at, u16::from(w));
            p16(page, at + 2, u16::from(h));
            p16(page, at + 4, stride as u16);
            p16(page, at + 14, 3);
            let raster = canvas(e.glyph, p.width, p.height, p.ascent, &v.limits, budget)?;
            let row = usize::from(w).div_ceil(8);
            let mask = at + 16 + stride * usize::from(h);
            for y in 0..usize::from(h) {
                page[mask + y * stride..mask + y * stride + row]
                    .copy_from_slice(&raster[y * row..(y + 1) * row]);
            }
            at += image;
            p16(page, 36 + j * 2, (at - data) as u16);
        }
        page_at += page_len;
    }
    Ok(out)
}
