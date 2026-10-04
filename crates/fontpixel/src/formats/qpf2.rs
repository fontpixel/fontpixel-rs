//! Qt QPF2 tagged headers, sfnt cmap, native glyph maps and cached rasters.
use super::{
    Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions,
    w5d_common::*,
};
use crate::bitmap::{Bounds, Gray8, Mono};
use crate::model::{
    Codepoint, Font, FontCharset, Glyph, GlyphBitmap, Property, PropertyValue, Vector,
};
use crate::resources::{OutputBundle, ReadContext, Written};
use crate::{Bitmap, FontSet, Report, ResourceLimits, Result};
use std::collections::BTreeMap;
/// QPF2 never implicitly opens a linked outline font.
#[derive(Debug, Clone, Default)]
pub struct Qpf2ReadOptions {}
/// Raster representation declared by the QPF2 header.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GlyphFormat {
    /// Byte alpha samples, consumed by Qt 4's QPF2 engine.
    #[default]
    Gray8,
    /// Packed monochrome as declared by the wire enum; Qt 4's alpha renderer
    /// ignores this tag and must not be used to display this representation.
    Mono,
}
/// Native QPF2 output policy.
#[derive(Debug, Clone)]
pub struct Qpf2WriteOptions {
    /// Native raster layout for newly encoded or edited data.
    pub glyph_format: GlyphFormat,
    /// Required to reduce Gray8 when requesting packed Mono output.
    pub gray_threshold: Option<u8>,
    /// Cumulative input/model/temporary/output/reread limits.
    pub limits: ResourceLimits,
    /// Maximum output bytes.
    pub max_output_bytes: usize,
}
impl Default for Qpf2WriteOptions {
    fn default() -> Self {
        Self {
            glyph_format: GlyphFormat::Gray8,
            gray_threshold: None,
            limits: ResourceLimits::default(),
            max_output_bytes: 64 << 20,
        }
    }
}
/// Registry adapter with explicit native options.
#[derive(Debug, Clone, Default)]
pub struct Qpf2Codec {
    /// Input policy.
    pub read_options: Qpf2ReadOptions,
    /// Output policy.
    pub write_options: Qpf2WriteOptions,
}
impl FormatDetector for Qpf2Codec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(detect(c.bytes()))
    }
}
impl FormatReader<FontSet> for Qpf2Codec {
    fn read(&self, c: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
        read(c.bytes(), &self.read_options, c.limits())
    }
}
impl FormatWriter<FontSet> for Qpf2Codec {
    fn write(&self, s: &FontSet, o: &WriteOptions) -> Result<Written> {
        let (bytes, report) = write(s, &self.write_options)?;
        Ok(Written {
            output: OutputBundle::single(o.primary_name.clone(), bytes),
            report,
        })
    }
}
/// Recognize the explicit QPF2 signature and complete tagged-header boundary.
pub fn detect(data: &[u8]) -> Confidence {
    if data.starts_with(b"QPF2")
        && data.get(8..10) == Some(&[2, 0])
        && u16be(data, 10).is_ok_and(|n| 12 + usize::from(n) <= data.len())
    {
        Confidence::Certain
    } else {
        Confidence::NoMatch
    }
}
fn tags(data: &[u8], b: &mut Budget<'_>) -> Result<Vec<(u16, Vec<u8>)>> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < data.len() {
        let tag = u16be(data, at)?;
        let n = usize::from(u16be(data, at + 2)?);
        at += 4;
        if tag == 19 {
            return Ok(out);
        }
        let value = slice(data, at, n)?;
        at += n;
        if (matches!(tag, 2 | 3 | 5..=14) && n != 4) || (matches!(tag, 15..=18) && n != 1) {
            return Err(invalid("QPF2 invalid native tag size"));
        }
        check(out.len() + 1, b.limits.max_resources, "QPF2 header tags")?;
        b.add(n + 64)?;
        out.push((tag, value.to_vec()));
    }
    Err(invalid("QPF2 missing end-of-header tag"))
}
fn field(tags: &[(u16, Vec<u8>)], tag: u16) -> Option<&[u8]> {
    tags.iter()
        .find(|(t, _)| *t == tag)
        .map(|(_, v)| v.as_slice())
}
fn fixed(tags: &[(u16, Vec<u8>)], tag: u16) -> Option<i64> {
    field(tags, tag).map(|v| i64::from(i32::from_be_bytes([v[0], v[1], v[2], v[3]])) / 64)
}
fn cmap_limits(b: &Budget<'_>) -> crate::sfnt::Limits {
    crate::sfnt::Limits {
        resources: b.limits.clone(),
        max_cmap_mappings: 0x110000.min(b.limits.max_total_bytes.saturating_sub(b.used) / 256),
        ..Default::default()
    }
}
/// Decode cached glyphs, reporting fractional-metric projection and unresolved linkage.
pub fn read(
    data: &[u8],
    _: &Qpf2ReadOptions,
    limits: &ResourceLimits,
) -> Result<(FontSet, Report)> {
    check(data.len(), limits.max_input_bytes, "QPF2 input bytes")?;
    let mut b = Budget::new(limits);
    b.add(data.len())?;
    decode(data, &mut b)
}
fn decode(data: &[u8], b: &mut Budget<'_>) -> Result<(FontSet, Report)> {
    if detect(data) == Confidence::NoMatch {
        return Err(invalid("QPF2 invalid header or unsupported version"));
    }
    b.add(4096)?;
    b.table(data.len())?;
    let hlen = usize::from(u16be(data, 10)?);
    let header = slice(data, 12, hlen)?;
    let tagged = tags(header, b)?;
    let format = match field(&tagged, 15) {
        Some([1]) => GlyphFormat::Mono,
        Some([8]) => GlyphFormat::Gray8,
        _ => return Err(invalid("QPF2 unsupported or absent glyph format")),
    };
    let mut blocks = BTreeMap::new();
    let mut at = 12 + hlen;
    while at < data.len() {
        check(blocks.len() + 1, b.limits.max_resources, "QPF2 blocks")?;
        let tag = u16be(data, at)?;
        let n = u32be(data, at + 4)? as usize;
        let value = slice(data, at + 8, n)?;
        check(n, b.limits.max_table_bytes, "QPF2 block bytes")?;
        b.add(128)?;
        if blocks.insert(tag, value).is_some() {
            return Err(invalid("QPF2 duplicate block tag"));
        }
        at = at
            .checked_add(8 + n)
            .ok_or_else(|| invalid("QPF2 block offset overflow"))?;
    }
    let gmap = *blocks
        .get(&1)
        .ok_or_else(|| invalid("QPF2 GMap block missing"))?;
    let raster = *blocks
        .get(&2)
        .ok_or_else(|| invalid("QPF2 Glyph block missing"))?;
    if gmap.len() % 4 != 0 {
        return Err(invalid("QPF2 GMap size is not a word multiple"));
    }
    let count = gmap.len() / 4;
    if count == 0 || count > 65535 {
        return Err(invalid("QPF2 glyph count outside 1..65535"));
    }
    check(count, b.limits.max_glyphs, "QPF2 glyph count")?;
    let mut report = Report::default();
    let mapping = if let Some(cmap) = blocks.get(&0) {
        crate::sfnt::cmap::decode(cmap, count as u16, &cmap_limits(b))?
    } else {
        b.loss(
            &mut report,
            "qpf2.external_cmap",
            "QPF2 has no embedded cmap; external outline lookup is unresolved",
        )?;
        BTreeMap::new()
    };
    b.add(mapping.len() * 256 + count * 32)?;
    let mut aliases = vec![Vec::new(); count];
    for (cp, gid) in mapping {
        aliases[usize::from(gid)].push(Codepoint::Unicode(cp));
    }
    let mut f = Font::default();
    f.source_format = Some("qpf2".into());
    f.charset = FontCharset {
        name: Some("unicode".into()),
        ..Default::default()
    };
    if let Some(name) = field(&tagged, 0) {
        b.table(name.len())?;
        f.names.family = Some(
            std::str::from_utf8(name)
                .map_err(|_| invalid("QPF2 invalid UTF-8 family"))?
                .into(),
        );
    }
    f.metrics.ascent = fixed(&tagged, 5);
    f.metrics.descent = fixed(&tagged, 6);
    f.metrics.line_gap = fixed(&tagged, 7);
    f.metrics.underline_thickness = fixed(&tagged, 11);
    f.metrics.underline_position = fixed(&tagged, 14).map(|v| -v);
    f.metrics.pixel_size = field(&tagged, 16).map(|v| u32::from(v[0]));
    for tag in [5, 6, 7, 11, 14] {
        if let Some(v) = field(&tagged, tag)
            && u32be(v, 0)? & 63 != 0
        {
            b.loss(&mut report,"qpf2.fractional_metric","Native 26.6 metric is retained in header; integer model metric truncates toward zero")?;
        }
    }
    if field(&tagged, 1).is_some_and(|v| !v.is_empty()) {
        b.loss(&mut report,"qpf2.linked_outline","Linked outline font metadata is retained; external shaping and uncached glyphs are unresolved")?;
    }
    b.table(header.len())?;
    f.properties.push(Property::new(
        "qpf2.header",
        PropertyValue::Bytes(header.to_vec()),
    ));
    f.properties.push(Property::new(
        "qpf2.source",
        PropertyValue::Bytes(data.to_vec()),
    ));
    let mut bounds: Option<Bounds> = None;
    for (gid, codes) in aliases.into_iter().enumerate() {
        let offset = u32be(gmap, gid * 4)?;
        if offset == u32::MAX {
            b.add(1024)?;
            let id = f.add_glyph(Glyph::default())?;
            f.remove_glyph(id);
            if !codes.is_empty() {
                b.loss(
                    &mut report,
                    "qpf2.uncached_glyph",
                    "Mapped glyph has no cached raster; mapping remains in native source only",
                )?;
            }
            continue;
        }
        let m = slice(raster, offset as usize, 6)?;
        let (w, h, row) = (usize::from(m[0]), usize::from(m[1]), usize::from(m[2]));
        let stride = if format == GlyphFormat::Gray8 {
            w
        } else {
            w.div_ceil(8)
        };
        if row < stride {
            return Err(invalid("QPF2 stride clips glyph"));
        }
        let raw = slice(raster, offset as usize + 6, row * h)?;
        b.glyph(w, h)?;
        if format == GlyphFormat::Gray8 {
            b.add((w - w.div_ceil(8)) * h)?;
        }
        b.table(stride * h)?;
        let mut packed = Vec::with_capacity(stride * h);
        for y in 0..h {
            packed.extend_from_slice(&raw[y * row..y * row + stride]);
        }
        let bitmap: GlyphBitmap = if format == GlyphFormat::Gray8 {
            Bitmap::<Gray8>::from_raw(w as u32, h as u32, &packed, bitmap_limits(b.limits))?.into()
        } else {
            Bitmap::<Mono>::from_raw(w as u32, h as u32, &packed, bitmap_limits(b.limits))?.into()
        };
        let mut g = Glyph::new(bitmap);
        g.codepoints = codes;
        g.metrics.bounds.x = i64::from(m[3] as i8);
        g.metrics.bounds.y = -i64::from(m[4] as i8) - h as i64;
        g.metrics.advance = Some(Vector::new(i64::from(m[5] as i8), 0));
        bounds = Some(if let Some(a) = bounds {
            let z = g.metrics.bounds;
            let x = a.x.min(z.x);
            let y = a.y.min(z.y);
            Bounds::new(
                ((a.x + i64::from(a.width)).max(z.x + i64::from(z.width)) - x) as u32,
                ((a.y + i64::from(a.height)).max(z.y + i64::from(z.height)) - y) as u32,
                x,
                y,
            )
        } else {
            g.metrics.bounds
        });
        f.add_glyph(g)?;
    }
    f.metrics.bounds = bounds;
    let mut set = FontSet::from(f);
    set.source_format = Some("qpf2".into());
    Ok((set, report))
}
fn ubyte(v: i64) -> Result<u8> {
    u8::try_from(v).map_err(|_| invalid("QPF2 unsigned metric outside byte"))
}
fn sbyte(v: i64) -> Result<u8> {
    i8::try_from(v)
        .map(|v| v as u8)
        .map_err(|_| invalid("QPF2 signed metric outside byte"))
}
/// Write read-only native QPF2 and audit its actual default reread.
pub fn write(set: &FontSet, o: &Qpf2WriteOptions) -> Result<(Vec<u8>, Report)> {
    let mut b = Budget::new(&o.limits);
    b.model(set)?;
    let f = set.single()?;
    let out = if let Some(raw) = bytes(&f.properties, "qpf2.source") {
        let (original, _) = decode(raw, &mut b)?;
        let native_tags = tags(slice(raw, 12, usize::from(u16be(raw, 10)?))?, &mut b)?;
        let requested = if o.glyph_format == GlyphFormat::Gray8 {
            8
        } else {
            1
        };
        if original.single()? == f
            && o.gray_threshold.is_none()
            && field(&native_tags, 15) == Some(&[requested])
        {
            b.table(raw.len())?;
            raw.to_vec()
        } else {
            encode(f, o, &mut b)?
        }
    } else {
        encode(f, o, &mut b)?
    };
    check(
        out.len(),
        o.max_output_bytes.min(o.limits.max_input_bytes),
        "QPF2 output bytes",
    )?;
    let (decoded, mut r) = decode(&out, &mut b)?;
    compare(set, &decoded, "qpf2", &mut b, &mut r)?;
    Ok((out, r))
}
fn tag(out: &mut Vec<u8>, kind: u16, value: &[u8]) -> Result<()> {
    let n = u16::try_from(value.len()).map_err(|_| invalid("QPF2 tag exceeds u16 length"))?;
    out.extend_from_slice(&kind.to_be_bytes());
    out.extend_from_slice(&n.to_be_bytes());
    out.extend_from_slice(value);
    Ok(())
}
fn set_tag(tags: &mut Vec<(u16, Vec<u8>)>, kind: u16, value: Vec<u8>) {
    if let Some((_, v)) = tags.iter_mut().find(|(k, _)| *k == kind) {
        *v = value;
    } else {
        tags.push((kind, value));
    }
}
fn encode(f: &Font, o: &Qpf2WriteOptions, b: &mut Budget<'_>) -> Result<Vec<u8>> {
    if f.is_empty() {
        return Err(invalid("QPF2 needs cached glyphs"));
    }
    let reserve_zero = f
        .glyphs()
        .next()
        .is_some_and(|(id, g)| id.index() == 0 && !g.codepoints.is_empty());
    let count = f
        .slot_count()
        .checked_add(usize::from(reserve_zero))
        .ok_or_else(|| invalid("QPF2 glyph count overflow"))?;
    if count > 65535 {
        return Err(invalid("QPF2 glyph count exceeds u16"));
    }
    check(count, b.limits.max_glyphs, "QPF2 glyph count")?;
    b.add(count * 128)?;
    let mut mapping = BTreeMap::new();
    let mut glyphs = vec![None; count];
    let mut raster_len = 0usize;
    let gray = o.glyph_format == GlyphFormat::Gray8;
    for (id, g) in f.glyphs() {
        let gid = id.index() + usize::from(reserve_zero);
        glyphs[gid] = Some(g);
        for cp in &g.codepoints {
            if let Codepoint::Unicode(v) = *cp {
                if gid == 0 {
                    return Err(invalid("QPF2 glyph zero cannot have Unicode mappings"));
                }
                b.add(256)?;
                mapping.insert(v, gid as u16);
            }
        }
        let w = i64::from(g.bitmap.width());
        let h = i64::from(g.bitmap.height());
        ubyte(w)?;
        ubyte(h)?;
        sbyte(g.metrics.bounds.x)?;
        let top = g
            .metrics
            .bounds
            .y
            .checked_add(h)
            .and_then(|n| n.checked_neg())
            .ok_or_else(|| invalid("QPF2 top overflow"))?;
        sbyte(top)?;
        sbyte(
            g.metrics
                .advance
                .or(f.metrics.glyph_defaults.advance)
                .map_or(w, |v| v.x),
        )?;
        if !gray && matches!(g.bitmap, GlyphBitmap::Gray8(_)) && o.gray_threshold.is_none() {
            return Err(invalid("QPF2 Gray8 reduction requires explicit threshold"));
        }
        raster_len = raster_len
            .checked_add(
                6 + (if gray {
                    w as usize
                } else {
                    (w as usize).div_ceil(8)
                }) * h as usize,
            )
            .ok_or_else(|| invalid("QPF2 raster size overflow"))?;
    }
    let cmap = crate::sfnt::cmap::encode(&mapping, count as u16, &cmap_limits(b))?.data;
    b.table(cmap.len())?;
    let mut ts = if let Some(h) = bytes(&f.properties, "qpf2.header") {
        tags(h, b)?
    } else {
        Vec::new()
    };
    if let Some(name) = &f.names.family {
        b.table(name.len())?;
        set_tag(&mut ts, 0, name.as_bytes().to_vec());
    }
    for (kind, v) in [
        (5, f.metrics.ascent),
        (6, f.metrics.descent),
        (7, f.metrics.line_gap),
        (11, f.metrics.underline_thickness),
        (
            14,
            f.metrics.underline_position.and_then(|n| n.checked_neg()),
        ),
    ] {
        if let Some(v) = v
            && fixed(&ts, kind) != Some(v)
        {
            let raw = i32::try_from(
                v.checked_mul(64)
                    .ok_or_else(|| invalid("QPF2 fixed metric overflow"))?,
            )
            .map_err(|_| invalid("QPF2 fixed metric outside i32"))?;
            set_tag(&mut ts, kind, raw.to_be_bytes().to_vec());
        }
    }
    if let Some(n) = f.metrics.pixel_size {
        set_tag(&mut ts, 16, vec![ubyte(i64::from(n))?]);
    }
    set_tag(&mut ts, 15, vec![if gray { 8 } else { 1 }]);
    let mut header = Vec::new();
    for (kind, v) in ts {
        b.add(v.len() + 4)?;
        tag(&mut header, kind, &v)?;
    }
    tag(&mut header, 19, &[])?;
    while header.len() % 4 != 0 {
        header.push(0);
    }
    let hlen =
        u16::try_from(header.len()).map_err(|_| invalid("QPF2 tagged header exceeds u16"))?;
    let aligned_raster = (raster_len + 3) & !3;
    let cmap_len = (cmap.len() + 3) & !3;
    let size = 12usize
        .checked_add(header.len())
        .and_then(|n| n.checked_add(24 + cmap_len + count * 4 + aligned_raster))
        .ok_or_else(|| invalid("QPF2 output overflow"))?;
    check(
        size,
        o.max_output_bytes.min(b.limits.max_input_bytes),
        "QPF2 output bytes",
    )?;
    b.table(size)?;
    let mut out = Vec::with_capacity(size);
    out.extend_from_slice(b"QPF2\xff\xff\xff\xff\x02\x00");
    out.extend_from_slice(&hlen.to_be_bytes());
    out.extend_from_slice(&header);
    block_header(&mut out, 0, cmap_len)?;
    out.extend_from_slice(&cmap);
    out.resize(out.len() + cmap_len - cmap.len(), 0);
    block_header(&mut out, 1, count * 4)?;
    let mut offset = 0usize;
    for g in &glyphs {
        if let Some(g) = g {
            out.extend_from_slice(
                &u32::try_from(offset)
                    .map_err(|_| invalid("QPF2 glyph offset exceeds u32"))?
                    .to_be_bytes(),
            );
            offset += 6
                + (if gray {
                    g.bitmap.width() as usize
                } else {
                    g.bitmap.width().div_ceil(8) as usize
                }) * g.bitmap.height() as usize;
        } else {
            out.extend_from_slice(&u32::MAX.to_be_bytes());
        }
    }
    block_header(&mut out, 2, aligned_raster)?;
    for g in glyphs.into_iter().flatten() {
        let w = g.bitmap.width();
        let h = g.bitmap.height();
        out.extend_from_slice(&[
            w as u8,
            h as u8,
            if gray { w as u8 } else { w.div_ceil(8) as u8 },
            sbyte(g.metrics.bounds.x)?,
            sbyte(-g.metrics.bounds.y - i64::from(h))?,
            sbyte(
                g.metrics
                    .advance
                    .or(f.metrics.glyph_defaults.advance)
                    .map_or(i64::from(w), |v| v.x),
            )?,
        ]);
        match (&g.bitmap, gray) {
            (GlyphBitmap::Gray8(m), true) => out.extend_from_slice(m.as_bytes()),
            (GlyphBitmap::Mono(m), false) => out.extend_from_slice(m.as_bytes()),
            (GlyphBitmap::Mono(m), true) => {
                for y in 0..h as usize {
                    for x in 0..w as usize {
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
                for y in 0..h as usize {
                    for chunk in m.as_bytes()[y * m.stride()..(y + 1) * m.stride()].chunks(8) {
                        let mut byte = 0;
                        for (bit, &v) in chunk.iter().enumerate() {
                            if v >= o.gray_threshold.expect("validated threshold") {
                                byte |= 0x80 >> bit;
                            }
                        }
                        out.push(byte);
                    }
                }
            }
        }
    }
    out.resize(size, 0);
    Ok(out)
}
fn block_header(out: &mut Vec<u8>, tag: u16, len: usize) -> Result<()> {
    out.extend_from_slice(&tag.to_be_bytes());
    out.extend_from_slice(&[0, 0]);
    out.extend_from_slice(
        &u32::try_from(len)
            .map_err(|_| invalid("QPF2 block exceeds u32"))?
            .to_be_bytes(),
    );
    Ok(())
}
