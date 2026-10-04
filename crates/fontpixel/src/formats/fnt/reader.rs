use super::{FntReadOptions, Version, charset_name, support::*};
use crate::bitmap::{Bitmap, Bounds};
use crate::model::{
    Codepoint, Font, FontCharset, FontMetrics, Glyph, GlyphMetrics, Property, PropertyValue,
    Rational, Spacing, Vector,
};
use crate::{Report, ResourceLimits, Result};
/// Read one bounded FNT payload. Padding following dfSize is ignored.
pub fn read(bytes: &[u8], o: &FntReadOptions, l: &ResourceLimits) -> Result<(Font, Report)> {
    check(bytes.len(), l.max_input_bytes, "FNT input bytes")?;
    parse(bytes, o, l, &mut Budget::new(bytes.len(), l)?)
}
pub(crate) fn parse(
    bytes: &[u8],
    o: &FntReadOptions,
    l: &ResourceLimits,
    budget: &mut Budget,
) -> Result<(Font, Report)> {
    let version = Version::from_word(get16(bytes, 0)?)?;
    let size = get32(bytes, 2)? as usize;
    let b = slice(bytes, 0, size)?;
    slice(b, 0, version.header())?;
    let kind = get16(b, 66)?;
    if kind & 1 != 0 {
        return Err(invalid("vector FNT is unsupported; expected raster font"));
    }
    if kind & 4 != 0 {
        return Err(invalid("memory-only FNT bitmaps are unsupported"));
    }
    if version == Version::Three && get32(b, 118)? & !0x13 != 0 {
        return Err(invalid(
            "FNT color, ABC or unknown layout flags are unsupported",
        ));
    }
    let first = b[95];
    let last = b[96];
    if first > last {
        return Err(invalid("reversed FNT character range"));
    }
    let count = usize::from(last - first) + 1;
    let height = u32::from(get16(b, 88)?);
    if height == 0 {
        return Err(invalid("FNT pixel height is zero"));
    }
    let ascent = i64::from(get16(b, 74)?);
    if ascent > i64::from(height) {
        return Err(invalid("FNT ascent exceeds pixel height"));
    }
    let descent = i64::from(height) - ascent;
    let fixed = get16(b, 86)?;
    let bits = get32(b, 113)? as usize;
    let entry = match version {
        Version::One => {
            if fixed == 0 {
                2
            } else {
                0
            }
        }
        Version::Two => 4,
        Version::Three => 6,
    };
    let table_bytes = (count + 1) * entry;
    check(table_bytes, l.max_table_bytes, "FNT character table")?;
    slice(b, version.header(), table_bytes)?;
    if bits < version.header() + table_bytes || bits > b.len() {
        return Err(invalid(
            "FNT bitmap offset overlaps header/table or exceeds file",
        ));
    }
    let face = cstr(b, get32(b, 105)? as usize, l)?;
    let device = cstr(b, get32(b, 101)? as usize, l)?;
    let copyright = &b[6..66];
    let copyright = &copyright[..copyright.iter().position(|&v| v == 0).unwrap_or(60)];
    budget.add(
        std::mem::size_of::<Font>() + 4096 + 2 * (face.len() + device.len() + copyright.len()),
    )?;
    let mut font = Font::default();
    let mut report = Report::default();
    font.names.family = Some(decode_text(face));
    font.names.copyright = Some(decode_text(copyright));
    let encoding = o.code_page.as_deref().or_else(|| charset_name(b[85]));
    if let Some(name) = o.code_page.as_deref() {
        let c =
            crate::charset::CharsetCodec::new(name, None).map_err(|e| invalid(e.to_string()))?;
        if !c.known() {
            return Err(invalid("unknown explicit FNT code page"));
        }
    }
    font.charset = FontCharset {
        name: Some(encoding.unwrap_or("unknown").into()),
        windows_charset: Some(b[85]),
        ..Default::default()
    };
    if encoding.is_none() {
        diag(
            &mut report,
            "unknown_charset",
            crate::report::Impact::None,
            None,
            None,
            format!(
                "Windows charset {} retained as legacy byte codes; explicit code page required for Unicode conversion",
                b[85]
            ),
        );
    }
    font.metrics = FontMetrics {
        pixel_size: Some(height),
        point_size: Some(Rational::integer(i64::from(get16(b, 68)?))),
        resolution: Some(Vector::new(
            i64::from(get16(b, 72)?),
            i64::from(get16(b, 70)?),
        )),
        ascent: Some(ascent),
        descent: Some(descent),
        line_gap: Some(i64::from(get16(b, 78)?)),
        line_height: Some(i64::from(height) + i64::from(get16(b, 78)?)),
        default_character: Some(Codepoint::Legacy(u32::from(first) + u32::from(b[97]))),
        spacing: Some(if fixed == 0 {
            Spacing::Proportional
        } else {
            Spacing::CharacterCell
        }),
        ..Default::default()
    };
    font.properties
        .push(Property::new("fnt.version", i64::from(version.word())));
    for &(key, at, n) in SCALARS {
        let val = match n {
            1 => i64::from(b[at]),
            2 => i64::from(get16(b, at)?),
            _ => i64::from(get32(b, at)?),
        };
        font.properties.push(Property::new(key, val));
    }
    font.properties
        .push(Property::new("fnt.device", decode_text(device)));
    if version != Version::One {
        font.properties
            .push(Property::new("fnt.dfReserved", i64::from(b[117])));
    }
    if version == Version::Three {
        for (key, at, n) in [
            ("fnt.dfFlags", 118, 4),
            ("fnt.dfAspace", 122, 2),
            ("fnt.dfBspace", 124, 2),
            ("fnt.dfCspace", 126, 2),
            ("fnt.dfColorPointer", 128, 4),
        ] {
            font.properties.push(Property::new(
                key,
                if n == 2 {
                    i64::from(get16(b, at)?)
                } else {
                    i64::from(get32(b, at)?)
                },
            ));
        }
        font.properties.push(Property::new(
            "fnt.dfReserved1",
            PropertyValue::Bytes(b[132..148].to_vec()),
        ));
    }
    let mut widths = Vec::with_capacity(count);
    let mut offsets = Vec::with_capacity(count);
    let stride = usize::from(get16(b, 99)?);
    if version == Version::One {
        let n = stride
            .checked_mul(height as usize)
            .ok_or_else(|| invalid("FNT strike overflow"))?;
        check(n, l.max_table_bytes, "FNT strike bytes")?;
        slice(b, bits, n)?;
        for i in 0..count {
            let x = if fixed != 0 {
                i * usize::from(fixed)
            } else {
                usize::from(get16(b, 117 + 2 * i)?)
            };
            let end = if fixed != 0 {
                x + usize::from(fixed)
            } else {
                usize::from(get16(b, 117 + 2 * (i + 1))?)
            };
            if end < x || end > stride * 8 {
                return Err(invalid(
                    "FNT v1 bit positions are reversed or exceed strike",
                ));
            }
            widths.push((end - x) as u32);
            offsets.push(x);
        }
    } else {
        for i in 0..count {
            let at = version.header() + i * entry;
            let w = u32::from(get16(b, at)?);
            let offset = if version == Version::Two {
                usize::from(get16(b, at + 2)?)
            } else {
                get32(b, at + 2)? as usize
            };
            let n = bitmap_size(w, height, l)?;
            if w != 0 && offset < bits {
                return Err(invalid("FNT glyph offset precedes bitmap data"));
            }
            if w != 0 {
                slice(b, offset, n)?;
            }
            widths.push(w);
            offsets.push(offset);
        }
    }
    let max_width = *widths.iter().max().unwrap_or(&0);
    font.metrics.bounds = Some(Bounds::new(max_width, height, 0, -descent));
    for i in 0..count {
        let width = widths[i];
        if width == 0 {
            continue;
        }
        let n = bitmap_size(width, height, l)?;
        budget.glyph(n, l)?;
        // The temporary packed bytes and Bitmap coexist during construction.
        check(
            budget.used.saturating_add(n),
            l.max_total_bytes,
            "FNT bitmap scratch bytes",
        )?;
        let row = (width as usize).div_ceil(8);
        let mut data = vec![0; n];
        if version == Version::One {
            let x = offsets[i];
            for y in 0..height as usize {
                for col in 0..row {
                    let bit = x + col * 8;
                    let a = b[bits + y * stride + bit / 8];
                    let next = if bit / 8 + 1 < stride {
                        b[bits + y * stride + bit / 8 + 1]
                    } else {
                        0
                    };
                    data[y * row + col] = if bit % 8 == 0 {
                        a
                    } else {
                        (a << (bit % 8)) | (next >> (8 - bit % 8))
                    };
                }
            }
        } else {
            for y in 0..height as usize {
                for col in 0..row {
                    data[y * row + col] = b[offsets[i] + col * height as usize + y];
                }
            }
        }
        let mut glyph = Glyph::new(Bitmap::<crate::bitmap::Mono>::from_raw(
            width,
            height,
            &data,
            bitmap_limits(l),
        )?);
        glyph
            .codepoints
            .push(Codepoint::Legacy(u32::from(first) + i as u32));
        glyph.metrics = GlyphMetrics {
            bounds: Bounds::new(width, height, 0, -descent),
            advance: Some(Vector::new(i64::from(width), 0)),
            ..Default::default()
        };
        font.add_glyph(glyph)?;
    }
    if font.is_empty() {
        return Err(invalid("FNT has no nonzero-width glyphs"));
    }
    if u32::from(first) + u32::from(b[97]) > u32::from(last) {
        diag(
            &mut report,
            "default_range",
            crate::report::Impact::None,
            None,
            None,
            "FNT default character lies outside the character range; retained",
        );
    }
    Ok((font, report))
}
