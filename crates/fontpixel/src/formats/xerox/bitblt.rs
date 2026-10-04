//! Plain/kerned strikes and their indexed counterparts.
use super::*;
struct Body<'a> {
    raster: &'a [u8],
    bounds: &'a [u8],
    row: usize,
    ascent: u16,
    descent: u16,
    count: usize,
}
fn body(b: &[u8], at: usize) -> Result<(Body<'_>, usize)> {
    let len = usize::from(u16_at(b, at, ByteOrder::Big)?) * 2;
    let data = slice(b, at, len)?;
    slice(data, 0, 10)?;
    let ascent = u16_at(data, 2, ByteOrder::Big)?;
    let descent = u16_at(data, 4, ByteOrder::Big)?;
    if u16_at(data, 6, ByteOrder::Big)? != 0 {
        return Err(invalid("Xerox BITBLT xoffset must be zero"));
    }
    let row = usize::from(u16_at(data, 8, ByteOrder::Big)?) * 2;
    let pixels = row * (usize::from(ascent) + usize::from(descent));
    let raster = slice(data, 10, pixels)?;
    let offsets = &data[10 + pixels..];
    if offsets.len() < 2 || offsets.len() % 2 != 0 {
        return Err(invalid("Xerox BITBLT boundary table is empty or odd"));
    }
    let count = offsets.len() / 2 - 1;
    let mut previous = 0;
    for i in 0..=count {
        let pos = usize::from(u16_at(offsets, i * 2, ByteOrder::Big)?);
        if pos < previous || pos > row * 8 {
            return Err(invalid("Xerox BITBLT horizontal boundary outside raster"));
        }
        previous = pos;
    }
    Ok((
        Body {
            raster,
            bounds: offsets,
            row,
            ascent,
            descent,
            count,
        },
        at + len,
    ))
}
pub(super) fn read(b: &[u8], l: &ResourceLimits, budget: &mut Budget) -> Result<(FontSet, Report)> {
    slice(b, 0, 8)?;
    let flags = u16_at(b, 0, ByteOrder::Big)?;
    if flags & 0x8000 == 0 || flags & 4095 != 0 {
        return Err(invalid("Xerox BITBLT header flags are invalid"));
    }
    let indexed = flags & 0x4000 != 0;
    let kerned = flags & 0x1000 != 0;
    let first = u16_at(b, 2, ByteOrder::Big)?;
    let last = u16_at(b, 4, ByteOrder::Big)?;
    if last < first {
        return Err(invalid("Xerox BITBLT character range is inverted"));
    }
    let n = usize::from(last - first) + 1;
    check(n + 1, l.max_glyphs, "Xerox BITBLT slots including dummy")?;
    let mut at = 8;
    let bbox = if kerned {
        let raw = slice(b, at, 8)?;
        at += 8;
        Some(raw)
    } else {
        None
    };
    let left = bbox.map_or(Ok(0), |v| {
        u16_at(v, 0, ByteOrder::Big).map(|x| i64::from(x as i16))
    })?;
    let mut bodies = Vec::new();
    let (ascent, descent, map) = if indexed {
        let ascent = u16_at(b, at, ByteOrder::Big)?;
        let descent = u16_at(b, at + 2, ByteOrder::Big)?;
        let count = usize::from(u16_at(b, at + 4, ByteOrder::Big)?);
        at += 6;
        if count == 0 || count > 128 {
            return Err(invalid("Xerox StrikeIndex needs 1..128 bodies"));
        }
        let map = slice(b, at, (n + 1) * 2)?;
        at += (n + 1) * 2;
        budget.items::<Body<'_>>(count)?;
        for _ in 0..count {
            let (body, end) = body(b, at)?;
            if body.ascent > ascent || body.descent > descent {
                return Err(invalid(
                    "Xerox indexed strike exceeds global vertical extent",
                ));
            }
            bodies.push(body);
            at = end;
        }
        (ascent, descent, Some(map))
    } else {
        let (body, end) = body(b, at)?;
        at = end;
        if body.count != n + 1 {
            return Err(invalid(
                "Xerox BITBLT body boundary count disagrees with code range",
            ));
        }
        let metrics = (body.ascent, body.descent);
        budget.items::<Body<'_>>(1)?;
        bodies.push(body);
        (metrics.0, metrics.1, None)
    };
    let spacing = if kerned {
        let data = slice(b, at, (n + 1) * 2)?;
        at += (n + 1) * 2;
        Some(data)
    } else {
        None
    };
    if at != b.len() {
        return Err(invalid("Xerox BITBLT bytes follow declared bodies"));
    }
    let mut f = source("xerox", b, n + 1, l, budget)?;
    f.properties.push(Property::new(
        "xerox.strike_flags",
        PropertyValue::Unsigned(u64::from(flags)),
    ));
    for i in 0..=n {
        let dummy = i == n;
        let mapped = if let Some(map) = map {
            let value = u16_at(map, i * 2, ByteOrder::Big)?;
            if value & 0x8000 != 0 {
                if dummy {
                    return Err(invalid("Xerox StrikeIndex dummy cannot be absent"));
                }
                continue;
            }
            (usize::from(value >> 8), usize::from(value & 255))
        } else {
            (0, i)
        };
        let body = bodies
            .get(mapped.0)
            .ok_or_else(|| invalid("Xerox StrikeIndex references absent body"))?;
        if mapped.1 >= body.count {
            return Err(invalid("Xerox StrikeIndex references absent body code"));
        }
        let x1 = usize::from(u16_at(body.bounds, mapped.1 * 2, ByteOrder::Big)?);
        let x2 = usize::from(u16_at(body.bounds, (mapped.1 + 1) * 2, ByteOrder::Big)?);
        let (x, advance) = if let Some(spacing) = spacing {
            let s = &spacing[i * 2..i * 2 + 2];
            if s == [255, 255] {
                if dummy {
                    return Err(invalid("Xerox BITBLT dummy spacing cannot be absent"));
                }
                continue;
            }
            (left + i64::from(s[0]), i64::from(s[1]))
        } else {
            if x1 == x2 && !dummy {
                continue;
            }
            (0, (x2 - x1) as i64)
        };
        let w = (x2 - x1) as u32;
        let h = u32::from(body.ascent) + u32::from(body.descent);
        check(
            w as usize * h as usize,
            l.max_pixels_per_glyph,
            "Xerox BITBLT glyph pixels",
        )?;
        let row = (w as usize).div_ceil(8);
        budget.add(row * h as usize + 256)?;
        let mut packed = vec![0; row * h as usize];
        for y in 0..h as usize {
            for dx in 0..w as usize {
                let sx = x1 + dx;
                if body.raster[y * body.row + sx / 8] & (128 >> (sx % 8)) != 0 {
                    packed[y * row + dx / 8] |= 128 >> (dx % 8);
                }
            }
        }
        let bitmap = raster(w, h, &packed, l, budget)?;
        if dummy {
            let mut g = Glyph::new(bitmap);
            g.name = Some("dummy".into());
            g.metrics.bounds.x = x;
            g.metrics.bounds.y = -i64::from(body.descent);
            g.metrics.advance = Some(Vector::new(advance, 0));
            g.metrics.left_bearing = Some(x);
            g.metrics.right_bearing = Some(advance - x - i64::from(w));
            let id = f.add_glyph(g)?;
            f.metrics.default_glyph = Some(id);
        } else {
            glyph(
                &mut f,
                bitmap,
                u32::from(first) + i as u32,
                x,
                -i64::from(body.descent),
                advance,
            )?;
        }
    }
    metrics(&mut f, i64::from(ascent) + i64::from(descent));
    f.metrics.ascent = Some(i64::from(ascent));
    f.metrics.descent = Some(i64::from(descent));
    if let Some(raw) = bbox {
        f.metrics.bounds = Some(Bounds::new(
            u32::from(u16_at(raw, 4, ByteOrder::Big)?),
            u32::from(u16_at(raw, 6, ByteOrder::Big)?),
            left,
            i64::from(u16_at(raw, 2, ByteOrder::Big)? as i16),
        ));
    }
    Ok((set(vec![f], "xerox"), Report::default()))
}
fn width(g: Option<&Glyph>, kerned: bool) -> Result<usize> {
    let Some(g) = g else { return Ok(0) };
    if kerned {
        Ok(g.bitmap.width() as usize)
    } else {
        usize::try_from(
            g.metrics
                .advance
                .map_or(i64::from(g.bitmap.width()), |a| a.x),
        )
        .map_err(|_| invalid("Xerox PlainStrike needs nonnegative advances"))
    }
}
fn write_body(
    out: &mut Vec<u8>,
    glyphs: &[Option<&Glyph>],
    kerned: bool,
    asc: u16,
    desc: u16,
    v: &XeroxWriteOptions,
    budget: &mut Budget,
) -> Result<()> {
    let mut columns = 0usize;
    for g in glyphs {
        columns = columns
            .checked_add(width(*g, kerned)?)
            .ok_or_else(|| invalid("Xerox strike width overflow"))?;
    }
    if columns > 65535 {
        return Err(invalid("Xerox strike boundary exceeds u16"));
    }
    let row = columns.div_ceil(16) * 2;
    let height = u32::from(asc) + u32::from(desc);
    let bytes = 10 + row * height as usize + (glyphs.len() + 1) * 2;
    if bytes / 2 > 65535 {
        return Err(invalid("Xerox strike body exceeds u16 word length"));
    }
    let at = out.len();
    grow(out, bytes, v, budget)?;
    put16(out, at, (bytes / 2) as u16, ByteOrder::Big);
    put16(out, at + 2, asc, ByteOrder::Big);
    put16(out, at + 4, desc, ByteOrder::Big);
    put16(out, at + 8, (row / 2) as u16, ByteOrder::Big);
    let boundaries = at + 10 + row * height as usize;
    let mut x = 0;
    for (i, g) in glyphs.iter().enumerate() {
        put16(out, boundaries + i * 2, x as u16, ByteOrder::Big);
        let w = width(*g, kerned)?;
        let bx = if kerned {
            g.map_or(0, |g| g.metrics.bounds.x)
        } else {
            0
        };
        let packed = project(
            *g,
            Bounds::new(w as u32, height, bx, -i64::from(desc)),
            &v.limits,
            budget,
        )?;
        let stride = w.div_ceil(8);
        for y in 0..height as usize {
            for dx in 0..w {
                if packed[y * stride + dx / 8] & (128 >> (dx % 8)) != 0 {
                    out[at + 10 + y * row + (x + dx) / 8] |= 128 >> ((x + dx) % 8);
                }
            }
        }
        x += w;
    }
    put16(out, boundaries + glyphs.len() * 2, x as u16, ByteOrder::Big);
    Ok(())
}
pub(super) fn write(
    f: &Font,
    v: &XeroxWriteOptions,
    budget: &mut Budget,
    report: &mut Report,
) -> Result<Vec<u8>> {
    let kerned = matches!(
        v.bitblt_layout,
        XeroxBitbltLayout::Kerned | XeroxBitbltLayout::KernedIndex
    );
    let indexed = matches!(
        v.bitblt_layout,
        XeroxBitbltLayout::PlainIndex | XeroxBitbltLayout::KernedIndex
    );
    let slots = select(f, 65535, v.unicode_as_legacy, "xerox", report, budget)?;
    let dummy = f.metrics.default_glyph.and_then(|id| f.glyph(id));
    if let Some(id) = f.metrics.default_glyph {
        report.entries.retain(|e| {
            !(e.code == "xerox.write.unencoded_glyph" && e.location.glyph_index == Some(id.index()))
        });
    }
    let first = *slots
        .keys()
        .next()
        .ok_or_else(|| invalid("Xerox BITBLT needs encoded glyphs"))?;
    let last = *slots
        .keys()
        .next_back()
        .ok_or_else(|| invalid("Xerox BITBLT needs encoded glyphs"))?;
    let n = (last - first + 1) as usize;
    check(n + 1, v.limits.max_glyphs, "Xerox BITBLT output slots")?;
    let ascent = f.metrics.ascent.unwrap_or_else(|| {
        f.glyphs()
            .map(|(_, g)| g.metrics.bounds.y + i64::from(g.bitmap.height()))
            .max()
            .unwrap_or(0)
    });
    let descent = f.metrics.descent.unwrap_or_else(|| {
        -f.glyphs()
            .map(|(_, g)| g.metrics.bounds.y)
            .min()
            .unwrap_or(0)
    });
    let asc = u16::try_from(ascent).map_err(|_| invalid("Xerox BITBLT ascent exceeds u16"))?;
    let desc = u16::try_from(descent).map_err(|_| invalid("Xerox BITBLT descent exceeds u16"))?;
    let minx = f
        .glyphs()
        .map(|(_, g)| g.metrics.bounds.x)
        .min()
        .unwrap_or(0);
    let maxright = f
        .glyphs()
        .map(|(_, g)| g.metrics.bounds.x + i64::from(g.bitmap.width()))
        .max()
        .unwrap_or(minx);
    let mut maxwidth = 0i64;
    for g in slots.values().copied().chain(dummy) {
        let advance = g
            .metrics
            .advance
            .map_or(i64::from(g.bitmap.width()), |a| a.x);
        maxwidth = maxwidth.max(advance);
        if kerned && advance == 255 && g.metrics.bounds.x - minx == 255 {
            return Err(invalid(
                "Xerox spacing 255/255 is reserved for absent glyphs",
            ));
        }
        if advance < 0 || kerned && (advance > 255 || g.metrics.bounds.x - minx > 255) {
            return Err(invalid("Xerox kerned spacing must fit unsigned bytes"));
        }
    }
    let header = 8 + usize::from(kerned) * 8;
    let mut out = Vec::new();
    grow(&mut out, header, v, budget)?;
    put16(
        &mut out,
        0,
        0x8000 | if kerned { 0x1000 } else { 0 } | if indexed { 0x4000 } else { 0 },
        ByteOrder::Big,
    );
    put16(&mut out, 2, first as u16, ByteOrder::Big);
    put16(&mut out, 4, last as u16, ByteOrder::Big);
    put16(
        &mut out,
        6,
        u16::try_from(maxwidth).map_err(|_| invalid("Xerox maximum advance exceeds u16"))?,
        ByteOrder::Big,
    );
    if kerned {
        for (at, value) in [(8, minx), (10, -i64::from(desc))] {
            put16(
                &mut out,
                at,
                i16::try_from(value).map_err(|_| invalid("Xerox font bbox offset exceeds i16"))?
                    as u16,
                ByteOrder::Big,
            );
        }
        put16(
            &mut out,
            12,
            u16::try_from(maxright - minx)
                .map_err(|_| invalid("Xerox font bbox width exceeds u16"))?,
            ByteOrder::Big,
        );
        put16(
            &mut out,
            14,
            asc.checked_add(desc)
                .ok_or_else(|| invalid("Xerox font bbox height exceeds u16"))?,
            ByteOrder::Big,
        );
    }
    if indexed {
        let map_at = out.len() + 6;
        grow(&mut out, 6 + (n + 1) * 2, v, budget)?;
        put16(&mut out, header, asc, ByteOrder::Big);
        put16(&mut out, header + 2, desc, ByteOrder::Big);
        for i in 0..=n {
            put16(&mut out, map_at + i * 2, 0x8000, ByteOrder::Big);
        }
        budget.add((slots.len() + 1).saturating_mul(64))?;
        let mut groups: Vec<Vec<Option<&Glyph>>> = vec![Vec::new()];
        let mut columns = 0usize;
        for (cp, g) in slots
            .iter()
            .map(|(cp, g)| (Some(*cp), Some(*g)))
            .chain(std::iter::once((None, dummy)))
        {
            let w = width(g, kerned)?;
            let group = groups.len() - 1;
            let candidate = columns.saturating_add(w);
            let words = 5
                + candidate.div_ceil(16) * (usize::from(asc) + usize::from(desc))
                + groups[group].len()
                + 2;
            if groups[group].len() == 256 || candidate > 65535 || words > 65535 {
                if groups[group].is_empty() {
                    return Err(invalid(
                        "Xerox single character exceeds indexed strike body limits",
                    ));
                }
                if groups.len() == 128 {
                    return Err(invalid("Xerox StrikeIndex exceeds 128 bodies"));
                }
                groups.push(Vec::new());
                columns = 0;
            }
            let group = groups.len() - 1;
            let index = groups[group].len();
            let slot = cp.map_or(n, |cp| (cp - first) as usize);
            put16(
                &mut out,
                map_at + slot * 2,
                ((group as u16) << 8) | index as u16,
                ByteOrder::Big,
            );
            groups[group].push(g);
            columns += w;
        }
        put16(&mut out, header + 4, groups.len() as u16, ByteOrder::Big);
        for group in groups {
            write_body(&mut out, &group, kerned, asc, desc, v, budget)?;
        }
    } else {
        budget.items::<Option<&Glyph>>(n + 1)?;
        let mut arranged: Vec<_> = (first..=last).map(|cp| slots.get(&cp).copied()).collect();
        arranged.push(dummy);
        write_body(&mut out, &arranged, kerned, asc, desc, v, budget)?;
    }
    if kerned {
        let at = out.len();
        grow(&mut out, (n + 1) * 2, v, budget)?;
        out[at..].fill(255);
        for i in 0..=n {
            let g = if i == n {
                dummy
            } else {
                slots.get(&(first + i as u32)).copied()
            };
            if let Some(g) = g {
                let advance = g
                    .metrics
                    .advance
                    .map_or(i64::from(g.bitmap.width()), |a| a.x);
                out[at + i * 2] = (g.metrics.bounds.x - minx) as u8;
                out[at + i * 2 + 1] = advance as u8;
            } else if i == n {
                out[at + i * 2] = 0;
                out[at + i * 2 + 1] = 0;
            }
        }
    }
    Ok(out)
}
