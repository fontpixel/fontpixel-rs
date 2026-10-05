//! RISC OS ZapFont and ZapRedraw UCS native chunks and translation tables.
use super::w5a2_common::*;
use super::{
    Confidence, FormatDetector, FormatReader, FormatWriter, ModelCharge, ReadOptions, WriteOptions,
};
use crate::resources::{OutputBundle, ReadContext, Written};
use std::collections::{BTreeMap, BTreeSet};
/// Native container layout.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ZapVariant {
    /// Original contiguous numeric-cell format.
    Old,
    /// ZapRedraw UCS chunks and optional null glyph.
    #[default]
    Redraw,
}
/// Native font interpretation.
#[derive(Debug, Clone, Default)]
pub struct ZapfontReadOptions {
    /// Require a particular native variant; default detects magic.
    pub variant: Option<ZapVariant>,
    /// Baseline below the cell top.
    pub baseline: Option<i64>,
    /// Explicit charset metadata for native legacy positions.
    pub charset: Option<String>,
}
/// Native writer profile.
#[derive(Debug, Clone, Default)]
pub struct ZapfontWriteOptions {
    /// Native output variant.
    pub variant: ZapVariant,
    /// Optional fixed output cell.
    pub cell: Option<Bounds>,
    /// Explicit Gray8 threshold.
    pub gray_threshold: Option<u8>,
    /// Permit Unicode numeric labels in legacy slots (including chunk zero).
    pub unicode_as_legacy: bool,
    /// Cumulative storage bounds.
    pub limits: ResourceLimits,
}
impl ZapfontWriteOptions {
    fn native(&self) -> NativeWriteOptions {
        NativeWriteOptions {
            cell: self.cell,
            gray_threshold: self.gray_threshold,
            unicode_as_legacy: self.unicode_as_legacy,
            limits: self.limits.clone(),
        }
    }
}
/// Native Zap font codec.
#[derive(Debug, Clone, Default)]
pub struct ZapfontCodec {
    /// Input choices.
    pub read_options: ZapfontReadOptions,
    /// Output choices.
    pub write_options: ZapfontWriteOptions,
}
impl FormatDetector for ZapfontCodec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(detect(c.bytes()))
    }
}
impl FormatReader<FontSet> for ZapfontCodec {
    fn read(&self, c: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
        read(c, &self.read_options)
    }
}
impl FormatWriter<FontSet> for ZapfontCodec {
    fn write(&self, s: &FontSet, o: &WriteOptions) -> Result<Written> {
        write(s, o, &self.write_options)
    }
}
impl ZapfontCodec {
    /// [`write`](FormatWriter::write) with the model reserved by the caller.
    pub(crate) fn write_reserved(&self, s: &FontSet, o: &WriteOptions) -> Result<Written> {
        write_charged(s, o, &self.write_options, ModelCharge::Reserved)
    }
}
/// Detect either exact native magic.
pub fn detect(s: &[u8]) -> Confidence {
    if s.starts_with(b"ZapFont\r") || s.starts_with(b"ZRUF") {
        Confidence::Certain
    } else {
        Confidence::NoMatch
    }
}
fn variant(s: &[u8]) -> Result<ZapVariant> {
    if s.starts_with(b"ZapFont\r") {
        Ok(ZapVariant::Old)
    } else if s.starts_with(b"ZRUF") {
        Ok(ZapVariant::Redraw)
    } else {
        Err(invalid("unknown Zap font signature"))
    }
}
fn interval(
    list: &mut Vec<(usize, usize)>,
    start: usize,
    len: usize,
    total: usize,
    b: &mut Budget,
) -> Result<()> {
    let end = start
        .checked_add(len)
        .ok_or_else(|| invalid("Zap section overflow"))?;
    if end > total {
        return Err(invalid("Zap section exceeds input"));
    }
    b.add(32)?;
    list.push((start, end));
    Ok(())
}
fn parse(
    s: &[u8],
    o: &ZapfontReadOptions,
    l: &ResourceLimits,
    b: &mut Budget,
) -> Result<(FontSet, Report)> {
    check(s.len(), l.max_input_bytes, "Zap input bytes")?;
    let v = variant(s)?;
    if o.variant.is_some_and(|x| x != v) {
        return Err(invalid("Zap native variant disagrees with reader option"));
    }
    let hdr = take(s, 0, 32)?;
    let (w, h) = (u32le(hdr, 8)?, u32le(hdr, 12)?);
    if w == 0 || h == 0 {
        return Err(invalid("Zap cell must be positive"));
    }
    let n = size(w, h, l)?;
    b.add(o.charset.as_ref().map_or(0, String::len))?;
    let opts = NativeReadOptions {
        baseline: o.baseline,
        charset: o.charset.clone(),
    };
    let mut f = base("zapfont", &opts, b)?;
    let mut report = Report::default();
    if v == ZapVariant::Old {
        let (first, last) = (u32le(hdr, 16)?, u32le(hdr, 20)?);
        if first >= last || u32le(hdr, 24)? != 0 || u32le(hdr, 28)? != 0 {
            return Err(invalid("invalid old Zap range or reserved fields"));
        }
        let count = (last - first) as usize;
        check(count, l.max_glyphs, "Zap glyph count")?;
        let bytes = count
            .checked_mul(n)
            .ok_or_else(|| invalid("Zap raster overflow"))?;
        if s.len() != 32usize.saturating_add(bytes) {
            return Err(invalid("Zap raster length mismatch"));
        }
        for (i, wire) in s[32..].chunks_exact(n).enumerate() {
            let mut raw = allocated(n, b)?;
            for (a, v) in raw.iter_mut().zip(wire) {
                *a = v.reverse_bits();
            }
            glyph(
                &mut f,
                Codepoint::Legacy(first + i as u32),
                w,
                h,
                &raw,
                &opts,
                l,
                b,
            )?;
        }
    } else {
        if u32le(hdr, 4)? != 0 || u32le(hdr, 24)? != 0 || u32le(hdr, 28)? != 0 {
            return Err(invalid("ZapRedraw reserved fields are nonzero"));
        }
        let dir = u32le(hdr, 16)? as usize;
        if dir == 0 || !dir.is_multiple_of(16) {
            return Err(invalid("ZapRedraw chunk directory length invalid"));
        }
        check(dir, l.max_table_bytes, "Zap chunk directory bytes")?;
        let entries = take(s, 32, dir)?;
        let null = u32le(hdr, 20)? as usize;
        let mut spans = Vec::new();
        interval(&mut spans, 0, 32 + dir, s.len(), b)?;
        let mut null_id = None;
        if null != 0 {
            check(1, l.max_glyphs, "Zap null glyph count")?;
            let wire = take(s, null, n)?;

            let mut raw = allocated(n, b)?;
            for (a, v) in raw.iter_mut().zip(wire) {
                *a = v.reverse_bits();
            }
            b.add(2048 + n)?;
            let mut g = Glyph::new(Bitmap::<Mono>::from_raw(w, h, &raw, bitmap_limits(l))?);
            g.name = Some("null".into());
            g.metrics.bounds.y = opts
                .baseline
                .unwrap_or(i64::from(h))
                .checked_sub(i64::from(h))
                .ok_or_else(|| invalid("Zap baseline overflow"))?;
            g.metrics.advance = Some(Vector::new(i64::from(w), 0));
            null_id = Some(f.add_glyph(g)?);
        }
        let mut chunks = BTreeSet::new();
        let mut chunk_zero = false;
        for entry in entries.as_chunks::<16>().0.iter() {
            let (base, count, off) = (
                u32le(entry, 0)?,
                u32le(entry, 4)? as usize,
                u32le(entry, 8)? as usize,
            );
            if base % 256 != 0
                || base > 0x10ff00
                || count == 0
                || count > 256
                || u32le(entry, 12)? != 0
            {
                return Err(invalid("invalid ZapRedraw chunk fields"));
            }
            b.add(64)?;
            if !chunks.insert(base) {
                return Err(invalid("duplicate ZapRedraw chunk"));
            }
            if base == 0 {
                if count != 256 {
                    return Err(invalid("ZapRedraw chunk zero must be full"));
                }
                chunk_zero = true;
            }
            check(
                f.len().saturating_add(count),
                l.max_glyphs,
                "Zap glyph count",
            )?;
            let table = if count < 256 { 256 } else { 0 };
            let bytes = count
                .checked_mul(n)
                .and_then(|v| v.checked_add(table))
                .ok_or_else(|| invalid("Zap chunk size overflow"))?;
            let data = take(s, off, bytes)?;
            interval(&mut spans, off, bytes, s.len(), b)?;
            b.add(count.saturating_mul(160) + 2048)?;
            let mut mappings = vec![Vec::new(); count];
            b.items::<usize>(256)?;
            let ordinals: Vec<usize> = if table == 0 {
                (1..=256).collect()
            } else {
                data[..256].iter().map(|v| usize::from(*v)).collect()
            };
            for (pos, ordinal) in ordinals.into_iter().enumerate() {
                if ordinal == 0 {
                    continue;
                }
                if ordinal > count {
                    return Err(invalid("Zap translation ordinal outside glyph data"));
                }
                let cp = base + pos as u32;
                let code = if base == 0 {
                    Codepoint::Legacy(cp)
                } else {
                    if char::from_u32(cp).is_none() {
                        return Err(invalid("ZapRedraw chunk contains invalid Unicode scalar"));
                    }
                    Codepoint::Unicode(cp)
                };
                b.add(32)?;
                mappings[ordinal - 1].push(code);
            }
            for (wire, codes) in data[table..].chunks_exact(n).zip(mappings) {
                let mut raw = allocated(n, b)?;
                for (a, v) in raw.iter_mut().zip(wire) {
                    *a = v.reverse_bits();
                }
                let id = glyph(
                    &mut f,
                    codes
                        .first()
                        .copied()
                        .unwrap_or(Codepoint::Legacy(u32::MAX)),
                    w,
                    h,
                    &raw,
                    &opts,
                    l,
                    b,
                )?;
                f.edit_glyph(id, |g| {
                    g.codepoints = codes;
                    Ok(())
                })?;
            }
        }
        if !chunk_zero {
            return Err(invalid("ZapRedraw requires full chunk zero"));
        }
        if null != 0
            && !spans
                .iter()
                .any(|(a, z)| *a <= null && null + n <= *z && *a >= 32 + dir)
        {
            interval(&mut spans, null, n, s.len(), b)?;
        }
        spans.sort_unstable();
        let mut end = 0;
        for (a, z) in spans {
            if a < end {
                return Err(invalid("ZapRedraw sections overlap"));
            }
            if a > end {
                b.loss(
                    &mut report,
                    "zapfont",
                    "read.padding",
                    Some(0),
                    None,
                    "Unassigned bytes between native sections are omitted",
                )?;
            }
            end = z;
        }
        if end != s.len() {
            b.loss(
                &mut report,
                "zapfont",
                "read.padding",
                Some(0),
                None,
                "Trailing bytes outside native sections are omitted",
            )?;
        }
        f.metrics.default_glyph = null_id;
    }
    Ok((complete(f, "zapfont")?, report))
}
/// Read original or chunked native font data with checked offsets and ordinals.
pub fn read(c: &mut ReadContext<'_>, o: &ZapfontReadOptions) -> Result<(FontSet, Report)> {
    let mut b = Budget::new(c.retained_input_bytes(), c.limits())?;
    parse(c.bytes(), o, c.limits(), &mut b)
}
fn append_glyph(
    out: &mut Vec<u8>,
    g: Option<&Glyph>,
    c: Bounds,
    l: &ResourceLimits,
    b: &mut Budget,
) -> Result<()> {
    let pixels = raster(g, c, l, b)?;
    out.extend(pixels.as_bytes().iter().map(|v| v.reverse_bits()));
    drop(pixels);
    b.release(raster_charge(c, l)?)
}
/// What [`raster`] admits for one cell bitmap, returned once it has dropped.
fn raster_charge(c: crate::bitmap::Bounds, l: &ResourceLimits) -> Result<usize> {
    size(c.width, c.height, l)?
        .checked_mul(2)
        .ok_or_else(|| invalid("Zap raster size overflow"))
}
/// An exactly sized zeroed buffer of `n` bytes, admitted before allocation.
fn exact(n: usize, b: &mut Budget) -> Result<Vec<u8>> {
    b.add(n)?;
    let mut v = Vec::new();
    v.try_reserve_exact(n)
        .map_err(|_| invalid("Zap allocation failed"))?;
    v.resize(n, 0);
    b.add(v.capacity() - n)?;
    Ok(v)
}
/// Emit the native selected variant; chunked output always includes chunk zero.
pub fn write(s: &FontSet, o: &WriteOptions, p: &ZapfontWriteOptions) -> Result<Written> {
    write_charged(s, o, p, ModelCharge::Estimate)
}
fn write_charged(
    s: &FontSet,
    o: &WriteOptions,
    p: &ZapfontWriteOptions,
    charge: ModelCharge,
) -> Result<Written> {
    let mut report = Report::default();
    let result = (|| {
        let native = p.native();
        let mut b = source_for(s, &native, charge)?;
        b.add(o.primary_name.as_str().len().saturating_mul(2))?;
        let source = convert_gray(s, native.gray_threshold, &p.limits, &mut b)?;
        let f = source.single()?;
        let c = cell(f, &native, None)?;
        let n = size(c.width, c.height, &p.limits)?;
        let mut out;
        if p.variant == ZapVariant::Old {
            let map = legacy_map(f, &native, u32::MAX - 1, &mut b)?;
            let first = *map
                .first_key_value()
                .ok_or_else(|| invalid("empty Zap output"))?
                .0;
            let last = map
                .last_key_value()
                .ok_or_else(|| invalid("empty Zap output"))?
                .0
                + 1;
            let count = (last - first) as usize;
            check(count, p.limits.max_glyphs, "Zap output glyph count")?;
            let total = count
                .checked_mul(n)
                .and_then(|v| v.checked_add(32))
                .ok_or_else(|| invalid("Zap output size overflow"))?;
            out = exact(total, &mut b)?;
            out[..8].copy_from_slice(b"ZapFont\r");
            put32(&mut out, 8, c.width);
            put32(&mut out, 12, c.height);
            put32(&mut out, 16, first);
            put32(&mut out, 20, last);
            for (i, wire) in out[32..].chunks_exact_mut(n).enumerate() {
                let pixels = raster(map.get(&(first + i as u32)).copied(), c, &p.limits, &mut b)?;
                for (a, v) in wire.iter_mut().zip(pixels.as_bytes()) {
                    *a = v.reverse_bits();
                }
                drop(pixels);
                b.release(raster_charge(c, &p.limits)?)?;
            }
        } else {
            let mut map = BTreeMap::new();
            for (_, g) in f.glyphs() {
                for code in &g.codepoints {
                    let cp = match code {
                        Codepoint::Legacy(v) if *v < 256 => *v,
                        Codepoint::Unicode(v) if *v >= 256 || p.unicode_as_legacy => *v,
                        _ => continue,
                    };
                    b.add(128)?;
                    if map.insert(cp, g).is_some() {
                        return Err(invalid("duplicate Zap native position"));
                    }
                }
            }
            if map.is_empty() {
                return Err(invalid("ZapRedraw has no storable mappings"));
            }
            let mut chunks = BTreeMap::<u32, Vec<u32>>::new();
            chunks.insert(0, (0..256).collect());
            b.add(4096)?;
            for &cp in map.keys().filter(|v| **v >= 256) {
                b.add(16)?;
                chunks.entry(cp & !255).or_default().push(cp);
            }
            let null = f
                .metrics
                .default_glyph
                .and_then(|id| f.glyph(id))
                .or_else(|| {
                    f.glyphs()
                        .find_map(|(_, g)| (g.name.as_deref() == Some("null")).then_some(g))
                });
            let count = chunks.values().map(Vec::len).sum::<usize>() + usize::from(null.is_some());
            check(count, p.limits.max_glyphs, "Zap output glyph count")?;
            let directory = chunks
                .len()
                .checked_mul(16)
                .ok_or_else(|| invalid("Zap directory overflow"))?;
            let total = 32usize
                .checked_add(directory)
                .and_then(|v| v.checked_add(count.checked_mul(n)?))
                .and_then(|v| {
                    v.checked_add(chunks.values().filter(|v| v.len() < 256).count() * 256)
                })
                .ok_or_else(|| invalid("Zap output size overflow"))?;
            check(total, u32::MAX as usize, "Zap 32-bit offsets")?;
            // The whole file is allocated once at its final size.
            b.add(total)?;
            out = Vec::new();
            out.try_reserve_exact(total)
                .map_err(|_| invalid("Zap allocation failed"))?;
            b.add(out.capacity() - total)?;
            out.resize(32 + directory, 0);
            out[..4].copy_from_slice(b"ZRUF");
            put32(&mut out, 8, c.width);
            put32(&mut out, 12, c.height);
            put32(&mut out, 16, directory as u32);
            if let Some(g) = null {
                put32(&mut out, 20, (32 + directory) as u32);
                append_glyph(&mut out, Some(g), c, &p.limits, &mut b)?;
            }
            for (i, (base, codes)) in chunks.into_iter().enumerate() {
                let offset = out.len() as u32;
                put32(&mut out, 32 + i * 16, base);
                put32(&mut out, 36 + i * 16, codes.len() as u32);
                put32(&mut out, 40 + i * 16, offset);
                if codes.len() < 256 {
                    let mut table = [0u8; 256];
                    for (i, cp) in codes.iter().enumerate() {
                        table[(cp % 256) as usize] = (i + 1) as u8;
                    }
                    out.extend_from_slice(&table);
                }
                for cp in codes {
                    append_glyph(&mut out, map.get(&cp).copied(), c, &p.limits, &mut b)?;
                }
            }
        }
        // The readback parses `out` in place; no copy of the input is made.
        check(
            out.len(),
            p.limits.max_input_bytes,
            "native output readback input bytes",
        )?;
        let before = b.used;
        let (back, r) = parse(&out, &Default::default(), &p.limits, &mut b)?;
        // The parser admitted each glyph and its temporaries before allocating
        // them. Once it returns only the decoded model and its report remain,
        // so their exact retained bytes replace those admissions.
        let live = back
            .retained_usage(b.max)?
            .bytes
            .checked_add(crate::resources::report_bytes(&r, b.max)?)
            .ok_or_else(|| invalid("Zap readback size overflow"))?;
        b.release(b.used - before)?;
        b.add(live)?;
        if report.entries.is_empty() {
            // Moving the reader's report allocates nothing new.
            report = r;
        } else {
            report.extend(r);
        }
        audit(s, &back, "zapfont", &mut report, &mut b)?;
        Ok(OutputBundle::single(o.primary_name.clone(), out))
    })();
    finish(result, report, "zapfont", o.strict)
}
