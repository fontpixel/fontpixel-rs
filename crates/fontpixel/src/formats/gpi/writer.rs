//! Original GPI bitmap and descriptor encoding with native record preservation.
use super::*;
fn bytes_property<'a>(f: &'a Font, key: &str) -> Option<&'a [u8]> {
    match f.properties.get(key) {
        Some(PropertyValue::Bytes(v)) => Some(v),
        _ => None,
    }
}
fn kind(f: &Font, o: &GpiWriteOptions) -> Result<GpiKind> {
    Ok(o.kind.unwrap_or(match int(&f.properties, "gpi.kind", 3)? {
        1 => GpiKind::Fixed,
        2 => GpiKind::Proportional,
        3 => GpiKind::Abc,
        _ => return Err(invalid("invalid native GPI kind property")),
    }))
}
pub(super) fn encode(
    s: &FontSet,
    w: &WriteOptions,
    o: &GpiWriteOptions,
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
    let ro = GpiReadOptions {
        include_unmapped_null: int(&s.properties, "gpi.include_unmapped_null", 1)? != 0,
    };
    if o.preserve_native
        && o.container.is_none()
        && o.kind.is_none()
        && o.codepage.is_none()
        && o.registry_id.is_none()
        && o.gray_threshold.is_none()
        && let Some(PropertyValue::Bytes(raw)) = s.properties.get("gpi.native")
    {
        check(raw.len(), cap, "GPI output bytes")?;
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
        return Err(invalid("GPI output requires a font"));
    }
    let pack = o
        .container
        .unwrap_or(match s.properties.get("gpi.container") {
            Some(PropertyValue::Text(v)) if v == "ne" => GpiContainer::Ne,
            Some(PropertyValue::Text(v)) if v == "lx" => GpiContainer::Lx,
            _ => {
                if converted.fonts.len() == 1 {
                    GpiContainer::Bare
                } else {
                    GpiContainer::Lx
                }
            }
        });
    budget.items::<(u16, Vec<u8>)>(converted.fonts.len())?;
    let mut resources = Vec::new();
    let mut ids = BTreeSet::new();
    for (i, f) in converted.fonts.iter().enumerate() {
        let id = u16::try_from(int(&f.properties, "gpi.resource_id", (i + 1) as i64)?)
            .map_err(|_| invalid("GPI resource ID exceeds word"))?;
        if !ids.insert(id) {
            return Err(invalid("duplicate GPI resource ID"));
        }
        resources.push((id, font(f, o, cap, &mut budget)?));
    }
    let out = container::wrap(&resources, pack, cap, &mut budget)?;
    check(out.len(), cap, "GPI output bytes")?;
    let mut rb = Budget::new(out.len(), &o.limits)?;
    let (back, rr) = reader::parse(&out, &ro, &o.limits, &mut rb)?;
    budget.add(rb.used)?;
    r.extend(rr);
    audit(s, &back, "gpi", r, &mut budget)?;
    Ok(OutputBundle::single(w.primary_name.clone(), out))
}
fn font(f: &Font, o: &GpiWriteOptions, cap: usize, budget: &mut Budget) -> Result<Vec<u8>> {
    let kind = kind(f, o)?;
    let mut codes = BTreeMap::new();
    let mut null = None;
    let mut ascent = f.metrics.ascent.unwrap_or(0).max(0);
    let mut descent = f.metrics.descent.unwrap_or(0).max(0);
    let mut fixed = 0i64;
    for (_, g) in f.glyphs() {
        ascent = ascent.max(g.metrics.bounds.y + i64::from(g.bitmap.height()));
        descent = descent.max(-g.metrics.bounds.y);
        fixed = fixed.max(
            g.metrics
                .advance
                .map_or(i64::from(g.bitmap.width()), |v| v.x),
        );
        if g.codepoints.is_empty() && null.replace(g).is_some() {
            return Err(invalid("GPI permits one extra unmapped .null glyph"));
        }
        for &cp in &g.codepoints {
            if cp.value() > 65535 || matches!(cp, Codepoint::Unicode(_)) && !o.unicode_as_legacy {
                return Err(invalid(
                    "GPI output requires word legacy codes (or explicit unicode_as_legacy)",
                ));
            }
            budget.add(64)?;
            if codes.insert(cp.value(), g).is_some() {
                return Err(invalid("duplicate GPI codepoint"));
            }
        }
    }
    let first = *codes
        .first_key_value()
        .ok_or_else(|| invalid("GPI font has no encoded glyphs"))?
        .0;
    let last = *codes.last_key_value().unwrap().0;
    let count = (last - first + 1) as usize;
    let n = count + usize::from(null.is_some());
    check(n, o.limits.max_glyphs, "GPI output glyphs")?;
    let height = ascent + descent;
    if height > 32767 || ascent > 32767 {
        return Err(invalid("GPI cell exceeds signed word"));
    }
    let minimum = if kind == GpiKind::Abc { 10 } else { 6 };
    let extra = f
        .glyphs()
        .filter_map(|(_, g)| match g.properties.get("gpi.descriptor_extra") {
            Some(PropertyValue::Bytes(v)) => Some(v.len()),
            _ => None,
        })
        .max()
        .unwrap_or(0);
    let cell = extra
        .checked_add(minimum)
        .ok_or_else(|| invalid("GPI character descriptor stride overflow"))?;
    check(cell, 65535, "GPI character descriptor stride")?;
    let msize = bytes_property(f, "gpi.metrics")
        .map_or(168, |v| v.len())
        .max(168);
    let dpos = msize
        .checked_add(20)
        .ok_or_else(|| invalid("GPI definition offset overflow"))?;
    let definition_len = n
        .checked_mul(cell)
        .and_then(|len| len.checked_add(28))
        .ok_or_else(|| invalid("GPI descriptor table size overflow"))?;
    let record_len = dpos
        .checked_add(definition_len)
        .ok_or_else(|| invalid("GPI output record size overflow"))?;
    // Sparse codes and preserved descriptor tails multiply independently.
    // Reject the complete table before allocating either output record.
    check(record_len, cap, "GPI output record bytes")?;
    budget.add(
        msize
            .saturating_mul(2)
            .saturating_add(n.saturating_mul(128)),
    )?;
    budget.add(
        definition_len
            .checked_mul(2)
            .ok_or(crate::Error::ResourceLimit {
                resource: "legacy aggregate bytes",
                limit: budget.max,
            })?,
    )?;
    let mut metrics = vec![0; msize];
    if let Some(v) = bytes_property(f, "gpi.metrics") {
        metrics[..v.len()].copy_from_slice(v);
    }
    p32(&mut metrics, 0, 1)?;
    p32(&mut metrics, 4, msize)?;
    for (at, name) in [
        (8, f.names.family.as_deref()),
        (40, f.names.full.as_deref().or(f.names.family.as_deref())),
    ] {
        if let Some(name) = name {
            let raw = latin_bytes(name)?;
            metrics[at..at + 32].fill(0);
            let len = raw.len().min(31);
            metrics[at..at + len].copy_from_slice(&raw[..len]);
        }
    }
    for (at, v) in [
        (
            72,
            o.registry_id
                .map(i64::from)
                .unwrap_or(int(&f.properties, "gpi.registry_id", 0)?),
        ),
        (
            74,
            o.codepage
                .map(i64::from)
                .unwrap_or(int(&f.properties, "gpi.codepage", 850)?),
        ),
    ] {
        p16(
            &mut metrics,
            at,
            u16::try_from(v).map_err(|_| invalid("GPI identity exceeds word"))?,
        );
    }
    for (at, v) in [
        (76, i64::from(f.metrics.pixel_size.unwrap_or(height as u32))),
        (80, ascent),
        (82, descent),
        (84, f.metrics.ascent.unwrap_or(ascent)),
        (86, f.metrics.descent.unwrap_or(descent)),
        (90, f.metrics.line_gap.unwrap_or(0)),
        (98, height),
        (152, f.metrics.underline_thickness.unwrap_or(0)),
        (154, -f.metrics.underline_position.unwrap_or(0)),
    ] {
        ps(&mut metrics, at, v)?;
    }
    if let Some(res) = f.metrics.resolution {
        ps(&mut metrics, 110, res.x)?;
        ps(&mut metrics, 112, res.y)?;
    }
    p16(&mut metrics, 114, first as u16);
    p16(&mut metrics, 116, (count - 1) as u16);
    let default = f
        .metrics
        .default_character
        .map(Codepoint::value)
        .filter(|v| (first..=last).contains(v))
        .unwrap_or(first);
    p16(&mut metrics, 118, (default - first) as u16);
    let point = if let Some(p) = f.metrics.point_size {
        let num = i128::from(p.numerator()) * 10;
        let den = i128::from(p.denominator());
        if num % den != 0 {
            return Err(invalid("GPI point size requires exact tenths"));
        }
        u16::try_from(num / den).map_err(|_| invalid("GPI point size exceeds word"))?
    } else {
        u16::try_from(height * 10).map_err(|_| invalid("GPI point size exceeds word"))?
    };
    for at in [122, 124, 126] {
        p16(&mut metrics, at, point);
    }
    let mut type_flags = u16v(&metrics, 128)?;
    if kind == GpiKind::Fixed {
        type_flags |= 1
    } else {
        type_flags &= !1
    }
    p16(&mut metrics, 128, type_flags);
    let definition_flags = u16v(&metrics, 130)? & !1;
    p16(&mut metrics, 130, definition_flags);
    let style = f.names.style.as_deref().unwrap_or("").to_ascii_lowercase();
    let selection = u16v(&metrics, 132)? & !0x21
        | if style.contains("bold") { 0x20 } else { 0 }
        | if style.contains("italic") { 1 } else { 0 };
    p16(&mut metrics, 132, selection);
    let mut definition = Vec::new();
    definition
        .try_reserve_exact(definition_len)
        .map_err(|_| crate::Error::ResourceLimit {
            resource: "GPI descriptor allocation",
            limit: budget.max,
        })?;
    definition.resize(definition_len, 0);
    if let Some(v) = bytes_property(f, "gpi.definition") {
        if v.len() != 28 {
            return Err(invalid("GPI definition property must be 28 bytes"));
        }
        definition[..28].copy_from_slice(v);
    }
    p32(&mut definition, 0, 2)?;
    p16(
        &mut definition,
        8,
        if kind == GpiKind::Fixed { 0x47 } else { 0x42 },
    );
    p16(
        &mut definition,
        10,
        if kind == GpiKind::Abc { 0xb8 } else { 0x81 },
    );
    p16(&mut definition, 12, cell as u16);
    ps(&mut definition, 14, fixed)?;
    ps(&mut definition, 16, height)?;
    ps(&mut definition, 18, fixed)?;
    ps(&mut definition, 26, ascent)?;
    for i in 0..n {
        let glyph = if i == count {
            null
        } else {
            codes.get(&(first + i as u32)).copied()
        };
        let p = 28 + i * cell;
        let Some(g) = glyph else {
            continue;
        };
        let bm = mono(g)?;
        let left = if kind == GpiKind::Abc {
            0
        } else {
            g.metrics.bounds.x.max(0) as usize
        };
        let width = if kind == GpiKind::Abc {
            bm.width() as usize
        } else {
            (i64::from(bm.width()) + left as i64)
                .max(g.metrics.advance.map_or(0, |v| v.x))
                .max(0) as usize
        };
        if width > 32767 {
            return Err(invalid("GPI bitmap width exceeds signed native metrics"));
        }
        if kind == GpiKind::Abc {
            ps(&mut definition, p + 4, g.metrics.bounds.x)?;
            ps(&mut definition, p + 6, width as i64)?;
            ps(
                &mut definition,
                p + 8,
                g.metrics
                    .advance
                    .map_or(width as i64 + g.metrics.bounds.x, |v| v.x)
                    - g.metrics.bounds.x
                    - width as i64,
            )?;
        } else {
            p16(&mut definition, p + 4, width as u16);
        }
        if let Some(PropertyValue::Bytes(v)) = g.properties.get("gpi.descriptor_extra") {
            definition[p + minimum..p + minimum + v.len()].copy_from_slice(v);
        }
        if g.properties.get("gpi.missing") == Some(&PropertyValue::Integer(1))
            && bm.as_bytes().iter().all(|&v| v == 0)
        {
            continue;
        }
        check(
            width.saturating_mul(height as usize),
            o.limits.max_pixels_per_glyph,
            "GPI output glyph pixels",
        )?;
        let stride = width.div_ceil(8);
        let size = stride * height as usize;
        let offset = definition.len();
        check(
            dpos.saturating_add(offset).saturating_add(size),
            cap,
            "GPI output record bytes",
        )?;
        budget.add(size.saturating_mul(2))?;
        definition.resize(offset + size, 0);
        p32(&mut definition, p, dpos + offset)?;
        let top = ascent - g.metrics.bounds.y - i64::from(bm.height());
        if top < 0 || top + i64::from(bm.height()) > height {
            return Err(invalid("GPI cell clips glyph vertically"));
        }
        for y in 0..bm.height() as usize {
            for (x, &v) in bm.as_bytes()[y * bm.stride()..(y + 1) * bm.stride()]
                .iter()
                .enumerate()
            {
                let col = left / 8 + x;
                let shift = left % 8;
                definition[offset + col * height as usize + top as usize + y] |= v >> shift;
                if shift != 0 && col + 1 < stride {
                    definition[offset + (col + 1) * height as usize + top as usize + y] |=
                        v << (8 - shift);
                }
            }
        }
    }
    let dlen = definition.len();
    p32(&mut definition, 4, dlen)?;
    let tail = bytes_property(f, "gpi.tail").unwrap_or(&[255, 255, 255, 255, 8, 0, 0, 0]);
    if bytes_property(f, "gpi.tail").is_none() {
        p16(&mut metrics, 160, 0);
    }
    let total = 20 + metrics.len() + definition.len() + tail.len();
    check(total, cap, "GPI output bytes")?;
    budget.add(total.saturating_mul(2))?;
    let mut out = vec![0; 20];
    p32(&mut out, 0, 0xfffffffe)?;
    p32(&mut out, 4, 20)?;
    out[8..18].copy_from_slice(b"OS/2 FONT\0");
    out.extend(metrics);
    out.extend(definition);
    out.extend(tail);
    Ok(out)
}
