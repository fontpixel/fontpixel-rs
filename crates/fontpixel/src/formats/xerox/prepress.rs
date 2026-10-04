//! AC typed indexes and word-addressed raster segments.
use super::*;
pub(super) fn read(b: &[u8], l: &ResourceLimits, budget: &mut Budget) -> Result<(FontSet, Report)> {
    let mut at = 0;
    let mut first = b.len();
    let mut names = BTreeMap::new();
    let mut records = Vec::new();
    let mut unknown = false;
    let mut terminated = false;
    while at < first {
        let tag = u16_at(b, at, ByteOrder::Big)?;
        let kind = tag >> 12;
        let words = usize::from(tag & 4095);
        if kind == 0 {
            if words > 1 {
                slice(b, at, words * 2)?;
            }
            terminated = true;
            break;
        }
        if words == 0 {
            return Err(invalid("Xerox AC index entry has zero length"));
        }
        let data = slice(b, at, words * 2)?;
        match kind {
            1 => {
                if data.len() != 24 || data[4] > 19 {
                    return Err(invalid("Xerox AC name index size is invalid"));
                }
                let code = u16_at(data, 2, ByteOrder::Big)?;
                budget.add(128 + usize::from(data[4]) * 2)?;
                if names
                    .insert(code, latin(&data[5..5 + usize::from(data[4])]))
                    .is_some()
                {
                    return Err(invalid("Xerox AC duplicate name code"));
                }
            }
            3 => {
                if data.len() != 22 {
                    return Err(invalid(
                        "Xerox AC character index length is not eleven words",
                    ));
                }
                let start = (u32_at(data, 10, ByteOrder::Big)? as usize)
                    .checked_mul(2)
                    .ok_or_else(|| invalid("Xerox AC segment offset overflow"))?;
                let len = (u32_at(data, 14, ByteOrder::Big)? as usize)
                    .checked_mul(2)
                    .ok_or_else(|| invalid("Xerox AC segment length overflow"))?;
                slice(b, start, len)?;
                if start < at + data.len() {
                    return Err(invalid("Xerox AC segment overlaps index"));
                }
                first = first.min(start);
                budget.add(64)?;
                records.push((at, start, len));
            }
            _ => {
                unknown = true;
            }
        }
        at += data.len();
    }
    if records.is_empty() {
        return Err(invalid("Xerox AC contains no raster segments"));
    }
    let mut fonts = Vec::new();
    let mut report = Report::default();
    let mut count = 0;
    if unknown {
        loss(
            &mut report,
            "ac_nonraster_indexes",
            "Non-raster index entries remain in raw metadata and have no bitmap-font equivalent.",
            0,
        );
    }
    if !terminated {
        report.push(ReportEntry {
            level: Level::Warning,
            impact: Impact::Repair,
            code: "xerox.ac_implicit_index_end".into(),
            location: Location::default(),
            message: "Accepted a legacy file whose first segment implicitly terminates its index."
                .into(),
        });
    }
    budget.items::<Font>(records.len())?;
    for (fi, (entry, start, len)) in records.into_iter().enumerate() {
        let h = &b[entry..entry + 22];
        if h[5] < h[4] {
            return Err(invalid("Xerox AC character range is inverted"));
        }
        let n = usize::from(h[5] - h[4]) + 1;
        count += n;
        check(count, l.max_glyphs, "Xerox AC aggregate slots")?;
        let segment = slice(b, start, len)?;
        slice(segment, 0, n * 20)?;
        let mut f = source("xerox", segment, n, l, budget)?;
        budget.add(512)?;
        f.properties.push(Property::new(
            "xerox.ac_header",
            PropertyValue::Bytes(h.to_vec()),
        ));
        f.properties.push(Property::new(
            "xerox.face",
            PropertyValue::Unsigned(u64::from(h[3])),
        ));
        let rotation = u16_at(h, 8, ByteOrder::Big)?;
        f.properties.push(Property::new(
            "xerox.rotation_minutes",
            PropertyValue::Unsigned(u64::from(rotation)),
        ));
        f.names.family = names.get(&u16::from(h[2])).cloned();
        f.metrics.point_size = Some(Rational::new(
            i64::from(u16_at(h, 6, ByteOrder::Big)?) * 72,
            2540,
        )?);
        let dx = u16_at(h, 18, ByteOrder::Big)?;
        let dy = u16_at(h, 20, ByteOrder::Big)?;
        if dx == 0 || dy == 0 {
            return Err(invalid("Xerox AC zero resolution"));
        }
        f.metrics.resolution = Some(Vector::new(i64::from(dx / 10), i64::from(dy / 10)));
        if dx % 10 != 0 || dy % 10 != 0 {
            loss(
                &mut report,
                "ac_fractional_resolution",
                "Tenths-of-DPI values are retained; integer model resolution is rounded down.",
                fi,
            );
        }
        if rotation != 0 {
            loss(
                &mut report,
                "ac_rotation",
                "The physical raster and signed advance vector are retained; the segment rotation in arc-minutes remains metadata because the model has no rotation field.",
                fi,
            );
        }
        let anchor = n * 16;
        let mut fractional = false;
        for i in 0..n {
            let cd = &segment[i * 16..(i + 1) * 16];
            let hgt = u16_at(cd, 14, ByteOrder::Big)? as i16;
            let ptr = u32_at(segment, anchor + i * 4, ByteOrder::Big)?;
            if hgt == -1 {
                if ptr != u32::MAX {
                    return Err(invalid("Xerox AC absent character has a raster pointer"));
                }
                continue;
            }
            if hgt < 0 || ptr == u32::MAX {
                return Err(invalid("Xerox AC character height/pointer mismatch"));
            }
            let p = anchor
                .checked_add(
                    (ptr as usize)
                        .checked_mul(2)
                        .ok_or_else(|| invalid("Xerox AC raster offset overflow"))?,
                )
                .ok_or_else(|| invalid("Xerox AC raster offset overflow"))?;
            if p < n * 20 {
                return Err(invalid("Xerox AC raster overlaps metrics or directory"));
            }
            let def = u16_at(segment, p, ByteOrder::Big)?;
            let row = usize::from(def >> 10) * 2;
            let w = u32::from(def & 1023);
            let height = u32::from(hgt as u16);
            if w != u32::from(u16_at(cd, 12, ByteOrder::Big)?)
                || row != height.div_ceil(16) as usize * 2
            {
                return Err(invalid(
                    "Xerox AC raster definition disagrees with character bounds",
                ));
            }
            let raw = slice(segment, p + 2, row * w as usize)?;
            check(
                w as usize * height as usize,
                l.max_pixels_per_glyph,
                "Xerox AC pixels",
            )?;
            let stride = (w as usize).div_ceil(8);
            budget.add(stride * height as usize + 512)?;
            let mut packed = vec![0; stride * height as usize];
            for y in 0..height as usize {
                let sx = height as usize - 1 - y;
                for x in 0..w as usize {
                    if raw[x * row + sx / 8] & (128 >> (sx % 8)) != 0 {
                        packed[y * stride + x / 8] |= 128 >> (x % 8);
                    }
                }
            }
            let wx = u32_at(cd, 0, ByteOrder::Big)? as i32;
            let wy = u32_at(cd, 4, ByteOrder::Big)? as i32;
            fractional |= wx % 65536 != 0 || wy % 65536 != 0;
            let advance = Vector::new(
                i64::from(wx).div_euclid(65536),
                i64::from(wy).div_euclid(65536),
            );
            let bitmap = raster(w, height, &packed, l, budget)?;
            let cp = u32::from(h[4]) + i as u32;
            glyph(
                &mut f,
                bitmap,
                cp,
                i64::from(u16_at(cd, 8, ByteOrder::Big)? as i16),
                i64::from(u16_at(cd, 10, ByteOrder::Big)? as i16),
                advance.x,
            )?;
            let id = f
                .glyph_id(Codepoint::Legacy(cp))
                .ok_or_else(|| invalid("Xerox AC glyph index"))?;
            f.edit_glyph(id, |g| {
                g.metrics.advance = Some(advance);
                g.properties.push(Property::new(
                    "xerox.width_fixed",
                    PropertyValue::List(
                        [wx, wy]
                            .into_iter()
                            .map(|v| PropertyValue::Integer(i64::from(v)))
                            .collect(),
                    ),
                ));
                Ok(())
            })?;
        }
        if fractional {
            loss(
                &mut report,
                "ac_fractional_width",
                "Signed 16.16 advances are retained; integer device advances are rounded down.",
                fi,
            );
        }
        let line = f
            .glyphs()
            .map(|(_, g)| i64::from(g.bitmap.height()))
            .max()
            .unwrap_or(0);
        metrics(&mut f, line);
        fonts.push(f);
    }
    let mut s = set(fonts, "xerox");
    budget.add(b.len() + 256)?;
    s.properties.push(Property::new(
        "xerox.ac_file",
        PropertyValue::Bytes(b.to_vec()),
    ));
    Ok((s, report))
}
fn fixed(g: &Glyph) -> Result<[i32; 2]> {
    let desired = g
        .metrics
        .advance
        .unwrap_or(Vector::new(i64::from(g.bitmap.width()), 0));
    if let Some(PropertyValue::List(v)) = g.properties.get("xerox.width_fixed") {
        if v.len() != 2 {
            return Err(invalid("Xerox AC fixed width needs two integers"));
        }
        let mut old = [0i32; 2];
        for (i, v) in v.iter().enumerate() {
            old[i] = match v {
                PropertyValue::Integer(n) => {
                    i32::try_from(*n).map_err(|_| invalid("Xerox AC fixed width exceeds i32"))?
                }
                _ => return Err(invalid("Xerox AC fixed width requires integers")),
            };
        }
        if i64::from(old[0]).div_euclid(65536) == desired.x
            && i64::from(old[1]).div_euclid(65536) == desired.y
        {
            return Ok(old);
        }
    }
    let cv = |n: i64| {
        n.checked_mul(65536)
            .and_then(|n| i32::try_from(n).ok())
            .ok_or_else(|| invalid("Xerox AC advance exceeds signed 16.16 range"))
    };
    Ok([cv(desired.x)?, cv(desired.y)?])
}
pub(super) fn write(
    s: &FontSet,
    v: &XeroxWriteOptions,
    budget: &mut Budget,
    report: &mut Report,
) -> Result<Vec<u8>> {
    if s.fonts.is_empty() {
        return Err(invalid("Xerox AC output needs fonts"));
    }
    let mut names = BTreeMap::new();
    budget.add(s.fonts.len().saturating_mul(256))?;
    for f in &s.fonts {
        let name = f.names.family.as_deref().unwrap_or("Font");
        if name.chars().count() > 19 || name.chars().any(|c| c as u32 > 255) {
            return Err(invalid(
                "Xerox AC family name exceeds nineteen Latin-1 bytes",
            ));
        }
        if !names.contains_key(name) {
            if names.len() == 256 {
                return Err(invalid("Xerox AC has more than 256 families"));
            }
            names.insert(name, names.len() as u8);
        }
    }
    let index_size = names.len() * 24 + s.fonts.len() * 22 + 2;
    output_limit(index_size, v.max_output_bytes, &v.limits, budget)?;
    let mut out = vec![0; index_size];
    let mut at = 0;
    for (&name, &code) in &names {
        put16(&mut out, at, 0x100c, ByteOrder::Big);
        put16(&mut out, at + 2, u16::from(code), ByteOrder::Big);
        out[at + 4] = name.chars().count() as u8;
        for (i, c) in name.chars().enumerate() {
            out[at + 5 + i] = c as u8;
        }
        at += 24;
    }
    put16(&mut out, index_size - 2, 1, ByteOrder::Big);
    let mut count = 0;
    for (fi, f) in s.fonts.iter().enumerate() {
        let slots = select(f, 255, v.unicode_as_legacy, "xerox", report, budget)?;
        let first = *slots
            .keys()
            .next()
            .ok_or_else(|| invalid("Xerox AC needs encoded glyphs"))?;
        let last = *slots
            .keys()
            .next_back()
            .ok_or_else(|| invalid("Xerox AC needs encoded glyphs"))?;
        let n = (last - first + 1) as usize;
        count += n;
        check(count, v.limits.max_glyphs, "Xerox AC aggregate slots")?;
        let entry = names.len() * 24 + fi * 22;
        let start = out.len();
        grow(&mut out, n * 20, v, budget)?;
        for i in 0..n {
            put16(&mut out, start + i * 16 + 14, u16::MAX, ByteOrder::Big);
            put32(&mut out, start + n * 16 + i * 4, u32::MAX, ByteOrder::Big);
        }
        for (&cp, g) in &slots {
            let bitmap = mono(g)?;
            let w = bitmap.width();
            let h = bitmap.height();
            if w > 1023 || h > 1008 {
                return Err(invalid(
                    "Xerox AC raster dimensions exceed 10-bit width or 6-bit word height",
                ));
            }
            let ci = (cp - first) as usize;
            let cd = start + ci * 16;
            let widths = fixed(g)?;
            put32(&mut out, cd, widths[0] as u32, ByteOrder::Big);
            put32(&mut out, cd + 4, widths[1] as u32, ByteOrder::Big);
            put16(
                &mut out,
                cd + 8,
                i16::try_from(g.metrics.bounds.x)
                    .map_err(|_| invalid("Xerox AC x offset exceeds i16"))? as u16,
                ByteOrder::Big,
            );
            put16(
                &mut out,
                cd + 10,
                i16::try_from(g.metrics.bounds.y)
                    .map_err(|_| invalid("Xerox AC y offset exceeds i16"))? as u16,
                ByteOrder::Big,
            );
            put16(&mut out, cd + 12, w as u16, ByteOrder::Big);
            put16(&mut out, cd + 14, h as u16, ByteOrder::Big);
            let row = h.div_ceil(16) as usize * 2;
            let p = out.len();
            put32(
                &mut out,
                start + n * 16 + ci * 4,
                u32::try_from((p - start - n * 16) / 2)
                    .map_err(|_| invalid("Xerox AC directory pointer exceeds u32"))?,
                ByteOrder::Big,
            );
            grow(&mut out, 2 + row * w as usize, v, budget)?;
            put16(
                &mut out,
                p,
                ((row as u16 / 2) << 10) | w as u16,
                ByteOrder::Big,
            );
            for y in 0..h as usize {
                let sx = h as usize - 1 - y;
                for x in 0..w as usize {
                    if bitmap.as_bytes()[y * bitmap.stride() + x / 8] & (128 >> (x % 8)) != 0 {
                        out[p + 2 + x * row + sx / 8] |= 128 >> (sx % 8);
                    }
                }
            }
        }
        put16(&mut out, entry, 0x300b, ByteOrder::Big);
        out[entry + 2] = *names
            .get(f.names.family.as_deref().unwrap_or("Font"))
            .ok_or_else(|| invalid("Xerox AC name lookup"))?;
        out[entry + 3] = u8::try_from(int(&f.properties, "xerox.face", 0)?)
            .map_err(|_| invalid("Xerox AC face exceeds u8"))?;
        out[entry + 4] = first as u8;
        out[entry + 5] = last as u8;
        let old = bytes(f, "xerox.ac_header").filter(|b| b.len() == 22);
        let size = if let Some(h) = old {
            u16_at(h, 6, ByteOrder::Big)?
        } else {
            let p = f.metrics.point_size.unwrap_or(Rational::integer(12));
            let n = i128::from(p.numerator()) * 2540 / (i128::from(p.denominator()) * 72);
            u16::try_from(n).map_err(|_| invalid("Xerox AC point size exceeds mica range"))?
        };
        put16(&mut out, entry + 6, size, ByteOrder::Big);
        let rotation = v.rotation_minutes.unwrap_or(
            u16::try_from(int(&f.properties, "xerox.rotation_minutes", 0)?)
                .map_err(|_| invalid("Xerox AC rotation exceeds u16"))?,
        );
        put16(&mut out, entry + 8, rotation, ByteOrder::Big);
        put32(
            &mut out,
            entry + 10,
            u32::try_from(start / 2)
                .map_err(|_| invalid("Xerox AC segment pointer exceeds u32"))?,
            ByteOrder::Big,
        );
        let len = out.len() - start;
        put32(
            &mut out,
            entry + 14,
            u32::try_from(len / 2).map_err(|_| invalid("Xerox AC segment length exceeds u32"))?,
            ByteOrder::Big,
        );
        let dpi = f.metrics.resolution.unwrap_or(Vector::new(300, 300));
        for (k, value) in [dpi.x, dpi.y].into_iter().enumerate() {
            let mut tenth = u16::try_from(
                value
                    .checked_mul(10)
                    .ok_or_else(|| invalid("Xerox AC DPI overflow"))?,
            )
            .map_err(|_| invalid("Xerox AC DPI exceeds tenths range"))?;
            if let Some(h) = old {
                let previous = u16_at(h, 18 + k * 2, ByteOrder::Big)?;
                if i64::from(previous / 10) == value {
                    tenth = previous;
                }
            }
            if tenth == 0 {
                return Err(invalid("Xerox AC zero resolution"));
            }
            put16(&mut out, entry + 18 + k * 2, tenth, ByteOrder::Big);
        }
    }
    Ok(out)
}
