use super::*;
use crate::model::GlyphId;
struct Strike<'a> {
    font: &'a Font,
    page: u16,
    device: [u8; 8],
    reserved: [u8; 6],
    width: u8,
    height: u8,
    xa: u8,
    ya: u8,
    cells: Vec<Option<(GlyphId, &'a Glyph)>>,
}
fn byte_prop(font: &Font, key: &str) -> Result<u8> {
    match font.headers.get(key) {
        None => Ok(0),
        Some(PropertyValue::Integer(n)) => {
            u8::try_from(*n).map_err(|_| invalid("CPI aspect must fit a byte"))
        }
        _ => Err(invalid("CPI aspect property must be integer")),
    }
}
fn bytes_prop<const N: usize>(font: &Font, key: &str, default: [u8; N]) -> Result<[u8; N]> {
    match font.headers.get(key) {
        None => Ok(default),
        Some(PropertyValue::Bytes(b)) => b
            .as_slice()
            .try_into()
            .map_err(|_| invalid("CPI header property has incorrect byte length")),
        _ => Err(invalid("CPI header property must be bytes")),
    }
}
fn plan<'a>(s: &'a FontSet, o: &CpiWriteOptions, r: &mut BoundedReport) -> Result<Vec<Strike<'a>>> {
    let mut out = Vec::new();
    let mut total = 0usize;
    for (fi, font) in s.fonts.iter().enumerate() {
        let page = font
            .charset
            .code_page
            .and_then(|n| u16::try_from(n).ok())
            .or(o.code_page)
            .or_else(|| {
                font.charset
                    .name
                    .as_deref()
                    .and_then(|s| s.strip_prefix("cp"))
                    .and_then(|s| s.parse::<u16>().ok())
            });
        let page = page.ok_or_else(|| {
            invalid("CPI output requires code_page metadata or an explicit fallback")
        })?;
        let b = font
            .metrics
            .bounds
            .or_else(|| font.glyphs().next().map(|(_, g)| g.metrics.bounds))
            .ok_or_else(|| invalid("CPI requires a nonempty strike"))?;
        let width = u8::try_from(b.width).map_err(|_| invalid("CPI cell width exceeds 255"))?;
        let height = u8::try_from(b.height).map_err(|_| invalid("CPI cell height exceeds 255"))?;
        cell_size(b.width, b.height, &o.limits)?;
        let max = font
            .glyphs()
            .flat_map(|(_, g)| &g.codepoints)
            .filter_map(|cp| match cp {
                Codepoint::Legacy(n) if *n < 65535 => Some(*n as usize),
                _ => None,
            })
            .max();
        let count = if o.version == Version::DrFont {
            256
        } else {
            max.map_or(0, |n| n + 1)
        };
        check(count, o.limits.max_glyphs, "CPI strike glyph count")?;
        if count == 0 {
            for (id, g) in font.glyphs() {
                for cp in &g.codepoints {
                    note(
                        r,
                        "cpi",
                        "write.mapping",
                        Some(fi),
                        Some(id.index()),
                        Some(cp.value()),
                        Impact::Loss,
                        "CPI requires a legacy positional code",
                    );
                }
            }
            return Err(invalid("CPI strike contains no encodable legacy codes"));
        }
        total = total
            .checked_add(count)
            .ok_or_else(|| invalid("CPI output glyph count overflow"))?;
        check(
            total,
            o.limits.max_glyphs,
            "CPI aggregate output glyph count",
        )?;
        r.reserve(count.saturating_mul(std::mem::size_of::<Option<(GlyphId, &Glyph)>>()))?;
        let mut cells: Vec<Option<(GlyphId, &Glyph)>> = vec![None; count];
        for (id, g) in font.glyphs() {
            let mut mapped = false;
            for &cp in &g.codepoints {
                if let Codepoint::Legacy(n) = cp
                    && let Some(slot) = cells.get_mut(n as usize)
                {
                    if let Some((old, _)) = slot {
                        note(
                            r,
                            "cpi",
                            "write.duplicate",
                            Some(fi),
                            Some(old.index()),
                            Some(n),
                            Impact::Loss,
                            "Duplicate positional mapping replaced by the later glyph",
                        );
                    }
                    *slot = Some((id, g));
                    mapped = true;
                } else {
                    note(
                        r,
                        "cpi",
                        "write.mapping",
                        Some(fi),
                        Some(id.index()),
                        Some(cp.value()),
                        Impact::Loss,
                        "Mapping outside the CPI positional domain omitted",
                    );
                }
            }
            if !mapped {
                note(
                    r,
                    "cpi",
                    "write.unencoded",
                    Some(fi),
                    Some(id.index()),
                    None,
                    Impact::Loss,
                    "Glyph without an encodable CPI position omitted",
                );
            }
        }
        out.push(Strike {
            font,
            page,
            device: bytes_prop(font, "cpi.device", *b"EGA     ")?,
            reserved: bytes_prop(font, "cpi.entry_reserved", [0; 6])?,
            width,
            height,
            xa: byte_prop(font, "cpi.xaspect")?,
            ya: byte_prop(font, "cpi.yaspect")?,
            cells,
        });
    }
    if out.is_empty() {
        return Err(invalid("CPI output requires at least one strike"));
    }
    Ok(out)
}
fn u16_out(out: &mut Output, v: usize) -> Result<()> {
    out.add(
        &u16::try_from(v)
            .map_err(|_| invalid("CPI 16-bit field overflow"))?
            .to_le_bytes(),
    )
}
fn u32_out(out: &mut Output, v: usize) -> Result<()> {
    out.add(
        &u32::try_from(v)
            .map_err(|_| invalid("CPI 32-bit offset overflow"))?
            .to_le_bytes(),
    )
}
fn screen(out: &mut Output, s: &Strike<'_>) -> Result<()> {
    out.add(&[s.height, s.width, s.ya, s.xa])?;
    u16_out(out, s.cells.len())
}
fn bitmaps(out: &mut Output, s: &Strike<'_>, o: &CpiWriteOptions) -> Result<()> {
    let cell = Bounds::new(u32::from(s.width), u32::from(s.height), 0, 0);
    let size = cell_size(cell.width, cell.height, &o.limits)?;
    let blank = vec![0; size];
    for g in &s.cells {
        if let Some((_, g)) = g {
            out.add(raster(g, cell, o.gray_threshold, &o.limits)?.as_bytes())?;
        } else {
            out.add(&blank)?;
        }
    }
    Ok(())
}
pub(super) fn write(s: &FontSet, o: &CpiWriteOptions) -> Result<(Vec<u8>, Report)> {
    s.validate(&o.limits)?;
    let retained = model_cost(s, &o.limits)?;
    let mut report = BoundedReport::new(retained, &o.limits)?;
    let result = (|| {
        report.reserve(s.fonts.len().saturating_mul(512))?;
        let strikes = plan(s, o, &mut report)?;
        // Consecutive groups retain file-order strikes, including repeated pages.
        let mut groups: Vec<std::ops::Range<usize>> = Vec::new();
        let mut start = 0;
        for i in 1..=strikes.len() {
            if i == strikes.len()
                || strikes[i].page != strikes[start].page
                || strikes[i].device != strikes[start].device
                || strikes[i].reserved != strikes[start].reserved
            {
                groups.push(start..i);
                start = i;
            }
        }
        if o.version == Version::Cp && groups.len() != 1 {
            return Err(invalid(
                "standalone CP stores exactly one codepage/device group",
            ));
        }
        if groups.len() > 65535 {
            return Err(invalid("CPI codepage count exceeds u16"));
        }
        let dr = o.version == Version::DrFont;
        let dr_strikes = groups[0].len();
        if dr {
            if dr_strikes > 255 {
                return Err(invalid("DRFONT strike count exceeds byte"));
            }
            for group in &groups {
                if group.len() != dr_strikes {
                    return Err(invalid("DRFONT pages must have identical strike counts"));
                }
                for (i, index) in group.clone().enumerate() {
                    let a = &strikes[index];
                    let b = &strikes[i];
                    if a.width != b.width
                        || a.height != b.height
                        || cell_size(u32::from(a.width), u32::from(a.height), &o.limits)? > 255
                    {
                        return Err(invalid(
                            "DRFONT pages must have identical cell geometry with byte-sized cell lengths",
                        ));
                    }
                }
            }
            if groups.len() * 256 > 65536 {
                return Err(invalid("DRFONT shared bitmap index exceeds u16"));
            }
        }
        let mut lengths = Vec::new();
        let mut final_size = if o.version == Version::Cp {
            0
        } else {
            25 + if dr { 1 + 5 * dr_strikes } else { 0 }
        };
        for group in &groups {
            let mut size = 0usize;
            for i in group.clone() {
                let st = &strikes[i];
                let n = 6 + if dr {
                    0
                } else {
                    st.cells.len()
                        * cell_size(u32::from(st.width), u32::from(st.height), &o.limits)?
                };
                size = size
                    .checked_add(n)
                    .ok_or_else(|| invalid("CPI page size overflow"))?;
            }
            if size > 65535 {
                return Err(invalid(
                    "CPI codepage font data exceeds its 16-bit size field",
                ));
            }
            check(size, o.limits.max_table_bytes, "CPI page bytes")?;
            lengths.push(size);
            final_size = final_size
                .checked_add(34 + size + if dr { 512 } else { 0 })
                .ok_or_else(|| invalid("CPI output size overflow"))?;
        }
        let tables_start = final_size;
        let mut dr_offsets = Vec::new();
        if dr {
            for st in &strikes[..dr_strikes] {
                dr_offsets.push(final_size);
                final_size = final_size
                    .checked_add(
                        groups.len()
                            * 256
                            * cell_size(u32::from(st.width), u32::from(st.height), &o.limits)?,
                    )
                    .ok_or_else(|| invalid("DRFONT table size overflow"))?;
            }
        }
        let tail = match s.properties.get("cpi.trailing") {
            Some(PropertyValue::Bytes(b)) => b.as_slice(),
            None => &[],
            _ => return Err(invalid("cpi.trailing must be bytes")),
        };
        final_size = final_size
            .checked_add(tail.len())
            .ok_or_else(|| invalid("CPI tail size overflow"))?;
        if o.version == Version::Cpx && final_size > 58880 {
            return Err(invalid(
                "CPX decoded FONT exceeds FreeDOS MODE's 58880-byte limit",
            ));
        }
        check(
            final_size,
            (if o.version == Version::Cpx {
                usize::MAX
            } else {
                o.max_output_bytes
            })
            .min(o.limits.max_input_bytes)
            .min(o.limits.max_total_bytes),
            "CPI output bytes",
        )?;
        let total_cells = strikes.iter().map(|s| s.cells.len()).sum::<usize>();
        let bitmap_bytes = strikes
            .iter()
            .map(|s| s.cells.len() * usize::from(s.height) * usize::from(s.width).div_ceil(8))
            .sum::<usize>();
        let scratch = strikes
            .iter()
            .map(|s| usize::from(s.height) * usize::from(s.width).div_ceil(8))
            .max()
            .unwrap_or(0)
            .saturating_mul(3)
            .saturating_add(
                s.fonts
                    .iter()
                    .flat_map(|f| f.glyphs())
                    .map(|(_, g)| g.bitmap.as_bytes().len())
                    .max()
                    .unwrap_or(0)
                    .saturating_mul(2),
            );
        report.reserve(
            final_size
                .saturating_mul(2)
                .saturating_add(scratch)
                .saturating_add(1024),
        )?;
        let mut out = Output::new(final_size);
        if o.version != Version::Cp {
            out.add(match o.version {
                Version::Font | Version::Cpx => b"\xffFONT   ",
                Version::FontNt => b"\xffFONT.NT",
                Version::DrFont => b"\x7fDRFONT ",
                _ => return Err(invalid("unsupported CPI container")),
            })?;
            let reserved = match s.properties.get("cpi.file_reserved") {
                Some(PropertyValue::Bytes(b)) if b.len() == 8 => b.as_slice(),
                None => &[0; 8],
                _ => return Err(invalid("cpi.file_reserved must be eight bytes")),
            };
            out.add(reserved)?;
            u16_out(&mut out, 1)?;
            out.add(&[1])?;
            u32_out(&mut out, 23 + if dr { 1 + 5 * dr_strikes } else { 0 })?;
            if dr {
                out.add(&[dr_strikes as u8])?;
                for st in &strikes[..dr_strikes] {
                    out.add(&[
                        cell_size(u32::from(st.width), u32::from(st.height), &o.limits)? as u8,
                    ])?;
                }
                for &offset in &dr_offsets {
                    u32_out(&mut out, offset)?;
                }
            }
            u16_out(&mut out, groups.len())?;
        }
        for (gi, group) in groups.iter().enumerate() {
            let st = &strikes[group.start];
            let here = out.bytes.len();
            let block = 34 + lengths[gi] + if dr { 512 } else { 0 };
            u16_out(&mut out, 28)?;
            u32_out(
                &mut out,
                if gi + 1 == groups.len() {
                    0
                } else if o.version == Version::FontNt {
                    block
                } else {
                    here + block
                },
            )?;
            u16_out(&mut out, 1)?;
            out.add(&st.device)?;
            u16_out(&mut out, usize::from(st.page))?;
            out.add(&st.reserved)?;
            u32_out(
                &mut out,
                if o.version == Version::FontNt {
                    28
                } else {
                    here + 28
                },
            )?;
            u16_out(&mut out, if dr { 2 } else { 1 })?;
            u16_out(&mut out, group.len())?;
            u16_out(&mut out, lengths[gi])?;
            for i in group.clone() {
                screen(&mut out, &strikes[i])?;
                if !dr {
                    bitmaps(&mut out, &strikes[i], o)?;
                }
            }
            if dr {
                for index in 0..256 {
                    u16_out(&mut out, gi * 256 + index)?;
                }
            }
        }
        if dr {
            if out.bytes.len() != tables_start {
                return Err(invalid("DRFONT internal offset mismatch"));
            }
            for size in 0..dr_strikes {
                for group in &groups {
                    bitmaps(&mut out, &strikes[group.start + size], o)?;
                }
            }
        }
        out.add(tail)?;
        let bytes = if o.version == Version::Cpx {
            super::pack::pack(out.bytes, o, &mut report)?
        } else {
            out.bytes
        };
        let before_readback = report.used();
        report.reserve(
            total_cells
                .saturating_mul(2048)
                .saturating_add(bitmap_bytes.saturating_mul(2))
                .saturating_add(
                    s.fonts.len().saturating_mul(
                        std::mem::size_of::<Font>()
                            .saturating_mul(2)
                            .saturating_add(2048),
                    ),
                )
                .saturating_add(tail.len().saturating_mul(2))
                .saturating_add(1024),
        )?;
        let (decoded, read_report) = reader::parse(
            &bytes,
            &CpiReadOptions::default(),
            &o.limits,
            before_readback,
            0,
        )?;
        if read_report.has_loss() {
            return Err(invalid("writer generated unreadable CPI gaps"));
        }
        compare(s, &decoded, "cpi", &mut report);
        // The source font reference keeps planning borrowed rather than copying models.
        let _ = strikes.first().map(|s| s.font.len());
        Ok(bytes)
    })();
    match result {
        Ok(bytes) => Ok((bytes, report.finish()?)),
        Err(e) => {
            if matches!(e, crate::Error::ResourceLimit { .. }) {
                return Err(e);
            }
            failed_projection(s, "cpi", &mut report);
            fail(report, "cpi", e)
        }
    }
}
