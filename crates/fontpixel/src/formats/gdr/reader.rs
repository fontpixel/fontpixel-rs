use super::{GdrReadOptions, wire::*};
use crate::bitmap::{Bitmap, Bounds};
use crate::formats::legacy_common::*;
use crate::model::{Codepoint, Font, FontSet, Glyph, Property, PropertyValue, Spacing, Vector};
use crate::resources::ReadContext;
use crate::{Report, ResourceLimits, Result};
use std::collections::BTreeMap;

/// Decode every native GDR strike, mapping range, metric and compressed bitmap.
pub fn read(c: &mut ReadContext<'_>, o: &GdrReadOptions) -> Result<(FontSet, Report)> {
    parse_budget(c.bytes(), o, c.limits(), c.retained_input_bytes())
}
pub(super) fn parse(b: &[u8], o: &GdrReadOptions, l: &ResourceLimits) -> Result<(FontSet, Report)> {
    parse_budget(b, o, l, b.len())
}
fn parse_budget(
    b: &[u8],
    o: &GdrReadOptions,
    l: &ResourceLimits,
    retained: usize,
) -> Result<(FontSet, Report)> {
    check(b.len(), l.max_input_bytes, "GDR input")?;
    if !b.starts_with(&MAGIC) {
        return Err(invalid("invalid GDR direct-store UIDs/checksum"));
    }
    let mut budget = Budget::new(retained, l)?;
    let mut report = Report::default();
    let root = u32_at(b, 16, E)? as usize;
    if root < 20 {
        return Err(invalid("GDR root overlaps UIDs"));
    }
    let mut c = Cursor::new(b, root);
    let version = c.u32()?;
    if ![39, 40, 41, 42].contains(&version) {
        return Err(invalid("unsupported GDR compiler version"));
    }
    let uid = c.u32()?;
    let aspect = c.u32()?;
    let data = c.u32()? as usize;
    if aspect == 0 || aspect > i32::MAX as u32 {
        return Err(invalid("invalid GDR pixel aspect ratio"));
    }
    let mut set = FontSet {
        source_format: Some("gdr".into()),
        ..Default::default()
    };
    budget.add(512)?;
    for (k, v) in [
        ("gdr.version", version),
        ("gdr.collection_uid", uid),
        ("gdr.pixel_aspect", aspect),
    ] {
        set.properties.push(Property::new(k, i64::from(v)));
    }
    let nc = c.count(l.max_resources, "GDR copyright count")?;
    for _ in 0..nc {
        let s = c.string(l, &mut budget)?;
        set.properties.push(Property::new("gdr.copyright", s));
    }
    let mut c = Cursor::new(b, data);
    let nf = c.count(l.max_resources, "GDR strike count")?;
    if nf == 0 {
        return Err(invalid("empty GDR font store"));
    }
    budget.items::<Font>(nf)?;
    let mut uids = BTreeMap::new();
    let mut glyphs = 0usize;
    for fi in 0..nf {
        let uid = c.u32()?;
        if uids.insert(uid, fi).is_some() {
            return Err(invalid("duplicate GDR strike UID"));
        }
        let metrics = c.bytes(7)?;
        if metrics[0] > 1 || metrics[1] > 1 || metrics[2] > 1 {
            return Err(invalid("GDR invalid font style flags"));
        }
        let height = metrics[3] as i8;
        let ascent = metrics[4] as i8;
        if height < 0 {
            return Err(invalid("negative GDR cell height"));
        }
        let extra = if version == 42 {
            Some(c.bytes(5)?)
        } else {
            None
        };
        let encoding = c.u32()?;
        if encoding != 0 {
            return Err(invalid(
                "unsupported GDR bitmap encoding (expected native monochrome row packets)",
            ));
        }
        let inline = version == 40;
        let (metric_at, nm) = if inline {
            (0, 0)
        } else {
            (
                c.u32()? as usize,
                c.count(l.max_table_bytes / 5, "GDR metric count")?,
            )
        };
        let ns = c.count(l.max_resources, "GDR code sections")?;
        let mut f = Font::default();
        f.source_format = Some("gdr".into());
        f.charset.name = Some(if o.unicode { "unicode" } else { "gdr-native" }.into());
        f.metrics.pixel_size = Some(height as u32);
        f.metrics.ascent = Some(i64::from(ascent));
        f.metrics.descent = Some(i64::from(height) - i64::from(ascent));
        f.metrics.spacing = Some(if metrics[2] == 0 {
            Spacing::Monospaced
        } else {
            Spacing::Proportional
        });
        f.names.style = Some(
            match (metrics[0], metrics[1]) {
                (0, 0) => "Regular",
                (0, 1) => "Bold",
                (1, 0) => "Italic",
                _ => "Bold Italic",
            }
            .into(),
        );
        budget.add(1024)?;
        for (k, v) in [
            ("gdr.uid", i64::from(uid)),
            ("gdr.max_width", i64::from(metrics[5] as i8)),
            ("gdr.max_normal_width", i64::from(metrics[6] as i8)),
        ] {
            f.properties.push(Property::new(k, v));
        }
        if let Some(extra) = extra {
            for (k, &v) in [
                "gdr.capital_ascent",
                "gdr.max_ascent",
                "gdr.standard_descent",
                "gdr.max_descent",
                "gdr.line_gap",
            ]
            .iter()
            .zip(extra)
            {
                f.properties.push(Property::new(*k, i64::from(v as i8)));
            }
            f.metrics.line_gap = Some(i64::from(extra[4] as i8));
        }
        let table = if inline {
            &[][..]
        } else {
            let mut mc = Cursor::new(b, metric_at);
            if mc.count(l.max_table_bytes / 5, "GDR metric count")? != nm {
                return Err(invalid("GDR metric counts disagree"));
            }
            mc.bytes(
                nm.checked_mul(5)
                    .ok_or_else(|| invalid("GDR metric length overflow"))?,
            )?
        };
        let entry_size = if inline { 8 } else { 2 };
        let absent = if inline { 0xffff } else { 0x7fff };
        let mut previous = None;
        for _ in 0..ns {
            let first = c.u16()?;
            let last = c.u16()?;
            let offsets = c.u32()? as usize;
            let bitmap = c.u32()? as usize;
            if first > last || previous.is_some_and(|v| first <= v) {
                return Err(invalid("GDR code sections overlap or are unordered"));
            }
            previous = Some(last);
            let count = usize::from(last - first) + 1;
            let mut oc = Cursor::new(b, offsets);
            if oc.count(l.max_glyphs, "GDR section offsets")? != count {
                return Err(invalid("GDR character range and offsets disagree"));
            }
            let offsets = oc.bytes(count * entry_size)?;
            let mut bc = Cursor::new(b, bitmap);
            let size = bc.count(l.max_table_bytes, "GDR bitmap table")?;
            let bits = bc.bytes(size)?;
            // Offsets may alias the same encoded glyph. Bound each glyph by its next
            // physical offset, independent of code order, so corruption cannot read
            // through the following glyph's packet.
            budget.items::<usize>(count + 1)?;
            let mut ends = Vec::with_capacity(count + 1);
            for i in 0..count {
                let p = usize::from(u16_at(offsets, entry_size * i, E)?);
                if p != absent {
                    if p > size || (!inline && p == size) {
                        return Err(invalid("GDR glyph offset outside bitmap stream"));
                    }
                    ends.push(p);
                }
            }
            ends.push(size);
            ends.sort_unstable();
            ends.dedup();
            for i in 0..count {
                let at = usize::from(u16_at(offsets, entry_size * i, E)?);
                if at == absent {
                    continue;
                }
                glyphs = glyphs
                    .checked_add(1)
                    .ok_or_else(|| invalid("GDR glyph count overflow"))?;
                check(glyphs, l.max_glyphs, "GDR aggregate glyphs")?;
                budget.items::<Glyph>(1)?;
                budget.add(128)?;
                let end = ends
                    .get(
                        ends.binary_search(&at)
                            .map_err(|_| invalid("GDR glyph offset index"))?
                            + 1,
                    )
                    .copied()
                    .unwrap_or(size);
                let mut bits = Bits {
                    bytes: &bits[at..end],
                    at: 0,
                };
                let m = if inline {
                    &offsets[i * entry_size + 2..i * entry_size + 7]
                } else {
                    let wide = bits.get(1)? != 0;
                    let index = bits.get(if wide { 15 } else { 7 })? as usize;
                    slice(
                        table,
                        index
                            .checked_mul(5)
                            .ok_or_else(|| invalid("GDR metric index overflow"))?,
                        5,
                    )?
                };
                let top = i64::from(m[0] as i8);
                let height = i64::from(m[1] as i8);
                let left = i64::from(m[2] as i8);
                let advance = i64::from(m[3] as i8);
                let right = i64::from(m[4] as i8);
                let width = advance - left - right;
                if height < 0 || width < 0 {
                    return Err(invalid("GDR negative glyph dimensions"));
                }
                let width = width as u32;
                let height = height as u32;
                check(
                    width as usize * height as usize,
                    l.max_pixels_per_glyph,
                    "GDR glyph pixels",
                )?;
                let row = (width as usize).div_ceil(8);
                let n = row * height as usize;
                budget.add(n.saturating_mul(2))?;
                let mut raster = vec![0; n];
                let mut y = 0usize;
                while y < height as usize {
                    let literal = bits.get(1)? != 0;
                    let rows = bits.get(4)? as usize;
                    if rows == 0 || y + rows > height as usize {
                        return Err(invalid("invalid GDR row run"));
                    }
                    let source_rows = if literal { rows } else { 1 };
                    for dy in 0..source_rows {
                        for x in 0..width as usize {
                            if bits.get(1)? != 0 {
                                raster[(y + dy) * row + x / 8] |= 0x80 >> (x % 8);
                            }
                        }
                    }
                    if !literal {
                        for dy in 1..rows {
                            raster.copy_within(y * row..(y + 1) * row, (y + dy) * row);
                        }
                    }
                    y += rows;
                }
                let bitmap = Bitmap::<crate::bitmap::Mono>::from_raw(
                    width,
                    height,
                    &raster,
                    bitmap_limits(l),
                )?;
                let mut g = Glyph::new(bitmap);
                g.metrics.bounds = Bounds::new(width, height, left, top - i64::from(height));
                g.metrics.advance = Some(Vector::new(advance, 0));
                let code = u32::from(first) + i as u32;
                if o.unicode && char::from_u32(code).is_none() {
                    return Err(invalid(
                        "GDR Unicode map contains a surrogate; select native interpretation",
                    ));
                }
                let cp = if o.unicode {
                    Codepoint::Unicode(code)
                } else {
                    Codepoint::Legacy(code)
                };
                g.codepoints.push(cp);
                let id = f.add_glyph(g)?;
                if code == 0xfffd {
                    f.metrics.default_glyph = Some(id);
                    f.metrics.default_character = Some(cp);
                }
            }
        }
        set.fonts.push(f);
    }
    let nt = c.count(l.max_resources, "GDR typeface count")?;
    for _ in 0..nt {
        let name = c.string(l, &mut budget)?;
        let flags = c.byte()?;
        let na = c.count(l.max_resources, "GDR typeface references")?;
        budget.items::<PropertyValue>(na.saturating_mul(4) + 2)?;
        let mut refs = Vec::with_capacity(na);
        for _ in 0..na {
            let uid = c.u32()?;
            let sx = c.byte()? as i8;
            let sy = c.byte()? as i8;
            if sx <= 0 || sy <= 0 {
                return Err(invalid("GDR nonpositive typeface scale"));
            }
            refs.push(PropertyValue::List(vec![
                PropertyValue::Integer(i64::from(uid)),
                PropertyValue::Integer(i64::from(sx)),
                PropertyValue::Integer(i64::from(sy)),
            ]));
            if let Some(&fi) = uids.get(&uid) {
                if set.fonts[fi].names.family.is_none() {
                    budget.add(name.len())?;
                    set.fonts[fi].names.family = Some(name.clone());
                }
                if sx != 1 || sy != 1 {
                    budget.diagnostic(&mut report,"gdr","read.synthetic_typeface",crate::report::Impact::None,Some(fi),None,format!("Typeface {name:?} selects native strike with scale {sx} x {sy}; association retained in gdr.typeface"))?;
                }
            } else {
                budget.diagnostic(
                    &mut report,
                    "gdr",
                    "read.external_strike",
                    crate::report::Impact::None,
                    None,
                    None,
                    format!(
                        "Typeface {name:?} refers to external UID {uid:#x}; reference retained"
                    ),
                )?;
            }
        }
        set.properties.push(Property::new(
            "gdr.typeface",
            PropertyValue::List(vec![
                PropertyValue::Text(name),
                PropertyValue::Integer(i64::from(flags)),
                PropertyValue::List(refs),
            ]),
        ));
    }
    Ok((set, report))
}
