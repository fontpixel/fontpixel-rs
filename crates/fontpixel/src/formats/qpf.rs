//! Qt/Embedded legacy QPF packed metrics and three-pass glyph trees.
use super::{
    Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions,
    w5d_common::*,
};
use crate::bitmap::Bounds;
use crate::model::{
    Codepoint, Font, FontCharset, FontMetrics, Glyph, GlyphBitmap, Property, PropertyValue, Vector,
};
use crate::resources::{OutputBundle, ReadContext, Written};
use crate::{Bitmap, FontSet, Report, ResourceLimits, Result};
use std::collections::BTreeMap;

/// Metadata explicitly supplied from outside the headerless QPF record.
#[derive(Debug, Clone, Default)]
pub struct QpfReadOptions {
    /// Family name, conventionally carried in a QPF filename rather than bytes.
    pub family: Option<String>,
}
/// Native packed QPF output options.
#[derive(Debug, Clone)]
pub struct QpfWriteOptions {
    /// Explicit Gray8-to-Mono conversion; absence preserves native Gray8.
    pub gray_threshold: Option<u8>,
    /// Cumulative input/model/output/reread storage limits.
    pub limits: ResourceLimits,
    /// Maximum output bytes.
    pub max_output_bytes: usize,
}
impl Default for QpfWriteOptions {
    fn default() -> Self {
        Self {
            gray_threshold: None,
            limits: ResourceLimits::default(),
            max_output_bytes: 64 << 20,
        }
    }
}
/// Registry adapter for the packed legacy QPF format.
#[derive(Debug, Clone, Default)]
pub struct QpfCodec {
    /// Explicit external input metadata.
    pub read_options: QpfReadOptions,
    /// Output policy.
    pub write_options: QpfWriteOptions,
}
impl FormatDetector for QpfCodec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(detect(c.bytes()))
    }
}
impl FormatReader<FontSet> for QpfCodec {
    fn read(&self, c: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
        read(c.bytes(), &self.read_options, c.limits())
    }
}
impl FormatWriter<FontSet> for QpfCodec {
    fn write(&self, s: &FontSet, o: &WriteOptions) -> Result<Written> {
        let (bytes, report) = write(s, &self.write_options)?;
        Ok(Written {
            output: OutputBundle::single(o.primary_name.clone(), bytes),
            report,
        })
    }
}
struct Node {
    first: u16,
    count: usize,
}
fn tree(data: &[u8], b: &mut Budget<'_>) -> Result<(Vec<Node>, usize, usize)> {
    slice(data, 0, 10)?;
    let mut pending = vec![(0u32, 65535u32)];
    b.add(128)?;
    let mut nodes = Vec::new();
    let mut at = 10usize;
    let mut count = 0usize;
    while let Some((low, high)) = pending.pop() {
        check(nodes.len() + 1, b.limits.max_resources, "QPF tree nodes")?;
        let node = slice(data, at, 5)?;
        at += 5;
        let first = u16be(node, 0)?;
        let last = u16be(node, 2)?;
        if first > last || u32::from(first) < low || u32::from(last) > high {
            return Err(invalid("QPF unordered or overlapping tree range"));
        }
        let n = usize::from(last - first) + 1;
        count = count
            .checked_add(n)
            .ok_or_else(|| invalid("QPF glyph count overflow"))?;
        check(count, b.limits.max_glyphs, "QPF glyph count")?;
        b.add(128)?;
        nodes.push(Node { first, count: n });
        if node[4] & 2 != 0 {
            if u32::from(last) == high {
                return Err(invalid("QPF right child has no character range"));
            }
            pending.push((u32::from(last) + 1, high));
        }
        if node[4] & 1 != 0 {
            if u32::from(first) == low {
                return Err(invalid("QPF left child has no character range"));
            }
            pending.push((low, u32::from(first) - 1));
        }
    }
    slice(
        data,
        at,
        count
            .checked_mul(8)
            .ok_or_else(|| invalid("QPF metrics length overflow"))?,
    )?;
    Ok((nodes, at, count))
}
/// Bounded structural recognition of a magic-free packed QPF record.
pub fn detect(data: &[u8]) -> Confidence {
    let l = ResourceLimits {
        max_input_bytes: 1 << 20,
        max_total_bytes: 1 << 20,
        max_glyphs: 65536,
        max_resources: 1024,
        ..Default::default()
    };
    if data.len() > l.max_input_bytes {
        return Confidence::NoMatch;
    }
    let mut b = Budget::new(&l);
    let Ok((_, at, count)) = tree(data, &mut b) else {
        return Confidence::NoMatch;
    };
    let gray = data[6] & 1 != 0;
    let mut end = at + count * 8;
    for n in 0..count {
        let m = &data[at + n * 8..at + (n + 1) * 8];
        let minimum = if gray {
            usize::from(m[1])
        } else {
            usize::from(m[1]).div_ceil(8)
        };
        if usize::from(m[0]) < minimum {
            return Confidence::NoMatch;
        }
        end += usize::from(m[0]) * usize::from(m[2]);
        if end > data.len() {
            return Confidence::NoMatch;
        }
    }
    if end == data.len() {
        Confidence::Possible
    } else {
        Confidence::NoMatch
    }
}
/// Decode an explicitly packed, platform-independent QPF record.
pub fn read(data: &[u8], o: &QpfReadOptions, l: &ResourceLimits) -> Result<(FontSet, Report)> {
    check(data.len(), l.max_input_bytes, "QPF input bytes")?;
    let mut b = Budget::new(l);
    b.add(data.len())?;
    Ok((decode(data, o, &mut b)?, Report::default()))
}
fn decode(data: &[u8], o: &QpfReadOptions, b: &mut Budget<'_>) -> Result<FontSet> {
    let (nodes, metrics, count) = tree(data, b)?;
    b.table(data.len())?;
    b.add(4096)?;
    if let Some(family) = &o.family {
        b.table(family.len())?;
    }
    let mut font = Font::default();
    font.source_format = Some("qpf".into());
    font.names.family = o.family.clone();
    font.charset = FontCharset {
        name: Some("unicode".into()),
        ..Default::default()
    };
    let ascent = i64::from(data[0] as i8);
    let descent = i64::from(data[1] as i8);
    font.metrics = FontMetrics {
        ascent: Some(ascent),
        descent: Some(descent),
        line_gap: Some(i64::from(data[5] as i8)),
        pixel_size: u32::try_from(ascent + descent).ok(),
        underline_position: Some(-i64::from(data[7])),
        underline_thickness: Some(i64::from(data[8])),
        ..Default::default()
    };
    for (name, n) in [
        ("qpf.left-bearing", i64::from(data[2] as i8)),
        ("qpf.right-bearing", i64::from(data[3] as i8)),
        ("qpf.max-width", i64::from(data[4])),
        ("qpf.flags", i64::from(data[6])),
        ("qpf.reserved", i64::from(data[9])),
    ] {
        font.properties.push(Property::new(name, n));
    }
    font.properties.push(Property::new(
        "qpf.source",
        PropertyValue::Bytes(data.to_vec()),
    ));
    let gray = data[6] & 1 != 0;
    let mut metric = metrics;
    let mut raster = metrics + count * 8;
    let mut bounds: Option<Bounds> = None;
    for node in nodes {
        for index in 0..node.count {
            let m = slice(data, metric, 8)?;
            metric += 8;
            let width = usize::from(m[1]);
            let height = usize::from(m[2]);
            let row = usize::from(m[0]);
            let stride = if gray { width } else { width.div_ceil(8) };
            if row < stride {
                return Err(invalid("QPF row stride clips the glyph"));
            }
            let raw = slice(data, raster, row * height)?;
            raster += row * height;
            b.glyph(width, height)?;
            if gray {
                b.add((width - width.div_ceil(8)) * height)?;
            }
            b.table(stride * height)?;
            let mut packed = Vec::with_capacity(stride * height);
            for y in 0..height {
                packed.extend_from_slice(&raw[y * row..y * row + stride]);
            }
            let bitmap: GlyphBitmap = if gray {
                Bitmap::<crate::bitmap::Gray8>::from_raw(
                    width as u32,
                    height as u32,
                    &packed,
                    bitmap_limits(b.limits),
                )?
                .into()
            } else {
                Bitmap::<crate::bitmap::Mono>::from_raw(
                    width as u32,
                    height as u32,
                    &packed,
                    bitmap_limits(b.limits),
                )?
                .into()
            };
            let mut g = Glyph::new(bitmap);
            let cp = u32::from(node.first) + index as u32;
            g.codepoints.push(if char::from_u32(cp).is_some() {
                Codepoint::Unicode(cp)
            } else {
                Codepoint::Legacy(cp)
            });
            g.metrics.bounds.x = i64::from(m[4] as i8);
            g.metrics.bounds.y = i64::from(m[6] as i8) - height as i64;
            g.metrics.advance = Some(Vector::new(i64::from(m[5]), 0));
            for (name, v) in [("qpf.flags", m[3]), ("qpf.reserved", m[7])] {
                if v != 0 {
                    b.add(256)?;
                    g.properties.push(Property::new(name, i64::from(v)));
                }
            }
            bounds = Some(if let Some(rect) = bounds {
                union(rect, g.metrics.bounds)
            } else {
                g.metrics.bounds
            });
            font.add_glyph(g)?;
        }
    }
    font.metrics.bounds = bounds;
    let mut set = FontSet::from(font);
    set.source_format = Some("qpf".into());
    Ok(set)
}
fn byte(n: i64) -> Result<u8> {
    u8::try_from(n).map_err(|_| invalid("QPF unsigned metric exceeds one byte"))
}
fn signed(n: i64) -> Result<u8> {
    i8::try_from(n)
        .map(|v| v as u8)
        .map_err(|_| invalid("QPF signed metric exceeds one byte"))
}
fn prop(p: &crate::model::Properties, key: &str) -> Result<Option<i64>> {
    match p.get(key) {
        None => Ok(None),
        Some(PropertyValue::Integer(v)) => Ok(Some(*v)),
        _ => Err(invalid(format!("QPF {key} must be an integer"))),
    }
}
/// Encode a balanced native tree and validate the actual default reread model.
pub fn write(s: &FontSet, o: &QpfWriteOptions) -> Result<(Vec<u8>, Report)> {
    let mut b = Budget::new(&o.limits);
    b.model(s)?;
    let f = s.single()?;
    let mut replay = None;
    if let Some(raw) = bytes(&f.properties, "qpf.source") {
        let original = decode(
            raw,
            &QpfReadOptions {
                family: f.names.family.clone(),
            },
            &mut b,
        )?;
        if original.single()? == f && !(o.gray_threshold.is_some() && raw[6] & 1 != 0) {
            b.table(raw.len())?;
            replay = Some(raw.to_vec());
        }
    }
    let out = if let Some(raw) = replay {
        raw
    } else {
        encode(f, o, &mut b)?
    };
    check(
        out.len(),
        o.max_output_bytes.min(o.limits.max_input_bytes),
        "QPF output bytes",
    )?;
    let decoded = decode(&out, &QpfReadOptions::default(), &mut b)?;
    let mut r = Report::default();
    compare(s, &decoded, "qpf", &mut b, &mut r)?;
    Ok((out, r))
}
fn encode(f: &Font, o: &QpfWriteOptions, b: &mut Budget<'_>) -> Result<Vec<u8>> {
    let mut mapping = BTreeMap::new();
    for (_, g) in f.glyphs() {
        for cp in &g.codepoints {
            if cp.value() <= 65535 {
                b.add(128)?;
                mapping.insert(cp.value() as u16, g);
            }
        }
    }
    if mapping.is_empty() {
        return Err(invalid("QPF needs at least one 16-bit mapping"));
    }
    check(mapping.len(), o.limits.max_glyphs, "QPF output glyph count")?;
    let gray = o.gray_threshold.is_none()
        && mapping
            .values()
            .any(|g| matches!(g.bitmap, GlyphBitmap::Gray8(_)));
    let mut ranges: Vec<(u16, Vec<&Glyph>)> = Vec::new();
    let mut maximum = 0i64;
    let mut top = i64::MIN;
    let mut bottom = i64::MAX;
    let mut minleft = i64::MAX;
    let mut minright = i64::MAX;
    let mut size = 10usize;
    for (&cp, &g) in &mapping {
        let width = g.bitmap.width() as usize;
        let height = g.bitmap.height() as usize;
        byte(width as i64)?;
        byte(height as i64)?;
        signed(g.metrics.bounds.x)?;
        let glyph_top = g
            .metrics
            .bounds
            .y
            .checked_add(height as i64)
            .ok_or_else(|| invalid("QPF glyph top overflow"))?;
        signed(glyph_top)?;
        top = top.max(glyph_top);
        bottom = bottom.min(g.metrics.bounds.y);
        minleft = minleft.min(g.metrics.bounds.x);
        let advance = g
            .metrics
            .advance
            .or(f.metrics.glyph_defaults.advance)
            .map_or(width as i64, |v| v.x);
        byte(advance)?;
        maximum = maximum.max(advance);
        minright = minright.min(advance - g.metrics.bounds.x - width as i64);
        let stride = if gray { width } else { width.div_ceil(8) };
        byte(stride as i64)?;
        size = size
            .checked_add(8 + stride * height)
            .ok_or_else(|| invalid("QPF output size overflow"))?;
        b.add(32)?;
        if let Some((start, glyphs)) = ranges.last_mut()
            && u32::from(*start) + glyphs.len() as u32 == u32::from(cp)
        {
            glyphs.push(g);
        } else {
            b.add(64)?;
            ranges.push((cp, vec![g]));
        }
    }
    check(
        ranges.len(),
        o.limits.max_resources,
        "QPF output tree nodes",
    )?;
    size = size
        .checked_add(ranges.len() * 5)
        .ok_or_else(|| invalid("QPF tree size overflow"))?;
    check(
        size,
        o.max_output_bytes.min(o.limits.max_input_bytes),
        "QPF output bytes",
    )?;
    b.table(size)?;
    b.add(ranges.len() * 64)?;
    let mut order = Vec::with_capacity(ranges.len());
    let mut pending = vec![(0usize, ranges.len())];
    let mut out = Vec::with_capacity(size);
    out.extend_from_slice(&[
        signed(f.metrics.ascent.unwrap_or(top))?,
        signed(f.metrics.descent.unwrap_or(-bottom))?,
        signed(prop(&f.properties, "qpf.left-bearing")?.unwrap_or(minleft))?,
        signed(prop(&f.properties, "qpf.right-bearing")?.unwrap_or(minright))?,
        byte(prop(&f.properties, "qpf.max-width")?.unwrap_or(maximum))?,
        signed(f.metrics.line_gap.unwrap_or(0))?,
        (byte(prop(&f.properties, "qpf.flags")?.unwrap_or(0))? & !1) | u8::from(gray),
        byte(
            f.metrics
                .underline_position
                .unwrap_or(0)
                .checked_neg()
                .ok_or_else(|| invalid("QPF underline overflow"))?,
        )?,
        byte(f.metrics.underline_thickness.unwrap_or(0))?,
        byte(prop(&f.properties, "qpf.reserved")?.unwrap_or(0))?,
    ]);
    while let Some((lo, hi)) = pending.pop() {
        let middle = lo + (hi - lo) / 2;
        let (start, glyphs) = &ranges[middle];
        out.extend_from_slice(&start.to_be_bytes());
        out.extend_from_slice(&(*start as u32 + glyphs.len() as u32 - 1).to_be_bytes()[2..]);
        out.push(u8::from(lo < middle) | (u8::from(middle + 1 < hi) * 2));
        order.push(middle);
        if middle + 1 < hi {
            pending.push((middle + 1, hi));
        }
        if lo < middle {
            pending.push((lo, middle));
        }
    }
    for &range in &order {
        for &g in &ranges[range].1 {
            let width = g.bitmap.width();
            let height = g.bitmap.height();
            let advance = g
                .metrics
                .advance
                .or(f.metrics.glyph_defaults.advance)
                .map_or(i64::from(width), |v| v.x);
            out.extend_from_slice(&[
                if gray {
                    width as u8
                } else {
                    width.div_ceil(8) as u8
                },
                width as u8,
                height as u8,
                byte(prop(&g.properties, "qpf.flags")?.unwrap_or(0))?,
                signed(g.metrics.bounds.x)?,
                byte(advance)?,
                signed(g.metrics.bounds.y + i64::from(height))?,
                byte(prop(&g.properties, "qpf.reserved")?.unwrap_or(0))?,
            ]);
        }
    }
    for &range in &order {
        for &g in &ranges[range].1 {
            match (&g.bitmap, gray) {
                (GlyphBitmap::Mono(m), false) => out.extend_from_slice(m.as_bytes()),
                (GlyphBitmap::Gray8(m), true) => out.extend_from_slice(m.as_bytes()),
                (GlyphBitmap::Mono(m), true) => {
                    for y in 0..m.height() as usize {
                        for x in 0..m.width() as usize {
                            out.push(
                                if m.as_bytes()[y * m.stride() + x / 8] & (0x80 >> (x % 8)) != 0 {
                                    255
                                } else {
                                    0
                                },
                            );
                        }
                    }
                }
                (GlyphBitmap::Gray8(m), false) => {
                    for y in 0..m.height() as usize {
                        for chunk in m.as_bytes()[y * m.stride()..(y + 1) * m.stride()].chunks(8) {
                            let mut v = 0;
                            for (bit, &pixel) in chunk.iter().enumerate() {
                                if pixel >= o.gray_threshold.expect("explicit reduction") {
                                    v |= 0x80 >> bit;
                                }
                            }
                            out.push(v);
                        }
                    }
                }
            }
        }
    }
    debug_assert_eq!(out.len(), size);
    Ok(out)
}

fn union(a: Bounds, b: Bounds) -> Bounds {
    // Both rectangles came from byte-sized native geometry.
    let left = a.x.min(b.x);
    let bottom = a.y.min(b.y);
    let right = (a.x + i64::from(a.width)).max(b.x + i64::from(b.width));
    let top = (a.y + i64::from(a.height)).max(b.y + i64::from(b.height));
    Bounds::new((right - left) as u32, (top - bottom) as u32, left, bottom)
}
