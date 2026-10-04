use super::*;
use std::collections::BTreeSet;
pub(super) fn word(b: &[u8], at: usize) -> Result<u16> {
    let s = slice(b, at, 2)?;
    Ok(u16::from_le_bytes([s[0], s[1]]))
}
pub(super) fn dword(b: &[u8], at: usize) -> Result<u32> {
    let s = slice(b, at, 4)?;
    Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}
pub(super) fn slice(b: &[u8], at: usize, len: usize) -> Result<&[u8]> {
    b.get(
        at..at
            .checked_add(len)
            .ok_or_else(|| invalid("CPI offset overflow"))?,
    )
    .ok_or_else(|| invalid("truncated CPI structure or bitmap"))
}
fn pointer(v: u32, o: &CpiReadOptions) -> usize {
    match o.pointer_mode {
        PointerMode::Linear => v as usize,
        PointerMode::Segmented => ((v >> 16) as usize) * 16 + (v & 65535) as usize,
    }
}
pub(super) fn parse(
    b: &[u8],
    o: &CpiReadOptions,
    l: &ResourceLimits,
    retained: usize,
    asset_bytes: usize,
) -> Result<(FontSet, Report)> {
    check(b.len(), l.max_input_bytes, "CPI input bytes")?;
    if super::cpx::signature(b) {
        let data = super::cpx::unpack(b, l)?;
        let retained = retained
            .checked_add(data.len())
            .and_then(|n| n.checked_add(1024))
            .ok_or_else(|| invalid("CPX memory overflow"))?;
        let (mut set, report) = parse_unpacked(&data, o, l, retained, asset_bytes)?;
        set.properties.push(Property::new("cpi.container", "cpx"));
        return Ok((set, report));
    }
    parse_unpacked(b, o, l, retained, asset_bytes)
}
fn parse_unpacked(
    b: &[u8],
    o: &CpiReadOptions,
    l: &ResourceLimits,
    retained: usize,
    asset_bytes: usize,
) -> Result<(FontSet, Report)> {
    let version = if b.starts_with(b"\xffFONT   ") {
        Version::Font
    } else if b.starts_with(b"\xffFONT.NT") {
        Version::FontNt
    } else if b.starts_with(b"\x7fDRFONT ") {
        Version::DrFont
    } else {
        Version::Cp
    };
    let mut budget = Budget::new(retained, l)?;
    let mut report = Report::default();
    let mut set = FontSet {
        source_format: Some("cpi".into()),
        ..Default::default()
    };
    let mut ranges = Vec::new();
    let mut used = 0usize;
    let mut record = |start: usize, len: usize| -> Result<()> {
        slice(b, start, len)?;
        used = used.max(start + len);
        ranges.push((start, start + len));
        Ok(())
    };
    let (mut entry, count) = if version == Version::Cp {
        (0, 1)
    } else {
        record(0, 23)?;
        if word(b, 16)? != 1 || b[18] != 1 {
            return Err(invalid("unsupported CPI pointer count/type"));
        }
        if b[8..16].iter().any(|&x| x != 0) {
            budget.add(512)?;
            set.properties.push(Property::new(
                "cpi.file_reserved",
                PropertyValue::Bytes(b[8..16].to_vec()),
            ));
        }
        let at = pointer(dword(b, 19)?, o);
        record(at, 2)?;
        (at + 2, usize::from(word(b, at)?))
    };
    if count == 0 {
        return Err(invalid("CPI contains no codepages"));
    }
    check(count, l.max_glyphs, "CPI codepage count")?;
    let mut dr = Vec::new();
    if version == Version::DrFont {
        let n = usize::from(*b.get(23).ok_or_else(|| invalid("missing DRFONT header"))?);
        if n == 0 {
            return Err(invalid("DRFONT has no strikes"));
        }
        record(23, 1 + 5 * n)?;
        budget.add(n * 16)?;
        for i in 0..n {
            dr.push((
                usize::from(b[24 + i]),
                pointer(dword(b, 24 + n + i * 4)?, o),
            ));
        }
    }
    let mut visited = BTreeSet::new();
    let mut glyph_count = 0usize;
    for page_index in 0..count {
        if !visited.insert(entry) {
            return Err(invalid("CPI codepage chain contains a cycle"));
        }
        budget.add(64)?;
        record(entry, 28)?;
        let header_size = usize::from(word(b, entry)?);
        if header_size != 28 && header_size != 26 {
            return Err(invalid("unsupported CPI codepage entry size"));
        }
        if header_size == 26 {
            budget.add(1024usize.saturating_add(asset_bytes))?;
            note(
                &mut report,
                "cpi",
                "read.entry_size",
                None,
                None,
                None,
                Impact::Repair,
                "CPI 26-byte size marker interpreted as the documented 28-byte entry",
            );
        }
        let device = &b[entry + 8..entry + 16];
        if word(b, entry + 6)? != 1
            || [
                b"4201    ".as_slice(),
                b"4208    ",
                b"5202    ",
                b"1050    ",
            ]
            .contains(&device)
        {
            return Err(invalid(format!(
                "CPI page {page_index} is a printer or unsupported device; screen glyphs cannot be extracted"
            )));
        }
        let page = u32::from(word(b, entry + 16)?);
        let next = pointer(dword(b, entry + 2)?, o);
        let info = if version == Version::Cp {
            entry + 28
        } else {
            let p = pointer(dword(b, entry + 24)?, o);
            if version == Version::FontNt {
                entry
                    .checked_add(p)
                    .ok_or_else(|| invalid("CPI relative pointer overflow"))?
            } else {
                p
            }
        };
        record(info, 6)?;
        let mut cp_version = word(b, info)?;
        if cp_version == 0 {
            cp_version = 1;
            budget.add(1024usize.saturating_add(asset_bytes))?;
            note(
                &mut report,
                "cpi",
                "read.version_zero",
                None,
                None,
                None,
                Impact::Repair,
                "Toshiba CPI version zero treated as FONT",
            );
        }
        if !matches!(cp_version, 1 | 2) || (cp_version == 2) != (version == Version::DrFont) {
            return Err(invalid("CPI codepage format differs from container"));
        }
        let strikes = usize::from(word(b, info + 2)?);
        if strikes == 0 {
            return Err(invalid("CPI page has no strikes"));
        }
        check(strikes, l.max_glyphs, "CPI strike count")?;
        if version == Version::DrFont && strikes != dr.len() {
            return Err(invalid("DRFONT strike count differs from extended header"));
        }
        let declared = usize::from(word(b, info + 4)?);
        check(declared, l.max_table_bytes, "CPI codepage table bytes")?;
        let mut at = info + 6;
        let mut indices = None;
        if cp_version == 2 {
            let ix = at
                .checked_add(strikes * 6)
                .ok_or_else(|| invalid("DRFONT index offset overflow"))?;
            record(ix, 512)?;
            indices = Some(slice(b, ix, 512)?);
        }
        let mut page_data = 0usize;
        let mut dr_iter = dr.iter();
        for _ in 0..strikes {
            let dr_entry = dr_iter.next();
            record(at, 6)?;
            let h = u32::from(b[at]);
            let w = u32::from(b[at + 1]);
            let ya = b[at + 2];
            let xa = b[at + 3];
            let n = usize::from(word(b, at + 4)?);
            at += 6;
            if n == 0 {
                return Err(invalid("CPI strike has no characters"));
            }
            glyph_count = glyph_count
                .checked_add(n)
                .ok_or_else(|| invalid("CPI glyph count overflow"))?;
            check(glyph_count, l.max_glyphs, "CPI aggregate glyph count")?;
            let cell = cell_size(w, h, l)?;
            let bitmap_bytes = n
                .checked_mul(cell)
                .ok_or_else(|| invalid("CPI bitmap size overflow"))?;
            check(bitmap_bytes, l.max_table_bytes, "CPI bitmap table")?;
            budget.add(
                n.checked_mul(1152 + cell)
                    .ok_or_else(|| invalid("CPI model size overflow"))?,
            )?;
            budget.add(
                std::mem::size_of::<Font>()
                    .saturating_mul(2)
                    .saturating_add(512),
            )?;
            let base = if cp_version == 2 {
                let &(dr_cell, dr_offset) =
                    dr_entry.ok_or_else(|| invalid("missing DRFONT size table"))?;
                if n != 256 || dr_cell != cell {
                    return Err(invalid(
                        "DRFONT requires 256 glyphs and matching cell sizes",
                    ));
                }
                dr_offset
            } else {
                record(at, bitmap_bytes)?;
                let base = at;
                at += bitmap_bytes;
                base
            };
            page_data = page_data
                .checked_add(6 + if cp_version == 1 { bitmap_bytes } else { 0 })
                .ok_or_else(|| invalid("CPI page size overflow"))?;
            let mut font = Font::default();
            font.source_format = Some("cpi".into());
            font.metrics = metrics(w, h, 0);
            font.charset.name = Some(format!("cp{page}"));
            font.charset.code_page = Some(page);
            font.headers.push(Property::new(
                "cpi.device",
                PropertyValue::Bytes(device.to_vec()),
            ));
            if b[entry + 18..entry + 24].iter().any(|&x| x != 0) {
                font.headers.push(Property::new(
                    "cpi.entry_reserved",
                    PropertyValue::Bytes(b[entry + 18..entry + 24].to_vec()),
                ));
            }
            if xa != 0 {
                font.headers
                    .push(Property::new("cpi.xaspect", i64::from(xa)));
            }
            if ya != 0 {
                font.headers
                    .push(Property::new("cpi.yaspect", i64::from(ya)));
            }
            let fi = set.fonts.len();
            for index in 0..n {
                let bitmap_index = if let Some(ix) = indices {
                    usize::from(word(ix, index * 2)?)
                } else {
                    index
                };
                let offset = bitmap_index
                    .checked_mul(cell)
                    .and_then(|n| base.checked_add(n))
                    .ok_or_else(|| invalid("CPI bitmap offset overflow"))?;
                if cp_version == 2 {
                    record(offset, cell)?;
                }
                let raw = slice(b, offset, cell)?;
                let mut g = Glyph::new(Bitmap::<crate::bitmap::Mono>::from_raw(
                    w,
                    h,
                    raw,
                    bitmap_limits(l),
                )?);
                g.metrics = glyph_metrics(Bounds::new(w, h, 0, 0));
                g.codepoints.push(Codepoint::Legacy(index as u32));
                if g.bitmap.as_bytes() != raw {
                    budget.add(1024usize.saturating_add(asset_bytes))?;
                    note(
                        &mut report,
                        "cpi",
                        "read.padding",
                        Some(fi),
                        Some(index),
                        None,
                        Impact::Repair,
                        "CPI unused row padding normalized",
                    );
                }
                font.add_glyph(g)?;
            }
            set.fonts.push(font);
        }
        if declared != page_data {
            budget.add(1024usize.saturating_add(asset_bytes))?;
            note(
                &mut report,
                "cpi",
                "read.page_size",
                None,
                None,
                None,
                Impact::Repair,
                format!("CPI declared page size {declared} differs from parsed {page_data}"),
            );
        }
        if page_index + 1 < count {
            entry = if version == Version::FontNt {
                entry
                    .checked_add(next)
                    .ok_or_else(|| invalid("CPI next pointer overflow"))?
            } else {
                next
            };
            if entry == 0 || next == 0xffff_ffff {
                return Err(invalid("CPI chain ends before declared page count"));
            }
        }
    }
    // Ignore zero alignment, preserve all tail bytes, and explicitly disclose
    // unreferenced nonzero interior bytes without retaining a second full input.
    ranges.sort_unstable();
    let mut end = 0;
    let mut gaps = false;
    for (start, stop) in ranges {
        if start > end && b[end..start].iter().any(|&x| x != 0) {
            gaps = true;
        }
        end = end.max(stop);
    }
    if gaps {
        budget.add(1024usize.saturating_add(asset_bytes))?;
        note(
            &mut report,
            "cpi",
            "read.unreferenced",
            None,
            None,
            None,
            Impact::Loss,
            "Unreferenced nonzero interior bytes are not retained",
        );
    }
    if used < b.len() {
        let tail = &b[used..];
        check(tail.len(), l.max_table_bytes, "CPI trailing bytes")?;
        budget.add(tail.len().saturating_add(512))?;
        set.properties.push(Property::new(
            "cpi.trailing",
            PropertyValue::Bytes(tail.to_vec()),
        ));
    }
    Ok((set, report))
}
