use super::{IigsReadOptions, support::*};
use crate::bitmap::Bounds;
use crate::formats::Confidence;
use crate::model::{
    Codepoint, Font, FontCharset, FontMetrics, Glyph, Property, PropertyValue, Rational, Spacing,
    Vector,
};
use crate::{Bitmap, FontSet, Report, ResourceLimits, Result};
pub(super) fn word(b: &[u8], at: usize) -> Result<u16> {
    let s = b
        .get(
            at..at
                .checked_add(2)
                .ok_or_else(|| invalid("IIgs offset overflow"))?,
        )
        .ok_or_else(|| invalid("IIgs truncated word"))?;
    Ok(u16::from_le_bytes([s[0], s[1]]))
}
pub(super) struct Header {
    pub anchor: usize,
    pub mf: usize,
    pub version: u16,
    pub first: usize,
    pub count: usize,
    pub height: usize,
    pub row: usize,
    pub loc: usize,
    pub ow: usize,
}
pub(super) fn header(b: &[u8]) -> Result<Header> {
    let anchor = 1 + usize::from(*b.first().ok_or_else(|| invalid("IIgs empty file"))?);
    let words = usize::from(word(b, anchor)?);
    let version = word(b, anchor + 8)?;
    if !matches!(version, 0x101 | 0x105) {
        return Err(invalid("IIgs unsupported font record version"));
    }
    let minimum = if version == 0x105 { 7 } else { 6 };
    if words < minimum {
        return Err(invalid("IIgs Macintosh record overlaps its header"));
    }
    let high = if version == 0x105 {
        usize::from(word(b, anchor + 12)?)
    } else {
        0
    };
    let mf = anchor + words * 2;
    let first = usize::from(word(b, mf + 2)?);
    let last = usize::from(word(b, mf + 4)?);
    if first > last || last > 255 {
        return Err(invalid("IIgs character range is not an ordered byte range"));
    }
    let count = last - first + 2;
    let height = usize::from(word(b, mf + 14)?);
    let row = usize::from(word(b, mf + 24)?) * 2;
    let strike_len = row
        .checked_mul(height)
        .ok_or_else(|| invalid("IIgs strike size overflow"))?;
    let strike_end = mf
        .checked_add(26)
        .and_then(|v| v.checked_add(strike_len))
        .ok_or_else(|| invalid("IIgs table offset overflow"))?;
    let ow = (high << 16 | usize::from(word(b, mf + 16)?))
        .checked_mul(2)
        .and_then(|v| v.checked_add(mf + 16))
        .ok_or_else(|| invalid("IIgs width table offset overflow"))?;
    let loc = ow
        .checked_sub((count + 1) * 2)
        .ok_or_else(|| invalid("IIgs location offset underflow"))?;
    let end = ow
        .checked_add((count + 1) * 2)
        .ok_or_else(|| invalid("IIgs width end overflow"))?;
    if loc < strike_end || end > b.len() {
        return Err(invalid("IIgs overlapping or truncated tables"));
    }
    if word(b, ow + count * 2)? != 0xffff || word(b, ow + (count - 1) * 2)? == 0xffff {
        return Err(invalid(
            "IIgs missing required symbol or width-table terminator",
        ));
    }
    let mut previous = usize::from(word(b, loc)?);
    for i in 0..count {
        let next = usize::from(word(b, loc + (i + 1) * 2)?);
        let w = word(b, ow + i * 2)?;
        if w != 0xffff && (w & 255 == 255 || w >> 8 == 255) {
            return Err(invalid("IIgs defined width and offset must be in 0..=254"));
        }
        if next < previous || next > row * 8 || w == 0xffff && previous != next {
            return Err(invalid("IIgs invalid location table"));
        }
        previous = next;
    }
    Ok(Header {
        anchor,
        mf,
        version,
        first,
        count,
        height,
        row,
        loc,
        ow,
    })
}
/// Recognize a fully validated record layout, without allocating glyph bitmaps.
pub fn detect(b: &[u8]) -> Confidence {
    if super::binscii::detect(b) || header(b).is_ok() {
        Confidence::Possible
    } else {
        Confidence::NoMatch
    }
}
/// Parse a standalone font data fork. File-system type/auxiliary metadata is external.
pub fn read(data: &[u8], o: &IigsReadOptions, l: &ResourceLimits) -> Result<(FontSet, Report)> {
    check(data.len(), l.max_input_bytes, "IIgs input bytes")?;
    let mut b = Budget::new(l);
    b.add(data.len())?;
    Ok((decode_input(data, o, &mut b)?, Report::default()))
}
pub(super) fn decode_input(
    data: &[u8],
    o: &IigsReadOptions,
    b: &mut Budget<'_>,
) -> Result<FontSet> {
    if header(data).is_err() && super::binscii::detect(data) {
        let raw = super::binscii::decode(data, b)?;
        let mut set = decode(&raw, o, b)?;
        b.table(data.len())?;
        set.properties.push(Property::new(
            "iigs.binscii-source",
            PropertyValue::Bytes(data.to_vec()),
        ));
        Ok(set)
    } else {
        decode(data, o, b)
    }
}
pub(super) fn decode(data: &[u8], o: &IigsReadOptions, b: &mut Budget<'_>) -> Result<FontSet> {
    let h = header(data)?;
    check(h.count, b.limits.max_glyphs, "IIgs glyph slots")?;
    b.table(data.len())?;
    b.add(4096)?;
    if let Some(s) = &o.charset {
        b.table(s.len())?;
    }
    let charset =
        crate::charset::CharsetCodec::new("macroman", None).map_err(|e| invalid(e.to_string()))?;
    let name: String = data[1..h.anchor]
        .iter()
        .map(|&c| {
            charset
                .cp2unicode(u32::from(c))
                .and_then(char::from_u32)
                .unwrap_or(char::REPLACEMENT_CHARACTER)
        })
        .collect();
    let mut f = Font::default();
    f.source_format = Some("iigs".into());
    f.names.family = Some(name);
    f.names.style = Some(style_name(word(data, h.anchor + 4)?));
    f.charset = FontCharset {
        name: Some(o.charset.clone().unwrap_or_else(|| "unknown".into())),
        ..Default::default()
    };
    for (key, value) in [
        ("iigs.family-id", word(data, h.anchor + 2)?),
        ("iigs.style", word(data, h.anchor + 4)?),
        ("iigs.version", h.version),
        ("iigs.fbr-extent", word(data, h.anchor + 10)?),
    ] {
        f.properties
            .push(Property::new(key, PropertyValue::Integer(i64::from(value))));
    }
    f.properties.push(Property::new(
        "iigs.source",
        PropertyValue::Bytes(data.to_vec()),
    ));
    let kern = i64::from(word(data, h.mf + 8)? as i16);
    // Apple IIgs Toolbox Reference II p16-48 explicitly permits rectangles
    // entirely above or below the baseline: either metric can be negative.
    let ascent = i64::from(word(data, h.mf + 18)? as i16);
    let descent = i64::from(word(data, h.mf + 20)? as i16);
    f.metrics = FontMetrics {
        pixel_size: Some(h.height as u32),
        point_size: Some(Rational::integer(i64::from(word(data, h.anchor + 6)?))),
        ascent: Some(ascent),
        descent: Some(descent),
        bounds: Some(Bounds::new(
            u32::from(word(data, h.mf + 12)?),
            h.height as u32,
            kern,
            -descent,
        )),
        line_gap: Some(i64::from(word(data, h.mf + 22)? as i16)),
        ..Default::default()
    };
    let mut previous = usize::from(word(data, h.loc)?);
    let mut advances = None;
    let mut mono = true;
    for i in 0..h.count {
        let next = usize::from(word(data, h.loc + (i + 1) * 2)?);
        let w = word(data, h.ow + i * 2)?;
        if w != 0xffff {
            let width = next - previous;
            b.glyph(width, h.height)?;
            let stride = width.div_ceil(8);
            let len = stride * h.height;
            b.table(len)?;
            let mut raster = vec![0; len];
            for y in 0..h.height {
                let src = &data[h.mf + 26 + y * h.row..h.mf + 26 + (y + 1) * h.row];
                let shift = previous % 8;
                for x in 0..stride {
                    let at = previous / 8 + x;
                    raster[y * stride + x] = src[at] << shift
                        | if shift == 0 {
                            0
                        } else {
                            src.get(at + 1).copied().unwrap_or(0) >> (8 - shift)
                        };
                }
            }
            let mut g = Glyph::new(Bitmap::<crate::bitmap::Mono>::from_raw(
                width as u32,
                h.height as u32,
                &raster,
                bitmap_limits(b.limits),
            )?);
            drop(raster);
            b.used -= len;
            g.metrics.bounds.x = kern + i64::from(w >> 8);
            g.metrics.bounds.y = -descent;
            let advance = w & 255;
            g.metrics.advance = Some(Vector::new(i64::from(advance), 0));
            if i != h.count - 1 {
                g.codepoints.push(Codepoint::Legacy((h.first + i) as u32));
            }
            let id = f.add_glyph(g)?;
            if i == h.count - 1 {
                f.metrics.default_glyph = Some(id);
            } else {
                if let Some(a) = advances {
                    mono &= a == advance;
                }
                advances = Some(advance);
            }
        }
        previous = next;
    }
    f.metrics.spacing = Some(if mono {
        Spacing::Monospaced
    } else {
        Spacing::Proportional
    });
    let mut s = FontSet::from(f);
    s.source_format = Some("iigs".into());
    Ok(s)
}
pub(super) fn style_name(bits: u16) -> String {
    let mut s = String::new();
    for (i, name) in ["Bold", "Italic", "Underline", "Outline", "Shadow"]
        .iter()
        .enumerate()
    {
        if bits & (1 << i) != 0 {
            if !s.is_empty() {
                s.push(' ');
            }
            s.push_str(name);
        }
    }
    if s.is_empty() { "Regular".into() } else { s }
}
