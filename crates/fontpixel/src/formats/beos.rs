//! BeOS bitmap fonts: sparse Unicode hash tables, float metrics and nibble coverage.
use super::legacy_common::*;
use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
use crate::bitmap::{Bitmap, Gray8};
use crate::model::{
    Codepoint, Font, FontSet, Glyph, GlyphBitmap, Property, PropertyValue, Rational, Vector,
};
use crate::report::Impact;
use crate::resources::{OutputBundle, ReadContext, Written};
use crate::{Report, ResourceLimits, Result};
use std::collections::{BTreeMap, BTreeSet};
/// The native author specifies eight coverage levels in four-bit storage.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum BeosCoverage {
    /// Original BeOS format, nibble values 0 through 7.
    #[default]
    Native8,
    /// Explicit compatibility with tools using all sixteen nibble values.
    FullNibble,
}
impl BeosCoverage {
    fn max(self) -> u8 {
        if self == Self::Native8 { 7 } else { 15 }
    }
}
/// Native reading and explicit compatibility choices.
#[derive(Debug, Clone, Default)]
pub struct BeosReadOptions {
    /// Coverage interpretation; defaults to the original eight-level format.
    pub coverage: BeosCoverage,
    /// Permit ties-to-even integer advance projection while retaining exact float bits.
    pub round_fractional_metrics: bool,
}
/// Bounded native output choices.
#[derive(Debug, Clone)]
pub struct BeosWriteOptions {
    /// Coverage palette; absent preserves source or chooses native eight levels.
    pub coverage: Option<BeosCoverage>,
    /// Explicit nearest-level quantization for nonrepresentable Gray8 samples.
    pub quantize_gray: bool,
    /// Explicit permission to use legacy numeric labels as Unicode BMP codes.
    pub legacy_as_unicode: bool,
    /// Permit native fractional advances retained in source properties.
    pub round_fractional_metrics: bool,
    /// Shared resource limits.
    pub limits: ResourceLimits,
    /// Output cap, also bounded by max_input_bytes.
    pub max_output_bytes: usize,
}
impl Default for BeosWriteOptions {
    fn default() -> Self {
        Self {
            coverage: None,
            quantize_gray: false,
            legacy_as_unicode: false,
            round_fractional_metrics: false,
            limits: ResourceLimits::default(),
            max_output_bytes: 64 * 1024 * 1024,
        }
    }
}
/// Registry codec.
#[derive(Debug, Clone, Default)]
pub struct BeosCodec {
    /// Input choices.
    pub read_options: BeosReadOptions,
    /// Output choices.
    pub write_options: BeosWriteOptions,
}
impl FormatDetector for BeosCodec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(if c.bytes().starts_with(b"|Be;") {
            Confidence::Certain
        } else {
            Confidence::NoMatch
        })
    }
}
impl FormatReader<FontSet> for BeosCodec {
    fn read(&self, c: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
        read(c, &self.read_options)
    }
}
impl FormatWriter<FontSet> for BeosCodec {
    fn write(&self, s: &FontSet, w: &WriteOptions) -> Result<Written> {
        write(s, w, &self.write_options)
    }
}
fn word(b: &[u8], at: usize) -> Result<u16> {
    u16_at(b, at, ByteOrder::Big)
}
fn long(b: &[u8], at: usize) -> Result<u32> {
    u32_at(b, at, ByteOrder::Big)
}
fn projected(bits: u32, round: bool) -> Result<i64> {
    let f = f32::from_bits(bits);
    if !f.is_finite() || f64::from(f) < f64::from(i32::MIN) || f64::from(f) > f64::from(i32::MAX) {
        return Err(invalid(
            "BeOS advance is nonfinite or exceeds the model range",
        ));
    }
    if f.fract() != 0.0 && !round {
        return Err(invalid(
            "fractional BeOS advance requires explicit round_fractional_metrics",
        ));
    }
    Ok(f.round_ties_even() as i64)
}
fn coverage(v: u8, max: u8) -> u8 {
    ((u16::from(v) * 255 + u16::from(max) / 2) / u16::from(max)) as u8
}
fn hash(code: u16, mask: usize) -> usize {
    ((usize::from(code) >> 2) ^ (usize::from(code) << 3)) & mask
}
/// Read all physical glyphs, retaining unmapped records, aliases and opaque fields.
pub fn read(c: &mut ReadContext<'_>, o: &BeosReadOptions) -> Result<(FontSet, Report)> {
    let mut b = Budget::new(c.retained_input_bytes(), c.limits())?;
    parse(c.bytes(), o, c.limits(), &mut b)
}
fn parse(
    b: &[u8],
    o: &BeosReadOptions,
    l: &ResourceLimits,
    budget: &mut Budget,
) -> Result<(FontSet, Report)> {
    slice(b, 0, 36)?;
    if &b[..4] != b"|Be;" || long(b, 4)? as usize != b.len() {
        return Err(invalid("invalid BeOS magic or total size"));
    }
    check(b.len(), l.max_table_bytes, "BeOS table bytes")?;
    check(1, l.max_resources, "BeOS strike")?;
    let nf = usize::from(word(b, 8)?);
    let ns = usize::from(word(b, 10)?);
    let names = slice(b, 36, nf + ns + 2)?;
    if names[nf] != 0
        || names[nf + ns + 1] != 0
        || names[..nf].contains(&0)
        || names[nf + 1..nf + ns + 1].contains(&0)
    {
        return Err(invalid("invalid BeOS terminated family/style strings"));
    }
    let capacity = usize::from(word(b, 22)?) + 1;
    if !capacity.is_power_of_two() {
        return Err(invalid("BeOS hash capacity is not a power of two"));
    }
    let table = 36 + names.len();
    slice(b, table, capacity * 8)?;
    let start = table + capacity * 8;
    budget.items::<Font>(1)?;
    budget.add(
        capacity
            .saturating_mul(96)
            .saturating_add(names.len() * 4)
            .saturating_add(2048),
    )?;
    let mut loc: BTreeMap<usize, Vec<(u16, u16)>> = BTreeMap::new();
    let mut code_set = BTreeSet::new();
    let mut prefix = vec![0usize; capacity + 1];
    for i in 0..capacity {
        let p = long(b, table + i * 8)?;
        prefix[i + 1] = prefix[i] + usize::from(p == u32::MAX);
        if p == u32::MAX {
            continue;
        }
        let code = word(b, table + i * 8 + 4)?;
        if char::from_u32(u32::from(code)).is_none() || !code_set.insert(code) {
            return Err(invalid("invalid or duplicate BeOS Unicode mapping"));
        }
        if (p as usize) < start || (p as usize) >= b.len() {
            return Err(invalid("BeOS glyph pointer outside raster records"));
        }
        loc.entry(p as usize)
            .or_default()
            .push((code, word(b, table + i * 8 + 6)?));
    }
    for i in 0..capacity {
        if long(b, table + i * 8)? == u32::MAX {
            continue;
        }
        let h = hash(word(b, table + i * 8 + 4)?, capacity - 1);
        let empty = if h <= i {
            prefix[i + 1] - prefix[h]
        } else {
            prefix[capacity] - prefix[h] + prefix[i + 1]
        };
        if empty != 0 {
            return Err(invalid(
                "BeOS mapping is unreachable through its native hash chain",
            ));
        }
    }
    let mut f = Font::default();
    f.source_format = Some("beos".into());
    f.names.family = Some(latin(&names[..nf]));
    f.names.style = Some(latin(&names[nf + 1..nf + ns + 1]));
    f.charset.name = Some("unicode".into());
    f.metrics.point_size = Some(Rational::integer(i64::from(word(b, 24)?)));
    f.properties.push(Property::new(
        "beos.hash_capacity",
        PropertyValue::Unsigned(capacity as u64),
    ));
    f.properties.push(Property::new(
        "beos.coverage_levels",
        i64::from(o.coverage.max() + 1),
    ));
    let mut opaque = b[12..22].to_vec();
    opaque.extend_from_slice(&b[26..36]);
    f.properties.push(Property::new(
        "beos.header_opaque",
        PropertyValue::Bytes(opaque),
    ));
    let mut at = start;
    let mut report = Report::default();
    if o.coverage == BeosCoverage::FullNibble {
        budget.diagnostic(
            &mut report,
            "beos",
            "read.full_nibble",
            Impact::Repair,
            Some(0),
            None,
            "Explicit sixteen-level nibble interpretation instead of native eight levels",
        )?;
    }
    while at < b.len() {
        check(f.len() + 1, l.max_glyphs, "BeOS glyph count")?;
        budget.items::<Glyph>(1)?;
        budget.add(512)?;
        let p = at;
        slice(b, at, 24)?;
        let left = i64::from(word(b, at + 8)? as i16);
        let top = i64::from(word(b, at + 10)? as i16);
        let right = i64::from(word(b, at + 12)? as i16);
        let bottom = i64::from(word(b, at + 14)? as i16);
        let width = right - left + 1;
        let height = bottom - top + 1;
        if width < 0 || height < 0 {
            return Err(invalid("inverted BeOS glyph edges"));
        }
        let advance = long(b, at + 16)?;
        let dy = long(b, at + 20)?;
        if !f32::from_bits(dy).is_finite() {
            return Err(invalid("nonfinite BeOS secondary float"));
        }
        let dx = projected(advance, o.round_fractional_metrics)?;
        if f32::from_bits(advance).fract() != 0.0 {
            budget.diagnostic(
                &mut report,
                "beos",
                "read.fractional_advance",
                Impact::Repair,
                Some(0),
                Some(f.len()),
                "Projected fractional advance ties-to-even; exact IEEE-754 bits retained",
            )?;
        }
        check(
            width as usize * height as usize,
            l.max_pixels_per_glyph,
            "BeOS glyph pixels",
        )?;
        let row = (width as usize).div_ceil(2);
        let n = row * height as usize;
        let src = slice(b, at + 24, n)?;
        budget.add(
            (width as usize * height as usize)
                .saturating_mul(2)
                .saturating_add(height as usize),
        )?;
        let mut pixels = vec![0; width as usize * height as usize];
        let mut pad = Vec::new();
        for y in 0..height as usize {
            for x in 0..width as usize {
                let value = (src[y * row + x / 2] >> if x % 2 == 0 { 4 } else { 0 }) & 15;
                if value > o.coverage.max() {
                    return Err(invalid("BeOS sample exceeds selected coverage palette"));
                }
                pixels[y * width as usize + x] = coverage(value, o.coverage.max());
            }
            if width % 2 != 0 {
                pad.push(src[y * row + row - 1] & 15);
            }
        }
        let mut g = Glyph::new(Bitmap::<Gray8>::from_raw(
            width as u32,
            height as u32,
            &pixels,
            bitmap_limits(l),
        )?);
        g.metrics.bounds.x = left;
        g.metrics.bounds.y = -1 - bottom;
        g.metrics.advance = Some(Vector::new(dx, 0));
        g.properties.push(Property::new(
            "beos.record_opaque",
            PropertyValue::Bytes(b[at..at + 8].to_vec()),
        ));
        g.properties.push(Property::new(
            "beos.advance_bits",
            PropertyValue::Unsigned(u64::from(advance)),
        ));
        g.properties.push(Property::new(
            "beos.secondary_bits",
            PropertyValue::Unsigned(u64::from(dy)),
        ));
        if !pad.is_empty() {
            g.properties.push(Property::new(
                "beos.nibble_padding",
                PropertyValue::Bytes(pad),
            ));
        }
        if let Some(labels) = loc.remove(&p) {
            let mut reserved = Vec::with_capacity(labels.len() * 4);
            for (code, res) in labels {
                g.codepoints.push(Codepoint::Unicode(u32::from(code)));
                reserved.extend_from_slice(&code.to_be_bytes());
                reserved.extend_from_slice(&res.to_be_bytes());
            }
            g.properties.push(Property::new(
                "beos.mapping_reserved",
                PropertyValue::Bytes(reserved),
            ));
        }
        f.add_glyph(g)?;
        at += 24 + n;
    }
    if !loc.is_empty() {
        return Err(invalid("BeOS location points inside a glyph record"));
    }
    let mut set = FontSet::from(f);
    set.source_format = Some("beos".into());
    Ok((set, report))
}
/// Encode all physical glyphs and bounded collision chains, then audit the reread.
pub fn write(s: &FontSet, w: &WriteOptions, o: &BeosWriteOptions) -> Result<Written> {
    let mut r = Report::default();
    let result = encode(s, w, o, &mut r);
    finish(result, r, "beos", w.strict)
}
fn find_free(parent: &mut [usize], mut at: usize) -> usize {
    let mut root = at;
    while parent[root] != root {
        root = parent[root];
    }
    while parent[at] != at {
        let next = parent[at];
        parent[at] = root;
        at = next;
    }
    root
}
fn encode(
    s: &FontSet,
    w: &WriteOptions,
    o: &BeosWriteOptions,
    r: &mut Report,
) -> Result<OutputBundle> {
    let mut budget = source_budget(s, &o.limits)?;
    if s.fonts.len() != 1 {
        return Err(invalid("BeOS output requires one selected strike"));
    }
    let f = &s.fonts[0];
    let palette = if let Some(v) = o.coverage {
        v
    } else {
        match int(&f.properties, "beos.coverage_levels", 8)? {
            8 => BeosCoverage::Native8,
            16 => BeosCoverage::FullNibble,
            _ => return Err(invalid("BeOS coverage levels must be eight or sixteen")),
        }
    };
    let mut labels = BTreeMap::new();
    let mut records = Vec::new();
    budget.items::<usize>(f.len())?;
    for (id, g) in f.glyphs() {
        let index = records.len();
        records.push(g);
        for &cp in &g.codepoints {
            if cp.value() > 65535
                || char::from_u32(cp.value()).is_none()
                || matches!(cp, Codepoint::Legacy(_)) && !o.legacy_as_unicode
            {
                budget.loss(
                    r,
                    "beos",
                    "write.mapping",
                    Some(0),
                    Some(id.index()),
                    format!("Mapping {cp:?} is outside the selected Unicode BMP domain"),
                )?;
                continue;
            }
            budget.add(64)?;
            if labels.insert(cp.value() as u16, index).is_some() {
                budget.loss(
                    r,
                    "beos",
                    "write.duplicate",
                    Some(0),
                    Some(id.index()),
                    format!("Last glyph wins for {cp:?}"),
                )?;
            }
        }
    }
    let min_capacity = labels.len().max(1).next_power_of_two();
    let source_capacity = usize::try_from(int(
        &f.properties,
        "beos.hash_capacity",
        min_capacity as i64,
    )?)
    .map_err(|_| invalid("BeOS hash capacity out of range"))?;
    let capacity = if source_capacity >= min_capacity
        && source_capacity <= 65536
        && source_capacity.is_power_of_two()
    {
        source_capacity
    } else {
        min_capacity
    };
    let family = latin_bytes(f.names.family.as_deref().unwrap_or("fontpixel"))?;
    let style = latin_bytes(f.names.style.as_deref().unwrap_or("Regular"))?;
    if family.len() > 65535 || style.len() > 65535 || family.contains(&0) || style.contains(&0) {
        return Err(invalid("invalid BeOS family/style length or embedded NUL"));
    }
    let point = f.metrics.point_size.unwrap_or(Rational::integer(i64::from(
        f.metrics.pixel_size.unwrap_or(12),
    )));
    if point.numerator() % i64::from(point.denominator()) != 0 {
        return Err(invalid("BeOS point size must be an integer"));
    }
    let point = u16::try_from(point.numerator() / i64::from(point.denominator()))
        .map_err(|_| invalid("BeOS point size exceeds word"))?;
    let table = 38 + family.len() + style.len();
    let start = table + capacity * 8;
    let n = records.iter().try_fold(start, |n, g| {
        n.checked_add(24 + (g.bitmap.width() as usize).div_ceil(2) * g.bitmap.height() as usize)
            .ok_or_else(|| invalid("BeOS output overflow"))
    })?;
    check(
        n,
        o.max_output_bytes
            .min(o.limits.max_input_bytes)
            .min(o.limits.max_table_bytes)
            .min(u32::MAX as usize),
        "BeOS output bytes",
    )?;
    budget.add(
        n.saturating_mul(2)
            .saturating_add(capacity.saturating_mul(32))
            .saturating_add(labels.len().saturating_mul(32)),
    )?;
    let mut out = vec![0; n];
    out[..4].copy_from_slice(b"|Be;");
    put32(&mut out, 4, n as u32, ByteOrder::Big);
    put16(&mut out, 8, family.len() as u16, ByteOrder::Big);
    put16(&mut out, 10, style.len() as u16, ByteOrder::Big);
    put16(&mut out, 22, (capacity - 1) as u16, ByteOrder::Big);
    put16(&mut out, 24, point, ByteOrder::Big);
    put16(&mut out, 26, 0x300, ByteOrder::Big);
    if let Some(PropertyValue::Bytes(v)) = f.properties.get("beos.header_opaque")
        && v.len() == 20
    {
        out[12..22].copy_from_slice(&v[..10]);
        out[26..36].copy_from_slice(&v[10..]);
    }
    out[36..36 + family.len()].copy_from_slice(&family);
    out[37 + family.len()..37 + family.len() + style.len()].copy_from_slice(&style);
    let mut offsets = Vec::with_capacity(records.len());
    let mut at = start;
    for (index, g) in records.iter().enumerate() {
        offsets.push(at);
        put32(&mut out, at, 0x4996b438, ByteOrder::Big);
        put32(&mut out, at + 4, 0x4996b440, ByteOrder::Big);
        if let Some(PropertyValue::Bytes(v)) = g.properties.get("beos.record_opaque")
            && v.len() == 8
        {
            out[at..at + 8].copy_from_slice(v);
        }
        let left = g.metrics.bounds.x;
        let bottom = -1 - g.metrics.bounds.y;
        let top = bottom - i64::from(g.bitmap.height()) + 1;
        let right = left + i64::from(g.bitmap.width()) - 1;
        for (off, v) in [(8, left), (10, top), (12, right), (14, bottom)] {
            put16(
                &mut out,
                at + off,
                i16::try_from(v).map_err(|_| invalid("BeOS edge exceeds signed word"))? as u16,
                ByteOrder::Big,
            );
        }
        let advance = g
            .metrics
            .advance
            .map_or(i64::from(g.bitmap.width()), |v| v.x);
        let mut bits = (advance as f32).to_bits();
        if let Some(PropertyValue::Unsigned(raw)) = g.properties.get("beos.advance_bits") {
            let raw = u32::try_from(*raw).map_err(|_| invalid("BeOS float bits exceed word"))?;
            if projected(raw, true)? == advance {
                bits = raw;
            }
        }
        if projected(bits, o.round_fractional_metrics)? != advance {
            return Err(invalid(
                "BeOS advance cannot be represented exactly as an IEEE-754 float",
            ));
        }
        put32(&mut out, at + 16, bits, ByteOrder::Big);
        let dy = u32::try_from(int(&g.properties, "beos.secondary_bits", 0)?)
            .map_err(|_| invalid("BeOS secondary float bits exceed word"))?;
        if !f32::from_bits(dy).is_finite() {
            return Err(invalid("nonfinite BeOS secondary float"));
        }
        put32(&mut out, at + 20, dy, ByteOrder::Big);
        let width = g.bitmap.width() as usize;
        let height = g.bitmap.height() as usize;
        let row = width.div_ceil(2);
        let mut quantized = false;
        for y in 0..height {
            for x in 0..width {
                let sample = match &g.bitmap {
                    GlyphBitmap::Gray8(b) => b.as_bytes()[y * b.stride() + x],
                    GlyphBitmap::Mono(_) => {
                        let b = mono(g)?;
                        if b.as_bytes()[y * b.stride() + x / 8] & (128 >> (x % 8)) == 0 {
                            0
                        } else {
                            255
                        }
                    }
                };
                let level = ((u16::from(sample) * u16::from(palette.max()) + 127) / 255) as u8;
                if coverage(level, palette.max()) != sample {
                    if !o.quantize_gray {
                        return Err(invalid(
                            "BeOS sample is outside the exact native palette; quantize_gray is required",
                        ));
                    }
                    quantized = true;
                }
                out[at + 24 + y * row + x / 2] |= level << if x % 2 == 0 { 4 } else { 0 };
            }
        }
        if !width.is_multiple_of(2)
            && let Some(PropertyValue::Bytes(v)) = g.properties.get("beos.nibble_padding")
            && v.len() == height
        {
            for y in 0..height {
                out[at + 24 + y * row + row - 1] |= v[y] & 15;
            }
        }
        if quantized {
            budget.loss(
                r,
                "beos",
                "write.quantize",
                Some(0),
                Some(index),
                "Grayscale samples quantized to the selected native palette",
            )?;
        }
        at += 24 + row * height;
    }
    for i in 0..capacity {
        put32(&mut out, table + i * 8, u32::MAX, ByteOrder::Big);
    }
    let mut reserved = BTreeMap::new();
    for (index, g) in records.iter().enumerate() {
        if let Some(PropertyValue::Bytes(v)) = g.properties.get("beos.mapping_reserved") {
            for pair in v.as_chunks::<4>().0 {
                let code = u16::from_be_bytes([pair[0], pair[1]]);
                if labels.get(&code) == Some(&index) {
                    budget.add(64)?;
                    reserved.insert(code, u16::from_be_bytes([pair[2], pair[3]]));
                }
            }
        }
    }
    let mut parent: Vec<usize> = (0..=capacity).collect();
    for (&code, &index) in &labels {
        let mut slot = find_free(&mut parent, hash(code, capacity - 1));
        if slot == capacity {
            slot = find_free(&mut parent, 0);
        }
        if slot == capacity {
            return Err(invalid("BeOS hash table is full"));
        }
        parent[slot] = find_free(&mut parent, slot + 1);
        let p = table + slot * 8;
        put32(&mut out, p, offsets[index] as u32, ByteOrder::Big);
        put16(&mut out, p + 4, code, ByteOrder::Big);
        if let Some(&value) = reserved.get(&code) {
            put16(&mut out, p + 6, value, ByteOrder::Big);
        }
    }
    let opts = BeosReadOptions {
        coverage: palette,
        round_fractional_metrics: o.round_fractional_metrics,
    };
    let mut b = Budget::new(out.len(), &o.limits)?;
    let (back, rr) = parse(&out, &opts, &o.limits, &mut b)?;
    r.extend(rr);
    budget.add(b.used)?;
    audit(s, &back, "beos", r, &mut budget)?;
    Ok(OutputBundle::single(w.primary_name.clone(), out))
}
