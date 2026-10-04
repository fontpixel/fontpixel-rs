//! AL self-relative directories and extension chains.
use super::*;
#[derive(Clone, Copy)]
struct Piece {
    at: usize,
    xw: u16,
    hd: u8,
    height: u8,
}
pub(super) fn read(
    b: &[u8],
    o: &XeroxReadOptions,
    l: &ResourceLimits,
    budget: &mut Budget,
) -> Result<(FontSet, Report)> {
    slice(b, 0, 6)?;
    let line = u16_at(b, 0, ByteOrder::Big)?;
    let baseline = o.alto_baseline.unwrap_or(b[2] & 127);
    let mut at = 4;
    let mut first = b.len();
    let mut pieces = Vec::new();
    while at < first {
        let pointer = u16_at(b, at, ByteOrder::Big)?;
        if pointer == 0 {
            at += 2;
            break;
        }
        let p = at
            .checked_add(usize::from(pointer) * 2)
            .ok_or_else(|| invalid("Xerox AL self-relative pointer overflow"))?;
        let d = slice(b, p, 4)?;
        let h = d[3];
        let start = p
            .checked_sub(usize::from(h) * 2)
            .ok_or_else(|| invalid("Xerox AL raster precedes file"))?;
        if start < at + 2 {
            return Err(invalid("Xerox AL character overlaps directory"));
        }
        first = first.min(start);
        budget.items::<Piece>(1)?;
        pieces.push(Piece {
            at: start,
            xw: u16_at(b, p, ByteOrder::Big)?,
            hd: d[2],
            height: h,
        });
        if pieces.len() > 32768 {
            return Err(invalid("Xerox AL extension index exceeds 15 bits"));
        }
        at += 2;
    }
    if pieces.is_empty() || at > first {
        return Err(invalid("Xerox AL has no valid pointer directory"));
    }
    for p in &pieces {
        if p.at < at {
            return Err(invalid("Xerox AL raster overlaps completed directory"));
        }
    }
    let mut f = source("xerox", b, pieces.len().min(256), l, budget)?;
    f.properties.push(Property::new(
        "xerox.al_header",
        PropertyValue::Bytes(b[..4].to_vec()),
    ));
    budget.items::<u16>(pieces.len())?;
    let mut visited = vec![0u16; pieces.len()];
    let mut count = 0;
    for cp in 0..pieces.len().min(256) {
        let first = pieces[cp];
        if first.height == 0 && first.hd == 0 && first.xw == 1 {
            continue;
        }
        count += 1;
        check(count, l.max_glyphs, "Xerox AL glyphs")?;
        let mut current = cp;
        let mut chain = Vec::new();
        let mut bottom = i64::MAX;
        let mut top = i64::MIN;
        let last;
        loop {
            let p = *pieces
                .get(current)
                .ok_or_else(|| invalid("Xerox AL extension index outside directory"))?;
            if visited[current] == cp as u16 + 1 {
                return Err(invalid("Xerox AL extension cycle"));
            }
            visited[current] = cp as u16 + 1;
            budget.items::<Piece>(1)?;
            chain.push(p);
            let y = i64::from(baseline) - i64::from(p.hd) - i64::from(p.height);
            bottom = bottom.min(y);
            top = top.max(y + i64::from(p.height));
            if p.xw & 1 != 0 {
                last = i64::from((p.xw - 1) / 2);
                break;
            }
            current = usize::from(p.xw / 2);
        }
        let width = u32::try_from(chain.len() * 16)
            .map_err(|_| invalid("Xerox AL chained width exceeds u32"))?;
        let height = (top - bottom) as u32;
        check(
            width as usize * height as usize,
            l.max_pixels_per_glyph,
            "Xerox AL chained pixels",
        )?;
        let row = width as usize / 8;
        budget.add(row * height as usize)?;
        let mut packed = vec![0; row * height as usize];
        for (i, p) in chain.iter().enumerate() {
            let y = i64::from(baseline) - i64::from(p.hd) - i64::from(p.height);
            let dy = (top - y - i64::from(p.height)) as usize;
            for iy in 0..usize::from(p.height) {
                packed[(dy + iy) * row + i * 2..(dy + iy) * row + i * 2 + 2]
                    .copy_from_slice(slice(b, p.at + iy * 2, 2)?);
            }
        }
        let bitmap = raster(width, height, &packed, l, budget)?;
        glyph(
            &mut f,
            bitmap,
            cp as u32,
            0,
            bottom,
            (chain.len() as i64 - 1) * 16 + last,
        )?;
    }
    metrics(&mut f, i64::from(line));
    Ok((set(vec![f], "xerox"), Report::default()))
}
pub(super) fn write(
    f: &Font,
    v: &XeroxWriteOptions,
    budget: &mut Budget,
    report: &mut Report,
) -> Result<Vec<u8>> {
    let slots = select(f, 255, v.unicode_as_legacy, "xerox", report, budget)?;
    let old = bytes(f, "xerox.al_header").filter(|b| b.len() == 4);
    let baseline = match v.alto_baseline.or_else(|| old.map(|b| b[2] & 127)) {
        Some(b) => b,
        None => u8::try_from(f.metrics.ascent.unwrap_or_else(|| {
            f.glyphs()
                .map(|(_, g)| g.metrics.bounds.y + i64::from(g.bitmap.height()))
                .max()
                .unwrap_or(0)
        }))
        .map_err(|_| invalid("Xerox AL baseline exceeds u8"))?,
    };
    if baseline > 127 {
        return Err(invalid("Xerox AL baseline exceeds seven bits"));
    }
    let line = u16::try_from(f.metrics.line_height.unwrap_or(i64::from(baseline)))
        .map_err(|_| invalid("Xerox AL line height exceeds u16"))?;
    let mut extra = 0;
    let mut size = 0usize;
    let mut maxwidth = 0;
    for g in slots.values() {
        let b = mono(g)?;
        let n = (b.width() as usize).div_ceil(16).max(1);
        let advance = g.metrics.advance.map_or(i64::from(b.width()), |a| a.x);
        if g.metrics.bounds.x != 0
            || b.height() > 255
            || !(0..=255).contains(&advance)
            || advance < (n as i64 - 1) * 16
        {
            return Err(invalid(
                "Xerox AL glyph bearing, height or advance is not representable",
            ));
        }
        extra += n - 1;
        size = size
            .checked_add(n * (b.height() as usize * 2 + 4))
            .ok_or_else(|| invalid("Xerox AL size overflow"))?;
        maxwidth = maxwidth.max(advance as u8);
    }
    let count = 256 + extra;
    if count > 32768 {
        return Err(invalid("Xerox AL extension directory exceeds 15 bits"));
    }
    let table_end = 4 + (count + 1) * 2;
    let total = table_end + 4 + size;
    output_limit(total, v.max_output_bytes, &v.limits, budget)?;
    let mut out = vec![0; total];
    put16(&mut out, 0, line, ByteOrder::Big);
    out[2] = baseline | 128;
    out[3] = maxwidth;
    put16(&mut out, table_end, 1, ByteOrder::Big);
    for cp in 0..count {
        put16(
            &mut out,
            4 + cp * 2,
            u16::try_from((table_end - (4 + cp * 2)) / 2)
                .map_err(|_| invalid("Xerox AL dummy pointer exceeds u16"))?,
            ByteOrder::Big,
        );
    }
    let mut at = table_end + 4;
    let mut next_extension = 256;
    for (&cp, g) in &slots {
        let bitmap = mono(g)?;
        let n = (bitmap.width() as usize).div_ceil(16).max(1);
        let hd =
            u8::try_from(i64::from(baseline) - g.metrics.bounds.y - i64::from(bitmap.height()))
                .map_err(|_| invalid("Xerox AL top skip exceeds u8"))?;
        let mut index = cp as usize;
        let advance = g.metrics.advance.map_or(i64::from(bitmap.width()), |a| a.x);
        for part in 0..n {
            for y in 0..bitmap.height() as usize {
                for x in 0..16 {
                    let sx = part * 16 + x;
                    if sx < bitmap.width() as usize
                        && bitmap.as_bytes()[y * bitmap.stride() + sx / 8] & (128 >> (sx % 8)) != 0
                    {
                        out[at + y * 2 + x / 8] |= 128 >> (x % 8);
                    }
                }
            }
            let p = at + bitmap.height() as usize * 2;
            let rel = (p - (4 + index * 2)) / 2;
            put16(
                &mut out,
                4 + index * 2,
                u16::try_from(rel)
                    .map_err(|_| invalid("Xerox AL self-relative pointer exceeds u16"))?,
                ByteOrder::Big,
            );
            let xw = if part + 1 == n {
                (advance - (n as i64 - 1) * 16) as u16 * 2 + 1
            } else {
                next_extension as u16 * 2
            };
            put16(&mut out, p, xw, ByteOrder::Big);
            out[p + 2] = hd;
            out[p + 3] = bitmap.height() as u8;
            at = p + 4;
            if part + 1 < n {
                index = next_extension;
                next_extension += 1;
            }
        }
    }
    Ok(out)
}
