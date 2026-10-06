//! NetBSD WSFT fixed-cell mono and alpha resources with independent wire orders.
use super::legacy_common::*;
use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
use crate::bitmap::{Bitmap, Bounds, Gray8, Mono};
use crate::model::{
    Codepoint, Font, FontSet, Glyph, GlyphBitmap, Property, PropertyValue, Spacing, Vector,
};
use crate::report::Impact;
use crate::resources::{OutputBundle, ReadContext, Written};
use crate::{Report, ResourceLimits, Result};
use std::collections::BTreeMap;
/// Native order tag, independently applicable to bits and bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum WsfontOrder {
    /// Driver-specific known order, interpreted by the input policy.
    Known,
    /// Most significant bit / leftmost byte first.
    LeftToRight,
    /// Least significant bit / rightmost byte first.
    RightToLeft,
}
impl WsfontOrder {
    fn wire(self) -> u32 {
        match self {
            Self::Known => 0,
            Self::LeftToRight => 1,
            Self::RightToLeft => 2,
        }
    }
    fn read(v: u32) -> Result<Self> {
        match v {
            0 => Ok(Self::Known),
            1 => Ok(Self::LeftToRight),
            2 => Ok(Self::RightToLeft),
            _ => Err(invalid("invalid WSF order identifier")),
        }
    }
}
/// Native raster class; the wire selects alpha when stride is at least width.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum WsfontDepth {
    /// Packed one-bit rows.
    Mono,
    /// One coverage byte per pixel.
    Alpha,
}
/// Explicit interpretation of driver-specific order zero.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct WsfontReadOptions {
    /// Meaning of a KNOWN bit-order tag, default left-to-right.
    pub known_bit_order: WsfontOrder,
    /// Meaning of a KNOWN byte-order tag, default left-to-right.
    pub known_byte_order: WsfontOrder,
}
impl Default for WsfontReadOptions {
    fn default() -> Self {
        Self {
            known_bit_order: WsfontOrder::LeftToRight,
            known_byte_order: WsfontOrder::LeftToRight,
        }
    }
}
/// Native bounded output policy.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct WsfontWriteOptions {
    /// Native character encoding identifier; absent preserves its source property.
    pub encoding: Option<u32>,
    /// Wire bit order; absent preserves source or uses left-to-right.
    pub bit_order: Option<WsfontOrder>,
    /// Wire byte order; absent preserves source or uses left-to-right.
    pub byte_order: Option<WsfontOrder>,
    /// Raster class; absent chooses alpha if any input glyph is Gray8.
    pub depth: Option<WsfontDepth>,
    /// Wire row bytes; absent preserves a compatible source stride or uses a tight row.
    pub stride: Option<u32>,
    /// Explicit permission to interpret Unicode numbers as native codes.
    pub unicode_as_legacy: bool,
    /// Explicit threshold when selecting mono output for Gray8 input.
    pub gray_threshold: Option<u8>,
    /// Interpretation of native KNOWN order tags, also used for the final reread.
    pub known_orders: WsfontReadOptions,
    /// Shared resource limits.
    pub limits: ResourceLimits,
    /// Output cap, also bounded by max_input_bytes.
    pub max_output_bytes: usize,
}
impl Default for WsfontWriteOptions {
    fn default() -> Self {
        Self {
            encoding: None,
            bit_order: None,
            byte_order: None,
            depth: None,
            stride: None,
            unicode_as_legacy: false,
            gray_threshold: None,
            known_orders: WsfontReadOptions::default(),
            limits: ResourceLimits::default(),
            max_output_bytes: 64 * 1024 * 1024,
        }
    }
}
/// Registry codec.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct WsfontCodec {
    /// Native read choices.
    pub read_options: WsfontReadOptions,
    /// Native write choices.
    pub write_options: WsfontWriteOptions,
}
impl FormatDetector for WsfontCodec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(if c.bytes().starts_with(b"WSFT") {
            Confidence::Certain
        } else {
            Confidence::NoMatch
        })
    }
}
impl FormatReader<FontSet> for WsfontCodec {
    fn read(&self, c: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
        read(c, &self.read_options)
    }
}
impl FormatWriter<FontSet> for WsfontCodec {
    fn write(&self, s: &FontSet, w: &WriteOptions) -> Result<Written> {
        write(s, w, &self.write_options)
    }
}
fn value(b: &[u8], at: usize) -> Result<u32> {
    u32_at(b, at, ByteOrder::Little)
}
fn order(v: WsfontOrder, known: WsfontOrder) -> Result<bool> {
    match if v == WsfontOrder::Known { known } else { v } {
        WsfontOrder::Known => Err(invalid(
            "WSF KNOWN interpretation must specify an actual order",
        )),
        WsfontOrder::LeftToRight => Ok(false),
        WsfontOrder::RightToLeft => Ok(true),
    }
}
fn encoding_name(v: u32) -> &'static str {
    match v {
        0 => "iso8859-1",
        1 => "cp437",
        2 => "pcvt",
        3 => "iso8859-7",
        4 => "iso8859-2",
        5 => "koi8-r",
        _ => "wsfont-native",
    }
}
/// Decode the native stride-selected depth without dropping row padding.
pub fn read(c: &mut ReadContext<'_>, o: &WsfontReadOptions) -> Result<(FontSet, Report)> {
    let mut budget = Budget::new(c.retained_input_bytes(), c.limits())?;
    parse(c.bytes(), o, c.limits(), &mut budget)
}
fn parse(
    b: &[u8],
    o: &WsfontReadOptions,
    l: &ResourceLimits,
    budget: &mut Budget,
) -> Result<(FontSet, Report)> {
    slice(b, 0, 100)?;
    if &b[..4] != b"WSFT" {
        return Err(invalid("not a WSFT resource"));
    }
    let first = value(b, 68)?;
    let count = value(b, 72)? as usize;
    let encoding = value(b, 76)?;
    let width = value(b, 80)?;
    let height = value(b, 84)?;
    let stride = value(b, 88)? as usize;
    let bit = WsfontOrder::read(value(b, 92)?)?;
    let byte = WsfontOrder::read(value(b, 96)?)?;
    let rb = order(bit, o.known_bit_order)?;
    let ry = order(byte, o.known_byte_order)?;
    if width == 0
        || height == 0
        || count == 0
        || u64::from(first) + count as u64 > u64::from(u32::MAX) + 1
    {
        return Err(invalid("invalid WSF geometry or character range"));
    }
    check(count, l.max_glyphs, "WSF glyph count")?;
    check(1, l.max_resources, "WSF strike")?;
    check(
        (width as usize)
            .checked_mul(height as usize)
            .ok_or_else(|| invalid("WSF pixel count overflow"))?,
        l.max_pixels_per_glyph,
        "WSF glyph pixels",
    )?;
    let alpha = stride >= width as usize;
    let row = if alpha {
        width as usize
    } else {
        width.div_ceil(8) as usize
    };
    if stride < row {
        return Err(invalid("WSF stride is smaller than a row"));
    }
    let cell = stride
        .checked_mul(height as usize)
        .ok_or_else(|| invalid("WSF cell size overflow"))?;
    let size = cell
        .checked_mul(count)
        .ok_or_else(|| invalid("WSF raster size overflow"))?;
    if b.len()
        != 100usize
            .checked_add(size)
            .ok_or_else(|| invalid("WSF total size overflow"))?
    {
        return Err(invalid("WSF raster length does not match header"));
    }
    check(size, l.max_table_bytes, "WSF raster table")?;
    budget.items::<Font>(1)?;
    budget.items::<Glyph>(count)?;
    budget.add(
        size.saturating_mul(3)
            .saturating_add(count.saturating_mul(256))
            .saturating_add(2048),
    )?;
    let mut report = Report::default();
    if bit == WsfontOrder::Known || byte == WsfontOrder::Known {
        budget.diagnostic(
            &mut report,
            "wsfont",
            "read.known_order",
            Impact::Repair,
            Some(0),
            None,
            format!(
                "Interpreted native KNOWN tags using {:?} bits and {:?} bytes",
                o.known_bit_order, o.known_byte_order
            ),
        )?;
    }
    let name_field = &b[4..68];
    let name_end = name_field.iter().position(|&v| v == 0).unwrap_or(64);
    let mut f = Font::default();
    f.source_format = Some("wsfont".into());
    f.names.family = Some(latin(&name_field[..name_end]));
    f.charset.name = Some(encoding_name(encoding).into());
    f.metrics.bounds = Some(Bounds::new(width, height, 0, 0));
    f.metrics.pixel_size = Some(height);
    f.metrics.spacing = Some(Spacing::CharacterCell);
    for (k, v) in [
        ("wsfont.encoding", encoding),
        ("wsfont.stride", stride as u32),
        ("wsfont.bit_order", bit.wire()),
        ("wsfont.byte_order", byte.wire()),
    ] {
        f.properties
            .push(Property::new(k, PropertyValue::Unsigned(u64::from(v))));
    }
    f.properties.push(Property::new(
        "wsfont.name_field",
        PropertyValue::Bytes(name_field.to_vec()),
    ));
    let partial = !alpha && !width.is_multiple_of(8);
    for i in 0..count {
        let src = &b[100 + cell * i..100 + cell * (i + 1)];
        let mut raster = vec![0; row * height as usize];
        let mut padding =
            Vec::with_capacity((stride - row + usize::from(partial)) * height as usize);
        for y in 0..height as usize {
            for x in 0..stride {
                let mut v = src[y * stride + if ry { stride - 1 - x } else { x }];
                if rb {
                    v = v.reverse_bits();
                }
                if x < row {
                    raster[y * row + x] = v;
                }
                if x >= row {
                    padding.push(v);
                } else if partial && x + 1 == row {
                    padding.push(v & (255 >> (width % 8)));
                }
            }
        }
        let bitmap: GlyphBitmap = if alpha {
            Bitmap::<Gray8>::from_raw(width, height, &raster, bitmap_limits(l))?.into()
        } else {
            Bitmap::<Mono>::from_raw(width, height, &raster, bitmap_limits(l))?.into()
        };
        let mut g = Glyph::new(bitmap);
        g.metrics.advance = Some(Vector::new(i64::from(width), 0));
        g.codepoints.push(Codepoint::Legacy(first + i as u32));
        if !padding.is_empty() {
            g.properties.push(Property::new(
                "wsfont.padding",
                PropertyValue::Bytes(padding),
            ));
        }
        f.add_glyph(g)?;
    }
    let mut set = FontSet::from(f);
    set.source_format = Some("wsfont".into());
    Ok((set, report))
}
/// Encode fixed cells, preserve compatible padding, and audit the same-limit reread.
pub fn write(s: &FontSet, w: &WriteOptions, o: &WsfontWriteOptions) -> Result<Written> {
    let mut r = Report::default();
    let result = encode(s, w, o, &mut r);
    finish(result, r, "wsfont", w.strict)
}
fn encode(
    s: &FontSet,
    w: &WriteOptions,
    o: &WsfontWriteOptions,
    r: &mut Report,
) -> Result<OutputBundle> {
    let mut budget = source_budget(s, &o.limits)?;
    if s.fonts.len() != 1 {
        return Err(invalid("WSF output requires one selected strike"));
    }
    let alpha = match o.depth {
        Some(WsfontDepth::Alpha) => true,
        Some(WsfontDepth::Mono) => false,
        None => s.fonts[0]
            .glyphs()
            .any(|(_, g)| matches!(g.bitmap, GlyphBitmap::Gray8(_))),
    };
    let converted = if alpha {
        None
    } else {
        Some(convert_gray(s, o.gray_threshold, &o.limits, &mut budget)?)
    };
    let f = &converted.as_deref().unwrap_or(s).fonts[0];
    let encoding = o.encoding.unwrap_or(
        u32::try_from(int(&f.properties, "wsfont.encoding", 0)?)
            .map_err(|_| invalid("WSF encoding exceeds word"))?,
    );
    let bit = o.bit_order.unwrap_or(WsfontOrder::read(
        u32::try_from(int(&f.properties, "wsfont.bit_order", 1)?)
            .map_err(|_| invalid("WSF bit order exceeds word"))?,
    )?);
    let byte = o.byte_order.unwrap_or(WsfontOrder::read(
        u32::try_from(int(&f.properties, "wsfont.byte_order", 1)?)
            .map_err(|_| invalid("WSF byte order exceeds word"))?,
    )?);
    let rb = order(bit, o.known_orders.known_bit_order)?;
    let ry = order(byte, o.known_orders.known_byte_order)?;
    let mut map = BTreeMap::new();
    let (mut width, mut top, mut bottom) = (0i64, 0i64, 0i64);
    for (id, g) in f.glyphs() {
        if g.metrics.bounds.x < 0 {
            return Err(invalid("WSF cells cannot contain negative left bearings"));
        }
        width = width
            .max(g.metrics.bounds.x + i64::from(g.bitmap.width()))
            .max(g.metrics.advance.map_or(0, |v| v.x));
        top = top.max(g.metrics.bounds.y + i64::from(g.bitmap.height()));
        bottom = bottom.min(g.metrics.bounds.y);
        for &cp in &g.codepoints {
            if matches!(cp, Codepoint::Unicode(_)) && !o.unicode_as_legacy {
                budget.loss(
                    r,
                    "wsfont",
                    "write.mapping",
                    Some(0),
                    Some(id.index()),
                    format!("Unicode mapping {cp:?} requires explicit native-domain permission"),
                )?;
                continue;
            }
            budget.add(64)?;
            if map.insert(cp.value(), g).is_some() {
                budget.loss(
                    r,
                    "wsfont",
                    "write.duplicate",
                    Some(0),
                    Some(id.index()),
                    format!("Last glyph wins for {cp:?}"),
                )?;
            }
        }
    }
    if let Some(cell) = f.metrics.bounds
        && cell.x == 0
        && i64::from(cell.width) >= width
        && cell.y <= bottom
        && cell.y + i64::from(cell.height) >= top
    {
        width = i64::from(cell.width);
        bottom = cell.y;
        top = cell.y + i64::from(cell.height);
    }
    let width = u32::try_from(width).map_err(|_| invalid("WSF width exceeds word"))?;
    let height = u32::try_from(top - bottom).map_err(|_| invalid("WSF height exceeds word"))?;
    if width == 0 || height == 0 {
        return Err(invalid("WSF cells must have positive dimensions"));
    }
    check(
        (width as usize)
            .checked_mul(height as usize)
            .ok_or_else(|| invalid("WSF area overflow"))?,
        o.limits.max_pixels_per_glyph,
        "WSF glyph pixels",
    )?;
    let row = if alpha {
        width as usize
    } else {
        width.div_ceil(8) as usize
    };
    let source_stride = usize::try_from(int(&f.properties, "wsfont.stride", row as i64)?)
        .map_err(|_| invalid("WSF stride out of range"))?;
    let stride = o.stride.map(|v| v as usize).unwrap_or(
        if source_stride >= row && (source_stride >= width as usize) == alpha {
            source_stride
        } else {
            row
        },
    );
    if stride < row || (stride >= width as usize) != alpha {
        return Err(invalid(
            "WSF stride selects the wrong native depth or cannot contain the row",
        ));
    }
    let first = *map
        .first_key_value()
        .ok_or_else(|| invalid("WSF requires a representable codepoint"))?
        .0;
    let last = *map
        .last_key_value()
        .ok_or_else(|| invalid("missing WSF codepoint"))?
        .0;
    let count = u64::from(last) - u64::from(first) + 1;
    if count > u64::from(u32::MAX) {
        return Err(invalid("WSF character count exceeds word"));
    }
    let count = count as usize;
    check(count, o.limits.max_glyphs, "WSF output glyph count")?;
    let cell = stride
        .checked_mul(height as usize)
        .ok_or_else(|| invalid("WSF cell size overflow"))?;
    let size = cell
        .checked_mul(count)
        .ok_or_else(|| invalid("WSF raster size overflow"))?;
    check(size, o.limits.max_table_bytes, "WSF raster table")?;
    let n = size
        .checked_add(100)
        .ok_or_else(|| invalid("WSF size overflow"))?;
    check(
        n,
        o.max_output_bytes.min(o.limits.max_input_bytes),
        "WSF output bytes",
    )?;
    budget.add(n.saturating_mul(2))?;
    let mut out = vec![0; n];
    out[..4].copy_from_slice(b"WSFT");
    let name = f.names.family.as_deref().unwrap_or("fontpixel");
    let namebytes = latin_bytes(name)?;
    if namebytes.len() > 64 || namebytes.contains(&0) {
        return Err(invalid(
            "WSF name must be at most 64 Latin-1 bytes without NUL",
        ));
    }
    out[4..4 + namebytes.len()].copy_from_slice(&namebytes);
    if let Some(PropertyValue::Bytes(original)) = f.properties.get("wsfont.name_field")
        && original.len() == 64
    {
        let end = original.iter().position(|&v| v == 0).unwrap_or(64);
        if original[..end] == namebytes {
            out[4..68].copy_from_slice(original);
        }
    }
    for (at, v) in [
        (68, first),
        (72, count as u32),
        (76, encoding),
        (80, width),
        (84, height),
        (88, stride as u32),
        (92, bit.wire()),
        (96, byte.wire()),
    ] {
        put32(&mut out, at, v, ByteOrder::Little);
    }
    let partial = !alpha && !width.is_multiple_of(8);
    let padrow = stride - row + usize::from(partial);
    for i in 0..count {
        let Some(g) = map.get(&(first + i as u32)).copied() else {
            continue;
        };
        let dest = &mut out[100 + i * cell..100 + (i + 1) * cell];
        if let Some(PropertyValue::Bytes(p)) = g.properties.get("wsfont.padding")
            && p.len() == padrow * height as usize
        {
            for y in 0..height as usize {
                let mut k = y * padrow;
                if partial {
                    dest[y * stride + row - 1] = p[k] & (255 >> (width % 8));
                    k += 1;
                }
                dest[y * stride + row..(y + 1) * stride].copy_from_slice(&p[k..(y + 1) * padrow]);
            }
        }
        let dx = g.metrics.bounds.x as usize;
        let dy = (top - g.metrics.bounds.y - i64::from(g.bitmap.height())) as usize;
        match (&g.bitmap, alpha) {
            (GlyphBitmap::Gray8(bm), true) => {
                for y in 0..bm.height() as usize {
                    dest[(dy + y) * stride + dx..(dy + y) * stride + dx + bm.width() as usize]
                        .copy_from_slice(
                            &bm.as_bytes()[y * bm.stride()..y * bm.stride() + bm.width() as usize],
                        );
                }
            }
            (GlyphBitmap::Mono(bm), true) => {
                for y in 0..bm.height() as usize {
                    for x in 0..bm.width() as usize {
                        dest[(dy + y) * stride + dx + x] =
                            if bm.as_bytes()[y * bm.stride() + x / 8] & (128 >> (x % 8)) == 0 {
                                0
                            } else {
                                255
                            };
                    }
                }
            }
            (GlyphBitmap::Mono(_), false) => {
                insert(dest, stride, dx, dy, mono(g)?);
            }
            _ => {
                return Err(invalid(
                    "WSF mono output requires explicit grayscale conversion",
                ));
            }
        }
        for y in 0..height as usize {
            let row = &mut dest[y * stride..(y + 1) * stride];
            if rb {
                for v in row.iter_mut() {
                    *v = v.reverse_bits();
                }
            }
            if ry {
                row.reverse();
            }
        }
    }
    let mut b = Budget::new(out.len(), &o.limits)?;
    let (back, rr) = parse(&out, &o.known_orders, &o.limits, &mut b)?;
    r.extend(rr);
    budget.add(b.used)?;
    audit(s, &back, "wsfont", r, &mut budget)?;
    Ok(OutputBundle::single(w.primary_name.clone(), out))
}
