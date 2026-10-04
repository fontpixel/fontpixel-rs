//! Original bounded strip construction and native CVT/Unicode directory output.
use super::*;
#[derive(Clone, Copy)]
struct Leaf {
    record: u8,
    sector: u8,
    len: u16,
}
fn leaf_into(out: &mut [u8], at: usize, e: Leaf) {
    out[at] = e.record;
    out[at + 1] = e.sector;
    put16(out, at + 2, e.len, ByteOrder::Little);
}
fn variant(f: &Font, o: &GeosWriteOptions) -> GeosVariant {
    o.variant.unwrap_or(match f.properties.get("geos.variant") {
        Some(PropertyValue::Text(s)) => match s.as_str() {
            "extended" => GeosVariant::Extended,
            "utf8" => GeosVariant::Utf8,
            "mega" => GeosVariant::Mega,
            "mega-utf8" => GeosVariant::MegaUtf8,
            _ => GeosVariant::Standard,
        },
        _ => GeosVariant::Standard,
    })
}
fn unicode(v: GeosVariant) -> bool {
    matches!(v, GeosVariant::Utf8 | GeosVariant::MegaUtf8)
}
fn mega(v: GeosVariant) -> bool {
    matches!(v, GeosVariant::Mega | GeosVariant::MegaUtf8)
}
fn map_bytes(entries: &BTreeMap<u32, Leaf>, base: usize, budget: &mut Budget) -> Result<Vec<u8>> {
    // At most 32 + 16*64 + 5*64*64 block leaves, bounded by Unicode scalar range.
    budget.add(344)?;
    let mut out = vec![0; 172];
    let mut high: BTreeMap<u32, usize> = BTreeMap::new();
    let mut astral: BTreeMap<u32, usize> = BTreeMap::new();
    for (&block, &e) in entries {
        if block < 32 {
            leaf_into(&mut out, block as usize * 4, e);
            continue;
        }
        let group = block / 64;
        let p = if let Some(&p) = high.get(&group) {
            p
        } else {
            let pointer = if block < 1024 {
                128 + group as usize * 2
            } else {
                let a = group / 64;
                let table = if let Some(&p) = astral.get(&a) {
                    p
                } else {
                    let p = out.len();
                    put(&mut out, 160 + a as usize * 2, base + p)?;
                    budget.add(256)?;
                    out.resize(p + 128, 0);
                    astral.insert(a, p);
                    p
                };
                table + (group % 64) as usize * 2
            };
            let p = out.len();
            put(&mut out, pointer, base + p)?;
            budget.add(512)?;
            out.resize(p + 256, 0);
            high.insert(group, p);
            p
        };
        leaf_into(&mut out, p + (block % 64) as usize * 4, e);
    }
    check(
        base.saturating_add(out.len()),
        65535,
        "GEOS Unicode map offsets",
    )?;
    Ok(out)
}
struct StripPolicy {
    ascent: u32,
    height: u32,
    extended: bool,
    index_only: bool,
    subset: Option<usize>,
}
fn strip(
    glyphs: &[Option<&Glyph>],
    p: &StripPolicy,
    l: &ResourceLimits,
    budget: &mut Budget,
) -> Result<Vec<u8>> {
    let header = if p.extended { 14 } else { 8 };
    let mut widths = Vec::with_capacity(glyphs.len());
    budget.add(glyphs.len().saturating_mul(48))?;
    let mut total = 0usize;
    for (i, g) in glyphs.iter().enumerate() {
        let outside = p.subset.is_some_and(|part| i / 16 != part);
        let w = if outside {
            1
        } else if let Some(g) = g {
            if p.extended {
                g.bitmap.width() as usize
            } else {
                let advance = g
                    .metrics
                    .advance
                    .map_or(i64::from(g.bitmap.width()), |v| v.x);
                (i64::from(g.bitmap.width()) + g.metrics.bounds.x.max(0))
                    .max(advance)
                    .max(0) as usize
            }
        } else {
            0
        };
        total = total
            .checked_add(w)
            .ok_or_else(|| invalid("GEOS strip width overflow"))?;
        check(total, 65535, "GEOS strip bit offsets")?;
        widths.push(w);
    }
    let stride = total.div_ceil(8);
    let xo = header;
    let ko = xo + 2 * (glyphs.len() + 1);
    let bo = ko + if p.extended { 2 * glyphs.len() } else { 0 };
    let pixels = if p.index_only {
        0
    } else {
        stride * p.height as usize
    };
    let len = bo + pixels;
    check(len, 64770, "GEOS VLIR record bytes")?;
    check(len, l.max_table_bytes, "GEOS strip table bytes")?;
    budget.add(len.saturating_mul(2))?;
    let mut out = vec![0; len];
    out[0] = (p.ascent - 1) as u8;
    put(&mut out, 1, stride)?;
    out[3] = p.height as u8;
    put(&mut out, 4, xo)?;
    put(&mut out, 6, bo)?;
    if p.extended {
        put(
            &mut out,
            8,
            0xa000 | if glyphs.len() < 95 { 0x4000 } else { 0 },
        )?;
        put(&mut out, 10, ko)?;
        put(&mut out, 12, len)?;
    }
    let mut x = 0;
    for (i, (g, &width)) in glyphs.iter().zip(&widths).enumerate() {
        put(&mut out, xo + 2 * i, x)?;
        if !p.subset.is_some_and(|part| i / 16 != part)
            && let Some(g) = g
        {
            let bm = mono(g)?;
            if p.extended {
                out[ko + 2 * i] = i8::try_from(g.metrics.bounds.x)
                    .map_err(|_| invalid("GEOS offset exceeds signed byte"))?
                    as u8;
                out[ko + 2 * i + 1] =
                    u8::try_from(g.metrics.advance.map_or(i64::from(bm.width()), |v| v.x))
                        .map_err(|_| invalid("GEOS advance exceeds unsigned byte"))?;
            }
            if !p.index_only {
                let top = i64::from(p.ascent) - g.metrics.bounds.y - i64::from(bm.height());
                if top < 0 || top + i64::from(bm.height()) > i64::from(p.height) {
                    return Err(invalid("GEOS selected cell clips glyph vertically"));
                }
                let left = if p.extended {
                    0
                } else {
                    g.metrics.bounds.x.max(0) as usize
                };
                check(
                    width.saturating_mul(p.height as usize),
                    l.max_pixels_per_glyph,
                    "GEOS output glyph pixels",
                )?;
                insert(&mut out[bo..], stride, x + left, top as usize, bm);
            }
        }
        x += width;
    }
    put(&mut out, xo + 2 * glyphs.len(), x)?;
    Ok(out)
}
fn append_map(b: &mut Vec<u8>, entries: &BTreeMap<u32, Leaf>, budget: &mut Budget) -> Result<()> {
    let at = b.len();
    let flags = usize::from(word(b, 8)?) | 0x1000;
    put(b, 8, flags)?;
    put(b, 12, at)?;
    b.extend(map_bytes(entries, at, budget)?);
    check(b.len(), 64770, "GEOS Unicode base record bytes")
}
pub(super) fn encode(
    s: &FontSet,
    w: &WriteOptions,
    o: &GeosWriteOptions,
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

    let include_del = o
        .include_del
        .unwrap_or(int(&s.properties, "geos.include_del", 0)? != 0);
    let ro = GeosReadOptions { include_del };
    let cap = o
        .max_output_bytes
        .min(o.limits.max_input_bytes)
        .min(o.limits.max_table_bytes);
    if o.preserve_native
        && o.container.is_none()
        && o.variant.is_none()
        && o.font_id.is_none()
        && o.family.is_none()
        && o.gray_threshold.is_none()
        && let Some(PropertyValue::Bytes(raw)) = s.properties.get("geos.native")
    {
        check(raw.len(), cap, "GEOS output bytes")?;
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
        return Err(invalid("GEOS output requires a font"));
    }
    let container = o.container.unwrap_or(
        if s.properties.get("geos.container") == Some(&PropertyValue::Text("bare".into())) {
            GeosContainer::Bare
        } else {
            GeosContainer::Cvt
        },
    );
    if container == GeosContainer::Bare
        && (converted.fonts.len() != 1
            || mega(variant(&converted.fonts[0], o))
            || unicode(variant(&converted.fonts[0], o)))
    {
        return Err(invalid(
            "GEOS bare output requires one non-MEGA, non-Unicode strike",
        ));
    }
    budget.items::<Vec<u8>>(127)?;
    let mut records: Vec<Vec<u8>> = vec![Vec::new(); 127];
    let has_mega = converted.fonts.iter().any(|f| mega(variant(f, o)));
    let mut reserved = BTreeSet::from([126usize]);
    if has_mega {
        reserved.extend(48..=54);
    }
    let mut assigned = Vec::new();
    let mut seen_mega = false;
    for f in &converted.fonts {
        if mega(variant(f, o)) {
            if seen_mega {
                return Err(invalid("GEOS CVT has one MEGA slot family"));
            }
            seen_mega = true;
            assigned.push(48);
            continue;
        }
        let wanted = usize::try_from(int(
            &f.properties,
            "geos.record",
            i64::from(f.metrics.pixel_size.unwrap_or(12).clamp(1, 63)),
        )?)
        .map_err(|_| invalid("negative GEOS record slot"))?;
        let slot = if wanted < 126 && !reserved.contains(&wanted) {
            wanted
        } else {
            (1..64)
                .find(|i| !reserved.contains(i))
                .ok_or_else(|| invalid("GEOS native point-size slots exhausted"))?
        };
        reserved.insert(slot);
        assigned.push(slot);
    }
    let mut global: Vec<Leaf> = Vec::new();
    for (f, &slot) in converted.fonts.iter().zip(&assigned) {
        let v = variant(f, o);
        let extended = v != GeosVariant::Standard;
        let mut codes = BTreeMap::new();
        let mut ascent = f.metrics.ascent.unwrap_or(0).max(1);
        let mut descent = f.metrics.descent.unwrap_or(0).max(0);
        for (_, g) in f.glyphs() {
            ascent = ascent.max(g.metrics.bounds.y + i64::from(g.bitmap.height()));
            descent = descent.max(-g.metrics.bounds.y);
            for &cp in &g.codepoints {
                if unicode(v) && matches!(cp, Codepoint::Legacy(_)) && !o.legacy_as_unicode {
                    return Err(invalid(
                        "GEOS Unicode output requires explicit legacy_as_unicode",
                    ));
                }
                if !unicode(v) && matches!(cp, Codepoint::Unicode(_)) && !o.unicode_as_legacy {
                    return Err(invalid(
                        "GEOS byte-coded output requires explicit unicode_as_legacy",
                    ));
                }
                let cp = cp.value();
                if cp < 32
                    || (!unicode(v) && cp > if include_del { 127 } else { 126 })
                    || char::from_u32(cp).is_none()
                {
                    return Err(invalid("codepoint is outside selected GEOS variant"));
                }
                if cp == 127 && !include_del {
                    return Err(invalid("DEL output requires include_del"));
                }
                budget.add(64)?;
                if codes.insert(cp, g).is_some() {
                    return Err(invalid("duplicate GEOS codepoint"));
                }
            }
        }
        let height = ascent + descent;
        if ascent > 256 || height > 255 || height == 0 {
            return Err(invalid("GEOS ascent or height exceeds native byte"));
        }
        let mut p = StripPolicy {
            ascent: ascent as u32,
            height: height as u32,
            extended,
            index_only: false,
            subset: None,
        };
        let count = if include_del || mega(v) { 96 } else { 95 };
        budget.add(count * 16)?;
        let base: Vec<_> = (32..32 + count as u32)
            .map(|cp| codes.get(&cp).copied())
            .collect();
        let mut entries = BTreeMap::new();
        if unicode(v) {
            let blocks: BTreeSet<_> = codes
                .keys()
                .copied()
                .filter(|&cp| cp >= 128)
                .map(|cp| cp / 64)
                .collect();
            budget.add(blocks.len().saturating_mul(64))?;
            let mut current = None;
            for block in blocks {
                budget.add(64 * 16)?;
                let chars: Vec<_> = (0..64)
                    .map(|i| codes.get(&(block * 64 + i)).copied())
                    .collect();
                let data = strip(&chars, &p, &o.limits, &mut budget)?;
                if data.len() > 64770 {
                    return Err(invalid("GEOS Unicode sub-strike exceeds VLIR capacity"));
                }
                let ri = if let Some(i) = current
                    .filter(|&i: &usize| records[i].len().div_ceil(254) * 254 + data.len() <= 64770)
                {
                    i
                } else {
                    let i = (64..126)
                        .chain(1..64)
                        .find(|i| !reserved.contains(i))
                        .ok_or_else(|| invalid("GEOS Unicode auxiliary record slots exhausted"))?;
                    reserved.insert(i);
                    current = Some(i);
                    i
                };
                let start = records[ri].len().div_ceil(254) * 254;
                budget.add(start - records[ri].len() + data.len())?;
                records[ri].resize(start, 0);
                records[ri].extend(&data);
                let e = Leaf {
                    record: ri as u8,
                    sector: (start / 254) as u8,
                    len: data.len() as u16,
                };
                budget.add(128)?;
                entries.insert(block, e);
                global.push(e);
            }
        }
        if mega(v) {
            for part in 0..6 {
                p.subset = Some(part);
                records[48 + part] = strip(&base, &p, &o.limits, &mut budget)?;
            }
            p.subset = None;
            p.index_only = true;
            records[54] = strip(&base, &p, &o.limits, &mut budget)?;
            if unicode(v) {
                append_map(&mut records[54], &entries, &mut budget)?;
            }
        } else {
            records[slot] = strip(&base, &p, &o.limits, &mut budget)?;
            if unicode(v) {
                append_map(&mut records[slot], &entries, &mut budget)?;
            }
        }
    }
    if !global.is_empty() {
        check((global.len() + 1) * 4, 64770, "GEOS Unicode index bytes")?;
        budget.add((global.len() + 1) * 8)?;
        records[126] = vec![0; (global.len() + 1) * 4];
        for (i, &e) in global.iter().enumerate() {
            leaf_into(&mut records[126], 4 * i, e);
        }
    }
    let out = if container == GeosContainer::Bare {
        std::mem::take(&mut records[assigned[0]])
    } else {
        let n = records.iter().try_fold(762usize, |n, r| {
            n.checked_add(r.len().div_ceil(254) * 254)
                .ok_or_else(|| invalid("GEOS output length overflow"))
        })?;
        check(n, cap, "GEOS output bytes")?;
        budget.add(n.saturating_mul(2))?;
        let mut out = vec![0; n];
        out[0] = 0x83;
        out[3..19].fill(0xa0);
        out[21] = 1;
        out[22] = 8;
        // Deterministic native timestamp: 1900-01-01 00:00.
        out[24] = 1;
        out[25] = 1;
        let family = o
            .family
            .as_deref()
            .or(converted.fonts[0].names.family.as_deref())
            .unwrap_or("Fontpixel");
        let name = latin_bytes(family)?;
        let len = name.len().min(16);
        out[3..3 + len].copy_from_slice(&name[..len]);
        out[30..58].copy_from_slice(b"PRG formatted GEOS file V1.0");
        put(&mut out, 28, n / 254 - 1)?;
        out[254] = 3;
        out[255] = 21;
        out[256] = 0xbf;
        out[254 + 66] = 0x83;
        out[254 + 67] = 8;
        out[254 + 68] = 1;
        out[254 + 75..254 + 95].fill(0);
        out[254 + 75..254 + 85].copy_from_slice(b"Fontpixel ");
        let id = o.font_id.map(i64::from).unwrap_or(int(
            &converted.fonts[0].properties,
            "geos.font_id",
            0,
        )?);
        if !(0..1024).contains(&id) {
            return Err(invalid("GEOS font identifier exceeds ten bits"));
        }
        put(&mut out, 254 + 126, id as usize)?;
        let registered: Vec<usize> = assigned
            .iter()
            .flat_map(|&i| {
                if has_mega && i == 48 {
                    (48..55).collect()
                } else {
                    vec![i]
                }
            })
            .collect();
        if registered.len() > 15 {
            return Err(invalid(
                "GEOS native info block holds at most fifteen size entries",
            ));
        }
        for (i, &slot) in registered.iter().enumerate() {
            if slot > 63 {
                return Err(invalid("GEOS info block point-size slot exceeds six bits"));
            }
            put(&mut out, 254 + 95 + 2 * i, records[slot].len())?;
            put(&mut out, 254 + 128 + 2 * i, ((id as usize) << 6) | slot)?;
        }
        let mut at = 762;
        for (i, data) in records.iter().enumerate() {
            if data.is_empty() {
                out[509 + 2 * i] = 255;
                continue;
            }
            let sectors = data.len().div_ceil(254);
            if sectors > 255 {
                return Err(invalid("GEOS VLIR record exceeds 255 sectors"));
            }
            out[508 + 2 * i] = sectors as u8;
            out[509 + 2 * i] = (255 - (sectors * 254 - data.len())) as u8;
            out[at..at + data.len()].copy_from_slice(data);
            at += sectors * 254;
        }
        out
    };
    check(out.len(), cap, "GEOS output bytes")?;
    let mut rb = Budget::new(out.len(), &o.limits)?;
    let (back, rr) = reader::parse(&out, &ro, &o.limits, &mut rb)?;
    budget.add(rb.used)?;
    r.extend(rr);
    audit(s, &back, "geos", r, &mut budget)?;
    Ok(OutputBundle::single(w.primary_name.clone(), out))
}
