//! Bounded native map expansion and half-cell assembly.
use super::*;
pub(super) fn parse(
    b: &[u8],
    l: &ResourceLimits,
    budget: &mut Budget,
) -> Result<(FontSet, Report)> {
    slice(b, 0, 32)?;
    if &b[..8] != b"VFNT0002" {
        return Err(invalid("not a VFNT0002 resource"));
    }
    check(b.len(), l.max_table_bytes, "VFNT file bytes")?;
    let w = u32::from(b[8]);
    let h = u32::from(b[9]);
    if w == 0 || h == 0 || w > 128 || h > 128 {
        return Err(invalid("VFNT native dimensions must be 1..128"));
    }
    let count = u32_at(b, 12, ByteOrder::Big)? as usize;
    if count == 0 || count > 131072 {
        return Err(invalid("VFNT native glyph count must be 1..131072"));
    }
    check(count, l.max_glyphs, "VFNT pool glyph count")?;
    let cell = w.div_ceil(8) as usize * h as usize;
    let pool_size = cell * count;
    if pool_size > 2097152 {
        return Err(invalid("VFNT pool exceeds native two-megabyte limit"));
    }
    let pool = slice(b, 32, pool_size)?;
    let mut counts = [0usize; 4];
    for (i, n) in counts.iter_mut().enumerate() {
        *n = u32_at(b, 16 + i * 4, ByteOrder::Big)? as usize;
        if *n > 65536 {
            return Err(invalid("VFNT mapping table exceeds native record limit"));
        }
    }
    let table = 32 + pool_size;
    if b.len() != table + counts.iter().sum::<usize>() * 8 {
        return Err(invalid("VFNT table lengths do not match resource"));
    }
    let mut expanded = 0usize;
    for p in (table..b.len()).step_by(8) {
        expanded = expanded
            .checked_add(usize::from(u16_at(b, p + 6, ByteOrder::Big)?) + 1)
            .ok_or_else(|| invalid("VFNT expansion overflow"))?;
    }
    budget.add(
        b.len()
            .saturating_mul(2)
            .saturating_add(expanded.saturating_mul(192))
            .saturating_add(count.saturating_mul(8)),
    )?;
    let mut maps: [BTreeMap<u32, u16>; 4] = std::array::from_fn(|_| BTreeMap::new());
    let mut used = BTreeSet::new();
    let mut at = table;
    for (map, &n) in maps.iter_mut().zip(&counts) {
        let mut previous = None;
        for _ in 0..n {
            let first = u32_at(b, at, ByteOrder::Big)?;
            let dst = usize::from(u16_at(b, at + 4, ByteOrder::Big)?);
            let length = usize::from(u16_at(b, at + 6, ByteOrder::Big)?) + 1;
            at += 8;
            let last = first
                .checked_add(length as u32 - 1)
                .ok_or_else(|| invalid("VFNT Unicode range overflow"))?;
            if last > 0x10ffff
                || (first <= 0xdfff && last >= 0xd800)
                || previous.is_some_and(|v| first <= v)
                || dst + length > count
            {
                return Err(invalid("invalid VFNT mapping range, order or destination"));
            }
            previous = Some(last);
            for i in 0..length {
                let index = (dst + i) as u16;
                map.insert(first + i as u32, index);
                used.insert(usize::from(index));
            }
        }
    }
    let bold = !maps[2].is_empty() || !maps[3].is_empty();
    let fonts = if bold { 2 } else { 1 };
    check(fonts, l.max_resources, "VFNT styles")?;
    budget.items::<Font>(fonts)?;
    let mut set = FontSet {
        source_format: Some("vtfont".into()),
        ..Default::default()
    };
    set.properties.push(Property::new(
        "vtfont.native",
        PropertyValue::Bytes(b.to_vec()),
    ));
    let mut total = 0usize;
    for style in 0..fonts {
        let mut codes = BTreeSet::new();
        codes.extend(maps[0].keys().copied());
        codes.extend(maps[1].keys().copied());
        if style == 1 {
            codes.extend(maps[2].keys().copied());
            codes.extend(maps[3].keys().copied());
        }
        let mut groups: BTreeMap<(u16, Option<u16>), Vec<u32>> = BTreeMap::new();
        groups.insert((0, None), Vec::new());
        for cp in codes {
            let normal = maps[0].get(&cp).copied().unwrap_or(0);
            let left = if style == 1 {
                maps[2]
                    .get(&cp)
                    .copied()
                    .filter(|&v| v != 0)
                    .unwrap_or(normal)
            } else {
                normal
            };
            let right = if maps[1].contains_key(&cp) || (style == 1 && maps[3].contains_key(&cp)) {
                let normal = maps[1].get(&cp).copied().unwrap_or(0);
                Some(if style == 1 {
                    maps[3]
                        .get(&cp)
                        .copied()
                        .filter(|&v| v != 0)
                        .unwrap_or(normal)
                } else {
                    normal
                })
            } else {
                None
            };
            groups.entry((left, right)).or_default().push(cp);
        }
        let unused = if style == 0 {
            (1..count).filter(|i| !used.contains(i)).count()
        } else {
            0
        };
        let ng = groups.len() + unused;
        total = total
            .checked_add(ng)
            .ok_or_else(|| invalid("VFNT model glyph count overflow"))?;
        check(total, l.max_glyphs, "VFNT model glyph count")?;
        budget.items::<Glyph>(ng)?;
        budget.add(ng.saturating_mul(256).saturating_add(1024))?;
        let mut f = Font::default();
        f.source_format = Some("vtfont".into());
        f.names.style = Some(if style == 0 { "Regular" } else { "Bold" }.into());
        f.charset.name = Some("unicode".into());
        f.metrics.bounds = Some(Bounds::new(w, h, 0, 0));
        f.metrics.pixel_size = Some(h);
        f.metrics.spacing = Some(Spacing::CharacterCell);
        f.properties
            .push(Property::new("vtfont.cell_width", i64::from(w)));
        f.properties
            .push(Property::new("vtfont.cell_height", i64::from(h)));
        for ((left, right), codes) in groups {
            let width = if right.is_some() { 2 * w } else { w };
            check(
                width as usize * h as usize,
                l.max_pixels_per_glyph,
                "VFNT glyph pixels",
            )?;
            let row = width.div_ceil(8) as usize;
            budget.add((row * h as usize + cell).saturating_mul(2))?;
            let mut bytes = vec![0; row * h as usize];
            let left_bm = Bitmap::<Mono>::from_raw(
                w,
                h,
                &pool[usize::from(left) * cell..(usize::from(left) + 1) * cell],
                bitmap_limits(l),
            )?;
            insert(&mut bytes, row, 0, 0, &left_bm);
            if let Some(index) = right {
                let right_bm = Bitmap::<Mono>::from_raw(
                    w,
                    h,
                    &pool[usize::from(index) * cell..(usize::from(index) + 1) * cell],
                    bitmap_limits(l),
                )?;
                insert(&mut bytes, row, w as usize, 0, &right_bm);
            }
            let mut g = Glyph::new(Bitmap::<Mono>::from_raw(
                width,
                h,
                &bytes,
                bitmap_limits(l),
            )?);
            g.metrics.advance = Some(Vector::new(i64::from(width), 0));
            g.codepoints = codes.into_iter().map(Codepoint::Unicode).collect();
            g.properties
                .push(Property::new("vtfont.left_index", i64::from(left)));
            if let Some(index) = right {
                g.properties
                    .push(Property::new("vtfont.right_index", i64::from(index)));
            }
            let id = f.add_glyph(g)?;
            if left == 0 && right.is_none() {
                f.metrics.default_glyph = Some(id);
            }
        }
        if style == 0 {
            for index in 1..count {
                if used.contains(&index) {
                    continue;
                }
                check(
                    w as usize * h as usize,
                    l.max_pixels_per_glyph,
                    "VFNT unused glyph pixels",
                )?;
                budget.add(cell.saturating_mul(2))?;
                let mut g = Glyph::new(Bitmap::<Mono>::from_raw(
                    w,
                    h,
                    &pool[index * cell..(index + 1) * cell],
                    bitmap_limits(l),
                )?);
                g.metrics.advance = Some(Vector::new(i64::from(w), 0));
                g.properties
                    .push(Property::new("vtfont.unused_index", index as i64));
                f.add_glyph(g)?;
            }
        }
        set.fonts.push(f);
    }
    Ok((set, Report::default()))
}
