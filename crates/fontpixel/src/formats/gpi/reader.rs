//! Bounded GPI records, optional tables and complete native glyph ranges.
use super::*;
pub(super) fn parse(
    b: &[u8],
    o: &GpiReadOptions,
    l: &ResourceLimits,
    budget: &mut Budget,
) -> Result<(FontSet, Report)> {
    check(b.len(), l.max_table_bytes, "GPI source bytes")?;
    let (pack, resources) = if b.starts_with(&[254, 255, 255, 255]) {
        budget.add(b.len())?;
        (GpiContainer::Bare, vec![(1, b.to_vec())])
    } else {
        container::extract(b, l, budget)?
    };
    check(resources.len(), l.max_resources, "GPI strikes")?;
    budget.items::<Font>(resources.len())?;
    let mut set = FontSet {
        source_format: Some("gpi".into()),
        ..Default::default()
    };
    let mut total = 0;
    budget.add(b.len().saturating_mul(2).saturating_add(2048))?;
    set.properties.push(Property::new(
        "gpi.native",
        PropertyValue::Bytes(b.to_vec()),
    ));
    set.properties.push(Property::new(
        "gpi.container",
        match pack {
            GpiContainer::Bare => "bare",
            GpiContainer::Ne => "ne",
            GpiContainer::Lx => "lx",
        },
    ));
    set.properties.push(Property::new(
        "gpi.include_unmapped_null",
        i64::from(o.include_unmapped_null),
    ));
    for (id, raw) in resources {
        let mut f = font(&raw, o, l, budget, &mut total)?;
        f.properties
            .push(Property::new("gpi.resource_id", i64::from(id)));
        set.fonts.push(f);
    }
    if set.fonts.is_empty() {
        return Err(invalid("OS/2 module contains no GPI bitmap fonts"));
    }
    Ok((set, Report::default()))
}
fn name(b: &[u8]) -> String {
    latin(&b[..b.iter().position(|&v| v == 0).unwrap_or(b.len())])
}
fn font(
    b: &[u8],
    o: &GpiReadOptions,
    l: &ResourceLimits,
    budget: &mut Budget,
    total: &mut usize,
) -> Result<Font> {
    slice(b, 0, 20)?;
    if u32v(b, 0)? != 0xfffffffe || u32v(b, 4)? != 20 || !b[8..20].starts_with(b"OS/2 FONT") {
        return Err(invalid("invalid GPI start record"));
    }
    let mlen = u32v(b, 24)? as usize;
    if mlen < 168 || u32v(b, 20)? != 1 {
        return Err(invalid("invalid GPI metrics record"));
    }
    let m = slice(b, 20, mlen)?;
    if u16v(m, 130)? & 1 != 0 {
        return Err(invalid("GPI outline records are not bitmap fonts"));
    }
    let dpos = 20 + mlen;
    let dlen = u32v(b, dpos + 4)? as usize;
    let d = slice(b, dpos, dlen)?;
    if dlen < 28 || u32v(d, 0)? != 2 {
        return Err(invalid("invalid GPI font-definition record"));
    }
    let kind = match (u16v(d, 8)?, u16v(d, 10)?) {
        (0x47, 0x81) => GpiKind::Fixed,
        (0x42, 0x81) => GpiKind::Proportional,
        (0x42, 0xb8) => GpiKind::Abc,
        _ => return Err(invalid("unsupported GPI font/character definition flags")),
    };
    let cell = usize::from(u16v(d, 12)?);
    let minimum = if kind == GpiKind::Abc { 10 } else { 6 };
    if cell < minimum {
        return Err(invalid("GPI descriptor stride is too small"));
    }
    let h = i16v(d, 16)?;
    if h < 0 {
        return Err(invalid("negative GPI cell height"));
    }
    let base = i16v(d, 26)?;
    let first = u32::from(u16v(m, 114)?);
    let count = usize::from(u16v(m, 116)?) + 1;
    if first + count as u32 > 65536 {
        return Err(invalid("GPI character range exceeds word codes"));
    }
    slice(d, 28, count * cell)?;
    let mut n = count;
    if o.include_unmapped_null {
        let minptr = (0..count)
            .filter_map(|i| u32v(d, 28 + i * cell).ok())
            .filter(|&p| p != 0)
            .min()
            .map(|v| v as usize)
            .unwrap_or(dpos + dlen);
        let extra = 28 + count * cell;
        if extra + cell <= dlen && minptr == dpos + extra + cell {
            let pointer = u32v(d, extra)? as usize;
            let width = if kind == GpiKind::Abc {
                let value = i16v(d, extra + 6)?;
                if value == i64::from(i16::MIN) {
                    i16v(d, 22)?
                } else {
                    value
                }
            } else {
                i64::from(u16v(d, extra + 4)?)
            };
            // An uncounted gap can be padding. Only expose a structurally valid
            // extra descriptor; preserve all other bytes in the native resource.
            if width >= 0
                && (pointer == 0
                    || (pointer >= minptr
                        && pointer
                            .checked_add((width as usize).div_ceil(8) * h as usize)
                            .is_some_and(|end| end <= dpos + dlen)))
            {
                n += 1;
            }
        }
    }
    *total = total
        .checked_add(n)
        .ok_or_else(|| invalid("GPI glyph count overflow"))?;
    check(*total, l.max_glyphs, "GPI cumulative glyphs")?;
    budget.items::<Glyph>(n)?;
    budget.add(n.saturating_mul(cell + 256).saturating_add(mlen * 2 + 2048))?;
    let mut f = Font::default();
    f.source_format = Some("gpi".into());
    f.names.family = Some(name(&m[8..40]));
    f.names.full = Some(name(&m[40..72]));
    f.names.style = Some(
        match u16v(m, 132)? & 0x21 {
            0x21 => "Bold Italic",
            0x20 => "Bold",
            1 => "Italic",
            _ => "Regular",
        }
        .into(),
    );
    let codepage = u16v(m, 74)?;
    f.charset.name = Some(if codepage == 0 || codepage == 850 {
        "ibm-ugl".into()
    } else {
        format!("ibm-{codepage}")
    });
    f.metrics.ascent = Some(i16v(m, 84)?);
    f.metrics.descent = Some(i16v(m, 86)?);
    f.metrics.pixel_size = Some(u32::try_from(i16v(m, 76)?).unwrap_or(h as u32));
    f.metrics.point_size = Some(Rational::new(i64::from(u16v(m, 122)?), 10)?);
    f.metrics.resolution = Some(Vector::new(i16v(m, 110)?, i16v(m, 112)?));
    f.metrics.line_gap = Some(i16v(m, 90)?);
    f.metrics.line_height = Some(i16v(m, 98)? + i16v(m, 90)?);
    f.metrics.underline_thickness = Some(i16v(m, 152)?);
    f.metrics.underline_position = Some(-i16v(m, 154)?);
    f.metrics.spacing = Some(if kind == GpiKind::Fixed {
        Spacing::Monospaced
    } else {
        Spacing::Proportional
    });
    f.metrics.default_character = Some(Codepoint::Legacy(first + u32::from(u16v(m, 118)?)));
    f.properties.push(Property::new(
        "gpi.metrics",
        PropertyValue::Bytes(m.to_vec()),
    ));
    f.properties.push(Property::new(
        "gpi.definition",
        PropertyValue::Bytes(d[..28].to_vec()),
    ));
    f.properties.push(Property::new(
        "gpi.kind",
        match kind {
            GpiKind::Fixed => 1i64,
            GpiKind::Proportional => 2,
            GpiKind::Abc => 3,
        },
    ));
    f.properties
        .push(Property::new("gpi.codepage", i64::from(codepage)));
    f.properties
        .push(Property::new("gpi.registry_id", i64::from(u16v(m, 72)?)));
    for i in 0..n {
        let entry = &d[28 + i * cell..28 + (i + 1) * cell];
        let ptr = u32v(entry, 0)? as usize;
        let (a, width, c) = if kind == GpiKind::Abc {
            let val = |off: usize, def: usize| -> Result<i64> {
                let v = i16v(entry, off)?;
                if v == i64::from(i16::MIN) {
                    let v = i16v(d, def)?;
                    if v == i64::from(i16::MIN) {
                        return Err(invalid("GPI undefined ABC default"));
                    }
                    Ok(v)
                } else {
                    Ok(v)
                }
            };
            (val(4, 20)?, val(6, 22)?, val(8, 24)?)
        } else {
            (0, i64::from(u16v(entry, 4)?), 0)
        };
        if width < 0 {
            return Err(invalid("negative GPI bitmap width"));
        }
        check(
            (width as usize).saturating_mul(h as usize),
            l.max_pixels_per_glyph,
            "GPI glyph pixels",
        )?;
        let stride = (width as usize).div_ceil(8);
        let size = stride * h as usize;
        budget.add(size.saturating_mul(2))?;
        let mut raster = vec![0; size];
        if ptr != 0 {
            if ptr < dpos + 28 + n * cell || ptr.checked_add(size).is_none_or(|v| v > dpos + dlen) {
                return Err(invalid("GPI glyph bitmap outside definition record"));
            }
            let data = slice(b, ptr, size)?;
            for y in 0..h as usize {
                for x in 0..stride {
                    raster[y * stride + x] = data[x * h as usize + y];
                }
            }
        }
        let mut g = Glyph::new(Bitmap::<Mono>::from_raw(
            width as u32,
            h as u32,
            &raster,
            bitmap_limits(l),
        )?);
        g.metrics.bounds.x = a;
        g.metrics.bounds.y = base - h;
        g.metrics.advance = Some(Vector::new(
            if kind == GpiKind::Fixed {
                i16v(d, 18)?
            } else {
                a + width + c
            },
            0,
        ));
        if kind == GpiKind::Abc {
            g.metrics.abc = Some(AbcWidths { a, b: width, c });
        }
        if i < count {
            g.codepoints.push(Codepoint::Legacy(first + i as u32));
        } else {
            g.name = Some(".null".into());
        }
        if ptr == 0 {
            g.properties.push(Property::new("gpi.missing", 1i64));
        }
        if cell > minimum {
            g.properties.push(Property::new(
                "gpi.descriptor_extra",
                PropertyValue::Bytes(entry[minimum..].to_vec()),
            ));
        }
        f.add_glyph(g)?;
    }
    let tail = dpos + dlen;
    let mut at = tail;
    let pairs = usize::from(u16v(m, 160)?);
    let mut ended = false;
    while at < b.len() {
        let id = u32v(b, at)?;
        let len = u32v(b, at + 4)? as usize;
        if id == 0xffffffff {
            if len != 8 {
                return Err(invalid("invalid GPI end record"));
            }
            at += 8;
            ended = true;
            break;
        }
        let size = if id == 3 {
            // Both documented ten-byte and packed nine-byte headers exist in descriptions.
            let candidates = [9usize, 10]
                .into_iter()
                .filter(|&h| {
                    let next = at + h + pairs * 6;
                    matches!(u32v(b, next), Ok(4 | 0xffffffff))
                })
                .collect::<Vec<_>>();
            if candidates.len() != 1 {
                return Err(invalid("ambiguous or truncated GPI kerning header"));
            }
            candidates[0] + pairs * 6
        } else {
            len
        };
        if size < 8 {
            return Err(invalid("invalid GPI optional record size"));
        }
        slice(b, at, size)?;
        at += size;
    }
    if !ended || b[at..].iter().any(|&v| v != 0) {
        return Err(invalid("GPI record stream has no exact end marker"));
    }
    budget.add((b.len() - tail) * 2)?;
    f.properties.push(Property::new(
        "gpi.tail",
        PropertyValue::Bytes(b[tail..].to_vec()),
    ));
    if let Some(cp) = f.metrics.default_character {
        f.metrics.default_glyph = f.glyph_id(cp);
    }
    Ok(f)
}
