use super::{PalmContainer, PalmReadOptions, PalmVersion, PalmWriteOptions, reader};
use crate::formats::WriteOptions;
use crate::formats::legacy_common::*;
use crate::model::{FontSet, Glyph, PropertyValue};
use crate::resources::{OutputBundle, Written};
use crate::{Report, Result};
use std::collections::BTreeMap;
const E: ByteOrder = ByteOrder::Big;

/// Encode native Palm resources; validate actual output under the same resource limits.
pub fn write(s: &FontSet, w: &WriteOptions, o: &PalmWriteOptions) -> Result<Written> {
    let mut report = Report::default();
    let result = encode(s, w, o, &mut report);
    finish(result, report, "palm", w.strict)
}
fn encode(
    s: &FontSet,
    w: &WriteOptions,
    o: &PalmWriteOptions,
    r: &mut Report,
) -> Result<OutputBundle> {
    let mut budget = source_budget(s, &o.limits)?;
    let fonts = if matches!(o.version, PalmVersion::Gray1 | PalmVersion::Gray2) {
        std::borrow::Cow::Borrowed(s)
    } else {
        convert_gray(s, o.gray_threshold, &o.limits, &mut budget)?
    };
    if fonts.fonts.is_empty() {
        return Err(invalid("Palm needs at least one font"));
    }
    check(fonts.fonts.len(), o.limits.max_resources, "Palm strikes")?;
    if !o.densities.is_empty() && o.densities.len() != fonts.fonts.len() {
        return Err(invalid("Palm density count must equal strike count"));
    }
    let mut groups: Vec<Vec<usize>> = Vec::new();
    if o.container == PalmContainer::Bare {
        groups.push((0..fonts.fonts.len()).collect());
    } else {
        let mut original_groups = BTreeMap::new();
        for (i, f) in fonts.fonts.iter().enumerate() {
            if matches!(
                o.version,
                PalmVersion::V2 | PalmVersion::Afnx | PalmVersion::FontSubst
            ) && f.properties.get("palm.density").is_some()
                && let (Some(PropertyValue::Integer(index)), Some(PropertyValue::Bytes(info))) = (
                    f.properties.get("palm.record_index"),
                    f.properties.get("palm.record_info"),
                )
            {
                let key = (*index, info.clone());
                let group = *original_groups.entry(key).or_insert_with(|| {
                    groups.push(Vec::new());
                    groups.len() - 1
                });
                groups[group].push(i);
            } else {
                groups.push(vec![i]);
            }
        }
    }
    if o.version == PalmVersion::V1 && groups.iter().any(|g| g.len() != 1) {
        return Err(invalid("Palm v1 bare output requires one strike"));
    }
    let mut records = Vec::new();
    if matches!(o.version, PalmVersion::Gray1 | PalmVersion::Gray2) {
        if o.container != PalmContainer::Prc {
            return Err(invalid("GrayFont requires a PRC resource database"));
        }
        records = super::gray::encode(&fonts, o, r, &mut budget)?;
        groups.clear();
    }
    for group in groups {
        let f = &fonts.fonts[group[0]];
        let mut info = match f.properties.get("palm.record_info") {
            Some(PropertyValue::Bytes(b))
                if b.len()
                    == if o.container == PalmContainer::Prc {
                        6
                    } else {
                        4
                    } =>
            {
                b.clone()
            }
            _ => {
                if o.container == PalmContainer::Prc {
                    let mut v = if o.version == PalmVersion::V1 {
                        b"NFNT".to_vec()
                    } else if matches!(o.version, PalmVersion::Afnx | PalmVersion::FontSubst) {
                        b"afnx".to_vec()
                    } else {
                        b"nfnt".to_vec()
                    };
                    v.extend_from_slice(
                        &u16::try_from(records.len().saturating_add(128))
                            .map_err(|_| invalid("Palm resource ID exceeds word"))?
                            .to_be_bytes(),
                    );
                    v
                } else {
                    (records.len() as u32 + 1).to_be_bytes().to_vec()
                }
            }
        };
        if o.container == PalmContainer::Prc
            && matches!(&info[..4], b"NFNT" | b"nfnt" | b"afnx" | b"GrFn")
        {
            info[..4].copy_from_slice(if o.version == PalmVersion::V1 {
                b"NFNT"
            } else if matches!(o.version, PalmVersion::Afnx | PalmVersion::FontSubst) {
                b"afnx"
            } else {
                b"nfnt"
            });
        }
        let payload = resource(&fonts, &group, o, r, &mut budget)?;
        let index = int(&f.properties, "palm.record_index", records.len() as i64)?;
        records.push((index, info, payload));
    }
    let bytes = if o.container == PalmContainer::Bare {
        records.remove(0).2
    } else {
        for p in fonts
            .properties
            .iter()
            .filter(|p| p.name == "palm.opaque_record")
        {
            if let PropertyValue::List(v) = &p.value
                && let [
                    PropertyValue::Unsigned(i),
                    PropertyValue::Bytes(info),
                    PropertyValue::Bytes(payload),
                ] = v.as_slice()
            {
                if matches!(o.version, PalmVersion::Gray1 | PalmVersion::Gray2)
                    && super::gray::bitmap_tag(info.get(..4).unwrap_or_default())
                {
                    continue;
                }
                if info.len()
                    == if o.container == PalmContainer::Prc {
                        6
                    } else {
                        4
                    }
                {
                    budget.add(info.len() + payload.len() + 64)?;
                    records.push((*i as i64, info.clone(), payload.clone()));
                }
            }
        }
        records.sort_by_key(|e| e.0);
        check(
            records.len(),
            o.limits.max_resources.min(65535),
            "Palm database records",
        )?;
        if o.container == PalmContainer::Prc {
            let mut identities = std::collections::BTreeSet::new();
            for (_, info, _) in &records {
                if !identities.insert(info.as_slice()) {
                    return Err(invalid("Palm resource identities collide after conversion"));
                }
            }
        }
        let entry_size = if o.container == PalmContainer::Prc {
            10
        } else {
            8
        };
        let mut n = 80 + records.len() * entry_size;
        let aux: Vec<&[u8]> = ["palm.app_info", "palm.sort_info"]
            .into_iter()
            .map(|k| match fonts.properties.get(k) {
                Some(PropertyValue::Bytes(v)) => v.as_slice(),
                _ => &[],
            })
            .collect();
        n = n.saturating_add(aux.iter().map(|b| b.len()).sum::<usize>());
        for (_, _, p) in &records {
            n = n.saturating_add(p.len());
        }
        output_limit(n, o)?;
        budget.add(n)?;
        let mut b = vec![0; n];
        if let Some(PropertyValue::Bytes(h)) = fonts.properties.get("palm.database_header") {
            if h.len() == 72 {
                b[..72].copy_from_slice(h);
            }
        } else {
            b[60..68].copy_from_slice(b"FontFont");
        }
        let family = fonts.fonts[0]
            .names
            .family
            .as_deref()
            .unwrap_or("fontpixel");
        let name: Vec<u8> = family
            .chars()
            .map(|c| u8::try_from(c as u32).unwrap_or(b'?'))
            .take(31)
            .collect();
        b[..32].fill(0);
        b[..name.len()].copy_from_slice(&name);
        let flags = u16_at(&b, 32, E)?;
        put16(
            &mut b,
            32,
            if o.container == PalmContainer::Prc {
                flags | 1
            } else {
                flags & !1
            },
            E,
        );
        put16(&mut b, 76, records.len() as u16, E);
        let mut at = 80 + records.len() * entry_size;
        for (i, a) in aux.into_iter().enumerate() {
            put32(
                &mut b,
                52 + 4 * i,
                if a.is_empty() { 0 } else { at as u32 },
                E,
            );
            b[at..at + a.len()].copy_from_slice(a);
            at += a.len();
        }
        for (i, (_, info, payload)) in records.iter().enumerate() {
            let e = 78 + i * entry_size;
            if o.container == PalmContainer::Prc {
                b[e..e + 6].copy_from_slice(info);
                put32(&mut b, e + 6, at as u32, E);
            } else {
                put32(&mut b, e, at as u32, E);
                b[e + 4..e + 8].copy_from_slice(info);
            }
            b[at..at + payload.len()].copy_from_slice(payload);
            at += payload.len();
        }
        b
    };
    output_limit(bytes.len(), o)?;
    let (decoded, rr) = reader::parse(
        &bytes,
        &PalmReadOptions {
            container: Some(o.container),
            ..Default::default()
        },
        &o.limits,
    )?;
    r.extend(rr);
    budget.add(source_budget(&decoded, &o.limits)?.used)?;
    audit(s, &decoded, "palm", r, &mut budget)?;
    Ok(OutputBundle::single(w.primary_name.clone(), bytes))
}
fn output_limit(n: usize, o: &PalmWriteOptions) -> Result<()> {
    check(
        n,
        o.max_output_bytes
            .min(o.limits.max_input_bytes)
            .min(u32::MAX as usize),
        "Palm output",
    )
}
fn unscale(n: i64, d: u16) -> Result<u16> {
    let num = n
        .checked_mul(72)
        .ok_or_else(|| invalid("Palm metric overflow"))?;
    if num < 0 || num % i64::from(d) != 0 {
        return Err(invalid(
            "Palm metrics must have exact nonnegative standard coordinates",
        ));
    }
    u16::try_from(num / i64::from(d)).map_err(|_| invalid("Palm metric exceeds 16 bits"))
}
struct Strike<'a> {
    slots: BTreeMap<u32, &'a Glyph>,
    default: Option<&'a Glyph>,
    density: u16,
    ascent: u16,
    descent: u16,
    height: u16,
    maxwidth: u16,
}
fn resource(
    s: &FontSet,
    indices: &[usize],
    o: &PalmWriteOptions,
    r: &mut Report,
    budget: &mut Budget,
) -> Result<Vec<u8>> {
    let e = if matches!(o.version, PalmVersion::Afnx | PalmVersion::FontSubst) {
        ByteOrder::Little
    } else {
        E
    };
    let mut strikes = Vec::new();
    let mut first = 65535;
    let mut last = 0;
    for &fi in indices {
        let f = &s.fonts[fi];
        let d = o.densities.get(fi).copied().unwrap_or(
            u16::try_from(int(&f.properties, "palm.density", 72)?)
                .map_err(|_| invalid("Palm invalid density"))?,
        );
        reader::density(d)?;
        if o.version == PalmVersion::V1 && d != 72 {
            return Err(invalid("Palm v1 supports density 72 only; select V2"));
        }
        let mut slots = BTreeMap::new();
        for (id, g) in f.glyphs() {
            if g.codepoints.is_empty() && f.metrics.default_glyph != Some(id) {
                budget.loss(
                    r,
                    "palm",
                    "write.unencoded_glyph",
                    Some(fi),
                    Some(id.index()),
                    "Unencoded glyph is not the missing character",
                )?;
            }
            for c in &g.codepoints {
                let v = c.value();
                if v > 65535
                    || matches!(c, crate::model::Codepoint::Unicode(_)) && !o.unicode_as_legacy
                {
                    budget.loss(
                        r,
                        "palm",
                        "write.mapping",
                        Some(fi),
                        Some(id.index()),
                        format!("Cannot encode {c:?} as Palm character"),
                    )?;
                } else {
                    budget.add(64)?;
                    if slots.insert(v, g).is_some() {
                        budget.loss(
                            r,
                            "palm",
                            "write.duplicate",
                            Some(fi),
                            Some(id.index()),
                            format!("Last glyph wins for {c:?}"),
                        )?;
                    }
                }
            }
        }
        if slots.is_empty() {
            return Err(invalid("Palm font has no representable character slots"));
        }
        first = first.min(
            *slots
                .first_key_value()
                .ok_or_else(|| invalid("empty Palm slots"))?
                .0,
        );
        last = last.max(
            *slots
                .last_key_value()
                .ok_or_else(|| invalid("empty Palm slots"))?
                .0,
        );
        let default = f.metrics.default_glyph.and_then(|id| f.glyph(id));
        let all = slots.values().copied().chain(default);
        let mut top = 0i64;
        let mut bottom = 0i64;
        let mut maxwidth = 0u16;
        for g in all {
            top = top.max(g.metrics.bounds.y + i64::from(g.metrics.bounds.height));
            bottom = bottom.min(g.metrics.bounds.y);
            maxwidth = maxwidth.max(unscale(i64::from(g.metrics.bounds.width), d)?);
        }
        let ascent = unscale(f.metrics.ascent.unwrap_or(top).max(top), d)?;
        let descent = unscale(f.metrics.descent.unwrap_or(-bottom).max(-bottom), d)?;
        let height = ascent
            .checked_add(descent)
            .ok_or_else(|| invalid("Palm height exceeds 16 bits"))?;
        strikes.push(Strike {
            slots,
            default,
            density: d,
            ascent,
            descent,
            height,
            maxwidth,
        });
    }
    let base = &strikes[0];
    let n = (last - first + 2) as usize;
    check(n, o.limits.max_glyphs, "Palm character span")?;
    check((n + 1) * 2, o.limits.max_table_bytes, "Palm table")?;
    budget.items::<u16>(n + 1)?;
    budget.add(n * 2)?;
    let mut locations = vec![0u16; n + 1];
    let mut widths = vec![255; n * 2];
    let mut x = 0u16;
    for i in 0..n {
        locations[i] = x;
        let g = if i + 1 == n {
            base.default
        } else {
            base.slots.get(&(first + i as u32)).copied()
        };
        if let Some(g) = g {
            let width = unscale(i64::from(g.metrics.bounds.width), base.density)?;
            x = x
                .checked_add(width)
                .ok_or_else(|| invalid("Palm strip exceeds 65535 columns"))?;
            let advance = unscale(
                g.metrics
                    .advance
                    .map_or(i64::from(g.metrics.bounds.width), |v| v.x),
                base.density,
            )?;
            widths[2 * i] = i8::try_from(int(&g.properties, "palm.ignored_offset", 0)?)
                .map_err(|_| invalid("Palm ignored offset exceeds signed byte"))?
                as u8;
            widths[2 * i + 1] =
                u8::try_from(advance).map_err(|_| invalid("Palm advance exceeds byte"))?;
        } else if i + 1 == n {
            widths[2 * i] = 0;
            widths[2 * i + 1] = 0;
        }
    }
    locations[n] = x;
    let row = usize::from(x).div_ceil(16) * 2;
    let head = if o.version == PalmVersion::V1 {
        26
    } else {
        if matches!(o.version, PalmVersion::Afnx | PalmVersion::FontSubst) {
            32 + 8 * strikes.len()
        } else {
            30 + 6 * strikes.len()
        }
    };
    let mut size = head + (n + 1) * 2 + n * 2 + 2;
    for st in &strikes {
        if st.ascent != base.ascent || st.descent != base.descent || st.height != base.height {
            return Err(invalid(
                "Palm V2 strikes must share standard-coordinate vertical metrics",
            ));
        }
        let scaled_row = row * usize::from(st.density) / 72;
        let height = usize::from(st.height) * usize::from(st.density) / 72;
        size = size.saturating_add(scaled_row.saturating_mul(height));
    }
    output_limit(size, o)?;
    check(size, o.limits.max_table_bytes, "Palm resource")?;
    budget.add(size)?;
    let mut b = vec![0; size];
    for (at, v) in [
        (
            0,
            if o.version == PalmVersion::V1 {
                0x9000
            } else if o.version == PalmVersion::FontSubst {
                0x0200
            } else {
                0x9200
            },
        ),
        (2, first as u16),
        (4, last as u16),
        (6, base.maxwidth),
        (12, base.maxwidth),
        (14, base.height),
        (18, base.ascent),
        (20, base.descent),
        (24, (row / 2) as u16),
    ] {
        put16(&mut b, at, v, e);
    }
    let f = &s.fonts[indices[0]];
    for (at, key) in [(8, "palm.kern_max"), (10, "palm.n_descent")] {
        put16(
            &mut b,
            at,
            i16::try_from(int(&f.properties, key, 0)?)
                .map_err(|_| invalid("Palm reserved metric exceeds signed word"))?
                as u16,
            e,
        );
    }
    put16(
        &mut b,
        22,
        unscale(f.metrics.line_gap.unwrap_or(0), base.density)?,
        e,
    );
    let loc = if o.version == PalmVersion::V1 {
        26 + row * usize::from(base.height)
    } else {
        head
    };
    let wo = loc + (n + 1) * 2;
    put16(
        &mut b,
        16,
        u16::try_from((wo - 16) / 2)
            .map_err(|_| invalid("Palm width table exceeds offset range"))?,
        e,
    );
    for (i, &v) in locations.iter().enumerate() {
        put16(&mut b, loc + i * 2, v, e);
    }
    b[wo..wo + n * 2].copy_from_slice(&widths);
    b[wo + n * 2..wo + n * 2 + 2].fill(255);
    if o.version != PalmVersion::V1 {
        put16(&mut b, 26, 1, e);
        put16(&mut b, 28, strikes.len() as u16, e);
    }
    let mut at = if o.version == PalmVersion::V1 {
        26
    } else {
        wo + n * 2 + 2
    };
    for (si, st) in strikes.iter().enumerate() {
        if o.version != PalmVersion::V1 {
            let base = if matches!(o.version, PalmVersion::Afnx | PalmVersion::FontSubst) {
                32 + 8 * si
            } else {
                30 + 6 * si
            };
            put16(&mut b, base, st.density, e);
            put32(
                &mut b,
                base + if matches!(o.version, PalmVersion::Afnx | PalmVersion::FontSubst) {
                    4
                } else {
                    2
                },
                at as u32,
                e,
            );
        }
        let stride = row * usize::from(st.density) / 72;
        let height = usize::from(st.height) * usize::from(st.density) / 72;
        let len = stride * height;
        for i in 0..n {
            let g = if i + 1 == n {
                st.default
            } else {
                st.slots.get(&(first + i as u32)).copied()
            };
            if let Some(g) = g {
                let width = unscale(i64::from(g.metrics.bounds.width), st.density)?;
                let advance = unscale(
                    g.metrics
                        .advance
                        .map_or(i64::from(g.metrics.bounds.width), |v| v.x),
                    st.density,
                )?;
                if width != locations[i + 1] - locations[i]
                    || u16::from(widths[2 * i + 1]) != advance
                {
                    return Err(invalid(
                        "Palm V2 strikes must share character locations and advances",
                    ));
                }
                if g.metrics.bounds.x != 0 {
                    return Err(invalid(
                        "Palm does not support nonzero glyph bearings; normalize glyph cells explicitly",
                    ));
                }
                let top = i64::from(st.ascent) * i64::from(st.density) / 72
                    - g.metrics.bounds.y
                    - i64::from(g.metrics.bounds.height);
                if top < 0 || top as u64 + u64::from(g.metrics.bounds.height) > height as u64 {
                    return Err(invalid("Palm glyph exceeds vertical cell"));
                }
                insert(
                    &mut b[at..at + len],
                    stride,
                    usize::from(locations[i]) * usize::from(st.density) / 72,
                    top as usize,
                    mono(g)?,
                );
            } else if widths[2 * i..2 * i + 2] != [255, 255] && i + 1 < n {
                return Err(invalid("Palm V2 strikes have different character coverage"));
            }
        }
        at += len;
    }
    Ok(b)
}
