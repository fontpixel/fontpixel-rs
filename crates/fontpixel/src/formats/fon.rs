//! Bounded Windows FON resource containers: NE read/write and PE read.
use super::fnt::{self, FntReadOptions, FntWriteOptions, support::*};
use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
use crate::model::FontSet;
use crate::resources::{OutputBundle, ReadContext, Written};
use crate::{Report, ResourceLimits, Result};
use std::collections::BTreeSet;

/// Immutable FON codec; output always uses an NE resource-only container.
#[derive(Debug, Clone, Default)]
pub struct FonCodec {
    /// FNT legacy charset interpretation for every resource.
    pub read_options: FntReadOptions,
    /// FNT version, code page, and shared resource limits for every output font.
    pub write_options: FntWriteOptions,
}
impl FormatDetector for FonCodec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(detect_limited(
            c.bytes(),
            c.limits(),
            c.retained_input_bytes(),
        ))
    }
}
impl FormatReader<FontSet> for FonCodec {
    fn read(&self, c: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
        let result = parse(
            c.bytes(),
            &self.read_options,
            c.limits(),
            c.retained_input_bytes(),
        )?;
        Ok(result)
    }
}
impl FormatWriter<FontSet> for FonCodec {
    fn write(&self, set: &FontSet, o: &WriteOptions) -> Result<Written> {
        let (bytes, report) = write(set, &self.write_options)?;
        Ok(Written {
            output: OutputBundle::single(o.primary_name.clone(), bytes),
            report,
        })
    }
}
fn header_offset(b: &[u8]) -> Result<usize> {
    if b.starts_with(b"MZ") {
        let at = get32(b, 60)? as usize;
        if at < 64 {
            return Err(invalid("FON executable header overlaps MZ header"));
        }
        Ok(at)
    } else if b.starts_with(b"NE") || b.starts_with(b"PE\0\0") {
        Ok(0)
    } else {
        Err(invalid("missing FON executable signature"))
    }
}
/// Recognize bounded Windows FONT directories and FNT headers in NE/PE files.
pub fn detect(b: &[u8]) -> Confidence {
    detect_limited(b, &ResourceLimits::default(), b.len())
}
fn detect_limited(b: &[u8], l: &ResourceLimits, retained: usize) -> Confidence {
    let probe = || -> Result<Confidence> {
        let at = header_offset(b)?;
        let mut budget = Budget::new(retained, l)?;
        let mut report = Report::default();
        let ne = slice(b, at, 2)? == b"NE";
        let resources = if ne {
            ne_resources(b, at, l, &mut report, &mut budget)?
        } else if slice(b, at, 4)? == b"PE\0\0" {
            pe_resources(b, at, l, &mut report, &mut budget)?
        } else {
            return Ok(Confidence::NoMatch);
        };
        if resources.is_empty()
            || resources
                .iter()
                .any(|r| fnt::detect(&b[r.offset..r.offset + r.size]) != Confidence::Certain)
        {
            return Ok(Confidence::NoMatch);
        }
        // OS/2 has a different resource directory. A Windows FONT directory
        // with that target byte is compatible/contradictory evidence, not a
        // reason to override the independent GPI detector's Possible match.
        Ok(if ne && b[at + 54] == 1 {
            Confidence::Possible
        } else {
            Confidence::Certain
        })
    };
    probe().unwrap_or(Confidence::NoMatch)
}
fn fon_loss(r: &mut Report, code: &str, message: impl Into<String>) {
    loss(r, code, None, None, message);
    if let Some(e) = r.entries.last_mut() {
        e.code = format!("fon.{code}");
        e.location.font_index = None;
    }
}
#[derive(Clone, Copy)]
struct Resource {
    offset: usize,
    size: usize,
}
fn ne_name(b: &[u8], v: u16) -> Result<()> {
    if v & 0x8000 == 0 {
        let at = usize::from(v);
        let n = *b
            .get(at)
            .ok_or_else(|| invalid("NE name offset exceeds resource table"))?;
        slice(b, at + 1, usize::from(n))?;
    }
    Ok(())
}
fn ne_resources(
    b: &[u8],
    at: usize,
    l: &ResourceLimits,
    r: &mut Report,
    budget: &mut Budget,
) -> Result<Vec<Resource>> {
    let h = slice(b, at, 64)?;
    let start = usize::from(get16(h, 36)?);
    let end = usize::from(get16(h, 38)?);
    if start < 64 || end <= start {
        return Err(invalid("invalid NE resource table bounds"));
    }
    let table = slice(
        b,
        at.checked_add(start)
            .ok_or_else(|| invalid("NE table offset overflow"))?,
        end - start,
    )?;
    check(table.len(), l.max_table_bytes, "NE resource table bytes")?;
    let shift = get16(table, 0)?;
    if shift > 16 {
        return Err(invalid("NE alignment shift exceeds 16"));
    }
    if get16(h, 28)? != 0 {
        budget.add(512)?;
        fon_loss(
            r,
            "executable_sections",
            "NE executable segments are not retained",
        );
    }
    let mut pos = 2;
    let mut resources = Vec::new();
    let mut total = 0usize;
    loop {
        let kind = get16(table, pos)?;
        if kind == 0 {
            break;
        }
        ne_name(table, kind)?;
        let count = usize::from(get16(table, pos + 2)?);
        slice(table, pos, 8)?;
        total = total
            .checked_add(count)
            .ok_or_else(|| invalid("NE resource count overflow"))?;
        check(total, l.max_resources, "FON resource count")?;
        let entries = slice(table, pos + 8, count * 12)?;
        budget.add(count.saturating_mul(512))?;
        for i in 0..count {
            let e = &entries[i * 12..i * 12 + 12];
            ne_name(table, get16(e, 6)?)?;
            let offset = usize::from(get16(e, 0)?)
                .checked_shl(u32::from(shift))
                .ok_or_else(|| invalid("NE resource offset overflow"))?;
            let size = usize::from(get16(e, 2)?)
                .checked_shl(u32::from(shift))
                .ok_or_else(|| invalid("NE resource length overflow"))?;
            check(size, l.max_resource_bytes, "NE resource bytes")?;
            slice(b, offset, size)?;
            if kind == 0x8008 {
                resources.push(Resource { offset, size });
                if usize::from(get16(e, 6)?) != 0x8000 + resources.len() {
                    fon_loss(
                        r,
                        "resource_identity",
                        "NE resource name/ordinal will be regenerated in output order",
                    );
                }
            } else if kind != 0x8007 {
                fon_loss(
                    r,
                    "ignored_resource",
                    format!("NE resource type {kind:#06x} is not a FONT/FONTDIR resource"),
                );
            }
        }
        pos += 8 + count * 12;
    }
    Ok(resources)
}
#[derive(Clone, Copy)]
struct Section {
    rva: u32,
    virtual_size: u32,
    raw: usize,
    size: usize,
}
fn rva_map(b: &[u8], sections: &[Section], rva: u32, n: usize) -> Result<usize> {
    let mut found = None;
    for s in sections {
        if let Some(delta) = rva.checked_sub(s.rva) {
            let delta = delta as usize;
            let extent = s.size.max(s.virtual_size as usize);
            if delta < extent || (delta == extent && n == 0) {
                if delta.checked_add(n).is_none_or(|v| v > s.size) {
                    return Err(invalid("PE RVA points into unbacked/overrun section bytes"));
                }
                let at = s
                    .raw
                    .checked_add(delta)
                    .ok_or_else(|| invalid("PE raw offset overflow"))?;
                slice(b, at, n)?;
                if found.replace(at).is_some() {
                    return Err(invalid("ambiguous overlapping PE RVA sections"));
                }
            }
        }
    }
    found.ok_or_else(|| invalid("PE resource RVA has no backing section"))
}
struct PeTree<'a, 'b> {
    budget: &'b mut Budget,
    root: &'a [u8],
    limits: &'a ResourceLimits,
    visited: BTreeSet<usize>,
    entries: usize,
}
impl PeTree<'_, '_> {
    fn directory(&mut self, at: usize) -> Result<Vec<(u32, u32)>> {
        self.budget.add(64)?;
        if !self.visited.insert(at) {
            return Err(invalid("PE resource directory cycle or shared directory"));
        }
        let names = usize::from(get16(self.root, at + 12)?);
        let ids = usize::from(get16(self.root, at + 14)?);
        let count = names + ids;
        self.entries = self
            .entries
            .checked_add(count)
            .ok_or_else(|| invalid("PE resource count overflow"))?;
        check(
            self.entries,
            self.limits
                .max_resources
                .saturating_mul(3)
                .saturating_add(16),
            "PE directory entries",
        )?;
        let table = slice(self.root, at + 16, count * 8)?;
        self.budget.add(count.saturating_mul(512))?;
        let mut result = Vec::with_capacity(count);
        for i in 0..count {
            let name = get32(table, 8 * i)?;
            let next = get32(table, 8 * i + 4)?;
            if (i < names) != (name & 0x80000000 != 0) {
                return Err(invalid("PE named/ordinal entry counts disagree"));
            }
            if name & 0x80000000 != 0 {
                let off = (name & 0x7fffffff) as usize;
                let n = usize::from(get16(self.root, off)?) * 2;
                slice(self.root, off + 2, n)?;
                check(n, self.limits.max_table_bytes, "PE resource name bytes")?;
            }
            result.push((name, next));
        }
        Ok(result)
    }
}
fn pe_resources(
    b: &[u8],
    at: usize,
    l: &ResourceLimits,
    r: &mut Report,
    budget: &mut Budget,
) -> Result<Vec<Resource>> {
    let coff = slice(b, at, 24)?;
    let n = usize::from(get16(coff, 6)?);
    check(n, l.max_resources, "PE section count")?;
    let opt_len = usize::from(get16(coff, 20)?);
    let opt = slice(b, at + 24, opt_len)?;
    let (count_at, dirs) = match get16(opt, 0)? {
        0x10b => (92, 96),
        0x20b => (108, 112),
        _ => return Err(invalid("unsupported PE optional-header magic")),
    };
    if get32(opt, count_at)? < 3 {
        return Err(invalid("PE has no resource data directory"));
    }
    let root_rva = get32(opt, dirs + 16)?;
    let root_size = get32(opt, dirs + 20)? as usize;
    check(root_size, l.max_table_bytes, "PE resource directory bytes")?;
    let section_table = slice(b, at + 24 + opt_len, n * 40)?;
    budget.add(n.saturating_mul(512))?;
    let mut sections = Vec::with_capacity(n);
    for i in 0..n {
        let s = &section_table[i * 40..i * 40 + 40];
        let raw = get32(s, 20)? as usize;
        let size = get32(s, 16)? as usize;
        slice(b, raw, size)?;
        sections.push(Section {
            rva: get32(s, 12)?,
            virtual_size: get32(s, 8)?,
            raw,
            size,
        });
        if get32(s, 36)? & 0x20000020 != 0 {
            fon_loss(
                r,
                "executable_sections",
                "PE executable section content is not retained",
            );
        }
    }
    let root_at = rva_map(b, &sections, root_rva, root_size)?;
    let root = slice(b, root_at, root_size)?;
    let mut tree = PeTree {
        budget,
        root,
        limits: l,
        visited: BTreeSet::new(),
        entries: 0,
    };
    let mut resources = Vec::new();
    for (kind, next) in tree.directory(0)? {
        if kind != 8 {
            if kind != 7 {
                fon_loss(
                    r,
                    "ignored_resource",
                    format!("PE resource type {kind:#x} is not FONT/FONTDIR"),
                );
            }
            continue;
        }
        if next & 0x80000000 == 0 {
            return Err(invalid("PE FONT type must point to a name directory"));
        }
        for (name, next) in tree.directory((next & 0x7fffffff) as usize)? {
            if next & 0x80000000 == 0 {
                return Err(invalid("PE FONT name must point to a language directory"));
            }
            for (lang, next) in tree.directory((next & 0x7fffffff) as usize)? {
                if next & 0x80000000 != 0 {
                    return Err(invalid("PE language entry must point to resource data"));
                }
                let e = slice(root, next as usize, 16)?;
                let size = get32(e, 4)? as usize;
                check(size, l.max_resource_bytes, "PE font resource bytes")?;
                let offset = rva_map(b, &sections, get32(e, 0)?, size)?;
                check(resources.len() + 1, l.max_resources, "FON font count")?;
                resources.push(Resource { offset, size });
                if name != resources.len() as u32 || lang != 0 || get32(e, 8)? != 0 {
                    fon_loss(
                        r,
                        "resource_identity",
                        "PE name, language and resource code-page metadata are not retained in NE output",
                    );
                }
            }
        }
    }
    Ok(resources)
}
/// Extract all raster FONT resources from an NE or PE container.
pub fn read(b: &[u8], o: &FntReadOptions, l: &ResourceLimits) -> Result<(FontSet, Report)> {
    check(b.len(), l.max_input_bytes, "FON input bytes")?;
    parse(b, o, l, b.len())
}
fn parse(
    b: &[u8],
    o: &FntReadOptions,
    l: &ResourceLimits,
    retained: usize,
) -> Result<(FontSet, Report)> {
    let at = header_offset(b)?;
    let mut report = Report::default();
    let sig = slice(b, at, 2)?;
    let mut budget = Budget::new(retained, l)?;
    let resources = if sig == b"NE" {
        ne_resources(b, at, l, &mut report, &mut budget)?
    } else if slice(b, at, 4)? == b"PE\0\0" {
        pe_resources(b, at, l, &mut report, &mut budget)?
    } else {
        return Err(invalid("unsupported FON executable signature"));
    };
    if resources.is_empty() {
        return Err(invalid("FON contains no FONT resources"));
    }
    budget.add(
        resources.len() * (std::mem::size_of::<Resource>() + std::mem::size_of::<crate::Font>()),
    )?;
    let mut set = FontSet {
        source_format: Some("fon".into()),
        ..Default::default()
    };
    for (index, resource) in resources.iter().enumerate() {
        let (font, mut r) =
            fnt::reader::parse(slice(b, resource.offset, resource.size)?, o, l, &mut budget)?;
        for e in &mut r.entries {
            e.location.font_index = Some(index);
        }
        report.extend(r);
        set.fonts.push(font);
    }
    Ok((set, report))
}
fn align(n: usize, shift: u32) -> Result<usize> {
    let mask = (1usize << shift) - 1;
    n.checked_add(mask)
        .map(|v| v & !mask)
        .ok_or_else(|| invalid("NE alignment overflow"))
}
/// Write a deterministic resource-only NE .FON containing every font in order.
pub fn write(set: &FontSet, o: &FntWriteOptions) -> Result<(Vec<u8>, Report)> {
    if set.fonts.is_empty() {
        return Err(invalid("FON needs at least one font"));
    }
    check(
        set.fonts.len() + 1,
        o.limits.max_resources,
        "FON resource count",
    )?;
    check(set.fonts.len(), 32767, "NE font ordinal range")?;
    set.validate(&o.limits)?;
    let n = set.fonts.len();
    let table_len = 2 + 8 + 12 + 8 + n * 12 + 2 + 8 + 1;
    check(
        table_len,
        o.limits.max_table_bytes,
        "NE resource table bytes",
    )?;
    check(table_len - 9, 0x7fff, "NE FONTDIR name offset")?;
    // Retain all source models and their diagnostic reservations while encoding
    // each font. The current font's reservation is charged inside fnt::write;
    // subtract only that reservation when computing its remaining allowance.
    let mut budget = Budget::new(properties_cost(&set.properties, &o.limits)?, &o.limits)?;
    budget.add(std::mem::size_of::<FontSet>() + n * (std::mem::size_of::<Vec<u8>>() + 32) + 2)?;
    let mut source_costs = Vec::with_capacity(n);
    for font in &set.fonts {
        let cost = model_cost(font, &o.limits)?;
        budget.add(cost)?;
        source_costs.push(cost);
    }
    let mut report = Report::default();
    let mut fonts = Vec::new();
    let mut fontdir = Vec::new();
    fontdir.extend_from_slice(&(set.fonts.len() as u16).to_le_bytes());
    let mut work = budget.used;
    for (index, font) in set.fonts.iter().enumerate() {
        let mut opts = o.clone();
        opts.limits.max_total_bytes = opts
            .limits
            .max_total_bytes
            .checked_sub(work - source_costs[index])
            .ok_or_else(|| invalid("FON aggregate memory exhausted"))?;
        opts.max_output_bytes = opts.max_output_bytes.min(o.limits.max_resource_bytes);
        let (b, mut r) = match fnt::write(font, &opts) {
            Ok(result) => result,
            Err(crate::Error::ReportedErrors(mut failed)) => {
                for e in &mut failed.entries {
                    e.location.font_index = Some(index);
                }
                report.extend(failed);
                return Err(crate::Error::ReportedErrors(report));
            }
            Err(error) => return Err(error),
        };
        for e in &mut r.entries {
            e.location.font_index = Some(index);
        }
        report.extend(r);
        let dev = cstr(&b, get32(&b, 101)? as usize, &o.limits)?;
        let face = cstr(&b, get32(&b, 105)? as usize, &o.limits)?;
        let dir_next = fontdir
            .len()
            .checked_add(117 + dev.len() + face.len())
            .ok_or_else(|| invalid("FONTDIR size overflow"))?;
        check(dir_next, o.limits.max_resource_bytes, "FONTDIR bytes")?;
        check(
            work.saturating_add(b.len()).saturating_add(dir_next),
            o.limits.max_total_bytes,
            "FON working bytes",
        )?;
        fontdir.extend_from_slice(&((index + 1) as u16).to_le_bytes());
        fontdir.extend_from_slice(&b[..113]);
        fontdir.extend_from_slice(dev);
        fontdir.push(0);
        fontdir.extend_from_slice(face);
        fontdir.push(0);
        check(fontdir.len(), o.limits.max_resource_bytes, "FONTDIR bytes")?;
        work = work
            .checked_add(b.len() + 115 + dev.len() + face.len() + 2)
            .ok_or_else(|| invalid("FON work overflow"))?;
        check(work, o.limits.max_total_bytes, "FON working bytes")?;
        fonts.push(b);
    }
    for p in set.properties.iter() {
        fon_loss(
            &mut report,
            "container_property",
            format!("Container property {:?} has no generated NE field", p.name),
        );
    }
    let resident = b"\x07FONTLIB\0\0\0";
    let nonresident = b"\x11FONTRES fontpixel\0\0\0";
    let ne = 128usize;
    let resident_at = 64 + table_len;
    let entry_at = resident_at + resident.len();
    let non_at = entry_at + 2;
    let data_at = ne + non_at + nonresident.len();
    word(resident_at as i64, "NE resident table offset")?;
    word(entry_at as i64, "NE entry table offset")?;
    let mut shift = 4u32;
    let (starts, total) = loop {
        let mut pos = align(data_at, shift)?;
        let mut starts = vec![pos];
        pos = align(pos + fontdir.len(), shift)?;
        starts.push(pos);
        for b in &fonts {
            pos = align(pos + b.len(), shift)?;
            starts.push(pos);
        }
        if pos >> shift <= 65535 {
            break (starts, pos);
        }
        shift += 1;
        if shift > 16 {
            return Err(invalid("FON exceeds NE aligned offset range"));
        }
    };
    for resource in starts.windows(2) {
        check(
            resource[1] - resource[0],
            o.limits.max_resource_bytes,
            "NE aligned resource bytes",
        )?;
    }
    check(total, o.max_output_bytes, "FON output bytes")?;
    check(
        work.saturating_add(total),
        o.limits.max_total_bytes,
        "FON aggregate output bytes",
    )?;
    let mut out = vec![0; total];
    out[..2].copy_from_slice(b"MZ");
    put16(&mut out, 2, (ne % 512) as u16);
    put16(&mut out, 4, 1);
    put16(&mut out, 8, 4);
    put16(&mut out, 12, 0xffff);
    put16(&mut out, 24, 64);
    put32(&mut out, 60, ne as u32);
    // Minimal DOS stub: mov ax,4c01h; int 21h. Resource-only NE is never executed.
    out[64..69].copy_from_slice(&[0xb8, 0x01, 0x4c, 0xcd, 0x21]);
    let h = &mut out[ne..ne + 64];
    h[..2].copy_from_slice(b"NE");
    h[2] = 5;
    h[3] = 10;
    put16(h, 4, entry_at as u16);
    put16(h, 6, 2);
    put16(h, 12, 0x8308);
    put16(h, 32, nonresident.len() as u16);
    put16(h, 34, 64);
    put16(h, 36, 64);
    put16(h, 38, resident_at as u16);
    put16(h, 40, entry_at as u16);
    put16(h, 42, entry_at as u16);
    put32(h, 44, (ne + non_at) as u32);
    put16(h, 50, shift as u16);
    h[54] = 2;
    put16(h, 62, 0x300);
    let t = &mut out[ne + 64..ne + 64 + table_len];
    put16(t, 0, shift as u16);
    put16(t, 2, 0x8007);
    put16(t, 4, 1);
    put16(t, 10, (starts[0] >> shift) as u16);
    put16(t, 12, ((starts[1] - starts[0]) >> shift) as u16);
    put16(t, 14, 0x0c50);
    put16(t, 16, (table_len - 9) as u16);
    put16(t, 22, 0x8008);
    put16(t, 24, n as u16);
    for i in 0..n {
        let at = 30 + i * 12;
        put16(t, at, (starts[i + 1] >> shift) as u16);
        put16(t, at + 2, ((starts[i + 2] - starts[i + 1]) >> shift) as u16);
        put16(t, at + 4, 0x1c30);
        put16(t, at + 6, 0x8001 + i as u16);
    }
    let name_at = table_len - 9;
    t[name_at..name_at + 8].copy_from_slice(b"\x07FONTDIR");
    out[ne + resident_at..ne + resident_at + resident.len()].copy_from_slice(resident);
    out[ne + non_at..ne + non_at + nonresident.len()].copy_from_slice(nonresident);
    out[starts[0]..starts[0] + fontdir.len()].copy_from_slice(&fontdir);
    for (i, b) in fonts.iter().enumerate() {
        out[starts[i + 1]..starts[i + 1] + b.len()].copy_from_slice(b);
    }
    Ok((out, report))
}
