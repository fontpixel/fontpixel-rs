//! GEM/GDOS fonts with independent integer and bitmap word order.
use super::legacy_common::*;
use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
use crate::bitmap::Bounds;
pub use crate::bitmap::ByteOrder;
use crate::model::{Codepoint, Font, FontSet, Glyph, Property, Rational, Spacing, Vector};
use crate::resources::{OutputBundle, ReadContext, Written};
use crate::{Report, ResourceLimits, Result};

/// Explicit GEM input interpretation.
#[derive(Debug, Clone, Default)]
pub struct GemReadOptions {
    /// Integer order; absent requires unique structural detection.
    pub byte_order: Option<ByteOrder>,
    /// Raster word order; absent follows the standard-form flag.
    pub bitmap_word_order: Option<ByteOrder>,
}
/// Bounded GEM output choices.
#[derive(Debug, Clone)]
pub struct GemWriteOptions {
    /// Integer byte order, default Motorola big endian.
    pub byte_order: ByteOrder,
    /// Raster word order, independent of integer byte order.
    pub bitmap_word_order: ByteOrder,
    /// Explicitly allow Unicode scalar numbers as untagged legacy codes.
    pub unicode_as_legacy: bool,
    /// Shared geometry and allocation limits.
    pub limits: ResourceLimits,
    /// Maximum output length, also bounded by reader limits.
    pub max_output_bytes: usize,
    /// Explicit threshold for grayscale input; absent rejects grayscale.
    pub gray_threshold: Option<u8>,
}
impl Default for GemWriteOptions {
    fn default() -> Self {
        Self {
            byte_order: ByteOrder::Big,
            bitmap_word_order: ByteOrder::Big,
            unicode_as_legacy: false,
            limits: ResourceLimits::default(),
            gray_threshold: None,
            max_output_bytes: 64 * 1024 * 1024,
        }
    }
}
/// Registry codec.
#[derive(Debug, Clone, Default)]
pub struct GemCodec {
    /// Reader interpretation.
    pub read_options: GemReadOptions,
    /// Writer choices.
    pub write_options: GemWriteOptions,
}
impl FormatDetector for GemCodec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(if header(c.bytes(), &self.read_options).is_ok() {
            Confidence::Possible
        } else {
            Confidence::NoMatch
        })
    }
}
impl FormatReader<FontSet> for GemCodec {
    fn read(&self, c: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
        read(c, &self.read_options)
    }
}
impl FormatWriter<FontSet> for GemCodec {
    fn write(&self, s: &FontSet, o: &WriteOptions) -> Result<Written> {
        write(s, o, &self.write_options)
    }
}
#[derive(Clone)]
struct Header {
    e: ByteOrder,
    first: u16,
    last: u16,
    flags: u16,
    hor: usize,
    off: usize,
    data: usize,
    stride: usize,
    height: u32,
}
fn candidate(b: &[u8], e: ByteOrder) -> Result<Header> {
    slice(b, 0, 88)?;
    let first = u16_at(b, 36, e)?;
    let last = u16_at(b, 38, e)?;
    if first > last {
        return Err(invalid("GEM reversed character range"));
    }
    let h = Header {
        e,
        first,
        last,
        flags: u16_at(b, 66, e)?,
        hor: u32_at(b, 68, e)? as usize,
        off: u32_at(b, 72, e)? as usize,
        data: u32_at(b, 76, e)? as usize,
        stride: u16_at(b, 80, e)? as usize,
        height: u32::from(u16_at(b, 82, e)?),
    };
    if h.height == 0 || h.off < 88 || h.data < 88 {
        return Err(invalid("invalid GEM dimensions or offsets"));
    }
    let n = usize::from(last - first) + 1;
    slice(b, h.off, (n + 1) * 2)?;
    let mut last = 0;
    for i in 0..=n {
        let off = u16_at(b, h.off + i * 2, e)? as usize;
        if off < last || off > h.stride * 8 {
            return Err(invalid("invalid GEM character offsets"));
        }
        last = off;
    }
    if h.flags & 2 != 0 {
        if h.hor < 88 {
            return Err(invalid("invalid GEM horizontal table"));
        }
        slice(b, h.hor, n * 2)?;
    }
    if h.flags & 32 == 0 {
        slice(b, h.data, h.stride * h.height as usize)?;
    } else {
        slice(b, 88, 64)?;
        slice(b, h.data, 1)?;
    }
    Ok(h)
}
fn header(b: &[u8], o: &GemReadOptions) -> Result<Header> {
    if let Some(e) = o.byte_order {
        return candidate(b, e);
    }
    match (
        candidate(b, ByteOrder::Big),
        candidate(b, ByteOrder::Little),
    ) {
        (Ok(h), Err(_)) | (Err(_), Ok(h)) => Ok(h),
        (Ok(_), Ok(_)) => Err(invalid("ambiguous GEM byte order; supply byte_order")),
        _ => Err(invalid("invalid GEM header")),
    }
}
const KEYS: [(&str, usize); 14] = [
    ("gem.font_id", 0),
    ("gem.top", 40),
    ("gem.ascent", 42),
    ("gem.half", 44),
    ("gem.descent", 46),
    ("gem.bottom", 48),
    ("gem.max_char_width", 50),
    ("gem.max_cell_width", 52),
    ("gem.left_offset", 54),
    ("gem.right_offset", 56),
    ("gem.thicken", 58),
    ("gem.ul_size", 60),
    ("gem.lighten", 62),
    ("gem.skew", 64),
];
/// Decode one GEM font using structural byte-order detection.
pub fn read(c: &mut ReadContext<'_>, o: &GemReadOptions) -> Result<(FontSet, Report)> {
    let mut budget = Budget::new(c.retained_input_bytes(), c.limits())?;
    parse(c.bytes(), o, c.limits(), &mut budget)
}
fn parse(
    b: &[u8],
    o: &GemReadOptions,
    l: &ResourceLimits,
    budget: &mut Budget,
) -> Result<(FontSet, Report)> {
    let h = header(b, o)?;
    if h.flags & 32 == 0 || u32_at(b, 88, h.e)? == 0 {
        return parse_single(b, o, l, budget);
    }
    let mut offset = 0usize;
    let mut visited = std::collections::BTreeSet::new();
    let mut result: Option<FontSet> = None;
    let mut report = Report::default();
    let mut glyphs = 0usize;
    loop {
        if !visited.insert(offset) {
            return Err(invalid("GEM linked-section cycle"));
        }
        check(visited.len(), l.max_resources, "GEM sections")?;
        budget.add(64)?;
        let raw = slice(b, offset, 152)?;
        let first = u16_at(raw, 36, h.e)?;
        let last = u16_at(raw, 38, h.e)?;
        if first > last {
            return Err(invalid("GEM section character range is reversed"));
        }
        let count = usize::from(last - first) + 1;
        if u16_at(raw, 66, h.e)? & 32 == 0 {
            return Err(invalid("GEM linked section has no extended header"));
        }
        let next = u32_at(raw, 88, h.e)? as usize;
        if next != 0 && (next <= offset || next >= b.len()) {
            return Err(invalid("GEM next section offset is invalid"));
        }
        let original_data = u32_at(raw, 76, h.e)? as usize;
        let data_end = if next == 0 { b.len() } else { next };
        if original_data >= data_end {
            return Err(invalid("GEM compressed data overlaps section boundary"));
        }
        let hor_bytes = if u16_at(raw, 66, h.e)? & 2 != 0 {
            count * 2
        } else {
            0
        };
        let off = 152 + hor_bytes;
        let data = off + (count + 1) * 2;
        let compressed_len = data_end - original_data;
        check(compressed_len, l.max_table_bytes, "GEM compressed section")?;
        budget.add(data + compressed_len + 152)?;
        let mut standalone = vec![0; data + compressed_len];
        standalone[..152].copy_from_slice(raw);
        put32(&mut standalone, 88, 0, h.e);
        if hor_bytes != 0 {
            let orig = u32_at(raw, 68, h.e)? as usize;
            standalone[152..off].copy_from_slice(slice(b, orig, hor_bytes)?);
            put32(&mut standalone, 68, 152, h.e);
        }
        standalone[off..data].copy_from_slice(slice(
            b,
            u32_at(raw, 72, h.e)? as usize,
            (count + 1) * 2,
        )?);
        standalone[data..].copy_from_slice(slice(b, original_data, compressed_len)?);
        put32(&mut standalone, 72, off as u32, h.e);
        put32(&mut standalone, 76, data as u32, h.e);
        let (mut part, part_report) = parse_single(
            &standalone,
            &GemReadOptions {
                byte_order: Some(h.e),
                bitmap_word_order: o.bitmap_word_order,
            },
            l,
            budget,
        )?;
        report.extend(part_report);
        let font = part.fonts.remove(0);
        glyphs = glyphs
            .checked_add(font.len())
            .ok_or_else(|| invalid("GEM glyph count overflow"))?;
        check(glyphs, l.max_glyphs, "GEM total glyphs")?;
        if let Some(result) = &mut result {
            let target = &mut result.fonts[0];
            if target.metrics != font.metrics || target.names != font.names {
                budget.diagnostic(&mut report,"gem","read.section_metadata",crate::report::Impact::Loss,Some(0),None,"Linked GEM section has differing global metadata; first section is authoritative, original headers retained")?;
            }
            for (_, g) in font.glyphs() {
                budget.items::<Glyph>(1)?;
                budget.add(g.bitmap.as_bytes().len() + 128)?;
                target.add_glyph(g.clone())?;
            }
        } else {
            result = Some(part_with_font(font));
        }
        if let Some(result) = &mut result {
            result.fonts[0].headers.push(Property::new(
                format!("gem.section.{}.header", visited.len() - 1),
                crate::model::PropertyValue::Bytes(raw.to_vec()),
            ));
        }
        if next == 0 {
            break;
        }
        offset = next;
    }
    Ok((
        result.ok_or_else(|| invalid("empty GEM section chain"))?,
        report,
    ))
}
fn part_with_font(f: Font) -> FontSet {
    let mut set = FontSet::from(f);
    set.source_format = Some("gem".into());
    set
}
fn parse_single(
    b: &[u8],
    o: &GemReadOptions,
    l: &ResourceLimits,
    budget: &mut Budget,
) -> Result<(FontSet, Report)> {
    let h = header(b, o)?;
    let count = usize::from(h.last - h.first) + 1;
    check(count, l.max_glyphs, "GEM glyphs")?;
    check((count + 1) * 2, l.max_table_bytes, "GEM offset table")?;
    let n = h.stride * h.height as usize;
    check(n, l.max_table_bytes, "GEM bitmap strip")?;
    budget.items::<Glyph>(count)?;
    budget.add(n)?;
    let mut strip = if h.flags & 32 != 0 {
        decode(b, &h, l)?
    } else {
        slice(b, h.data, n)?.to_vec()
    };
    if h.flags & 32 == 0
        && o.bitmap_word_order.unwrap_or(if h.flags & 4 != 0 {
            ByteOrder::Big
        } else {
            ByteOrder::Little
        }) == ByteOrder::Little
        && h.stride != 0
    {
        for row in strip.chunks_mut(h.stride) {
            for word in row.as_chunks_mut::<2>().0 {
                word.swap(0, 1);
            }
        }
    }
    let bottom = i64::from(u16_at(b, 48, h.e)?);
    let mut f = Font::default();
    f.names.full = Some(latin(
        b[4..36].split(|x| *x == 0).next().unwrap_or_default(),
    ));
    f.metrics.point_size = Some(Rational::new(i64::from(u16_at(b, 2, h.e)? as i16), 1)?);
    f.metrics.pixel_size = Some(h.height);
    f.metrics.ascent = Some(i64::from(u16_at(b, 42, h.e)?) + 1);
    f.metrics.descent = Some(i64::from(u16_at(b, 46, h.e)?));
    f.metrics.line_height = Some(i64::from(h.height));
    f.metrics.underline_thickness = Some(i64::from(u16_at(b, 60, h.e)?));
    f.metrics.spacing = Some(if h.flags & 8 != 0 {
        Spacing::Monospaced
    } else {
        Spacing::Proportional
    });
    for (key, at) in KEYS {
        f.properties
            .push(Property::new(key, i64::from(u16_at(b, at, h.e)?)));
    }
    f.properties.push(Property::new(
        "gem.flags",
        i64::from(h.flags & !(2 | 4 | 8 | 32)),
    ));
    f.properties.push(Property::new(
        "gem.next_font",
        i64::from(u32_at(b, 84, h.e)?),
    ));
    budget.add(4096)?;
    if h.flags & 32 != 0 {
        budget.add(256)?;
        f.headers.push(Property::new(
            "gem.extended_header",
            crate::model::PropertyValue::Bytes(slice(b, 88, 64)?.to_vec()),
        ));
    }
    for i in 0..count {
        let x = usize::from(u16_at(b, h.off + i * 2, h.e)?);
        let end = usize::from(u16_at(b, h.off + (i + 1) * 2, h.e)?);
        let width = (end - x) as u32;
        let bitmap = extract(&strip, h.stride, x, width, h.height, l, budget)?;
        let mut g = Glyph::new(bitmap);
        let (left, right) = if h.flags & 2 != 0 {
            (
                -(b[h.hor + 2 * i] as i8 as i64),
                -(b[h.hor + 2 * i + 1] as i8 as i64),
            )
        } else {
            (0, 0)
        };
        g.metrics.bounds = Bounds::new(width, h.height, left, -bottom);
        g.metrics.advance = Some(Vector::new(left + i64::from(width) + right, 0));
        g.codepoints
            .push(Codepoint::Legacy(u32::from(h.first) + i as u32));
        budget.add(128)?;
        f.add_glyph(g)?;
    }
    f.source_format = Some("gem".into());
    let mut s = FontSet::from(f);
    s.source_format = Some("gem".into());
    let mut report = Report::default();
    if h.flags & 32 != 0 {
        budget.diagnostic(
            &mut report,
            "gem",
            "read.compression",
            crate::report::Impact::Repair,
            Some(0),
            None,
            "Expanded GEM differential run-length bitmap",
        )?;
    }
    Ok((s, report))
}
fn decode(b: &[u8], h: &Header, l: &ResourceLimits) -> Result<Vec<u8>> {
    let n = h.stride * h.height as usize;
    check(n, l.max_table_bytes, "GEM decoded bitmap")?;
    let src = slice(b, h.data, b.len() - h.data)?;
    let total = n
        .checked_mul(8)
        .and_then(|n| n.checked_add(1))
        .ok_or_else(|| invalid("GEM bitmap overflow"))?;
    let mut at = 0usize;
    let mut bit = || -> Result<bool> {
        let byte = at / 8;
        let index = if h.e == ByteOrder::Little {
            byte ^ 1
        } else {
            byte
        };
        let v = src
            .get(index)
            .ok_or_else(|| invalid("truncated GEM compressed bitmap"))?;
        let value = v & (0x80 >> (at % 8)) != 0;
        at += 1;
        Ok(value)
    };
    let mut out = vec![0; n];
    let mut pos = 0usize;
    while pos < total {
        let mut zeros = 0;
        while !bit()? {
            zeros += 1;
            if zeros > 13 {
                return Err(invalid("invalid GEM zero run"));
            }
        }
        let mut run = if zeros == 0 { 0 } else { 1 };
        for _ in 0..if zeros == 0 { 3 } else { zeros + 2 } {
            run = (run << 1) | usize::from(bit()?);
        }
        run += 1;
        let repeat = run == 65536;
        if repeat {
            run = 65535;
        }
        pos = pos
            .checked_add(run)
            .ok_or_else(|| invalid("GEM run overflow"))?;
        if pos >= total {
            break;
        }
        if repeat {
            continue;
        }
        loop {
            let more = bit()?;
            if pos > 0 && pos < total {
                let i = pos - 1;
                out[i / 8] |= 0x80 >> (i % 8);
            }
            pos += 1;
            if !more || pos >= total {
                break;
            }
        }
    }
    for y in 1..h.height as usize {
        for x in 0..h.stride {
            out[y * h.stride + x] ^= out[(y - 1) * h.stride + x];
        }
    }
    Ok(out)
}
/// Encode one strike; the report compares the model with the actual decoded output.
fn encode(
    s: &FontSet,
    w: &WriteOptions,
    o: &GemWriteOptions,
    report: &mut Report,
) -> Result<OutputBundle> {
    let source = s;
    let mut budget = source_budget(s, &o.limits)?;
    let converted = convert_gray(s, o.gray_threshold, &o.limits, &mut budget)?;
    let s = converted.as_ref();
    let f = s.single()?;
    let slots = select(f, 65535, o.unicode_as_legacy, "gem", report, &mut budget)?;
    let first = *slots
        .first_key_value()
        .ok_or_else(|| invalid("no GEM glyphs"))?
        .0;
    let last = *slots
        .last_key_value()
        .ok_or_else(|| invalid("no GEM glyphs"))?
        .0;
    let count = (last - first + 1) as usize;
    check(count, o.limits.max_glyphs, "GEM glyphs")?;
    let bottom = slots
        .values()
        .map(|g| -g.metrics.bounds.y)
        .max()
        .unwrap_or(0)
        .max(0);
    let ascent = slots
        .values()
        .map(|g| g.metrics.bounds.y + g.metrics.bounds.height as i64)
        .max()
        .unwrap_or(0)
        .max(0);
    let height = u16::try_from(ascent + bottom).map_err(|_| invalid("GEM height out of range"))?;
    if height == 0 {
        return Err(invalid("GEM requires positive height"));
    }
    let width: usize = slots
        .values()
        .map(|g| g.bitmap.width() as usize)
        .try_fold(0usize, |n, v| n.checked_add(v))
        .ok_or_else(|| invalid("GEM strip width overflow"))?;
    check(width, u16::MAX as usize, "GEM strip bit offset")?;
    let stride = width.div_ceil(16) * 2;
    let data = 88 + count * 2 + (count + 1) * 2;
    let length = data + stride * height as usize;
    check(
        length,
        o.max_output_bytes.min(o.limits.max_input_bytes),
        "GEM output bytes",
    )?;
    check(
        stride * height as usize,
        o.limits.max_table_bytes,
        "GEM bitmap strip",
    )?;
    check(
        (count + 1) * 2,
        o.limits.max_table_bytes,
        "GEM offset table",
    )?;
    budget.add(length)?;
    let mut out = vec![0; length];
    let e = o.byte_order;
    for (key, at) in KEYS {
        let default = match at {
            40 => ascent.saturating_sub(1),
            42 => ascent.saturating_sub(1),
            46 | 48 => bottom,
            44 => ascent / 2,
            50 | 52 => slots
                .values()
                .map(|g| g.bitmap.width() as i64)
                .max()
                .unwrap_or(0),
            58 | 60 => 1,
            62 | 64 => 0x5555,
            _ => 0,
        };
        let v = int(&f.properties, key, default)?;
        put16(
            &mut out,
            at,
            u16::try_from(v).map_err(|_| invalid(format!("{key} out of range")))?,
            e,
        );
    }
    put16(&mut out, 48, bottom as u16, e);
    let point = f.metrics.point_size.map_or(i64::from(height), |v| {
        v.numerator() / i64::from(v.denominator())
    });
    put16(
        &mut out,
        2,
        i16::try_from(point).map_err(|_| invalid("GEM point size out of range"))? as u16,
        e,
    );
    let name: Vec<u8> = (f
        .names
        .full
        .as_deref()
        .or(f.names.family.as_deref())
        .unwrap_or(""))
    .chars()
    .take(32)
    .map(|c| u8::try_from(u32::from(c)).unwrap_or(b'?'))
    .collect();
    let take = name.len().min(32);
    out[4..4 + take].copy_from_slice(&name[..take]);
    put16(&mut out, 36, first as u16, e);
    put16(&mut out, 38, last as u16, e);
    let mut flags = u16::try_from(int(&f.properties, "gem.flags", 0)?)
        .map_err(|_| invalid("GEM flags out of range"))?
        & !(2 | 4 | 8 | 32);
    flags |= 2;
    if o.bitmap_word_order == ByteOrder::Big {
        flags |= 4;
    }
    if f.metrics.spacing == Some(Spacing::Monospaced) {
        flags |= 8;
    }
    put16(&mut out, 66, flags, e);
    put32(&mut out, 68, 88, e);
    put32(&mut out, 72, (88 + count * 2) as u32, e);
    put32(&mut out, 76, data as u32, e);
    put16(&mut out, 80, stride as u16, e);
    put16(&mut out, 82, height, e);
    put32(
        &mut out,
        84,
        u32::try_from(int(&f.properties, "gem.next_font", 0)?)
            .map_err(|_| invalid("GEM next pointer out of range"))?,
        e,
    );
    let mut x = 0usize;
    for i in 0..count {
        put16(&mut out, 88 + count * 2 + i * 2, x as u16, e);
        if let Some(g) = slots.get(&(first + i as u32)) {
            let b = mono(g)?;
            let left = g.metrics.bounds.x;
            let right = g
                .metrics
                .advance
                .or(f.metrics.glyph_defaults.advance)
                .map_or(i64::from(b.width()), |v| v.x)
                - left
                - i64::from(b.width());
            out[88 + i * 2] =
                i8::try_from(-left).map_err(|_| invalid("GEM left bearing out of range"))? as u8;
            out[89 + i * 2] =
                i8::try_from(-right).map_err(|_| invalid("GEM right bearing out of range"))? as u8;
            let top = usize::try_from(ascent - g.metrics.bounds.y - i64::from(b.height()))
                .map_err(|_| invalid("GEM glyph outside cell"))?;
            insert(&mut out[data..], stride, x, top, b);
            x += b.width() as usize;
        }
    }
    put16(&mut out, 88 + count * 2 + count * 2, x as u16, e);
    if o.bitmap_word_order == ByteOrder::Little {
        for word in out[data..].as_chunks_mut::<2>().0 {
            word.swap(0, 1);
        }
    }
    let (decoded, _) = parse(
        &out,
        &GemReadOptions {
            byte_order: Some(e),
            bitmap_word_order: Some(o.bitmap_word_order),
        },
        &o.limits,
        &mut budget,
    )?;
    audit(source, &decoded, "gem", report, &mut budget)?;
    Ok(OutputBundle::single(w.primary_name.clone(), out))
}

/// Encode and enforce strict mode before returning any output assets.
pub fn write(s: &FontSet, w: &WriteOptions, o: &GemWriteOptions) -> Result<Written> {
    let mut report = Report::default();
    let result = encode(s, w, o, &mut report);
    finish(result, report, "gem", w.strict)
}
