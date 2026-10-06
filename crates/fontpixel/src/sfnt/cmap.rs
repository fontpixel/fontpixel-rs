//! Complete Unicode cmap writing and bounded nominal-map reading.
use super::working::{Budget, Buffer, Owned, Vector};
use super::{Limits, bounded, invalid, to16, to32};
use crate::Result;
use read_fonts::{
    FontData, FontRead,
    tables::cmap::{Cmap, CmapSubtable, PlatformId},
};
use std::collections::{BTreeMap, BTreeSet};

/// A Unicode scalar to glyph ID mapping, in ascending scalar order.
pub type Mapping = BTreeMap<u32, u16>;
/// Serialized cmap and a compatibility fact callers can report.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct EncodedCmap {
    /// Complete table bytes, including full-Unicode format 12 unless
    /// [`FullRecords::WhenNeeded`] found format 4 sufficient.
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
#[derive(Clone, Copy)]
struct BmpSegment {
    start: u16,
    end: u16,
    delta: u16,
    linear: bool,
}
fn format4(mapping: &Mapping, budget: &Budget) -> Result<Option<Owned<Vec<u8>>>> {
    let mut count = 1usize; // Mandatory missing-glyph sentinel.
    let mut previous = None;
    for (&cp, _) in mapping.range(..0xffff) {
        if previous.is_none_or(|last| last + 1 != cp) {
            count += 1;
        }
        previous = Some(cp);
    }
    let mut segments = Vector::<BmpSegment>::with_capacity(budget, count)?;
    for (&cp, &gid) in mapping.range(..0xffff) {
        if let Some(last) = segments.last_mut()
            && u32::from(last.end) + 1 == cp
        {
            last.end = cp as u16;
            last.linear &= gid == (cp as u16).wrapping_add(last.delta);
        } else {
            segments.push(BmpSegment {
                start: cp as u16,
                end: cp as u16,
                delta: gid.wrapping_sub(cp as u16),
                linear: true,
            })?;
        }
    }
    segments.push(BmpSegment {
        start: 0xffff,
        end: 0xffff,
        delta: 1,
        linear: true,
    })?;
    let mut glyph_count = 0usize;
    for (index, segment) in segments.iter().enumerate() {
        if !segment.linear {
            let offset = 2 * (count - index + glyph_count);
            if offset > 65535 {
                return Ok(None);
            }
            glyph_count += usize::from(segment.end - segment.start) + 1;
        }
    }
    let length = 16 + 8 * count + 2 * glyph_count;
    if length > 65535 {
        return Ok(None);
    }
    let log = count.ilog2() as u16;
    let search = 2 * (1 << log);
    let mut out = Buffer::with_capacity(budget, length)?;
    for n in [
        4,
        to16(length)?,
        0,
        to16(2 * count)?,
        search,
        log,
        to16(2 * count)? - search,
    ] {
        out.extend(&n.to_be_bytes())?;
    }
    for segment in segments.iter() {
        out.extend(&segment.end.to_be_bytes())?;
    }
    out.extend(&0u16.to_be_bytes())?;
    for segment in segments.iter() {
        out.extend(&segment.start.to_be_bytes())?;
    }
    for segment in segments.iter() {
        let delta = if segment.linear { segment.delta } else { 0 };
        out.extend(&delta.to_be_bytes())?;
    }
    let mut glyph_count = 0;
    for (index, segment) in segments.iter().enumerate() {
        let offset = if segment.linear {
            0
        } else {
            let offset = to16(2 * (count - index + glyph_count))?;
            glyph_count += usize::from(segment.end - segment.start) + 1;
            offset
        };
        out.extend(&offset.to_be_bytes())?;
    }
    for segment in segments.iter().filter(|segment| !segment.linear) {
        for (_, &gid) in mapping.range(u32::from(segment.start)..=u32::from(segment.end)) {
            out.extend(&gid.to_be_bytes())?;
        }
    }
    Ok(Some(out.into_owned()))
}
/// Encode all mappings; invalid scalars or glyph IDs fail, never clamp/omit.
/// Unicode and Windows platform records share the same subtable bytes.
pub fn encode(mapping: &Mapping, num_glyphs: u16, limits: &Limits) -> Result<EncodedCmap> {
    encode_records(mapping, num_glyphs, limits, true)
}
/// Which cmap encoding records list the full-repertoire format-12 subtable.
///
/// The format-4 subtable (BMP scalars except U+FFFF) is written under (0, 3)
/// and (3, 1) whenever it is representable. Every policy decodes to the same
/// mapping: no scalar is ever dropped.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
pub enum FullRecords {
    /// List format 12 under both (0, 4) and (3, 10) (native default).
    #[default]
    Always,
    /// Omit the redundant (0, 4) record for a BMP-only map; maps with a
    /// supplementary-plane scalar keep both records.
    #[cfg_attr(feature = "serde", serde(rename = "non-bmp"))]
    NonBmpOnly,
    /// Write format 12 only when format 4 cannot hold the whole map (a
    /// supplementary scalar, U+FFFF, or no representable format 4), and list
    /// it under (3, 10) only. A map that format 4 covers gets no format 12.
    WhenNeeded,
}
/// [`encode`] with an explicit Unicode-platform record choice for maps without
/// supplementary-plane scalars. `unicode_full_for_bmp == true` (the [`encode`]
/// default) lists the format-12 subtable under both (0, 4) and (3, 10);
/// `false` omits the redundant (0, 4) record for a BMP-only map. Maps with a
/// supplementary scalar always keep both records. Subtable bytes, and therefore
/// every decoded mapping, are identical either way.
pub fn encode_records(
    mapping: &Mapping,
    num_glyphs: u16,
    limits: &Limits,
    unicode_full_for_bmp: bool,
) -> Result<EncodedCmap> {
    encode_with_records(
        mapping,
        num_glyphs,
        limits,
        if unicode_full_for_bmp {
            FullRecords::Always
        } else {
            FullRecords::NonBmpOnly
        },
    )
}
/// [`encode`] with an explicit [`FullRecords`] policy.
pub fn encode_with_records(
    mapping: &Mapping,
    num_glyphs: u16,
    limits: &Limits,
    records: FullRecords,
) -> Result<EncodedCmap> {
    let budget = Budget::new(limits.resources.max_total_bytes, "sfnt working bytes");
    let (encoded, _lease) =
        encode_records_with_budget(mapping, num_glyphs, limits, &budget, records)?.into_parts();
    Ok(encoded)
}
/// Encode into the caller's ledger without charging its borrowed input map.
pub(crate) fn encode_with_budget(
    mapping: &Mapping,
    num_glyphs: u16,
    limits: &Limits,
    budget: &Budget,
) -> Result<Owned<EncodedCmap>> {
    encode_records_with_budget(mapping, num_glyphs, limits, budget, FullRecords::Always)
}
/// [`encode_with_budget`] with an explicit [`FullRecords`] policy.
pub(crate) fn encode_records_with_budget(
    mapping: &Mapping,
    num_glyphs: u16,
    limits: &Limits,
    budget: &Budget,
    records: FullRecords,
) -> Result<Owned<EncodedCmap>> {
    check(mapping, num_glyphs, limits)?;
    let max = mapping
        .len()
        .checked_mul(12)
        .and_then(|n| n.checked_add(16))
        .ok_or_else(|| invalid("cmap size overflow"))?;
    limits.table(max)?;
    let mut group_count = 0usize;
    let mut previous: Option<(u32, u16)> = None;
    for (&cp, &gid) in mapping {
        if previous.is_none_or(|(p, g)| p + 1 != cp || u32::from(g) + 1 != u32::from(gid)) {
            group_count += 1;
        }
        previous = Some((cp, gid));
    }
    let bmp = format4(mapping, budget)?;
    let has_bmp_subtable = bmp.is_some();
    let supplementary = mapping.last_key_value().is_some_and(|(&cp, _)| cp > 0xffff);
    let (unicode_full, windows_full) = match records {
        FullRecords::Always => (true, true),
        FullRecords::NonBmpOnly => (supplementary, true),
        FullRecords::WhenNeeded => (
            false,
            !has_bmp_subtable || supplementary || mapping.contains_key(&0xffff),
        ),
    };
    let count = if has_bmp_subtable { 2 } else { 0 }
        + usize::from(unicode_full)
        + usize::from(windows_full);
    let header = 4 + 8 * count;
    let full_offset = header + bmp.as_ref().map_or(0, |bytes| bytes.len());
    let full_len = if windows_full {
        16 + 12 * group_count
    } else {
        0
    };
    let size = full_offset + full_len;
    limits.table(size)?;
    let mut out = Buffer::with_capacity(budget, size)?;
    out.extend(&0u16.to_be_bytes())?;
    out.extend(&(count as u16).to_be_bytes())?;
    let entries = [
        (0u16, 3u16, header, has_bmp_subtable),
        (0, 4, full_offset, unicode_full),
        (3, 1, header, has_bmp_subtable),
        (3, 10, full_offset, windows_full),
    ];
    for &(platform, encoding, offset, present) in &entries {
        if !present {
            continue;
        }
        out.extend(&platform.to_be_bytes())?;
        out.extend(&encoding.to_be_bytes())?;
        out.extend(&to32(offset)?.to_be_bytes())?;
    }
    if let Some(bytes) = &bmp {
        out.extend(bytes)?;
    }
    drop(bmp);
    if windows_full {
        format12(&mut out, mapping, full_len, group_count)?;
    }
    let (data, lease) = out.into_parts();
    Ok(Owned::from_parts(
        EncodedCmap {
            data,
            has_bmp_subtable,
        },
        lease,
    ))
}
/// Append the format-12 subtable of every mapping.
fn format12(
    out: &mut Buffer,
    mapping: &Mapping,
    full_len: usize,
    group_count: usize,
) -> Result<()> {
    out.extend(&12u16.to_be_bytes())?;
    out.extend(&0u16.to_be_bytes())?;
    out.extend(&to32(full_len)?.to_be_bytes())?;
    out.extend(&0u32.to_be_bytes())?;
    out.extend(&to32(group_count)?.to_be_bytes())?;
    let mut current: Option<(u32, u32, u16)> = None;
    for (&cp, &gid) in mapping {
        if let Some((start, end, first)) = &mut current
            && cp == *end + 1
            && u32::from(gid) == u32::from(*first) + (cp - *start)
        {
            *end = cp;
        } else {
            if let Some((start, end, first)) = current {
                out.extend(&start.to_be_bytes())?;
                out.extend(&end.to_be_bytes())?;
                out.extend(&u32::from(first).to_be_bytes())?;
            }
            current = Some((cp, cp, gid));
        }
    }
    if let Some((start, end, first)) = current {
        out.extend(&start.to_be_bytes())?;
        out.extend(&end.to_be_bytes())?;
        out.extend(&u32::from(first).to_be_bytes())?;
    }
    Ok(())
}
/// Decode the preferred Unicode nominal cmap through Fontations. Does not
/// silently reinterpret symbol/Macintosh maps or variation sequences as Unicode.
pub fn decode(data: &[u8], num_glyphs: u16, limits: &Limits) -> Result<Mapping> {
    let budget = Budget::new(limits.resources.max_total_bytes, "sfnt working bytes");
    let (mapping, _lease) =
        decode_with_budget(data, num_glyphs, limits, None, &budget)?.into_parts();
    Ok(mapping)
}
/// Validate the entire nominal map but allocate entries only for the requested
/// scalars. Work limits and invalid unselected mappings are still checked.
pub(crate) fn decode_with_budget(
    data: &[u8],
    num_glyphs: u16,
    limits: &Limits,
    selection: Option<&BTreeSet<u32>>,
    budget: &Budget,
) -> Result<Owned<Mapping>> {
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
    let mut lease = budget.reserve(0)?;
    let mut result = Mapping::new();
    let mut previous = None;
    let mut count = 0;
    let mut insert = |cp: u32, gid: read_fonts::types::GlyphId| -> Result<()> {
        if gid.to_u32() == 0 {
            return Ok(());
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
        if selection.is_none_or(|codes| codes.contains(&cp)) {
            lease.grow_to(budget.tree_bytes::<u32, u16>(result.len() + 1)?)?;
            result.insert(cp, gid.to_u32() as u16);
        }
        Ok(())
    };
    // Avoid an unaccounted boxed iterator; both paths feed the identical checks.
    match &table {
        CmapSubtable::Format0(table) => {
            for (cp, &gid) in table.glyph_id_array().iter().enumerate() {
                insert(cp as u32, read_fonts::types::GlyphId::from(u32::from(gid)))?;
            }
        }
        _ => {
            for (cp, gid) in table.iter() {
                insert(cp, gid)?;
            }
        }
    }
    Ok(Owned::from_parts(result, lease))
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

#[cfg(test)]
mod working_tests {
    use super::*;

    fn small_mapping() -> Mapping {
        [
            (65, 3),
            (66, 7),
            (67, 8),
            (69, 10),
            (0xffff, 9),
            (0x1f600, 12),
        ]
        .into()
    }

    #[test]
    fn bmp_runs_keep_the_existing_wire_encoding() {
        let budget = Budget::new(4096, "test cmap bytes");
        let bytes = format4(&small_mapping(), &budget).unwrap().unwrap();
        let words = [
            4u16, 46, 0, 6, 4, 1, 2, 67, 69, 0xffff, 0, 65, 69, 0xffff, 0, 65477, 1, 6, 0, 0, 3, 7,
            8,
        ];
        let expected: Vec<_> = words.into_iter().flat_map(u16::to_be_bytes).collect();
        assert_eq!(bytes.as_slice(), expected);
    }

    fn records(data: &[u8]) -> Vec<(u16, u16)> {
        let count = usize::from(u16::from_be_bytes([data[2], data[3]]));
        (0..count)
            .map(|i| {
                let at = 4 + 8 * i;
                (
                    u16::from_be_bytes([data[at], data[at + 1]]),
                    u16::from_be_bytes([data[at + 2], data[at + 3]]),
                )
            })
            .collect()
    }

    #[test]
    fn record_policies_keep_every_mapping_and_the_default_bytes() {
        let limits = Limits::default();
        let bmp: Mapping = [(65, 1), (66, 2), (0x4e00, 3)].into();
        let supplementary: Mapping = [(65, 1), (0x1f600, 2)].into();
        let ffff: Mapping = [(65, 1), (0xffff, 2)].into();
        for mapping in [&bmp, &supplementary, &ffff, &small_mapping()] {
            let n = 13;
            let native = encode(mapping, n, &limits).unwrap().data;
            assert_eq!(
                encode_with_records(mapping, n, &limits, FullRecords::Always)
                    .unwrap()
                    .data,
                native
            );
            assert_eq!(
                encode_with_records(mapping, n, &limits, FullRecords::NonBmpOnly)
                    .unwrap()
                    .data,
                encode_records(mapping, n, &limits, false).unwrap().data
            );
            for policy in [
                FullRecords::Always,
                FullRecords::NonBmpOnly,
                FullRecords::WhenNeeded,
            ] {
                let data = encode_with_records(mapping, n, &limits, policy)
                    .unwrap()
                    .data;
                assert_eq!(&decode(&data, n, &limits).unwrap(), mapping);
            }
        }
        let when = |m: &Mapping| {
            records(
                &encode_with_records(m, 13, &limits, FullRecords::WhenNeeded)
                    .unwrap()
                    .data,
            )
        };
        // fontTools' set: format 4 only, plus (3,10) when format 4 is not enough.
        assert_eq!(when(&bmp), [(0, 3), (3, 1)]);
        assert_eq!(when(&supplementary), [(0, 3), (3, 1), (3, 10)]);
        // U+FFFF lives only in format 12, so it is kept rather than dropped.
        assert_eq!(when(&ffff), [(0, 3), (3, 1), (3, 10)]);
        let data = encode_with_records(&bmp, 13, &limits, FullRecords::WhenNeeded)
            .unwrap()
            .data;
        let format4 = encode(&bmp, 13, &limits).unwrap().data;
        assert!(data.len() < format4.len());
        assert_eq!(
            records(&format4),
            [(0, 3), (0, 4), (3, 1), (3, 10)],
            "native default"
        );
    }

    #[test]
    fn encoder_counts_bmp_and_final_output_overlap() {
        let mapping = small_mapping();
        let limits = Limits::default();
        // Three eight-byte segment records + 46-byte BMP is the earlier peak;
        // the 46-byte BMP and 158-byte complete cmap then coexist.
        assert_eq!(std::mem::size_of::<BmpSegment>(), 8);
        let budget = Budget::new(204, "test cmap bytes");
        let encoded = encode_with_budget(&mapping, 13, &limits, &budget).unwrap();
        assert_eq!(encoded.data.len(), 158);
        assert_eq!(budget.remaining(), 46);
        assert_eq!(decode(&encoded.data, 13, &limits).unwrap(), mapping);
        drop(encoded);
        assert_eq!(budget.remaining(), 204);
        let short = Budget::new(203, "test cmap bytes");
        assert!(matches!(
            encode_with_budget(&mapping, 13, &limits, &short),
            Err(crate::Error::ResourceLimit { .. })
        ));
        assert_eq!(short.remaining(), 203);
    }

    fn format13() -> Vec<u8> {
        let mut data = Vec::new();
        for n in [0u16, 1, 3, 10] {
            data.extend(n.to_be_bytes());
        }
        data.extend(12u32.to_be_bytes());
        data.extend(13u16.to_be_bytes());
        data.extend(0u16.to_be_bytes());
        for n in [28u32, 0, 1, 0x20, 0x2000, 1] {
            data.extend(n.to_be_bytes());
        }
        data
    }

    #[test]
    fn selection_avoids_full_map_storage_but_keeps_source_work_limit() {
        let data = format13();
        let selected = BTreeSet::from([65]);
        let budget = Budget::new(1024, "test cmap bytes");
        let mapping =
            decode_with_budget(&data, 2, &Limits::default(), Some(&selected), &budget).unwrap();
        assert_eq!(*mapping, BTreeMap::from([(65, 1)]));
        drop(mapping);
        let limits = Limits {
            max_cmap_mappings: 100,
            ..Limits::default()
        };
        assert!(matches!(
            decode_with_budget(&data, 2, &limits, Some(&selected), &budget),
            Err(crate::Error::ResourceLimit {
                resource: "sfnt cmap mappings",
                ..
            })
        ));
        let limits = Limits::default();
        let live = budget.reserve(512).unwrap();
        assert!(matches!(
            decode_with_budget(&data, 2, &limits, Some(&selected), &budget),
            Err(crate::Error::ResourceLimit { .. })
        ));
        drop(live);
        assert_eq!(budget.remaining(), 1024);
    }

    #[test]
    fn invalid_unselected_mapping_still_fails() {
        let mut data = Vec::new();
        for n in [0u16, 1, 3, 1] {
            data.extend(n.to_be_bytes());
        }
        data.extend(12u32.to_be_bytes());
        for n in [0u16, 262, 0] {
            data.extend(n.to_be_bytes());
        }
        data.resize(12 + 262, 0);
        data[18 + 65] = 1;
        data[18 + 66] = 2;
        let selected = BTreeSet::from([65]);
        let budget = Budget::new(1024, "test cmap bytes");
        assert!(matches!(
            decode_with_budget(&data, 2, &Limits::default(), Some(&selected), &budget),
            Err(crate::Error::InvalidInput(_))
        ));
        assert_eq!(budget.remaining(), 1024);
    }
}
