use super::*;
use crate::formats::x11;
use crate::model::Codepoint;
use std::collections::BTreeMap;

#[derive(Clone, Copy)]
struct Table<'a> {
    bytes: &'a [u8],
    fmt: u32,
}
struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
    msb: bool,
}
impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self
            .pos
            .checked_add(n)
            .ok_or_else(|| invalid("PCF offset overflow"))?;
        let result = self
            .bytes
            .get(self.pos..end)
            .ok_or_else(|| invalid("PCF data truncated while reading table"))?;
        self.pos = end;
        Ok(result)
    }
    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16> {
        let b = self.take(2)?;
        let a = [b[0], b[1]];
        Ok(if self.msb {
            u16::from_be_bytes(a)
        } else {
            u16::from_le_bytes(a)
        })
    }
    fn u32(&mut self) -> Result<u32> {
        let b = self.take(4)?;
        let a = [b[0], b[1], b[2], b[3]];
        Ok(if self.msb {
            u32::from_be_bytes(a)
        } else {
            u32::from_le_bytes(a)
        })
    }
    fn count(&mut self) -> Result<usize> {
        Ok(self.u32()? as usize)
    }
    fn metric(&mut self, compressed: bool) -> Result<Metric> {
        let mut m = [0; 6];
        if compressed {
            for v in &mut m[..5] {
                *v = i32::from(self.u8()?) - 128;
            }
        } else {
            for v in &mut m[..5] {
                *v = i32::from(self.u16()? as i16);
            }
            m[5] = i32::from(self.u16()?);
        }
        Ok(m)
    }
    fn records(&self, count: usize, size: usize) -> Result<()> {
        let n = count
            .checked_mul(size)
            .ok_or_else(|| invalid("PCF count overflow"))?;
        if n > self.bytes.len().saturating_sub(self.pos) {
            return Err(invalid("PCF data truncated while reading records"));
        }
        Ok(())
    }
}
impl<'a> Table<'a> {
    fn cursor(self, allowed: &[u32]) -> Result<Cursor<'a>> {
        if !allowed.contains(&(self.fmt & 0xffffff00)) {
            return Err(invalid(format!(
                "unsupported PCF table format {:#x}",
                self.fmt
            )));
        }
        Ok(Cursor {
            bytes: self.bytes,
            pos: 4,
            msb: self.fmt & 4 != 0,
        })
    }
}
struct Budget {
    used: usize,
    maximum: usize,
}
impl Budget {
    fn add(&mut self, n: usize) -> Result<()> {
        self.used = self
            .used
            .checked_add(n)
            .ok_or_else(|| limit("PCF model bytes", self.maximum))?;
        bound(self.used, self.maximum, "PCF model bytes")
    }
    fn string(&mut self, bytes: &[u8], offset: usize) -> Result<String> {
        let rest = bytes
            .get(offset..)
            .filter(|r| !r.is_empty())
            .ok_or_else(|| invalid("PCF string offset out of range"))?;
        let raw = &rest[..rest.iter().position(|&b| b == 0).unwrap_or(rest.len())];
        // UTF-8/Latin-1 plus projected metadata and the source snapshot.
        self.add(raw.len().saturating_mul(8).saturating_add(64))?;
        Ok(match std::str::from_utf8(raw) {
            Ok(s) => s.to_owned(),
            Err(_) => raw.iter().map(|&b| char::from(b)).collect(),
        })
    }
}
fn metrics(table: Table<'_>, limits: &ResourceLimits) -> Result<Vec<Metric>> {
    let mut c = table.cursor(&[0, 0x100])?;
    let compressed = table.fmt & 0xffffff00 == 0x100;
    let n = if compressed {
        usize::from(c.u16()?)
    } else {
        c.count()?
    };
    bound(n, limits.max_glyphs, "PCF glyph count")?;
    c.records(n, if compressed { 5 } else { 12 })?;
    bound(
        n.saturating_mul(std::mem::size_of::<Glyph>() + 128),
        limits.max_total_bytes,
        "PCF model bytes",
    )?;
    (0..n).map(|_| c.metric(compressed)).collect()
}
struct Accel {
    ascent: i64,
    descent: i64,
    min: Metric,
    max: Metric,
    raw: Vec<u8>,
}
fn accelerator(table: Table<'_>) -> Result<Accel> {
    let mut c = table.cursor(&[0, 0x100])?;
    c.take(8)?;
    let ascent = i64::from(c.u32()? as i32);
    let descent = i64::from(c.u32()? as i32);
    c.u32()?;
    let min = c.metric(false)?;
    let max = c.metric(false)?;
    if table.fmt & 0xffffff00 == 0x100 {
        c.metric(false)?;
        c.metric(false)?;
    }
    Ok(Accel {
        ascent,
        descent,
        min,
        max,
        raw: accelerator_fields(&table.bytes[4..c.pos], c.msb),
    })
}
pub(super) fn read(data: &[u8], limits: &ResourceLimits) -> Result<(Font, Report)> {
    bound(
        data.len(),
        limits.max_input_bytes.min(limits.max_total_bytes),
        "PCF input bytes",
    )?;
    if !data.starts_with(MAGIC) {
        return Err(invalid("not a PCF file (bad magic)"));
    }
    let mut report = Report::default();
    let mut cursor = Cursor {
        bytes: data,
        pos: 4,
        msb: false,
    };
    let count = cursor.count()?;
    if count > 1024 {
        return Err(invalid(format!("implausible PCF table count {count}")));
    }
    cursor.records(count, 16)?;
    let toc_end = 8 + count * 16;
    let mut tables = BTreeMap::new();
    for _ in 0..count {
        let kind = cursor.u32()?;
        let fmt = cursor.u32()?;
        let size = cursor.count()?;
        let off = cursor.count()?;
        bound(size, limits.max_table_bytes, "PCF table bytes")?;
        if off < toc_end {
            return Err(invalid("PCF table overlaps its directory"));
        }
        let end = off
            .checked_add(size)
            .ok_or_else(|| invalid("PCF table range overflow"))?;
        let end = if matches!(kind, ACCELERATORS | BDF_ACCELERATORS) {
            end.min(data.len())
        } else {
            end
        };
        let bytes = data
            .get(off..end)
            .ok_or_else(|| invalid("PCF table range exceeds file"))?;
        let mut c = Cursor {
            bytes,
            pos: 0,
            msb: false,
        };
        let actual = c.u32()?;
        if actual != fmt {
            notice(
                &mut report,
                "format_mismatch",
                format!("Table {kind:#x} format differs from its TOC; table format wins"),
            );
        }
        if let std::collections::btree_map::Entry::Vacant(e) = tables.entry(kind) {
            e.insert(Table { bytes, fmt: actual });
        } else {
            notice(
                &mut report,
                "duplicate_table",
                format!("Duplicate table {kind:#x}; first entry is used"),
            );
            if let Some(entry) = report.entries.last_mut() {
                entry.impact = Impact::Loss;
            }
        }
    }
    let m = metrics(
        *tables
            .get(&METRICS)
            .ok_or_else(|| invalid("PCF file has no METRICS table"))?,
        limits,
    )?;
    let n = m.len();
    let mut budget = Budget {
        used: 0,
        maximum: limits.max_total_bytes,
    };
    budget.add(n.saturating_mul(std::mem::size_of::<Glyph>() + 128))?;
    for v in &m {
        if v[1] < v[0] || v[3] + v[4] < 0 {
            return Err(invalid(
                "PCF glyph metrics describe a negative sized bitmap",
            ));
        }
        bound(
            (v[1] - v[0]) as usize * (v[3] + v[4]) as usize,
            limits.max_pixels_per_glyph,
            "PCF glyph pixels",
        )?;
    }
    let ink = tables
        .get(&INK_METRICS)
        .map(|&t| metrics(t, limits))
        .transpose()?;
    if ink.as_ref().is_some_and(|v| v.len() != n) {
        return Err(invalid("PCF ink metrics count differs from metrics count"));
    }
    let bitmap_table = *tables
        .get(&BITMAPS)
        .ok_or_else(|| invalid("PCF file has no BITMAPS table"))?;
    let mut c = bitmap_table.cursor(&[0])?;
    let cnt = c.count()?;
    if cnt != n {
        return Err(invalid(format!(
            "PCF bitmap count {cnt} != metrics count {n}"
        )));
    }
    c.records(n, 4)?;
    let offsets: Vec<_> = (0..n).map(|_| c.count()).collect::<Result<_>>()?;
    let mut sizes = [0usize; 4];
    for v in &mut sizes {
        *v = c.count()?;
    }
    let pad_index = (bitmap_table.fmt & 3) as usize;
    let pad = 1 << pad_index;
    let scan = 1 << ((bitmap_table.fmt >> 4) & 3);
    if scan > 4 {
        return Err(invalid(format!(
            "invalid PCF scan unit in format {:#x}",
            bitmap_table.fmt
        )));
    }
    let block_size = sizes[pad_index];
    bound(block_size, limits.max_table_bytes, "PCF bitmap block bytes")?;
    let mut block = c.take(block_size)?.to_vec();
    let bit_msb = bitmap_table.fmt & 8 != 0;
    let byte_msb = bitmap_table.fmt & 4 != 0;
    if bit_msb != byte_msb && scan > 1 {
        swap_units(&mut block, scan);
    }
    if !bit_msb {
        for b in &mut block {
            *b = b.reverse_bits();
        }
    }
    let mut properties = Properties::default();
    if let Some(&t) = tables.get(&PROPERTIES) {
        let mut c = t.cursor(&[0])?;
        let np = c.count()?;
        c.records(np, 9)?;
        budget.add(np.saturating_mul(192))?;
        let mut entries = Vec::with_capacity(np);
        for _ in 0..np {
            entries.push((c.count()?, c.u8()?, c.u32()?));
        }
        c.take((4 - (np & 3)) & 3)?;
        let len = c.count()?;
        let strings = c.take(len)?;
        for (offset, is_string, value) in entries {
            let name = budget.string(strings, offset)?;
            let value = if is_string != 0 {
                PropertyValue::Text(budget.string(strings, value as usize)?)
            } else {
                PropertyValue::Integer(i64::from(value as i32))
            };
            properties.push(Property::new(name, value));
        }
    }
    let a = tables
        .get(&ACCELERATORS)
        .map(|&t| accelerator(t))
        .transpose()?;
    let b = tables
        .get(&BDF_ACCELERATORS)
        .map(|&t| accelerator(t))
        .transpose()?;
    let accel = a.as_ref().or(b.as_ref());
    let mut codes = vec![Vec::<u32>::new(); n];
    let mut default_ch = 0xffff;
    if let Some(&t) = tables.get(&ENCODINGS) {
        let mut c = t.cursor(&[0])?;
        let fc = c.u16()?;
        let lc = c.u16()?;
        let fr = c.u16()?;
        let lr = c.u16()?;
        default_ch = c.u16()?;
        if fc <= lc && fr <= lr {
            if lc > 255 || lr > 255 {
                return Err(invalid(
                    "PCF encoding rows and columns must be within 0..255",
                ));
            }
            let nc = usize::from(lc - fc + 1);
            let total = nc * usize::from(lr - fr + 1);
            c.records(total, 2)?;
            budget.add(total * 4)?;
            for k in 0..total {
                let gi = c.u16()?;
                if gi != 0xffff {
                    let slot = codes.get_mut(usize::from(gi)).ok_or_else(|| {
                        invalid(format!("PCF encoding refers to missing glyph {gi}"))
                    })?;
                    slot.push(
                        ((u32::from(fr) + (k / nc) as u32) << 8)
                            | (u32::from(fc) + (k % nc) as u32),
                    );
                }
            }
        } else if (fc, lc, fr, lr) != (256, 0, 256, 0) {
            notice(
                &mut report,
                "empty_encodings",
                "Reversed encoding range is treated as empty".into(),
            );
        }
    }
    let sw = tables
        .get(&SWIDTHS)
        .map(|&t| -> Result<Vec<i32>> {
            let mut c = t.cursor(&[0])?;
            if c.count()? != n {
                return Err(invalid("PCF swidth count differs from metrics count"));
            }
            c.records(n, 4)?;
            (0..n).map(|_| c.u32().map(|v| v as i32)).collect()
        })
        .transpose()?;
    let names = tables
        .get(&GLYPH_NAMES)
        .map(|&t| -> Result<Vec<String>> {
            let mut c = t.cursor(&[0])?;
            if c.count()? != n {
                return Err(invalid("PCF glyph name count differs from metrics count"));
            }
            c.records(n, 4)?;
            let offsets: Vec<_> = (0..n).map(|_| c.count()).collect::<Result<_>>()?;
            let size = c.count()?;
            let strings = c.take(size)?;
            offsets
                .into_iter()
                .map(|off| budget.string(strings, off))
                .collect()
        })
        .transpose()?;
    let (mn, mx) = if let Some(a) = accel {
        (a.min, a.max)
    } else {
        let (mut mn, mut mx) = initial_bounds();
        for m in &m {
            minmax(&mut mn, &mut mx, m);
        }
        (mn, mx)
    };
    let bounds = if mx[1] >= mn[0] && mx[3] + mx[4] >= 0 && mn[0] != 32767 {
        Bounds::new(
            (mx[1] - mn[0]) as u32,
            (mx[3] + mx[4]) as u32,
            i64::from(mn[0]),
            -i64::from(mx[4]),
        )
    } else {
        Bounds::default()
    };
    let ps = prop_int(&properties, "POINT_SIZE")
        .map(|v| ((v as f64 / 10.0) + 0.5).floor() as i64)
        .or_else(|| prop_int(&properties, "PIXEL_SIZE"))
        .unwrap_or(i64::from(bounds.height));
    let xr = prop_int(&properties, "RESOLUTION_X").unwrap_or(75);
    let yr = prop_int(&properties, "RESOLUTION_Y").unwrap_or(75);
    let name = properties
        .iter()
        .find_map(|p| match &p.value {
            PropertyValue::Text(s) if p.name == "FONT" => Some(s.clone()),
            _ => None,
        })
        .unwrap_or_default();
    if let Some(a) = accel {
        if properties.get_case_insensitive("FONT_ASCENT").is_none() {
            properties.push(Property::new("FONT_ASCENT", a.ascent));
        }
        if properties.get_case_insensitive("FONT_DESCENT").is_none() {
            properties.push(Property::new("FONT_DESCENT", a.descent));
        }
    }
    if properties.get_case_insensitive("DEFAULT_CHAR").is_none()
        && ![0, 0xffff].contains(&default_ch)
    {
        properties.push(Property::new("DEFAULT_CHAR", i64::from(default_ch)));
    }
    let mut font = Font::default();
    font.source_format = Some("pcf".into());
    font.properties = properties;
    font.headers.push(Property::new(
        "STARTFONT",
        PropertyValue::Decimal("2.1".into()),
    ));
    font.headers.push(Property::new("FONT", name));
    font.headers.push(Property::new("SIZE", list([ps, xr, yr])));
    font.headers.push(Property::new(
        "FONTBOUNDINGBOX",
        list([
            i64::from(bounds.width),
            i64::from(bounds.height),
            bounds.x,
            bounds.y,
        ]),
    ));
    font.headers.push(Property::new("METRICSSET", 0i64));
    x11::project(&mut font);
    x11::save_snapshot(&mut font);
    x11::derive(&mut font);
    // PCF SPEC names/codes use the declared Unicode identity class. Unknown
    // charsets remain legacy, even though a later explicit codec may fall back
    // to identity conversion (BDF's compatibility projection is different).
    let unicode = crate::charset::is_unicode_charset(&font.charset.canonical_name());
    if let Some(cp) = font.metrics.default_character {
        font.metrics.default_character = Some(if unicode {
            Codepoint::Unicode(cp.value())
        } else {
            Codepoint::Legacy(cp.value())
        });
    }
    for (i, metric) in m.iter().enumerate() {
        let w = (metric[1] - metric[0]) as u32;
        let h = (metric[3] + metric[4]) as u32;
        let row = row_bytes(w as usize, pad);
        let length = row
            .checked_mul(h as usize)
            .ok_or_else(|| invalid("PCF bitmap extent overflow"))?;
        let end = offsets[i]
            .checked_add(length)
            .ok_or_else(|| invalid("PCF bitmap offset overflow"))?;
        let bytes = block
            .get(offsets[i]..end)
            .ok_or_else(|| invalid(format!("PCF bitmap of glyph {i} exceeds the bitmap data")))?;
        let stride = (w as usize).div_ceil(8);
        budget.add(stride.saturating_mul(h as usize))?;
        let mut packed = Vec::with_capacity(stride * h as usize);
        if stride != 0 {
            for row in bytes.chunks_exact(row) {
                packed.extend_from_slice(&row[..stride]);
            }
        }
        let bitmap = Bitmap::<Mono>::from_raw(w, h, &packed, bitmap_limits(limits))?;
        let mut glyph = Glyph::new(bitmap);
        glyph.metrics.bounds = Bounds::new(w, h, i64::from(metric[0]), -i64::from(metric[4]));
        glyph.metrics.advance = Some(Vector::new(i64::from(metric[2]), 0));
        glyph.metrics.scalable_advance = Some(Vector::new(
            sw.as_ref().map_or_else(
                || swidth(ps as f64, xr, metric[2], bounds.width),
                |v| i64::from(v[i]),
            ),
            0,
        ));
        glyph.codepoints = codes[i]
            .iter()
            .map(|&cp| {
                if unicode {
                    Codepoint::Unicode(cp)
                } else {
                    Codepoint::Legacy(cp)
                }
            })
            .collect();
        glyph.name = Some(names.as_ref().map_or_else(
            || match codes[i].first() {
                None => format!("glyph{i}"),
                Some(cp) if !unicode => format!("char{cp}"),
                Some(cp) => format!("uni{cp:04X}"),
            },
            |n| n[i].clone(),
        ));
        if metric[5] != 0 {
            glyph
                .properties
                .push(Property::new("pcf.attributes", i64::from(metric[5])));
        }
        if let Some(ink) = &ink {
            glyph.properties.push(Property::new(
                "pcf.ink_metrics",
                list(ink[i].map(i64::from)),
            ));
        }
        font.add_glyph(glyph)?;
    }
    for (key, value) in [("pcf.accelerators", a), ("pcf.bdf_accelerators", b)] {
        if let Some(a) = value {
            font.headers
                .push(Property::new(key, PropertyValue::Bytes(a.raw)));
        }
    }
    font.headers
        .push(Property::new("pcf.byte_order", i64::from(byte_msb)));
    font.headers
        .push(Property::new("pcf.default_char", i64::from(default_ch)));
    for (&kind, t) in &tables {
        if !matches!(
            kind,
            PROPERTIES
                | ACCELERATORS
                | METRICS
                | BITMAPS
                | INK_METRICS
                | ENCODINGS
                | SWIDTHS
                | GLYPH_NAMES
                | BDF_ACCELERATORS
        ) {
            budget.add(t.bytes.len())?;
            font.headers.push(Property::new(
                format!("pcf.table.{kind}"),
                PropertyValue::Bytes(t.bytes.to_vec()),
            ));
            notice(
                &mut report,
                "unknown_table",
                format!("Preserved unknown table {kind:#x} as provenance"),
            );
        }
    }
    font.validate(limits)?;
    Ok((font, report))
}
