//! Original native BSWF table and fixed-point bitmap record construction.
use super::*;
fn bytes<'a>(f: &'a Font, key: &str) -> Option<&'a [u8]> {
    match f.properties.get(key) {
        Some(PropertyValue::Bytes(b)) => Some(b),
        _ => None,
    }
}
fn metric(header: &mut [u8], at: usize, value: i64) -> Result<()> {
    if rounded(fixed(header, at)?) != value {
        put_fixed(
            header,
            at,
            value
                .checked_mul(256)
                .ok_or_else(|| invalid("PC/GEOS metric overflow"))?,
        )?;
    }
    Ok(())
}
pub(super) fn encode(
    s: &FontSet,
    w: &WriteOptions,
    o: &PcgeosWriteOptions,
    r: &mut Report,
) -> Result<OutputBundle> {
    let mut budget = source_budget(s, &o.limits)?;
    for f in &s.fonts {
        for value in [
            f.metrics.ascent,
            f.metrics.descent,
            f.metrics.line_gap,
            f.metrics.line_height,
            f.metrics.underline_position,
            f.metrics.underline_thickness,
        ]
        .into_iter()
        .flatten()
        {
            if i32::try_from(value).is_err() {
                return Err(invalid(
                    "native font metrics exceed safe signed 32-bit coordinates",
                ));
            }
        }
    }

    let cap = o
        .max_output_bytes
        .min(o.limits.max_input_bytes)
        .min(o.limits.max_table_bytes);
    let variant = o.variant.unwrap_or(
        if s.properties.get("pcgeos.variant") == Some(&PropertyValue::Text("dbcs".into())) {
            PcgeosVariant::Dbcs
        } else {
            PcgeosVariant::Sbcs
        },
    );
    let dbcs = variant == PcgeosVariant::Dbcs;
    let ro = PcgeosReadOptions {
        variant: Some(variant),
        round_fractional_metrics: o.round_fractional_metrics
            || int(&s.properties, "pcgeos.round_fractional_metrics", 0)? != 0,
    };
    if o.preserve_native
        && o.variant.is_none()
        && o.font_id.is_none()
        && o.family.is_none()
        && o.gray_threshold.is_none()
        && let Some(PropertyValue::Bytes(raw)) = s.properties.get("pcgeos.native")
    {
        check(raw.len(), cap, "PC/GEOS output bytes")?;
        let mut rb = Budget::new(raw.len(), &o.limits)?;
        let (back, rr) = reader::parse(raw, &ro, &o.limits, &mut rb)?;
        budget.add(rb.used)?;
        if &back == s {
            r.extend(rr);
            return Ok(OutputBundle::single(w.primary_name.clone(), raw.clone()));
        }
    }
    let converted = convert_gray(s, o.gray_threshold, &o.limits, &mut budget)?;
    if converted.fonts.is_empty() {
        return Err(invalid("PC/GEOS output requires a font"));
    }
    let stride = if dbcs { 9 } else { 10 };
    let info_len = if dbcs { 57 } else { 33 };
    let header_len = info_len + converted.fonts.len() * stride;
    check(header_len, 65533, "PC/GEOS info offsets")?;
    check(8 + header_len, cap, "PC/GEOS output bytes")?;
    budget.add((8 + header_len) * 2)?;
    let mut out = vec![0; 8 + header_len];
    out[..4].copy_from_slice(b"BSWF");
    out[5] = if dbcs { 2 } else { 1 };
    put(&mut out, 6, header_len)?;
    if let Some(PropertyValue::Bytes(old)) = s.properties.get("pcgeos.info")
        && old.len() == info_len
    {
        out[8..8 + info_len].copy_from_slice(old);
    }
    let f = &converted.fonts[0];
    let id = o
        .font_id
        .map(i64::from)
        .unwrap_or(int(&f.properties, "pcgeos.font_id", 0)?);
    put(
        &mut out,
        8,
        usize::try_from(id).map_err(|_| invalid("negative PC/GEOS font ID"))?,
    )?;
    put(&mut out, 10, 0)?;
    out[12] = (out[12] & 0x0f)
        | 0x80
        | if matches!(
            f.metrics.spacing,
            Some(Spacing::Monospaced | Spacing::CharacterCell)
        ) {
            0x40
        } else {
            0
        };
    let family = o
        .family
        .as_deref()
        .or(f.names.family.as_deref())
        .unwrap_or("Fontpixel");
    let name_len = if dbcs { 40 } else { 20 };
    out[13..13 + name_len].fill(0);
    if dbcs {
        for (i, ch) in family.chars().take(19).enumerate() {
            let cp = u16::try_from(u32::from(ch))
                .map_err(|_| invalid("PC/GEOS DBCS family requires UCS-2 characters"))?;
            put(&mut out, 13 + 2 * i, usize::from(cp))?;
        }
    } else {
        let raw = latin_bytes(family)?;
        let n = raw.len().min(19);
        out[13..13 + n].copy_from_slice(&raw[..n]);
    }
    put(&mut out, 13 + name_len, info_len + 2)?;
    put(&mut out, 15 + name_len, header_len + 2)?;
    put(&mut out, 17 + name_len, 0)?;
    put(&mut out, 19 + name_len, 0)?;
    let mut low = 65535;
    let mut high = 0;
    for (i, f) in converted.fonts.iter().enumerate() {
        let (record, first, last) = font(f, dbcs, o, &mut budget)?;
        low = low.min(first);
        high = high.max(last);
        let at = 8 + info_len + i * stride;
        let mut style = u8::try_from(int(&f.properties, "pcgeos.style", 0)?)
            .map_err(|_| invalid("PC/GEOS style exceeds byte"))?;
        if let Some(name) = &f.names.style {
            let name = name.to_ascii_lowercase();
            style = style & !0x30
                | if name.contains("bold") { 0x20 } else { 0 }
                | if name.contains("italic") { 0x10 } else { 0 };
        }
        out[at] = style;
        let point = f.metrics.point_size.unwrap_or(Rational::integer(i64::from(
            f.metrics.pixel_size.unwrap_or(12),
        )));
        let numerator = i128::from(point.numerator()) * if dbcs { 1 } else { 256 };
        let denominator = i128::from(point.denominator());
        if numerator % denominator != 0 {
            return Err(invalid("PC/GEOS point size is not exactly representable"));
        }
        let value = i64::try_from(numerator / denominator)
            .map_err(|_| invalid("PC/GEOS point size overflow"))?;
        if dbcs {
            out[at + 1] = u8::try_from(value)
                .ok()
                .filter(|&v| v < 128)
                .ok_or_else(|| invalid("PC/GEOS DBCS point size must be 0..127"))?;
            out[at + 2] =
                u8::try_from(int(&f.properties, "pcgeos.charset", i64::from(first >> 8))?)
                    .map_err(|_| invalid("PC/GEOS charset exceeds byte"))?;
        } else {
            put_fixed(&mut out, at + 1, value)?;
        }
        put(&mut out, at + if dbcs { 3 } else { 4 }, record.len())?;
        let position = out.len();
        put32(
            &mut out,
            at + if dbcs { 5 } else { 6 },
            u32::try_from(position).map_err(|_| invalid("PC/GEOS file offset exceeds dword"))?,
            ByteOrder::Little,
        );
        check(
            out.len().saturating_add(record.len()),
            cap,
            "PC/GEOS output bytes",
        )?;
        budget.add(record.len())?;
        out.extend(record);
    }
    if dbcs {
        put(&mut out, 61, low as usize)?;
        put(&mut out, 63, high as usize)?;
    }
    let mut rb = Budget::new(out.len(), &o.limits)?;
    let (back, rr) = reader::parse(&out, &ro, &o.limits, &mut rb)?;
    budget.add(rb.used)?;
    r.extend(rr);
    audit(s, &back, "pcgeos", r, &mut budget)?;
    Ok(OutputBundle::single(w.primary_name.clone(), out))
}
fn font(
    f: &Font,
    dbcs: bool,
    o: &PcgeosWriteOptions,
    budget: &mut Budget,
) -> Result<(Vec<u8>, u32, u32)> {
    let mut codes = BTreeMap::new();
    let mut top = 0;
    let mut bottom = 0;
    for (_, g) in f.glyphs() {
        top = top.max(g.metrics.bounds.y + i64::from(g.bitmap.height()));
        bottom = bottom.min(g.metrics.bounds.y);
        for &cp in &g.codepoints {
            if dbcs && matches!(cp, Codepoint::Legacy(_)) && !o.legacy_as_unicode {
                return Err(invalid("PC/GEOS DBCS requires explicit legacy_as_unicode"));
            }
            if !dbcs && matches!(cp, Codepoint::Unicode(_)) && !o.unicode_as_legacy {
                return Err(invalid("PC/GEOS SBCS requires explicit unicode_as_legacy"));
            }
            if cp.value() > if dbcs { 65535 } else { 255 }
                || dbcs && char::from_u32(cp.value()).is_none()
            {
                return Err(invalid("codepoint outside selected PC/GEOS layout"));
            }
            budget.add(64)?;
            if codes.insert(cp.value(), g).is_some() {
                return Err(invalid("duplicate PC/GEOS codepoint"));
            }
        }
    }
    let first = *codes
        .first_key_value()
        .ok_or_else(|| invalid("PC/GEOS strike has no native codes"))?
        .0;
    let last = *codes.last_key_value().unwrap().0;
    let n = (last - first + 1) as usize;
    check(n, o.limits.max_glyphs, "PC/GEOS character slots")?;
    let hs = if dbcs { 70 } else { 71 };
    let cs = if dbcs { 6 } else { 8 };
    check(hs + n * cs, 65535, "PC/GEOS native FontBuf descriptors")?;
    budget.add((hs + n * cs) * 2)?;
    let mut out = vec![0; hs + n * cs];
    let source = bytes(f, "pcgeos.header").filter(|v| v.len() == hs);
    if let Some(old) = source {
        out[..hs].copy_from_slice(old);
    }
    let under = if dbcs { 46 } else { 43 };
    put(&mut out, 2, 0)?;
    if source.is_none() {
        put_fixed(&mut out, 25, top * 256)?;
        put_fixed(&mut out, 13, (top - bottom) * 256)?;
        put_fixed(&mut out, 28, -bottom * 256)?;
    }
    let desc = f.metrics.descent.unwrap_or(-bottom);
    metric(&mut out, 28, desc)?;
    if let Some(asc) = f.metrics.ascent {
        let old = fixed(&out, 13)? - fixed(&out, 16)? - fixed(&out, 28)?;
        if rounded(old) != asc {
            let q = asc * 256 + fixed(&out, 16)? + fixed(&out, 28)?;
            put_fixed(&mut out, 13, q)?;
        }
    }
    if let Some(gap) = f.metrics.line_gap {
        metric(&mut out, 31, gap)?;
    }
    let baseline = fixed(&out, 25)?;
    if let Some(pos) = f.metrics.underline_position
        && rounded(baseline - fixed(&out, under)?) != pos
    {
        put_fixed(&mut out, under, baseline - pos * 256)?;
    }
    if let Some(thickness) = f.metrics.underline_thickness {
        metric(&mut out, under + 3, thickness)?;
    }
    let default = f
        .metrics
        .default_character
        .map(Codepoint::value)
        .filter(|&v| (first..=last).contains(&v))
        .unwrap_or(first);
    if dbcs {
        put(&mut out, 40, first as usize)?;
        put(&mut out, 42, last as usize)?;
        put(&mut out, 44, default as usize)?;
    } else {
        out[40] = first as u8;
        out[41] = last as u8;
        out[42] = default as u8;
    }
    put(
        &mut out,
        hs - 5,
        f.metrics.pixel_size.unwrap_or((top - bottom) as u32) as usize,
    )?;
    out[hs - 3] &= !0x18;
    let mut pair_count = 0;
    let mut kern_start = Vec::new();
    let mut kern_end = Vec::new();
    match (
        bytes(f, "pcgeos.kern_pairs"),
        bytes(f, "pcgeos.kern_values"),
    ) {
        (Some(p), Some(v)) => {
            if p.len() != v.len() || p.len() % 2 != 0 {
                return Err(invalid(
                    "PC/GEOS pair/value tables must have matching word counts",
                ));
            }
            pair_count = p.len() / 2;
            check(
                out.len().saturating_add(p.len()).saturating_add(v.len()),
                65535,
                "PC/GEOS kerning tables",
            )?;
            put(&mut out, 34, pair_count)?;
            let at = out.len();
            put(&mut out, 36, at)?;
            budget.add(p.len() + v.len())?;
            out.extend(p);
            let at = out.len();
            put(&mut out, 38, at)?;
            out.extend(v);
            for pair in p.as_chunks::<2>().0 {
                kern_end.push(u32::from(pair[0]));
                kern_start.push(u32::from(pair[1]));
            }
        }
        (None, None) => {
            put(&mut out, 34, 0)?;
            let at = out.len();
            put(&mut out, 36, at)?;
            put(&mut out, 38, at)?;
        }
        _ => return Err(invalid("PC/GEOS requires both kerning tables")),
    }
    // Byte-code kerning membership is constant-size, avoiding a scan for every glyph.
    let mut first_kern = [false; 256];
    let mut second_kern = [false; 256];
    for cp in kern_start {
        first_kern[cp as usize] = true;
    }
    for cp in kern_end {
        second_kern[cp as usize] = true;
    }
    let mut advance_sum = 0i64;
    let mut max_advance = 0i64;
    for i in 0..n {
        let p = hs + i * cs;
        let cp = first + i as u32;
        let Some(&g) = codes.get(&cp) else {
            out[p + 5] = 8;
            continue;
        };
        let bm = mono(g)?;
        if bm.width() > 255 || bm.height() > 255 {
            return Err(invalid("PC/GEOS glyph dimensions exceed bytes"));
        }
        let y = rounded(baseline) - g.metrics.bounds.y - i64::from(bm.height());
        let x = i8::try_from(g.metrics.bounds.x)
            .map_err(|_| invalid("PC/GEOS x bearing exceeds signed byte"))?;
        let y = i8::try_from(y).map_err(|_| invalid("PC/GEOS y offset exceeds signed byte"))?;
        let advance = g.metrics.advance.map_or(i64::from(bm.width()), |v| v.x);
        let old = int(&g.properties, "pcgeos.advance_q8", advance * 256)?;
        let q = if rounded(old) == advance {
            old
        } else {
            advance * 256
        };
        put_fixed(&mut out, p + 2, q)?;
        advance_sum += q;
        max_advance = max_advance.max(q);
        let mut flags = u8::try_from(int(&g.properties, "pcgeos.flags", 0)?)
            .map_err(|_| invalid("PC/GEOS glyph flags exceed byte"))?;
        flags = flags & !0x40 | if x < 0 { 0x40 } else { 0 };
        if pair_count > 0 && cp < 256 {
            flags = flags & !6
                | if first_kern[cp as usize] { 4 } else { 0 }
                | if second_kern[cp as usize] { 2 } else { 0 };
        }
        if flags & 8 != 0 && bm.as_bytes().iter().any(|&v| v != 0) {
            flags &= !8;
        }
        out[p + 5] = flags;
        if !dbcs {
            put(
                &mut out,
                p + 6,
                usize::try_from(int(&g.properties, "pcgeos.usage", 0)?)
                    .map_err(|_| invalid("negative PC/GEOS usage"))?,
            )?;
        }
        let len = 4 + if flags & 8 != 0 {
            0
        } else {
            bm.as_bytes().len().max(1)
        };
        check(
            out.len().saturating_add(len),
            65535,
            "PC/GEOS FontBuf bytes",
        )?;
        let at = out.len();
        put(&mut out, p, at)?;
        budget.add(len.saturating_mul(2))?;
        out.extend([bm.width() as u8, bm.height() as u8, y as u8, x as u8]);
        if flags & 8 == 0 {
            if bm.as_bytes().is_empty() {
                out.push(0);
            } else {
                out.extend(bm.as_bytes());
            }
        }
    }
    if source.is_none() {
        put_fixed(&mut out, 4, advance_sum / codes.len() as i64)?;
        put_fixed(&mut out, 7, max_advance)?;
    }
    let len = out.len();
    put(&mut out, 0, len)?;
    Ok((out, first, last))
}
