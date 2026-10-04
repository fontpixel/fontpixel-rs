use super::{PalmContainer, PalmReadOptions, PalmRounding};
use crate::bitmap::Bounds;
use crate::formats::Confidence;
use crate::formats::legacy_common::*;
use crate::model::{Codepoint, Font, FontSet, Glyph, Property, PropertyValue, Vector};
use crate::resources::ReadContext;
use crate::{Report, ResourceLimits, Result};
use std::collections::{BTreeMap, BTreeSet};
const E: ByteOrder = ByteOrder::Big;

pub(super) fn detect(b: &[u8]) -> Confidence {
    if b.len() >= 78 && &b[60..68] == b"FontFont" {
        return Confidence::Certain;
    }
    if b.len() >= 26 && matches!(&b[..2], [0x90, 0] | [0x92, 0] | [0, 0x92] | [0, 0x02]) {
        return Confidence::Possible;
    }
    if b.len() >= 88
        && b[33] & 1 != 0
        && matches!(&b[78..82], b"NFNT" | b"nfnt" | b"afnx" | b"GrFn")
    {
        return Confidence::Possible;
    }
    Confidence::NoMatch
}
fn scale(n: u32, density: u16, rounding: PalmRounding) -> Result<u32> {
    let n = n * u32::from(density);
    if rounding == PalmRounding::Exact && !n.is_multiple_of(72) {
        return Err(invalid(
            "Palm fractional density coordinate; choose explicit Floor rounding",
        ));
    }
    Ok(n / 72)
}
pub(super) fn density(d: u16) -> Result<()> {
    if [72, 108, 144, 216, 288].contains(&d) {
        Ok(())
    } else {
        Err(invalid("unsupported Palm density identifier"))
    }
}

/// Decode all font resources and density strikes, accounting for their combined allocation.
pub fn read(c: &mut ReadContext<'_>, o: &PalmReadOptions) -> Result<(FontSet, Report)> {
    parse_budget(c.bytes(), o, c.limits(), c.retained_input_bytes())
}
pub(super) fn parse(
    b: &[u8],
    o: &PalmReadOptions,
    l: &ResourceLimits,
) -> Result<(FontSet, Report)> {
    parse_budget(b, o, l, b.len())
}
fn parse_budget(
    b: &[u8],
    o: &PalmReadOptions,
    l: &ResourceLimits,
    retained: usize,
) -> Result<(FontSet, Report)> {
    check(b.len(), l.max_input_bytes, "Palm input")?;
    let mut budget = Budget::new(retained, l)?;
    let mut report = Report::default();
    let mut glyph_count = 0usize;
    let mut out = FontSet {
        source_format: Some("palm".into()),
        ..Default::default()
    };
    let container = o.container.unwrap_or_else(|| {
        if matches!(
            b.get(..2),
            Some([0x90, 0] | [0x92, 0] | [0, 0x92] | [0, 0x02])
        ) {
            PalmContainer::Bare
        } else if b.get(33).is_some_and(|v| v & 1 != 0) {
            PalmContainer::Prc
        } else {
            PalmContainer::Pdb
        }
    });
    if container == PalmContainer::Bare {
        resource(
            b,
            o,
            l,
            &mut budget,
            &mut out,
            &mut report,
            &mut glyph_count,
        )?;
        return Ok((out, report));
    }
    slice(b, 0, 78)?;
    if (u16_at(b, 32, E)? & 1 != 0) != (container == PalmContainer::Prc) {
        return Err(invalid(
            "Palm database attributes disagree with container option",
        ));
    }
    budget.add(512)?;
    out.properties.push(Property::new(
        "palm.database_header",
        PropertyValue::Bytes(b[..72].to_vec()),
    ));
    let name = latin(b[..32].split(|x| *x == 0).next().unwrap_or_default());
    let entry_size = if container == PalmContainer::Prc {
        10
    } else {
        8
    };
    let mut seen = BTreeSet::new();
    let mut entries = Vec::new();
    let mut list = 72usize;
    let mut lists = Vec::new();
    loop {
        if list < 72 {
            return Err(invalid("Palm record list overlaps database header"));
        }
        if !seen.insert(list) {
            return Err(invalid("Palm record-list cycle"));
        }
        check(seen.len(), l.max_resources, "Palm record lists")?;
        budget.add(64)?;
        let next = u32_at(b, list, E)? as usize;
        let count = usize::from(u16_at(b, list + 4, E)?);
        check(
            entries.len().saturating_add(count),
            l.max_resources,
            "Palm database records",
        )?;
        check(count * entry_size, l.max_table_bytes, "Palm directory")?;
        let directory = slice(b, list + 6, count * entry_size)?;
        lists.push((list, list + 6 + directory.len()));
        budget.items::<(usize, usize, Vec<u8>)>(count)?;
        for i in 0..count {
            let entry = &directory[i * entry_size..(i + 1) * entry_size];
            let (offset, info) = if container == PalmContainer::Prc {
                (u32_at(entry, 6, E)? as usize, &entry[..6])
            } else {
                (u32_at(entry, 0, E)? as usize, &entry[4..])
            };
            if offset < 78 || offset >= b.len() {
                return Err(invalid("Palm payload offset outside file"));
            }
            entries.push((offset, info.to_vec()));
        }
        if next == 0 {
            break;
        }
        list = next;
    }
    lists.sort_unstable();
    if lists.windows(2).any(|w| w[0].1 > w[1].0) {
        return Err(invalid("Palm record lists overlap"));
    }
    let in_directory = |at: usize| {
        let i = lists.partition_point(|&(start, _)| start <= at);
        i > 0 && at < lists[i - 1].1
    };
    budget.items::<usize>(
        entries
            .len()
            .saturating_add(lists.len().saturating_mul(2))
            .saturating_add(3),
    )?;
    let mut bounds: Vec<usize> = entries.iter().map(|e| e.0).collect();
    for at in [52, 56] {
        let v = u32_at(b, at, E)? as usize;
        if v != 0 {
            if v < 78 || v >= b.len() {
                return Err(invalid("Palm auxiliary block outside file"));
            }
            bounds.push(v);
        }
    }
    for &(start, end) in &lists {
        bounds.push(start);
        bounds.push(end);
    }
    bounds.push(b.len());
    bounds.sort_unstable();
    bounds.dedup();
    let mut gray_resources = BTreeMap::new();
    let mut identities = BTreeSet::new();
    if container == PalmContainer::Prc {
        budget.add(entries.len().saturating_mul(80))?;
        for (offset, info) in &entries {
            budget.add(64)?;
            if !identities.insert(info.as_slice()) {
                return Err(invalid("duplicate Palm resource identity"));
            }
            let idx = bounds
                .binary_search(offset)
                .map_err(|_| invalid("Palm offset index"))?;
            let tag = [info[0], info[1], info[2], info[3]];
            if super::gray::bitmap_tag(&tag)
                && gray_resources
                    .insert(
                        (tag, u16_at(info, 4, E)?),
                        slice(b, *offset, bounds[idx + 1] - offset)?,
                    )
                    .is_some()
            {
                return Err(invalid("duplicate GrayFont bitmap resource identity"));
            }
        }
    }
    for (i, (offset, info)) in entries.into_iter().enumerate() {
        if in_directory(offset) {
            return Err(invalid("Palm payload overlaps directory"));
        }
        let at = bounds
            .binary_search(&offset)
            .map_err(|_| invalid("Palm offset index"))?;
        let payload = slice(b, offset, bounds[at + 1] - offset)?;
        check(payload.len(), l.max_table_bytes, "Palm resource")?;
        let first = out.fonts.len();
        let is_gray = container == PalmContainer::Prc
            && info[..4] == *b"GrFn"
            && payload
                .get(..2)
                .is_some_and(|v| v[0] == 0 && (1..=15).contains(&v[1]));
        if is_gray {
            let fonts = super::gray::decode(
                payload,
                &gray_resources,
                l,
                &mut budget,
                &mut glyph_count,
                out.fonts.len(),
            )?;
            out.fonts.extend(fonts);
            for f in &mut out.fonts[first..] {
                budget.add(name.len() + 256)?;
                f.names.family = Some(name.clone());
                f.properties
                    .push(Property::new("palm.record_index", i as i64));
                f.properties.push(Property::new(
                    "palm.record_info",
                    PropertyValue::Bytes(info.clone()),
                ));
            }
        } else if matches!(
            payload.get(..2),
            Some([0x90, 0] | [0x92, 0] | [0, 0x92] | [0, 0x02])
        ) {
            resource(
                payload,
                o,
                l,
                &mut budget,
                &mut out,
                &mut report,
                &mut glyph_count,
            )?;
            for f in &mut out.fonts[first..] {
                budget.add(name.len() + 256)?;
                f.names.family = Some(name.clone());
                f.properties
                    .push(Property::new("palm.record_index", i as i64));
                f.properties.push(Property::new(
                    "palm.record_info",
                    PropertyValue::Bytes(info.clone()),
                ));
            }
        } else {
            budget.add(payload.len() + info.len() + 128)?;
            out.properties.push(Property::new(
                "palm.opaque_record",
                PropertyValue::List(vec![
                    PropertyValue::Unsigned(i as u64),
                    PropertyValue::Bytes(info),
                    PropertyValue::Bytes(payload.to_vec()),
                ]),
            ));
        }
    }
    for (at, key) in [(52, "palm.app_info"), (56, "palm.sort_info")] {
        let v = u32_at(b, at, E)? as usize;
        if v != 0 {
            if in_directory(v) {
                return Err(invalid("Palm auxiliary block overlaps directory"));
            }
            let end = bounds[bounds
                .binary_search(&v)
                .map_err(|_| invalid("Palm offset index"))?
                + 1];
            budget.add(end - v + 128)?;
            out.properties
                .push(Property::new(key, PropertyValue::Bytes(b[v..end].to_vec())));
        }
    }
    if out.fonts.is_empty() {
        return Err(invalid(
            "Palm database contains no supported font resources",
        ));
    }
    out.validate(l)?;
    Ok((out, report))
}

fn resource(
    b: &[u8],
    o: &PalmReadOptions,
    l: &ResourceLimits,
    budget: &mut Budget,
    out: &mut FontSet,
    report: &mut Report,
    glyph_count: &mut usize,
) -> Result<()> {
    slice(b, 0, 26)?;
    let afnx = matches!(&b[..2], [0, 0x92] | [0, 0x02]);
    let e = if afnx { ByteOrder::Little } else { E };
    let v2 = matches!(u16_at(b, 0, e)?, 0x9200 | 0x0200);
    if !v2 && u16_at(b, 0, e)? != 0x9000 {
        return Err(invalid("unsupported Palm font type"));
    }
    let first = u16_at(b, 2, e)?;
    let last = u16_at(b, 4, e)?;
    if first > last {
        return Err(invalid("Palm reversed character range"));
    }
    let n = usize::from(last - first) + 2;
    check((n + 1) * 2, l.max_table_bytes, "Palm location table")?;
    let h = u32::from(u16_at(b, 14, e)?);
    let row = usize::from(u16_at(b, 24, e)?) * 2;
    let (loc, dens) = if v2 {
        if u16_at(b, 26, e)? != 1 {
            return Err(invalid("unsupported Palm extended font version"));
        }
        let count = usize::from(u16_at(b, 28, e)?);
        if count == 0 {
            return Err(invalid("Palm font has no density strikes"));
        }
        check(
            out.fonts.len().saturating_add(count),
            l.max_resources,
            "Palm strikes",
        )?;
        budget.items::<(u16, usize)>(count)?;
        let base = if afnx { 32 } else { 30 };
        let ds_size = if afnx { 8 } else { 6 };
        slice(b, base, count * ds_size)?;
        let mut ds = Vec::with_capacity(count);
        let mut unique = BTreeSet::new();
        for i in 0..count {
            let d = u16_at(b, base + i * ds_size, e)?;
            density(d)?;
            if !unique.insert(d) {
                return Err(invalid("duplicate Palm density"));
            }
            ds.push((d, u32_at(b, base + i * ds_size + ds_size - 4, e)? as usize));
        }
        (base + count * ds_size, ds)
    } else {
        (26 + row * h as usize, vec![(72, 26)])
    };
    slice(b, loc, (n + 1) * 2)?;
    let wo = 16 + usize::from(u16_at(b, 16, e)?) * 2;
    if wo < loc + (n + 1) * 2 {
        return Err(invalid("Palm width table overlaps locations"));
    }
    slice(b, wo, n * 2)?;
    let mut previous = 0;
    for i in 0..=n {
        let v = usize::from(u16_at(b, loc + i * 2, e)?);
        if v < previous || v > row * 8 {
            return Err(invalid("Palm invalid bitmap locations"));
        }
        previous = v;
    }
    let present = (0..n)
        .filter(|&i| b[wo + 2 * i..wo + 2 * i + 2] != [255, 255])
        .count();
    *glyph_count = glyph_count.saturating_add(present.saturating_mul(dens.len()));
    check(*glyph_count, l.max_glyphs, "Palm aggregate glyphs")?;
    check(
        out.fonts.len().saturating_add(dens.len()),
        l.max_resources,
        "Palm strikes",
    )?;
    for (d, bits) in dens {
        let fi = out.fonts.len();
        budget.items::<Font>(1)?;
        budget.items::<Glyph>(present)?;
        let height = scale(h, d, o.rounding)?;
        let stride = scale(row as u32, d, o.rounding)? as usize;
        let size = stride
            .checked_mul(height as usize)
            .ok_or_else(|| invalid("Palm strip size overflow"))?;
        check(size, l.max_table_bytes, "Palm bitmap strip")?;
        if v2 && bits < wo + n * 2 {
            return Err(invalid("Palm density bitmap overlaps tables"));
        }
        let strip = slice(b, bits, size)?;
        let ascent = scale(u32::from(u16_at(b, 18, e)?), d, o.rounding)?;
        let descent = scale(u32::from(u16_at(b, 20, e)?), d, o.rounding)?;
        let mut f = Font::default();
        f.source_format = Some("palm".into());
        f.charset.name = Some("palm-os".into());
        f.metrics.pixel_size = Some(height);
        f.metrics.ascent = Some(i64::from(ascent));
        f.metrics.descent = Some(i64::from(descent));
        f.metrics.line_gap = Some(i64::from(scale(
            u32::from(u16_at(b, 22, e)?),
            d,
            o.rounding,
        )?));
        f.metrics.resolution = Some(Vector::new(i64::from(d), i64::from(d)));
        f.metrics.bounds = Some(Bounds::new(
            scale(u32::from(u16_at(b, 12, e)?), d, o.rounding)?,
            height,
            0,
            i64::from(ascent) - i64::from(height),
        ));
        budget.add(1024)?;
        if afnx && b[1] == 2 {
            f.properties.push(Property::new("palm.fontsubst", 1i64));
        }
        for (key, val) in [
            ("palm.density", i64::from(d)),
            ("palm.version", if v2 { 2 } else { 1 }),
            ("palm.max_width", i64::from(u16_at(b, 6, e)?)),
            ("palm.kern_max", i64::from(u16_at(b, 8, e)? as i16)),
            ("palm.n_descent", i64::from(u16_at(b, 10, e)? as i16)),
        ] {
            f.properties.push(Property::new(key, val));
        }
        for i in 0..n {
            if b[wo + 2 * i..wo + 2 * i + 2] == [255, 255] {
                continue;
            }
            let x = scale(u32::from(u16_at(b, loc + i * 2, e)?), d, o.rounding)?;
            let end = scale(u32::from(u16_at(b, loc + (i + 1) * 2, e)?), d, o.rounding)?;
            let width = end - x;
            let mut g = Glyph::new(extract(
                strip, stride, x as usize, width, height, l, budget,
            )?);
            g.metrics.bounds = Bounds::new(width, height, 0, i64::from(ascent) - i64::from(height));
            g.metrics.advance = Some(Vector::new(
                i64::from(scale(u32::from(b[wo + 2 * i + 1]), d, o.rounding)?),
                0,
            ));
            if b[wo + 2 * i] != 0 {
                budget.add(128)?;
                g.properties.push(Property::new(
                    "palm.ignored_offset",
                    i64::from(b[wo + 2 * i] as i8),
                ));
            }
            if i + 1 < n {
                g.codepoints
                    .push(Codepoint::Legacy(u32::from(first) + i as u32));
            }
            budget.add(128)?;
            let id = f.add_glyph(g)?;
            if i + 1 == n {
                f.metrics.default_glyph = Some(id);
            }
        }
        if o.rounding == PalmRounding::Floor && d == 108 {
            budget.diagnostic(
                report,
                "palm",
                "read.fractional_rounding",
                crate::report::Impact::Repair,
                Some(fi),
                None,
                "Explicit floor interpretation of 108-dpi coordinates",
            )?;
        }
        out.fonts.push(f);
    }
    Ok(())
}
