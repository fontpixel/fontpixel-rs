//! Native pool interning and sorted range emission without quadratic alias scans.
use super::*;
fn native_cell(
    g: Option<&Glyph>,
    width: u32,
    height: u32,
    top: i64,
    l: &ResourceLimits,
    budget: &mut Budget,
) -> Result<Bitmap<Mono>> {
    check(
        width as usize * height as usize,
        l.max_pixels_per_glyph,
        "VFNT output cell pixels",
    )?;
    let row = width.div_ceil(8) as usize;
    let n = row * height as usize;
    budget.add(n.saturating_mul(2))?;
    let mut bytes = vec![0; n];
    if let Some(g) = g {
        let bm = mono(g)?;
        let x = g.metrics.bounds.x;
        let y = top - g.metrics.bounds.y - i64::from(bm.height());
        if x < 0
            || y < 0
            || x + i64::from(bm.width()) > i64::from(width)
            || y + i64::from(bm.height()) > i64::from(height)
        {
            return Err(invalid("VFNT source ink does not fit the selected cell"));
        }
        insert(&mut bytes, row, x as usize, y as usize, bm);
    }
    Bitmap::from_raw(width, height, &bytes, bitmap_limits(l))
}
fn intern(
    bytes: &[u8],
    nonzero: bool,
    pool: &mut Vec<Vec<u8>>,
    index: &mut BTreeMap<Vec<u8>, u16>,
    budget: &mut Budget,
    l: &ResourceLimits,
) -> Result<u16> {
    if let Some(&id) = index.get(bytes)
        && (!nonzero || id != 0)
    {
        return Ok(id);
    }
    if pool.len() >= 65536 {
        return Err(invalid(
            "VFNT writer cannot address more than 65536 raster cells",
        ));
    }
    check(pool.len() + 1, l.max_glyphs, "VFNT output pool glyph count")?;
    check(
        (pool.len() + 1).saturating_mul(bytes.len()),
        2097152,
        "VFNT native pool bytes",
    )?;
    budget.add(bytes.len().saturating_mul(4).saturating_add(192))?;
    let id = pool.len() as u16;
    pool.push(bytes.to_vec());
    index.insert(bytes.to_vec(), id);
    Ok(id)
}
pub(super) fn encode(
    s: &FontSet,
    w: &WriteOptions,
    o: &VtfontWriteOptions,
    r: &mut Report,
) -> Result<OutputBundle> {
    let mut budget = source_budget(s, &o.limits)?;
    if o.preserve_native
        && o.cell_width.is_none()
        && o.cell_height.is_none()
        && o.gray_threshold.is_none()
        && let Some(PropertyValue::Bytes(raw)) = s.properties.get("vtfont.native")
    {
        check(
            raw.len(),
            o.max_output_bytes.min(o.limits.max_input_bytes),
            "VFNT output bytes",
        )?;
        let mut b = Budget::new(raw.len(), &o.limits)?;
        let (back, rr) = reader::parse(raw, &o.limits, &mut b)?;
        budget.add(b.used)?;
        if &back == s {
            r.extend(rr);
            budget.add(raw.len())?;
            return Ok(OutputBundle::single(w.primary_name.clone(), raw.clone()));
        }
    }
    let converted = convert_gray(s, o.gray_threshold, &o.limits, &mut budget)?;
    if converted.fonts.is_empty() || converted.fonts.len() > 2 {
        return Err(invalid("VFNT needs one or two selected styles"));
    }
    let mut fonts: Vec<(usize, &Font)> = converted.fonts.iter().enumerate().collect();
    if fonts.len() == 2
        && fonts[0]
            .1
            .names
            .style
            .as_deref()
            .is_some_and(|v| v.eq_ignore_ascii_case("bold"))
    {
        fonts.swap(0, 1);
    }
    let f = fonts[0].1;
    let derived = f
        .metrics
        .bounds
        .map(|v| i64::from(v.width))
        .unwrap_or_else(|| {
            f.glyphs()
                .filter_map(|(_, g)| {
                    g.metrics
                        .advance
                        .map(|v| v.x)
                        .filter(|&v| v > 0)
                        .or_else(|| (g.bitmap.width() > 0).then_some(i64::from(g.bitmap.width())))
                })
                .min()
                .unwrap_or(0)
        });
    let width =
        o.cell_width
            .map(i64::from)
            .unwrap_or(int(&f.properties, "vtfont.cell_width", derived)?);
    let (mut bottom, mut top) = (0i64, 0i64);
    for (_, f) in &fonts {
        for (_, g) in f.glyphs() {
            bottom = bottom.min(g.metrics.bounds.y);
            top = top.max(g.metrics.bounds.y + i64::from(g.bitmap.height()));
        }
    }
    let height = o.cell_height.map(i64::from).unwrap_or(int(
        &f.properties,
        "vtfont.cell_height",
        top - bottom,
    )?);
    if !(1..=128).contains(&width) || !(1..=128).contains(&height) || top - bottom > height {
        return Err(invalid("VFNT cells must fit native 1..128 dimensions"));
    }
    top = bottom + height;
    let width = width as u32;
    let height = height as u32;
    let default = f
        .metrics
        .default_glyph
        .and_then(|id| f.glyph(id))
        .or_else(|| f.glyph_for_codepoint(Codepoint::Unicode(0xfffd)));
    let bm = native_cell(default, width, height, top, &o.limits, &mut budget)?;
    let mut pool = Vec::new();
    let mut index = BTreeMap::new();
    intern(
        bm.as_bytes(),
        false,
        &mut pool,
        &mut index,
        &mut budget,
        &o.limits,
    )?;
    let mut maps: [BTreeMap<u32, u16>; 4] = std::array::from_fn(|_| BTreeMap::new());
    for (position, (fi, f)) in fonts.iter().enumerate() {
        let bold = position == 1
            || (fonts.len() == 1
                && f.names
                    .style
                    .as_deref()
                    .is_some_and(|v| v.eq_ignore_ascii_case("bold")));
        let map = if bold { 2 } else { 0 };
        for (id, g) in f.glyphs() {
            let extent = g.metrics.bounds.x + i64::from(g.bitmap.width());
            let advance = g.metrics.advance.map_or(extent, |v| v.x);
            let full = if extent > i64::from(width) || advance > i64::from(width) {
                2 * width
            } else {
                width
            };
            if extent > i64::from(2 * width) || advance > i64::from(2 * width) {
                return Err(invalid("VFNT glyph needs more than two cells"));
            }
            let cell = native_cell(Some(g), full, height, top, &o.limits, &mut budget)?;
            let lhs = extract(
                cell.as_bytes(),
                cell.stride(),
                0,
                width,
                height,
                &o.limits,
                &mut budget,
            )?;
            let left = intern(
                lhs.as_bytes(),
                bold,
                &mut pool,
                &mut index,
                &mut budget,
                &o.limits,
            )?;
            let right = if full == 2 * width {
                let rhs = extract(
                    cell.as_bytes(),
                    cell.stride(),
                    width as usize,
                    width,
                    height,
                    &o.limits,
                    &mut budget,
                )?;
                Some(intern(
                    rhs.as_bytes(),
                    bold,
                    &mut pool,
                    &mut index,
                    &mut budget,
                    &o.limits,
                )?)
            } else {
                None
            };
            for &cp in &g.codepoints {
                if char::from_u32(cp.value()).is_none()
                    || matches!(cp, Codepoint::Legacy(_)) && !o.legacy_as_unicode
                {
                    budget.loss(r,"vtfont","write.mapping",Some(*fi),Some(id.index()),format!("Mapping {cp:?} needs explicit Unicode-domain permission or is not a scalar"))?;
                    continue;
                }
                budget.add(if right.is_some() { 128 } else { 64 })?;
                if maps[map].insert(cp.value(), left).is_some() {
                    budget.loss(
                        r,
                        "vtfont",
                        "write.duplicate",
                        Some(*fi),
                        Some(id.index()),
                        format!("Last glyph wins for {cp:?}"),
                    )?;
                }
                if let Some(right) = right {
                    maps[map + 1].insert(cp.value(), right);
                } else {
                    maps[map + 1].remove(&cp.value());
                }
            }
        }
    }
    let mut runs: [Vec<(u32, u16, u16)>; 4] = std::array::from_fn(|_| Vec::new());
    for (map, run) in maps.iter().zip(runs.iter_mut()) {
        budget.add(map.len().saturating_mul(24))?;
        for (&src, &dst) in map {
            if let Some(last) = run.last_mut()
                && last.0 + u32::from(last.2) + 1 == src
                && u32::from(last.1) + u32::from(last.2) + 1 == u32::from(dst)
                && last.2 < u16::MAX
            {
                last.2 += 1;
            } else {
                run.push((src, dst, 0));
            }
        }
        if run.len() > 65536 {
            return Err(invalid("VFNT native mapping-record count exceeded"));
        }
    }
    let cell = width.div_ceil(8) as usize * height as usize;
    let pool_size = pool.len() * cell;
    let size = 32 + pool_size + 8 * runs.iter().map(Vec::len).sum::<usize>();
    check(
        size,
        o.max_output_bytes
            .min(o.limits.max_input_bytes)
            .min(o.limits.max_table_bytes),
        "VFNT output bytes",
    )?;
    budget.add(size.saturating_mul(2))?;
    let mut out = vec![0; size];
    out[..8].copy_from_slice(b"VFNT0002");
    out[8] = width as u8;
    out[9] = height as u8;
    put32(&mut out, 12, pool.len() as u32, ByteOrder::Big);
    for (i, run) in runs.iter().enumerate() {
        put32(&mut out, 16 + i * 4, run.len() as u32, ByteOrder::Big);
    }
    let mut at = 32;
    for bytes in pool {
        out[at..at + cell].copy_from_slice(&bytes);
        at += cell;
    }
    for run in runs {
        for (src, dst, len) in run {
            put32(&mut out, at, src, ByteOrder::Big);
            put16(&mut out, at + 4, dst, ByteOrder::Big);
            put16(&mut out, at + 6, len, ByteOrder::Big);
            at += 8;
        }
    }
    let mut b = Budget::new(out.len(), &o.limits)?;
    let (back, rr) = reader::parse(&out, &o.limits, &mut b)?;
    r.extend(rr);
    budget.add(b.used)?;
    audit(s, &back, "vtfont", r, &mut budget)?;
    Ok(OutputBundle::single(w.primary_name.clone(), out))
}
