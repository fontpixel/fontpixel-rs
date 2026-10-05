//! X11 SNF v4, including machine bitfields, independent bitmap layout and properties.
use super::legacy_common::*;
use super::{
    Confidence, FormatDetector, FormatReader, FormatWriter, ModelCharge, ReadOptions, WriteOptions,
};
pub use crate::bitmap::ByteOrder;
use crate::bitmap::{Bitmap, Bounds, ByteLayout, Mono};
use crate::model::{Codepoint, Font, FontSet, Glyph, Property, PropertyValue, Vector};
use crate::resources::{OutputBundle, ReadContext, Written};
use crate::{Report, ResourceLimits, Result};
/// SNF input options. Bitmap layout is not part of the historical SNF header.
#[derive(Debug, Clone, Default)]
pub struct SnfReadOptions {
    /// Integer byte order; absent uses both version stamps.
    pub byte_order: Option<ByteOrder>,
    /// Complete raster interpretation. Absent uses a layout property or unambiguous inference.
    pub bitmap: Option<ByteLayout>,
}
/// SNF output choices.
#[derive(Debug, Clone)]
pub struct SnfWriteOptions {
    /// Integer byte order and corresponding native C bitfield allocation.
    pub byte_order: ByteOrder,
    /// Independent bitmap packing.
    pub bitmap: ByteLayout,
    /// Add a legal X11 vendor property describing the otherwise unrecorded layout.
    pub include_layout_property: bool,
    /// Aggregate and per-glyph bounds.
    pub limits: ResourceLimits,
    /// Maximum output length, additionally capped by reader limits.
    pub max_output_bytes: usize,
    /// Explicit threshold for grayscale input; absent rejects grayscale.
    pub gray_threshold: Option<u8>,
}
impl Default for SnfWriteOptions {
    fn default() -> Self {
        Self {
            byte_order: ByteOrder::Little,
            bitmap: ByteLayout {
                row_alignment: 4,
                ..Default::default()
            },
            include_layout_property: true,
            limits: ResourceLimits::default(),
            gray_threshold: None,
            max_output_bytes: 64 * 1024 * 1024,
        }
    }
}
/// SNF registry codec.
#[derive(Debug, Clone, Default)]
pub struct SnfCodec {
    /// Explicit input interpretation.
    pub read_options: SnfReadOptions,
    /// Output representation.
    pub write_options: SnfWriteOptions,
}
impl FormatDetector for SnfCodec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(if header(c.bytes(), &self.read_options).is_ok() {
            Confidence::Certain
        } else {
            Confidence::NoMatch
        })
    }
}
impl FormatReader<FontSet> for SnfCodec {
    fn read(&self, c: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
        read(c, &self.read_options)
    }
}
impl FormatWriter<FontSet> for SnfCodec {
    fn write(&self, s: &FontSet, o: &WriteOptions) -> Result<Written> {
        write(s, o, &self.write_options)
    }
}
impl SnfCodec {
    /// [`write`](FormatWriter::write) with the model reserved by the caller.
    pub(crate) fn write_reserved(&self, s: &FontSet, o: &WriteOptions) -> Result<Written> {
        write_charged(s, o, &self.write_options, ModelCharge::Reserved)
    }
}
const LAYOUT: &str = "_FONTPIXEL_SNF_BITMAP_FORMAT";
#[derive(Clone, Copy)]
struct Header {
    e: ByteOrder,
    first_col: u32,
    last_col: u32,
    first_row: u32,
    count: usize,
    bitmap: usize,
    bitmap_size: usize,
    props: usize,
    nprops: usize,
    strings: usize,
    nstrings: usize,
    ink: bool,
}
fn bits(v: u32, e: ByteOrder) -> (usize, bool, u8) {
    match e {
        ByteOrder::Little => ((v & 0xffffff) as usize, v & 0x1000000 != 0, (v >> 25) as u8),
        ByteOrder::Big => ((v >> 8) as usize, v & 128 != 0, (v & 127) as u8),
    }
}
fn pack_bits(offset: usize, exists: bool, pad: u8, e: ByteOrder) -> u32 {
    match e {
        ByteOrder::Little => offset as u32 | u32::from(exists) << 24 | u32::from(pad) << 25,
        ByteOrder::Big => (offset as u32) << 8 | u32::from(exists) << 7 | u32::from(pad),
    }
}
fn header(b: &[u8], o: &SnfReadOptions) -> Result<Header> {
    slice(b, 0, 108)?;
    let e = if let Some(e) = o.byte_order {
        e
    } else if u32_at(b, 0, ByteOrder::Little)? == 4 && u32_at(b, 104, ByteOrder::Little)? == 4 {
        ByteOrder::Little
    } else {
        ByteOrder::Big
    };
    if u32_at(b, 0, e)? != 4 || u32_at(b, 104, e)? != 4 {
        return Err(invalid("invalid SNF v4 version stamps"));
    }
    let first_col = u32_at(b, 28, e)?;
    let last_col = u32_at(b, 32, e)?;
    let first_row = u32_at(b, 36, e)?;
    let last_row = u32_at(b, 40, e)?;
    if first_col > last_col || last_col > 255 || first_row > last_row || last_row > 255 {
        return Err(invalid("invalid SNF character rectangle"));
    }
    if ![0, 1].contains(&u32_at(b, 96, e)?) || ![0, 1].contains(&u32_at(b, 100, e)?) {
        return Err(invalid("SNF supports one monochrome glyph set"));
    }
    let count = ((last_col - first_col + 1) * (last_row - first_row + 1)) as usize;
    let bitmap = 108 + count * 16;
    let bitmap_size = bits(u32_at(b, 92, e)?, e).0;
    let props = bitmap + bitmap_size.div_ceil(4) * 4;
    let nprops = u32_at(b, 44, e)? as usize;
    let strings = props
        .checked_add(
            nprops
                .checked_mul(12)
                .ok_or_else(|| invalid("SNF properties overflow"))?,
        )
        .ok_or_else(|| invalid("SNF offset overflow"))?;
    let nstrings = u32_at(b, 48, e)? as usize;
    let flags = u32_at(b, 24, e)?;
    let ink = flags
        & match e {
            ByteOrder::Little => 8,
            ByteOrder::Big => 0x10000000,
        }
        != 0;
    slice(b, 108, count * 16)?;
    slice(b, bitmap, bitmap_size.div_ceil(4) * 4)?;
    slice(b, props, nprops * 12)?;
    slice(b, strings, nstrings)?;
    if ink {
        slice(b, strings + nstrings, (count + 2) * 16)?;
    }
    Ok(Header {
        e,
        first_col,
        last_col,
        first_row,
        count,
        bitmap,
        bitmap_size,
        props,
        nprops,
        strings,
        nstrings,
        ink,
    })
}
#[derive(Clone, Copy)]
struct Metric {
    v: [i16; 5],
    attr: u16,
    offset: usize,
    exists: bool,
    pad: u8,
}
fn metric(b: &[u8], at: usize, e: ByteOrder) -> Result<Metric> {
    let mut v = [0; 5];
    for (i, x) in v.iter_mut().enumerate() {
        *x = u16_at(b, at + i * 2, e)? as i16;
    }
    let (offset, exists, pad) = bits(u32_at(b, at + 12, e)?, e);
    Ok(Metric {
        v,
        attr: u16_at(b, at + 10, e)?,
        offset,
        exists,
        pad,
    })
}
impl Metric {
    fn size(self) -> Result<(u32, u32)> {
        let width = i32::from(self.v[1]) - i32::from(self.v[0]);
        let height = i32::from(self.v[3]) + i32::from(self.v[4]);
        if width < 0 || height < 0 {
            return Err(invalid("negative SNF glyph rectangle"));
        }
        Ok((width as u32, height as u32))
    }
}
fn canonical_metric(m: Metric) -> Vec<u8> {
    let mut b = vec![0; 12];
    for (i, v) in m.v.iter().enumerate() {
        put16(&mut b, i * 2, *v as u16, ByteOrder::Big);
    }
    put16(&mut b, 10, m.attr, ByteOrder::Big);
    b
}
fn string(b: &[u8], at: usize) -> Result<&[u8]> {
    let s = b
        .get(at..)
        .ok_or_else(|| invalid("SNF string offset outside table"))?;
    let n = s
        .iter()
        .position(|x| *x == 0)
        .ok_or_else(|| invalid("unterminated SNF property string"))?;
    Ok(&s[..n])
}
fn layout_number(l: ByteLayout) -> i64 {
    i64::from(l.row_alignment)
        | i64::from(l.scan_unit) << 8
        | i64::from(l.bit_order == crate::bitmap::BitOrder::LsbFirst) << 16
        | i64::from(l.byte_order == ByteOrder::Little) << 17
        | i64::from(l.row_order == crate::bitmap::RowOrder::BottomUp) << 18
}
fn number_layout(v: i64) -> Result<ByteLayout> {
    if !(0..=0x70808).contains(&v) || v & !0x70f0f != 0 {
        return Err(invalid("invalid SNF bitmap format property"));
    }
    let l = ByteLayout {
        row_alignment: (v & 255) as u8,
        scan_unit: ((v >> 8) & 255) as u8,
        bit_order: if v & 0x10000 != 0 {
            crate::bitmap::BitOrder::LsbFirst
        } else {
            crate::bitmap::BitOrder::MsbFirst
        },
        byte_order: if v & 0x20000 != 0 {
            ByteOrder::Little
        } else {
            ByteOrder::Big
        },
        row_order: if v & 0x40000 != 0 {
            crate::bitmap::RowOrder::BottomUp
        } else {
            crate::bitmap::RowOrder::TopDown
        },
    };
    stride(l, 0)?;
    Ok(l)
}
fn validate_layout(b: &[u8], h: Header, l: ByteLayout) -> Result<()> {
    for i in 0..h.count {
        let m = metric(b, 108 + i * 16, h.e)?;
        if !m.exists {
            continue;
        }
        let (w, height) = m.size()?;
        let n = stride(l, w)?
            .checked_mul(height as usize)
            .ok_or_else(|| invalid("SNF glyph bytes overflow"))?;
        if m.offset
            .checked_add(n)
            .is_none_or(|end| end > h.bitmap_size)
        {
            return Err(invalid("SNF glyph exceeds bitmap area"));
        }
    }
    Ok(())
}
fn infer_layout(b: &[u8], h: Header) -> Result<ByteLayout> {
    // The format cannot establish bit or scan order. Even matching row sizes do not prove them.
    let mut aligns = Vec::new();
    for align in [1, 2, 4, 8] {
        let l = ByteLayout {
            row_alignment: align,
            ..Default::default()
        };
        let mut next = 0usize;
        let mut ok = true;
        for i in 0..h.count {
            let m = metric(b, 108 + i * 16, h.e)?;
            if !m.exists {
                continue;
            }
            let (w, height) = m.size()?;
            if m.offset != next {
                ok = false;
                break;
            }
            next += stride(l, w)? * height as usize;
        }
        if ok && next == h.bitmap_size {
            aligns.push(align);
        }
    }
    Err(invalid(format!(
        "SNF does not record bitmap bit/scan order; supply bitmap ByteLayout (structurally possible row alignments: {aligns:?})"
    )))
}
/// Decode a SNF font. Untagged historical rasters require their originating server's bitmap layout.
pub fn read(c: &mut ReadContext<'_>, o: &SnfReadOptions) -> Result<(FontSet, Report)> {
    let mut budget = Budget::new(c.retained_input_bytes(), c.limits())?;
    parse(c.bytes(), o, c.limits(), &mut budget)
}
fn parse(
    b: &[u8],
    o: &SnfReadOptions,
    l: &ResourceLimits,
    budget: &mut Budget,
) -> Result<(FontSet, Report)> {
    let h = header(b, o)?;
    check(h.count, l.max_glyphs, "SNF slots")?;
    check(h.count * 16, l.max_table_bytes, "SNF metrics")?;
    check(h.nprops * 12, l.max_table_bytes, "SNF properties")?;
    check(h.nstrings, l.max_table_bytes, "SNF strings")?;
    check(h.bitmap_size, l.max_table_bytes, "SNF bitmaps")?;
    budget.items::<Glyph>(h.count)?;
    budget.items::<Property>(h.nprops + 12)?;
    let strings = slice(b, h.strings, h.nstrings)?;
    let mut f = Font::default();
    for i in 0..h.nprops {
        let at = h.props + i * 12;
        let name = string(strings, u32_at(b, at, h.e)? as usize)?;
        let v = u32_at(b, at + 4, h.e)?;
        let indirect = u32_at(b, at + 8, h.e)?;
        if indirect > 1 {
            return Err(invalid("SNF property indirect flag is not boolean"));
        }
        budget.add(name.len().saturating_mul(2) + 64)?;
        let value = if indirect != 0 {
            let s = string(strings, v as usize)?;
            budget.add(s.len().saturating_mul(2))?;
            PropertyValue::Text(latin(s))
        } else {
            PropertyValue::Integer(i64::from(v as i32))
        };
        f.properties.push(Property::new(latin(name), value));
    }
    let layout = if let Some(layout) = o.bitmap {
        layout
    } else if let Some(PropertyValue::Integer(v)) = f.properties.get(LAYOUT) {
        number_layout(*v)?
    } else {
        infer_layout(b, h)?
    };
    validate_layout(b, h, layout)?;
    for (name, at) in [
        ("snf.all_exist", 4),
        ("snf.draw_direction", 8),
        ("snf.no_overlap", 12),
        ("snf.constant_metrics", 16),
        ("snf.terminal_font", 20),
        ("snf.flags", 24),
        ("snf.pixel_depth", 96),
        ("snf.glyph_sets", 100),
    ] {
        let value = u32_at(b, at, h.e)?;
        let value = if at == 24 && h.e == ByteOrder::Big {
            value.reverse_bits()
        } else {
            value
        };
        f.headers.push(Property::new(name, i64::from(value)));
    }
    for (name, at) in [("snf.minbounds", 64), ("snf.maxbounds", 80)] {
        f.headers.push(Property::new(
            name,
            PropertyValue::Bytes(canonical_metric(metric(b, at, h.e)?)),
        ));
    }
    budget.add(4096)?;
    f.metrics.ascent = Some(i64::from(u32_at(b, 60, h.e)? as i32));
    f.metrics.descent = Some(i64::from(u32_at(b, 56, h.e)? as i32));
    for (key, field) in [
        ("FONT", &mut f.names.xlfd),
        ("FAMILY_NAME", &mut f.names.family),
        ("FULL_NAME", &mut f.names.full),
        ("COPYRIGHT", &mut f.names.copyright),
        ("FOUNDRY", &mut f.names.foundry),
        ("CHARSET_REGISTRY", &mut f.charset.registry),
        ("CHARSET_ENCODING", &mut f.charset.encoding),
    ] {
        if let Some(PropertyValue::Text(s)) = f.properties.get(key) {
            budget.add(s.len())?;
            *field = Some(s.clone());
        }
    }
    if let Some(PropertyValue::Integer(v)) = f.properties.get("PIXEL_SIZE") {
        f.metrics.pixel_size = u32::try_from(*v).ok();
    }
    let unicode = f
        .charset
        .registry
        .as_deref()
        .is_some_and(|s| s.eq_ignore_ascii_case("ISO10646"))
        && f.charset.encoding.as_deref() == Some("1");
    let default = u32_at(b, 52, h.e)?;
    if default != 65535 {
        f.metrics.default_character = Some(if unicode {
            Codepoint::Unicode(default)
        } else {
            Codepoint::Legacy(default)
        });
    }
    if h.ink {
        for (name, at) in [
            ("snf.ink_minbounds", h.strings + h.nstrings),
            ("snf.ink_maxbounds", h.strings + h.nstrings + 16),
        ] {
            f.headers.push(Property::new(
                name,
                PropertyValue::Bytes(canonical_metric(metric(b, at, h.e)?)),
            ));
        }
    }
    for i in 0..h.count {
        let m = metric(b, 108 + i * 16, h.e)?;
        if !m.exists {
            continue;
        }
        let (w, height) = m.size()?;
        let size = stride(layout, w)? * height as usize;
        check(
            w as usize * height as usize,
            l.max_pixels_per_glyph,
            "SNF glyph pixels",
        )?;
        budget.add(size.saturating_mul(2) + 128)?;
        let mut g = Glyph::new(Bitmap::<Mono>::from_bytes(
            w,
            height,
            slice(b, h.bitmap + m.offset, size)?,
            layout,
            bitmap_limits(l),
        )?);
        g.metrics.bounds = Bounds::new(w, height, i64::from(m.v[0]), -i64::from(m.v[4]));
        g.metrics.advance = Some(Vector::new(i64::from(m.v[2]), 0));
        g.properties
            .push(Property::new("snf.attributes", i64::from(m.attr)));
        if m.pad != 0 {
            g.properties
                .push(Property::new("snf.char_padding", i64::from(m.pad)));
        }
        let cols = (h.last_col - h.first_col + 1) as usize;
        let cp =
            ((h.first_row + i as u32 / cols as u32) << 8) | (h.first_col + i as u32 % cols as u32);
        if unicode && char::from_u32(cp).is_none() {
            return Err(invalid("SNF ISO10646 mapping is not a Unicode scalar"));
        }
        g.codepoints.push(if unicode {
            Codepoint::Unicode(cp)
        } else {
            Codepoint::Legacy(cp)
        });
        if h.ink {
            g.properties.push(Property::new(
                "snf.ink_metrics",
                PropertyValue::Bytes(canonical_metric(metric(
                    b,
                    h.strings + h.nstrings + 32 + i * 16,
                    h.e,
                )?)),
            ));
        }
        f.add_glyph(g)?;
    }
    f.source_format = Some("snf".into());
    let mut s = FontSet::from(f);
    s.source_format = Some("snf".into());
    Ok((s, Report::default()))
}
fn put_metric(b: &mut [u8], at: usize, m: Metric, e: ByteOrder) {
    for (i, v) in m.v.iter().enumerate() {
        put16(b, at + i * 2, *v as u16, e);
    }
    put16(b, at + 10, m.attr, e);
    put32(b, at + 12, pack_bits(m.offset, m.exists, m.pad, e), e);
}
fn glyph_metric(g: &Glyph, offset: usize) -> Result<Metric> {
    let bb = g.metrics.bounds;
    let vals = [
        bb.x,
        bb.x + i64::from(bb.width),
        g.metrics.advance.map_or(i64::from(bb.width), |v| v.x),
        bb.y + i64::from(bb.height),
        -bb.y,
    ];
    let mut v = [0; 5];
    for (i, val) in vals.into_iter().enumerate() {
        v[i] = i16::try_from(val).map_err(|_| invalid("SNF metric out of i16 range"))?;
    }
    Ok(Metric {
        v,
        attr: u16::try_from(int(&g.properties, "snf.attributes", 0)?)
            .map_err(|_| invalid("SNF attributes out of range"))?,
        offset,
        exists: true,
        pad: u8::try_from(int(&g.properties, "snf.char_padding", 0)?)
            .map_err(|_| invalid("SNF padding out of range"))?,
    })
}
fn push_string(
    strings: &mut Vec<u8>,
    s: &str,
    budget: &mut Budget,
    l: &ResourceLimits,
) -> Result<u32> {
    if s.contains('\0') {
        return Err(invalid("SNF strings cannot contain NUL"));
    }
    budget.add(s.len().saturating_mul(2) + 1)?;
    let value = latin_bytes(s)?;
    let at = u32::try_from(strings.len()).map_err(|_| invalid("SNF string table exceeds u32"))?;
    check(
        strings.len().saturating_add(value.len() + 1),
        l.max_table_bytes,
        "SNF strings",
    )?;
    strings.extend_from_slice(&value);
    strings.push(0);
    Ok(at)
}
/// Encode one SNF v4 strike with explicit machine layout.
fn encode(
    s: &FontSet,
    w: &WriteOptions,
    o: &SnfWriteOptions,
    charge: ModelCharge,
    report: &mut Report,
) -> Result<OutputBundle> {
    let source = s;
    stride(o.bitmap, 0)?;
    let mut budget = source_budget_for(s, &o.limits, charge)?;
    let converted = convert_gray(s, o.gray_threshold, &o.limits, &mut budget)?;
    let s = converted.as_ref();
    let f = s.single()?;
    let slots = select(f, 65535, true, "snf", report, &mut budget)?;
    let first_row = slots.keys().map(|c| c >> 8).min().unwrap_or(0);
    let last_row = slots.keys().map(|c| c >> 8).max().unwrap_or(0);
    let first_col = slots.keys().map(|c| c & 255).min().unwrap_or(0);
    let last_col = slots.keys().map(|c| c & 255).max().unwrap_or(0);
    let cols = last_col - first_col + 1;
    let count = ((last_row - first_row + 1) * cols) as usize;
    check(count, o.limits.max_glyphs, "SNF slots")?;
    check(count * 16, o.limits.max_table_bytes, "SNF metrics")?;
    let mut bitmap_size = 0usize;
    for g in slots.values() {
        let bm = mono(g)?;
        bitmap_size = bitmap_size
            .checked_add(stride(o.bitmap, bm.width())? * bm.height() as usize)
            .ok_or_else(|| invalid("SNF bitmap overflow"))?;
        glyph_metric(g, 0)?;
    }
    check(
        bitmap_size,
        0xffffff.min(o.limits.max_table_bytes),
        "SNF bitmaps",
    )?;
    budget.add(bitmap_size.saturating_mul(2))?;
    let mut props = Vec::new();
    let mut strings = Vec::new();
    budget.items::<(u32, u32, u32)>(f.properties.len() + 8)?;
    let mut have_layout = false;
    for p in f.properties.iter() {
        let value = if p.name == LAYOUT {
            have_layout = true;
            if !o.include_layout_property {
                continue;
            }
            PropertyValue::Integer(layout_number(o.bitmap))
        } else {
            p.value.clone()
        };
        let name = push_string(&mut strings, &p.name, &mut budget, &o.limits)?;
        let (value, indirect) = match value {
            PropertyValue::Text(s) => (push_string(&mut strings, &s, &mut budget, &o.limits)?, 1),
            PropertyValue::Integer(v) => {
                if let Ok(v) = i32::try_from(v) {
                    (v as u32, 0)
                } else {
                    continue;
                }
            }
            PropertyValue::Unsigned(v) => {
                if let Ok(v) = u32::try_from(v) {
                    (v, 0)
                } else {
                    continue;
                }
            }
            _ => continue,
        };
        props.push((name, value, indirect));
    }
    if o.include_layout_property && !have_layout {
        let name = push_string(&mut strings, LAYOUT, &mut budget, &o.limits)?;
        props.push((name, layout_number(o.bitmap) as u32, 0));
    }
    for (key, value) in [
        ("FONT", &f.names.xlfd),
        ("FAMILY_NAME", &f.names.family),
        ("FULL_NAME", &f.names.full),
        ("COPYRIGHT", &f.names.copyright),
        ("FOUNDRY", &f.names.foundry),
    ] {
        if f.properties.get(key).is_none()
            && let Some(value) = value
        {
            let name = push_string(&mut strings, key, &mut budget, &o.limits)?;
            let value = push_string(&mut strings, value, &mut budget, &o.limits)?;
            props.push((name, value, 1));
        }
    }
    let unicode = f
        .glyphs()
        .flat_map(|(_, g)| &g.codepoints)
        .any(|c| matches!(c, Codepoint::Unicode(_)));
    if unicode && f.properties.get("CHARSET_REGISTRY").is_none() {
        for (key, value) in [("CHARSET_REGISTRY", "ISO10646"), ("CHARSET_ENCODING", "1")] {
            let name = push_string(&mut strings, key, &mut budget, &o.limits)?;
            let value = push_string(&mut strings, value, &mut budget, &o.limits)?;
            props.push((name, value, 1));
        }
    }
    let ink = f
        .glyphs()
        .any(|(_, g)| g.properties.get("snf.ink_metrics").is_some());
    let bitmap = 108 + count * 16;
    let prop_at = bitmap + bitmap_size.div_ceil(4) * 4;
    let str_at = prop_at + props.len() * 12;
    let ink_at = str_at + strings.len();
    let length = ink_at + if ink { (count + 2) * 16 } else { 0 };
    check(props.len() * 12, o.limits.max_table_bytes, "SNF properties")?;
    check(
        length,
        o.max_output_bytes.min(o.limits.max_input_bytes),
        "SNF output bytes",
    )?;
    budget.add(length)?;
    let mut out = vec![0; length];
    let e = o.byte_order;
    for (at, value) in [
        (0, 4),
        (28, first_col),
        (32, last_col),
        (36, first_row),
        (40, last_row),
        (44, props.len() as u32),
        (48, strings.len() as u32),
        (96, 1),
        (100, 1),
        (104, 4),
    ] {
        put32(&mut out, at, value, e);
    }
    let default = match f.metrics.default_character {
        Some(Codepoint::Legacy(v) | Codepoint::Unicode(v)) if v <= 65535 => v,
        _ => 65535,
    };
    put32(&mut out, 52, default, e);
    put32(
        &mut out,
        56,
        i32::try_from(f.metrics.descent.unwrap_or(0))
            .map_err(|_| invalid("SNF descent out of range"))? as u32,
        e,
    );
    put32(
        &mut out,
        60,
        i32::try_from(f.metrics.ascent.unwrap_or(0))
            .map_err(|_| invalid("SNF ascent out of range"))? as u32,
        e,
    );
    for (name, at, default) in [
        ("snf.all_exist", 4, i64::from(count == slots.len())),
        ("snf.draw_direction", 8, 0),
        ("snf.no_overlap", 12, 0),
        ("snf.constant_metrics", 16, 0),
        ("snf.terminal_font", 20, 0),
        ("snf.pixel_depth", 96, 1),
        ("snf.glyph_sets", 100, 1),
    ] {
        put32(
            &mut out,
            at,
            u32::try_from(int(&f.headers, name, default)?)
                .map_err(|_| invalid("SNF header value out of range"))?,
            e,
        );
    }
    let flags = (int(&f.headers, "snf.flags", i64::from(first_row == last_row))? as u32 & !8)
        | (u32::from(ink) * 8);
    put32(
        &mut out,
        24,
        if e == ByteOrder::Big {
            flags.reverse_bits()
        } else {
            flags
        },
        e,
    );
    let mut min = Metric {
        v: [i16::MAX; 5],
        attr: u16::MAX,
        offset: 0,
        exists: false,
        pad: 0,
    };
    let mut max = Metric {
        v: [i16::MIN; 5],
        attr: 0,
        offset: bitmap_size,
        exists: false,
        pad: 0,
    };
    let mut offset = 0usize;
    for (row, col) in
        (first_row..=last_row).flat_map(|row| (first_col..=last_col).map(move |col| (row, col)))
    {
        let i = ((row - first_row) * cols + col - first_col) as usize;
        if let Some(g) = slots.get(&((row << 8) | col)) {
            let m = glyph_metric(g, offset)?;
            for j in 0..5 {
                min.v[j] = min.v[j].min(m.v[j]);
                max.v[j] = max.v[j].max(m.v[j]);
            }
            min.attr = min.attr.min(m.attr);
            max.attr = max.attr.max(m.attr);
            put_metric(&mut out, 108 + i * 16, m, e);
            let bm = mono(g)?;
            let data = bm.to_bytes(o.bitmap)?;
            out[bitmap + offset..bitmap + offset + data.len()].copy_from_slice(&data);
            offset += data.len();
            if ink {
                put_ink(
                    &mut out,
                    ink_at + 32 + i * 16,
                    g.properties.get("snf.ink_metrics"),
                    m,
                    e,
                )?;
            }
        }
    }
    put_metric(&mut out, 64, min, e);
    put_metric(&mut out, 80, max, e);
    if ink {
        put_ink(&mut out, ink_at, f.headers.get("snf.ink_minbounds"), min, e)?;
        put_ink(
            &mut out,
            ink_at + 16,
            f.headers.get("snf.ink_maxbounds"),
            max,
            e,
        )?;
    }
    for (i, (name, value, indirect)) in props.into_iter().enumerate() {
        put32(&mut out, prop_at + i * 12, name, e);
        put32(&mut out, prop_at + i * 12 + 4, value, e);
        put32(&mut out, prop_at + i * 12 + 8, indirect, e);
    }
    out[str_at..ink_at].copy_from_slice(&strings);
    let (decoded, _) = parse(
        &out,
        &SnfReadOptions {
            byte_order: Some(e),
            bitmap: Some(o.bitmap),
        },
        &o.limits,
        &mut budget,
    )?;
    audit(source, &decoded, "snf", report, &mut budget)?;
    Ok(OutputBundle::single(w.primary_name.clone(), out))
}
fn put_ink(
    b: &mut [u8],
    at: usize,
    value: Option<&PropertyValue>,
    fallback: Metric,
    e: ByteOrder,
) -> Result<()> {
    let mut m = fallback;
    m.offset = 0;
    m.exists = false;
    m.pad = 0;
    if let Some(PropertyValue::Bytes(bytes)) = value {
        if bytes.len() != 12 {
            return Err(invalid("SNF ink metric requires 12 bytes"));
        }
        for i in 0..5 {
            m.v[i] = u16_at(bytes, i * 2, ByteOrder::Big)? as i16;
        }
        m.attr = u16_at(bytes, 10, ByteOrder::Big)?;
    }
    put_metric(b, at, m, e);
    Ok(())
}

fn stride(l: ByteLayout, w: u32) -> Result<usize> {
    if ![1, 2, 4, 8].contains(&l.row_alignment) || ![1, 2, 4, 8].contains(&l.scan_unit) {
        return Err(invalid("SNF alignment and scan unit must be 1,2,4,8"));
    }
    let swap = matches!(
        (l.bit_order, l.byte_order),
        (crate::bitmap::BitOrder::MsbFirst, ByteOrder::Little)
            | (crate::bitmap::BitOrder::LsbFirst, ByteOrder::Big)
    );
    if swap && l.scan_unit > l.row_alignment {
        return Err(invalid("SNF scan unit exceeds row alignment"));
    }
    Ok((w as usize).div_ceil(8).div_ceil(l.row_alignment as usize) * l.row_alignment as usize)
}

/// Encode and enforce strict mode before returning any output assets.
pub fn write(s: &FontSet, w: &WriteOptions, o: &SnfWriteOptions) -> Result<Written> {
    write_charged(s, w, o, ModelCharge::Estimate)
}
fn write_charged(
    s: &FontSet,
    w: &WriteOptions,
    o: &SnfWriteOptions,
    charge: ModelCharge,
) -> Result<Written> {
    let mut report = Report::default();
    let result = encode(s, w, o, charge, &mut report);
    finish(result, report, "snf", w.strict)
}
