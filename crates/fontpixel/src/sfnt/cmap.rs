//! Complete Unicode cmap writing and bounded nominal-map reading.
use super::{Limits, bounded, buffer, invalid, p16, p32, to16, to32};
use crate::Result;
use read_fonts::{
    FontData, FontRead,
    tables::cmap::{Cmap, CmapSubtable, PlatformId},
};
use std::collections::BTreeMap;

/// A Unicode scalar to glyph ID mapping, in ascending scalar order.
pub type Mapping = BTreeMap<u32, u16>;
/// Serialized cmap and a compatibility fact callers can report.
#[derive(Clone, Debug)]
pub struct EncodedCmap {
    /// Complete table bytes, always including full-Unicode format 12.
    pub data: Vec<u8>,
    /// Whether a complete representable BMP format 4 is included (U+FFFF is only
    /// in format 12). False never means the full-Unicode map was truncated.
    pub has_bmp_subtable: bool,
}
fn check(mapping: &Mapping, num_glyphs: u16, limits: &Limits) -> Result<()> {
    limits.glyphs(num_glyphs as usize)?;
    bounded(
        mapping.len(),
        limits.max_cmap_mappings,
        "sfnt cmap mappings",
    )?;
    for (&cp, &gid) in mapping {
        if char::from_u32(cp).is_none() {
            return Err(invalid("cmap contains a non-scalar codepoint"));
        }
        if gid == 0 || gid >= num_glyphs {
            return Err(invalid("cmap glyph ID outside 1..numGlyphs"));
        }
    }
    Ok(())
}
fn format4(mapping: &Mapping) -> Result<Option<Vec<u8>>> {
    // Maximal contiguous runs, delta-coded where possible, otherwise a glyph
    // array. This avoids one segment per glyph for non-linear dense maps.
    let mut segments = Vec::<(u16, u16, Vec<u16>)>::new();
    for (&cp, &gid) in mapping.range(..0xffff) {
        if let Some(last) = segments.last_mut()
            && u32::from(last.1) + 1 == cp
        {
            last.1 = cp as u16;
            last.2.push(gid);
        } else {
            segments.push((cp as u16, cp as u16, vec![gid]));
        }
    }
    segments.push((0xffff, 0xffff, vec![0]));
    let count = segments.len();
    let mut deltas = Vec::new();
    let mut glyphs = Vec::new();
    let mut offsets = Vec::new();
    for (i, (start, _, ids)) in segments.iter().enumerate() {
        let delta = ids[0].wrapping_sub(*start);
        if ids
            .iter()
            .enumerate()
            .all(|(j, id)| *id == start.wrapping_add(j as u16).wrapping_add(delta))
        {
            deltas.push(delta);
            offsets.push(0);
        } else {
            let off = 2 * (count - i + glyphs.len());
            if off > 65535 {
                return Ok(None);
            }
            deltas.push(0);
            offsets.push(off as u16);
            glyphs.extend(ids);
        }
    }
    let length = 16 + 8 * count + 2 * glyphs.len();
    if length > 65535 {
        return Ok(None);
    }
    let log = count.ilog2() as u16;
    let search = 2 * (1 << log);
    let mut out = buffer(length)?;
    for n in [
        4,
        to16(length)?,
        0,
        to16(2 * count)?,
        search,
        log,
        to16(2 * count)? - search,
    ] {
        p16(&mut out, n);
    }
    for (_, end, _) in &segments {
        p16(&mut out, *end);
    }
    p16(&mut out, 0);
    for (start, _, _) in &segments {
        p16(&mut out, *start);
    }
    for n in deltas.into_iter().chain(offsets).chain(glyphs) {
        p16(&mut out, n);
    }
    Ok(Some(out))
}
/// Encode all mappings; invalid scalars or glyph IDs fail, never clamp/omit.
/// Unicode and Windows platform records share the same subtable bytes.
pub fn encode(mapping: &Mapping, num_glyphs: u16, limits: &Limits) -> Result<EncodedCmap> {
    check(mapping, num_glyphs, limits)?;
    // Budget the maximum possible format-12 work before constructing groups.
    let max = 16usize
        .checked_add(
            mapping
                .len()
                .checked_mul(12)
                .ok_or_else(|| invalid("cmap size overflow"))?,
        )
        .ok_or_else(|| invalid("cmap size overflow"))?;
    limits.table(max)?;
    let mut groups = Vec::<(u32, u32, u16)>::new();
    for (&cp, &gid) in mapping {
        if let Some((start, end, first)) = groups.last_mut()
            && cp == *end + 1
            && u32::from(gid) == u32::from(*first) + (cp - *start)
        {
            *end = cp;
        } else {
            groups.push((cp, cp, gid));
        }
    }
    let bmp = format4(mapping)?;
    let count = if bmp.is_some() { 4 } else { 2 };
    let header = 4 + 8 * count;
    let full_offset = header + bmp.as_ref().map_or(0, Vec::len);
    let full_len = 16 + 12 * groups.len();
    let size = full_offset + full_len;
    limits.table(size)?;
    let mut out = buffer(size)?;
    p16(&mut out, 0);
    p16(&mut out, count as u16);
    let records: Vec<(u16, u16, usize)> = if bmp.is_some() {
        vec![
            (0, 3, header),
            (0, 4, full_offset),
            (3, 1, header),
            (3, 10, full_offset),
        ]
    } else {
        vec![(0, 4, full_offset), (3, 10, full_offset)]
    };
    for (platform, encoding, offset) in records {
        p16(&mut out, platform);
        p16(&mut out, encoding);
        p32(&mut out, to32(offset)?);
    }
    if let Some(b) = &bmp {
        out.extend(b);
    }
    p16(&mut out, 12);
    p16(&mut out, 0);
    p32(&mut out, to32(full_len)?);
    p32(&mut out, 0);
    p32(&mut out, to32(groups.len())?);
    for (start, end, gid) in groups {
        p32(&mut out, start);
        p32(&mut out, end);
        p32(&mut out, u32::from(gid));
    }
    Ok(EncodedCmap {
        data: out,
        has_bmp_subtable: bmp.is_some(),
    })
}
/// Decode the preferred Unicode nominal cmap through Fontations. Does not
/// silently reinterpret symbol/Macintosh maps or variation sequences as Unicode.
pub fn decode(data: &[u8], num_glyphs: u16, limits: &Limits) -> Result<Mapping> {
    limits.table(data.len())?;
    limits.glyphs(num_glyphs as usize)?;
    let cmap = Cmap::read(FontData::new(data)).map_err(invalid)?;
    if cmap.version() != 0 {
        return Err(invalid("unsupported cmap version"));
    }
    // Prefer full repertoire, then BMP; never let a BMP table shadow format 12.
    let mut selected = None;
    for (platform, encoding) in [
        (PlatformId::Windows, 10),
        (PlatformId::Unicode, 6),
        (PlatformId::Unicode, 4),
        (PlatformId::Windows, 1),
        (PlatformId::Unicode, 3),
        (PlatformId::Unicode, 2),
        (PlatformId::Unicode, 1),
        (PlatformId::Unicode, 0),
    ] {
        if let Some(record) = cmap
            .encoding_records()
            .iter()
            .find(|r| r.platform_id() == platform && r.encoding_id() == encoding)
        {
            let offset = record.subtable_offset().to_u32() as usize;
            if offset < 4 + cmap.encoding_records().len() * 8 {
                return Err(invalid("cmap subtable overlaps encoding records"));
            }
            let format = super::u16_at(data, offset)?;
            let length = match format {
                0 | 4 | 6 => usize::from(super::u16_at(data, offset + 2)?),
                10 | 12 | 13 => super::u32_at(data, offset + 4)? as usize,
                _ => return Err(invalid("unsupported nominal cmap format")),
            };
            let raw = super::bytes(data, offset, length)?;
            selected = Some(CmapSubtable::read(FontData::new(raw)).map_err(invalid)?);
            break;
        }
    }
    let table = selected.ok_or_else(|| invalid("no Unicode nominal cmap"))?;
    if !matches!(
        table,
        CmapSubtable::Format0(_)
            | CmapSubtable::Format4(_)
            | CmapSubtable::Format6(_)
            | CmapSubtable::Format10(_)
            | CmapSubtable::Format12(_)
            | CmapSubtable::Format13(_)
    ) {
        return Err(invalid("unsupported nominal cmap format"));
    }
    // Validate large group spans before iterating. A tiny malicious format 13
    // must not expand billions of codepoints or hide out-of-range values.
    match &table {
        CmapSubtable::Format4(t) => {
            let count = usize::from(t.seg_count_x2()) / 2;
            if count == 0
                || t.start_code().len() != count
                || t.end_code().len() != count
                || t.id_delta().len() != count
                || t.id_range_offsets().len() != count
                || t.seg_count_x2() % 2 != 0
                || t.end_code().last().is_none_or(|e| e.get() != 65535)
            {
                return Err(invalid(
                    "invalid format-4 segment count or terminal segment",
                ));
            }
            let mut previous = None;
            let mut expanded = 0usize;
            for i in 0..count {
                let start = t.start_code()[i].get();
                let end = t.end_code()[i].get();
                if start > end
                    || previous.is_some_and(|p| p >= start)
                    || (start <= 0xdfff && end >= 0xd800)
                {
                    return Err(invalid("invalid format-4 segment range"));
                }
                previous = Some(end);
                let offset = usize::from(t.id_range_offsets()[i].get());
                if offset != 0
                    && (offset % 2 != 0
                        || offset / 2 < count - i
                        || offset / 2 - (count - i) + usize::from(end - start)
                            >= t.glyph_id_array().len())
                {
                    return Err(invalid("invalid format-4 glyph array offset"));
                }
                // The mandatory sentinel is not a mapping and consumes no budget.
                expanded += usize::from(end - start) + usize::from(end != 65535);
                bounded(expanded, limits.max_cmap_mappings, "sfnt cmap mappings")?;
            }
            if t.map_codepoint(65535u32).is_some_and(|g| g.to_u32() != 0) {
                return Err(invalid("format-4 sentinel must map to missing glyph"));
            }
        }
        CmapSubtable::Format0(t) => {
            if t.glyph_id_array().len() != 256 {
                return Err(invalid("truncated format-0 glyph array"));
            }
        }
        CmapSubtable::Format6(t) => {
            if t.glyph_id_array().len() != usize::from(t.entry_count()) {
                return Err(invalid("truncated format-6 glyph array"));
            }
            if u32::from(t.first_code()) + u32::from(t.entry_count()) > 65536 {
                return Err(invalid("format-6 range overflow"));
            }
            bounded(
                t.entry_count() as usize,
                limits.max_cmap_mappings,
                "sfnt cmap mappings",
            )?;
        }
        CmapSubtable::Format10(t) => {
            if t.glyph_id_array().len() != t.num_chars() as usize {
                return Err(invalid("truncated format-10 glyph array"));
            }
            let end = t
                .start_char_code()
                .checked_add(t.num_chars())
                .ok_or_else(|| invalid("format-10 range overflow"))?;
            if end > 0x110000 {
                return Err(invalid("format-10 range overflow"));
            }
            bounded(
                t.num_chars() as usize,
                limits.max_cmap_mappings,
                "sfnt cmap mappings",
            )?;
        }
        CmapSubtable::Format12(t) => {
            if t.groups().len() != t.num_groups() as usize {
                return Err(invalid("truncated format-12 groups"));
            }
            validate_groups(
                t.groups()
                    .iter()
                    .map(|g| (g.start_char_code(), g.end_char_code(), g.start_glyph_id())),
                num_glyphs,
                limits,
                false,
            )?;
        }
        CmapSubtable::Format13(t) => {
            if t.groups().len() != t.num_groups() as usize {
                return Err(invalid("truncated format-13 groups"));
            }
            validate_groups(
                t.groups()
                    .iter()
                    .map(|g| (g.start_char_code(), g.end_char_code(), g.glyph_id())),
                num_glyphs,
                limits,
                true,
            )?;
        }
        _ => {}
    }
    let mut result = Mapping::new();
    let mut previous = None;
    let mut count = 0;
    // Fontations exposes format 0's bytes but its generic iterator does not
    // enumerate that variant. Handle the fixed 256-byte array explicitly.
    let pairs: Box<dyn Iterator<Item = (u32, read_fonts::types::GlyphId)> + '_> = match &table {
        CmapSubtable::Format0(t) => Box::new(
            t.glyph_id_array()
                .iter()
                .enumerate()
                .map(|(cp, gid)| (cp as u32, read_fonts::types::GlyphId::from(u32::from(*gid)))),
        ),
        _ => Box::new(table.iter()),
    };
    for (cp, gid) in pairs {
        if gid.to_u32() == 0 {
            continue;
        }
        count += 1;
        bounded(count, limits.max_cmap_mappings, "sfnt cmap mappings")?;
        if char::from_u32(cp).is_none() || gid.to_u32() >= u32::from(num_glyphs) {
            return Err(invalid("invalid cmap scalar or glyph ID"));
        }
        if previous.is_some_and(|p| p >= cp) {
            return Err(invalid("unordered or overlapping cmap segments"));
        }
        previous = Some(cp);
        if gid.to_u32() != 0 {
            result.insert(cp, gid.to_u32() as u16);
        }
    }
    Ok(result)
}
fn validate_groups(
    groups: impl Iterator<Item = (u32, u32, u32)>,
    num_glyphs: u16,
    limits: &Limits,
    constant: bool,
) -> Result<()> {
    let mut count = 0usize;
    let mut previous = None;
    for (start, end, gid) in groups {
        if start > end
            || end > 0x10ffff
            || (start <= 0xdfff && end >= 0xd800)
            || previous.is_some_and(|p| p >= start)
        {
            return Err(invalid("invalid cmap group range"));
        }
        previous = Some(end);
        let span = end - start;
        let last = if constant {
            gid
        } else {
            gid.checked_add(span)
                .ok_or_else(|| invalid("cmap glyph overflow"))?
        };
        if last >= u32::from(num_glyphs) {
            return Err(invalid("cmap group glyph outside maxp"));
        }
        count = count
            .checked_add(span as usize + 1)
            .ok_or_else(|| invalid("cmap count overflow"))?;
        bounded(count, limits.max_cmap_mappings, "sfnt cmap mappings")?;
    }
    Ok(())
}
