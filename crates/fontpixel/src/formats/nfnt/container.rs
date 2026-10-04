use super::{NfntReadOptions, raster, support::*};
use crate::formats::Confidence;
use crate::model::{FontSet, Property, PropertyValue, Rational};
use crate::resources::ReadContext;
use crate::{Font, Report, ResourceLimits, Result};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone)]
pub(super) struct Resource<'a> {
    pub kind: [u8; 4],
    pub id: u16,
    pub attr: u8,
    pub handle: [u8; 4],
    pub name: Option<&'a [u8]>,
    pub data: &'a [u8],
}
impl Resource<'_> {
    pub fn metadata(&self) -> Vec<u8> {
        let mut m = Vec::with_capacity(13 + self.name.map_or(0, <[u8]>::len));
        m.extend_from_slice(&self.kind);
        m.extend_from_slice(&self.id.to_be_bytes());
        m.push(self.attr);
        m.extend_from_slice(&self.handle);
        m.extend_from_slice(&self.name.map_or(0xffff, |n| n.len() as u16).to_be_bytes());
        if let Some(n) = self.name {
            m.extend_from_slice(n);
        }
        m
    }
}
pub(super) fn unpack_record(b: &[u8]) -> Result<Resource<'_>> {
    let n = u16be(b, 11)?;
    if n != 0xffff && n > 255 {
        return Err(invalid("resource name exceeds Pascal string limit"));
    }
    let len = if n == 0xffff { 0 } else { usize::from(n) };
    let h = slice(b, 0, 13)?;
    let name = if n == 0xffff {
        None
    } else {
        Some(slice(b, 13, len)?)
    };
    Ok(Resource {
        kind: h[0..4].try_into().map_err(|_| invalid("resource type"))?,
        id: u16be(h, 4)?,
        attr: h[6],
        handle: h[7..11]
            .try_into()
            .map_err(|_| invalid("resource handle"))?,
        name,
        data: slice(b, 13 + len, b.len() - 13 - len)?,
    })
}
fn fork_signature(b: &[u8]) -> bool {
    if b.len() < 30 {
        return false;
    }
    let Ok(d) = u32be(b, 0) else { return false };
    let Ok(m) = u32be(b, 4) else { return false };
    d >= 16
        && m >= 16
        && (d as usize) < b.len()
        && (m as usize) < b.len()
        && u32be(b, 12).is_ok_and(|n| n >= 28)
}
fn mac_signature(b: &[u8]) -> bool {
    b.len() >= 128
        && b[0] == 0
        && (1..=63).contains(&b[1])
        && b[74] == 0
        && b[82] == 0
        && (u32be(b, 87).is_ok_and(|n| n >= 30) || u32be(b, 83).is_ok_and(|n| n >= 30))
}
pub(super) fn detect(b: &[u8]) -> Confidence {
    if b.starts_with(&[0, 5, 0x16, 0]) || b.starts_with(&[0, 5, 0x16, 7]) {
        return if super::probe::apple(b).unwrap_or(false) {
            Confidence::Certain
        } else {
            Confidence::NoMatch
        };
    }
    if mac_signature(b) && super::probe::mac(b).unwrap_or(false) {
        return Confidence::Certain;
    }
    if fork_signature(b) && super::probe::fork(b).unwrap_or(false) {
        return Confidence::Possible;
    }
    if raster::plausible(b) {
        return Confidence::Possible;
    }
    Confidence::NoMatch
}
fn retained(
    set: &mut FontSet,
    key: &str,
    b: &[u8],
    budget: &mut Budget,
    l: &ResourceLimits,
) -> Result<()> {
    check(b.len(), l.max_table_bytes, "NFNT retained metadata")?;
    budget.add(128 + key.len() + b.len())?;
    set.properties
        .push(Property::new(key, PropertyValue::Bytes(b.to_vec())));
    Ok(())
}
pub(super) fn read_context(
    c: &mut ReadContext<'_>,
    o: &NfntReadOptions,
) -> Result<(FontSet, Report)> {
    if let Some(path) = &o.apple_double_resource {
        let data = c.resource(path)?;
        let l = c.limits();
        let mut budget = Budget::new(c.retained_input_bytes(), l)?;
        budget.resource(1, l)?;
        let (mut set, r) = parse(&data, o, l, &mut budget)?;
        if !data.starts_with(&[0, 5, 0x16, 7]) {
            return Err(invalid("companion is not AppleDouble"));
        }
        retained(&mut set, "nfnt.data-fork", c.bytes(), &mut budget, l)?;
        return Ok((set, r));
    }
    let mut budget = Budget::new(c.retained_input_bytes(), c.limits())?;
    let (set, mut report) = parse(c.bytes(), o, c.limits(), &mut budget)?;
    for e in &mut report.entries {
        if let Some(name) = c.name() {
            check(name.len(), c.limits().max_table_bytes, "NFNT asset name")?;
            budget.add(name.len())?;
            e.location.asset = Some(name.to_owned());
        }
    }
    Ok((set, report))
}
pub(super) fn parse(
    b: &[u8],
    o: &NfntReadOptions,
    l: &ResourceLimits,
    budget: &mut Budget,
) -> Result<(FontSet, Report)> {
    let mut set = FontSet {
        source_format: Some("nfnt".into()),
        ..FontSet::default()
    };
    let mut report = Report::default();
    let fork = if b.starts_with(&[0, 5, 0x16, 0]) || b.starts_with(&[0, 5, 0x16, 7]) {
        let version = u32be(b, 4)?;
        if ![0x10000, 0x20000].contains(&version) {
            return Err(invalid("unsupported AppleSingle/Double version"));
        }
        let n = usize::from(u16be(b, 24)?);
        check(n, l.max_resources, "NFNT envelope entries")?;
        budget.resource(n, l)?;
        let end = 26 + n * 12;
        slice(b, 0, end)?;
        budget.add(n * 48)?;
        let mut occupied = vec![(0, end)];
        let mut ids = BTreeSet::new();
        let mut fork = None;
        let mut data_fork = None;
        retained(&mut set, "nfnt.apple-header", slice(b, 0, 24)?, budget, l)?;
        for i in 0..n {
            let at = 26 + i * 12;
            let id = u32be(b, at)?;
            if !ids.insert(id) {
                return Err(invalid("duplicate Apple entry ID"));
            }
            let offset = u32be(b, at + 4)? as usize;
            let len = u32be(b, at + 8)? as usize;
            check(len, l.max_resource_bytes, "NFNT Apple entry bytes")?;
            let data = slice(b, offset, len)?;
            occupied.push((offset, len));
            if id == 2 {
                fork = Some(data);
            } else if id == 1 {
                if b[3] == 7 {
                    return Err(invalid("AppleDouble contains a data-fork entry"));
                }
                data_fork = Some(data);
            } else {
                retained(&mut set, &format!("nfnt.apple-entry.{id}"), data, budget, l)?;
            }
        }
        ranges(&mut occupied)?;
        nonzero_gaps(b, &occupied, budget, &mut report, "apple_padding")?;
        if let Some(fork) = fork {
            if let Some(data) = data_fork {
                retained(&mut set, "nfnt.data-fork", data, budget, l)?;
            }
            fork
        } else if let Some(data) = data_fork.filter(|b| fork_signature(b)) {
            budget.add(128)?;
            set.properties
                .push(Property::new("nfnt.apple-font-fork", 1i64));
            data
        } else {
            return Err(invalid("Apple envelope has no font resource fork"));
        }
    } else if mac_signature(b) {
        if b[122] >= 129 {
            if b[123] > 130 {
                return Err(invalid("unsupported MacBinary minimum version"));
            }
            if u16be(b, 124)? != crc16(&b[..124]) {
                return Err(invalid("MacBinary header CRC mismatch"));
            }
        }
        let secondary = if b[122] >= 129 {
            usize::from(u16be(b, 120)?)
        } else {
            0
        };
        let data_len = u32be(b, 83)? as usize;
        budget.resource(
            1 + usize::from(data_len > 0)
                + usize::from(secondary > 0)
                + usize::from(u16be(b, 99)? > 0),
            l,
        )?;
        let fork_len = u32be(b, 87)? as usize;
        check(data_len, l.max_resource_bytes, "NFNT MacBinary data fork")?;
        check(
            fork_len,
            l.max_resource_bytes,
            "NFNT MacBinary resource fork",
        )?;
        let data_at = 128 + align(secondary, 128)?;
        let fork_at = data_at
            .checked_add(align(data_len, 128)?)
            .ok_or_else(|| invalid("MacBinary offset overflow"))?;
        let data = slice(b, data_at, data_len)?;
        let fork = slice(b, fork_at, fork_len)?;
        retained(
            &mut set,
            "nfnt.macbinary-header",
            slice(b, 0, 128)?,
            budget,
            l,
        )?;
        if secondary > 0 {
            retained(
                &mut set,
                "nfnt.macbinary-secondary",
                slice(b, 128, secondary)?,
                budget,
                l,
            )?;
        }
        if !data.is_empty() && !fork.is_empty() {
            retained(&mut set, "nfnt.data-fork", data, budget, l)?;
        }
        let comment_len = usize::from(u16be(b, 99)?);
        let comment_at = fork_at
            .checked_add(align(fork_len, 128)?)
            .ok_or_else(|| invalid("MacBinary comment offset overflow"))?;
        let mut occupied = vec![
            (0, 128),
            (128, secondary),
            (data_at, data_len),
            (fork_at, fork_len),
        ];
        if comment_len > 0 {
            retained(
                &mut set,
                "nfnt.macbinary-comment",
                slice(b, comment_at, comment_len)?,
                budget,
                l,
            )?;
            occupied.push((comment_at, comment_len));
        }
        ranges(&mut occupied)?;
        nonzero_gaps(b, &occupied, budget, &mut report, "macbinary_padding")?;
        if fork.is_empty() && fork_signature(data) {
            budget.add(128)?;
            set.properties
                .push(Property::new("nfnt.macbinary-font-fork", 1i64));
            data
        } else {
            fork
        }
    } else if fork_signature(b) {
        b
    } else {
        set.fonts.push(raster::parse(b, o, l, budget)?);
        return Ok((set, report));
    };
    parse_fork(fork, o, l, budget, &mut set, &mut report)?;
    Ok((set, report))
}
fn nonzero_gaps(
    b: &[u8],
    occupied: &[(usize, usize)],
    budget: &mut Budget,
    r: &mut Report,
    code: &str,
) -> Result<()> {
    let mut end = 0;
    for &(at, n) in occupied {
        if at > end && slice(b, end, at - end)?.iter().any(|v| *v != 0) {
            budget.diagnostic(
                r,
                code,
                None,
                None,
                None,
                "Nonzero unreferenced container bytes are not retained",
            )?;
        }
        end = end.max(at + n);
    }
    if end < b.len() && b[end..].iter().any(|v| *v != 0) {
        budget.diagnostic(
            r,
            code,
            None,
            None,
            None,
            "Nonzero trailing container bytes are not retained",
        )?;
    }
    Ok(())
}
fn resource_name(b: &[u8]) -> Result<String> {
    let codec =
        crate::charset::CharsetCodec::new("macroman", None).map_err(|e| invalid(e.to_string()))?;
    let mut s = String::new();
    for &v in b {
        let cp = codec
            .cp2unicode(u32::from(v))
            .ok_or_else(|| invalid("unmapped MacRoman name byte"))?;
        s.push(char::from_u32(cp).ok_or_else(|| invalid("invalid MacRoman name"))?);
    }
    Ok(s)
}
pub(super) fn style_name(style: u16) -> String {
    let names = [
        "Bold",
        "Italic",
        "Underline",
        "Outline",
        "Shadow",
        "Condensed",
        "Extended",
    ];
    let mut s = String::new();
    for (i, n) in names.iter().enumerate() {
        if style & (1 << i) != 0 {
            if !s.is_empty() {
                s.push(' ');
            }
            s.push_str(n);
        }
    }
    if s.is_empty() {
        s.push_str("Regular");
    }
    s
}
fn apply_fond(
    font: &mut Font,
    r: &Resource<'_>,
    fond: &Resource<'_>,
    size: u16,
    style: u16,
) -> Result<()> {
    if let Some(n) = fond.name.or(r.name) {
        font.names.family = Some(resource_name(n)?);
    }
    font.names.style = Some(style_name(style));
    font.metrics.pixel_size = Some(u32::from(size));
    font.metrics.point_size = Some(Rational::integer(i64::from(size)));
    font.properties
        .push(Property::new("nfnt.fond-id", i64::from(fond.id)));
    font.properties
        .push(Property::new("nfnt.fond-style", i64::from(style)));
    Ok(())
}
fn parse_fork(
    b: &[u8],
    o: &NfntReadOptions,
    l: &ResourceLimits,
    budget: &mut Budget,
    set: &mut FontSet,
    report: &mut Report,
) -> Result<()> {
    let da = u32be(b, 0)? as usize;
    let ma = u32be(b, 4)? as usize;
    let dl = u32be(b, 8)? as usize;
    let ml = u32be(b, 12)? as usize;
    let data = slice(b, da, dl)?;
    let map = slice(b, ma, ml)?;
    check(ml, l.max_table_bytes, "NFNT resource map")?;
    let mut occupied = vec![(0, 16), (da, dl), (ma, ml)];
    ranges(&mut occupied)?;
    nonzero_gaps(b, &occupied, budget, report, "fork_padding")?;
    slice(map, 0, 28)?;
    // Resource map header copies can be zero in data-fork resources.
    if map[..16].iter().any(|v| *v != 0) && map[..16] != b[..16] {
        return Err(invalid("resource map header copy differs"));
    }
    retained(set, "nfnt.map-fields", slice(map, 16, 8)?, budget, l)?;
    let ta = usize::from(u16be(map, 24)?);
    let na = usize::from(u16be(map, 26)?);
    if ta < 28 || na > map.len() {
        return Err(invalid("invalid resource map list offset"));
    }
    let nt = usize::from(u16be(map, ta)?.wrapping_add(1));
    check(nt, l.max_resources, "NFNT resource types")?;
    slice(map, ta + 2, nt * 8)?;
    budget.add(nt * 64)?;
    let mut res = Vec::new();
    let mut kinds = BTreeSet::new();
    let mut refs = vec![(0, 28), (ta, 2 + nt * 8)];
    let mut data_ranges = Vec::new();
    let mut name_ranges = Vec::new();
    for t in 0..nt {
        let at = ta + 2 + t * 8;
        let kind: [u8; 4] = slice(map, at, 4)?
            .try_into()
            .map_err(|_| invalid("type tag"))?;
        if !kinds.insert(kind) {
            return Err(invalid("duplicate resource type"));
        }
        let n = usize::from(u16be(map, at + 4)?.wrapping_add(1));
        check(res.len() + n, l.max_resources, "NFNT resources")?;
        budget.resource(n, l)?;
        budget.add(n * 512)?;
        let ra = ta + usize::from(u16be(map, at + 6)?);
        slice(map, ra, n * 12)?;
        refs.push((ra, n * 12));
        let mut ids = BTreeSet::new();
        for i in 0..n {
            let at = ra + i * 12;
            let id = u16be(map, at)?;
            if !ids.insert(id) {
                return Err(invalid("duplicate resource type/ID"));
            }
            let no = u16be(map, at + 2)?;
            let name = if no == 0xffff {
                None
            } else {
                let at = na + usize::from(no);
                let len = usize::from(
                    *slice(map, at, 1)?
                        .first()
                        .ok_or_else(|| invalid("resource name"))?,
                );
                name_ranges.push((at, len + 1));
                Some(slice(map, at + 1, len)?)
            };
            let flags = u32be(map, at + 4)?;
            let offset = (flags & 0xffffff) as usize;
            let len = u32be(data, offset)? as usize;
            check(len, l.max_resource_bytes, "NFNT embedded resource")?;
            let payload = slice(data, offset + 4, len)?;
            data_ranges.push((offset, len + 4));
            res.push(Resource {
                kind,
                id,
                attr: (flags >> 24) as u8,
                handle: slice(map, at + 8, 4)?
                    .try_into()
                    .map_err(|_| invalid("handle"))?,
                name,
                data: payload,
            });
        }
    }
    ranges(&mut refs)?;
    name_ranges.sort_unstable();
    name_ranges.dedup();
    refs.extend(name_ranges);
    ranges(&mut refs)?;
    ranges(&mut data_ranges)?;
    nonzero_gaps(map, &refs, budget, report, "map_padding")?;
    nonzero_gaps(data, &data_ranges, budget, report, "resource_padding")?;
    let mut associations = BTreeMap::new();
    for (idx, r) in res.iter().enumerate().filter(|(_, r)| r.kind == *b"FOND") {
        slice(r.data, 0, 54)?;
        let n = usize::from(u16be(r.data, 52)?.wrapping_add(1));
        check(n, l.max_resources, "NFNT FOND associations")?;
        slice(r.data, 54, n * 6)?;
        budget.add(n * 64)?;
        for field in [16, 20, 24] {
            let offset = u32be(r.data, field)? as usize;
            if offset != 0 && (offset < 54 + n * 6 || offset >= r.data.len()) {
                return Err(invalid("FOND optional table offset is invalid"));
            }
        }
        for i in 0..n {
            let at = 54 + i * 6;
            let size = u16be(r.data, at)?;
            let style = u16be(r.data, at + 2)?;
            let id = u16be(r.data, at + 4)?;
            if size != 0 && associations.insert(id, (idx, size, style)).is_some() {
                return Err(invalid("conflicting FOND bitmap association"));
            }
        }
    }
    for r in &res {
        if r.kind == *b"NFNT" || (r.kind == *b"FONT" && r.id & 127 != 0) {
            let kind = u16be(r.data, 0)?;
            let palette = if kind & 0x0280 != 0 {
                let data = if kind & 0x80 != 0 {
                    res.iter()
                        .find(|p| p.kind == *b"fctb" && p.id == r.id)
                        .map(|p| p.data)
                } else {
                    None
                };
                Some(super::palette::Palette::parse(
                    data,
                    1usize << ((kind >> 2) & 3),
                    l,
                )?)
            } else {
                None
            };
            let mut f = raster::parse_with_palette(r.data, o, l, budget, palette.as_ref())?;
            if let Some(palette) = palette {
                budget.diagnostic(report,"palette_to_gray",Some(set.fonts.len()),None,None,"Indexed RGB colors projected to Gray8 coverage; original NFNT and fctb bytes retained")?;
                if palette.missing {
                    budget.diagnostic(report,"palette_port_colors",Some(set.fonts.len()),None,None,"Unspecified font colors use an explicit white-background/black-foreground ramp")?;
                }
            }
            let metadata = r.metadata();
            budget.add(256 + metadata.len())?;
            f.properties.push(Property::new(
                "nfnt.resource",
                PropertyValue::Bytes(metadata),
            ));
            if let Some(&(idx, size, style)) = associations.get(&r.id) {
                apply_fond(&mut f, r, &res[idx], size, style)?;
            } else if r.kind == *b"FONT" {
                let family = (r.id >> 7) & 255;
                let size = r.id & 127;
                let name = res
                    .iter()
                    .find(|n| n.kind == *b"FONT" && n.id == family * 128)
                    .and_then(|n| n.name)
                    .or(r.name);
                if let Some(name) = name {
                    f.names.family = Some(resource_name(name)?);
                }
                f.metrics.pixel_size = Some(u32::from(size));
                f.metrics.point_size = Some(Rational::integer(i64::from(size)));
                budget.add(256)?;
                f.properties
                    .push(Property::new("nfnt.font-family-id", i64::from(family)));
            } else if let Some(n) = r.name {
                f.names.family = Some(resource_name(n)?);
            }
            set.fonts.push(f);
        } else {
            let len = 13 + r.name.map_or(0, <[u8]>::len) + r.data.len();
            check(len, l.max_table_bytes, "NFNT retained resource")?;
            budget.add(256 + len)?;
            let mut raw = r.metadata();
            raw.extend_from_slice(r.data);
            set.properties
                .push(Property::new("nfnt.resource", PropertyValue::Bytes(raw)));
        }
    }
    if set.fonts.is_empty() {
        return Err(invalid("resource fork has no NFNT/FONT bitmap fonts"));
    }
    Ok(())
}
