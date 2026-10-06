//! U8/M page-addressed glyphs and bounded radix-64 mapping trees.
use super::device_common::*;
use super::{Confidence, ModelCharge, WriteOptions};
use crate::model::{Codepoint, Font, FontSet, Glyph, Property, PropertyValue};
use crate::resources::{OutputBundle, ReadContext, Written};
use crate::{Report, ResourceLimits, Result};
use std::collections::BTreeMap;
/// Native header detection also accepts an optional two-byte load address.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct U8mReadOptions {}
/// Bounded U8/M output choices.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct U8mWriteOptions {
    /// Optional load-address prefix; otherwise retain a source prefix if present.
    pub load_address: Option<u16>,
    /// Explicit grayscale-to-monochrome conversion threshold.
    pub gray_threshold: Option<u8>,
    /// Cumulative resource limits, including map expansion and readback.
    pub limits: ResourceLimits,
    /// Additional output cap.
    pub max_output_bytes: usize,
}
impl Default for U8mWriteOptions {
    fn default() -> Self {
        Self {
            load_address: None,
            gray_threshold: None,
            limits: ResourceLimits::default(),
            max_output_bytes: 64 * 1024 * 1024,
        }
    }
}
codec!(U8mCodec, U8mReadOptions, U8mWriteOptions);
fn base(b: &[u8]) -> Option<usize> {
    if b.starts_with(b"U8/M") {
        Some(0)
    } else if b.get(2..6) == Some(b"U8/M") {
        Some(2)
    } else {
        None
    }
}
/// Detect the native header and optional load prefix.
pub fn detect(b: &[u8]) -> Confidence {
    if base(b).is_some() {
        Confidence::Certain
    } else {
        Confidence::NoMatch
    }
}
/// Decode real map entries without scanning unused Unicode code positions.
pub fn read(c: &mut ReadContext<'_>, _: &U8mReadOptions) -> Result<(FontSet, Report)> {
    let mut budget = Budget::new(c.retained_input_bytes(), c.limits())?;
    parse(c.bytes(), c.limits(), &mut budget)
}
struct Maps<'a> {
    bytes: &'a [u8],
    table: usize,
    count: usize,
    glyphs: usize,
}
impl Maps<'_> {
    fn entries(&self, index: usize) -> Result<&[u8]> {
        if index >= self.count {
            return Err(invalid("U8/M map index is out of range"));
        }
        let v = r32(self.bytes, self.table + index * 4)?;
        let p = (v & 0xffffff) as usize;
        let n = (v >> 24) as usize;
        if n > 64 || p % 256 + n * 4 > 256 || (n != 0 && p < 256) {
            return Err(invalid("U8/M map crosses page or header"));
        }
        let data = slice(self.bytes, p, n * 4)?;
        let mut end = None;
        for e in data.as_chunks::<4>().0 {
            if e[0] > e[1] || e[1] >= 64 || end.is_some_and(|last| last >= e[0]) {
                return Err(invalid("U8/M map ranges overlap or exceed six bits"));
            }
            end = Some(e[1]);
        }
        Ok(data)
    }
    fn walk(
        &self,
        index: usize,
        depth: usize,
        prefix: u32,
        native: bool,
        out: &mut BTreeMap<Codepoint, usize>,
        budget: &mut Budget,
    ) -> Result<()> {
        if index == 0 {
            return Ok(());
        }
        // Depth decreases on every call; even cyclic indices cannot cause recursion growth.
        budget.add(64)?;
        for e in self.entries(index)?.as_chunks::<4>().0 {
            let first = usize::from(u16::from_le_bytes([e[2], e[3]]));
            for k in e[0]..=e[1] {
                let value = first + usize::from(k - e[0]);
                let cp = prefix | (u32::from(k) << ((depth - 1) * 6));
                if depth > 1 {
                    self.walk(value, depth - 1, cp, native, out, budget)?;
                } else if value != 0 {
                    if value >= self.glyphs {
                        return Err(invalid("U8/M glyph index is out of range"));
                    }
                    let cp = if native {
                        Codepoint::Legacy(cp)
                    } else {
                        if char::from_u32(cp).is_none() {
                            return Err(invalid("U8/M mapping is not a Unicode scalar"));
                        }
                        Codepoint::Unicode(cp)
                    };
                    budget.add(128)?;
                    out.insert(cp, value);
                }
            }
        }
        Ok(())
    }
}
fn parse(input: &[u8], l: &ResourceLimits, budget: &mut Budget) -> Result<(FontSet, Report)> {
    let base = base(input).ok_or_else(|| invalid("invalid U8/M magic"))?;
    let b = &input[base..];
    slice(b, 0, 256)?;
    check(input.len(), l.max_table_bytes, "U8/M table bytes")?;
    let n = usize::from(r16(b, 130)?);
    let gt = usize::from(r16(b, 128)?) * 256;
    let mc = usize::from(r16(b, 134)?);
    let mt = usize::from(r16(b, 132)?) * 256;
    if n == 0 || mc == 0 || gt < 256 || mt < 256 || b[4] > 118 || b[123] != 0 {
        return Err(invalid("invalid U8/M master table"));
    }
    slice(b, gt, n * 4)?;
    slice(b, mt, mc * 4)?;
    allocation(n, l, budget)?;
    budget.items::<Vec<Codepoint>>(n)?;
    let maps = Maps {
        bytes: b,
        table: mt,
        count: mc,
        glyphs: n,
    };
    if !maps.entries(0)?.is_empty() {
        return Err(invalid("U8/M map zero is not empty"));
    }
    // Validate even unreachable map records, while traversing only reachable mappings.
    for i in 1..mc {
        maps.entries(i)?;
    }
    let mut mappings = BTreeMap::new();
    for i in 0..6 {
        maps.walk(
            usize::from(r16(b, 240 + i * 2)?),
            3,
            (i as u32) << 18,
            false,
            &mut mappings,
            budget,
        )?;
    }
    for i in 0..16 {
        maps.walk(
            usize::from(r16(b, 208 + i * 2)?),
            2,
            (i as u32) << 12,
            false,
            &mut mappings,
            budget,
        )?;
    }
    for i in 0..32 {
        maps.walk(
            usize::from(r16(b, 144 + i * 2)?),
            1,
            (i as u32) << 6,
            false,
            &mut mappings,
            budget,
        )?;
    }
    for i in 0..4 {
        maps.walk(
            usize::from(r16(b, 136 + i * 2)?),
            1,
            (i as u32) << 6,
            true,
            &mut mappings,
            budget,
        )?;
    }
    let mut aliases = vec![Vec::new(); n];
    for (cp, index) in mappings {
        aliases[index].push(cp);
    }
    let mut f = Font::default();
    f.names.family = Some(
        std::str::from_utf8(&b[5..5 + usize::from(b[4])])
            .map_err(|_| invalid("U8/M family is not UTF-8"))?
            .to_owned(),
    );
    f.properties
        .push(Property::new("u8m.family_id", i64::from(r16(b, 124)?)));
    f.properties
        .push(Property::new("u8m.style", i64::from(b[126])));
    f.properties
        .push(Property::new("u8m.point_size", i64::from(b[127])));
    if base == 2 {
        f.properties
            .push(Property::new("u8m.load_address", i64::from(r16(input, 0)?)));
    }
    f.metrics.ascent = Some(i64::from(b[252]));
    f.metrics.descent = Some(i64::from(b[253]));
    f.metrics.line_gap = Some(i64::from(b[254]));
    f.metrics.line_height = Some(i64::from(b[255]));
    for (i, cps) in aliases.into_iter().enumerate() {
        let v = r32(b, gt + i * 4)?;
        let p = (v & 0xffffff) as usize;
        let adv = i64::from(v >> 24);
        let mut g = if p == 0 {
            glyph(0, 0, 0, 0, adv, &[], l)?
        } else {
            let header = slice(b, p, 4)?;
            let h = u32::from(header[2]);
            let w = u32::from(header[3]);
            let len = (w as usize * h as usize).div_ceil(8);
            if p < 256 || p % 256 + 4 + len > 256 {
                return Err(invalid("U8/M bitmap crosses page or header"));
            }
            let data = slice(b, p + 4, len)?;
            let mut rows = raster(w, h, l, budget)?;
            let stride = (w as usize).div_ceil(8);
            for y in 0..h as usize {
                for x in 0..w as usize {
                    let k = y * w as usize + x;
                    if data[k / 8] & (0x80 >> (k % 8)) != 0 {
                        rows[y * stride + x / 8] |= 0x80 >> (x % 8);
                    }
                }
            }
            glyph(
                w,
                h,
                i64::from(header[1] as i8),
                -i64::from(header[0] as i8) - i64::from(h),
                adv,
                &rows,
                l,
            )?
        };
        g.codepoints = cps;
        let id = f.add_glyph(g)?;
        if i == 0 {
            f.metrics.default_glyph = Some(id);
        }
    }
    Ok((set(f, "u8m"), Report::default()))
}
fn add_mapping(
    maps: &mut Vec<BTreeMap<u8, u16>>,
    root: &mut u16,
    depth: usize,
    cp: u32,
    value: u16,
    budget: &mut Budget,
) -> Result<()> {
    if *root == 0 {
        check(maps.len() + 1, 65535, "U8/M maps")?;
        budget.add(64)?;
        *root = maps.len() as u16;
        maps.push(BTreeMap::new());
    }
    let mut index = usize::from(*root);
    for level in (0..depth).rev() {
        let k = ((cp >> (level * 6)) & 63) as u8;
        if level == 0 {
            budget.add(96)?;
            maps[index].insert(k, value);
        } else if let Some(&child) = maps[index].get(&k) {
            index = usize::from(child);
        } else {
            check(maps.len() + 1, 65535, "U8/M maps")?;
            budget.add(160)?;
            let child = maps.len();
            maps.push(BTreeMap::new());
            maps[index].insert(k, child as u16);
            index = child;
        }
    }
    Ok(())
}
fn page_record(at: usize, size: usize) -> Result<usize> {
    if size > 256 {
        return Err(invalid("U8/M record exceeds a page"));
    }
    Ok(if at % 256 + size > 256 {
        at.next_multiple_of(256)
    } else {
        at
    })
}
/// Write native page tables and continuous pixel bits, then audit native readback.
pub fn write(s: &FontSet, o: &WriteOptions, v: &U8mWriteOptions) -> Result<Written> {
    write_charged(s, o, v, ModelCharge::Estimate)
}
impl U8mCodec {
    /// [`write`](super::FormatWriter::write) with the model reserved by the caller.
    pub(crate) fn write_reserved(&self, s: &FontSet, o: &WriteOptions) -> Result<Written> {
        write_charged(s, o, &self.write_options, ModelCharge::Reserved)
    }
}
fn write_charged(
    s: &FontSet,
    o: &WriteOptions,
    v: &U8mWriteOptions,
    charge: ModelCharge,
) -> Result<Written> {
    let mut report = Report::default();
    let result = (|| {
        let mut budget = source_budget_for(s, &v.limits, charge)?;
        let converted = convert_gray(s, v.gray_threshold, &v.limits, &mut budget)?;
        let f = converted.single()?;
        let default = f
            .metrics
            .default_glyph
            .and_then(|id| f.glyph(id))
            .filter(|g| g.codepoints.is_empty());
        budget.items::<Option<&Glyph>>(f.len() + 1)?;
        let mut glyphs = vec![default];
        for (_, g) in f.glyphs() {
            if default.is_some_and(|d| std::ptr::eq(d, g)) {
                continue;
            }
            glyphs.push(Some(g));
        }
        check(glyphs.len(), 65535.min(v.limits.max_glyphs), "U8/M glyphs")?;
        let mut roots = [0u16; 58];
        let mut maps = vec![BTreeMap::new()];
        for (i, g) in glyphs.iter().enumerate().skip(1) {
            if let Some(g) = g {
                for cp in &g.codepoints {
                    let (root, depth, cp) = match *cp {
                        Codepoint::Legacy(c) if c <= 255 => ((c >> 6) as usize, 1, c),
                        Codepoint::Unicode(c) if c < 0x800 => (4 + (c >> 6) as usize, 1, c),
                        Codepoint::Unicode(c) if c < 0x10000 => (36 + (c >> 12) as usize, 2, c),
                        Codepoint::Unicode(c) => (52 + (c >> 18) as usize, 3, c),
                        _ => continue,
                    };
                    add_mapping(
                        &mut maps,
                        &mut roots[root],
                        depth,
                        cp,
                        i as u16,
                        &mut budget,
                    )?;
                }
            }
        }
        let mt = 256;
        let mut size = mt + maps.len() * 4;
        budget.items::<usize>(maps.len() + glyphs.len())?;
        let mut locations = Vec::with_capacity(maps.len());
        for m in &maps {
            size = page_record(size, m.len() * 4)?;
            locations.push(if m.is_empty() { 0 } else { size });
            size += m.len() * 4;
        }
        let gt = size.next_multiple_of(256);
        size = gt + glyphs.len() * 4;
        let mut bitmaps = Vec::with_capacity(glyphs.len());
        for g in &glyphs {
            let len = if let Some(g) = g {
                let bb = g.metrics.bounds;
                byte(i64::from(bb.width))?;
                byte(i64::from(bb.height))?;
                4 + (bb.width as usize * bb.height as usize).div_ceil(8)
            } else {
                0
            };
            size = page_record(size, len)?;
            bitmaps.push(if len == 0 { 0 } else { size });
            size += len;
        }
        check(size, 0x1000000, "U8/M address space")?;
        let load = v
            .load_address
            .or(match f.properties.get("u8m.load_address") {
                Some(PropertyValue::Integer(n)) => {
                    Some(u16::try_from(*n).map_err(|_| invalid("U8/M load address exceeds u16"))?)
                }
                None => None,
                _ => return Err(invalid("u8m.load_address must be an integer")),
            });
        let prefix = usize::from(load.is_some()) * 2;
        let mut out = output(size + prefix, v.max_output_bytes, &v.limits, &mut budget)?;
        if let Some(address) = load {
            p16(&mut out, 0, address);
        }
        let b = &mut out[prefix..];
        b[..4].copy_from_slice(b"U8/M");
        let name = f.names.family.as_deref().unwrap_or("");
        if name.len() > 118 || name.contains('\0') {
            return Err(invalid("U8/M name exceeds 118 UTF-8 bytes"));
        }
        b[4] = name.len() as u8;
        b[5..5 + name.len()].copy_from_slice(name.as_bytes());
        p16(
            b,
            124,
            u16::try_from(int(&f.properties, "u8m.family_id", 0)?)
                .map_err(|_| invalid("U8/M family ID exceeds u16"))?,
        );
        b[126] = byte(int(&f.properties, "u8m.style", 0)?)?;
        b[127] = byte(int(
            &f.properties,
            "u8m.point_size",
            i64::from(f.metrics.pixel_size.unwrap_or(0)),
        )?)?;
        p16(b, 128, (gt / 256) as u16);
        p16(b, 130, glyphs.len() as u16);
        p16(b, 132, (mt / 256) as u16);
        p16(b, 134, maps.len() as u16);
        for (i, &root) in roots.iter().enumerate() {
            p16(b, 136 + i * 2, root);
        }
        b[252] = byte(f.metrics.ascent.unwrap_or(0))?;
        b[253] = byte(f.metrics.descent.unwrap_or(0))?;
        b[254] = byte(f.metrics.line_gap.unwrap_or(0))?;
        b[255] = byte(f.metrics.line_height.unwrap_or(0))?;
        for (i, m) in maps.iter().enumerate() {
            let at = locations[i];
            p32(b, mt + i * 4, at as u32 | ((m.len() as u32) << 24));
            for (j, (&k, &value)) in m.iter().enumerate() {
                b[at + j * 4] = k;
                b[at + j * 4 + 1] = k;
                p16(b, at + j * 4 + 2, value);
            }
        }
        for (i, g) in glyphs.iter().enumerate() {
            let Some(g) = g else {
                continue;
            };
            let bm = mono(g)?;
            let bb = g.metrics.bounds;
            let adv = byte(
                g.metrics
                    .advance
                    .or(f.metrics.glyph_defaults.advance)
                    .map_or(i64::from(bb.width), |a| a.x),
            )?;
            let at = bitmaps[i];
            p32(b, gt + i * 4, at as u32 | (u32::from(adv) << 24));
            b[at] = i8::try_from(-bb.y - i64::from(bb.height))
                .map_err(|_| invalid("U8/M top offset exceeds i8"))? as u8;
            b[at + 1] =
                i8::try_from(bb.x).map_err(|_| invalid("U8/M left offset exceeds i8"))? as u8;
            b[at + 2] = bb.height as u8;
            b[at + 3] = bb.width as u8;
            for y in 0..bb.height as usize {
                for x in 0..bb.width as usize {
                    if bm.as_bytes()[y * bm.stride() + x / 8] & (0x80 >> (x % 8)) != 0 {
                        let k = y * bb.width as usize + x;
                        b[at + 4 + k / 8] |= 0x80 >> (k % 8);
                    }
                }
            }
        }
        let (target, r) = parse(&out, &v.limits, &mut budget)?;
        report.extend(r);
        audit(s, &target, "u8m", &mut report, &mut budget)?;
        Ok(OutputBundle::single(o.primary_name.clone(), out))
    })();
    finish(result, report, "u8m", o.strict)
}
