//! IntMetrics v0/v2 tables, explicit sidecars and exact scalable vectors.
use super::*;
struct Metrics<'a> {
    name: &'a [u8],
    mapping: Option<&'a [u8]>,
    entries: Vec<[i16; 6]>,
    extra: bool,
}
fn parse<'a>(b: &'a [u8], l: &ResourceLimits, budget: &mut Budget) -> Result<Metrics<'a>> {
    slice(b, 0, 52)?;
    check(b.len(), l.max_table_bytes, "RISC OS IntMetrics bytes")?;
    if u32_at(b, 40, ByteOrder::Little)? != 16 || u32_at(b, 44, ByteOrder::Little)? != 16 {
        return Err(invalid("RISC OS IntMetrics scale words differ from 16"));
    }
    let version = b[49];
    if version > 2 {
        return Err(invalid("RISC OS IntMetrics version exceeds 2"));
    }
    let flags = if version < 2 {
        if b[50] != 0 || b[51] != 0 {
            return Err(invalid("RISC OS old IntMetrics reserved bytes are nonzero"));
        }
        0
    } else {
        b[50]
    };
    if flags & 0x90 != 0 {
        return Err(invalid("RISC OS IntMetrics reserved flags are set"));
    }
    let n = usize::from(b[48]) + usize::from(b[51]) * 256;
    check(n, l.max_glyphs, "RISC OS metric slots")?;
    budget.items::<[i16; 6]>(n)?;
    let mut entries = vec![[0; 6]; n];
    let mut at = 52;
    let mapsize = if flags & 32 != 0 {
        let size = usize::from(u16_at(b, at, ByteOrder::Little)?);
        at += 2;
        size
    } else {
        256
    };
    if mapsize > 0 && n > 256 {
        return Err(invalid(
            "RISC OS 8-bit metric map cannot index more than 256 slots",
        ));
    }
    let mapping = if mapsize == 0 {
        None
    } else {
        let map = slice(b, at, mapsize)?;
        at += mapsize;
        if map.iter().any(|v| *v != 0 && usize::from(*v) >= n) {
            return Err(invalid("RISC OS metric mapping exceeds table"));
        }
        Some(map)
    };
    for col in 0..6 {
        let omitted = if col < 4 {
            flags & 1 != 0
        } else if col == 4 {
            flags & 2 != 0
        } else {
            flags & 4 != 0
        };
        if omitted {
            continue;
        }
        let data = slice(b, at, n * 2)?;
        at += n * 2;
        for (i, entry) in entries.iter_mut().enumerate() {
            entry[col] = u16_at(data, i * 2, ByteOrder::Little)? as i16;
        }
    }
    let extra = flags & 8 != 0;
    if extra {
        let table = at;
        let offsets = [
            usize::from(u16_at(b, at, ByteOrder::Little)?),
            usize::from(u16_at(b, at + 2, ByteOrder::Little)?),
            usize::from(u16_at(b, at + 4, ByteOrder::Little)?),
            usize::from(u16_at(b, at + 6, ByteOrder::Little)?),
        ];
        if offsets[0] < 8 || offsets.windows(2).any(|p| p[0] > p[1]) || table + offsets[3] > b.len()
        {
            return Err(invalid("RISC OS IntMetrics extra area offsets are invalid"));
        }
        let misc = slice(b, table + offsets[0], offsets[1] - offsets[0])?;
        slice(misc, 0, 28)?;
        if flags & 2 != 0 {
            let x = u16_at(misc, 8, ByteOrder::Little)? as i16;
            for entry in &mut entries {
                entry[4] = x;
            }
        }
        if flags & 4 != 0 {
            let y = u16_at(misc, 10, ByteOrder::Little)? as i16;
            for entry in &mut entries {
                entry[5] = y;
            }
        }
        let kern = slice(b, table + offsets[1], offsets[2] - offsets[1])?;
        let codebytes = if flags & 64 != 0 { 2 } else { 1 };
        let mut k = 0;
        let code = |data: &[u8], at: usize| -> Result<u16> {
            if codebytes == 2 {
                u16_at(data, at, ByteOrder::Little)
            } else {
                Ok(u16::from(slice(data, at, 1)?[0]))
            }
        };
        if !kern.is_empty() {
            loop {
                let left = code(kern, k)?;
                k += codebytes;
                if left == 0 {
                    break;
                }
                loop {
                    let right = code(kern, k)?;
                    k += codebytes;
                    if right == 0 {
                        break;
                    }
                    let n = usize::from(flags & 2 == 0) * 2 + usize::from(flags & 4 == 0) * 2;
                    slice(kern, k, n)?;
                    k += n;
                }
            }
            if k != kern.len() {
                return Err(invalid("RISC OS kern data contains unused trailing bytes"));
            }
        }
        if offsets[2] != offsets[3] || table + offsets[3] != b.len() {
            return Err(invalid("RISC OS reserved metrics areas must be empty"));
        }
    } else if at != b.len() {
        return Err(invalid(
            "RISC OS bytes follow IntMetrics tables without extra-data flag",
        ));
    }
    Ok(Metrics {
        name: &b[..40],
        mapping,
        entries,
        extra,
    })
}
fn rounded(n: i128, d: i128) -> Result<i64> {
    if d <= 0 {
        return Err(invalid("RISC OS zero metric scale"));
    }
    let sign = if n < 0 { -1 } else { 1 };
    let a = n.abs();
    let q = a / d;
    let r = a % d;
    let v = q + if r * 2 > d || (r * 2 == d && q % 2 != 0) {
        1
    } else {
        0
    };
    i64::try_from(v * sign).map_err(|_| invalid("RISC OS metric overflow"))
}
fn scale(f: &Font) -> Result<(i128, i128, i128)> {
    let p = f
        .metrics
        .point_size
        .ok_or_else(|| invalid("RISC OS missing point size"))?;
    let dpi = f
        .metrics
        .resolution
        .ok_or_else(|| invalid("RISC OS missing resolution"))?;
    if p.numerator() <= 0 || dpi.x <= 0 || dpi.y <= 0 {
        return Err(invalid("RISC OS point size and DPI must be positive"));
    }
    let multiply = |a: i128, b: i128, c: i128| {
        a.checked_mul(b)
            .and_then(|v| v.checked_mul(c))
            .ok_or_else(|| invalid("RISC OS metric scale overflow"))
    };
    let horizontal = match f.properties.get("riscos.x_size") {
        Some(PropertyValue::Integer(x)) if *x > 0 => multiply(
            i128::from(*x),
            i128::from(dpi.x),
            i128::from(p.denominator()),
        )?,
        Some(_) => return Err(invalid("RISC OS horizontal point size must be positive")),
        None => multiply(i128::from(p.numerator()), i128::from(dpi.x), 16)?,
    };
    Ok((
        horizontal,
        multiply(i128::from(p.numerator()), i128::from(dpi.y), 16)?,
        i128::from(p.denominator()) * 16 * 72000,
    ))
}
pub(super) fn apply(
    s: &mut FontSet,
    raw: &[u8],
    l: &ResourceLimits,
    budget: &mut Budget,
    report: &mut Report,
) -> Result<()> {
    let m = parse(raw, l, budget)?;
    budget.add(raw.len() + 256)?;
    s.properties.push(Property::new(
        "riscos.intmetrics",
        PropertyValue::Bytes(raw.to_vec()),
    ));
    for (fi, f) in s.fonts.iter_mut().enumerate() {
        let name = latin(m.name).trim_end_matches('\r').to_string();
        budget.add(name.len() * 2 + 256)?;
        let (family, style) = name.split_once('.').unwrap_or((&name, ""));
        f.names.family = Some(family.into());
        if !style.is_empty() {
            f.names.style = Some(style.into());
        }
        let (xs, ys, denom) = scale(f)?;
        budget.items::<crate::model::GlyphId>(f.len())?;
        let ids: Vec<_> = f.glyphs().map(|(id, _)| id).collect();
        let mut missing = false;
        for id in ids {
            let g = f
                .glyph(id)
                .ok_or_else(|| invalid("RISC OS metric glyph index"))?;
            let cp = g
                .codepoints
                .iter()
                .find_map(|c| match c {
                    Codepoint::Legacy(v) => Some(*v as usize),
                    _ => None,
                })
                .ok_or_else(|| invalid("RISC OS glyph lacks native code"))?;
            let index = if let Some(map) = m.mapping {
                let Some(&i) = map.get(cp) else {
                    missing = true;
                    continue;
                };
                if i == 0 {
                    missing = true;
                    continue;
                }
                usize::from(i)
            } else {
                cp
            };
            let Some(entry) = m.entries.get(index) else {
                missing = true;
                continue;
            };
            budget.add(g.bitmap.as_bytes().len() + 512)?;
            f.edit_glyph(id, |g| {
                g.metrics.scalable_advance =
                    Some(Vector::new(i64::from(entry[4]), i64::from(entry[5])));
                let adv = Vector::new(
                    rounded(i128::from(entry[4]) * xs, denom)?,
                    rounded(i128::from(entry[5]) * ys, denom)?,
                );
                g.metrics.advance = Some(adv);
                g.metrics.right_bearing =
                    Some(adv.x - g.metrics.bounds.x - i64::from(g.bitmap.width()));
                g.properties.push(Property::new(
                    "riscos.scalable_bounds",
                    PropertyValue::List(
                        entry[..4]
                            .iter()
                            .map(|v| PropertyValue::Integer(i64::from(*v)))
                            .collect(),
                    ),
                ));
                Ok(())
            })?;
        }
        if missing {
            loss(
                report,
                "metrics_missing",
                "Some bitmap codes have no IntMetrics entry; their bitmap-derived advance remains unchanged.",
                fi,
            );
        }
        if m.extra {
            loss(
                report,
                "metrics_extra",
                "IntMetrics miscellaneous and kerning data are retained in the sidecar; the core model has no native representation for every field.",
                fi,
            );
        }
    }
    Ok(())
}
pub(super) fn encode(
    s: &FontSet,
    v: &RiscosWriteOptions,
    budget: &mut Budget,
    report: &mut Report,
) -> Result<Vec<u8>> {
    let f = s
        .fonts
        .first()
        .ok_or_else(|| invalid("RISC OS metric output needs a font"))?;
    let slots = select(f, 65534, v.unicode_as_legacy, "riscos", report, budget)?;
    let mapped = v.metrics_version == RiscosMetricsVersion::V0;
    let count = if mapped {
        if slots.len() > 254 || slots.keys().any(|c| *c > 255) {
            return Err(invalid(
                "RISC OS IntMetrics v0 needs at most 254 codes in 0..255",
            ));
        }
        slots.len() + 1
    } else {
        *slots
            .keys()
            .next_back()
            .ok_or_else(|| invalid("RISC OS metric output needs codes"))? as usize
            + 1
    };
    check(count, v.limits.max_glyphs, "RISC OS metric output slots")?;
    let base = if mapped { 308 } else { 54 };
    let size = base + count * 12;
    output_limit(size, v.max_output_bytes, &v.limits, budget)?;
    let mut out = vec![0; size];
    out[..40].fill(13);
    let name = if let Some(style) = &f.names.style {
        format!("{}.{}", f.names.family.as_deref().unwrap_or("Font"), style)
    } else {
        f.names.family.clone().unwrap_or_else(|| "Font".into())
    };
    if name.chars().count() > 40 || name.chars().any(|c| c as u32 > 255 || c == '\r') {
        return Err(invalid(
            "RISC OS IntMetrics name must be at most 40 non-CR Latin-1 bytes",
        ));
    }
    for (i, c) in name.chars().enumerate() {
        out[i] = c as u8;
    }
    put32(&mut out, 40, 16, ByteOrder::Little);
    put32(&mut out, 44, 16, ByteOrder::Little);
    out[48] = count as u8;
    out[49] = if mapped { 0 } else { 2 };
    out[50] = if mapped { 0 } else { 32 };
    out[51] = (count >> 8) as u8;
    let (xs, ys, denom) = scale(f)?;
    for (i, (&cp, g)) in slots.iter().enumerate() {
        let index = if mapped { i + 1 } else { cp as usize };
        if mapped {
            out[52 + cp as usize] = index as u8;
        }
        let bounds =
            if let Some(PropertyValue::List(values)) = g.properties.get("riscos.scalable_bounds") {
                if values.len() != 4 {
                    return Err(invalid("RISC OS scalable bbox needs four integers"));
                }
                let mut result = [0i64; 4];
                for (i, v) in values.iter().enumerate() {
                    result[i] = match v {
                        PropertyValue::Integer(n) => *n,
                        _ => return Err(invalid("RISC OS scalable bbox needs integer fields")),
                    };
                }
                result
            } else {
                [
                    rounded(i128::from(g.metrics.bounds.x) * denom, xs)?,
                    rounded(i128::from(g.metrics.bounds.y) * denom, ys)?,
                    rounded(
                        i128::from(g.metrics.bounds.x + i64::from(g.bitmap.width())) * denom,
                        xs,
                    )?,
                    rounded(
                        i128::from(g.metrics.bounds.y + i64::from(g.bitmap.height())) * denom,
                        ys,
                    )?,
                ]
            };
        let advance = if let Some(a) = g.metrics.scalable_advance {
            a
        } else {
            let a = g
                .metrics
                .advance
                .unwrap_or(Vector::new(i64::from(g.bitmap.width()), 0));
            Vector::new(
                rounded(i128::from(a.x) * denom, xs)?,
                rounded(i128::from(a.y) * denom, ys)?,
            )
        };
        for (col, value) in bounds.into_iter().chain([advance.x, advance.y]).enumerate() {
            put16(
                &mut out,
                base + (col * count + index) * 2,
                i16::try_from(value).map_err(|_| invalid("RISC OS scalable metric exceeds i16"))?
                    as u16,
                ByteOrder::Little,
            );
        }
    }
    Ok(out)
}
